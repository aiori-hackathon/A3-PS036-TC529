//! SVG rendering for the Phase 16 report charts.
//!
//! Turns the prepared series from [`super`] into five self-contained SVG
//! files. The measured-vs-modelled distinction is a rendering rule applied to
//! every chart, not a per-chart choice:
//!
//! * **measured** values — solid fills, with 95% CI whiskers where available;
//! * **extrapolated** values — hollow bars (white interior, coloured
//!   outline) plus `≈…` value annotations;
//! * **modelled** values (OLS fit, analytical memory estimates) — grey
//!   dashed line / hollow diamond markers.
//!
//! Every chart ends with a three-line footnote naming the source files a
//! reader should cross-check. All extrapolations are descriptive summaries
//! of the measured runs, never guarantees.

use std::path::Path;

use plotters::coord::Shift;
use plotters::element::ErrorBarOrientV;
use plotters::prelude::*;

use super::{
    AmortizationData, BarChartData, MemoryData, COLOR_MODEL, COLOR_WITHOUT_MTL, COLOR_WITH_MTL,
};
use crate::error::ReportError;

/// Footnotes for the signing-time chart.
pub const SIGNING_FOOTNOTES: [&str; 3] = [
    "Source: target/climb-results.json → benchmark.signing (consolidated from target/criterion/).",
    "Solid bars = Criterion-measured mean with 95% CI whiskers; hollow bars = extrapolated.",
    "Extrapolation is N × that condition's mean measured per-message cost — descriptive, not measured.",
];

/// Footnotes for the verification-time chart.
pub const VERIFYING_FOOTNOTES: [&str; 3] = [
    "Source: target/climb-results.json → benchmark.verifying (consolidated from target/criterion/).",
    "Solid bars = Criterion-measured mean with 95% CI whiskers; hollow bars = extrapolated.",
    "Trust-cached ladder rows are measured-only; extrapolated rows are N × mean per-message cost.",
];

/// Footnotes for the transmitted-bytes chart.
pub const WIRE_FOOTNOTES: [&str; 3] = [
    "Source: target/climb-results.json → wire_size (from target/wire-report.json, `cargo xtask wire-report`).",
    "Both series are measured: MTL file-level totals vs one-signature-per-message totals.",
    "−% labels compare the two totals for that dataset size; no value on this chart is modelled.",
];

/// Footnotes for the amortization-curve chart.
pub const AMORTIZATION_FOOTNOTES: [&str; 3] = [
    "Source: target/climb-results.json → benchmark.signing (Criterion, pinned taskset -c 2).",
    "Points = measured per-message time; dashed line = OLS fit of the with-MTL batch total.",
    "The fit (slope + intercept/N) extrapolates only along the measured with-MTL shape.",
];

/// Footnotes for the measured-vs-analytical memory chart.
pub const MEMORY_FOOTNOTES: [&str; 3] = [
    "Source: target/climb-results.json → memory (from target/memory-report.json; model: climb-analytical).",
    "Filled circles + whiskers = measured peak-RSS deltas (min/mean/max); analytical = stacked bars (libMTL pages + harness buffers, ESTIMATE).",
    "Ratios shown as measured ÷ analytical_total; residual annotated (measured − analytical_total) with bytes and %.",
];

/// Canvas width for every chart, in pixels.
const CHART_W: u32 = 960;
/// Canvas height for the grouped bar charts.
const BAR_H: u32 = 640;
/// Canvas height for the amortization and memory charts.
const CURVE_H: u32 = 600;
/// Height reserved at the bottom of the canvas for the footnote block.
const FOOT_H: u32 = 48;

/// y-axis label area (left), in pixels — used to estimate plot geometry for
/// centring annotation text above bars.
const Y_LABEL_AREA: u32 = 92;
/// Outer margin applied on every side, in pixels.
const MARGIN: u32 = 6;

/// Map a plotters drawing error onto [`ReportError`] with the target path.
fn chart_err(path: &Path, e: impl std::fmt::Display) -> ReportError {
    ReportError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(e.to_string()),
    }
}

