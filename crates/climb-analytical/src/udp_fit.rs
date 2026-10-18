//! Analytical model of DNS response sizes over UDP and the TCP fallback cost.
//!
//! A DNSSEC-signed response carries one RRSIG per covered RRset, and each
//! RRSIG's signature field is one of three shapes (the `SignatureConfig`s):
//!
//! * **A — plain SLH-DSA**: one full signature per RRSIG, no ladder.
//! * **B — MTL full** (`MTL-Type 0x01`): per RRSIG, the signed ladder plus a
//!   condensed signature: `lead(1) + condensed + ladder + len(4) + plain`.
//! * **C — MTL amortised** (`MTL-Type 0x02`): per RRSIG, only the lead byte
//!   and the condensed signature: `lead(1) + condensed`.
//!
//! The condensed length varies with the leaf's authentication-path depth, so
//! per-leaf signature-field lengths form a histogram; a response with
//! `rrsig_count` RRSIGs sums `rrsig_count` independent draws from it, which is
//! the discrete convolution of the histogram with itself
//! ([`convolve_rrsigs`]). Adding the response overhead (header, question,
//! RR headers, OPT; the SigTag variant where the condensed form carries the
//! MTL SigTag option) gives the modelled response length distribution, which
//! is banded against the 1,232-byte EDNS UDP size and the 4,096-byte practical
//! ceiling to estimate how often a response fits UDP, signals TC, or needs
//! TCP.
//!
//! # No measurements, no guarantees
//!
//! Every number this module produces is an analytical estimate from tree shape
//! and observed overheads — never a measured wire length, and never a claim
//! that a real resolver behaves in any particular way. The formulas are
//! cross-checked against a recorded capture at one point (see
//! `infer_zone_leaf_count` and the `udp_fit` section of the consolidated
//! results), which validates the model at that point, not in general.

use std::collections::BTreeMap;

use crate::ladder::{predicted_condensed_len, predicted_ladder_len, CONDENSED_FIXED_LEN};
use crate::{HASH_SIZE, MTL_INDEX_LEN};

/// The three signature-field shapes a response's RRSIGs may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureConfig {
    /// One full plain SLH-DSA signature per RRSIG, no ladder.
    Plain,
    /// MTL full form: signed ladder plus condensed signature per RRSIG.
    MtlFull,
    /// MTL amortised form: condensed signature per RRSIG, one shared ladder.
    MtlAmortised,
}

impl SignatureConfig {
    /// Short display label for tables and legends.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            SignatureConfig::Plain => "A (plain SLH-DSA)",
            SignatureConfig::MtlFull => "B (MTL non-amortised)",
            SignatureConfig::MtlAmortised => "C (MTL amortised)",
        }
    }
}

/// Where a response profile's numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProfileSource {
    /// Overheads measured from a captured response.
    Captured,
    /// Parameters chosen for modelling, not measured from a capture.
    Parameter,
}

/// One response-composition profile: RRSIG count, section overheads, question
/// length.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResponseProfile {
    /// Stable machine name (e.g. `positive_a_with_ns`).
    pub name: String,
    /// Human label for tables and charts.
    pub label: String,
    /// Whether the overheads were captured or chosen as parameters.
    pub source: ProfileSource,
    /// RRSIG records per response.
    pub rrsig_count: usize,
    /// Non-signature response bytes without the MTL SigTag option.
    pub overhead_bytes: usize,
    /// Non-signature response bytes with the MTL SigTag option present.
    pub overhead_bytes_sigttag: usize,
    /// Question-section bytes.
    pub question_bytes: usize,
    /// Provenance and caveats for this profile.
    pub note: String,
}

/// Wrapper matching the on-disk profile-file layout.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProfileData {
    /// The profiles, in file order.
    pub profiles: Vec<ResponseProfile>,
}

/// One length bucket of a response-component histogram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondensedHistogramBucket {
    /// Component length in bytes.
    pub len: usize,
    /// How many leaves (or leaf tuples) produce this length.
    pub count: u64,
}

/// Distribution of component lengths over leaves (or convolved leaf tuples).
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseLengthHistogram {
    /// Buckets in ascending length order.
    pub buckets: Vec<CondensedHistogramBucket>,
}

