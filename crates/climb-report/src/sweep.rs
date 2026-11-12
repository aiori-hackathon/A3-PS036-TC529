//! Batch-size sweep: the measured rows behind the break-even analysis.
//!
//! A sweep row records, for one batch size `N`, the real libMTL ladder and
//! condensed-signature bytes, the three wire totals (A plain SLH-DSA, B MTL
//! without amortisation, C MTL amortised), and the measured time to sign the
//! whole batch once. Rows above the measured limit carry the same fields but
//! are labelled [`EvidenceClass::Extrapolated`]: their shape comes from the
//! closed-form ladder model and their time from the per-operation cost model.
//!
//! The analysis in this module is pure: [`analyze`] takes the rows and returns
//! the crossovers, the UDP-fit ranges and the best batch-size range. It never
//! starts from another derived field, so `cargo xtask verify-numbers` can
//! recompute every published crossover from `/derived/sweep/rows`.
//!
//! # Formulas
//!
//! * `A = N × plain_signature_len` — one full SLH-DSA signature per message.
//! * `B = condensed_total + N × signed_ladder_len` — one complete MTL
//!   signature per message.
//! * `C = condensed_total + signed_ladder_len` — one shared ladder.
//! * signing `A = N × t_plain_sign`, `B = N × t_mtl_single`,
//!   `C = measured batch time` (or `t_plain_sign + N × tree_per_message` when
//!   extrapolated).
//! * verification `A = N × t_plain_verify`,
//!   `C` cached `= N × t_verify_condensed_cached`,
//!   `C` cold `= t_cold + (N − 1) × t_verify_condensed_cached`, i.e. the
//!   ladder is validated once and the remaining messages reuse it.
//! * UDP response maximum for a profile is
//!   `overhead + rrsig_count × signature_field`, where the field is the plain
//!   signature for A, `ladder + 4 + plain + 1 + condensed` for B and
//!   `1 + condensed` for C.

use serde::{Deserialize, Serialize};

use crate::results::EvidenceClass;
use climb_analytical::{
    builtin_profiles, rungs, ResponseProfile, SignatureConfig, CONDENSED_FIXED_LEN, HASH_SIZE,
    UDP_MTU_1232,
};

/// Schema version of the sweep document and the embedded section.
pub const SWEEP_SCHEMA_VERSION: u32 = 1;

/// Batch sizes measured by `cargo xtask sweep --quick`.
pub const QUICK_SIZES: [u64; 9] = [1, 2, 4, 8, 16, 32, 64, 256, 1_024];

/// Largest batch size in the default sweep (2^20).
pub const DEFAULT_MAX_N: u64 = 1_048_576;

/// Per-operation costs the extrapolated rows are built from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepCostModel {
    /// One plain SLH-DSA signature, measured.
    pub t_plain_sign_ns: f64,
    /// One plain SLH-DSA verification, measured.
    pub t_plain_verify_ns: f64,
    /// One complete MTL signature (a batch of one), measured.
    pub t_mtl_single_sign_ns: f64,
    /// One condensed verification against a trust-cached ladder, measured.
    pub t_verify_condensed_cached_ns: f64,
    /// One condensed verification that validates the ladder first, measured.
    pub t_verify_condensed_cold_ns: f64,
    /// Plain SLH-DSA signature length read from liboqs.
    pub plain_signature_len_bytes: u64,
    /// Sign samples taken per timed operation.
    pub sign_samples: usize,
    /// Verification samples taken per timed operation.
    pub verify_samples: usize,
    /// Tree work per message, derived from the largest measured batch:
    /// `(batch_sign_ns − t_plain_sign_ns) / N`, floored at zero.
    pub tree_per_message_ns: f64,
    /// The formula the `C` signing column is modelled with above the limit.
    pub formula: String,
}

impl Default for SweepCostModel {
    fn default() -> Self {
        Self {
            t_plain_sign_ns: 0.0,
            t_plain_verify_ns: 0.0,
            t_mtl_single_sign_ns: 0.0,
            t_verify_condensed_cached_ns: 0.0,
            t_verify_condensed_cold_ns: 0.0,
            plain_signature_len_bytes: 0,
            sign_samples: 0,
            verify_samples: 0,
            tree_per_message_ns: 0.0,
            formula: "C = t_plain_sign + N × tree_per_message".to_string(),
        }
    }
}