/// Snap `(lo, hi)` outward to whole decades, guaranteeing `lo < hi` and both
/// strictly positive so `.log_scale()` accepts them.
fn decade_bounds(lo: f64, hi: f64) -> (f64, f64) {
    let lo = if lo.is_finite() && lo > 0.0 { lo } else { 1.0 };
    let hi = if hi.is_finite() && hi > lo {
        hi
    } else {
        lo * 1e3
    };
    let lo_d = 10f64.powf(lo.log10().floor());
    let mut hi_d = 10f64.powf(hi.log10().ceil());
    if hi_d <= lo_d {
        hi_d = lo_d * 10.0;
    }
    (lo_d, hi_d)
}

/// Draw the three-line footnote block into the bottom strip of the canvas.
fn draw_footnotes<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    footnotes: &[&str; 3],
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let gray = RGBColor(95, 95, 100);
    let style = TextStyle::from(("sans-serif", 10)).color(&gray);
    for (i, line) in footnotes.iter().enumerate() {
        area.draw_text(line, &style, (6, 5 + i as i32 * 15))?;
    }
    Ok(())
}

/// Render one grouped log-scale bar chart (signing / verifying / wire).
///
/// Bars of one dataset size share a slot centred on that size; the slot is
/// 0.62 decades wide so neighbouring slots (≥1 decade apart) never collide.
pub fn draw_bar_chart(
    path: &Path,
    data: &BarChartData,
    footnotes: &[&str; 3],
) -> Result<(), ReportError> {
    let root = SVGBackend::new(path, (CHART_W, BAR_H)).into_drawing_area();
    root.fill(&WHITE).map_err(|e| chart_err(path, e))?;
    let (plot, foot) = root.split_vertically(BAR_H - FOOT_H);

    // Grouped dataset sizes, ascending.
    let mut groups: Vec<u64> = Vec::new();
    for series in &data.series {
        for p in &series.points {
            if p.value > 0.0 && !groups.contains(&p.n) {
                groups.push(p.n);
            }
        }
    }
    groups.sort_unstable();

    let (x_lo, x_hi) = match (groups.first(), groups.last()) {
        (Some(&mn), Some(&mx)) if mx > mn => (mn as f64 * 0.4, mx as f64 * 2.5),
        (Some(&mn), Some(_)) => (mn as f64 * 0.4, mn as f64 * 2.5),
        _ => (40.0, 2_500_000.0),
    };

    // y range: every value, CI bound, and group annotation.
    let (mut y_lo, mut y_hi) = (f64::INFINITY, 0.0f64);
    for series in &data.series {
        for p in &series.points {
            if p.value <= 0.0 {
                continue;
            }
            y_lo = y_lo.min(p.value);
            y_hi = y_hi.max(p.value);
            if let Some(c) = p.ci_lower {
                if c > 0.0 {
                    y_lo = y_lo.min(c);
                }
            }
            if let Some(c) = p.ci_upper {
                y_hi = y_hi.max(c);
            }
        }
    }
    for (_, y, _) in &data.group_annotations {
        if *y > 0.0 {
            y_hi = y_hi.max(*y);
        }
    }
    if !y_lo.is_finite() {
        y_lo = 1.0;
        y_hi = 1_000.0;
    }
    let (y_lo, y_hi) = decade_bounds(y_lo, y_hi);

    let mut chart = ChartBuilder::on(&plot)
        .margin(MARGIN)
        .caption(data.title.as_str(), ("sans-serif", 17))
        .x_label_area_size(46)
        .y_label_area_size(Y_LABEL_AREA)
        .build_cartesian_2d((x_lo..x_hi).log_scale(), (y_lo..y_hi).log_scale())
        .map_err(|e| chart_err(path, e))?;
    chart
        .configure_mesh()
        .x_desc(data.x_desc.as_str())
        .y_desc(data.y_desc.as_str())
        .x_label_formatter(&|v: &f64| (data.x_fmt)(*v))
        .y_label_formatter(&|v: &f64| (data.y_fmt)(*v))
        .label_style(("sans-serif", 11))
        .axis_desc_style(("sans-serif", 12))
        .draw()
        .map_err(|e| chart_err(path, e))?;

    let s_count = data.series.len();
    let slot = 0.62_f64; // decades per group slot
    let bar_w = if s_count > 0 {
        slot / s_count as f64
    } else {
        slot
    };
    // Approximate plot width in pixels, for centring text labels.
    let px_per_decade =
        (CHART_W as f64 - Y_LABEL_AREA as f64 - 2.0 * MARGIN as f64) / (x_hi / x_lo).log10();

    // Pass 1: bar bodies (solid = measured, white = extrapolated) + legend.
    let mut any_label = false;
    for (i, series) in data.series.iter().enumerate() {
        let offset = (i as f64 - (s_count as f64 - 1.0) / 2.0) * bar_w;
        let body_style = if series.hollow {
            ShapeStyle::from(WHITE).filled()
        } else {
            ShapeStyle::from(series.color).filled()
        };
        let rects: Vec<Rectangle<(f64, f64)>> = series
            .points
            .iter()
            .filter(|p| p.value > 0.0 && p.value >= y_lo)
            .filter_map(|p| {
                let c = (p.n as f64).log10() + offset;
                let left = 10f64.powf(c - bar_w / 2.0);
                let right = 10f64.powf(c + bar_w / 2.0);
                if left < x_lo || right > x_hi {
                    return None;
                }
                Some(Rectangle::new([(left, y_lo), (right, p.value)], body_style))
            })
            .collect();
        if rects.is_empty() {
            continue;
        }
        let color = series.color;
        let hollow = series.hollow;
        let label = series.label.clone();
        let drawings = chart.draw_series(rects).map_err(|e| chart_err(path, e))?;
        drawings.label(label).legend(move |(x, y)| {
            Rectangle::new([(x, y), (x + 16, y + 11)], {
                if hollow {
                    ShapeStyle::from(color).stroke_width(2)
                } else {
                    ShapeStyle::from(color).filled()
                }
            })
        });
        any_label = true;
    }

    // Pass 2: coloured outlines on the extrapolated (hollow) bars.
    for (i, series) in data.series.iter().enumerate() {
        if !series.hollow {
            continue;
        }
        let offset = (i as f64 - (s_count as f64 - 1.0) / 2.0) * bar_w;
        let outline = ShapeStyle::from(series.color).stroke_width(2);
        let rects: Vec<Rectangle<(f64, f64)>> = series
            .points
            .iter()
            .filter(|p| p.value > 0.0 && p.value >= y_lo)
            .filter_map(|p| {
                let c = (p.n as f64).log10() + offset;
                let left = 10f64.powf(c - bar_w / 2.0);
                let right = 10f64.powf(c + bar_w / 2.0);
                if left < x_lo || right > x_hi {
                    return None;
                }
                Some(Rectangle::new([(left, y_lo), (right, p.value)], outline))
            })
            .collect();
        if !rects.is_empty() {
            chart.draw_series(rects).map_err(|e| chart_err(path, e))?;
        }
    }

    // Pass 3: 95% CI whiskers on measured bars (never on extrapolated ones).
    for (i, series) in data.series.iter().enumerate() {
        if series.hollow {
            continue;
        }
        let offset = (i as f64 - (s_count as f64 - 1.0) / 2.0) * bar_w;
        let bars: Vec<ErrorBar<f64, f64, ErrorBarOrientV<f64, f64>>> = series
            .points
            .iter()
            .filter_map(|p| {
                let (lo, hi) = (p.ci_lower?, p.ci_upper?);
                if p.value <= 0.0 || lo <= 0.0 || hi <= 0.0 {
                    return None;
                }
                let centre = 10f64.powf((p.n as f64).log10() + offset);
                Some(ErrorBar::new_vertical(
                    centre,
                    lo.max(y_lo),
                    p.value,
                    hi,
                    series.color,
                    7,
                ))
            })
            .collect();
        if !bars.is_empty() {
            chart.draw_series(bars).map_err(|e| chart_err(path, e))?;
        }
    }

    // Pass 4: value annotations above individual bars (≈ extrapolated values).
    let dark = RGBColor(70, 70, 75);
    let ann_style = TextStyle::from(("sans-serif", 9)).color(&dark);
    let mut annotations: Vec<Text<'_, (f64, f64), String>> = Vec::new();
    for (i, series) in data.series.iter().enumerate() {
        let offset = (i as f64 - (s_count as f64 - 1.0) / 2.0) * bar_w;
        for p in &series.points {
            let Some(text) = &p.annotation else { continue };
            let baseline = p.value.max(p.ci_upper.unwrap_or(p.value));
            if baseline <= 0.0 {
                continue;
            }
            let half_px = text.chars().count() as f64 * 5.4 / 2.0;
            let shift = half_px / px_per_decade;
            let x = 10f64.powf((p.n as f64).log10() + offset - shift);
            let y = (baseline * 1.9).min(y_hi / 1.2);
            annotations.push(Text::new(text.clone(), (x, y), ann_style.clone()));
        }
    }
    if !annotations.is_empty() {
        chart
            .draw_series(annotations)
            .map_err(|e| chart_err(path, e))?;
    }

    // Pass 5: group-level annotations (transmitted-bytes reduction labels).
    let group_style = TextStyle::from(("sans-serif", 12)).color(&dark);
    let group_texts: Vec<Text<'_, (f64, f64), String>> = data
        .group_annotations
        .iter()
        .filter_map(|(n, y, text)| {
            if *y <= 0.0 {
                return None;
            }
            let half_px = text.chars().count() as f64 * 6.0 / 2.0;
            let shift = half_px / px_per_decade;
            let x = 10f64.powf((*n as f64).log10() - shift);
            let y = (*y).min(y_hi / 1.15);
            Some(Text::new(text.clone(), (x, y), group_style.clone()))
        })
        .collect();
    if !group_texts.is_empty() {
        chart
            .draw_series(group_texts)
            .map_err(|e| chart_err(path, e))?;
    }

    if any_label {
        chart
            .configure_series_labels()
            .position(SeriesLabelPosition::UpperLeft)
            .background_style(WHITE.mix(0.85))
            .border_style(BLACK)
            .label_font(("sans-serif", 11))
            .draw()
            .map_err(|e| chart_err(path, e))?;
    }

    draw_footnotes(&foot, footnotes).map_err(|e| chart_err(path, e))?;
    root.present().map_err(|e| chart_err(path, e))?;
    Ok(())
}

