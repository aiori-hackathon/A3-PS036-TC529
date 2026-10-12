//! Read-only view model over the consolidated results file (phase 19.1).
//!
//! [`load`] is the only I/O entry point and never panics: a missing or
//! unparseable file becomes a [`LoadState`] variant the UI renders instead of
//! an unrecovered crash. Every KPI string is derived from the parsed
//! [`ClimbResults`] through `climb_report::charts::fmt_*` — no result number
//! appears as a literal anywhere in this crate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use climb_report::charts::{fmt_count, per_msg_mean};
use climb_report::{ClimbResults, DnsCaptureSource};

/// How the consolidated results file looks at load time.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadState {
    /// No load attempt has happened yet (the startup load runs before the
    /// first frame).
    NotLoaded,
    /// The file parsed; [`ResultsView`] holds the derived view model.
    /// Boxed so the enum stays small (one large variant, hot path is a single
    /// deref).
    Loaded(Box<ResultsView>),
    /// The file does not exist; `cargo xtask consolidate` creates it.
    Missing { path: PathBuf },
    /// The file exists but could not be read or parsed.
    Invalid { path: PathBuf, error: String },
}

impl LoadState {
    /// Short one-line summary for the trace log.
    pub fn summary(&self) -> String {
        match self {
            LoadState::NotLoaded => "not loaded".to_string(),
            LoadState::Loaded(_) => "loaded".to_string(),
            LoadState::Missing { path } => format!("missing: {}", path.display()),
            LoadState::Invalid { path, .. } => format!("invalid: {}", path.display()),
        }
    }

    /// Whether results are currently usable in the UI.
    pub fn is_loaded(&self) -> bool {
        matches!(self, LoadState::Loaded(_))
    }

    /// Whether a load attempt failed (missing or invalid file).
    pub fn failed(&self) -> bool {
        matches!(self, LoadState::Missing { .. } | LoadState::Invalid { .. })
    }
}

/// Whether the embedded report claims match the results file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportHealth {
    /// No `climb-report/report.html` sits next to the results file.
    NoReport,
    /// Every embedded claim matches the results file.
    Agree,
    /// `n` embedded claims disagree with the results file.
    Drift(usize),
}

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

/// Results files older than this count as stale (warning marker, not an error).
const MAX_FRESH_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Derived, render-ready view over a parsed [`ClimbResults`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResultsView {
    results: ClimbResults,
    /// Age of the results file at load time.
    age: Option<Duration>,
    report_health: ReportHealth,
}

impl ResultsView {
    pub fn new(results: ClimbResults, age: Option<Duration>, report_health: ReportHealth) -> Self {
        Self {
            results,
            age,
            report_health,
        }
    }

    /// The parsed results.
    pub fn results(&self) -> &ClimbResults {
        &self.results
    }

    /// Results-file age at load time; `None` when the timestamp is unknown.
    pub fn age(&self) -> Option<Duration> {
        self.age
    }

    /// True when the results file is fresh enough to highlight as healthy.
    pub fn is_fresh(&self) -> bool {
        matches!(self.age, Some(age) if age <= MAX_FRESH_AGE)
    }

    pub fn report_health(&self) -> ReportHealth {
        self.report_health
    }

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
}

fn speedup(slower: f64, faster: f64) -> String {
    format!("{}x", fmt_count(slower / faster))
}

/// Read a string field from an environment JSON object, if present.
fn json_str(obj: &serde_json::Value, key: &str) -> Option<String> {
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

/// Load and parse the consolidated results, returning a renderable state.
///
/// Reading and parsing failures become [`LoadState`] variants — this function
/// never panics and never returns a partial view.
pub fn load(path: &Path) -> LoadState {
    let metadata = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(_) => {
            return LoadState::Missing {
                path: path.to_path_buf(),
            }
        }
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return LoadState::Invalid {
                path: path.to_path_buf(),
                error: error.to_string(),
            };
        }
    };
    let results: ClimbResults = match serde_json::from_str(&source) {
        Ok(results) => results,
        Err(error) => {
            return LoadState::Invalid {
                path: path.to_path_buf(),
                error: error.to_string(),
            };
        }
    };
    let age = metadata.modified().ok().and_then(|m| m.elapsed().ok());
    let report_health = report_health(path, &results);
    LoadState::Loaded(Box::new(ResultsView::new(results, age, report_health)))
}

/// Check the `climb-report/report.html` next to the results file for claim
/// drift. Missing report counts as `NoReport`, never an error.
fn report_health(results_path: &Path, results: &ClimbResults) -> ReportHealth {
    let html = match std::fs::read_to_string(report_path(results_path)) {
        Ok(html) => html,
        Err(_) => return ReportHealth::NoReport,
    };
    let failures = climb_report::check_report(&html, results);
    if failures.is_empty() {
        ReportHealth::Agree
    } else {
        ReportHealth::Drift(failures.len())
    }
}