/// One measured or extrapolated batch size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepRow {
    /// Messages in the batch.
    pub n: u64,
    /// `measured` when the row signed a real batch of this size, `extrapolated`
    /// when its shape and cost come from the validated model.
    pub class: EvidenceClass,
    /// Rungs libMTL emitted (measured) or the model predicts.
    pub rung_count: usize,
    /// Unsigned ladder-buffer length in bytes.
    pub ladder_buffer_len: usize,
    /// Signed ladder length in bytes (buffer + length field + signature).
    pub signed_ladder_len: usize,
    /// Embedded underlying signature length in bytes.
    pub signature_len: u64,
    /// Smallest condensed signature at this size.
    pub condensed_min: usize,
    /// Mean condensed signature at this size.
    pub condensed_mean: f64,
    /// Largest condensed signature at this size.
    pub condensed_max: usize,
    /// Sum of every condensed signature at this size.
    pub condensed_total: u64,
    /// Configuration A wire total: `N × plain_signature_len`.
    pub wire_a_bytes: u64,
    /// Configuration B wire total: `condensed_total + N × signed_ladder_len`.
    pub wire_b_bytes: u64,
    /// Configuration C wire total: `condensed_total + signed_ladder_len`.
    pub wire_c_bytes: u64,
    /// Measured median time to sign the batch once; `None` for extrapolated
    /// rows.
    pub batch_sign_ns: Option<f64>,
    /// Configuration A signing time: `N × t_plain_sign`.
    pub signing_a_ns: f64,
    /// Configuration B signing time: `N × t_mtl_single`.
    pub signing_b_ns: f64,
    /// Configuration C signing time: measured batch time, or the model above
    /// the limit.
    pub signing_c_ns: f64,
    /// Configuration A verification time: `N × t_plain_verify`.
    pub verify_a_ns: f64,
    /// Configuration C trust-cached verification: `N × t_cached`.
    pub verify_c_cached_ns: f64,
    /// Configuration C cold verification: `N × t_cold`.
    pub verify_c_cold_ns: f64,
}

/// A break-even point for one comparison of C against A.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepCrossover {
    /// Stable metric id (`wire`, `signing`, `verify-cached`, `verify-cold`).
    pub metric: String,
    /// Human label for tables.
    pub label: String,
    /// Smallest `N` in the sweep at which C is at or below A; `None` when no
    /// row reached parity.
    pub n: Option<u64>,
    /// The comparison baseline, always `A`.
    pub baseline: String,
    /// `derived` when the crossing row was measured, `extrapolated` when it
    /// came from the model.
    pub class: EvidenceClass,
    /// The formula the crossing was computed with.
    pub formula: String,
}

/// One (response profile, configuration) UDP-fit range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepUdpFitRow {
    /// Stable profile id.
    pub profile: String,
    /// Human profile label.
    pub profile_label: String,
    /// Where the profile overheads came from (`captured` or `parameter`).
    pub profile_source: String,
    /// Configuration id (`plain`, `mtl-full`, `mtl-amortised`).
    pub config: String,
    /// Human configuration label.
    pub config_label: String,
    /// Smallest `N` whose largest modelled response fits 1,232 bytes.
    pub first_fit_n: Option<u64>,
    /// Largest `N` whose largest modelled response fits 1,232 bytes.
    pub last_fit_n: Option<u64>,
    /// Largest modelled response at the last fitting size, in bytes.
    pub max_response_bytes_at_last_fit: u64,
    /// Always `analytical`: the response length is modelled, not captured.
    pub class: EvidenceClass,
}

/// The batch-size range in which C wins on bytes, signing time, cached and cold
/// verification while a response still fits one UDP packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepBestRange {
    /// First `N` in the range.
    pub start_n: u64,
    /// Last `N` in the range, or `None` when no fitting size was measured.
    pub end_n: Option<u64>,
    /// Why the range ends where it does.
    pub reason: String,
    /// Weakest evidence among the crossing inputs.
    pub class: EvidenceClass,
}