/// Render the amortization curve: measured per-message costs plus the OLS
/// fit of the with-MTL batch total.
///
/// The fit is defined on the batch total (`intercept + slope·N`), so it is
/// drawn as per-message `slope + intercept/N`. Both axes are log-scale.
pub fn draw_amortization(
    path: &Path,
    data: &AmortizationData,
    footnotes: &[&str; 3],
) -> Result<(), ReportError> {
    let root = SVGBackend::new(path, (CHART_W, CURVE_H)).into_drawing_area();
    root.fill(&WHITE).map_err(|e| chart_err(path, e))?;
    let (plot, foot) = root.split_vertically(CURVE_H - FOOT_H);

    let ns: Vec<u64> = data
        .with_points
        .iter()
        .chain(data.without_points.iter())
        .map(|(n, _)| *n)
        .collect();
    let (x_lo, x_hi) = match (ns.iter().min(), ns.iter().max()) {
        (Some(&mn), Some(&mx)) if mx > mn => (mn as f64 * 0.4, mx as f64 * 2.5),
        (Some(&mn), Some(_)) => (mn as f64 * 0.4, mn as f64 * 2.5),
        _ => (40.0, 2_500_000.0),
    };

    // Sample the OLS fit across the x range (per-message form).
    let fit_curve: Vec<(f64, f64)> = match data.fit {
        Some((intercept, slope)) => {
            let steps = 48u32;
            let lo_l = x_lo.log10();
            let hi_l = x_hi.log10();
            (0..=steps)
                .map(|k| {
                    let n = 10f64.powf(lo_l + (hi_l - lo_l) * k as f64 / steps as f64);
                    (n, slope + intercept / n)
                })
                .collect()
        }
        None => Vec::new(),
    };

    let (mut y_lo, mut y_hi) = (f64::INFINITY, 0.0f64);
    for &(_, v) in data.with_points.iter().chain(data.without_points.iter()) {
        if v > 0.0 && v.is_finite() {
            y_lo = y_lo.min(v);
            y_hi = y_hi.max(v);
        }
    }
    for &(_, v) in fit_curve.iter() {
        if v > 0.0 && v.is_finite() {
            y_lo = y_lo.min(v);
            y_hi = y_hi.max(v);
        }
    }
    if !y_lo.is_finite() {
        y_lo = 1.0;
        y_hi = 1_000.0;
    }
    let (y_lo, y_hi) = decade_bounds(y_lo, y_hi);

    let mut chart = ChartBuilder::on(&plot)
        .margin(MARGIN)
        .caption(data.title.as_str(), ("sans-serif", 17))
        .x_label_area_size(46)
        .y_label_area_size(Y_LABEL_AREA)
        .build_cartesian_2d((x_lo..x_hi).log_scale(), (y_lo..y_hi).log_scale())
        .map_err(|e| chart_err(path, e))?;
    chart
        .configure_mesh()
        .x_desc(data.x_desc.as_str())
        .y_desc(data.y_desc.as_str())
        .x_label_formatter(&|v: &f64| super::fmt_count(*v))
        .y_label_formatter(&|v: &f64| super::fmt_time_ns(*v))
        .label_style(("sans-serif", 11))
        .axis_desc_style(("sans-serif", 12))
        .draw()
        .map_err(|e| chart_err(path, e))?;

    let mut any_label = false;
    if !data.with_points.is_empty() {
        let pts = data.with_points.clone();
        let drawings = chart
            .draw_series(
                pts.iter()
                    .map(|&(n, v)| Circle::new((n as f64, v), 5, COLOR_WITH_MTL.filled())),
            )
            .map_err(|e| chart_err(path, e))?;
        drawings
            .label("With MTL — measured per-message")
            .legend(|(x, y)| Circle::new((x + 7, y + 5), 5, COLOR_WITH_MTL.filled()));
        any_label = true;
    }
    if !data.without_points.is_empty() {
        let pts = data.without_points.clone();
        let drawings = chart
            .draw_series(
                pts.iter()
                    .map(|&(n, v)| Circle::new((n as f64, v), 5, COLOR_WITHOUT_MTL.filled())),
            )
            .map_err(|e| chart_err(path, e))?;
        drawings
            .label("Without MTL — measured per-message")
            .legend(|(x, y)| Circle::new((x + 7, y + 5), 5, COLOR_WITHOUT_MTL.filled()));
        any_label = true;
    }
    if !fit_curve.is_empty() {
        let style = ShapeStyle::from(COLOR_MODEL).stroke_width(2);
        let drawings = chart
            .draw_series(DashedLineSeries::new(
                fit_curve.iter().copied(),
                6,
                6,
                style,
            ))
            .map_err(|e| chart_err(path, e))?;
        drawings
            .label("OLS fit of with-MTL batch total (extrapolated)")
            .legend(|(x, y)| {
                Rectangle::new(
                    [(x, y + 4), (x + 20, y + 7)],
                    ShapeStyle::from(COLOR_MODEL).filled(),
                )
            });
        any_label = true;
    }
    if any_label {
        chart
            .configure_series_labels()
            .position(SeriesLabelPosition::UpperLeft)
            .background_style(WHITE.mix(0.85))
            .border_style(BLACK)
            .label_font(("sans-serif", 11))
            .draw()
            .map_err(|e| chart_err(path, e))?;
    }

    draw_footnotes(&foot, footnotes).map_err(|e| chart_err(path, e))?;
    root.present().map_err(|e| chart_err(path, e))?;
    Ok(())
}

