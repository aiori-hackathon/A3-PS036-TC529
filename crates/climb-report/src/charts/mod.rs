//! Chart data preparation for the report charts.
//!
//! Builds plotters-friendly series from [`ClimbResults`] (the single source of
//! truth of consolidated results) and renders six SVGs under
//! `target/climb-report/charts/`. Measured Criterion rows are visually
//! distinguished from values extrapolated from them (hollow bars, dashed fit),
//! and every chart carries a three-line footnote naming its source file.
//!
//! All extrapolations here are descriptive summaries of the benchmarked runs,
//! not guarantees: the without-MTL totals are `N x` the mean measured
//! per-message cost of the same condition.

#[cfg(feature = "native")]
pub mod draw;

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native")]
use crate::error::ReportError;
#[cfg(feature = "native")]
use crate::results::ClimbResults;
use crate::results::{
    size_display, MemoryResult, PlainTimingResult, ReuseResult, SigningBenchmarkResult,
    VerifyingBenchmarkResult, WireSizeResult,
};

/// An RGB series colour, independent of any rendering library so the data
/// preparation used by read-only consumers does not pull in a chart backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeriesColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl SeriesColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to the SVG renderer's colour type.
    #[cfg(feature = "native")]
    pub(crate) fn to_plotters(self) -> plotters::style::RGBColor {
        plotters::style::RGBColor(self.r, self.g, self.b)
    }
}

/// Filenames of the seven charts, in generation order.
pub const CHART_FILES: [&str; 7] = [
    "signing_time.svg",
    "verifying_time.svg",
    "transmitted_bytes.svg",
    "amortization_curve.svg",
    "memory_measured_vs_analytical.svg",
    "reuse_curve.svg",
    "udp_fit.svg",
];

/// Series colour: with MTL.
pub const COLOR_WITH_MTL: SeriesColor = SeriesColor::new(31, 119, 180);
/// Series colour: without MTL.
pub const COLOR_WITHOUT_MTL: SeriesColor = SeriesColor::new(214, 94, 0);
/// Series colour: trust-cached ladder verification.
pub const COLOR_TRUST_CACHED: SeriesColor = SeriesColor::new(44, 160, 44);
/// Series colour: plain SLH-DSA (configuration A, no ladder).
pub const COLOR_PLAIN: SeriesColor = SeriesColor::new(148, 103, 189);
/// Series colour: analytical / fitted (non-measured) values.
pub const COLOR_MODEL: SeriesColor = SeriesColor::new(90, 90, 95);
/// Series colour: Ed25519 classical context baseline.
pub const COLOR_CLASSICAL_ED25519: SeriesColor = SeriesColor::new(140, 86, 75);
/// Series colour: ECDSA P-256 classical context baseline.
pub const COLOR_CLASSICAL_ECDSA_P256: SeriesColor = SeriesColor::new(227, 119, 194);
/// Series colour: RSA-2048 classical context baseline.
pub const COLOR_CLASSICAL_RSA2048: SeriesColor = SeriesColor::new(23, 190, 207);