/// The complete sweep section embedded under `derived.sweep`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepSection {
    /// Schema version.
    pub schema_version: u32,
    /// When the sweep was recorded.
    pub generated_at: String,
    /// Algorithm string measured.
    pub algorithm: String,
    /// Deterministic dataset seed.
    pub seed: u64,
    /// Largest batch size measured with the real library.
    pub measured_max_n: u64,
    /// Why the measured range stops where it does.
    pub limit_reason: String,
    /// Wall-clock time of the sweep, in milliseconds.
    pub elapsed_ms: u64,
    /// Per-operation costs the extrapolated rows use.
    pub cost_model: SweepCostModel,
    /// Rows in ascending `N` order.
    pub rows: Vec<SweepRow>,
    /// Break-even points, recomputed by [`analyze`].
    pub crossovers: Vec<SweepCrossover>,
    /// UDP-fit ranges per profile and configuration.
    pub udp_fit: Vec<SweepUdpFitRow>,
    /// Best batch-size range, when one exists.
    pub best_range: Option<SweepBestRange>,
}

/// Measured inputs for one real batch, as returned by the measurement crate.
pub type MeasuredSweepRow = climb_bench::SweepMeasuredRow;

/// Build the full row for a measured batch.
#[must_use]
pub fn measured_row(measured: MeasuredSweepRow, cost_model: &SweepCostModel) -> SweepRow {
    let n = measured.n;
    let wire_a_bytes = n.saturating_mul(cost_model.plain_signature_len_bytes);
    let wire_b_bytes = measured
        .condensed_total
        .saturating_add(n.saturating_mul(measured.signed_ladder_len as u64));
    let wire_c_bytes = measured
        .condensed_total
        .saturating_add(measured.signed_ladder_len as u64);
    SweepRow {
        n,
        class: EvidenceClass::Measured,
        rung_count: measured.rung_count,
        ladder_buffer_len: measured.ladder_buffer_len,
        signed_ladder_len: measured.signed_ladder_len,
        signature_len: measured.signature_len,
        condensed_min: measured.condensed_min,
        condensed_mean: measured.condensed_mean,
        condensed_max: measured.condensed_max,
        condensed_total: measured.condensed_total,
        wire_a_bytes,
        wire_b_bytes,
        wire_c_bytes,
        batch_sign_ns: Some(measured.batch_sign_ns),
        signing_a_ns: n as f64 * cost_model.t_plain_sign_ns,
        signing_b_ns: n as f64 * cost_model.t_mtl_single_sign_ns,
        signing_c_ns: measured.batch_sign_ns,
        verify_a_ns: n as f64 * cost_model.t_plain_verify_ns,
        verify_c_cached_ns: n as f64 * cost_model.t_verify_condensed_cached_ns,
        verify_c_cold_ns: cost_model.t_verify_condensed_cold_ns
            + (n.saturating_sub(1)) as f64 * cost_model.t_verify_condensed_cached_ns,
    }
}

