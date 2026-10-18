//! Read-only view model over the consolidated results file (phase 19.1).
//!
//! [`load`] is the only I/O entry point and never panics: a missing or
//! unparseable file becomes a [`LoadState`] variant the UI renders instead of
//! an unrecovered crash. Every KPI string is derived from the parsed
//! [`ClimbResults`] through `climb_report::charts::fmt_*` — no result number
//! appears as a literal anywhere in this crate.

mod amortization;
mod ladder;
mod memory;
mod overview;
mod reuse;
mod signing;
mod wire;

pub use amortization::{per_message, AmortizationView, ValueMode};
pub use ladder::{
    badge as ladder_badge, load_ladder, load_ladder_auto, BadgeKind, LadderBadge, LadderLoadState,
    LadderSource,
};
pub use overview::{rung_state, Kpi, KpiKind, RUNG_TICK};
pub use reuse::ReuseRow;
pub use signing::{DetailLine, PerfCell, SigningRow, VerifyingRow};
pub use wire::{UdpFitTableData, UdpFitTableRow, UdpGaugeData};

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use climb_report::ClimbResults;

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
    /// The report is older than the results file, so its claims describe a
    /// previous run and are not compared.
    ReportStale,
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
}

/// Title-case a section's size label so the Wire/Memory tabs (which store it
/// lowercase) match the Signing/Verifying tabs' spelling.
pub(crate) fn title_size(size: &str) -> String {
    let mut chars = size.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
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
    let report = report_path(results_path);
    let results_mtime = mtime(results_path);
    let report_mtime = mtime(&report);
    let drift = std::fs::read_to_string(&report)
        .ok()
        .map(|html| climb_report::check_report(&html, results).len());
    artifact_state(results_mtime, report_mtime, drift)
}

/// Classify the results/report artifact pair.
///
/// A missing report is [`ReportHealth::NoReport`]. A report older than the
/// results file is [`ReportHealth::ReportStale`], checked before the drift
/// count because a stale report's claims describe a previous run and must not
/// be trusted even when they happen to agree. Otherwise the drift count decides
/// between [`ReportHealth::Agree`] and [`ReportHealth::Drift`].
pub fn artifact_state(
    results_mtime: Option<SystemTime>,
    report_mtime: Option<SystemTime>,
    drift_failures: Option<usize>,
) -> ReportHealth {
    let Some(report_mtime) = report_mtime else {
        return ReportHealth::NoReport;
    };
    if results_mtime.is_some_and(|results| report_mtime < results) {
        return ReportHealth::ReportStale;
    }
    match drift_failures {
        Some(0) | None => ReportHealth::Agree,
        Some(count) => ReportHealth::Drift(count),
    }
}

/// File modification time; `None` when the file is absent or carries no stamp.
fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

/// `climb-report/report.html` next to the results file.
fn report_path(results_path: &Path) -> PathBuf {
    results_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("climb-report")
        .join("report.html")
}