impl ResponseLengthHistogram {
    /// Total number of draws represented by the histogram.
    #[must_use]
    pub fn total_count(&self) -> u64 {
        self.buckets.iter().map(|b| b.count).sum()
    }

    /// Largest length with a non-zero count, or `None` when empty.
    #[must_use]
    pub fn max_len(&self) -> Option<usize> {
        self.buckets
            .iter()
            .filter(|b| b.count > 0)
            .map(|b| b.len)
            .max()
    }
}

/// Everything that can go wrong while fitting a zone into UDP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UdpFitError {
    /// The modelled zone has no leaves.
    #[error("zone has no leaves (leaf_count must be at least 1)")]
    EmptyZone,
    /// A length or count exceeded the model's integer range.
    #[error("response-length model overflowed its integer range")]
    Overflow,
    /// No plain SLH-DSA signature length was available to build config A/B.
    #[error("plain SLH-DSA signature length is missing (got 0)")]
    MissingPlainSignature,
}

/// Modelled per-size fit outcome for one (config, profile) pair.
#[derive(Debug, Clone, PartialEq)]
pub struct UdpFitRow {
    /// Signature configuration this row models.
    pub config: SignatureConfig,
    /// Profile name this row models.
    pub profile_name: String,
    /// Zone leaf count (one batch signs one ladder).
    pub size_n: u64,
    /// Messages the band counts are distributed over.
    pub total_messages: u64,
    /// Modelled largest response length in the distribution.
    pub max_bytes: usize,
    /// Messages whose response fits 1,232 bytes.
    pub fit_1232_count: u64,
    /// Percentage of messages fitting 1,232 bytes.
    pub fit_1232_percent: f64,
    /// Messages whose response exceeds 1,232 but not 4,096 bytes (TC band).
    pub tc_band_count: u64,
    /// Percentage of messages in the TC band.
    pub tc_band_percent: f64,
    /// Messages whose response exceeds 4,096 bytes.
    pub over_4096_count: u64,
    /// Percentage of messages whose response needs TCP.
    pub needs_tcp_percent: f64,
    /// TCP fallback cost for the largest modelled response.
    pub tcp: TcpFallbackCost,
}

/// Cost of falling back to TCP for one truncated response.
#[derive(Debug, Clone, PartialEq)]
pub struct TcpFallbackCost {
    /// Round trips: UDP query + TC reply, TCP handshake, TCP exchange.
    pub round_trips: usize,
    /// Bytes of the original UDP query.
    pub udp_query_bytes: usize,
    /// TCP payload bytes: the response plus its 2-byte length prefix, counted
    /// twice (request re-send + response) is *not* modelled — this is the
    /// response side only.
    pub tcp_payload_bytes: usize,
    /// Assumed round-trip time in milliseconds, when one was supplied.
    pub rtt_ms: Option<f64>,
    /// Modelled fallback delay: `round_trips × rtt_ms`, or `None` without an
    /// RTT assumption.
    pub ms_modeled: Option<f64>,
}

/// EDNS UDP payload size used as the fit threshold (1,232 bytes).
pub const UDP_MTU_1232: usize = 1232;
/// Practical response-size ceiling above which UDP is not attempted (4,096
/// bytes).
pub const UDP_MTU_4096: usize = 4096;

/// Dataset sizes the fit is reported at, in messages (one batch, one ladder).
pub const COMPARISON_SIZES: [u64; 4] = [100, 1_000, 10_000, 1_000_000];

/// Bytes of the original UDP query: DNS header (12) + question + EDNS OPT
/// (11).
#[must_use]
pub const fn udp_query_bytes(question_bytes: usize) -> usize {
    12 + question_bytes + 11
}

/// TCP fallback cost for a response of `response_bytes`, given the query's
/// question length and an optional RTT assumption.
///
/// Three round trips are counted: the UDP query and TC reply, the TCP
/// handshake, and the TCP query/response exchange. Without `rtt_ms` the
/// modelled delay stays `None` — no latency is invented.
#[must_use]
pub fn tcp_fallback_cost(
    response_bytes: usize,
    question_bytes: usize,
    rtt_ms: Option<f64>,
) -> TcpFallbackCost {
    let round_trips = 3;
    TcpFallbackCost {
        round_trips,
        udp_query_bytes: udp_query_bytes(question_bytes),
        tcp_payload_bytes: response_bytes + 4,
        rtt_ms,
        ms_modeled: rtt_ms.map(|r| r * round_trips as f64),
    }
}