/// Generate all six SVG charts into `out_dir` from `results`.
///
/// Returns the paths written, in [`CHART_FILES`] order.
#[cfg(feature = "native")]
pub fn generate_charts(
    results: &ClimbResults,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, ReportError> {
    std::fs::create_dir_all(out_dir).map_err(|source| ReportError::Io {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let mut signing = timing_bar_data(&results.benchmark.signing, false, true);
    add_plain_timing_series(&mut signing, &results.plain, false, true);
    let mut verifying = timing_bar_data(&results.benchmark.verifying, true, false);
    add_plain_timing_series(&mut verifying, &results.plain, true, false);
    let wire = wire_bar_data(&results.wire_size, &results.plain);
    let amortization = amortization_data(&results.benchmark.signing);
    let memory = memory_data(&results.memory.measured);
    let reuse = reuse_data(&results.derived.reuse);

    let mut written = Vec::with_capacity(CHART_FILES.len());

    let mut data = signing;
    data.title = "Batch signing time vs dataset size".to_string();
    data.x_desc = "dataset size (messages)".to_string();
    data.y_desc = "total batch time".to_string();
    let path = out_dir.join(CHART_FILES[0]);
    draw::draw_bar_chart(&path, &data, &draw::SIGNING_FOOTNOTES)?;
    written.push(path);

    let mut data = verifying;
    data.title = "Batch verification time vs dataset size".to_string();
    data.x_desc = "dataset size (messages)".to_string();
    data.y_desc = "total batch time".to_string();
    let path = out_dir.join(CHART_FILES[1]);
    draw::draw_bar_chart(&path, &data, &draw::VERIFYING_FOOTNOTES)?;
    written.push(path);

    let mut data = wire;
    data.title = "Transmitted bytes vs dataset size".to_string();
    data.x_desc = "dataset size (messages)".to_string();
    data.y_desc = "total transmitted bytes".to_string();
    let path = out_dir.join(CHART_FILES[2]);
    draw::draw_bar_chart(&path, &data, &draw::WIRE_FOOTNOTES)?;
    written.push(path);

    let mut data = amortization;
    data.title = "MTL amortised signing: amortization curve".to_string();
    data.x_desc = "dataset size (messages)".to_string();
    data.y_desc = "per-message signing time".to_string();
    let path = out_dir.join(CHART_FILES[3]);
    draw::draw_amortization(&path, &data, &draw::AMORTIZATION_FOOTNOTES)?;
    written.push(path);

    let mut data = memory;
    data.title = "Peak RSS: measured vs analytical model".to_string();
    data.x_desc = "dataset size (messages)".to_string();
    data.y_desc = "peak RSS".to_string();
    let path = out_dir.join(CHART_FILES[4]);
    draw::draw_memory(&path, &data, &draw::MEMORY_FOOTNOTES)?;
    written.push(path);

    let mut data = reuse;
    data.title = "Ladder reuse vs reuse count (time & bytes, normalized)".to_string();
    data.x_desc = "reuse count r".to_string();
    data.y_desc = "ratio vs baseline (per-message)".to_string();
    let path = out_dir.join(CHART_FILES[5]);
    draw::draw_reuse(&path, &data, &draw::REUSE_FOOTNOTES)?;
    written.push(path);

    let mut data = udp_fit(results.derived.udp_fit.as_ref());
    data.title = "Modelled DNS response size over UDP vs dataset size".to_string();
    data.x_desc = "dataset size (messages, one ladder per batch)".to_string();
    data.y_desc = "modelled max response bytes".to_string();
    let path = out_dir.join(CHART_FILES[6]);
    draw::draw_udp_fit(&path, &data, &draw::UDP_FIT_FOOTNOTES)?;
    written.push(path);

    Ok(written)
}

/// Build reuse curve data: one line per metric (time ratio, bytes ratio) for
/// each size present.
#[derive(Debug, Clone)]
pub struct ReuseMetric {
    pub label: String,
    pub points: Vec<(f64, f64)>, // (r, ratio)
    pub extrapolated: bool,
}

impl ReuseMetric {
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Debug, Clone)]
pub struct ReuseSeries {
    pub size_label: String,
    pub metrics: Vec<ReuseMetric>,
}

#[derive(Debug, Clone)]
pub struct ReuseData {
    pub series: Vec<ReuseSeries>,
    pub title: String,
    pub x_desc: String,
    pub y_desc: String,
    pub extrapolated_any: bool,
}

/// Break-even reuse count as a chart-legend token: `1`, `1000`, or an em dash
/// when no candidate reached parity.
fn r_star_label(r: Option<u64>) -> String {
    match r {
        Some(r) => r.to_string(),
        None => "\u{2014}".to_string(),
    }
}

pub fn reuse_data(results: &[ReuseResult]) -> ReuseData {
    let mut series = Vec::new();
    let mut extrapolated_any = false;
    for res in results {
        if res.curve.is_empty() {
            continue;
        }
        extrapolated_any |= res.class == crate::results::EvidenceClass::Extrapolated;
        let mut metrics = Vec::with_capacity(2);
        let bt = res.baseline_time_ns;
        if bt > 0.0 {
            let mut t_pts = Vec::with_capacity(res.curve.len());
            for p in &res.curve {
                t_pts.push((p.reuse as f64, p.effective_time_ns / bt));
            }
            metrics.push(ReuseMetric {
                label: format!(
                    "{} · time (r*={})",
                    size_display(&res.size),
                    r_star_label(res.r_star_time)
                ),
                points: t_pts,
                extrapolated: res.class == crate::results::EvidenceClass::Extrapolated,
            });
        }
        let bb = res.baseline_bytes;
        if bb > 0.0 {
            let mut b_pts = Vec::with_capacity(res.curve.len());
            for p in &res.curve {
                b_pts.push((p.reuse as f64, p.effective_bytes / bb));
            }
            metrics.push(ReuseMetric {
                label: format!(
                    "{} · bytes (r*={})",
                    size_display(&res.size),
                    r_star_label(res.r_star_bytes)
                ),
                points: b_pts,
                extrapolated: res.class == crate::results::EvidenceClass::Extrapolated,
            });
        }
        if metrics.is_empty() {
            continue;
        }
        series.push(ReuseSeries {
            size_label: size_display(&res.size),
            metrics,
        });
    }
    ReuseData {
        series,
        title: String::new(),
        x_desc: "reuse count r".to_string(),
        y_desc: "ratio vs baseline (per-message)".to_string(),
        extrapolated_any,
    }
}

pub trait BenchRow {
    /// Dataset size label as stored in the results JSON (e.g. `"Small"`).
    fn size(&self) -> &str;
    /// Condition label (e.g. `"With MTL"`).
    fn condition(&self) -> &str;
    /// Number of messages in the benchmarked batch.
    fn messages(&self) -> u64;
    /// Mean batch time in nanoseconds.
    fn mean_ns(&self) -> f64;
    /// 95% CI lower bound in nanoseconds.
    fn ci_lower_ns(&self) -> f64;
    /// 95% CI upper bound in nanoseconds.
    fn ci_upper_ns(&self) -> f64;
}

impl BenchRow for SigningBenchmarkResult {
    fn size(&self) -> &str {
        &self.size
    }
    fn condition(&self) -> &str {
        &self.condition
    }
    fn messages(&self) -> u64 {
        self.messages
    }
    fn mean_ns(&self) -> f64 {
        self.mean_ns
    }
    fn ci_lower_ns(&self) -> f64 {
        self.ci_lower_ns
    }
    fn ci_upper_ns(&self) -> f64 {
        self.ci_upper_ns
    }
}

impl BenchRow for VerifyingBenchmarkResult {
    fn size(&self) -> &str {
        &self.size
    }
    fn condition(&self) -> &str {
        &self.condition
    }
    fn messages(&self) -> u64 {
        self.messages
    }
    fn mean_ns(&self) -> f64 {
        self.mean_ns
    }
    fn ci_lower_ns(&self) -> f64 {
        self.ci_lower_ns
    }
    fn ci_upper_ns(&self) -> f64 {
        self.ci_upper_ns
    }
}

/// Map a results size label to its message count, case-insensitively.
///
/// Returns `None` for labels outside the four benchmarked dataset sizes.
pub fn size_n(size: &str) -> Option<u64> {
    match size.to_lowercase().as_str() {
        "small" => Some(100),
        "onek" => Some(1_000),
        "medium" => Some(10_000),
        "large" => Some(1_000_000),
        _ => None,
    }
}

/// Least-squares fit of `total = intercept + slope * n` over `(n, total)` points.
///
/// Returns `(intercept, slope)`, or `None` when fewer than two points or the
/// `n` values do not vary.
pub fn fit_linear_total(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x: f64 = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y: f64 = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    Some((intercept, slope))
}

/// Decimal places chosen by magnitude, so axis and annotation labels stay short.
pub fn sig_decimals(v: f64) -> usize {
    let a = v.abs();
    if a >= 100.0 {
        0
    } else if a >= 10.0 {
        1
    } else if a >= 1.0 {
        2
    } else {
        3
    }
}

/// Format `v` with [`sig_decimals`] precision, dropping trailing fractional
/// zeros and a dangling decimal point (`2.00` → `2`, `2.50` → `2.5`).
fn fmt_trimmed(v: f64) -> String {
    let d = sig_decimals(v);
    let s = format!("{v:.d$}");
    if !s.contains('.') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Format a nanosecond duration with a unit scaled to its magnitude.
pub fn fmt_time_ns(v: f64) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    let a = v.abs();
    let (scale, unit) = if a < 1e3 {
        (1.0, "ns")
    } else if a < 1e6 {
        (1e3, "µs")
    } else if a < 1e9 {
        (1e6, "ms")
    } else if a < 60e9 {
        (1e9, "s")
    } else if a < 3_600e9 {
        (60e9, "min")
    } else if a < 86_400e9 {
        (3_600e9, "h")
    } else {
        (86_400e9, "d")
    };
    let scaled = v / scale;
    format!("{} {}", fmt_trimmed(scaled), unit)
}

/// Format a byte count with decimal SI units (kB, MB, GB, ...).
pub fn fmt_bytes(v: f64) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    let a = v.abs();
    let (scale, unit) = if a < 1e3 {
        (1.0, "B")
    } else if a < 1e6 {
        (1e3, "kB")
    } else if a < 1e9 {
        (1e6, "MB")
    } else if a < 1e12 {
        (1e9, "GB")
    } else {
        (1e12, "TB")
    };
    let scaled = v / scale;
    format!("{} {}", fmt_trimmed(scaled), unit)
}

/// Format a byte count with binary IEC units (KiB, MiB, GiB, ...).
pub fn fmt_mib(v: f64) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    let a = v.abs();
    let (scale, unit) = if a < 1024.0 {
        (1.0, "B")
    } else if a < 1024f64.powi(2) {
        (1024.0, "KiB")
    } else if a < 1024f64.powi(3) {
        (1024f64.powi(2), "MiB")
    } else {
        (1024f64.powi(3), "GiB")
    };
    let scaled = v / scale;
    format!("{} {}", fmt_trimmed(scaled), unit)
}

/// Format a count with thousands separators (e.g. `1,000,000`).
pub fn fmt_count(v: f64) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    let n = v.round() as i64;
    let negative = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// One bar in a grouped log-scale bar chart.
#[derive(Debug, Clone)]
pub struct BarPoint {
    /// Dataset size (message count) — the x position.
    pub n: u64,
    /// Bar height (y value).
    pub value: f64,
    /// 95% CI lower bound, when this point is a measured row.
    pub ci_lower: Option<f64>,
    /// 95% CI upper bound, when this point is a measured row.
    pub ci_upper: Option<f64>,
    /// Optional label drawn above the bar (e.g. an extrapolation annotation).
    pub annotation: Option<String>,
}

/// One series of bars: a labelled, coloured group sharing measured/extrapolated styling.
#[derive(Debug, Clone)]
pub struct BarSeriesData {
    /// Legend label, already carrying a "(measured)"/"(extrapolated)" suffix.
    pub label: String,
    /// Series colour.
    pub color: SeriesColor,
    /// `true` for extrapolated/modelled series (hollow bars, dashed edges).
    pub hollow: bool,
    /// Points in this series; empty series are dropped before drawing.
    pub points: Vec<BarPoint>,
}