/// `climb-report/report.html` next to the results file.
fn report_path(results_path: &Path) -> PathBuf {
    results_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("climb-report")
        .join("report.html")
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
    use climb_report::results::{
        AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, EnvironmentFingerprint,
        MemoryResults, SigningBenchmarkResult, VerifyingBenchmarkResult, WireSizeResult,
    };
    use climb_report::DnsPipelineHeadline;

    fn signing_row(size: &str, condition: &str, mean_ns: f64) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 1,
            mean_ns,
            ci_lower_ns: mean_ns * 0.9,
            ci_upper_ns: mean_ns * 1.1,
            speedup: None,
        }
    }

    fn environment() -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "2026-10-12T00:00:00Z".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({ "rustc_version": "rustc 1.98.1 (x 2026-01-01)" }),
            cpu: serde_json::json!({ "model_name": "Test CPU Model" }),
            submodules: vec![
                serde_json::json!({ "path": "third_party/liboqs", "sha": "0123456789abcdef" }),
                serde_json::json!({ "path": "third_party/MTL", "sha": "abcdef0123456789" }),
            ],
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({ "cpu_governor": "powersave", "pin_cpu": 2 }),
        }
    }

    fn view_with(
        signing: Vec<SigningBenchmarkResult>,
        verifying: Vec<VerifyingBenchmarkResult>,
        wire: Vec<WireSizeResult>,
        dns: Option<DnsPipelineHeadline>,
        derived: DerivedSummary,
    ) -> ResultsView {
        ResultsView::new(
            ClimbResults {
                version: "1.0".to_string(),
                generated_at: "2026-10-12T00:00:00Z".to_string(),
                workspace_root: "/tmp/climb".to_string(),
                benchmark: BenchmarkResults { signing, verifying },
                wire_size: wire,
                memory: MemoryResults {
                    measured: Vec::new(),
                    analytical_model: AnalyticalMemoryModel {
                        tree_page_size_bytes: 4096,
                        tree_max_pages: 1,
                        randomizer_pages: 1,
                        hash_size_bytes: 32,
                    },
                },
                environment: environment(),
                dns_pipeline: dns,
                derived,
                warnings: Vec::new(),
            },
            Some(Duration::from_secs(60)),
            ReportHealth::Agree,
        )
    }

    fn dns_headline(full: u64, condensed: u64) -> DnsPipelineHeadline {
        DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(full),
            condensed_proof_bytes_www_a: Some(condensed),
            reduction_percent_www_a: Some((full - condensed) as f64 / full as f64 * 100.0),
            notes: None,
            source: DnsCaptureSource::RecordedFixture,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        }
    }

    fn wire_row(size: &str, messages: usize, reduction: f64) -> WireSizeResult {
        WireSizeResult {
            size: size.to_string(),
            message_count: messages,
            ladder_len: 120,
            condensed_min: 100,
            condensed_mean: 100.0,
            condensed_max: 100,
            condensed_uniform: true,
            with_mtl_total_bytes: 100,
            without_mtl_total_bytes: 1000,
            reduction_percent: reduction,
        }
    }

    fn empty_derived() -> DerivedSummary {
        DerivedSummary {
            overall_wire_reduction_percent: None,
            largest_wire_reduction_percent: None,
            overall_verifying_trust_cached_speedup_avg: None,
            headline: None,
        }
    }

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
    fn freshness_flag_uses_the_24h_threshold() {
        let fresh = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert!(fresh.is_fresh());
        let stale = ResultsView::new(
            fresh.results.clone(),
            Some(Duration::from_secs(24 * 60 * 60 + 1)),
            ReportHealth::Agree,
        );
        assert!(!stale.is_fresh());
        let unknown = ResultsView::new(fresh.results.clone(), None, ReportHealth::Agree);
        assert!(!unknown.is_fresh(), "unknown age must not claim fresh");
    }

    #[test]
    fn load_of_missing_path_is_missing_state() {
        let path = std::env::temp_dir().join("climb-tui-definitely-missing.json");
        let _ = std::fs::remove_file(&path);
        let state = load(&path);
        assert!(matches!(state, LoadState::Missing { .. }));
        assert!(state.failed());
        assert!(!state.is_loaded());
    }

    #[test]
    fn load_of_garbage_file_is_invalid_and_never_panics() {
        let dir = std::env::temp_dir().join(format!("climb-tui-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("results.json");
        std::fs::write(&path, "{\"version\": not json at all").expect("write garbage");
        let state = load(&path);
        let LoadState::Invalid { error, .. } = state else {
            panic!("expected Invalid, got {state:?}");
        };
        assert!(!error.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_health_is_no_report_when_html_is_absent() {
        let dir = std::env::temp_dir().join(format!("climb-tui-health-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let results = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived())
            .results
            .clone();
        let health = report_health(&dir.join("results.json"), &results);
        assert_eq!(health, ReportHealth::NoReport);
        let _ = std::fs::remove_dir_all(&dir);
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
}