/// Render the measured-vs-analytical memory comparison.
///
/// Filled circles with min/max whiskers are measured peak-RSS deltas;
/// hollow grey diamonds are the analytical model. Ratios are redrawn next to
/// each pair from the stored byte counts.
pub fn draw_memory(
    path: &Path,
    data: &MemoryData,
    footnotes: &[&str; 3],
) -> Result<(), ReportError> {
    let root = SVGBackend::new(path, (CHART_W, CURVE_H)).into_drawing_area();
    root.fill(&WHITE).map_err(|e| chart_err(path, e))?;
    let (plot, foot) = root.split_vertically(CURVE_H - FOOT_H);

    let (x_lo, x_hi) = match (
        data.points.iter().map(|p| p.n).min(),
        data.points.iter().map(|p| p.n).max(),
    ) {
        (Some(mn), Some(mx)) if mx > mn => (mn as f64 * 0.4, mx as f64 * 2.5),
        (Some(mn), Some(_)) => (mn as f64 * 0.4, mn as f64 * 2.5),
        _ => (40.0, 2_500_000.0),
    };

    let (mut y_lo, mut y_hi) = (f64::INFINITY, 0.0f64);
    for p in &data.points {
        for v in [
            p.measured,
            p.measured_min,
            p.measured_max,
            p.analytical_total,
            p.libmtl_pages,
            p.harness_buffers,
        ] {
            if v > 0.0 && v.is_finite() {
                y_lo = y_lo.min(v);
                y_hi = y_hi.max(v);
            }
        }
    }
    if !y_lo.is_finite() {
        y_lo = 1.0;
        y_hi = 1_000.0;
    }
    let (y_lo, y_hi) = decade_bounds(y_lo, y_hi);

    let mut chart = ChartBuilder::on(&plot)
        .margin(MARGIN)
        .caption(data.title.as_str(), ("sans-serif", 17))
        .x_label_area_size(46)
        .y_label_area_size(Y_LABEL_AREA)
        .build_cartesian_2d((x_lo..x_hi).log_scale(), (y_lo..y_hi).log_scale())
        .map_err(|e| chart_err(path, e))?;
    chart
        .configure_mesh()
        .x_desc(data.x_desc.as_str())
        .y_desc(data.y_desc.as_str())
        .x_label_formatter(&|v: &f64| super::fmt_count(*v))
        .y_label_formatter(&|v: &f64| super::fmt_mib(*v))
        .label_style(("sans-serif", 11))
        .axis_desc_style(("sans-serif", 12))
        .draw()
        .map_err(|e| chart_err(path, e))?;

    let mut any_label = false;
    if data.points.iter().any(|p| p.measured > 0.0) {
        let points = data.points.clone();
        let drawings = chart
            .draw_series(points.iter().filter_map(|p| {
                if p.measured <= 0.0 {
                    return None;
                }
                Some(Circle::new(
                    (p.n as f64, p.measured),
                    5,
                    COLOR_WITH_MTL.filled(),
                ))
            }))
            .map_err(|e| chart_err(path, e))?;
        drawings
            .label("Measured peak RSS (mean, min–max whiskers)")
            .legend(|(x, y)| Circle::new((x + 7, y + 5), 5, COLOR_WITH_MTL.filled()));
        any_label = true;
    }
    if data.points.iter().any(|p| p.analytical_total > 0.0) {
        let points = data.points.clone();
        // Draw stacked analytical bars: libMTL pages (bottom) then harness buffers (top)
        let bar_w = 0.30_f64;
        // Bottom segment: libMTL pages
        let drawings_bottom = chart
            .draw_series(points.iter().filter_map(|p| {
                if p.analytical_total <= 0.0 {
                    return None;
                }
                let base = p.libmtl_pages;
                if base <= 0.0 {
                    return None;
                }
                let c = (p.n as f64).log10();
                let left = 10f64.powf(c - bar_w / 2.0);
                let right = 10f64.powf(c + bar_w / 2.0);
                Some(Rectangle::new(
                    [(left, y_lo), (right, base)],
                    ShapeStyle::from(COLOR_WITHOUT_MTL).filled(),
                ))
            }))
            .map_err(|e| chart_err(path, e))?;
        drawings_bottom.label("libMTL pages only").legend(|(x, y)| {
            Rectangle::new(
                [(x, y), (x + 16, y + 11)],
                ShapeStyle::from(COLOR_WITHOUT_MTL).filled(),
            )
        });
        // Top segment: harness buffers (stacked on libMTL pages)
        let drawings_top = chart
            .draw_series(points.iter().filter_map(|p| {
                if p.analytical_total <= 0.0 {
                    return None;
                }
                let base = p.libmtl_pages;
                let topv = p.harness_buffers;
                if topv <= 0.0 {
                    return None;
                }
                let c = (p.n as f64).log10();
                let left = 10f64.powf(c - bar_w / 2.0);
                let right = 10f64.powf(c + bar_w / 2.0);
                Some(Rectangle::new(
                    [(left, base), (right, base + topv)],
                    ShapeStyle::from(RGBColor(44, 160, 44)).filled(),
                ))
            }))
            .map_err(|e| chart_err(path, e))?;
        drawings_top
            .label("harness buffers (ESTIMATE)")
            .legend(|(x, y)| {
                Rectangle::new(
                    [(x, y), (x + 16, y + 11)],
                    ShapeStyle::from(RGBColor(44, 160, 44)).filled(),
                )
            });
        any_label = true;
    }

    // Measured min/max whiskers.
    let whiskers: Vec<ErrorBar<f64, f64, ErrorBarOrientV<f64, f64>>> = data
        .points
        .iter()
        .filter_map(|p| {
            if p.measured <= 0.0 || p.measured_min <= 0.0 || p.measured_max <= 0.0 {
                return None;
            }
            Some(ErrorBar::new_vertical(
                p.n as f64,
                p.measured_min,
                p.measured,
                p.measured_max,
                COLOR_WITH_MTL,
                7,
            ))
        })
        .collect();
    if !whiskers.is_empty() {
        chart
            .draw_series(whiskers)
            .map_err(|e| chart_err(path, e))?;
    }

    // Ratio labels and residual annotations above each measured whisker
    let dark = RGBColor(70, 70, 75);
    let ratio_style = TextStyle::from(("sans-serif", 10)).color(&dark);
    let residual_style = TextStyle::from(("sans-serif", 9)).color(&dark);
    let decades_x = (x_hi / x_lo).log10();
    let px_per_decade = (CHART_W as f64 - Y_LABEL_AREA as f64 - 2.0 * MARGIN as f64) / decades_x;
    let labels: Vec<Text<'_, (f64, f64), String>> = data
        .points
        .iter()
        .filter_map(|p| {
            if p.analytical_total <= 0.0 || p.measured <= 0.0 {
                return None;
            }
            let ratio = p.measured / p.analytical_total;
            let text = format!("{ratio:.2}x");
            let half_px = text.chars().count() as f64 * 6.0 / 2.0;
            let shift = half_px / px_per_decade;
            let x = 10f64.powf((p.n as f64).log10() - shift);
            let y = (p.measured_max * 1.9).min(y_hi / 1.2);
            Some(Text::new(text, (x, y), ratio_style.clone()))
        })
        .collect();
    if !labels.is_empty() {
        chart.draw_series(labels).map_err(|e| chart_err(path, e))?;
    }
    let residuals: Vec<Text<'_, (f64, f64), String>> = data
        .points
        .iter()
        .filter_map(|p| {
            if p.analytical_total <= 0.0 || p.measured <= 0.0 {
                return None;
            }
            // Format residual: signed bytes and percent
            let resid = p.residual as i64;
            let pct = p.residual_percent;
            let text = format!("residual: {resid:+} B ({pct:+.1}%)");
            let half_px = text.chars().count() as f64 * 5.4 / 2.0;
            let shift = half_px / px_per_decade;
            let x = 10f64.powf((p.n as f64).log10() - shift);
            let y = ((p.measured_max * 1.9).min(y_hi / 1.2)) + 14.0;
            Some(Text::new(text, (x, y), residual_style.clone()))
        })
        .collect();
    if !residuals.is_empty() {
        chart
            .draw_series(residuals)
            .map_err(|e| chart_err(path, e))?;
    }

    if any_label {
        chart
            .configure_series_labels()
            .position(SeriesLabelPosition::UpperLeft)
            .background_style(WHITE.mix(0.85))
            .border_style(BLACK)
            .label_font(("sans-serif", 11))
            .draw()
            .map_err(|e| chart_err(path, e))?;
    }

    draw_footnotes(&foot, footnotes).map_err(|e| chart_err(path, e))?;
    root.present().map_err(|e| chart_err(path, e))?;
    Ok(())
}
