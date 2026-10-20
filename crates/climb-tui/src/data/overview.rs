//! Overview events: KPI cards, environment panel and artifact health.

use std::time::Duration;

use climb_report::charts::{fmt_count, per_msg_mean};
use climb_report::DnsCaptureSource;

use super::ResultsView;

/// One headline number for a KPI card: the big value plus supporting caption.
///
/// Extrapolated values carry the `≈` prefix in [`Kpi::value`] and
/// [`KpiKind::Extrapolated`] so the UI can style and footnote them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kpi {
    pub value: String,
    pub caption: String,
    pub kind: KpiKind,
}

/// Where a KPI number came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KpiKind {
    Measured,
    Extrapolated,
}

impl Kpi {
    fn measured(value: impl Into<String>, caption: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            caption: caption.into(),
            kind: KpiKind::Measured,
        }
    }

    fn extrapolated(value: impl Into<String>, caption: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            caption: caption.into(),
            kind: KpiKind::Extrapolated,
        }
    }
}

impl ResultsView {
    // --- KPI 1: live/recorded DNS response reduction -----------------------

    /// The headline DNS reduction (full vs. condensed response bytes), from
    /// the capture or recorded fixture.
    pub fn dns_reduction(&self) -> Option<Kpi> {
        let d = self.results.dns_pipeline.as_ref()?;
        let full = d.full_signature_bytes_www_a?;
        let condensed = d.condensed_proof_bytes_www_a?;
        let percent = d.reduction_percent_www_a?;
        Some(Kpi::measured(
            format!("{percent:.1}%"),
            format!(
                "{} B \u{2192} {} B",
                fmt_count(full as f64),
                fmt_count(condensed as f64)
            ),
        ))
    }

    // --- KPI 2: wire reduction at the largest size -------------------------

    /// Wire reduction at the largest measured size (the one that matters for
    /// ladder scaling).
    pub fn wire_reduction_largest(&self) -> Option<Kpi> {
        let largest = self
            .results
            .wire_size
            .iter()
            .max_by_key(|w| w.message_count)?;
        Some(Kpi::measured(
            format!("\u{2212}{:.1}%", largest.reduction_percent),
            format!("{} msgs", fmt_count(largest.message_count as f64)),
        ))
    }

    // --- KPI 3: signing speed-up at one size --------------------------------

    /// Signing speed-up at `size`: without-MTL mean ÷ with-MTL mean.
    ///
    /// Measured when both conditions exist for that size; otherwise
    /// extrapolated from the per-message cost of measured without-MTL rows,
    /// exactly as the charts do ([`per_msg_mean`]).
    pub fn signing_speedup_at(&self, size: &str) -> Option<Kpi> {
        let rows: Vec<_> = self
            .results
            .benchmark
            .signing
            .iter()
            .filter(|r| r.size.eq_ignore_ascii_case(size))
            .collect();
        let with = rows
            .iter()
            .find(|r| r.condition.contains("With MTL"))
            .map(|r| r.mean_ns)?;
        if with <= 0.0 {
            return None;
        }
        match rows
            .iter()
            .find(|r| r.condition == "Without MTL")
            .map(|r| r.mean_ns)
        {
            Some(without) => Some(Kpi::measured(speedup(without, with), size.to_string())),
            None => {
                let per_msg = per_msg_mean(&self.results.benchmark.signing, "Without MTL")?;
                let messages = rows
                    .iter()
                    .find(|r| r.condition.contains("With MTL"))
                    .map(|r| r.messages)?;
                if messages == 0 {
                    return None;
                }
                Some(Kpi::extrapolated(
                    format!("≈{}", speedup(per_msg * messages as f64, with)),
                    size.to_string(),
                ))
            }
        }
    }

    // --- KPI 4: cached-ladder verifying speed-up ----------------------------