/// Fully prepared data for one grouped bar chart.
#[derive(Debug, Clone)]
pub struct BarChartData {
    /// Chart title (set by [`generate_charts`]).
    pub title: String,
    /// x-axis description.
    pub x_desc: String,
    /// y-axis description.
    pub y_desc: String,
    /// Series in legend order; empty ones are dropped by the builders.
    pub series: Vec<BarSeriesData>,
    /// Group-level annotations: `(n, y, text)`, drawn once per dataset size.
    pub group_annotations: Vec<(u64, f64, String)>,
    /// Formatter for x-axis tick labels (dataset sizes).
    pub x_fmt: fn(f64) -> String,
    /// Formatter for y-axis tick labels.
    pub y_fmt: fn(f64) -> String,
}

/// Data for the amortization curve chart (per-message time vs dataset size).
#[derive(Debug, Clone)]
pub struct AmortizationData {
    /// Chart title (set by [`generate_charts`]).
    pub title: String,
    /// x-axis description.
    pub x_desc: String,
    /// y-axis description.
    pub y_desc: String,
    /// Measured with-MTL points: `(n, per-message ns)`.
    pub with_points: Vec<(u64, f64)>,
    /// Measured without-MTL points: `(n, per-message ns)`.
    pub without_points: Vec<(u64, f64)>,
    /// OLS fit of the with-MTL batch total: `(intercept, slope)` in ns.
    pub fit: Option<(f64, f64)>,
}

/// One point of the measured-vs-analytical memory chart.
#[derive(Debug, Clone)]
pub struct MemoryPoint {
    /// Dataset size (message count).
    pub n: u64,
    /// Measured peak RSS delta (median) in bytes: the headline footprint metric.
    pub measured_peak: f64,
    /// Measured resident RSS delta (median) in bytes, compared against the model.
    pub measured: f64,
    /// Measured minimum in bytes (whisker low).
    pub measured_min: f64,
    /// Measured maximum in bytes (whisker high).
    pub measured_max: f64,
    /// Analytical model estimate in bytes (libMTL pages).
    pub libmtl_pages: f64,
    /// Harness buffers estimate in bytes.
    pub harness_buffers: f64,
    /// Dataset message buffers estimate in bytes.
    pub dataset_buffers: f64,
    /// Keypair (key image) estimate in bytes.
    pub keypair: f64,
    /// Analytical total estimate in bytes (libMTL + harness + dataset + keypair).
    pub analytical_total: f64,
    /// Residual (measured - analytical_total).
    pub residual: f64,
    /// Residual percent.
    pub residual_percent: f64,
    /// Ratio label, e.g. `"0.41x"`.
    pub ratio_label: String,
    /// Measured / analytical_total ratio label.
    pub measured_analytical_total_ratio_label: String,
}

/// Fully prepared data for the memory chart.
#[derive(Debug, Clone)]
pub struct MemoryData {
    /// Chart title (set by [`generate_charts`]).
    pub title: String,
    /// x-axis description.
    pub x_desc: String,
    /// y-axis description.
    pub y_desc: String,
    /// Points in ascending dataset-size order.
    pub points: Vec<MemoryPoint>,
}

/// Fixed dataset sizes spanned by the timing charts (unioned with row sizes).
const UNIVERSE: [u64; 4] = [100, 1_000, 10_000, 1_000_000];

/// Mean per-message cost (ns) across measured rows of one condition.
///
/// Public because the TUI Overview reuses the exact same extrapolation method
/// as the charts; keeping the shared implementation here guarantees the two
/// never drift apart.
pub fn per_msg_mean(rows: &[impl BenchRow], condition: &str) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0.0;
    for row in rows {
        if row.condition() != condition || row.mean_ns() <= 0.0 || row.messages() == 0 {
            continue;
        }
        sum += row.mean_ns() / row.messages() as f64;
        count += 1.0;
    }
    if count > 0.0 {
        Some(sum / count)
    } else {
        None
    }
}

/// Append the configuration-A (plain SLH-DSA) series to a batch-total timing
/// chart.
///
/// Every plain bar is the `N × per-message` identity from a single-message
/// measurement (no full-batch plain run exists), so it is always hollow.
#[cfg(feature = "native")]
fn add_plain_timing_series(
    data: &mut BarChartData,
    plain: &[PlainTimingResult],
    verifying: bool,
    annotate: bool,
) {
    let mut points = Vec::new();
    for row in plain {
        let Some(n) = size_n(&row.size) else {
            continue;
        };
        let mean = if verifying {
            row.verifying_mean_ns
        } else {
            row.signing_mean_ns
        };
        let Some(mean) = mean else {
            continue;
        };
        if mean <= 0.0 {
            continue;
        }
        let value = n as f64 * mean;
        let annotation = if annotate {
            Some(format!("≈{}", fmt_time_ns(value)))
        } else {
            None
        };
        points.push(BarPoint {
            n,
            value,
            ci_lower: None,
            ci_upper: None,
            annotation,
        });
    }
    if points.is_empty() {
        return;
    }
    data.series.push(BarSeriesData {
        label: "Plain SLH-DSA (extrapolated)".to_string(),
        color: COLOR_PLAIN,
        hollow: true,
        points,
    });
}

/// Build the signing/verifying timing bar chart data.
///
/// Measured rows become solid bars with CI whiskers; dataset sizes without a
/// row for a condition become hollow extrapolated bars (`n x` the mean
/// per-message cost of that condition's measured rows). Trust-cached series
/// are measured-only and never extrapolated. When `annotate_extrapolated` is
/// set, extrapolated bars are labelled with their formatted value.
///
/// Every bar is the cost of the **whole batch** at that size: a measured
/// row's mean is scaled by `n / messages` (see the scaling rule in the
/// measured-series lookup), which leaves a whole-batch row untouched and
/// turns a per-message row into its batch total.
pub fn timing_bar_data<T: BenchRow>(
    rows: &[T],
    trust: bool,
    annotate_extrapolated: bool,
) -> BarChartData {
    // Universe: the four benchmarked sizes plus any size present in the rows.
    let mut universe: Vec<u64> = UNIVERSE.to_vec();
    for row in rows {
        if let Some(n) = size_n(row.size()) {
            if !universe.contains(&n) {
                universe.push(n);
            }
        }
    }
    universe.sort_unstable();
    universe.dedup();

    // Measured lookups per condition: n -> (mean, ci_lower, ci_upper),
    // each already scaled to the cost of the whole batch of `n` messages.
    //
    // A row's mean is the total of one measured iteration, and that
    // iteration covered `messages` messages, so the batch total is
    // `mean x n / messages`. The rule is uniform: for a row that already
    // measured the whole batch (`messages == n`) it is a no-op, and for the
    // per-message baseline (`messages == 1`) it turns one signature into
    // the cost of signing all `n` messages.
    let measured = |condition: &str| -> Vec<(u64, f64, f64, f64)> {
        let mut out = Vec::new();
        for row in rows {
            if row.condition() != condition || row.mean_ns() <= 0.0 || row.messages() == 0 {
                continue;
            }
            let Some(n) = size_n(row.size()) else {
                continue;
            };
            let scale = n as f64 / row.messages() as f64;
            out.push((
                n,
                row.mean_ns() * scale,
                row.ci_lower_ns() * scale,
                row.ci_upper_ns() * scale,
            ));
        }
        out.sort_by_key(|p| p.0);
        out
    };

    let mut series: Vec<BarSeriesData> = Vec::new();

    // Measured series, in fixed legend order.
    let measured_specs: [(&str, &str, SeriesColor); 3] = [
        ("With MTL (measured)", "With MTL", COLOR_WITH_MTL),
        ("Without MTL (measured)", "Without MTL", COLOR_WITHOUT_MTL),
        (
            "Trust-cached ladder (measured)",
            "MTL (trust cached)",
            COLOR_TRUST_CACHED,
        ),
    ];
    let want_trust = trust;
    for (label, condition, color) in measured_specs {
        if condition == "MTL (trust cached)" && !want_trust {
            continue;
        }
        let points: Vec<BarPoint> = measured(condition)
            .into_iter()
            .map(|(n, value, ci_lower, ci_upper)| BarPoint {
                n,
                value,
                ci_lower: Some(ci_lower),
                ci_upper: Some(ci_upper),
                annotation: None,
            })
            .collect();
        series.push(BarSeriesData {
            label: label.to_string(),
            color,
            hollow: false,
            points,
        });
    }

    // Extrapolated series (never for trust-cached).
    let extrap_specs: [(&str, &str, SeriesColor); 2] = [
        ("With MTL (extrapolated)", "With MTL", COLOR_WITH_MTL),
        (
            "Without MTL (extrapolated)",
            "Without MTL",
            COLOR_WITHOUT_MTL,
        ),
    ];
    for (label, condition, color) in extrap_specs {
        let Some(per_msg) = per_msg_mean(rows, condition) else {
            continue;
        };
        let measured_ns: Vec<u64> = measured(condition).into_iter().map(|p| p.0).collect();
        let mut points = Vec::new();
        for &n in &universe {
            if measured_ns.contains(&n) {
                continue;
            }
            let value = n as f64 * per_msg;
            let annotation = if annotate_extrapolated {
                Some(format!("≈{}", fmt_time_ns(value)))
            } else {
                None
            };
            points.push(BarPoint {
                n,
                value,
                ci_lower: None,
                ci_upper: None,
                annotation,
            });
        }
        series.push(BarSeriesData {
            label: label.to_string(),
            color,
            hollow: true,
            points,
        });
    }

    series.retain(|s| !s.points.is_empty());

    BarChartData {
        title: String::new(),
        x_desc: String::new(),
        y_desc: String::new(),
        x_fmt: fmt_count,
        y_fmt: fmt_time_ns,
        series,
        group_annotations: Vec::new(),
    }
}