/// Build the model row for a batch size above the measured limit.
///
/// The ladder shape is the closed-form decomposition validated against real
/// libMTL bytes at every measured size; the condensed statistics are the exact
/// weighted distribution over that decomposition.
#[must_use]
pub fn extrapolated_row(n: u64, cost_model: &SweepCostModel) -> SweepRow {
    let ladder = rungs(n);
    let rung_count = ladder.len();
    let ladder_buffer_len = predicted_ladder_len_bytes(rung_count);
    let signed_ladder_len = ladder_buffer_len
        .saturating_add(4)
        .saturating_add(cost_model.plain_signature_len_bytes as usize);
    let mut condensed_min = usize::MAX;
    let mut condensed_max = 0usize;
    let mut condensed_total = 0u64;
    for rung in &ladder {
        let depth = rung.size().trailing_zeros() as usize;
        let len = CONDENSED_FIXED_LEN + depth * HASH_SIZE;
        condensed_min = condensed_min.min(len);
        condensed_max = condensed_max.max(len);
        condensed_total = condensed_total.saturating_add(len as u64 * rung.size());
    }
    let (condensed_min, condensed_max) = if rung_count == 0 {
        (0, 0)
    } else {
        (condensed_min, condensed_max)
    };
    let condensed_mean = if n == 0 {
        0.0
    } else {
        condensed_total as f64 / n as f64
    };
    let wire_a_bytes = n.saturating_mul(cost_model.plain_signature_len_bytes);
    let wire_b_bytes = condensed_total.saturating_add(n.saturating_mul(signed_ladder_len as u64));
    let wire_c_bytes = condensed_total.saturating_add(signed_ladder_len as u64);
    SweepRow {
        n,
        class: EvidenceClass::Extrapolated,
        rung_count,
        ladder_buffer_len,
        signed_ladder_len,
        signature_len: cost_model.plain_signature_len_bytes,
        condensed_min,
        condensed_mean,
        condensed_max,
        condensed_total,
        wire_a_bytes,
        wire_b_bytes,
        wire_c_bytes,
        batch_sign_ns: None,
        signing_a_ns: n as f64 * cost_model.t_plain_sign_ns,
        signing_b_ns: n as f64 * cost_model.t_mtl_single_sign_ns,
        signing_c_ns: cost_model.t_plain_sign_ns
            + n as f64 * cost_model.tree_per_message_ns.max(0.0),
        verify_a_ns: n as f64 * cost_model.t_plain_verify_ns,
        verify_c_cached_ns: n as f64 * cost_model.t_verify_condensed_cached_ns,
        verify_c_cold_ns: cost_model.t_verify_condensed_cold_ns
            + (n.saturating_sub(1)) as f64 * cost_model.t_verify_condensed_cached_ns,
    }
}

/// Unsigned ladder-buffer length for `leaf_count` leaves, given its rung count.
#[must_use]
pub fn predicted_ladder_len_bytes(rung_count: usize) -> usize {
    2 + 2 * HASH_SIZE + 2 + rung_count * (2 * climb_analytical::MTL_INDEX_LEN + HASH_SIZE)
}

/// The four break-even metrics, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverMetric {
    /// Wire bytes.
    Wire,
    /// Batch signing time.
    Signing,
    /// Trust-cached verification.
    VerifyCached,
    /// Cold verification.
    VerifyCold,
}

impl CrossoverMetric {
    /// Stable metric id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            CrossoverMetric::Wire => "wire",
            CrossoverMetric::Signing => "signing",
            CrossoverMetric::VerifyCached => "verify-cached",
            CrossoverMetric::VerifyCold => "verify-cold",
        }
    }

    /// Human label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            CrossoverMetric::Wire => "Wire bytes, C vs A",
            CrossoverMetric::Signing => "Batch signing time, C vs A",
            CrossoverMetric::VerifyCached => "Verification (trust-cached), C vs A",
            CrossoverMetric::VerifyCold => "Verification (cold), C vs A",
        }
    }

    /// The formula the comparison uses.
    #[must_use]
    pub const fn formula(self) -> &'static str {
        match self {
            CrossoverMetric::Wire => {
                "C = ladder + sum(condensed); A = N x plain signature; first N with C <= A"
            }
            CrossoverMetric::Signing => {
                "C = batch sign time (one ladder signature + tree work); A = N x t_plain_sign"
            }
            CrossoverMetric::VerifyCached => {
                "C = N x t_verify_condensed_cached; A = N x t_plain_verify"
            }
            CrossoverMetric::VerifyCold => {
                "C = t_cold + (N - 1) x t_verify_condensed_cached (ladder validated once); A = N x t_plain_verify"
            }
        }
    }

    /// `(c, a)` for one row.
    #[must_use]
    pub fn values(self, row: &SweepRow) -> (f64, f64) {
        match self {
            CrossoverMetric::Wire => (row.wire_c_bytes as f64, row.wire_a_bytes as f64),
            CrossoverMetric::Signing => (row.signing_c_ns, row.signing_a_ns),
            CrossoverMetric::VerifyCached => (row.verify_c_cached_ns, row.verify_a_ns),
            CrossoverMetric::VerifyCold => (row.verify_c_cold_ns, row.verify_a_ns),
        }
    }
}

