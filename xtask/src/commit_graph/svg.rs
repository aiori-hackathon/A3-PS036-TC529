//! Rendering of the commit-graph artwork: a wide card, a narrow card and the
//! two compact strips.
//!
//! One layout struct describes the geometry of every variant, so the header,
//! KPI block, staircase, day bars and footer share their rules instead of
//! drifting apart. All positions are in SVG user units; the cards are shown at
//! 1:1, while the narrow variants are shown slightly under their nominal width
//! and the readability floor accounts for that scale.
//!
//! Every string is drawn as outlines, no element points at a file or a URL,
//! and the only animation is CSS inside the document. The final state, with
//! animation disabled, is the complete artwork.

use anyhow::{bail, Context, Result};
use std::fmt::Write as _;
use std::path::Path;

use super::text::{glyph_defs, text_markup, Font, TextBox, Typeface, UsedGlyphs};
use super::{civil_from_days, parse_iso_date, CommitGraph, MilestoneStatus, StyleConfig};

/// Palette of the artwork. Only three inks carry data or state; everything
/// else is neutral, and orange never touches a glyph.
pub(crate) struct Palette {
    background: String,
    border: String,
    ink: String,
    muted: String,
    orange: String,
    teal: String,
    teal_text: String,
}

impl Palette {
    pub(crate) fn load(root: &Path, style: &StyleConfig) -> Result<Self> {
        let mut palette = Self::default();
        let path = root.join("docs/media/brand.toml");
        if let Ok(text) = std::fs::read_to_string(&path) {
            let value: toml::Value = toml::from_str(&text)
                .with_context(|| format!("{} is not valid TOML", path.display()))?;
            if let Some(hex) = hex_at(&value, &["brand", "orange", "hex"]) {
                palette.orange = hex;
            }
        }
        if let Some(accent) = &style.accent {
            palette.orange = accent.clone();
        }
        Ok(palette)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: "#0E1217".to_string(),
            border: "#FFFFFF".to_string(),
            ink: "#E6EDF3".to_string(),
            muted: "#8B98A5".to_string(),
            orange: "#C55E33".to_string(),
            teal: "#5C8E8D".to_string(),
            teal_text: "#7FB5B3".to_string(),
        }
    }
}

fn hex_at(value: &toml::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str().map(str::to_string)
}

fn display_face(style: &StyleConfig) -> Typeface {
    if style.display_font == "JetBrains Mono" {
        Typeface::Mono
    } else {
        Typeface::Display
    }
}

/// Machine-readable numbers carried by the SVG `<desc>`; the documentation
/// check compares these with the JSON document.
pub(crate) fn desc_text(graph: &CommitGraph) -> String {
    let totals = &graph.totals;
    let milestones = graph
        .milestones
        .iter()
        .map(|milestone| {
            format!(
                "{} {} {}",
                milestone.date,
                milestone.label,
                milestone.status.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let authors = graph
        .per_author
        .iter()
        .map(|author| format!("{}={}", author.display, author.commits))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CLIMB commit graph. commits={} refresh_commits={} active_days={} longest_streak={} \
         current_streak={} lines_added={} lines_removed={} files_touched={} as_of={} date={} \
         authors={} first_commit={} last_commit={} busiest_day={} busiest_commits={} \
         streak_start={} streak_end={} Milestones: {milestones} Contributors: {authors}",
        totals.commits,
        totals.refresh_commits,
        totals.active_days,
        totals.longest_streak,
        totals.current_streak,
        totals.lines_added,
        totals.lines_removed,
        totals.files_touched,
        &graph.as_of_sha[..7.min(graph.as_of_sha.len())],
        graph.as_of_date,
        graph.per_author.len(),
        totals.first_commit_date,
        totals.last_commit_date,
        totals.busiest_day.date,
        totals.busiest_day.commits,
        totals.longest_streak_range.start,
        totals.longest_streak_range.end
    )
}

/// The only animation in the document. The staircase draws once, the bars rise
/// with a stagger capped so the whole sequence stays under 1.2 s, the flags
/// fade in after the line, and only the "now" ring loops.
fn style_block() -> &'static str {
    "\
.cg-stair{animation:cg-draw 1.6s cubic-bezier(.3,.8,.3,1) both}\
.cg-bar{transform-box:fill-box;transform-origin:center bottom;animation:cg-rise .4s ease-out both}\
.cg-flag{animation:cg-fade .4s ease-out 1.6s both}\
.cg-now-ring{transform-box:fill-box;transform-origin:center;animation:cg-pulse 3.2s ease-out infinite}\
@keyframes cg-draw{from{stroke-dashoffset:1}to{stroke-dashoffset:0}}\
@keyframes cg-rise{from{transform:scaleY(0)}to{transform:scaleY(1)}}\
@keyframes cg-fade{from{opacity:0}to{opacity:1}}\
@keyframes cg-pulse{from{opacity:.5;transform:scale(1)}to{opacity:0;transform:scale(2.6)}}\
@media (prefers-reduced-motion:reduce){.cg-stair,.cg-bar,.cg-flag,.cg-now-ring{animation:none}}"
}

/// One text run inside a group. A run may carry its own ink when a group mixes
/// a muted label with an inked value.
struct Run {
    text: String,
    x: f64,
    baseline: f64,
    fill: Option<String>,
}

fn run(text: impl Into<String>, x: f64, baseline: f64) -> Run {
    Run {
        text: text.into(),
        x,
        baseline,
        fill: None,
    }
}

fn tinted(text: impl Into<String>, x: f64, baseline: f64, fill: &str) -> Run {
    Run {
        text: text.into(),
        x,
        baseline,
        fill: Some(fill.to_string()),
    }
}

struct Canvas<'a> {
    font: &'a Font,
    body: String,
    used: UsedGlyphs,
}

impl<'a> Canvas<'a> {
    fn new(font: &'a Font) -> Self {
        Self {
            font,
            body: String::new(),
            used: UsedGlyphs::new(),
        }
    }