    /// Average trust-cached verifying speed-up across sizes (derived by the
    /// consolidator, same number the report's derived summary carries).
    pub fn cached_verify_speedup(&self) -> Option<Kpi> {
        let avg = self
            .results
            .derived
            .overall_verifying_trust_cached_speedup_avg?;
        Some(Kpi::measured(
            format!("{}x", fmt_count(avg)),
            "trust-cached verify".to_string(),
        ))
    }

    // --- KPI 5: C vs A (plain) three-way hero metric -----------------------

    /// Configuration C (MTL amortised) vs A (plain SLH-DSA) per-message
    /// signing speed-up at the Medium size — the "hero" three-way metric.
    /// Reads from the consolidated `comparisons` array (Derived, never typed).
    pub fn c_vs_a_signing_speedup(&self) -> Option<Kpi> {
        let comp = self
            .results
            .comparisons
            .iter()
            .find(|c| c.size.eq_ignore_ascii_case("Medium"))?;
        let speedup = comp.signing_speedup_c_vs_a?;
        let class = comp.class;
        let kind = if class == climb_report::results::EvidenceClass::Measured {
            KpiKind::Measured
        } else {
            KpiKind::Extrapolated
        };
        let prefix = if kind == KpiKind::Extrapolated {
            "≈"
        } else {
            ""
        };
        Some(Kpi {
            value: format!("{}{}x", prefix, fmt_count(speedup)),
            caption: "C vs A signing (Medium)".to_string(),
            kind,
        })
    }

    // --- Environment --------------------------------------------------------

    pub fn cpu_model(&self) -> Option<String> {
        json_str(&self.results.environment.cpu, "model_name")
    }

    pub fn governor(&self) -> Option<String> {
        json_str(&self.results.environment.benchmark_runtime, "cpu_governor")
    }

    pub fn pinned_core(&self) -> Option<String> {
        self.results
            .environment
            .benchmark_runtime
            .get("pin_cpu")
            .and_then(|v| v.as_u64())
            .map(|core| format!("cpu {core}"))
    }

    pub fn rustc(&self) -> Option<String> {
        json_str(&self.results.environment.toolchain, "rustc_version")
    }

    pub fn mtl_sha(&self) -> Option<String> {
        sha7(&self.results.environment.submodules, "MTL")
    }

    pub fn liboqs_sha(&self) -> Option<String> {
        sha7(&self.results.environment.submodules, "liboqs")
    }

    /// The DNS pipeline algorithm (from the capture; never typed in the TUI).
    pub fn algorithm(&self) -> Option<String> {
        self.results
            .dns_pipeline
            .as_ref()
            .and_then(|d| d.algorithm.clone())
    }

    // --- Artifact health ----------------------------------------------------

    /// "live capture" / "recorded <yyyy-mm-dd>" / None when no capture exists.
    pub fn dns_provenance(&self) -> Option<String> {
        let d = self.results.dns_pipeline.as_ref()?;
        let when = d
            .recorded_at
            .as_deref()
            .and_then(|ts| ts.get(..10))
            .map(|date| format!("recorded {date}"))
            .unwrap_or_else(|| "recorded fixture".to_string());
        match d.source {
            DnsCaptureSource::Live => Some("live capture".to_string()),
            DnsCaptureSource::RecordedFixture => Some(when),
        }
    }

    // --- Signature context ---------------------------------------------------

    /// Rows for the compact signature-context table: the classical baselines
    /// beside configurations A/B/C at the headline dataset size. Empty when
    /// the artifact carries no `derived.signature_context` section, so the
    /// Overview renders nothing rather than a blank frame. Every cell is
    /// formatted from the artifact's numbers, never typed here.
    pub fn context_table(&self) -> Vec<ContextRow> {
        self.results
            .derived
            .signature_context
            .iter()
            .map(|row| {
                let letter = row.label.chars().next().unwrap_or('\u{2014}');
                let name = row
                    .label
                    .split_once('(')
                    .and_then(|(_, inner)| inner.strip_suffix(')'))
                    .unwrap_or(&row.label)
                    .to_string();
                ContextRow {
                    letter: letter.to_string(),
                    name,
                    bytes: row
                        .signature_bytes
                        .map(context_bytes)
                        .unwrap_or_else(|| "\u{2014}".to_string()),
                    sign: row
                        .signing_mean_ns
                        .map(|ns| format!("{:.2}", ns / 1000.0))
                        .unwrap_or_else(|| "\u{2014}".to_string()),
                    verify: row
                        .verifying_mean_ns
                        .map(|ns| format!("{:.2}", ns / 1000.0))
                        .unwrap_or_else(|| "\u{2014}".to_string()),
                    fits: row
                        .fits_1232_percent
                        .map(context_percent)
                        .unwrap_or_else(|| "\u{2014}".to_string()),
                    classical: row.family == "classical (context)",
                }
            })
            .collect()
    }
}