/// Shared test fixtures for the child modules' unit tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use climb_report::results::{
        AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, EnvironmentFingerprint,
        MemoryResult, MemoryResults, SigningBenchmarkResult, VerifyingBenchmarkResult,
        WireSizeResult,
    };
    use climb_report::{DnsCaptureSource, DnsPipelineHeadline};

    pub(crate) fn signing_row(size: &str, condition: &str, mean_ns: f64) -> SigningBenchmarkResult {
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

    pub(crate) fn environment() -> EnvironmentFingerprint {
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

    pub(crate) fn view_with(
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
                ladder: None,
                plain: Vec::new(),
                comparisons: Vec::new(),
                derived,
                dns_anatomy: None,
                warnings: Vec::new(),
            },
            Some(Duration::from_secs(60)),
            ReportHealth::Agree,
        )
    }

    pub(crate) fn dns_headline(full: u64, condensed: u64) -> DnsPipelineHeadline {
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

    pub(crate) fn wire_row(size: &str, messages: usize, reduction: f64) -> WireSizeResult {
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
            plain_total_bytes: 0,
            plain_signature_len_bytes: 0,
        }
    }

    pub(crate) fn wire_size_row(
        size: &str,
        messages: usize,
        ladder: usize,
        condensed_mean: f64,
        with_mtl: u64,
        without_mtl: u64,
        reduction: f64,
    ) -> WireSizeResult {
        WireSizeResult {
            size: size.to_string(),
            message_count: messages,
            ladder_len: ladder,
            condensed_min: condensed_mean as usize,
            condensed_mean,
            condensed_max: condensed_mean as usize,
            condensed_uniform: true,
            with_mtl_total_bytes: with_mtl,
            without_mtl_total_bytes: without_mtl,
            reduction_percent: reduction,
            plain_total_bytes: 0,
            plain_signature_len_bytes: 0,
        }
    }

    /// A modelled UDP-fit section as test data: two response profiles x
    /// three signature configurations x two dataset sizes, with a
    /// zero-tolerance cross-check that matches exactly. The numbers here are
    /// fixtures for the view-model, not model output.
    pub(crate) fn udp_fit_section(rtt_ms: Option<f64>) -> climb_report::results::UdpFitSection {
        use climb_report::results::{UdpFitRowArtifact, UdpFitSection, UdpFitValidation};
        let profiles = [
            ("positive_a", "Positive A (www)", "captured"),
            ("nxdomain_nsec3", "NXDOMAIN/NSEC3 (parameter)", "parameter"),
        ];
        let configs = [
            ("plain", "A (plain SLH-DSA)"),
            ("mtl-full", "B (MTL non-amortised)"),
            ("mtl-amortised", "C (MTL amortised)"),
        ];
        let sizes = [100u64, 1_000_000];
        let mut rows = Vec::new();
        for (profile, label, source) in profiles {
            for (col, (config, config_label)) in configs.into_iter().enumerate() {
                for (size_idx, &messages) in sizes.iter().enumerate() {
                    let fit = 100.0 - 25.0 * col as f64 - 10.0 * size_idx as f64;
                    rows.push(UdpFitRowArtifact {
                        profile: profile.to_string(),
                        profile_label: label.to_string(),
                        profile_source: source.to_string(),
                        config: config.to_string(),
                        config_label: config_label.to_string(),
                        zone_leaves: 16,
                        messages,
                        max_response_bytes: 658 + 1_000 * col as u64 + size_idx as u64,
                        fit_1232_count: messages,
                        fit_1232_percent: fit,
                        tc_band_count: 0,
                        tc_band_percent: 0.0,
                        over_4096_count: 0,
                        needs_tcp_percent: 100.0 - fit,
                        tcp_round_trips: 3,
                        tcp_query_bytes: 46,
                        tcp_payload_bytes: 700,
                        tcp_modeled_ms: rtt_ms,
                        class: climb_report::results::EvidenceClass::Analytical,
                    });
                }
            }
        }
        UdpFitSection {
            rows,
            validation: Some(UdpFitValidation {
                zone_leaves: 16,
                plain_signature_len_bytes: 7856,
                recorded_full_response_bytes: 24_438,
                modeled_full_response_bytes: 24_438,
                recorded_condensed_response_bytes: 658,
                modeled_condensed_response_bytes: 658,
                tolerance_bytes: 0,
                matches: true,
            }),
            rtt_ms,
        }
    }

    pub(crate) fn memory_result(
        size: &str,
        messages: usize,
        measured: u64,
        libmtl: usize,
        harness: u64,
    ) -> MemoryResult {
        let dataset = messages as u64 * 536;
        let keypair = 186u64;
        let analytical = libmtl as u64 + harness + dataset + keypair;
        MemoryResult {
            size: size.to_string(),
            message_count: messages,
            measured_mean_bytes: measured,
            libmtl_analytical_bytes: libmtl,
            harness_buffers_bytes: harness,
            analytical_total_bytes: analytical,
            residual_bytes: measured as i64 - analytical as i64,
            residual_percent: (measured as f64 - analytical as f64) / analytical as f64 * 100.0,
            ratio: measured as f64 / analytical as f64,
            measured_analytical_total_ratio: measured as f64 / analytical as f64,
            measured_range_min_bytes: measured / 2,
            measured_range_max_bytes: measured * 2,
            measured_stddev_bytes: measured as f64 / 8.0,
            median_bytes: measured,
            iqr_bytes: measured / 4,
            samples: 3,
            method: "isolated-process".to_string(),
            dataset_buffers_bytes: dataset,
            keypair_bytes: keypair,
            measured_peak_bytes: measured,
            stages: None,
        }
    }

    pub(crate) fn view_with_memory(measured: Vec<MemoryResult>) -> ResultsView {
        let mut view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        view.results.memory.measured = measured;
        view.results.dns_anatomy = None;
        view
    }

    pub(crate) fn empty_derived() -> DerivedSummary {
        DerivedSummary {
            overall_wire_reduction_percent: None,
            largest_wire_reduction_percent: None,
            overall_verifying_trust_cached_speedup_avg: None,
            headline: None,
            reuse: Vec::new(),
            udp_fit: None,
        }
    }

    pub(crate) fn signing_row_n(
        size: &str,
        condition: &str,
        mean_ns: f64,
        messages: u64,
    ) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages,
            mean_ns,
            ci_lower_ns: mean_ns * 0.9,
            ci_upper_ns: mean_ns * 1.1,
            speedup: None,
        }
    }

    pub(crate) fn verifying_row(
        size: &str,
        condition: &str,
        mean_ns: f64,
        messages: u64,
    ) -> VerifyingBenchmarkResult {
        VerifyingBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages,
            mean_ns,
            ci_lower_ns: mean_ns * 0.9,
            ci_upper_ns: mean_ns * 1.1,
            speedup: None,
        }
    }

    pub(crate) fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{empty_derived, view_with};
    use super::*;

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
    fn artifact_state_without_a_report_is_no_report() {
        let now = SystemTime::now();
        assert_eq!(
            artifact_state(Some(now), None, None),
            ReportHealth::NoReport
        );
    }

    #[test]
    fn artifact_state_checks_staleness_before_drift() {
        let older = SystemTime::now();
        let newer = older + Duration::from_secs(60);
        assert_eq!(
            artifact_state(Some(newer), Some(older), Some(0)),
            ReportHealth::ReportStale,
            "a report older than the results is stale even with zero drift"
        );
        assert_eq!(
            artifact_state(Some(newer), Some(older), Some(3)),
            ReportHealth::ReportStale,
            "staleness wins over a non-zero drift count"
        );
    }

    #[test]
    fn artifact_state_counts_drift_when_report_is_current() {
        let now = SystemTime::now();
        assert_eq!(
            artifact_state(Some(now), Some(now), Some(0)),
            ReportHealth::Agree
        );
        assert_eq!(
            artifact_state(Some(now), Some(now), Some(2)),
            ReportHealth::Drift(2)
        );
    }

    /// Consolidate the committed ladder fixture and read the result back
    /// through the TUI's own loader, so the consumer of the new `ladder`
    /// section is exercised end to end.
    #[test]
    fn consolidated_ladder_section_parses_through_the_tui_loader() {
        let root = std::env::temp_dir().join(format!(
            "climb-tui-ladder-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("criterion")).expect("temp dir");
        let results_path = root.join("climb-results.json");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/ladder-verification.recorded.json");

        climb_report::consolidate_results(climb_report::ConsolidateOptions {
            force: true,
            criterion_dir: root.join("criterion"),
            wire_report_path: None,
            memory_report_path: None,
            dns_capture_path: root.join("no-capture.json"),
            dns_fixture_path: root.join("no-fixture.json"),
            ladder_verification_path: root.join("no-live.json"),
            ladder_fixture_path: fixture,
            environment_path: root.join("no-env.json"),
            baseline: "new".to_string(),
            rtt_ms: None,
            output_path: results_path.clone(),
        })
        .expect("consolidate from the committed ladder fixture");

        let state = load(&results_path);
        let LoadState::Loaded(view) = state else {
            panic!("expected the TUI to load the consolidated results, got {state:?}");
        };
        let ladder = view
            .results()
            .ladder
            .as_ref()
            .expect("consolidated results carry the ladder section");
        assert_eq!(ladder.source, climb_report::LadderSource::RecordedFixture);
        assert!(ladder.all_match, "the model must reproduce every size");
        assert!(!ladder.sizes.is_empty());
        assert!(
            ladder.sizes.iter().all(|row| row.model_matches),
            "every measured size must agree with the closed-form model"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The committed sample results document must load through the TUI loader
    /// and expose a reuse row per artifact entry, cross-checked against the
    /// parsed artifact itself (§7c: the TUI is one of the artifact's consumers).
    #[test]
    fn committed_sample_results_expose_reuse_rows_to_the_tui() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/climb-results.json");
        let state = load(&path);
        let LoadState::Loaded(view) = state else {
            panic!("expected the TUI to load the committed sample artifact, got {state:?}");
        };

        let rows = view.reuse_rows();
        let expected = &view.results().derived.reuse;
        assert!(
            !expected.is_empty(),
            "the committed sample artifact must carry a reuse section"
        );
        assert_eq!(rows.len(), expected.len());
        for (row, raw) in rows.iter().zip(expected) {
            assert_eq!(row.size, title_size(&raw.size));
            assert_eq!(row.time_r, raw.breakeven_reuse_time.or(raw.r_star_time));
            assert_eq!(row.bytes_r, raw.breakeven_reuse_bytes.or(raw.r_star_bytes));
            assert_eq!(row.class, raw.class.label());
        }
    }

    /// The committed sample results document must load through the TUI loader
    /// and expose the modelled UDP-fit table — headers, row order and every
    /// cell cross-checked against the parsed artifact itself (the TUI is one
    /// of the artifact's consumers).
    #[test]
    fn committed_sample_results_expose_udp_fit_rows_to_the_tui() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/climb-results.json");
        let state = load(&path);
        let LoadState::Loaded(view) = state else {
            panic!("expected the TUI to load the committed sample artifact, got {state:?}");
        };

        let section = view
            .results()
            .derived
            .udp_fit
            .as_ref()
            .expect("the committed sample artifact must carry a UDP-fit section");
        assert!(!section.rows.is_empty());

        let table = view.udp_fit_table();
        let mut sizes: Vec<u64> = section.rows.iter().map(|row| row.messages).collect();
        sizes.sort_unstable();
        sizes.dedup();
        let expected_headers: Vec<String> = sizes
            .iter()
            .map(|&n| climb_report::charts::fmt_count(n as f64))
            .collect();
        assert_eq!(table.size_headers, expected_headers);

        let mut pairs: Vec<(String, String)> = Vec::new();
        for row in &section.rows {
            let key = (row.profile.clone(), row.config.clone());
            if !pairs.contains(&key) {
                pairs.push(key);
            }
        }
        assert_eq!(table.rows.len(), pairs.len());
        for (table_row, (profile, config)) in table.rows.iter().zip(&pairs) {
            let expected: Vec<String> = sizes
                .iter()
                .map(|&size| {
                    section
                        .rows
                        .iter()
                        .find(|r| {
                            r.profile == *profile && r.config == *config && r.messages == size
                        })
                        .map(|r| format!("{:.1}%", r.fit_1232_percent))
                        .expect("every column size has a row for this pair")
                })
                .collect();
            assert_eq!(table_row.cells, expected, "{profile} / {config}");
        }
        assert!(!table.note.is_empty(), "the note must state provenance");
    }

    /// A results document written before the ladder section existed must still
    /// parse (§7e backward compatibility).
    #[test]
    fn results_without_a_ladder_section_still_load_in_the_tui() {
        let mut results = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived())
            .results
            .clone();
        results.ladder = None;
        let json = serde_json::to_string(&results).expect("serialize");
        let parsed: ClimbResults = serde_json::from_str(&json).expect("parse without ladder");
        assert_eq!(parsed.ladder, None);
    }
}