/// Build the transmitted-bytes bar chart data (both series measured).
///
/// Adds one group annotation per dataset size carrying the reduction label
/// (`−97.0%` style), positioned above the taller bar of the group.
pub fn wire_bar_data(rows: &[WireSizeResult], _plain: &[PlainTimingResult]) -> BarChartData {
    let mut sorted: Vec<&WireSizeResult> = rows.iter().collect();
    sorted.sort_by_key(|r| r.message_count);
    sorted.dedup_by_key(|r| r.message_count);

    let with_points: Vec<BarPoint> = sorted
        .iter()
        .map(|r| BarPoint {
            n: r.message_count as u64,
            value: r.with_mtl_total_bytes as f64,
            ci_lower: None,
            ci_upper: None,
            annotation: None,
        })
        .collect();
    let without_points: Vec<BarPoint> = sorted
        .iter()
        .map(|r| BarPoint {
            n: r.message_count as u64,
            value: r.without_mtl_total_bytes as f64,
            ci_lower: None,
            ci_upper: None,
            annotation: None,
        })
        .collect();

    let plain_points: Vec<BarPoint> = sorted
        .iter()
        .filter(|r| r.plain_total_bytes > 0)
        .map(|r| BarPoint {
            n: r.message_count as u64,
            value: r.plain_total_bytes as f64,
            ci_lower: None,
            ci_upper: None,
            annotation: None,
        })
        .collect();

    let group_annotations = sorted
        .iter()
        .map(|r| {
            let group_max = (r.with_mtl_total_bytes as f64)
                .max(r.without_mtl_total_bytes as f64)
                .max(r.plain_total_bytes as f64);
            let text = format!("−{:.1}% (C vs B)", r.reduction_percent);
            (r.message_count as u64, group_max * 3.0, text)
        })
        .collect();

    let mut series = vec![
        BarSeriesData {
            label: "MTL amortised (measured)".to_string(),
            color: COLOR_WITH_MTL,
            hollow: false,
            points: with_points,
        },
        BarSeriesData {
            label: "MTL non-amortised (measured)".to_string(),
            color: COLOR_WITHOUT_MTL,
            hollow: false,
            points: without_points,
        },
    ];
    if !plain_points.is_empty() {
        series.push(BarSeriesData {
            label: "Plain SLH-DSA (extrapolated)".to_string(),
            color: COLOR_PLAIN,
            hollow: true,
            points: plain_points,
        });
    }

    BarChartData {
        title: String::new(),
        x_desc: String::new(),
        y_desc: String::new(),
        series,
        x_fmt: fmt_count,
        y_fmt: fmt_bytes,
        group_annotations,
    }
}

/// Build the amortization curve data: measured per-message points plus an OLS
/// fit of the with-MTL batch total (`intercept + slope * n`, in ns).
pub fn amortization_data(rows: &[SigningBenchmarkResult]) -> AmortizationData {
    let mut with_points: Vec<(u64, f64)> = Vec::new();
    let mut without_points: Vec<(u64, f64)> = Vec::new();
    let mut fit_pts: Vec<(f64, f64)> = Vec::new();

    for row in rows {
        let Some(n) = size_n(row.size()) else {
            continue;
        };
        if row.mean_ns <= 0.0 || row.messages == 0 {
            continue;
        }
        let per_msg = row.mean_ns / row.messages as f64;
        match row.condition.as_str() {
            "With MTL" => {
                with_points.push((n, per_msg));
                // The fit line is over the batch total: the same `n /
                // messages` scaling the bar chart uses.
                fit_pts.push((n as f64, row.mean_ns * n as f64 / row.messages as f64));
            }
            "Without MTL" => without_points.push((n, per_msg)),
            _ => {}
        }
    }
    with_points.sort_by_key(|p| p.0);
    without_points.sort_by_key(|p| p.0);
    fit_pts.sort_by(|a, b| a.0.total_cmp(&b.0));

    let fit = fit_linear_total(&fit_pts);

    AmortizationData {
        title: String::new(),
        x_desc: String::new(),
        y_desc: String::new(),
        with_points,
        without_points,
        fit,
    }
}

/// Build the measured-vs-analytical memory chart data.
///
/// Ratios are recomputed from the stored bytes (never trusting the stored
/// ratio field) and formatted as `0.41x`-style labels.
pub fn memory_data(rows: &[MemoryResult]) -> MemoryData {
    let mut sorted: Vec<&MemoryResult> = rows.iter().collect();
    sorted.sort_by_key(|r| r.message_count);
    sorted.dedup_by_key(|r| r.message_count);

    let points = sorted
        .iter()
        .map(|r| {
            let analytical = r.analytical_total_bytes as f64;
            let measured = r.measured_mean_bytes as f64;
            let ratio = if analytical > 0.0 {
                measured / analytical
            } else {
                0.0
            };
            MemoryPoint {
                n: r.message_count as u64,
                measured_peak: r.measured_peak_bytes as f64,
                measured,
                measured_min: r.measured_range_min_bytes as f64,
                measured_max: r.measured_range_max_bytes as f64,
                libmtl_pages: r.libmtl_analytical_bytes as f64,
                harness_buffers: r.harness_buffers_bytes as f64,
                dataset_buffers: r.dataset_buffers_bytes as f64,
                keypair: r.keypair_bytes as f64,
                analytical_total: r.analytical_total_bytes as f64,
                residual: r.residual_bytes as f64,
                residual_percent: r.residual_percent,
                ratio_label: format!("{ratio:.2}x"),
                measured_analytical_total_ratio_label: format!("{ratio:.2}x"),
            }
        })
        .collect();

    MemoryData {
        title: String::new(),
        x_desc: String::new(),
        y_desc: String::new(),
        points,
    }
}