/// Histogram of per-leaf condensed-signature lengths for a zone of
/// `leaf_count` leaves.
///
/// One bucket per distinct authentication-path depth; bucket counts sum to
/// `leaf_count`. An empty zone yields an empty histogram.
#[must_use]
pub fn condensed_histogram(leaf_count: u64) -> ResponseLengthHistogram {
    let mut map: BTreeMap<usize, u64> = BTreeMap::new();
    for leaf in 0..leaf_count {
        if let Some(len) = predicted_condensed_len(leaf_count, leaf) {
            *map.entry(len).or_insert(0) += 1;
        }
    }
    ResponseLengthHistogram {
        buckets: map
            .into_iter()
            .map(|(len, count)| CondensedHistogramBucket { len, count })
            .collect(),
    }
}

/// Histogram of per-leaf signature-field lengths for one configuration.
///
/// * `Plain`: one bucket, the plain signature length, for every leaf.
/// * `MtlFull`: `ladder + 4 + plain + 1 + condensed(leaf)` per leaf.
/// * `MtlAmortised`: `1 + condensed(leaf)` per leaf.
///
/// Bucket counts sum to `leaf_count`.
fn signature_field_histogram(
    config: SignatureConfig,
    leaf_count: u64,
    plain_signature_len: usize,
) -> Result<ResponseLengthHistogram, UdpFitError> {
    if plain_signature_len == 0 {
        return Err(UdpFitError::MissingPlainSignature);
    }
    match config {
        SignatureConfig::Plain => Ok(ResponseLengthHistogram {
            buckets: vec![CondensedHistogramBucket {
                len: plain_signature_len,
                count: leaf_count,
            }],
        }),
        SignatureConfig::MtlFull => {
            let fixed = predicted_ladder_len(leaf_count)
                .checked_add(4)
                .and_then(|v| v.checked_add(plain_signature_len))
                .and_then(|v| v.checked_add(1))
                .ok_or(UdpFitError::Overflow)?;
            let mut map: BTreeMap<usize, u64> = BTreeMap::new();
            for leaf in 0..leaf_count {
                let condensed =
                    predicted_condensed_len(leaf_count, leaf).ok_or(UdpFitError::Overflow)?;
                let len = fixed.checked_add(condensed).ok_or(UdpFitError::Overflow)?;
                *map.entry(len).or_insert(0) += 1;
            }
            Ok(ResponseLengthHistogram {
                buckets: map
                    .into_iter()
                    .map(|(len, count)| CondensedHistogramBucket { len, count })
                    .collect(),
            })
        }
        SignatureConfig::MtlAmortised => {
            let mut map: BTreeMap<usize, u64> = BTreeMap::new();
            for leaf in 0..leaf_count {
                let condensed =
                    predicted_condensed_len(leaf_count, leaf).ok_or(UdpFitError::Overflow)?;
                let len = 1usize.checked_add(condensed).ok_or(UdpFitError::Overflow)?;
                *map.entry(len).or_insert(0) += 1;
            }
            Ok(ResponseLengthHistogram {
                buckets: map
                    .into_iter()
                    .map(|(len, count)| CondensedHistogramBucket { len, count })
                    .collect(),
            })
        }
    }
}

/// Convolve `hist` with itself `rrsig_count` times: the distribution of the
/// sum of `rrsig_count` independent draws from `hist`.
///
/// `rrsig_count == 0` yields the delta at zero (one empty draw). An empty
/// histogram convolved a positive number of times stays empty. Returns
/// `None` when a bucket count or length would overflow, so an absurd input
/// surfaces as an error instead of a wrapped number.
#[must_use]
pub fn convolve_rrsigs(
    hist: &ResponseLengthHistogram,
    rrsig_count: usize,
) -> Option<ResponseLengthHistogram> {
    if rrsig_count == 0 {
        return Some(ResponseLengthHistogram {
            buckets: vec![CondensedHistogramBucket { len: 0, count: 1 }],
        });
    }
    if hist.buckets.is_empty() {
        return Some(hist.clone());
    }
    let mut acc = hist.clone();
    for _ in 1..rrsig_count {
        acc = convolve_once(&acc, hist)?;
    }
    Some(acc)
}