    // A drawing primitive spells its geometry out: wrapping it in a struct
    // would turn every call site into a constructor invocation.
    #[allow(clippy::too_many_arguments)]
    fn rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: &str,
        stroke: Option<&str>,
        stroke_opacity: f64,
        rx: f64,
    ) {
        let _ = write!(
            self.body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{fill}\"",
            num(x),
            num(y),
            num(w),
            num(h),
            num(rx)
        );
        if let Some(stroke) = stroke {
            let _ = write!(self.body, " stroke=\"{stroke}\" stroke-width=\"1\"");
            if (stroke_opacity - 1.0).abs() > f64::EPSILON {
                let _ = write!(self.body, " stroke-opacity=\"{}\"", num(stroke_opacity));
            }
        }
        self.body.push_str("/>");
    }

    // The geometry and the presentation are separate call-site decisions; a
    // struct here would only rename the same arguments.
    #[allow(clippy::too_many_arguments)]
    fn mark_rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, class: &str, attrs: &str) {
        let _ = write!(
            self.body,
            "<rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
             rx=\"2\" fill=\"{fill}\"{attrs}/>",
            num(x),
            num(y),
            num(w),
            num(h)
        );
    }

    // A line is its two endpoints plus its stroke; keeping them in the call
    // keeps every rule readable at the point it is drawn.
    #[allow(clippy::too_many_arguments)]
    fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        width: f64,
        dash: Option<&str>,
        opacity: f64,
    ) {
        let _ = write!(
            self.body,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{stroke}\" stroke-width=\"{}\"",
            num(x1),
            num(y1),
            num(x2),
            num(y2),
            num(width)
        );
        if let Some(dash) = dash {
            let _ = write!(self.body, " stroke-dasharray=\"{dash}\"");
        }
        if (opacity - 1.0).abs() > f64::EPSILON {
            let _ = write!(self.body, " opacity=\"{}\"", num(opacity));
        }
        self.body.push_str("/>");
    }

    fn path(
        &mut self,
        d: &str,
        fill: Option<&str>,
        stroke: Option<&str>,
        width: f64,
        class: Option<&str>,
        extra: &str,
    ) {
        let _ = write!(self.body, "<path d=\"{d}\"");
        match fill {
            Some(fill) => {
                let _ = write!(self.body, " fill=\"{fill}\"");
            }
            None => self.body.push_str(" fill=\"none\""),
        }
        if let Some(stroke) = stroke {
            let _ = write!(
                self.body,
                " stroke=\"{stroke}\" stroke-width=\"{}\"",
                num(width)
            );
        }
        if let Some(class) = class {
            let _ = write!(self.body, " class=\"{class}\"");
        }
        self.body.push_str(extra);
        self.body.push_str("/>");
    }

    // A marker circle is its centre, radius and stroke; a builder would not
    // make the call sites clearer.
    #[allow(clippy::too_many_arguments)]
    fn mk_circle(
        &mut self,
        cx: f64,
        cy: f64,
        r: f64,
        fill: &str,
        stroke: &str,
        dashed: bool,
        class: &str,
    ) {
        let _ = write!(
            self.body,
            "<circle class=\"{class}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\" \
             stroke=\"{stroke}\" stroke-width=\"1\"",
            num(cx),
            num(cy),
            num(r)
        );
        if dashed {
            self.body.push_str(" stroke-dasharray=\"2 3\"");
        }
        self.body.push_str("/>");
    }

    fn width(&self, face: Typeface, text: &str, size: f64, tracking: f64) -> Result<f64> {
        self.font.width(face, text, size, tracking)
    }

    fn right_x(
        &self,
        face: Typeface,
        text: &str,
        right: f64,
        size: f64,
        tracking: f64,
    ) -> Result<f64> {
        Ok(right - self.width(face, text, size, tracking)?)
    }

    /// Draw one semantic text group. Its bounding box is the union of the
    /// exact ink boxes of its runs, and it is written to the artifact so the
    /// contract tests can recompute spacing without a font.
    fn text_group(
        &mut self,
        name: &str,
        face: Typeface,
        size: f64,
        fill: &str,
        runs: &[Run],
        tracking: f64,
    ) -> Result<TextBox> {
        let mut union: Option<TextBox> = None;
        let mut inner = String::new();
        for run in runs {
            let box_ = self
                .font
                .line_box(face, &run.text, run.x, run.baseline, size, tracking)?;
            union = Some(match union {
                Some(current) => current.union(box_),
                None => box_,
            });
            let ink = run.fill.as_deref().unwrap_or(fill);
            inner.push_str(&text_markup(
                self.font,
                face,
                &run.text,
                run.x,
                run.baseline,
                size,
                ink,
                tracking,
                &mut self.used,
            )?);
        }
        let union = union.context("a text group carries at least one run")?;
        let _ = write!(
            self.body,
            "<g class=\"cg-text-group\" data-name=\"{name}\" data-font=\"{}\" \
             data-size=\"{}\" fill=\"{fill}\" data-bbox=\"{} {} {} {}\">{inner}</g>",
            face.id(),
            num(size),
            num(union.x),
            num(union.y),
            num(union.width),
            num(union.height)
        );
        Ok(union)
    }

    fn finish(
        self,
        width: f64,
        height: f64,
        title: &str,
        desc: &str,
        palette: &Palette,
        area: bool,
    ) -> String {
        let glyphs = glyph_defs(self.font, &self.used);
        let mut out = String::new();
        let gradient = if area {
            format!(
                "<linearGradient id=\"cg-fill\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\
                 <stop offset=\"0\" stop-color=\"{}\" stop-opacity=\"0.22\"/>\
                 <stop offset=\"1\" stop-color=\"{}\" stop-opacity=\"0\"/></linearGradient>",
                palette.orange, palette.orange
            )
        } else {
            String::new()
        };
        let w = num(width);
        let h = num(height);
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" \
             width=\"{w}\" height=\"{h}\" role=\"img\" aria-labelledby=\"cg-title cg-desc\">\
             <title id=\"cg-title\">{}</title><desc id=\"cg-desc\">{}</desc>\
             <style>{}</style><defs>{gradient}{glyphs}</defs>",
            escape_xml(title),
            escape_xml(desc),
            style_block()
        );
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out
    }
}

fn num(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// Round the vertical axis to a readable maximum.
fn nice_max(value: u64) -> u64 {
    if value <= 4 {
        return value.max(1);
    }
    let target = (value as f64 / 4.0).ceil();
    let magnitude = 10f64.powf(target.log10().floor());
    let mut step = magnitude;
    for multiplier in [1.0, 2.0, 2.5, 5.0, 10.0] {
        step = magnitude * multiplier;
        if step >= target {
            break;
        }
    }
    let step = step.max(1.0) as u64;
    (value as f64 / step as f64).ceil() as u64 * step
}

fn month_name(month: i64) -> &'static str {
    const NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    NAMES
        .get((month - 1).max(0) as usize)
        .copied()
        .unwrap_or("")
}

/// `YYYY-MM-DD` as `22 Oct 2026`.
fn long_date(iso: &str) -> Result<String> {
    let (year, month, day) = civil_from_days(parse_iso_date(iso)?);
    Ok(format!("{day} {} {year}", month_name(month)))
}

/// `YYYY-MM-DD` as `12 Oct`.
fn short_date(iso: &str) -> Result<String> {
    let (_, month, day) = civil_from_days(parse_iso_date(iso)?);
    Ok(format!("{day} {}", month_name(month)))
}

/// Monday-based weekday index, 0 = Monday.
fn weekday(day: i64) -> i64 {
    // 1970-01-01 was a Thursday.
    (day + 3).rem_euclid(7)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Wide,
    Narrow,
    Compact,
    CompactNarrow,
}