/// First row (ascending `N`) at which C is no larger than A.
///
/// A linear scan, not a binary search: measured signing rows are not perfectly
/// monotone, so the scan reports the first row that actually reached parity
/// rather than assuming a shape. The returned class is the crossing row's own
/// class.
#[must_use]
pub fn first_crossover(rows: &[SweepRow], metric: CrossoverMetric) -> Option<(u64, EvidenceClass)> {
    rows.iter()
        .find(|row| {
            let (c, a) = metric.values(row);
            c <= a
        })
        .map(|row| (row.n, row.class))
}

/// Recompute the crossovers from the raw rows.
#[must_use]
pub fn crossovers(rows: &[SweepRow]) -> Vec<SweepCrossover> {
    [
        CrossoverMetric::Wire,
        CrossoverMetric::Signing,
        CrossoverMetric::VerifyCached,
        CrossoverMetric::VerifyCold,
    ]
    .into_iter()
    .map(|metric| {
        let found = first_crossover(rows, metric);
        SweepCrossover {
            metric: metric.id().to_string(),
            label: metric.label().to_string(),
            n: found.map(|(n, _)| n),
            baseline: "A".to_string(),
            class: found
                .map(|(_, class)| class)
                .unwrap_or(EvidenceClass::Extrapolated),
            formula: metric.formula().to_string(),
        }
    })
    .collect()
}

/// The largest modelled response for `profile` at `row`, for `config`.
#[must_use]
pub fn max_response_bytes(
    profile: &ResponseProfile,
    config: SignatureConfig,
    row: &SweepRow,
    plain_signature_len: u64,
) -> u64 {
    let overhead = match config {
        SignatureConfig::MtlAmortised => profile.overhead_bytes_sigttag,
        _ => profile.overhead_bytes,
    } as u64;
    let field = match config {
        SignatureConfig::Plain => plain_signature_len,
        SignatureConfig::MtlFull => row
            .signed_ladder_len
            .saturating_add(4)
            .saturating_add(plain_signature_len as usize)
            .saturating_add(1)
            .saturating_add(row.condensed_max) as u64,
        SignatureConfig::MtlAmortised => row.condensed_max.saturating_add(1) as u64,
        SignatureConfig::ClassicalEd25519
        | SignatureConfig::ClassicalEcdsaP256
        | SignatureConfig::ClassicalRsa2048 => plain_signature_len,
    };
    overhead.saturating_add((profile.rrsig_count as u64).saturating_mul(field))
}

fn config_id(config: SignatureConfig) -> &'static str {
    match config {
        SignatureConfig::Plain => "plain",
        SignatureConfig::MtlFull => "mtl-full",
        SignatureConfig::MtlAmortised => "mtl-amortised",
        SignatureConfig::ClassicalEd25519 => "classical-ed25519",
        SignatureConfig::ClassicalEcdsaP256 => "classical-ecdsa-p256",
        SignatureConfig::ClassicalRsa2048 => "classical-rsa2048",
    }
}

/// Recompute the UDP-fit ranges from the raw rows and the built-in profiles.
#[must_use]
pub fn udp_fit(rows: &[SweepRow], plain_signature_len: u64) -> Vec<SweepUdpFitRow> {
    let mut out = Vec::new();
    for profile in builtin_profiles() {
        for config in [
            SignatureConfig::Plain,
            SignatureConfig::MtlFull,
            SignatureConfig::MtlAmortised,
        ] {
            let mut first = None;
            let mut last = None;
            let mut last_max = 0u64;
            let mut max_over_rows = 0u64;
            for row in rows {
                let max = max_response_bytes(&profile, config, row, plain_signature_len);
                max_over_rows = max_over_rows.max(max);
                if max <= UDP_MTU_1232 as u64 {
                    if first.is_none() {
                        first = Some(row.n);
                    }
                    last = Some(row.n);
                    last_max = max;
                }
            }
            out.push(SweepUdpFitRow {
                profile: profile.name.clone(),
                profile_label: profile.label.clone(),
                profile_source: match profile.source {
                    climb_analytical::ProfileSource::Captured => "captured",
                    climb_analytical::ProfileSource::Parameter => "parameter",
                }
                .to_string(),
                config: config_id(config).to_string(),
                config_label: config.label().to_string(),
                first_fit_n: first,
                last_fit_n: last,
                max_response_bytes_at_last_fit: if first.is_some() {
                    last_max
                } else {
                    max_over_rows
                },
                class: EvidenceClass::Analytical,
            });
        }
    }
    out
}

