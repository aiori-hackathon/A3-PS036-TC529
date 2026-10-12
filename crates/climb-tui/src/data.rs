//! Read-only view model over the consolidated results file (phase 19.1).
//!
//! [`load`] is the only I/O entry point and never panics: a missing or
//! unparseable file becomes a [`LoadState`] variant the UI renders instead of
//! an unrecovered crash. Every KPI string is derived from the parsed
//! [`ClimbResults`] through `climb_report::charts::fmt_*` — no result number
//! appears as a literal anywhere in this crate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use climb_report::charts::{
    amortization_data, fmt_count, fmt_time_ns, per_msg_mean, size_n, timing_bar_data, BenchRow,
};
use climb_report::{ClimbResults, DnsCaptureSource};

use crate::widgets::log_bars::{Bar, BarGroup, BarKind};

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

/// Which quantity the performance bars and tables show, toggled with `v`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueMode {
    /// Batch mean over the whole dataset (ns).
    #[default]
    Total,
    /// Batch mean divided by message count (ns/message).
    PerMessage,
}

impl ValueMode {
    /// Label used in block titles and the status bar.
    pub fn label(self) -> &'static str {
        match self {
            ValueMode::Total => "total time",
            ValueMode::PerMessage => "per message",
        }
    }

    /// The other mode.
    pub fn toggled(self) -> Self {
        match self {
            ValueMode::Total => ValueMode::PerMessage,
            ValueMode::PerMessage => ValueMode::Total,
        }
    }

    /// Map a batch mean (ns) over `messages` messages to the displayed value.
    pub fn apply(self, mean_ns: f64, messages: u64) -> f64 {
        match self {
            ValueMode::Total => mean_ns,
            ValueMode::PerMessage => per_message(mean_ns, messages),
        }
    }
}

/// Per-message cost (ns); a zero-message batch is treated as a single sample.
pub fn per_message(mean_ns: f64, messages: u64) -> f64 {
    if messages == 0 {
        mean_ns
    } else {
        mean_ns / messages as f64
    }
}

/// One measured/extrapolated value in a performance table cell.
#[derive(Debug, Clone, PartialEq)]
pub struct PerfCell {
    pub value_ns: f64,
    pub kind: BarKind,
    pub ci: Option<(f64, f64)>,
}

/// One signing-tab table row: a dataset size and its two conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct SigningRow {
    pub size: String,
    pub n: u64,
    pub with: Option<PerfCell>,
    pub without: Option<PerfCell>,
    /// Measured or extrapolated speed-up, from the same source as the
    /// Overview KPI ([`ResultsView::signing_speedup_at`]).
    pub speedup: Option<Kpi>,
}

/// One verifying-tab table row: a dataset size and its three conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyingRow {
    pub size: String,
    pub n: u64,
    pub trust: Option<PerfCell>,
    pub full: Option<PerfCell>,
    pub without: Option<PerfCell>,
}

/// One condition's detail for the selected size (Signing tab panel).
#[derive(Debug, Clone, PartialEq)]
pub struct DetailLine {
    pub condition: String,
    pub mean_ns: f64,
    pub ci: Option<(f64, f64)>,
    pub per_message_ns: f64,
    pub samples: u64,
    pub source: String,
}

/// Render-ready amortization curve (log10-log10 coordinates for the chart).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmortizationView {
    pub has_data: bool,
    pub with_points: Vec<(f64, f64)>,
    pub without_points: Vec<(f64, f64)>,
    pub fit_points: Vec<(f64, f64)>,
    pub x_bounds: [f64; 2],
    pub y_bounds: [f64; 2],
    pub x_labels: Vec<String>,
    pub y_labels: Vec<String>,
}

/// A per-size value tagged with the climb-report series it came from.
struct PerfEntry {
    series: String,
    n: u64,
    cell: PerfCell,
}

/// Series label without the "(measured)"/"(extrapolated)" suffix.
fn series_name(label: &str) -> String {
    label.split(" (").next().unwrap_or(label).to_string()
}