impl Variant {
    fn size(self) -> (f64, f64) {
        match self {
            Self::Wide => (840.0, 540.0),
            Self::Narrow => (400.0, 640.0),
            Self::Compact => (840.0, 156.0),
            Self::CompactNarrow => (400.0, 200.0),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Wide => "CLIMB commit graph: The Climb",
            Self::Narrow => "CLIMB commit graph: The Climb (narrow)",
            Self::Compact => "CLIMB commit graph: compact strip",
            Self::CompactNarrow => "CLIMB commit graph: compact strip (narrow)",
        }
    }
}

/// Geometry of the four-column (wide) or two-by-two (narrow) KPI block.
struct KpiLayout {
    columns: usize,
    rows: usize,
    origin_x: f64,
    col_width: f64,
    row_pitch: f64,
    caption_size: f64,
    caption_baseline: f64,
    value_size: f64,
    value_baseline: f64,
    detail_size: f64,
    detail_baseline: f64,
    rules: bool,
}

/// Geometry of a cumulative staircase.
struct ChartLayout {
    left: f64,
    top: f64,
    data_right: f64,
    right: f64,
    bottom: f64,
    y_labels: bool,
    now_label: bool,
    axis_labels: bool,
    axis_baseline: f64,
    labels: bool,
}

/// Geometry of the day bars.
struct BarsLayout {
    baseline: f64,
    max_height: f64,
    last_days: Option<usize>,
}

/// Geometry of the footer strip.
struct FooterLayout {
    baseline: f64,
    size: f64,
    source_size: f64,
    /// Put the source on its own line under the contributors, for the narrow
    /// card where both cannot share a row.
    stacked: bool,
}

/// Geometry of a compact strip.
struct CompactLayout {
    numeral_size: f64,
    numeral_baseline: f64,
    caption_size: f64,
    caption_baseline: f64,
    stats_size: f64,
    stats_baselines: [f64; 3],
    spark: (f64, f64, f64, f64),
}

struct Layout {
    variant: Variant,
    width: f64,
    height: f64,
    margin: f64,
    eyebrow: &'static str,
    eyebrow_size: f64,
    eyebrow_baseline: f64,
    title_size: f64,
    title_baseline: f64,
    stamp_size: f64,
    stamp_baseline: f64,
    stamp_gap: f64,
    kpi: Option<KpiLayout>,
    chart: Option<ChartLayout>,
    bars: Option<BarsLayout>,
    footer: Option<FooterLayout>,
    compact: Option<CompactLayout>,
}

impl Layout {
    fn wide() -> Self {
        let (width, height) = Variant::Wide.size();
        Self {
            variant: Variant::Wide,
            width,
            height,
            margin: 32.0,
            eyebrow: "COMMIT HISTORY / PUBLIC REPOSITORY",
            eyebrow_size: 12.0,
            eyebrow_baseline: 48.0,
            title_size: 44.0,
            title_baseline: 96.0,
            stamp_size: 13.0,
            stamp_baseline: 72.0,
            stamp_gap: 24.0,
            kpi: Some(KpiLayout {
                columns: 4,
                rows: 1,
                origin_x: 32.0,
                col_width: 192.0,
                row_pitch: 0.0,
                caption_size: 12.0,
                caption_baseline: 120.0,
                value_size: 40.0,
                value_baseline: 160.0,
                detail_size: 16.0,
                detail_baseline: 184.0,
                rules: true,
            }),
            chart: Some(ChartLayout {
                left: 96.0,
                top: 208.0,
                data_right: 592.0,
                right: 808.0,
                bottom: 344.0,
                y_labels: true,
                now_label: true,
                axis_labels: true,
                axis_baseline: 480.0,
                labels: true,
            }),
            bars: Some(BarsLayout {
                baseline: 464.0,
                max_height: 48.0,
                last_days: None,
            }),
            footer: Some(FooterLayout {
                baseline: 512.0,
                size: 13.0,
                source_size: 12.0,
                stacked: false,
            }),
            compact: None,
        }
    }

    fn narrow() -> Self {
        let (width, height) = Variant::Narrow.size();
        Self {
            variant: Variant::Narrow,
            width,
            height,
            margin: 24.0,
            eyebrow: "PUBLIC HISTORY",
            eyebrow_size: 12.0,
            eyebrow_baseline: 40.0,
            title_size: 36.0,
            title_baseline: 80.0,
            stamp_size: 12.0,
            stamp_baseline: 40.0,
            stamp_gap: 16.0,
            kpi: Some(KpiLayout {
                columns: 2,
                rows: 2,
                origin_x: 24.0,
                col_width: 172.0,
                row_pitch: 64.0,
                caption_size: 12.0,
                caption_baseline: 112.0,
                value_size: 32.0,
                value_baseline: 144.0,
                detail_size: 14.0,
                detail_baseline: 168.0,
                rules: false,
            }),
            chart: Some(ChartLayout {
                left: 48.0,
                top: 248.0,
                data_right: 376.0,
                right: 376.0,
                bottom: 336.0,
                y_labels: false,
                now_label: false,
                axis_labels: false,
                axis_baseline: 0.0,
                labels: false,
            }),
            bars: Some(BarsLayout {
                baseline: 512.0,
                max_height: 32.0,
                last_days: Some(35),
            }),
            footer: Some(FooterLayout {
                baseline: 568.0,
                size: 13.0,
                source_size: 12.0,
                stacked: true,
            }),
            compact: None,
        }
    }

    fn compact() -> Self {
        let (width, height) = Variant::Compact.size();
        Self {
            variant: Variant::Compact,
            width,
            height,
            margin: 32.0,
            eyebrow: "",
            eyebrow_size: 0.0,
            eyebrow_baseline: 0.0,
            title_size: 0.0,
            title_baseline: 0.0,
            stamp_size: 12.0,
            stamp_baseline: 140.0,
            stamp_gap: 0.0,
            kpi: None,
            chart: None,
            bars: None,
            footer: None,
            compact: Some(CompactLayout {
                numeral_size: 72.0,
                numeral_baseline: 96.0,
                caption_size: 13.0,
                caption_baseline: 120.0,
                stats_size: 15.0,
                stats_baselines: [64.0, 88.0, 112.0],
                spark: (300.0, 620.0, 48.0, 104.0),
            }),
        }
    }

    fn compact_narrow() -> Self {
        let (width, height) = Variant::CompactNarrow.size();
        Self {
            variant: Variant::CompactNarrow,
            width,
            height,
            margin: 24.0,
            eyebrow: "",
            eyebrow_size: 0.0,
            eyebrow_baseline: 0.0,
            title_size: 0.0,
            title_baseline: 0.0,
            stamp_size: 12.0,
            stamp_baseline: 186.0,
            stamp_gap: 0.0,
            kpi: None,
            chart: None,
            bars: None,
            footer: None,
            compact: Some(CompactLayout {
                numeral_size: 56.0,
                numeral_baseline: 80.0,
                caption_size: 12.0,
                caption_baseline: 104.0,
                stats_size: 14.0,
                stats_baselines: [48.0, 72.0, 96.0],
                spark: (24.0, 376.0, 120.0, 168.0),
            }),
        }
    }
}