/// One row of the compact signature-context table on the Overview tab.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextRow {
    /// One-letter configuration id (E/P/R for the classical baselines, A/B/C
    /// for the post-quantum configurations) — the first character of the
    /// artifact's label.
    pub letter: String,
    /// The label without its letter wrapper, e.g. `Ed25519, context`.
    pub name: String,
    /// Signature bytes carried per message on the wire.
    pub bytes: String,
    /// Per-message sign cost, microseconds per operation.
    pub sign: String,
    /// Per-message verify cost, microseconds per operation.
    pub verify: String,
    /// Share of modelled responses that fit the UDP payload budget.
    pub fits: String,
    /// True for the classical context rows (not quantum-resistant).
    pub classical: bool,
}

/// Format a signature size for the compact context table: integral byte
/// counts group with thousands separators, fractional per-message shares keep
/// two decimals.
fn context_bytes(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{} B", fmt_count(v))
    } else {
        format!("{v:.2} B")
    }
}

/// Format a share-of-responses percentage for the compact context table:
/// whole percents without decimals, otherwise two.
fn context_percent(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}%")
    } else {
        format!("{v:.2}%")
    }
}

fn speedup(slower: f64, faster: f64) -> String {
    format!("{}x", fmt_count(slower / faster))
}

/// Read a string field from an environment JSON object, if present.
pub(super) fn json_str(obj: &serde_json::Value, key: &str) -> Option<String> {
    obj.get(key)?.as_str().map(str::to_string)
}

/// First 7 characters of the sha of the submodule whose path contains
/// `needle`, if present.
fn sha7(submodules: &[serde_json::Value], needle: &str) -> Option<String> {
    submodules.iter().find_map(|sub| {
        let path = sub.get("path")?.as_str()?;
        if path.contains(needle) {
            sub.get("sha")?
                .as_str()
                .map(|sha| sha.chars().take(7).collect())
        } else {
            None
        }
    })
}

/// Cadence of the Overview ladder animation: one rung lights every tick.
pub const RUNG_TICK: Duration = Duration::from_millis(150);