/// Flatten `timing_bar_data` into one entry per (series, size) point — the
/// exact values the report chart draws, so the tabs can never drift from it.
fn perf_entries<T: BenchRow>(rows: &[T], trust: bool, mode: ValueMode) -> Vec<PerfEntry> {
    let data = timing_bar_data(rows, trust, true);
    let mut out = Vec::new();
    for series in &data.series {
        let name = series_name(&series.label);
        for point in &series.points {
            let kind = if series.hollow {
                BarKind::Extrapolated
            } else {
                BarKind::Measured
            };
            out.push(PerfEntry {
                series: name.clone(),
                n: point.n,
                cell: PerfCell {
                    value_ns: mode.apply(point.value, point.n),
                    kind,
                    ci: point
                        .ci_lower
                        .zip(point.ci_upper)
                        .map(|(lo, hi)| (mode.apply(lo, point.n), mode.apply(hi, point.n))),
                },
            });
        }
    }
    out
}

/// Sorted `(n, label)` pairs for every size present as a row, label as stored.
fn size_labels<T: BenchRow>(rows: &[T]) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    for row in rows {
        if let Some(n) = size_n(row.size()) {
            if !out.iter().any(|(m, _)| *m == n) {
                out.push((n, row.size().to_string()));
            }
        }
    }
    out.sort_by_key(|(n, _)| *n);
    out
}

/// Label for a size, falling back to the formatted message count.
fn label_for(labels: &[(u64, String)], n: u64) -> String {
    labels
        .iter()
        .find(|(m, _)| *m == n)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| fmt_count(n as f64))
}

/// Distinct sizes across the entries, ascending.
fn entry_sizes(entries: &[PerfEntry]) -> Vec<u64> {
    let mut ns: Vec<u64> = entries.iter().map(|e| e.n).collect();
    ns.sort_unstable();
    ns.dedup();
    ns
}

/// Group the entries by size and map series names to display names.
fn group_bars(
    entries: &[PerfEntry],
    labels: &[(u64, String)],
    display: fn(&str) -> String,
) -> Vec<BarGroup> {
    entry_sizes(entries)
        .into_iter()
        .map(|n| BarGroup {
            label: label_for(labels, n),
            bars: entries
                .iter()
                .filter(|e| e.n == n)
                .map(|e| Bar {
                    name: display(&e.series),
                    value_ns: e.cell.value_ns,
                    kind: e.cell.kind,
                    ci: e.cell.ci,
                })
                .collect(),
        })
        .collect()
}

/// Series display names used on the Verifying tab.
fn verifying_display(series: &str) -> String {
    match series {
        "With MTL" => "Full ladder".to_string(),
        "Trust-cached ladder" => "Trust-cached".to_string(),
        other => other.to_string(),
    }
}

fn signing_entries(view: &ResultsView, mode: ValueMode) -> (Vec<PerfEntry>, Vec<(u64, String)>) {
    let rows = &view.results.benchmark.signing;
    (perf_entries(rows, false, mode), size_labels(rows))
}

fn verifying_entries(view: &ResultsView, mode: ValueMode) -> (Vec<PerfEntry>, Vec<(u64, String)>) {
    let rows = &view.results.benchmark.verifying;
    (perf_entries(rows, true, mode), size_labels(rows))
}