/// Recompute the best batch-size range.
#[must_use]
pub fn best_range(
    rows: &[SweepRow],
    crossovers: &[SweepCrossover],
    udp_fit: &[SweepUdpFitRow],
) -> Option<SweepBestRange> {
    let mut start: Option<u64> = None;
    let mut class = EvidenceClass::Derived;
    for crossover in crossovers {
        let n = crossover.n?;
        start = Some(start.map_or(n, |current: u64| current.max(n)));
        if crossover.class == EvidenceClass::Extrapolated {
            class = EvidenceClass::Extrapolated;
        }
    }
    let start = start?;
    let end = udp_fit
        .iter()
        .filter(|row| row.config == "mtl-amortised")
        .filter_map(|row| row.last_fit_n)
        .min()?;
    if end < start {
        return None;
    }
    let max_row = rows.iter().find(|row| row.n == end);
    if max_row.is_some_and(|row| row.class == EvidenceClass::Extrapolated) {
        class = EvidenceClass::Extrapolated;
    }
    Some(SweepBestRange {
        start_n: start,
        end_n: Some(end),
        reason: format!(
            "starts where every C-vs-A comparison has crossed (worst N = {start}); ends at the largest N whose worst-profile condensed response still fits {UDP_MTU_1232} B"
        ),
        class,
    })
}