/// The cumulative staircase geometry, built once and shared by the line, the
/// bars and the flags.
struct ChartGeom {
    left: f64,
    top: f64,
    data_right: f64,
    right: f64,
    bottom: f64,
    max: u64,
    days: usize,
    column: f64,
    cumulative: Vec<u64>,
}

impl ChartGeom {
    fn y(&self, value: u64) -> f64 {
        self.bottom - (value as f64 / self.max as f64) * (self.bottom - self.top)
    }

    fn centre(&self, index: usize) -> f64 {
        self.left + (index as f64 + 0.5) * self.column
    }

    fn cumulative_through(&self, index: usize) -> u64 {
        if self.cumulative.is_empty() {
            0
        } else {
            self.cumulative[index.min(self.cumulative.len() - 1)]
        }
    }
}

fn chart_geometry(graph: &CommitGraph, chart: &ChartLayout) -> Result<ChartGeom> {
    let days = graph.per_day.len().max(1);
    let column = (chart.data_right - chart.left) / days as f64;
    let mut cumulative = Vec::with_capacity(days);
    let mut total = 0u64;
    for day in &graph.per_day {
        total += day.commits;
        cumulative.push(total);
    }
    Ok(ChartGeom {
        left: chart.left,
        top: chart.top,
        data_right: chart.data_right,
        right: chart.right,
        bottom: chart.bottom,
        max: nice_max(graph.totals.commits),
        days,
        column,
        cumulative,
    })
}

fn draw_chart_labels(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    chart: &ChartLayout,
    geom: &ChartGeom,
    occupied: &mut Vec<TextBox>,
) -> Result<()> {
    if chart.y_labels {
        let mut runs = Vec::new();
        for step in 0..=2 {
            let value = geom.max / 2 * step as u64;
            let label = thousands(value);
            let x = canvas.right_x(Typeface::Mono, &label, geom.left - 12.0, 12.0, 0.0)?;
            runs.push(run(label, x, geom.y(value) + 4.0));
        }
        occupied.push(canvas.text_group(
            "y-ticks",
            Typeface::Mono,
            12.0,
            &palette.muted,
            &runs,
            0.0,
        )?);
    }

    if chart.now_label {
        let text = format!("now - {} commits", thousands(graph.totals.commits));
        let x = canvas.right_x(Typeface::Mono, &text, geom.data_right - 8.0, 14.0, 0.0)?;
        let baseline = geom.y(graph.totals.commits) + 18.0;
        occupied.push(canvas.text_group(
            "now-label",
            Typeface::Mono,
            14.0,
            &palette.ink,
            &[run(text, x, baseline)],
            0.0,
        )?);
    }

    if chart.axis_labels {
        let start = parse_iso_date(&graph.range.start)?;
        let mut runs = Vec::new();
        let mut last_right = f64::NEG_INFINITY;
        for (index, entry) in graph.per_day.iter().enumerate() {
            let day = start + index as i64;
            if weekday(day) != 0 {
                continue;
            }
            let label = short_date(&entry.date)?;
            let width = canvas.width(Typeface::Mono, &label, 12.0, 0.0)?;
            let centre = geom.centre(index);
            let x = (centre - width / 2.0).clamp(geom.left, geom.data_right - width);
            if x < last_right + 8.0 {
                continue;
            }
            last_right = x + width;
            runs.push(run(label, x, chart.axis_baseline));
        }
        let note = "runway not to scale";
        let note_x = canvas.right_x(Typeface::Mono, note, geom.right, 12.0, 0.0)?;
        runs.push(run(note, note_x, chart.axis_baseline));
        occupied.push(canvas.text_group(
            "axis-labels",
            Typeface::Mono,
            12.0,
            &palette.muted,
            &runs,
            0.0,
        )?);
    }
    Ok(())
}

fn draw_staircase(canvas: &mut Canvas, palette: &Palette, geom: &ChartGeom, area: bool) {
    let mut path = format!("M{} {}", num(geom.left), num(geom.bottom));
    let mut previous = 0u64;
    // Walk the cumulative array so the geometry and the data cannot disagree.
    for (index, value) in geom.cumulative.iter().enumerate() {
        let x0 = geom.left + index as f64 * geom.column;
        let x1 = x0 + geom.column;
        if *value != previous {
            let _ = write!(
                path,
                "L{} {}L{} {}",
                num(x0),
                num(geom.y(previous)),
                num(x0),
                num(geom.y(*value))
            );
        }
        let _ = write!(path, "L{} {}", num(x1), num(geom.y(*value)));
        previous = *value;
    }
    if area {
        let fill_path = format!(
            "{path}L{} {}L{} {}Z",
            num(geom.data_right),
            num(geom.bottom),
            num(geom.left),
            num(geom.bottom)
        );
        canvas.path(&fill_path, Some("url(#cg-fill)"), None, 0.0, None, "");
    }
    let _ = write!(
        canvas.body,
        "<path d=\"{path}\" fill=\"none\" stroke=\"{}\" stroke-width=\"3\" \
         class=\"cg-stair dm-line\" pathLength=\"1\" stroke-dasharray=\"1\" stroke-dashoffset=\"0\" \
         data-plot=\"{} {} {} {} {} {}\"/>",
        palette.orange,
        num(geom.left),
        num(geom.top),
        num(geom.data_right),
        num(geom.bottom),
        geom.max,
        geom.days
    );
    let end_y = geom.y(*geom.cumulative.last().unwrap_or(&0));
    canvas.mk_circle(
        geom.data_right,
        end_y,
        5.0,
        "none",
        &palette.teal,
        false,
        "cg-now-ring dm dm-now",
    );
    canvas.mk_circle(
        geom.data_right,
        end_y,
        3.5,
        &palette.orange,
        &palette.orange,
        false,
        "dm dm-now-dot",
    );
}

fn draw_bars(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    bars: &BarsLayout,
    geom: &ChartGeom,
) {
    let days: Vec<(usize, &super::DayEntry)> = match bars.last_days {
        Some(limit) => graph
            .per_day
            .iter()
            .enumerate()
            .skip(graph.per_day.len().saturating_sub(limit))
            .collect(),
        None => graph.per_day.iter().enumerate().collect(),
    };
    let count = days.len().max(1) as f64;
    let column_width = (geom.data_right - geom.left) / count;
    let max_commits = days
        .iter()
        .map(|(_, day)| day.commits)
        .max()
        .unwrap_or(0)
        .max(1);
    let bar_width = (column_width * 0.55).clamp(4.0, 10.0);
    for (visible, (index, day)) in days.iter().enumerate() {
        let centre = if bars.last_days.is_some() {
            geom.left + (visible as f64 + 0.5) * column_width
        } else {
            geom.centre(*index)
        };
        if day.commits == 0 {
            canvas.mk_circle(
                centre,
                bars.baseline - 1.0,
                1.0,
                &palette.muted,
                &palette.muted,
                false,
                "dm dm-quiet",
            );
            continue;
        }
        let height = if day.commits == 0 {
            0.0
        } else {
            (day.commits as f64 / max_commits as f64 * bars.max_height).max(3.0)
        };
        let delay = (visible * 30).min(800);
        let attrs = format!(
            " data-date=\"{}\" data-commits=\"{}\" style=\"animation-delay:{delay}ms\"",
            day.date, day.commits
        );
        canvas.mark_rect(
            centre - bar_width / 2.0,
            bars.baseline - height,
            bar_width,
            height,
            &palette.orange,
            "dm dm-bar cg-bar",
            &attrs,
        );
    }
}