/// One pairwise convolution of two length histograms.
fn convolve_once(
    a: &ResponseLengthHistogram,
    b: &ResponseLengthHistogram,
) -> Option<ResponseLengthHistogram> {
    let mut map: BTreeMap<usize, u64> = BTreeMap::new();
    for x in &a.buckets {
        for y in &b.buckets {
            let len = x.len.checked_add(y.len)?;
            let count = x.count.checked_mul(y.count)?;
            let entry = map.entry(len).or_insert(0);
            *entry = entry.checked_add(count)?;
        }
    }
    Some(ResponseLengthHistogram {
        buckets: map
            .into_iter()
            .map(|(len, count)| CondensedHistogramBucket { len, count })
            .collect(),
    })
}

/// Distribute `total` messages over the buckets' shares by the
/// largest-remainder method.
///
/// Each bucket's exact share is `total × count / sum(counts)`; the integer
/// parts are assigned first and the leftover units (always fewer than the
/// bucket count) go to the largest fractional parts, ties broken by bucket
/// order. The result sums exactly to `total`.
fn allocate_counts(counts: &[u64], total: u64) -> Vec<u64> {
    let sum: u128 = counts.iter().map(|&c| u128::from(c)).sum();
    if sum == 0 || total == 0 {
        return vec![0; counts.len()];
    }
    let total_u = u128::from(total);
    let mut base: Vec<u64> = Vec::with_capacity(counts.len());
    let mut remainders = Vec::with_capacity(counts.len());
    let mut assigned: u128 = 0;
    for &c in counts {
        let exact = total_u * u128::from(c);
        let whole = exact / sum;
        base.push(u64::try_from(whole).unwrap_or(0));
        remainders.push(exact % sum);
        assigned += whole;
    }
    let leftover = total_u - assigned;
    let mut order: Vec<usize> = (0..counts.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(remainders[i]), i));
    for &i in order.iter().take(leftover.try_into().unwrap_or(0)) {
        base[i] = base[i].saturating_add(1);
    }
    base
}

/// Model the UDP fit of one (config, profile) pair for a zone of
/// `leaf_count` leaves signed as one batch, distributed over
/// `total_messages` messages.
///
/// `plain_signature_len` is the plain SLH-DSA signature length to model (a
/// property of the signature scheme and parameter set, read from a measured
/// artifact by callers — never guessed here).
pub fn udp_fit_for_size(
    config: SignatureConfig,
    profile: &ResponseProfile,
    leaf_count: u64,
    total_messages: u64,
    plain_signature_len: usize,
) -> Result<UdpFitRow, UdpFitError> {
    if leaf_count == 0 {
        return Err(UdpFitError::EmptyZone);
    }
    let per_leaf = signature_field_histogram(config, leaf_count, plain_signature_len)?;
    let sig_totals =
        convolve_rrsigs(&per_leaf, profile.rrsig_count).ok_or(UdpFitError::Overflow)?;
    let overhead = match config {
        SignatureConfig::Plain | SignatureConfig::MtlFull => profile.overhead_bytes,
        SignatureConfig::MtlAmortised => profile.overhead_bytes_sigttag,
    };
    let mut response_lens = Vec::with_capacity(sig_totals.buckets.len());
    for b in &sig_totals.buckets {
        let len = b.len.checked_add(overhead).ok_or(UdpFitError::Overflow)?;
        response_lens.push((len, b.count));
    }
    let counts: Vec<u64> = response_lens.iter().map(|&(_, c)| c).collect();
    let allocated = allocate_counts(&counts, total_messages);

    let mut fit_1232_count = 0u64;
    let mut tc_band_count = 0u64;
    let mut over_4096_count = 0u64;
    let mut max_bytes = 0usize;
    for ((len, _), &n) in response_lens.iter().zip(&allocated) {
        let len = *len;
        if n == 0 {
            continue;
        }
        max_bytes = max_bytes.max(len);
        if len <= UDP_MTU_1232 {
            fit_1232_count += n;
        } else if len <= UDP_MTU_4096 {
            tc_band_count += n;
        } else {
            over_4096_count += n;
        }
    }
    let pct = |count: u64| {
        if total_messages == 0 {
            0.0
        } else {
            100.0 * count as f64 / total_messages as f64
        }
    };
    Ok(UdpFitRow {
        config,
        profile_name: profile.name.clone(),
        size_n: leaf_count,
        total_messages,
        max_bytes,
        fit_1232_count,
        fit_1232_percent: pct(fit_1232_count),
        tc_band_count,
        tc_band_percent: pct(tc_band_count),
        over_4096_count,
        needs_tcp_percent: pct(over_4096_count),
        tcp: tcp_fallback_cost(max_bytes, profile.question_bytes, None),
    })
}