/// One panel of the UDP-fit chart: a single response profile's modelled
/// response sizes, one series per signature configuration.
#[derive(Debug, Clone)]
pub struct UdpFitPanel {
    /// Human profile label (e.g. "Positive A with NS").
    pub profile_label: String,
    /// Provenance qualifier appended to the label: "captured" or "parameter".
    pub profile_source: String,
    /// One hollow series per configuration (A, B, C), in legend order.
    pub series: Vec<BarSeriesData>,
}

/// Fully prepared data for the UDP-fit chart (three stacked profile panels,
/// each with threshold lines drawn at the EDNS and TCP-fallback sizes).
#[derive(Debug, Clone)]
pub struct UdpFitData {
    /// Chart title (set by [`generate_charts`]).
    pub title: String,
    /// x-axis description.
    pub x_desc: String,
    /// y-axis description.
    pub y_desc: String,
    /// One panel per response profile, in profile-data order.
    pub panels: Vec<UdpFitPanel>,
}

/// Build the UDP-fit chart data from the analytical section.
///
/// Every bar is modelled, so every series is hollow; a missing section yields
/// empty panels rather than an error (the chart renders its own placeholder).
pub fn udp_fit(section: Option<&crate::results::UdpFitSection>) -> UdpFitData {
    // The three classical context series appear only when their rows were
    // consolidated (a per-config filter skips a series with no points).
    let configs: [(&str, &str, SeriesColor); 6] = [
        ("plain", "A (plain SLH-DSA, modelled)", COLOR_PLAIN),
        (
            "mtl-full",
            "B (MTL non-amortised, modelled)",
            COLOR_WITHOUT_MTL,
        ),
        (
            "mtl-amortised",
            "C (MTL amortised, modelled)",
            COLOR_WITH_MTL,
        ),
        (
            "classical-ed25519",
            "E (Ed25519, context, modelled)",
            COLOR_CLASSICAL_ED25519,
        ),
        (
            "classical-ecdsa-p256",
            "P (ECDSA P-256, context, modelled)",
            COLOR_CLASSICAL_ECDSA_P256,
        ),
        (
            "classical-rsa2048",
            "R (RSA-2048, context, modelled)",
            COLOR_CLASSICAL_RSA2048,
        ),
    ];
    let Some(section) = section else {
        return UdpFitData {
            title: String::new(),
            x_desc: String::new(),
            y_desc: String::new(),
            panels: Vec::new(),
        };
    };
    let mut panels: Vec<UdpFitPanel> = Vec::new();
    for row in &section.rows {
        if panels.iter().any(|p| p.profile_label == row.profile_label) {
            continue;
        }
        let mut series = Vec::new();
        for (config_id, label, color) in configs {
            let mut points: Vec<BarPoint> = section
                .rows
                .iter()
                .filter(|r| r.profile == row.profile && r.config == config_id)
                .map(|r| BarPoint {
                    n: r.messages,
                    value: r.max_response_bytes as f64,
                    ci_lower: None,
                    ci_upper: None,
                    annotation: None,
                })
                .collect();
            points.sort_by_key(|p| p.n);
            if points.is_empty() {
                continue;
            }
            series.push(BarSeriesData {
                label: label.to_string(),
                color,
                hollow: true,
                points,
            });
        }
        panels.push(UdpFitPanel {
            profile_label: row.profile_label.clone(),
            profile_source: row.profile_source.clone(),
            series,
        });
    }
    UdpFitData {
        title: String::new(),
        x_desc: String::new(),
        y_desc: String::new(),
        panels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signing_rows() -> Vec<SigningBenchmarkResult> {
        vec![
            SigningBenchmarkResult {
                size: "Small".to_string(),
                condition: "With MTL".to_string(),
                messages: 100,
                mean_ns: 595_778_430.5,
                ci_lower_ns: 587_330_435.756_5,
                ci_upper_ns: 605_604_599.07,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "Small".to_string(),
                condition: "Without MTL".to_string(),
                messages: 100,
                mean_ns: 72_598_508_466.3,
                ci_lower_ns: 70_251_843_959.242_51,
                ci_upper_ns: 75_037_881_402.4,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "OneK".to_string(),
                condition: "With MTL".to_string(),
                messages: 1_000,
                mean_ns: 716_176_020.81,
                ci_lower_ns: 695_266_629.962,
                ci_upper_ns: 741_940_671.137_25,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "OneK".to_string(),
                condition: "Without MTL".to_string(),
                messages: 1_000,
                mean_ns: 688_242_643_237.8,
                ci_lower_ns: 655_274_818_387.164_9,
                ci_upper_ns: 717_043_245_383.5,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "Medium".to_string(),
                condition: "With MTL".to_string(),
                messages: 10_000,
                mean_ns: 716_471_269.2,
                ci_lower_ns: 694_937_593.971_75,
                ci_upper_ns: 744_001_561.569_5,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "Large".to_string(),
                condition: "With MTL".to_string(),
                messages: 1_000_000,
                mean_ns: 6_697_315_994.18,
                ci_lower_ns: 6_555_920_127.774751,
                ci_upper_ns: 6_845_586_349.295,
                speedup: None,
            },
        ]
    }

    fn verifying_rows() -> Vec<VerifyingBenchmarkResult> {
        vec![
            VerifyingBenchmarkResult {
                size: "Small".to_string(),
                condition: "MTL (trust cached)".to_string(),
                messages: 100,
                mean_ns: 515_510.097_363_821_3,
                ci_lower_ns: 491_418.359_594_425_3,
                ci_upper_ns: 546_598.463_317_569_1,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Small".to_string(),
                condition: "With MTL".to_string(),
                messages: 100,
                mean_ns: 62_986_761.27,
                ci_lower_ns: 61_368_041.962_25,
                ci_upper_ns: 64_732_715.970_25,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Small".to_string(),
                condition: "Without MTL".to_string(),
                messages: 100,
                mean_ns: 59_983_793.13,
                ci_lower_ns: 59_226_095.212,
                ci_upper_ns: 60_774_214.995,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Medium".to_string(),
                condition: "MTL (trust cached)".to_string(),
                messages: 10_000,
                mean_ns: 86_940_231.45,
                ci_lower_ns: 84_180_194.903_75,
                ci_upper_ns: 90_290_208.116_25,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Medium".to_string(),
                condition: "With MTL".to_string(),
                messages: 10_000,
                mean_ns: 6_058_963_034.2,
                ci_lower_ns: 5_910_066_535.4,
                ci_upper_ns: 6_212_580_870.5,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Medium".to_string(),
                condition: "Without MTL".to_string(),
                messages: 10_000,
                mean_ns: 5_871_756_686.4,
                ci_lower_ns: 5_741_663_514.3,
                ci_upper_ns: 6_007_135_723.935,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: "Large".to_string(),
                condition: "MTL (trust cached)".to_string(),
                messages: 1_000_000,
                mean_ns: 12_372_015_590.05,
                ci_lower_ns: 12_052_696_901.971_75,
                ci_upper_ns: 12_693_463_667.932253,
                speedup: None,
            },
        ]
    }

    fn wire_rows() -> Vec<WireSizeResult> {
        vec![
            WireSizeResult {
                size: "small".to_string(),
                message_count: 100,
                ladder_len: 1,
                condensed_min: 0,
                condensed_mean: 0.0,
                condensed_max: 0,
                condensed_uniform: true,
                with_mtl_total_bytes: 24_424,
                without_mtl_total_bytes: 815_632,
                reduction_percent: 97.005_512_289_856_2,
                plain_total_bytes: 0,
                plain_signature_len_bytes: 0,
            },
            WireSizeResult {
                size: "medium".to_string(),
                message_count: 10_000,
                ladder_len: 1,
                condensed_min: 0,
                condensed_mean: 0.0,
                condensed_max: 0,
                condensed_uniform: true,
                with_mtl_total_bytes: 2_743_352,
                without_mtl_total_bytes: 83_295_296,
                reduction_percent: 96.706_474_276_770_68,
                plain_total_bytes: 0,
                plain_signature_len_bytes: 0,
            },
            WireSizeResult {
                size: "large".to_string(),
                message_count: 1_000_000,
                ladder_len: 1,
                condensed_min: 0,
                condensed_mean: 0.0,
                condensed_max: 0,
                condensed_uniform: true,
                with_mtl_total_bytes: 367_067_832,
                without_mtl_total_bytes: 8_487_059_712,
                reduction_percent: 95.674_970_549_800_68,
                plain_total_bytes: 0,
                plain_signature_len_bytes: 0,
            },
        ]
    }

    fn plain_fixture() -> Vec<PlainTimingResult> {
        use crate::results::EvidenceClass;
        vec![
            PlainTimingResult {
                size: "Small".to_string(),
                messages: 100,
                signing_mean_ns: Some(737_922_991.4),
                signing_ci_lower_ns: Some(725_100_000.0),
                signing_ci_upper_ns: Some(750_000_000.0),
                verifying_mean_ns: Some(736_473.46),
                verifying_ci_lower_ns: Some(720_000.0),
                verifying_ci_upper_ns: Some(750_000.0),
                class: EvidenceClass::Measured,
                samples: None,
            },
            PlainTimingResult {
                size: "OneK".to_string(),
                messages: 1_000,
                signing_mean_ns: Some(737_922_991.4),
                signing_ci_lower_ns: None,
                signing_ci_upper_ns: None,
                verifying_mean_ns: None,
                verifying_ci_lower_ns: None,
                verifying_ci_upper_ns: None,
                class: EvidenceClass::Extrapolated,
                samples: Some(2),
            },
            PlainTimingResult {
                size: "Medium".to_string(),
                messages: 10_000,
                signing_mean_ns: Some(689_912_358.1),
                signing_ci_lower_ns: Some(675_000_000.0),
                signing_ci_upper_ns: Some(705_000_000.0),
                verifying_mean_ns: Some(750_929.80),
                verifying_ci_lower_ns: Some(735_000.0),
                verifying_ci_upper_ns: Some(765_000.0),
                class: EvidenceClass::Measured,
                samples: None,
            },
            PlainTimingResult {
                size: "Large".to_string(),
                messages: 1_000_000,
                signing_mean_ns: Some(737_922_991.4),
                signing_ci_lower_ns: None,
                signing_ci_upper_ns: None,
                verifying_mean_ns: None,
                verifying_ci_lower_ns: None,
                verifying_ci_upper_ns: None,
                class: EvidenceClass::Extrapolated,
                samples: Some(2),
            },
        ]
    }

    fn memory_rows() -> Vec<MemoryResult> {
        // Values mirror the committed `target/memory-report.json` run and the
        // matching wire rows. Harness buffers = ladder + condensed sum +
        // (N+1) x 32-byte Vec overhead:
        //   small  = 7_992 + 16_432 + 101 x 32      = 27_656
        //   medium = 8_056 + 2_735_296 + 10_001 x 32 = 3_063_384
        //   large  = 8_120 + 367_059_712 + 1_000_001 x 32 = 399_067_864
        vec![
            MemoryResult {
                size: "small".to_string(),
                message_count: 100,
                measured_mean_bytes: 851_968,
                libmtl_analytical_bytes: 2_097_152,
                harness_buffers_bytes: 27_656,
                analytical_total_bytes: 2_178_594,
                residual_bytes: 851_968 - 2_178_594,
                residual_percent: ((851_968.0 - 2_178_594.0) / 2_178_594.0) * 100.0,
                measured_analytical_total_ratio: 851_968.0 / 2_178_594.0,
                ratio: 851_968.0 / 2_178_594.0,
                measured_range_min_bytes: 73_728,
                measured_range_max_bytes: 1_630_208,
                measured_stddev_bytes: 778_240.0,
                median_bytes: 851_968,
                iqr_bytes: 1_556_480,
                samples: 2,
                method: "isolated-process".to_string(),
                dataset_buffers_bytes: 53_600,
                keypair_bytes: 186,
                measured_peak_bytes: 851_968,
                stages: None,
            },
            MemoryResult {
                size: "medium".to_string(),
                message_count: 10_000,
                measured_mean_bytes: 2_258_944,
                libmtl_analytical_bytes: 2_097_152,
                harness_buffers_bytes: 3_063_384,
                analytical_total_bytes: 10_520_722,
                residual_bytes: 2_258_944 - 10_520_722,
                residual_percent: ((2_258_944.0 - 10_520_722.0) / 10_520_722.0) * 100.0,
                measured_analytical_total_ratio: 2_258_944.0 / 10_520_722.0,
                ratio: 2_258_944.0 / 10_520_722.0,
                measured_range_min_bytes: 942_080,
                measured_range_max_bytes: 3_575_808,
                measured_stddev_bytes: 1_316_864.0,
                median_bytes: 2_258_944,
                iqr_bytes: 2_633_728,
                samples: 2,
                method: "isolated-process".to_string(),
                dataset_buffers_bytes: 5_360_000,
                keypair_bytes: 186,
                measured_peak_bytes: 2_258_944,
                stages: None,
            },
            MemoryResult {
                size: "large".to_string(),
                message_count: 1_000_000,
                measured_mean_bytes: 320_679_936,
                libmtl_analytical_bytes: 49_283_072,
                harness_buffers_bytes: 399_067_864,
                analytical_total_bytes: 984_351_122,
                residual_bytes: 320_679_936 - 984_351_122,
                residual_percent: ((320_679_936.0 - 984_351_122.0) / 984_351_122.0) * 100.0,
                measured_analytical_total_ratio: 320_679_936.0 / 984_351_122.0,
                ratio: 320_679_936.0 / 984_351_122.0,
                measured_range_min_bytes: 80_093_184,
                measured_range_max_bytes: 561_266_688,
                measured_stddev_bytes: 240_586_752.0,
                median_bytes: 320_679_936,
                iqr_bytes: 481_173_504,
                samples: 2,
                method: "isolated-process".to_string(),
                dataset_buffers_bytes: 536_000_000,
                keypair_bytes: 186,
                measured_peak_bytes: 320_679_936,
                stages: None,
            },
        ]
    }

    #[test]
    fn size_n_maps_known_labels_case_insensitively() {
        assert_eq!(size_n("Small"), Some(100));
        assert_eq!(size_n("OneK"), Some(1_000));
        assert_eq!(size_n("MEDIUM"), Some(10_000));
        assert_eq!(size_n("large"), Some(1_000_000));
        assert_eq!(size_n("gigantic"), None);
    }

    #[test]
    fn fmt_time_ns_scales_units() {
        assert_eq!(fmt_time_ns(595_778_430.5), "596 ms");
        assert_eq!(fmt_time_ns(515_510.097), "516 µs");
        assert_eq!(fmt_time_ns(72_598_508_466.3), "1.21 min");
        assert_eq!(fmt_time_ns(1_234.0), "1.23 µs");
        assert_eq!(fmt_time_ns(42.0), "42 ns");
        assert_eq!(fmt_time_ns(2.5e9), "2.5 s");
        assert_eq!(fmt_time_ns(7.07e12), "1.96 h");
        assert_eq!(fmt_time_ns(7.07e14), "8.18 d");
        assert_eq!(fmt_time_ns(f64::NAN), "-");
    }

    #[test]
    fn fmt_bytes_and_mib() {
        assert_eq!(fmt_bytes(24_424.0), "24.4 kB");
        assert_eq!(fmt_bytes(8_487_059_712.0), "8.49 GB");
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_mib(851_968.0), "832 KiB");
        assert_eq!(fmt_mib(320_679_936.0), "306 MiB");
        assert_eq!(fmt_mib(2_097_152.0), "2 MiB");
    }

    #[test]
    fn fmt_count_groups_thousands() {
        assert_eq!(fmt_count(100.0), "100");
        assert_eq!(fmt_count(1_000.0), "1,000");
        assert_eq!(fmt_count(10_000.0), "10,000");
        assert_eq!(fmt_count(1_000_000.0), "1,000,000");
    }

    #[test]
    fn fit_linear_total_recovers_known_line() {
        let pts = [(1.0, 3.0), (2.0, 5.0), (3.0, 7.0)];
        let (intercept, slope) = fit_linear_total(&pts).expect("fit");
        assert!((intercept - 1.0).abs() < 1e-9, "intercept {intercept}");
        assert!((slope - 2.0).abs() < 1e-9, "slope {slope}");
        assert!(fit_linear_total(&[(1.0, 2.0)]).is_none());
        assert!(fit_linear_total(&[]).is_none());
        assert!(fit_linear_total(&[(1.0, 2.0), (1.0, 3.0)]).is_none());
    }

    #[test]
    fn signing_series_counts_and_annotations() {
        let data = timing_bar_data(&signing_rows(), false, true);
        // 3 series survive: with measured (4), without measured (2),
        // without extrapolated (2) — with-MTL has a row at every size.
        let labels: Vec<&str> = data.series.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(data.series.len(), 3, "labels: {labels:?}");
        let counts: Vec<usize> = data.series.iter().map(|s| s.points.len()).collect();
        assert_eq!(counts, vec![4, 2, 2]);

        // Extrapolated without-MTL Medium/Large annotations.
        let extrap = data
            .series
            .iter()
            .find(|s| s.label.contains("Without MTL (extrapolated)"))
            .expect("extrapolated series");
        let ann: Vec<&str> = extrap
            .points
            .iter()
            .filter_map(|p| p.annotation.as_deref())
            .collect();
        assert_eq!(ann, vec!["≈1.96 h", "≈8.18 d"]);
    }

    /// The uniform scaling rule: a row's mean is the cost of one measured
    /// iteration, so the cost of the whole batch is `mean x n / messages`. A
    /// whole-batch row (`messages == n`) is left untouched by the same rule,
    /// and a per-message baseline row (`messages == 1`) becomes the cost of
    /// signing all `n` messages.
    #[test]
    fn timing_bars_scale_a_per_message_baseline_to_the_batch_total() {
        let rows = vec![
            SigningBenchmarkResult {
                size: "Small".to_string(),
                condition: "With MTL".to_string(),
                messages: 100,
                mean_ns: 500_000_000.0,
                ci_lower_ns: 490_000_000.0,
                ci_upper_ns: 510_000_000.0,
                speedup: None,
            },
            SigningBenchmarkResult {
                size: "Small".to_string(),
                condition: "Without MTL".to_string(),
                messages: 1,
                mean_ns: 500_000_000.0,
                ci_lower_ns: 490_000_000.0,
                ci_upper_ns: 510_000_000.0,
                speedup: None,
            },
        ];
        let data = timing_bar_data(&rows, false, false);
        let point = |label: &str| {
            data.series
                .iter()
                .find(|s| s.label == label)
                .unwrap_or_else(|| panic!("series {label}"))
                .points
                .iter()
                .find(|p| p.n == 100)
                .unwrap_or_else(|| panic!("the Small point of {label}"))
                .clone()
        };

        // One signature per iteration: the batch of 100 costs 100 signatures.
        let without = point("Without MTL (measured)");
        assert!(
            (without.value - 50_000_000_000.0).abs() < 1e-6,
            "value: {}",
            without.value
        );
        assert!(
            (without.ci_lower.unwrap() - 49_000_000_000.0).abs() < 1e-6,
            "ci lower: {:?}",
            without.ci_lower
        );
        assert!(
            (without.ci_upper.unwrap() - 51_000_000_000.0).abs() < 1e-6,
            "ci upper: {:?}",
            without.ci_upper
        );

        // A whole-batch row is left untouched by the same rule.
        let with_mtl = point("With MTL (measured)");
        assert!(
            (with_mtl.value - 500_000_000.0).abs() < 1e-6,
            "value: {}",
            with_mtl.value
        );
    }

    #[test]
    fn verifying_series_counts_trust_has_no_extrapolation() {
        let data = timing_bar_data(&verifying_rows(), true, false);
        // 5 series: with (2), without (2), trust (3), with extrap (2),
        // without extrap (2).
        assert_eq!(data.series.len(), 5);
        let counts: Vec<usize> = data.series.iter().map(|s| s.points.len()).collect();
        assert_eq!(counts, vec![2, 2, 3, 2, 2]);
        // Trust series present and measured-only.
        let trust = data
            .series
            .iter()
            .find(|s| s.label.contains("Trust-cached"))
            .expect("trust series");
        assert!(!trust.hollow);
        assert!(!data
            .series
            .iter()
            .any(|s| s.label.contains("Trust") && s.hollow));
        // No annotations when annotate_extrapolated is false.
        assert!(data
            .series
            .iter()
            .all(|s| s.points.iter().all(|p| p.annotation.is_none())));
    }

    #[test]
    fn empty_rows_yield_empty_series() {
        let empty: Vec<SigningBenchmarkResult> = vec![];
        let data = timing_bar_data(&empty, false, true);
        assert!(data.series.is_empty());
        assert!(data.group_annotations.is_empty());

        let wire = wire_bar_data(&[], &[]);
        assert!(wire.series.iter().all(|s| s.points.is_empty()));
        assert!(wire.group_annotations.is_empty());

        let amort = amortization_data(&[]);
        assert!(amort.with_points.is_empty());
        assert!(amort.without_points.is_empty());
        assert!(amort.fit.is_none());

        let mem = memory_data(&[]);
        assert!(mem.points.is_empty());
    }

    #[test]
    fn wire_reduction_labels_and_series() {
        let rows = wire_rows();
        let data = wire_bar_data(&rows, &[]);
        assert_eq!(data.series.len(), 2);
        assert_eq!(data.series[0].points.len(), 3);
        assert_eq!(data.series[1].points.len(), 3);
        assert_eq!(data.group_annotations.len(), 3);
        let labels: Vec<&str> = data
            .group_annotations
            .iter()
            .map(|a| a.2.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["−97.0% (C vs B)", "−96.7% (C vs B)", "−95.7% (C vs B)"]
        );
        for (i, row) in rows.iter().enumerate() {
            let (n, y, _) = data.group_annotations[i];
            assert_eq!(n, row.message_count as u64);
            assert!(y > row.without_mtl_total_bytes as f64);
        }
    }

    #[test]
    fn wire_bar_data_adds_plain_series_when_plain_bytes_present() {
        let rows = vec![
            WireSizeResult {
                size: "small".to_string(),
                message_count: 100,
                ladder_len: 1,
                condensed_min: 0,
                condensed_mean: 0.0,
                condensed_max: 0,
                condensed_uniform: true,
                with_mtl_total_bytes: 24_424,
                without_mtl_total_bytes: 815_632,
                reduction_percent: 97.005,
                plain_total_bytes: 785_600,
                plain_signature_len_bytes: 7_856,
            },
            WireSizeResult {
                size: "medium".to_string(),
                message_count: 10_000,
                ladder_len: 1,
                condensed_min: 0,
                condensed_mean: 0.0,
                condensed_max: 0,
                condensed_uniform: true,
                with_mtl_total_bytes: 2_743_352,
                without_mtl_total_bytes: 83_295_296,
                reduction_percent: 96.706,
                plain_total_bytes: 78_560_000,
                plain_signature_len_bytes: 7_856,
            },
        ];
        let plain = plain_fixture();
        let data = wire_bar_data(&rows, &plain);
        assert_eq!(data.series.len(), 3);
        let plain_series = data
            .series
            .iter()
            .find(|s| s.label.contains("Plain SLH-DSA"))
            .expect("plain series");
        assert!(plain_series.hollow);
        assert_eq!(plain_series.points.len(), 2);
        assert_eq!(plain_series.points[0].n, 100);
        assert_eq!(plain_series.points[0].value, 785_600.0);
        assert_eq!(plain_series.points[1].n, 10_000);
        assert_eq!(plain_series.points[1].value, 78_560_000.0);
    }

    #[test]
    fn plain_fixture_helper_has_expected_sizes() {
        let plain = plain_fixture();
        let sizes: Vec<&str> = plain.iter().map(|r| r.size.as_str()).collect();
        assert_eq!(sizes, vec!["Small", "OneK", "Medium", "Large"]);
    }

    #[test]
    fn amortization_fit_matches_known_values() {
        let data = amortization_data(&signing_rows());
        assert_eq!(data.with_points.len(), 4);
        assert_eq!(data.without_points.len(), 2);
        let (intercept, slope) = data.fit.expect("fit");
        // Fit is descriptive of these runs: ~654 ms fixed + ~6.04 µs/msg.
        assert!(
            intercept > 6.4e8 && intercept < 6.7e8,
            "intercept {intercept}"
        );
        assert!(slope > 5_900.0 && slope < 6_200.0, "slope {slope}");
        // With-MTL per-message time falls with N (amortization).
        let first = data.with_points.first().expect("first");
        let last = data.with_points.last().expect("last");
        assert!(first.1 > last.1);
    }

    #[test]
    fn memory_ratios_recomputed_from_bytes() {
        let rows = memory_rows();
        let data = memory_data(&rows);
        assert_eq!(data.points.len(), 3);
        // Admin ratios are measured / analytical_total, not the stored ratio.
        assert_eq!(data.points[0].ratio_label, "0.39x");
        assert_eq!(data.points[1].ratio_label, "0.21x");
        assert_eq!(data.points[2].ratio_label, "0.33x");
        assert_eq!(data.points[2].measured, 320_679_936.0);
        assert_eq!(data.points[2].measured_peak, 320_679_936.0);
        assert_eq!(data.points[2].analytical_total, 984_351_122.0);
        assert_eq!(data.points[2].residual, -663_671_186.0);
        assert_eq!(data.points[0].measured_min, 73_728.0);
        assert_eq!(data.points[0].measured_max, 1_630_208.0);
    }

    #[test]
    fn memory_stacked_series_sums_to_analytical_total() {
        let rows = memory_rows();
        let data = memory_data(&rows);
        for p in &data.points {
            assert_eq!(
                p.libmtl_pages + p.harness_buffers + p.dataset_buffers + p.keypair,
                p.analytical_total,
                "stacked analytical terms must equal analytical_total at n={}",
                p.n
            );
        }
    }

    #[test]
    fn generate_charts_writes_seven_svgs_for_synthetic_results() {
        let results = synthetic_results();
        let dir = std::env::temp_dir().join(format!(
            "climb-report-charts-synth-{}-{}",
            std::process::id(),
            "gen"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = generate_charts(&results, &dir).expect("generate");
        assert_eq!(paths.len(), 7);
        for (i, path) in paths.iter().enumerate() {
            assert_eq!(
                path.file_name().and_then(|f| f.to_str()),
                Some(CHART_FILES[i])
            );
            let svg = std::fs::read_to_string(path).expect("read svg");
            assert!(svg.contains("<svg"), "missing <svg in {path:?}");
            assert!(svg.contains("</svg>"), "missing </svg> in {path:?}");
            assert!(svg.contains("<text"), "missing <text in {path:?}");
            assert!(svg.len() > 1_024, "{} too small", svg.len());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reuse_data_labels_break_evens_without_rust_option_debug() {
        use crate::results::{EvidenceClass, ReuseCurvePoint};
        let rows = vec![ReuseResult {
            size: "small".to_string(),
            batch_size: 100,
            curve: vec![
                ReuseCurvePoint {
                    reuse: 1,
                    effective_time_ns: 900.0,
                    effective_bytes: 8000.0,
                },
                ReuseCurvePoint {
                    reuse: 2,
                    effective_time_ns: 500.0,
                    effective_bytes: 4000.0,
                },
            ],
            breakeven_reuse_time: Some(1),
            breakeven_reuse_bytes: None,
            r_star_time: Some(1),
            r_star_bytes: None,
            baseline_time_ns: 1000.0,
            baseline_bytes: 7856.0,
            class: EvidenceClass::Derived,
        }];
        let data = reuse_data(&rows);
        assert_eq!(data.series.len(), 1);
        let labels: Vec<&str> = data.series[0]
            .metrics
            .iter()
            .map(ReuseMetric::label)
            .collect();
        assert_eq!(
            labels,
            vec!["Small · time (r*=1)", "Small · bytes (r*=\u{2014})"],
            "legend tokens must be plain r* values, never `Some(..)`"
        );
        // Ratios are normalised to the per-message baseline.
        assert_eq!(data.series[0].metrics[0].points[0], (1.0, 0.9));
        assert_eq!(data.series[0].metrics[1].points[0], (1.0, 8000.0 / 7856.0));
    }

    #[test]
    fn generate_charts_handles_empty_results_without_panic() {
        let results = empty_results();
        let dir = std::env::temp_dir().join(format!(
            "climb-report-charts-empty-{}-{}",
            std::process::id(),
            "gen"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = generate_charts(&results, &dir).expect("generate");
        assert_eq!(paths.len(), 7);
        for path in &paths {
            let svg = std::fs::read_to_string(path).expect("read svg");
            assert!(svg.contains("<svg"), "missing <svg in {path:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A minimal but fully populated [`ClimbResults`] covering every size.
    fn synthetic_results() -> ClimbResults {
        use crate::results::{
            AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, EnvironmentFingerprint,
            MemoryResults,
        };
        ClimbResults {
            version: "1.0".to_string(),
            generated_at: "test".to_string(),
            workspace_root: ".".to_string(),
            benchmark: BenchmarkResults {
                signing: signing_rows(),
                verifying: verifying_rows(),
            },
            wire_size: wire_rows(),
            memory: MemoryResults {
                measured: memory_rows(),
                analytical_model: AnalyticalMemoryModel {
                    tree_page_size_bytes: 4096,
                    tree_max_pages: 1,
                    randomizer_pages: 1,
                    hash_size_bytes: 32,
                },
            },
            environment: EnvironmentFingerprint {
                schema_version: 1,
                timestamp: "test".to_string(),
                system: serde_json::json!({}),
                toolchain: serde_json::json!({}),
                cpu: serde_json::json!({}),
                submodules: Vec::new(),
                docker: serde_json::json!({}),
                benchmark_runtime: serde_json::json!({}),
            },
            dns_pipeline: None,
            dns_anatomy: None,
            ladder: None,
            plain: Vec::new(),
            classical: Vec::new(),
            comparisons: Vec::new(),
            derived: DerivedSummary {
                overall_wire_reduction_percent: None,
                largest_wire_reduction_percent: None,
                overall_verifying_trust_cached_speedup_avg: None,
                headline: None,
                reuse: Vec::new(),
                udp_fit: None,
                signature_context: Vec::new(),
                stability: None,
                primitives: None,
                bound_ok: Some(true),
                baseline_ok: Some(true),
                bound_offenders: Vec::new(),
                baseline_offenders: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    /// A [`ClimbResults`] with every collection empty.
    fn empty_results() -> ClimbResults {
        let mut results = synthetic_results();
        results.benchmark.signing.clear();
        results.benchmark.verifying.clear();
        results.wire_size.clear();
        results.memory.measured.clear();
        results
    }
}