fn draw_annotations(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    chart: &ChartLayout,
    geom: &ChartGeom,
    occupied: &mut Vec<TextBox>,
) -> Result<()> {
    let start = parse_iso_date(&graph.range.start)?;
    let busiest_index = graph
        .per_day
        .iter()
        .enumerate()
        .fold((0usize, 0u64), |best, (index, day)| {
            if day.commits > best.1 {
                (index, day.commits)
            } else {
                best
            }
        })
        .0;
    let busiest = format!(
        "busiest day - {} commits - {}",
        graph.totals.busiest_day.commits,
        short_date(&graph.totals.busiest_day.date)?
    );
    let streak = format!("{}-day streak", graph.totals.longest_streak);
    let baseline = 400.0;
    let streak_x = canvas.right_x(Typeface::Mono, &streak, geom.data_right, 13.0, 0.0)?;
    let runs = vec![
        run(busiest, geom.left, baseline),
        tinted(streak, streak_x, baseline, &palette.teal_text),
    ];
    occupied.push(canvas.text_group(
        "annotations",
        Typeface::Mono,
        13.0,
        &palette.ink,
        &runs,
        0.0,
    )?);

    // Leader from the busiest label down to its bar.
    let busiest_x = geom.centre(busiest_index);
    canvas.line(
        geom.left + 242.0,
        baseline + 4.0,
        busiest_x,
        chart.bottom + 0.0,
        &palette.muted,
        1.0,
        None,
        0.45,
    );

    // Teal bracket over the longest streak, with small end ticks.
    let streak_start = parse_iso_date(&graph.totals.longest_streak_range.start)?;
    let streak_end = parse_iso_date(&graph.totals.longest_streak_range.end)?;
    let start_index = (streak_start - start).max(0) as usize;
    let end_index = (streak_end - start).max(0) as usize;
    if start_index < geom.days {
        let x0 = geom.centre(start_index);
        let x1 = geom
            .centre(end_index.min(geom.days - 1))
            .min(geom.data_right);
        let y = 408.0;
        let d = format!(
            "M{} {}L{} {}L{} {}M{} {}L{} {}",
            num(x0),
            num(y + 4.0),
            num(x0),
            num(y),
            num(x1),
            num(y),
            num(x1),
            num(y),
            num(x1),
            num(y + 4.0)
        );
        canvas.path(&d, None, Some(&palette.teal), 1.0, None, "");
    }
    Ok(())
}

fn flag_text(milestone: &super::GraphMilestone) -> Result<String> {
    Ok(format!(
        "{} - {}",
        milestone.short,
        short_date(&milestone.date)?
    ))
}