/// Infer the zone leaf count from one recorded full-signature field, one
/// recorded condensed field, and the plain signature length.
///
/// `condensed = cond_field − 1` pins the authentication-path depth `k`
/// (`condensed = CONDENSED_FIXED_LEN + k × HASH_SIZE`, so `(condensed −
/// CONDENSED_FIXED_LEN)` must divide evenly by the hash size). `ladder = full
/// − plain − 4 − cond_field` pins the rung count (`ladder = 36 + 32 ×
/// rungs`). Only a single-rung ladder determines `N` uniquely (then `N = 2^k`,
/// since one rung covers the whole zone); anything else is ambiguous and
/// yields `None`. This is a consistency check between the model and one
/// capture, not a general-purpose zone parser.
#[must_use]
pub fn infer_zone_leaf_count(
    full_field: usize,
    cond_field: usize,
    plain_signature_len: usize,
) -> Option<u64> {
    let condensed = cond_field.checked_sub(1)?;
    let extra = condensed.checked_sub(CONDENSED_FIXED_LEN)?;
    if extra % HASH_SIZE != 0 {
        return None;
    }
    let auth_path = extra / HASH_SIZE;
    let ladder = full_field
        .checked_sub(plain_signature_len)?
        .checked_sub(4)?
        .checked_sub(cond_field)?;
    let ladder_fixed = 2 + 2 * HASH_SIZE + 2;
    let rung_bytes = 2 * MTL_INDEX_LEN + HASH_SIZE;
    let rungs_minus = ladder.checked_sub(ladder_fixed)?;
    if rungs_minus % rung_bytes != 0 {
        return None;
    }
    if rungs_minus / rung_bytes != 1 {
        // More than one rung: the rung count pins popcount(N), not N itself.
        return None;
    }
    u64::try_from(auth_path)
        .ok()
        .and_then(|k| 1u64.checked_shl(u32::try_from(k).ok()?))
}

/// The response profiles compiled into the model (from
/// `data/udp-fit-profiles.json` next to this crate).
#[must_use]
pub fn builtin_profiles() -> Vec<ResponseProfile> {
    // The file ships with the crate and is parsed in tests; failure here is a
    // build-time packaging error, not a runtime condition.
    let data: ProfileData = serde_json::from_str(include_str!("../data/udp-fit-profiles.json"))
        .expect("built-in UDP-fit profile data is valid JSON");
    data.profiles
}