/// Recompute the crossovers, UDP-fit ranges and best range from the rows.
pub fn analyze(section: &mut SweepSection) {
    section.crossovers = crossovers(&section.rows);
    section.udp_fit = udp_fit(&section.rows, section.cost_model.plain_signature_len_bytes);
    section.best_range = best_range(&section.rows, &section.crossovers, &section.udp_fit);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> SweepCostModel {
        SweepCostModel {
            t_plain_sign_ns: 500_000_000.0,
            t_plain_verify_ns: 500_000.0,
            t_mtl_single_sign_ns: 505_000_000.0,
            t_verify_condensed_cached_ns: 3_000.0,
            t_verify_condensed_cold_ns: 503_000.0,
            plain_signature_len_bytes: 7_856,
            sign_samples: 3,
            verify_samples: 3,
            tree_per_message_ns: 30_000.0,
            formula: "C = t_plain_sign + N x tree_per_message".to_string(),
        }
    }

    fn rows() -> Vec<SweepRow> {
        [1u64, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1_024]
            .into_iter()
            .map(|n| extrapolated_row(n, &model()))
            .collect()
    }

    fn section() -> SweepSection {
        let mut section = SweepSection {
            schema_version: SWEEP_SCHEMA_VERSION,
            generated_at: "2026-11-13T00:00:00Z".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            seed: 1,
            measured_max_n: 1_024,
            limit_reason: "test".to_string(),
            elapsed_ms: 0,
            cost_model: model(),
            rows: rows(),
            crossovers: Vec::new(),
            udp_fit: Vec::new(),
            best_range: None,
        };
        analyze(&mut section);
        section
    }

    /// The linear search must agree with a deliberately independent
    /// brute-force scan, including a series that never crosses.
    #[test]
    fn crossover_search_matches_a_brute_force_scan() {
        let mut rows = rows();
        if let Some(row) = rows.first_mut() {
            row.wire_c_bytes = row.wire_a_bytes * 2;
        }
        let found = first_crossover(&rows, CrossoverMetric::Wire)
            .map(|(n, _)| n)
            .expect("a crossover");
        let mut expected = None;
        for row in &rows {
            if row.wire_c_bytes <= row.wire_a_bytes {
                expected = Some(row.n);
                break;
            }
        }
        assert_eq!(Some(found), expected);

        for row in &mut rows {
            row.signing_c_ns = row.signing_a_ns * 2.0;
        }
        assert_eq!(first_crossover(&rows, CrossoverMetric::Signing), None);
    }

    /// Wire, A and B totals grow strictly with N over the power-of-two rows.
    #[test]
    fn wire_totals_are_monotone_over_powers_of_two() {
        let rows = rows();
        for pair in rows.windows(2) {
            let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            assert!(right.wire_a_bytes > left.wire_a_bytes, "A at {}", right.n);
            assert!(right.wire_b_bytes > left.wire_b_bytes, "B at {}", right.n);
            assert!(right.wire_c_bytes > left.wire_c_bytes, "C at {}", right.n);
            assert!(
                right.condensed_max >= left.condensed_max,
                "condensed max at {}",
                right.n
            );
        }
    }

    /// C is below A on every metric from the modelled crossover onward.
    #[test]
    fn every_crossover_is_a_real_parity_point() {
        let section = section();
        for crossover in &section.crossovers {
            let Some(n) = crossover.n else {
                continue;
            };
            let metric = match crossover.metric.as_str() {
                "wire" => CrossoverMetric::Wire,
                "signing" => CrossoverMetric::Signing,
                "verify-cached" => CrossoverMetric::VerifyCached,
                "verify-cold" => CrossoverMetric::VerifyCold,
                other => panic!("unknown metric {other}"),
            };
            let row = section
                .rows
                .iter()
                .find(|row| row.n == n)
                .expect("crossing row");
            let (c, a) = metric.values(row);
            assert!(c <= a, "{} at {n}: {c} > {a}", crossover.metric);
        }
    }

    #[test]
    fn a_response_profile_reports_both_ends_of_its_udp_window() {
        let section = section();
        let row = section
            .udp_fit
            .iter()
            .find(|row| row.profile == "positive_a" && row.config == "mtl-amortised")
            .expect("positive_a / C row");
        assert_eq!(row.first_fit_n, Some(1));
        assert!(
            row.last_fit_n.unwrap_or(0) >= row.first_fit_n.unwrap_or(0),
            "last fit {}",
            row.last_fit_n.unwrap_or(0)
        );
        assert!(row.max_response_bytes_at_last_fit <= UDP_MTU_1232 as u64);
    }

    #[test]
    fn config_a_never_fits_a_udp_packet() {
        let section = section();
        for row in section.udp_fit.iter().filter(|row| row.config == "plain") {
            assert_eq!(row.first_fit_n, None);
            assert_eq!(row.last_fit_n, None);
        }
    }

    #[test]
    fn the_analysis_is_deterministic() {
        let mut first = section();
        let mut second = section();
        analyze(&mut first);
        analyze(&mut second);
        assert_eq!(
            serde_json::to_string(&first).expect("first serializes"),
            serde_json::to_string(&second).expect("second serializes")
        );
    }

    #[test]
    fn extrapolated_rows_are_labelled_and_carry_no_measured_time() {
        let row = extrapolated_row(65_536, &model());
        assert_eq!(row.class, EvidenceClass::Extrapolated);
        assert_eq!(row.batch_sign_ns, None);
        assert_eq!(row.signature_len, 7_856);
        assert_eq!(
            row.wire_c_bytes,
            row.condensed_total + row.signed_ladder_len as u64
        );
    }

    /// The committed sweep document is the schema contract: it parses, every
    /// stored crossover agrees with a fresh recomputation from its own rows,
    /// and its rows are ordered.
    #[test]
    fn the_committed_artifact_parses_and_recomputes() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sweep.recorded.json");
        let text = std::fs::read_to_string(&path).expect("read committed sweep artifact");
        let mut section: SweepSection =
            serde_json::from_str(&text).expect("parse committed sweep artifact");
        assert_eq!(section.schema_version, SWEEP_SCHEMA_VERSION);
        assert!(!section.rows.is_empty());
        assert!(section.rows.windows(2).all(|pair| {
            match (pair.first(), pair.get(1)) {
                (Some(left), Some(right)) => left.n < right.n,
                _ => true,
            }
        }));
        let stored = section.crossovers.clone();
        let stored_udp = section.udp_fit.clone();
        analyze(&mut section);
        assert_eq!(
            section.crossovers, stored,
            "stored crossovers must recompute"
        );
        assert_eq!(
            section.udp_fit, stored_udp,
            "stored UDP-fit rows must recompute"
        );
    }
}