/// Which of `rungs` ladder rungs is lit at `elapsed`, bottom (`0`) to top,
/// looping forever. Pure so animation tests need no clock.
pub fn rung_state(elapsed: Duration, rungs: usize) -> usize {
    if rungs == 0 {
        return 0;
    }
    let step = elapsed.as_millis() / RUNG_TICK.as_millis();
    (step % rungs as u128) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::test_support::{
        dns_headline, empty_derived, signing_row, view_with, wire_row,
    };
    use climb_report::results::{DerivedSummary, EnvironmentFingerprint};

    #[test]
    fn dns_kpi_matches_the_report_headline_numbers() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(dns_headline(24_438, 658)),
            empty_derived(),
        );
        let kpi = view.dns_reduction().expect("dns headline present");
        assert_eq!(kpi.value, "97.3%");
        assert_eq!(
            kpi.caption, "24,438 B \u{2192} 658 B",
            "same strings as the report"
        );
        assert_eq!(kpi.kind, KpiKind::Measured);
    }

    #[test]
    fn dns_kpi_is_none_without_a_capture() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert_eq!(view.dns_reduction(), None);
    }

    #[test]
    fn wire_kpi_picks_the_largest_message_count() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            vec![
                wire_row("large", 1_000_000, 97.0),
                wire_row("small", 100, 50.0),
            ],
            None,
            empty_derived(),
        );
        let kpi = view.wire_reduction_largest().expect("wire row present");
        assert_eq!(kpi.value, "\u{2212}97.0%");
        assert_eq!(kpi.caption, "1,000,000 msgs");
    }

    #[test]
    fn wire_kpi_is_none_without_wire_rows() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert_eq!(view.wire_reduction_largest(), None);
    }

    #[test]
    fn signing_speedup_measured_when_both_conditions_present() {
        let view = view_with(
            vec![
                signing_row("Medium", "With MTL", 1000.0),
                signing_row("Medium", "Without MTL", 40_000.0),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let kpi = view.signing_speedup_at("Medium").expect("row present");
        assert_eq!(kpi.value, "40x");
        assert_eq!(kpi.caption, "Medium");
        assert_eq!(kpi.kind, KpiKind::Measured);
    }

    #[test]
    fn signing_speedup_extrapolated_when_without_mtl_missing_at_size() {
        let view = view_with(
            vec![
                signing_row("Medium", "With MTL", 1000.0),
                signing_row("Small", "Without MTL", 100.0),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let kpi = view
            .signing_speedup_at("Medium")
            .expect("with-mtl row present");
        assert_eq!(kpi.kind, KpiKind::Extrapolated);
        assert!(
            kpi.value.starts_with('≈'),
            "extrapolated values carry the ≈ prefix, got {}",
            kpi.value
        );
    }

    #[test]
    fn signing_speedup_is_none_without_any_medium_row() {
        let view = view_with(
            vec![signing_row("Small", "With MTL", 1000.0)],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        assert_eq!(view.signing_speedup_at("Medium"), None);
    }

    #[test]
    fn cached_verify_kpi_comes_from_the_derived_summary() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            DerivedSummary {
                overall_verifying_trust_cached_speedup_avg: Some(388.162),
                ..empty_derived()
            },
        );
        let kpi = view
            .cached_verify_speedup()
            .expect("derived average present");
        assert_eq!(kpi.value, "388x");
        assert_eq!(kpi.kind, KpiKind::Measured);
    }

    #[test]
    fn none_sources_render_as_not_recorded_by_the_ui() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert_eq!(view.dns_reduction(), None);
        assert_eq!(view.wire_reduction_largest(), None);
        assert_eq!(view.signing_speedup_at("Medium"), None);
        assert_eq!(view.cached_verify_speedup(), None);
    }

    #[test]
    fn environment_accessors_read_the_json_fields() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(dns_headline(10, 5)),
            empty_derived(),
        );
        assert_eq!(view.cpu_model().as_deref(), Some("Test CPU Model"));
        assert_eq!(view.governor().as_deref(), Some("powersave"));
        assert_eq!(view.pinned_core().as_deref(), Some("cpu 2"));
        assert_eq!(view.rustc().as_deref(), Some("rustc 1.98.1 (x 2026-01-01)"));
        assert_eq!(view.mtl_sha().as_deref(), Some("abcdef0"));
        assert_eq!(view.liboqs_sha().as_deref(), Some("0123456"));
        assert_eq!(
            view.algorithm().as_deref(),
            Some("SLH-DSA-SHA2-128s-MTL-SHA2-128")
        );
    }

    #[test]
    fn environment_accessors_are_none_when_fields_are_absent() {
        let empty = EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "t".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({}),
            cpu: serde_json::json!({}),
            submodules: Vec::new(),
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({}),
        };
        let mut view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        view.results.environment = empty;
        assert_eq!(view.cpu_model(), None);
        assert_eq!(view.governor(), None);
        assert_eq!(view.pinned_core(), None);
        assert_eq!(view.rustc(), None);
        assert_eq!(view.mtl_sha(), None);
        assert_eq!(view.liboqs_sha(), None);
        assert_eq!(view.algorithm(), None);
        assert_eq!(view.dns_provenance(), None);
    }

    #[test]
    fn dns_provenance_uses_the_recorded_date() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(dns_headline(10, 5)),
            empty_derived(),
        );
        assert_eq!(
            view.dns_provenance().as_deref(),
            Some("recorded 2026-10-12")
        );
    }

    #[test]
    fn rung_state_lights_bottom_to_top_and_loops() {
        assert_eq!(rung_state(Duration::ZERO, 5), 0);
        assert_eq!(rung_state(Duration::from_millis(149), 5), 0);
        assert_eq!(rung_state(Duration::from_millis(150), 5), 1);
        assert_eq!(rung_state(Duration::from_millis(300), 5), 2);
        assert_eq!(
            rung_state(Duration::from_millis(750), 5),
            0,
            "wraps after full cycle"
        );
        assert_eq!(rung_state(Duration::from_millis(900), 5), 1);
    }

    #[test]
    fn rung_state_degrades_gracefully() {
        assert_eq!(rung_state(Duration::from_secs(1), 0), 0);
        assert_eq!(rung_state(Duration::from_secs(1), 1), 0);
    }

    #[test]
    fn context_table_reads_every_cell_from_the_artifact() {
        use climb_report::results::{EvidenceClass, SignatureContextRow};

        let mut view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        view.results.derived.signature_context = vec![
            SignatureContextRow {
                config: "classical-ed25519".to_string(),
                label: "E (Ed25519, context)".to_string(),
                family: "classical (context)".to_string(),
                size: "Medium".to_string(),
                signature_bytes: Some(64.0),
                signing_mean_ns: Some(1_100.0),
                verifying_mean_ns: Some(220.0),
                fits_1232_percent: Some(100.0),
                class: EvidenceClass::Measured,
            },
            SignatureContextRow {
                config: "classical-rsa2048".to_string(),
                label: "R (RSA-2048, context)".to_string(),
                family: "classical (context)".to_string(),
                size: "Medium".to_string(),
                signature_bytes: Some(256.0),
                signing_mean_ns: Some(9_900.0),
                verifying_mean_ns: None,
                fits_1232_percent: Some(99.5),
                class: EvidenceClass::Measured,
            },
            SignatureContextRow {
                config: "mtl-amortised".to_string(),
                label: "C (MTL amortised)".to_string(),
                family: "post-quantum".to_string(),
                size: "Medium".to_string(),
                signature_bytes: Some(6.58),
                signing_mean_ns: Some(10_000.0),
                verifying_mean_ns: Some(2_000.0),
                fits_1232_percent: Some(100.0),
                class: EvidenceClass::Measured,
            },
        ];

        let rows = view.context_table();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].letter, "E", "letter is the label's first character");
        assert_eq!(rows[0].name, "Ed25519, context", "wrapper stripped");
        assert_eq!(rows[0].bytes, "64 B");
        assert_eq!(rows[0].sign, "1.10", "ns means render as µs");
        assert_eq!(rows[0].verify, "0.22");
        assert_eq!(rows[0].fits, "100%", "whole percents drop decimals");
        assert!(rows[0].classical);

        assert_eq!(rows[1].letter, "R");
        assert_eq!(rows[1].bytes, "256 B");
        assert_eq!(
            rows[1].verify, "\u{2014}",
            "a missing cell is an em dash, never a zero"
        );
        assert_eq!(rows[1].fits, "99.50%", "fractions keep two decimals");

        assert_eq!(rows[2].letter, "C");
        assert_eq!(rows[2].name, "MTL amortised");
        assert_eq!(rows[2].bytes, "6.58 B", "fractional per-message share");
        assert!(!rows[2].classical, "post-quantum rows are not classical");
    }

    #[test]
    fn context_table_is_empty_without_the_artifact_section() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert!(
            view.context_table().is_empty(),
            "no section means the Overview renders no table"
        );
    }
}