/// Draw the flags and place their labels.
///
/// Every candidate position is measured from the glyph outlines and rejected
/// when it comes within eight pixels of a box already placed. If no candidate
/// fits, generation fails instead of drawing an overlap.
fn draw_flags(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    chart: &ChartLayout,
    geom: &ChartGeom,
    style: &StyleConfig,
    occupied: &mut Vec<TextBox>,
) -> Result<()> {
    if !style.show_flags {
        return Ok(());
    }
    let start = parse_iso_date(&graph.range.start)?;
    let end = parse_iso_date(&graph.range.end)?;
    let last_upcoming = graph
        .milestones
        .iter()
        .filter(|milestone| milestone.status == MilestoneStatus::Upcoming)
        .filter_map(|milestone| parse_iso_date(&milestone.date).ok())
        .max()
        .unwrap_or(end)
        .max(end);

    // The narrow card draws one text group for all its milestone numbers, so
    // the group budget stays below the narrow cap.
    let mut narrow_runs = Vec::new();
    let mut narrow_boxes: Vec<TextBox> = Vec::new();

    for (index, milestone) in graph.milestones.iter().enumerate() {
        let date = parse_iso_date(&milestone.date)?;
        let (base_x, base_y) = match milestone.status {
            MilestoneStatus::Done => {
                let day_index = (date - start).max(0) as usize;
                let x = geom.centre(day_index.min(geom.days - 1));
                (x, geom.y(geom.cumulative_through(day_index)))
            }
            MilestoneStatus::Upcoming => {
                let span = (last_upcoming - end).max(1) as f64;
                let fraction = ((date - end).max(0) as f64 / span).clamp(0.0, 1.0);
                // A chart with no runway still needs its future flags at the
                // right edge in date order; keep a minimum lane so two dates
                // cannot swap places or collapse onto one point.
                let runway_right = geom.right - 6.0;
                let runway_left = (geom.data_right + 24.0).min(runway_right - 36.0);
                (
                    runway_left + fraction * (runway_right - runway_left),
                    geom.bottom,
                )
            }
        };
        let done = milestone.status == MilestoneStatus::Done;
        let colour = if done {
            palette.teal.clone()
        } else {
            palette.muted.clone()
        };

        // The narrow card turns each milestone into a numbered dot on the line
        // and lists the numbers underneath; the wide card has room for the
        // full flag and its label.
        if !chart.labels {
            let _ = write!(
                canvas.body,
                "<circle class=\"dm dm-milestone\" data-date=\"{}\" data-status=\"{}\" \
                 cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\" stroke=\"{colour}\" stroke-width=\"1\"{} />",
                milestone.date,
                milestone.status.as_str(),
                num(base_x),
                num(base_y),
                if done { colour.as_str() } else { "none" },
                if done { "" } else { " stroke-dasharray=\"2 3\"" }
            );
            let number = (index + 1).to_string();
            let width = canvas.width(Typeface::Mono, &number, 12.0, 0.0)?;
            let x = (base_x - width / 2.0).clamp(24.0, geom.right - width - 2.0);
            let mut placed = None;
            for baseline in [base_y - 7.0, base_y + 18.0, base_y - 24.0, base_y + 32.0] {
                let box_ = canvas
                    .font
                    .line_box(Typeface::Mono, &number, x, baseline, 12.0, 0.0)?;
                if box_.y < 190.0 || box_.bottom() > 348.0 {
                    continue;
                }
                // These are single digits that label adjacent points; they may
                // sit close, so the sweep only has to keep them from touching.
                if occupied
                    .iter()
                    .chain(narrow_boxes.iter())
                    .any(|other| box_.intersects(other) || box_.gap(other) < 2.0)
                {
                    continue;
                }
                placed = Some((baseline, box_));
                break;
            }
            let Some((baseline, box_)) = placed else {
                bail!(
                    "commit-graph milestone number `{number}` cannot be placed on the narrow \
                     card without overlapping another label"
                );
            };
            narrow_runs.push(tinted(
                number,
                x,
                baseline,
                if done { &palette.ink } else { &palette.muted },
            ));
            narrow_boxes.push(box_);
            continue;
        }

        let pole_top = base_y
            - if milestone.status == MilestoneStatus::Done {
                18.0
            } else {
                22.0
            };
        let mut shapes = String::new();
        let _ = write!(
            shapes,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{colour}\" \
             stroke-width=\"1\"{} />",
            num(base_x),
            num(base_y),
            num(base_x),
            num(pole_top),
            if done {
                String::new()
            } else {
                " stroke-dasharray=\"3 3\"".to_string()
            }
        );
        let pennant = format!(
            "M{} {}L{} {}L{} {}Z",
            num(base_x),
            num(pole_top),
            num(base_x + 12.0),
            num(pole_top + 4.0),
            num(base_x),
            num(pole_top + 8.0)
        );
        let _ = write!(
            shapes,
            "<path d=\"{pennant}\" fill=\"{}\" stroke=\"{colour}\" stroke-width=\"1\"{} />",
            if done {
                colour.clone()
            } else {
                "none".to_string()
            },
            if done {
                String::new()
            } else {
                " stroke-dasharray=\"3 3\"".to_string()
            }
        );
        let _ = write!(
            shapes,
            "<circle class=\"dm dm-milestone\" data-date=\"{}\" data-status=\"{}\" \
             cx=\"{}\" cy=\"{}\" r=\"2.5\" fill=\"{}\" stroke=\"{colour}\" stroke-width=\"1\"/>",
            milestone.date,
            milestone.status.as_str(),
            num(base_x),
            num(base_y),
            if done {
                colour.clone()
            } else {
                "none".to_string()
            }
        );
        let _ = write!(canvas.body, "<g class=\"cg-flag\">{shapes}</g>");

        let text = flag_text(milestone)?;
        let size = 13.0;
        let width = canvas.width(Typeface::Mono, &text, size, 0.0)?;
        let candidates = [
            pole_top - 8.0,
            pole_top - 26.0,
            pole_top - 44.0,
            pole_top + 18.0,
        ];
        let mut placed: Option<(f64, f64, TextBox)> = None;
        'outer: for baseline in candidates {
            let anchors = [base_x - width / 2.0, base_x + 10.0, base_x - 10.0 - width];
            for anchor in anchors {
                let x = anchor.clamp(canvas_margin(chart, geom) + 2.0, geom.right - width - 2.0);
                let box_ = canvas
                    .font
                    .line_box(Typeface::Mono, &text, x, baseline, size, 0.0)?;
                if box_.x < canvas_margin(chart, geom) - 0.01 || box_.right() > geom.right + 0.01 {
                    continue;
                }
                if occupied
                    .iter()
                    .any(|other| box_.intersects(other) || box_.gap(other) < 8.0)
                {
                    continue;
                }
                placed = Some((x, baseline, box_));
                break 'outer;
            }
        }
        let Some((x, baseline, box_)) = placed else {
            bail!(
                "commit-graph milestone label `{text}` cannot be placed without overlapping \
                 another label; move a milestone, shorten its `short` label, or widen the card"
            );
        };
        let fill = if done { &palette.ink } else { &palette.muted };
        canvas.text_group(
            &format!("milestone-{}", index + 1),
            Typeface::Mono,
            size,
            fill,
            &[run(text, x, baseline)],
            0.0,
        )?;
        occupied.push(box_);
    }
    if !narrow_runs.is_empty() {
        canvas.text_group(
            "milestone-numbers",
            Typeface::Mono,
            12.0,
            &palette.ink,
            &narrow_runs,
            0.0,
        )?;
        occupied.extend(narrow_boxes);
    }
    Ok(())
}

/// The left content edge used by the flag placer. The chart starts at the y
/// gutter, but a flag label may use the gutter when the staircase is low.
fn canvas_margin(chart: &ChartLayout, geom: &ChartGeom) -> f64 {
    if chart.left < geom.right {
        // The card margin is the gutter's left edge minus the y-label width.
        (chart.left - 12.0).max(24.0)
    } else {
        24.0
    }
}

fn draw_kpi(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    kpi: &KpiLayout,
    display: Typeface,
) -> Result<()> {
    let captions = ["COMMITS", "ACTIVE DAYS", "LONGEST STREAK", "LINES CHANGED"];
    let values = [
        thousands(graph.totals.commits),
        thousands(graph.totals.active_days),
        thousands(graph.totals.longest_streak),
        format!("+{}", thousands(graph.totals.lines_added)),
    ];
    let mut caption_runs = Vec::new();
    let mut value_runs = Vec::new();
    let mut detail_runs = Vec::new();
    for index in 0..4 {
        let column = index % kpi.columns;
        let row = index / kpi.columns;
        if row >= kpi.rows {
            break;
        }
        let x = kpi.origin_x + column as f64 * (kpi.col_width + 8.0);
        let offset = row as f64 * kpi.row_pitch;
        caption_runs.push(run(captions[index], x, kpi.caption_baseline + offset));
        value_runs.push(run(values[index].clone(), x, kpi.value_baseline + offset));
        if index == 2 {
            detail_runs.push(run("days", x, kpi.detail_baseline + offset));
        }
        if index == 3 {
            detail_runs.push(run(
                format!("-{}", thousands(graph.totals.lines_removed)),
                x,
                kpi.detail_baseline + offset,
            ));
        }
    }
    canvas.text_group(
        "kpi-captions",
        Typeface::Mono,
        kpi.caption_size,
        &palette.muted,
        &caption_runs,
        0.08,
    )?;
    canvas.text_group(
        "kpi-values",
        display,
        kpi.value_size,
        &palette.ink,
        &value_runs,
        0.0,
    )?;
    if !detail_runs.is_empty() {
        canvas.text_group(
            "kpi-detail",
            display,
            kpi.detail_size,
            &palette.ink,
            &detail_runs,
            0.0,
        )?;
    }
    if kpi.rules {
        for column in 1..kpi.columns {
            let x = kpi.origin_x + column as f64 * (kpi.col_width + 8.0) - 12.0;
            canvas.line(
                x,
                kpi.caption_baseline - 8.0,
                x,
                kpi.detail_baseline + 8.0,
                &palette.border,
                1.0,
                None,
                0.10,
            );
        }
    }
    Ok(())
}