/// Convert per-message points to log10-log10 chart coordinates.
fn log_points(points: &[(u64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .filter(|(_, y)| y.is_finite() && *y > 0.0)
        .map(|(n, y)| ((*n as f64).log10(), y.log10()))
        .collect()
}

/// Sample the OLS fit of the with-MTL batch total as per-message time, over a
/// log-spaced sweep of `n` between the measured extremes.
fn log_fit(fit: Option<(f64, f64)>, n_min: f64, n_max: f64) -> Vec<(f64, f64)> {
    const STEPS: usize = 32;
    let Some((intercept, slope)) = fit else {
        return Vec::new();
    };
    let lo = n_min.log10();
    let hi = n_max.log10();
    if hi <= lo {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let t = i as f64 / STEPS as f64;
        let n = 10f64.powf(lo + t * (hi - lo));
        let per_msg = (intercept + slope * n) / n;
        if per_msg.is_finite() && per_msg > 0.0 {
            out.push((n.log10(), per_msg.log10()));
        }
    }
    out
}

/// Integer-decade axis bounds plus one label per decade in range.
fn decade_axis(min_log: f64, max_log: f64, fmt: fn(f64) -> String) -> ([f64; 2], Vec<String>) {
    let lo = if min_log.is_finite() {
        min_log.floor()
    } else {
        0.0
    };
    let mut hi = if max_log.is_finite() {
        max_log.ceil()
    } else {
        1.0
    };
    if hi <= lo {
        hi = lo + 1.0;
    }
    let mut labels = Vec::new();
    for k in lo as i64..=hi as i64 {
        labels.push(fmt(10f64.powi(k as i32)));
    }
    ([lo, hi], labels)
}

impl ResultsView {
    /// Dataset sizes shown on the Signing tab, ascending.
    pub fn signing_sizes(&self) -> Vec<String> {
        let (entries, labels) = signing_entries(self, ValueMode::Total);
        entry_sizes(&entries)
            .into_iter()
            .map(|n| label_for(&labels, n))
            .collect()
    }

    /// Dataset sizes shown on the Verifying tab, ascending.
    pub fn verifying_sizes(&self) -> Vec<String> {
        let (entries, labels) = verifying_entries(self, ValueMode::Total);
        entry_sizes(&entries)
            .into_iter()
            .map(|n| label_for(&labels, n))
            .collect()
    }

    /// One bar group per size: with-MTL vs without-MTL totals (or per-message
    /// costs), measured and extrapolated exactly as the report chart draws.
    pub fn signing_groups(&self, mode: ValueMode) -> Vec<BarGroup> {
        let (entries, labels) = signing_entries(self, mode);
        group_bars(&entries, &labels, |series| series.to_string())
    }

    /// One bar group per size: trust-cached / full-ladder / without-MTL.
    pub fn verifying_groups(&self, mode: ValueMode) -> Vec<BarGroup> {
        let (entries, labels) = verifying_entries(self, mode);
        group_bars(&entries, &labels, verifying_display)
    }

    /// Signing tab numbers table, one row per size, with a speed-up column.
    pub fn signing_rows(&self, mode: ValueMode) -> Vec<SigningRow> {
        let (entries, labels) = signing_entries(self, mode);
        entry_sizes(&entries)
            .into_iter()
            .map(|n| {
                let cell = |name: &str| {
                    entries
                        .iter()
                        .find(|e| e.n == n && e.series == name)
                        .map(|e| e.cell.clone())
                };
                let size = label_for(&labels, n);
                SigningRow {
                    size: size.clone(),
                    n,
                    with: cell("With MTL"),
                    without: cell("Without MTL"),
                    speedup: self.signing_speedup_at(&size),
                }
            })
            .collect()
    }

    /// Verifying tab numbers table, one row per size, three conditions.
    pub fn verifying_rows(&self, mode: ValueMode) -> Vec<VerifyingRow> {
        let (entries, labels) = verifying_entries(self, mode);
        entry_sizes(&entries)
            .into_iter()
            .map(|n| {
                let cell = |name: &str| {
                    entries
                        .iter()
                        .find(|e| e.n == n && e.series == name)
                        .map(|e| e.cell.clone())
                };
                VerifyingRow {
                    size: label_for(&labels, n),
                    n,
                    trust: cell("Trust-cached ladder"),
                    full: cell("With MTL"),
                    without: cell("Without MTL"),
                }
            })
            .collect()
    }

    /// The detail lines for one signing size: one measured or extrapolated
    /// line per condition, with the per-message math that produced it.
    pub fn signing_detail(&self, size: &str) -> Vec<DetailLine> {
        let Some(n) = size_n(size).or_else(|| {
            self.results
                .benchmark
                .signing
                .iter()
                .find(|r| r.size.eq_ignore_ascii_case(size))
                .and_then(|r| size_n(&r.size))
        }) else {
            return Vec::new();
        };
        let rows = &self.results.benchmark.signing;
        let mut out = Vec::new();
        for condition in ["With MTL", "Without MTL"] {
            let measured = rows.iter().find(|row| {
                row.size.eq_ignore_ascii_case(size)
                    && row.condition == condition
                    && row.mean_ns > 0.0
            });
            if let Some(row) = measured {
                out.push(DetailLine {
                    condition: condition.to_string(),
                    mean_ns: row.mean_ns,
                    ci: Some((row.ci_lower_ns, row.ci_upper_ns)),
                    per_message_ns: per_message(row.mean_ns, row.messages),
                    samples: row.messages,
                    source: format!("measured, {} messages", fmt_count(row.messages as f64)),
                });
            } else if let Some(per_msg) = per_msg_mean(rows, condition) {
                out.push(DetailLine {
                    condition: condition.to_string(),
                    mean_ns: per_msg * n as f64,
                    ci: None,
                    per_message_ns: per_msg,
                    samples: n,
                    source: format!(
                        "extrapolated: {} \u{00d7} measured per-message cost",
                        fmt_count(n as f64)
                    ),
                });
            }
        }
        out
    }

    /// The signing amortization curve: measured per-message points, the
    /// without-MTL baseline, and the OLS fit of the with-MTL batch total.
    pub fn amortization_view(&self) -> AmortizationView {
        let rows = &self.results.benchmark.signing;
        let data = amortization_data(rows);
        let with_points = log_points(&data.with_points);
        let without_points = log_points(&data.without_points);
        let ns: Vec<u64> = data
            .with_points
            .iter()
            .chain(data.without_points.iter())
            .map(|(n, _)| *n)
            .collect();
        if ns.is_empty() {
            return AmortizationView::default();
        }
        let n_min = ns.iter().copied().min().unwrap_or(0) as f64;
        let n_max = ns.iter().copied().max().unwrap_or(0) as f64;
        let fit_points = log_fit(data.fit, n_min, n_max);

        let mut all_log_y: Vec<f64> = with_points
            .iter()
            .chain(without_points.iter())
            .chain(fit_points.iter())
            .map(|(_, y)| *y)
            .filter(|y| y.is_finite())
            .collect();
        if all_log_y.is_empty() {
            return AmortizationView::default();
        }
        all_log_y.sort_by(f64::total_cmp);
        let (x_bounds, x_labels) = decade_axis(n_min.log10(), n_max.log10(), fmt_count);
        let (y_bounds, y_labels) =
            decade_axis(all_log_y[0], all_log_y[all_log_y.len() - 1], fmt_time_ns);

        AmortizationView {
            has_data: true,
            with_points,
            without_points,
            fit_points,
            x_bounds,
            y_bounds,
            x_labels,
            y_labels,
        }
    }
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

    fn signing_row_n(
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

    fn verifying_row(
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

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn value_mode_toggles_and_applies() {
        assert_eq!(ValueMode::default(), ValueMode::Total);
        assert_eq!(ValueMode::Total.toggled(), ValueMode::PerMessage);
        assert_eq!(ValueMode::PerMessage.toggled(), ValueMode::Total);
        assert_eq!(ValueMode::Total.label(), "total time");
        assert_eq!(ValueMode::PerMessage.label(), "per message");
        assert_eq!(ValueMode::Total.apply(1000.0, 100), 1000.0);
        assert_eq!(ValueMode::PerMessage.apply(1000.0, 100), 10.0);
        assert_eq!(per_message(50.0, 0), 50.0, "zero messages -> single sample");
    }

    #[test]
    fn signing_groups_use_measured_and_extrapolated_series() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
                signing_row_n("Medium", "With MTL", 50_000.0, 10_000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let groups = view.signing_groups(ValueMode::Total);
        let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Small", "OneK", "Medium", "1,000,000"],
            "Large is extrapolated-only, so its label falls back to the count"
        );

        let small = &groups[0];
        assert_eq!(small.bars[0].name, "With MTL");
        assert_eq!(small.bars[0].value_ns, 1000.0);
        assert_eq!(small.bars[0].kind, BarKind::Measured);
        assert_eq!(small.bars[0].ci, Some((900.0, 1100.0)));
        assert_eq!(small.bars[1].name, "Without MTL");
        assert_eq!(small.bars[1].value_ns, 100_000.0);

        let medium = &groups[2];
        assert_eq!(medium.bars[0].kind, BarKind::Measured);
        assert_eq!(medium.bars[0].value_ns, 50_000.0);
        assert_eq!(medium.bars[1].kind, BarKind::Extrapolated);
        assert_eq!(
            medium.bars[1].value_ns, 10_000_000.0,
            "1000/m x 10,000 msgs"
        );
        assert_eq!(medium.bars[1].ci, None);

        let large = &groups[3];
        assert_eq!(large.bars[0].kind, BarKind::Extrapolated);
        assert!(
            approx(large.bars[0].value_ns, (25.0 / 3.0) * 1_000_000.0),
            "per-message mean (10+10+5)/3 scaled to 1M msgs"
        );
        assert_eq!(large.bars[1].value_ns, 1_000_000_000.0);
    }

    #[test]
    fn signing_groups_in_per_message_mode_divide_by_message_count() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
                signing_row_n("Medium", "With MTL", 50_000.0, 10_000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let groups = view.signing_groups(ValueMode::PerMessage);

        let small = &groups[0];
        assert_eq!(small.bars[0].value_ns, 10.0, "1000 ns / 100 msgs");
        assert_eq!(small.bars[0].ci, Some((9.0, 11.0)));
        assert_eq!(small.bars[1].value_ns, 1000.0);

        let medium = &groups[2];
        assert_eq!(medium.bars[0].value_ns, 5.0);
        assert!(
            approx(medium.bars[1].value_ns, 1000.0),
            "extrapolated per-message cost is the mean itself"
        );

        let large = &groups[3];
        assert!(approx(large.bars[0].value_ns, 25.0 / 3.0));
    }

    #[test]
    fn signing_rows_carry_the_speedup_column() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
                signing_row_n("Medium", "With MTL", 50_000.0, 10_000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let rows = view.signing_rows(ValueMode::Total);
        let sizes: Vec<&str> = rows.iter().map(|r| r.size.as_str()).collect();
        assert_eq!(sizes, vec!["Small", "OneK", "Medium", "1,000,000"]);

        let small = &rows[0];
        assert_eq!(small.with.as_ref().unwrap().value_ns, 1000.0);
        assert_eq!(small.without.as_ref().unwrap().value_ns, 100_000.0);
        assert_eq!(
            small.speedup.as_ref().map(|k| k.value.clone()),
            view.signing_speedup_at("Small").map(|k| k.value)
        );

        let medium = &rows[2];
        assert_eq!(medium.without.as_ref().unwrap().kind, BarKind::Extrapolated);
        let kpi = medium.speedup.as_ref().expect("extrapolated speedup");
        assert_eq!(kpi.value, "≈200x");
        assert_eq!(kpi.kind, KpiKind::Extrapolated);
    }

    #[test]
    fn verifying_groups_rename_conditions_and_sizes() {
        let view = view_with(
            Vec::new(),
            vec![
                verifying_row("Small", "MTL (trust cached)", 5000.0, 100),
                verifying_row("Small", "With MTL", 100_000.0, 100),
                verifying_row("Small", "Without MTL", 90_000.0, 100),
                verifying_row("Medium", "MTL (trust cached)", 40_000.0, 10_000),
                verifying_row("Medium", "With MTL", 20_000_000.0, 10_000),
                verifying_row("Medium", "Without MTL", 18_000_000.0, 10_000),
                verifying_row("Large", "MTL (trust cached)", 800_000_000.0, 1_000_000),
            ],
            Vec::new(),
            None,
            empty_derived(),
        );
        assert_eq!(
            view.verifying_sizes(),
            vec!["Small", "1,000", "Medium", "Large"],
            "OneK has no measured row so its label falls back to the count"
        );

        let groups = view.verifying_groups(ValueMode::Total);
        let small = &groups[0];
        let names: Vec<&str> = small.bars.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Full ladder", "Without MTL", "Trust-cached"]);
        assert_eq!(small.bars[0].value_ns, 100_000.0);
        assert_eq!(small.bars[2].value_ns, 5000.0);

        let onek = &groups[1];
        let names: Vec<&str> = onek.bars.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Full ladder", "Without MTL"]);
        assert_eq!(onek.bars[0].kind, BarKind::Extrapolated);
        assert_eq!(onek.bars[0].value_ns, 1_500_000.0, "1500/m x 1000 msgs");

        let large = &groups[3];
        let names: Vec<&str> = large.bars.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Trust-cached", "Full ladder", "Without MTL"],
            "measured trust series first, extrapolated pairs after"
        );
        assert_eq!(large.bars[0].kind, BarKind::Measured);
        assert_eq!(large.bars[1].kind, BarKind::Extrapolated);
    }

    #[test]
    fn verifying_rows_mark_absent_conditions_as_none() {
        let view = view_with(
            Vec::new(),
            vec![
                verifying_row("Small", "MTL (trust cached)", 5000.0, 100),
                verifying_row("Small", "With MTL", 100_000.0, 100),
                verifying_row("Small", "Without MTL", 90_000.0, 100),
                verifying_row("Medium", "MTL (trust cached)", 40_000.0, 10_000),
                verifying_row("Medium", "With MTL", 20_000_000.0, 10_000),
                verifying_row("Medium", "Without MTL", 18_000_000.0, 10_000),
                verifying_row("Large", "MTL (trust cached)", 800_000_000.0, 1_000_000),
            ],
            Vec::new(),
            None,
            empty_derived(),
        );
        let rows = view.verifying_rows(ValueMode::Total);
        let sizes: Vec<&str> = rows.iter().map(|r| r.size.as_str()).collect();
        assert_eq!(sizes, vec!["Small", "1,000", "Medium", "Large"]);

        let small = &rows[0];
        assert_eq!(small.trust.as_ref().unwrap().value_ns, 5000.0);
        assert_eq!(small.full.as_ref().unwrap().value_ns, 100_000.0);
        assert_eq!(small.without.as_ref().unwrap().value_ns, 90_000.0);
        assert_eq!(small.trust.as_ref().unwrap().kind, BarKind::Measured);

        let onek = &rows[1];
        assert_eq!(onek.trust, None, "trust-cached is measured-only");
        assert_eq!(onek.full.as_ref().unwrap().kind, BarKind::Extrapolated);
        assert_eq!(
            onek.full.as_ref().unwrap().value_ns,
            1_500_000.0,
            "1500/m x 1000 msgs"
        );
        assert_eq!(onek.without.as_ref().unwrap().kind, BarKind::Extrapolated);
        assert_eq!(onek.without.as_ref().unwrap().value_ns, 1_350_000.0);

        let large = &rows[3];
        assert_eq!(large.trust.as_ref().unwrap().kind, BarKind::Measured);
        assert_eq!(large.full.as_ref().unwrap().value_ns, 1_500_000_000.0);
        assert_eq!(large.without.as_ref().unwrap().value_ns, 1_350_000_000.0);
    }

    #[test]
    fn signing_detail_lists_measured_and_extrapolated_conditions() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
                signing_row_n("Medium", "With MTL", 50_000.0, 10_000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let detail = view.signing_detail("Medium");
        assert_eq!(detail.len(), 2);

        let with = &detail[0];
        assert_eq!(with.condition, "With MTL");
        assert_eq!(with.mean_ns, 50_000.0);
        let (ci_lo, ci_hi) = with.ci.expect("measured CI present");
        assert!(approx(ci_lo, 45_000.0), "ci lower: {ci_lo}");
        assert!(approx(ci_hi, 55_000.0), "ci upper: {ci_hi}");
        assert_eq!(with.per_message_ns, 5.0);
        assert_eq!(with.samples, 10_000);
        assert!(with.source.contains("measured"), "{}", with.source);

        let without = &detail[1];
        assert_eq!(without.condition, "Without MTL");
        assert_eq!(without.ci, None);
        assert!(approx(without.mean_ns, 10_000_000.0));
        assert!(approx(without.per_message_ns, 1000.0));
        assert!(
            without.source.contains("extrapolated"),
            "{}",
            without.source
        );

        assert!(view.signing_detail("NotASize").is_empty());
    }

    #[test]
    fn amortization_view_builds_log10_points_and_decade_axes() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("Medium", "With MTL", 100_000.0, 10_000),
                signing_row_n("Large", "With MTL", 10_000_000.0, 1_000_000),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let amort = view.amortization_view();
        assert!(amort.has_data);
        assert_eq!(
            amort.with_points,
            vec![(2.0, 1.0), (3.0, 1.0), (4.0, 1.0), (6.0, 1.0)],
            "log10(n), log10(per-message ns); all with-MTL rows are 10 ns/msg"
        );
        assert_eq!(
            amort.without_points,
            vec![(2.0, 3.0), (3.0, 3.0)],
            "log10(1000 ns/msg) = 3"
        );
        assert_eq!(amort.fit_points.len(), 33, "32-step log-spaced sweep");
        assert!(
            amort.fit_points.iter().all(|(_, y)| approx(*y, 1.0)),
            "fit of the exact 10n line is per-message 10 ns everywhere"
        );
        assert_eq!(amort.x_bounds, [2.0, 6.0]);
        assert_eq!(
            amort.x_labels,
            vec!["100", "1,000", "10,000", "100,000", "1,000,000"]
        );
        assert_eq!(amort.y_bounds[1], 3.0, "y max decade is 1 ms");
        assert!(
            amort.y_bounds[0] == 1.0 || amort.y_bounds[0] == 0.0,
            "fp noise near the 10 ns boundary may floor to 0: {:?}",
            amort.y_bounds
        );
        assert_eq!(
            amort.y_labels.last().map(String::as_str),
            Some("1 \u{00b5}s")
        );
        assert!(amort.y_labels.contains(&"10 ns".to_string()));
        assert!(amort.y_labels.contains(&"100 ns".to_string()));
    }

    #[test]
    fn amortization_view_is_empty_without_signing_rows() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        let amort = view.amortization_view();
        assert!(!amort.has_data);
        assert!(amort.with_points.is_empty());
        assert!(amort.without_points.is_empty());
        assert!(amort.fit_points.is_empty());
    }
}