/// Load response profiles from an external JSON file, for overriding the
/// built-in set.
pub fn load_profiles_from_file(path: &str) -> Result<Vec<ResponseProfile>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    let data: ProfileData = serde_json::from_str(&content).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid json: {e}"),
        )
    })?;
    Ok(data.profiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain SLH-DSA signature length observed in the recorded capture's wire
    /// rows. Test data only — production callers read it from an artifact.
    const TEST_PLAIN_LEN: usize = 7856;

    fn profile(name: &str) -> ResponseProfile {
        builtin_profiles()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("built-in profile {name} missing"))
    }

    #[test]
    fn builtin_profiles_parse_with_expected_names_and_counts() {
        let profiles = builtin_profiles();
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["positive_a", "positive_a_with_ns", "nxdomain_nsec3"]
        );
        let counts: Vec<usize> = profiles.iter().map(|p| p.rrsig_count).collect();
        assert_eq!(counts, vec![1, 3, 2]);
        assert!(profiles.iter().all(|p| p.question_bytes == 23));
    }

    #[test]
    fn load_profiles_from_file_matches_builtin_set() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/udp-fit-profiles.json");
        let loaded = load_profiles_from_file(path).expect("profile file parses");
        assert_eq!(loaded, builtin_profiles());
    }

    #[test]
    fn load_profiles_from_file_rejects_invalid_json() {
        let dir = std::env::temp_dir().join(format!(
            "climb-udp-fit-bad-{}-{}",
            std::process::id(),
            "json"
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("bad.json");
        std::fs::write(&path, "{ not json").expect("write");
        let err = load_profiles_from_file(path.to_str().expect("utf-8 path"));
        assert!(err.is_err(), "invalid JSON must be rejected");
        let missing =
            load_profiles_from_file(dir.join("absent.json").to_str().expect("utf-8 path"));
        assert!(missing.is_err(), "missing file must be rejected");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn condensed_histogram_partitions_the_zone() {
        for leaf_count in [1u64, 2, 13, 16, 100] {
            let hist = condensed_histogram(leaf_count);
            assert_eq!(hist.total_count(), leaf_count, "N={leaf_count}");
            for b in &hist.buckets {
                let expected = CONDENSED_FIXED_LEN
                    + b.len.saturating_sub(CONDENSED_FIXED_LEN) / HASH_SIZE * HASH_SIZE;
                assert_eq!(b.len, expected, "bucket length is a condensed shape");
            }
        }
        assert!(condensed_histogram(0).buckets.is_empty());
    }

    #[test]
    fn signature_field_histograms_sum_to_the_zone() {
        for leaf_count in [1u64, 16, 100] {
            for config in [
                SignatureConfig::Plain,
                SignatureConfig::MtlFull,
                SignatureConfig::MtlAmortised,
            ] {
                let hist =
                    signature_field_histogram(config, leaf_count, TEST_PLAIN_LEN).expect("hist");
                assert_eq!(hist.total_count(), leaf_count, "{config:?} N={leaf_count}");
                assert!(hist.max_len().is_some());
            }
        }
    }

    #[test]
    fn convolution_matches_brute_force_enumeration() {
        let hist = ResponseLengthHistogram {
            buckets: vec![
                CondensedHistogramBucket { len: 10, count: 2 },
                CondensedHistogramBucket { len: 25, count: 1 },
            ],
        };
        let k = 3;
        let convolved = convolve_rrsigs(&hist, k).expect("convolution");
        // Counts sum to (sum of bucket counts)^k.
        assert_eq!(convolved.total_count(), 3u64.pow(k as u32));
        // Every enumerated triple appears with the right multiplicity.
        let mut expected: BTreeMap<usize, u64> = BTreeMap::new();
        for a in &hist.buckets {
            for b in &hist.buckets {
                for c in &hist.buckets {
                    *expected.entry(a.len + b.len + c.len).or_insert(0) +=
                        a.count * b.count * c.count;
                }
            }
        }
        let got: BTreeMap<usize, u64> =
            convolved.buckets.iter().map(|b| (b.len, b.count)).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn convolution_of_zero_rrsigs_is_a_delta_at_zero() {
        let hist = condensed_histogram(16);
        let convolved = convolve_rrsigs(&hist, 0).expect("convolution");
        assert_eq!(
            convolved.buckets,
            vec![CondensedHistogramBucket { len: 0, count: 1 }]
        );
    }

    #[test]
    fn convolution_of_an_empty_histogram_stays_empty() {
        let empty = condensed_histogram(0);
        let convolved = convolve_rrsigs(&empty, 3).expect("convolution");
        assert!(convolved.buckets.is_empty());
    }

    #[test]
    fn convolution_reports_count_overflow_instead_of_wrapping() {
        let hist = ResponseLengthHistogram {
            buckets: vec![CondensedHistogramBucket {
                len: 8,
                count: u64::MAX / 2 + 1,
            }],
        };
        assert!(convolve_rrsigs(&hist, 2).is_none());
    }

    #[test]
    fn n16_model_reproduces_the_recorded_signature_fields_exactly() {
        // The recorded capture: zone of 16 leaves, 3 RRSIGs per pass, full
        // fields of 8,069 B and condensed fields of 141 B (test data).
        let p = profile("positive_a_with_ns");
        let full =
            signature_field_histogram(SignatureConfig::MtlFull, 16, TEST_PLAIN_LEN).expect("hist");
        assert_eq!(full.max_len(), Some(8_069), "full signature field");
        let cond = signature_field_histogram(SignatureConfig::MtlAmortised, 16, TEST_PLAIN_LEN)
            .expect("hist");
        assert_eq!(cond.max_len(), Some(141), "condensed signature field");
        assert_eq!(p.rrsig_count, 3);
    }

    #[test]
    fn n16_model_reproduces_the_recorded_wire_lengths_at_zero_tolerance() {
        // Recorded wire lengths (test data): full response 24,438 B, condensed
        // response 658 B. The model must match byte-for-byte.
        let p = profile("positive_a_with_ns");
        let full =
            udp_fit_for_size(SignatureConfig::MtlFull, &p, 16, 16, TEST_PLAIN_LEN).expect("fit");
        assert_eq!(full.max_bytes, 24_438, "full response wire length");
        let cond = udp_fit_for_size(SignatureConfig::MtlAmortised, &p, 16, 16, TEST_PLAIN_LEN)
            .expect("fit");
        assert_eq!(cond.max_bytes, 658, "condensed response wire length");
    }

    #[test]
    fn positive_a_profile_reproduces_its_recorded_total() {
        // The single-RRSIG profile records 8,176 B = 107 + 8,069 (test data).
        let p = profile("positive_a");
        let full =
            udp_fit_for_size(SignatureConfig::MtlFull, &p, 16, 16, TEST_PLAIN_LEN).expect("fit");
        assert_eq!(full.max_bytes, 8_176);
    }

    #[test]
    fn band_counts_partition_the_total_messages() {
        for (config, leaf_count, total) in [
            (SignatureConfig::MtlAmortised, 16u64, 16u64),
            (SignatureConfig::MtlFull, 16, 16),
            (SignatureConfig::Plain, 16, 16),
            (SignatureConfig::MtlFull, 1_000, 1_000),
            (SignatureConfig::MtlAmortised, 10_000, 10_000),
        ] {
            for name in ["positive_a", "positive_a_with_ns", "nxdomain_nsec3"] {
                let p = profile(name);
                let row =
                    udp_fit_for_size(config, &p, leaf_count, total, TEST_PLAIN_LEN).expect("fit");
                let sum = row.fit_1232_count + row.tc_band_count + row.over_4096_count;
                assert_eq!(
                    sum, total,
                    "{config:?}/{name} N={leaf_count}: bands must sum to the total"
                );
                let pct_sum = row.fit_1232_percent + row.tc_band_percent + row.needs_tcp_percent;
                assert!(
                    (pct_sum - 100.0).abs() < 1e-9,
                    "{config:?}/{name}: percentages sum to 100, got {pct_sum}"
                );
            }
        }
    }

    #[test]
    fn largest_remainder_allocation_is_exact_and_prefers_larger_fractions() {
        // 10 messages over shares 1/3, 1/3, 1/6+1/6 → exact parts 3,3,1,1
        // with 2 leftover units going to the two largest fractional parts.
        let counts = vec![1u64, 1, 1];
        let alloc = allocate_counts(&counts, 10);
        let sum: u64 = alloc.iter().sum();
        assert_eq!(sum, 10);
        assert_eq!(alloc, vec![4, 3, 3], "ties broken by bucket order");
        // Zero total and zero shares both allocate nothing.
        assert_eq!(allocate_counts(&counts, 0), vec![0, 0, 0]);
        assert_eq!(allocate_counts(&[0, 0], 7), vec![0, 0]);
    }

    #[test]
    fn tcp_fallback_cost_counts_three_round_trips_and_response_payload() {
        let cost = tcp_fallback_cost(24_438, 23, None);
        assert_eq!(cost.round_trips, 3);
        assert_eq!(cost.udp_query_bytes, 12 + 23 + 11);
        assert_eq!(cost.tcp_payload_bytes, 24_438 + 4);
        assert_eq!(cost.rtt_ms, None);
        assert_eq!(cost.ms_modeled, None, "no RTT, no modelled latency");
    }

    #[test]
    fn tcp_cost_milliseconds_require_an_rtt_assumption() {
        let without = tcp_fallback_cost(658, 23, None);
        assert_eq!(without.ms_modeled, None);
        let with = tcp_fallback_cost(658, 23, Some(20.0));
        assert_eq!(with.rtt_ms, Some(20.0));
        assert_eq!(with.ms_modeled, Some(60.0));
    }

    #[test]
    fn udp_query_bytes_counts_header_question_and_opt() {
        assert_eq!(udp_query_bytes(23), 46);
    }

    #[test]
    fn empty_zone_is_an_error() {
        let p = profile("positive_a");
        let err = udp_fit_for_size(SignatureConfig::MtlFull, &p, 0, 0, TEST_PLAIN_LEN);
        assert_eq!(err, Err(UdpFitError::EmptyZone));
    }

    #[test]
    fn missing_plain_signature_is_an_error() {
        let p = profile("positive_a");
        let err = udp_fit_for_size(SignatureConfig::MtlFull, &p, 16, 16, 0);
        assert_eq!(err, Err(UdpFitError::MissingPlainSignature));
    }

    #[test]
    fn infer_zone_leaf_count_recovers_sixteen_from_recorded_fields() {
        // Recorded capture (test data): full field 8,069, condensed field 141,
        // plain 7,856 → single rung, auth path 4 → 16 leaves.
        assert_eq!(infer_zone_leaf_count(8_069, 141, TEST_PLAIN_LEN), Some(16));
    }

    #[test]
    fn infer_zone_leaf_count_is_ambiguous_for_multi_rung_ladders() {
        // Same plain and condensed fields but a two-rung ladder (64 + 36 = 100
        // bytes): popcount is pinned, N is not (test data).
        let full = 100 + 4 + TEST_PLAIN_LEN + 1 + 140;
        assert_eq!(infer_zone_leaf_count(full, 141, TEST_PLAIN_LEN), None);
    }

    #[test]
    fn infer_zone_leaf_count_rejects_inconsistent_fields() {
        // Undivisible condensed extra.
        assert_eq!(infer_zone_leaf_count(8_069, 77 + 1, TEST_PLAIN_LEN), None);
        // Underflowing full field.
        assert_eq!(infer_zone_leaf_count(100, 141, TEST_PLAIN_LEN), None);
        // Ladder too short for its fixed part.
        assert_eq!(infer_zone_leaf_count(7_897, 141, TEST_PLAIN_LEN), None);
        // Rung bytes not dividing evenly.
        let full = 36 + 17 + 4 + TEST_PLAIN_LEN + 1 + 140;
        assert_eq!(infer_zone_leaf_count(full, 141, TEST_PLAIN_LEN), None);
    }

    #[test]
    fn fit_row_carries_the_tcp_cost_without_invented_latency() {
        let p = profile("positive_a_with_ns");
        let row =
            udp_fit_for_size(SignatureConfig::MtlFull, &p, 16, 16, TEST_PLAIN_LEN).expect("fit");
        assert_eq!(row.tcp.round_trips, 3);
        assert_eq!(row.tcp.tcp_payload_bytes, row.max_bytes + 4);
        assert_eq!(row.tcp.ms_modeled, None);
        assert_eq!(row.size_n, 16);
        assert_eq!(row.total_messages, 16);
        assert_eq!(row.profile_name, "positive_a_with_ns");
    }

    #[test]
    fn zero_messages_yields_zero_bands_and_no_max() {
        let p = profile("positive_a");
        let row =
            udp_fit_for_size(SignatureConfig::MtlFull, &p, 16, 0, TEST_PLAIN_LEN).expect("fit");
        assert_eq!(row.fit_1232_count, 0);
        assert_eq!(row.tc_band_count, 0);
        assert_eq!(row.over_4096_count, 0);
        assert_eq!(row.max_bytes, 0);
        assert_eq!(row.fit_1232_percent, 0.0);
    }
}