fn draw_footer(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    layout: &Layout,
    footer: &FooterLayout,
) -> Result<()> {
    let width = layout.width;
    let margin = layout.margin;
    let full_source = format!("from the public history of {}", repository_slug(graph));
    let short_source = format!("public history of {}", repository_name(graph));
    let source_text = if footer.stacked {
        short_source.as_str()
    } else {
        full_source.as_str()
    };
    let source_x = if footer.stacked {
        margin
    } else {
        canvas.right_x(
            Typeface::Mono,
            source_text,
            width - margin,
            footer.source_size,
            0.0,
        )?
    };

    // Contributors on the left. The label is muted, the names are ink; the
    // group keeps one box so the spacing rules see the whole row.
    let label = "Contributors";
    let label_width = canvas.width(Typeface::Mono, label, footer.size, 0.0)?;
    let names_x = margin + label_width + 16.0;
    let authors: Vec<&super::AuthorEntry> = graph
        .per_author
        .iter()
        .filter(|author| author.commits > 0)
        .collect();
    let row_right = if footer.stacked {
        width - margin
    } else {
        source_x - 16.0
    };
    let mut shown = authors.len().min(4);
    let mut runs = vec![tinted(label, margin, footer.baseline, &palette.muted)];
    loop {
        let mut parts: Vec<String> = authors
            .iter()
            .take(shown)
            .map(|author| format!("{} - {}", author.display, thousands(author.commits)))
            .collect();
        if authors.len() > shown {
            parts.push(format!("+{} more", authors.len() - shown));
        }
        let mut x = names_x;
        let mut candidate = Vec::new();
        let mut widest = names_x;
        for part in parts {
            let part_width = canvas.width(Typeface::Mono, &part, footer.size, 0.0)?;
            candidate.push(run(part, x, footer.baseline));
            x += part_width + 18.0;
            widest = widest.max(x - 18.0);
        }
        if widest <= row_right || shown == 0 {
            runs.extend(candidate);
            break;
        }
        shown -= 1;
    }
    canvas.text_group(
        "contributors",
        Typeface::Mono,
        footer.size,
        &palette.ink,
        &runs,
        0.0,
    )?;

    // The stacked bar is only meaningful with more than one contributor, and
    // only the wide card has room for it beside the source line.
    if authors.len() > 1 && !footer.stacked {
        let total: u64 = authors.iter().map(|author| author.commits).sum();
        let mut cursor = margin;
        let bar_width = width - 2.0 * margin;
        for (index, author) in authors.iter().enumerate() {
            let segment = bar_width * author.commits as f64 / total.max(1) as f64;
            let colour = if index % 2 == 0 {
                &palette.orange
            } else {
                &palette.teal
            };
            canvas.mark_rect(
                cursor,
                492.0,
                segment.max(2.0),
                4.0,
                colour,
                "dm dm-bar",
                "",
            );
            cursor += segment;
        }
    }

    let mut source_runs = Vec::new();
    let refresh = graph.totals.refresh_commits;
    if footer.stacked {
        source_runs.push(run(
            source_text,
            margin,
            footer.baseline + footer.source_size + 5.0,
        ));
        if refresh > 0 {
            let note = format!("includes {} graph refreshes", refresh);
            source_runs.push(run(
                note,
                margin,
                footer.baseline + 2.0 * (footer.source_size + 5.0),
            ));
        }
    } else if refresh > 0 {
        source_runs.push(run(
            source_text,
            source_x,
            footer.baseline - footer.source_size - 4.0,
        ));
        let note = format!("includes {} graph refreshes", refresh);
        let x = canvas.right_x(
            Typeface::Mono,
            &note,
            width - margin,
            footer.source_size,
            0.0,
        )?;
        source_runs.push(run(note, x, footer.baseline));
    } else {
        source_runs.push(run(source_text, source_x, footer.baseline));
    }
    canvas.text_group(
        "footer-source",
        Typeface::Mono,
        footer.source_size,
        &palette.muted,
        &source_runs,
        0.0,
    )?;
    Ok(())
}

/// The repository the artwork describes. The data document does not carry the
/// public URL, and the picture must not either, so the slug is a constant that
/// names the same repository as the configuration.
fn repository_slug(_graph: &CommitGraph) -> &'static str {
    "aiori-hackathon/A3-PS036-TC529"
}

/// The repository name alone, for the narrow footer where the full slug does
/// not fit on one line.
fn repository_name(_graph: &CommitGraph) -> &'static str {
    "A3-PS036-TC529"
}

fn draw_compact(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    layout: &Layout,
    compact: &CompactLayout,
    display: Typeface,
) -> Result<()> {
    let margin = layout.margin;
    let total = thousands(graph.totals.commits);
    canvas.text_group(
        "commit-total",
        display,
        compact.numeral_size,
        &palette.ink,
        &[run(total, margin, compact.numeral_baseline)],
        0.0,
    )?;
    canvas.text_group(
        "commit-caption",
        Typeface::Mono,
        compact.caption_size,
        &palette.muted,
        &[run(
            "commits, public history",
            margin,
            compact.caption_baseline,
        )],
        0.04,
    )?;

    let stats = [
        format!("{} active days", graph.totals.active_days),
        format!("{}-day streak", graph.totals.longest_streak),
        format!(
            "last commit {}",
            short_date(&graph.totals.last_commit_date)?
        ),
    ];
    let mut runs = Vec::new();
    for (index, text) in stats.into_iter().enumerate() {
        let x = canvas.right_x(
            Typeface::Mono,
            &text,
            layout.width - margin,
            compact.stats_size,
            0.0,
        )?;
        runs.push(run(text, x, compact.stats_baselines[index]));
    }
    canvas.text_group(
        "stats",
        Typeface::Mono,
        compact.stats_size,
        &palette.ink,
        &runs,
        0.0,
    )?;

    // Sparkline: the same staircase, no fills, no labels.
    let (left, right, top, bottom) = compact.spark;
    let days = graph.per_day.len().max(1);
    let column = (right - left) / days as f64;
    let max = nice_max(graph.totals.commits);
    let mut path = format!("M{} {}", num(left), num(bottom));
    let mut previous = 0u64;
    for (index, day) in graph.per_day.iter().enumerate() {
        let value = previous + day.commits;
        let x0 = left + index as f64 * column;
        let x1 = x0 + column;
        let y = |value: u64| bottom - (value as f64 / max as f64) * (bottom - top);
        if value != previous {
            let _ = write!(
                path,
                "L{} {}L{} {}",
                num(x0),
                num(y(previous)),
                num(x0),
                num(y(value))
            );
        }
        let _ = write!(path, "L{} {}", num(x1), num(y(value)));
        previous = value;
    }
    let _ = write!(
        canvas.body,
        "<path d=\"{path}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\" \
         class=\"cg-stair\" pathLength=\"1\" stroke-dasharray=\"1\" stroke-dashoffset=\"0\"/>",
        palette.orange
    );
    canvas.mk_circle(
        right,
        bottom - (graph.totals.commits as f64 / max as f64) * (bottom - top),
        3.5,
        &palette.orange,
        &palette.orange,
        false,
        "dm dm-now-dot",
    );

    let sha = &graph.as_of_sha[..7.min(graph.as_of_sha.len())];
    let stamp = format!("as of {sha}");
    let x = canvas.right_x(
        Typeface::Mono,
        &stamp,
        layout.width - margin,
        layout.stamp_size,
        0.0,
    )?;
    canvas.text_group(
        "stamp",
        Typeface::Mono,
        layout.stamp_size,
        &palette.muted,
        &[run(stamp, x, layout.stamp_baseline)],
        0.0,
    )?;
    Ok(())
}

fn render(
    graph: &CommitGraph,
    palette: &Palette,
    style: &StyleConfig,
    layout: &Layout,
) -> Result<String> {
    let font = Font::load()?;
    let mut canvas = Canvas::new(&font);
    let display = display_face(style);
    let mut occupied: Vec<TextBox> = Vec::new();

    canvas.rect(
        0.5,
        0.5,
        layout.width - 1.0,
        layout.height - 1.0,
        &palette.background,
        Some(&palette.border),
        0.08,
        14.0,
    );

    if let Some(compact) = &layout.compact {
        draw_compact(&mut canvas, graph, palette, layout, compact, display)?;
        return Ok(canvas.finish(
            layout.width,
            layout.height,
            layout.variant.title(),
            &desc_text(graph),
            palette,
            false,
        ));
    }

    // Header: eyebrow, title and the two-line stamp.
    occupied.push(canvas.text_group(
        "eyebrow",
        Typeface::Mono,
        layout.eyebrow_size,
        &palette.muted,
        &[run(layout.eyebrow, layout.margin, layout.eyebrow_baseline)],
        0.08,
    )?);
    occupied.push(canvas.text_group(
        "title",
        display,
        layout.title_size,
        &palette.ink,
        &[run(
            style.title.clone(),
            layout.margin,
            layout.title_baseline,
        )],
        0.0,
    )?);
    let sha = &graph.as_of_sha[..7.min(graph.as_of_sha.len())];
    let stamp = format!("as of {sha}");
    let date = long_date(&graph.as_of_date)?;
    let stamp_x = canvas.right_x(
        Typeface::Mono,
        &stamp,
        layout.width - layout.margin,
        layout.stamp_size,
        0.0,
    )?;
    let date_x = canvas.right_x(
        Typeface::Mono,
        &date,
        layout.width - layout.margin,
        layout.stamp_size,
        0.0,
    )?;
    occupied.push(canvas.text_group(
        "stamp",
        Typeface::Mono,
        layout.stamp_size,
        &palette.muted,
        &[
            run(stamp, stamp_x, layout.stamp_baseline),
            run(date, date_x, layout.stamp_baseline + layout.stamp_gap),
        ],
        0.0,
    )?);

    if let Some(kpi) = &layout.kpi {
        draw_kpi(&mut canvas, graph, palette, kpi, display)?;
    }

    if let (Some(chart), Some(bars)) = (&layout.chart, &layout.bars) {
        let geom = chart_geometry(graph, chart)?;
        if chart.y_labels {
            for step in 0..=2 {
                let value = geom.max / 2 * step as u64;
                let y = geom.y(value);
                canvas.line(
                    geom.left,
                    y,
                    geom.right,
                    y,
                    &palette.border,
                    1.0,
                    if step == 0 { None } else { Some("1 4") },
                    if step == 0 { 0.18 } else { 0.08 },
                );
            }
        }
        draw_chart_labels(&mut canvas, graph, palette, chart, &geom, &mut occupied)?;
        draw_staircase(&mut canvas, palette, &geom, true);
        if style.show_bars {
            draw_bars(&mut canvas, graph, palette, bars, &geom);
        }
        if chart.axis_labels {
            // The axis break mark at the boundary of the two time regions.
            let x = geom.data_right;
            canvas.path(
                &format!(
                    "M{} {}L{} {}M{} {}L{} {}",
                    num(x - 3.0),
                    num(geom.bottom + 6.0),
                    num(x + 1.0),
                    num(geom.bottom - 2.0),
                    num(x + 1.0),
                    num(geom.bottom + 6.0),
                    num(x + 5.0),
                    num(geom.bottom - 2.0)
                ),
                None,
                Some(&palette.muted),
                1.0,
                None,
                "",
            );
        }
        if layout.variant == Variant::Wide {
            draw_annotations(&mut canvas, graph, palette, chart, &geom, &mut occupied)?;
        }
        draw_flags(
            &mut canvas,
            graph,
            palette,
            chart,
            &geom,
            style,
            &mut occupied,
        )?;
        if layout.variant == Variant::Narrow {
            draw_narrow_legend(&mut canvas, graph, palette)?;
        }
    }

    if let Some(footer) = &layout.footer {
        draw_footer(&mut canvas, graph, palette, layout, footer)?;
    }

    Ok(canvas.finish(
        layout.width,
        layout.height,
        layout.variant.title(),
        &desc_text(graph),
        palette,
        true,
    ))
}

/// The narrow card lists every milestone below the staircase; the past ones
/// are also numbered on the line itself.
fn draw_narrow_legend(canvas: &mut Canvas, graph: &CommitGraph, palette: &Palette) -> Result<()> {
    // A repository can carry no milestones; an empty legend is simply absent
    // rather than an empty text group.
    if graph.milestones.is_empty() {
        return Ok(());
    }
    let mut runs = Vec::new();
    for (index, milestone) in graph.milestones.iter().enumerate() {
        let baseline = 360.0 + index as f64 * 24.0;
        let label = format!(
            "{}. {} - {}",
            index + 1,
            milestone.short,
            short_date(&milestone.date)?
        );
        runs.push(run(label, 52.0, baseline));
    }
    canvas.text_group("milestones", Typeface::Mono, 13.0, &palette.ink, &runs, 0.0)?;
    for (index, milestone) in graph.milestones.iter().enumerate() {
        let baseline = 360.0 + index as f64 * 24.0;
        let done = milestone.status == MilestoneStatus::Done;
        let colour = if done {
            palette.teal.clone()
        } else {
            palette.muted.clone()
        };
        canvas.mk_circle(
            36.0,
            baseline - 4.0,
            7.0,
            if done { colour.as_str() } else { "none" },
            &colour,
            !done,
            "dm dm-milestone",
        );
    }
    Ok(())
}

pub(crate) fn render_full(
    graph: &CommitGraph,
    palette: &Palette,
    style: &StyleConfig,
) -> Result<String> {
    render(graph, palette, style, &Layout::wide())
}

pub(crate) fn render_narrow(
    graph: &CommitGraph,
    palette: &Palette,
    style: &StyleConfig,
) -> Result<String> {
    render(graph, palette, style, &Layout::narrow())
}

pub(crate) fn render_compact(
    graph: &CommitGraph,
    palette: &Palette,
    style: &StyleConfig,
) -> Result<String> {
    render(graph, palette, style, &Layout::compact())
}

pub(crate) fn render_compact_narrow(
    graph: &CommitGraph,
    palette: &Palette,
    style: &StyleConfig,
) -> Result<String> {
    render(graph, palette, style, &Layout::compact_narrow())
}

#[cfg(test)]
mod tests;
