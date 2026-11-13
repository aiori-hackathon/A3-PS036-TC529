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
            orange: "#FF5400".to_string(),
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

/// A non-text drawing primitive the collision rule applies to. Segments keep a
/// staircase exact instead of testing its bounding box, which would reject text
/// in the empty space above the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MarkShape {
    Segment {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Box {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

/// One mark plus the text group a leader belongs to, if any. A leader may touch
/// the run it points at and no other.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mark {
    pub(crate) kind: &'static str,
    pub(crate) owner: Option<String>,
    pub(crate) shape: MarkShape,
}

impl Mark {
    pub(crate) fn segment(kind: &'static str, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            kind,
            owner: None,
            shape: MarkShape::Segment { x1, y1, x2, y2 },
        }
    }

    pub(crate) fn boxed(kind: &'static str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            kind,
            owner: None,
            shape: MarkShape::Box {
                x,
                y,
                width,
                height,
            },
        }
    }

    pub(crate) fn leader(owner: &str, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            kind: "leader",
            owner: Some(owner.to_string()),
            shape: MarkShape::Segment { x1, y1, x2, y2 },
        }
    }

    pub(crate) fn intersects(&self, box_: &TextBox) -> bool {
        match self.shape {
            MarkShape::Box {
                x,
                y,
                width,
                height,
            } => x < box_.right() && box_.x < x + width && y < box_.bottom() && box_.y < y + height,
            MarkShape::Segment { x1, y1, x2, y2 } => segment_intersects_box(x1, y1, x2, y2, box_),
        }
    }

    /// Minimum distance from the mark to a text box: zero when they touch or
    /// overlap. Used to enforce an 8 px clearance from flag poles.
    pub(crate) fn gap_to(&self, box_: &TextBox) -> f64 {
        match self.shape {
            MarkShape::Box {
                x,
                y,
                width,
                height,
            } => {
                let other = TextBox {
                    x,
                    y,
                    width,
                    height,
                };
                box_.gap(&other)
            }
            MarkShape::Segment { x1, y1, x2, y2 } => segment_box_gap(x1, y1, x2, y2, box_),
        }
    }
}

/// True when a segment crosses or touches an axis-aligned ink box.
pub(crate) fn segment_intersects_box(x1: f64, y1: f64, x2: f64, y2: f64, box_: &TextBox) -> bool {
    let inside =
        |x: f64, y: f64| x >= box_.x && x <= box_.right() && y >= box_.y && y <= box_.bottom();
    if inside(x1, y1) || inside(x2, y2) {
        return true;
    }
    let edges = [
        (box_.x, box_.y, box_.right(), box_.y),
        (box_.right(), box_.y, box_.right(), box_.bottom()),
        (box_.right(), box_.bottom(), box_.x, box_.bottom()),
        (box_.x, box_.bottom(), box_.x, box_.y),
    ];
    edges
        .iter()
        .any(|edge| segments_cross(x1, y1, x2, y2, edge.0, edge.1, edge.2, edge.3))
}

// A segment-to-segment orientation test is two endpoints per line; naming the
// eight coordinates keeps the cross-product formula readable.
#[allow(clippy::too_many_arguments)]
fn segments_cross(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, dx: f64, dy: f64) -> bool {
    let orient = |px: f64, py: f64, qx: f64, qy: f64, rx: f64, ry: f64| {
        (qx - px) * (ry - py) - (qy - py) * (rx - px)
    };
    let o1 = orient(ax, ay, bx, by, cx, cy);
    let o2 = orient(ax, ay, bx, by, dx, dy);
    let o3 = orient(cx, cy, dx, dy, ax, ay);
    let o4 = orient(cx, cy, dx, dy, bx, by);
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

/// Distance from a point to an axis-aligned box: zero inside or on the edge.
fn point_box_gap(px: f64, py: f64, box_: &TextBox) -> f64 {
    let dx = (box_.x - px).max(px - box_.right()).max(0.0);
    let dy = (box_.y - py).max(py - box_.bottom()).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance from a point to a segment: zero when the point lies on it.
fn point_segment_gap(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    if dx == 0.0 && dy == 0.0 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    let t = ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);
    let cx = x1 + t * dx;
    let cy = y1 + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Minimum distance from a segment to an axis-aligned box: zero when they
/// touch or overlap.
fn segment_box_gap(x1: f64, y1: f64, x2: f64, y2: f64, box_: &TextBox) -> f64 {
    if segment_intersects_box(x1, y1, x2, y2, box_) {
        return 0.0;
    }
    let mut best = point_box_gap(x1, y1, box_).min(point_box_gap(x2, y2, box_));
    for (cx, cy) in [
        (box_.x, box_.y),
        (box_.right(), box_.y),
        (box_.right(), box_.bottom()),
        (box_.x, box_.bottom()),
    ] {
        best = best.min(point_segment_gap(cx, cy, x1, y1, x2, y2));
    }
    best
}

struct Canvas<'a> {
    font: &'a Font,
    body: String,
    used: UsedGlyphs,
    texts: Vec<(String, TextBox)>,
    marks: Vec<Mark>,
}

impl<'a> Canvas<'a> {
    fn new(font: &'a Font) -> Self {
        Self {
            font,
            body: String::new(),
            used: UsedGlyphs::new(),
            texts: Vec::new(),
            marks: Vec::new(),
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

    /// A one-pixel leader that names the text group it points at, so the
    /// collision rule can allow that one touch and no other.
    fn leader(&mut self, owner: &str, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str) {
        let _ = write!(
            self.body,
            "<line class=\"cg-leader\" data-owner=\"{owner}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" \
             y2=\"{}\" stroke=\"{stroke}\" stroke-width=\"1\" opacity=\"0.45\"/>",
            num(x1),
            num(y1),
            num(x2),
            num(y2)
        );
        self.marks.push(Mark::leader(owner, x1, y1, x2, y2));
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

    #[allow(clippy::too_many_arguments)]
    fn circle_mark(
        &mut self,
        cx: f64,
        cy: f64,
        r: f64,
        fill: &str,
        stroke: &str,
        dashed: bool,
        class: &str,
        mark: Mark,
    ) {
        self.mk_circle(cx, cy, r, fill, stroke, dashed, class);
        self.marks.push(mark);
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
        for run in runs {
            let box_ = self
                .font
                .line_box(face, &run.text, run.x, run.baseline, size, tracking)?;
            self.texts.push((name.to_string(), box_));
        }
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

    /// Recompute the layout's collision rule on the finished scene: a text run
    /// may not cross a data mark, a milestone label keeps eight pixels of
    /// clearance from every flag pole, and only a leader may touch the run it
    /// points at. Generation fails instead of publishing an overlapping label.
    fn check_collisions(&self) -> Result<()> {
        for (group, text) in &self.texts {
            // The pole clearance is a milestone-label rule: those labels are
            // placed around flag poles. Plot-anchored text (the annotations,
            // the axis labels and the `now` label) only has to stay clear of
            // ink, so a pole that ends below the chart cannot reject it.
            let milestone_label = group.starts_with("milestone-");
            for mark in &self.marks {
                if mark.kind == "leader" && mark.owner.as_deref() == Some(group.as_str()) {
                    continue;
                }
                if mark.kind == "pole" && milestone_label {
                    // Milestone labels need an 8 px clearance from every pole.
                    if mark.gap_to(text) < 8.0 {
                        bail!(
                            "commit-graph layout: `{group}` text box ({}, {}, {}, {}) is within 8 \
                             px of the pole mark",
                            num(text.x),
                            num(text.y),
                            num(text.width),
                            num(text.height)
                        );
                    }
                } else if mark.intersects(text) {
                    bail!(
                        "commit-graph layout: `{group}` text box ({}, {}, {}, {}) crosses the \
                         {} mark",
                        num(text.x),
                        num(text.y),
                        num(text.width),
                        num(text.height),
                        mark.kind
                    );
                }
            }
        }
        Ok(())
    }

    fn finish(
        self,
        width: f64,
        height: f64,
        title: &str,
        desc: &str,
        palette: &Palette,
        area: bool,
    ) -> Result<String> {
        self.check_collisions()?;
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
        Ok(out)
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
                bottom: 384.0,
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

/// Append a point unless it repeats the previous one. A staircase can dwell on
/// a day and produce a zero-length edge; leaving it in would make the stroke,
/// the fill edge and the collision marks disagree about where the line is.
fn push_point(points: &mut Vec<(f64, f64)>, point: (f64, f64)) {
    let repeats = points.last().is_some_and(|last| {
        (last.0 - point.0).abs() <= f64::EPSILON && (last.1 - point.1).abs() <= f64::EPSILON
    });
    if !repeats {
        points.push(point);
    }
}

/// The cumulative staircase as one deduplicated polyline, shared by the stroke,
/// the area fill and the collision marks.
fn staircase_points(geom: &ChartGeom) -> Vec<(f64, f64)> {
    let mut points = Vec::with_capacity(geom.days * 2 + 2);
    push_point(&mut points, (geom.left, geom.bottom));
    let mut previous = 0u64;
    for (index, value) in geom.cumulative.iter().enumerate() {
        let x0 = geom.left + index as f64 * geom.column;
        let x1 = x0 + geom.column;
        if *value != previous {
            push_point(&mut points, (x0, geom.y(previous)));
            push_point(&mut points, (x0, geom.y(*value)));
        }
        push_point(&mut points, (x1, geom.y(*value)));
        previous = *value;
    }
    points
}

fn polyline_path(points: &[(f64, f64)]) -> String {
    let mut path = String::new();
    for (index, (x, y)) in points.iter().enumerate() {
        let command = if index == 0 { 'M' } else { 'L' };
        let _ = write!(path, "{command}{} {}", num(*x), num(*y));
    }
    path
}

/// The closed area under the staircase. Its explicit segments are registered as
/// marks too: the fill edge is part of the drawing boundary, not decoration.
fn fill_polygon(points: &[(f64, f64)], geom: &ChartGeom) -> Vec<(f64, f64)> {
    let mut polygon = points.to_vec();
    push_point(&mut polygon, (geom.data_right, geom.bottom));
    push_point(&mut polygon, (geom.left, geom.bottom));
    polygon
}

fn register_polyline_marks(canvas: &mut Canvas<'_>, kind: &'static str, points: &[(f64, f64)]) {
    for pair in points.windows(2) {
        canvas.marks.push(Mark::segment(
            kind, pair[0].0, pair[0].1, pair[1].0, pair[1].1,
        ));
    }
}

/// One day's bar rectangle. The painter, the busiest-day annotation and the
/// collision marks all read this geometry so they cannot drift apart.
struct BarDay {
    index: usize,
    commits: u64,
    centre: f64,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

fn bar_geometry(graph: &CommitGraph, bars: &BarsLayout, geom: &ChartGeom) -> Vec<BarDay> {
    let visible: Vec<(usize, &super::DayEntry)> = match bars.last_days {
        Some(limit) => graph
            .per_day
            .iter()
            .enumerate()
            .skip(graph.per_day.len().saturating_sub(limit))
            .collect(),
        None => graph.per_day.iter().enumerate().collect(),
    };
    let count = visible.len().max(1) as f64;
    let column_width = (geom.data_right - geom.left) / count;
    let max_commits = visible
        .iter()
        .map(|(_, day)| day.commits)
        .max()
        .unwrap_or(0)
        .max(1);
    let bar_width = (column_width * 0.55).clamp(4.0, 10.0);
    visible
        .iter()
        .enumerate()
        .map(|(position, (index, day))| {
            let centre = if bars.last_days.is_some() {
                geom.left + (position as f64 + 0.5) * column_width
            } else {
                geom.centre(*index)
            };
            let height = if day.commits == 0 {
                0.0
            } else {
                (day.commits as f64 / max_commits as f64 * bars.max_height).max(3.0)
            };
            BarDay {
                index: *index,
                commits: day.commits,
                centre,
                left: centre - bar_width / 2.0,
                top: bars.baseline - height,
                width: bar_width,
                height,
            }
        })
        .collect()
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
        let size = 14.0;
        let end = (geom.data_right, geom.y(graph.totals.commits));
        let ring = 5.0;
        let width = canvas.width(Typeface::Mono, &text, size, 0.0)?;
        let candidates = [
            (geom.data_right - width, end.1 - ring - 7.0),
            (geom.data_right - ring - 6.0 - width, end.1 + 5.0),
            (geom.data_right + ring + 8.0, end.1 + 5.0),
        ];
        let mut placed: Option<(usize, f64, f64, TextBox)> = None;
        for (position, (x, baseline)) in candidates.into_iter().enumerate() {
            let box_ = canvas
                .font
                .line_box(Typeface::Mono, &text, x, baseline, size, 0.0)?;
            if box_.x < canvas_margin(chart, geom) - 0.01 || box_.right() > geom.right + 0.01 {
                continue;
            }
            if occupied
                .iter()
                .any(|other| box_.intersects(other) || box_.gap(other) < 2.0)
            {
                continue;
            }
            if canvas.marks.iter().any(|mark| mark.intersects(&box_)) {
                continue;
            }
            placed = Some((position, x, baseline, box_));
            break;
        }
        let Some((position, x, baseline, box_)) = placed else {
            bail!(
                "commit-graph `now` label `{text}` has no collision-free place around the end dot"
            );
        };
        occupied.push(canvas.text_group(
            "now-label",
            Typeface::Mono,
            size,
            &palette.ink,
            &[run(text, x, baseline)],
            0.0,
        )?);
        // The label is adjacent when it sits left or right of the dot. When it
        // sits above, a short leader closes the gap without touching the dot.
        if position == 0 {
            let gap = (end.1 - ring) - box_.bottom();
            if gap > 6.0 {
                canvas.leader(
                    "now-label",
                    geom.data_right,
                    box_.bottom(),
                    geom.data_right,
                    box_.bottom() + 6.0,
                    &palette.muted,
                );
            }
        }
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
    let points = staircase_points(geom);
    if area {
        let polygon = fill_polygon(&points, geom);
        canvas.path(
            &format!("{}Z", polyline_path(&polygon)),
            Some("url(#cg-fill)"),
            None,
            0.0,
            Some("cg-fill"),
            "",
        );
        register_polyline_marks(canvas, "fill-edge", &polygon);
    }
    let path = polyline_path(&points);
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
    register_polyline_marks(canvas, "staircase", &points);
    let end_y = geom.y(*geom.cumulative.last().unwrap_or(&0));
    canvas.circle_mark(
        geom.data_right,
        end_y,
        5.0,
        "none",
        &palette.teal,
        false,
        "cg-now-ring dm dm-now",
        Mark::boxed("now-ring", geom.data_right - 5.0, end_y - 5.0, 10.0, 10.0),
    );
    canvas.circle_mark(
        geom.data_right,
        end_y,
        3.5,
        &palette.orange,
        &palette.orange,
        false,
        "dm dm-now-dot",
        Mark::boxed("now-dot", geom.data_right - 3.5, end_y - 3.5, 7.0, 7.0),
    );
}

fn draw_bars(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    bars: &BarsLayout,
    geom: &ChartGeom,
) {
    for (visible, bar) in bar_geometry(graph, bars, geom).into_iter().enumerate() {
        if bar.commits == 0 {
            canvas.mk_circle(
                bar.centre,
                bars.baseline - 1.0,
                1.0,
                &palette.muted,
                &palette.muted,
                false,
                "dm dm-quiet",
            );
            continue;
        }
        let day = &graph.per_day[bar.index];
        let delay = (visible * 30).min(800);
        let attrs = format!(
            " data-date=\"{}\" data-commits=\"{}\" style=\"animation-delay:{delay}ms\"",
            day.date, bar.commits
        );
        canvas.mark_rect(
            bar.left,
            bar.top,
            bar.width,
            bar.height,
            &palette.orange,
            "dm dm-bar cg-bar",
            &attrs,
        );
        canvas
            .marks
            .push(Mark::boxed("bar", bar.left, bar.top, bar.width, bar.height));
    }
}

fn draw_annotations(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    chart: &ChartLayout,
    bars: &BarsLayout,
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
    let busiest_bar = bar_geometry(graph, bars, geom)
        .into_iter()
        .find(|bar| bar.index == busiest_index)
        .context("the busiest day has no bar")?;
    // Keep this compact: the label is anchored to the tallest bar, which can
    // sit near the right edge, so a longer sentence would run past the card's
    // right margin on the wide layout.
    let busiest = format!(
        "busiest: {} commits, {}",
        graph.totals.busiest_day.commits,
        short_date(&graph.totals.busiest_day.date)?
    );
    let size = 13.0;
    // Bracket geometry for the longest streak: the streak label is placed
    // left-aligned over it.
    let streak_start = parse_iso_date(&graph.totals.longest_streak_range.start)?;
    let streak_end = parse_iso_date(&graph.totals.longest_streak_range.end)?;
    let start_index = (streak_start - start).max(0) as usize;
    let end_index = (streak_end - start).max(0) as usize;
    let bracket_y = bars.baseline - bars.max_height - 6.0;
    let (bracket_x0, _bracket_x1) = if start_index < geom.days {
        (
            geom.centre(start_index),
            geom.centre(end_index.min(geom.days - 1))
                .min(geom.data_right),
        )
    } else {
        (geom.left, geom.left)
    };

    // The busiest label sits centred directly above its bar, with a short
    // vertical leader down to the bar.
    let busiest_width = canvas.width(Typeface::Mono, &busiest, size, 0.0)?;
    let margin = canvas_margin(chart, geom);
    let busiest_x = (busiest_bar.centre - busiest_width / 2.0)
        .clamp(margin + 2.0, geom.right - busiest_width - 2.0);
    let busiest_baseline = busiest_bar.top - 18.0;
    let busiest_box = canvas.font.line_box(
        Typeface::Mono,
        &busiest,
        busiest_x,
        busiest_baseline,
        size,
        0.0,
    )?;
    // The streak label is left-aligned over its bracket and must keep at least
    // sixteen pixels from the busiest label.
    let streak = format!("{}-day streak", graph.totals.longest_streak);
    let streak_x = bracket_x0;
    let mut placed_streak: Option<(f64, f64, TextBox)> = None;
    for offset in [8.0, 16.0, 24.0, 32.0, 40.0, 41.0, 48.0, 56.0] {
        let streak_baseline = bracket_y - offset;
        let box_ = canvas.font.line_box(
            Typeface::Mono,
            &streak,
            streak_x,
            streak_baseline,
            size,
            0.0,
        )?;
        if box_.x < margin - 0.01 || box_.right() > geom.right + 0.01 {
            continue;
        }
        if box_.gap(&busiest_box) < 16.0 {
            continue;
        }
        if canvas.marks.iter().any(|mark| mark.intersects(&box_)) {
            continue;
        }
        if occupied
            .iter()
            .any(|other| box_.intersects(other) || box_.gap(other) < 2.0)
        {
            continue;
        }
        placed_streak = Some((streak_x, streak_baseline, box_));
        break;
    }
    let Some((streak_x, streak_baseline, _)) = placed_streak else {
        bail!("commit-graph streak label `{streak}` has no collision-free place over its bracket");
    };

    let runs = vec![
        run(busiest, busiest_x, busiest_baseline),
        tinted(streak, streak_x, streak_baseline, &palette.teal_text),
    ];
    occupied.push(canvas.text_group(
        "annotations",
        Typeface::Mono,
        size,
        &palette.ink,
        &runs,
        0.0,
    )?);

    // A one-pixel vertical leader from the label straight down to the bar.
    let leader_start = busiest_box.bottom() + 2.0;
    if busiest_bar.top - leader_start >= 2.0 {
        canvas.leader(
            "annotations",
            busiest_bar.centre,
            leader_start,
            busiest_bar.centre,
            busiest_bar.top,
            &palette.muted,
        );
    }

    // Teal bracket over the longest streak, with small end ticks.
    if start_index < geom.days {
        let x0 = bracket_x0;
        let x1 = geom
            .centre(end_index.min(geom.days - 1))
            .min(geom.data_right);
        let y = bracket_y;
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

/// A milestone flag's fixed geometry. The shapes are drawn and registered
/// before any label is placed, so a label cannot land on a later flag's pole.
struct FlagGeom {
    index: usize,
    done: bool,
    base_x: f64,
    base_y: f64,
    pole_top: f64,
    text: String,
    width: f64,
}

/// Draw the flags and place their labels.
///
/// A wide-card label is centred directly above its pole and prefers the two
/// staggered levels. Every candidate is measured from the glyph outlines and
/// rejected when it comes within eight pixels of a box already placed or
/// touches any data mark. If neither staggered level fits, the remaining legal
/// band is searched before generation fails instead of drawing an overlap.
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

    let mut flags = Vec::with_capacity(graph.milestones.len());
    let mut narrow_runs = Vec::new();
    let mut narrow_boxes = Vec::new();
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
        if !chart.labels {
            let (number_run, number_box) = draw_narrow_flag(
                canvas,
                graph,
                palette,
                geom,
                occupied.as_slice(),
                index,
                base_x,
                base_y,
            )?;
            narrow_runs.push(number_run);
            narrow_boxes.push(number_box);
            continue;
        }
        let pole_top = base_y - if done { 18.0 } else { 22.0 };
        let text = flag_text(milestone)?;
        let width = canvas.width(Typeface::Mono, &text, 13.0, 0.0)?;
        // A label centred on a pole at the runway's right edge would overflow
        // the card; pull the pole inward by half the label so the label stays
        // centred instead of being clamped away from it. The runway is not to
        // scale, so the pole position is free.
        let base_x = if done {
            base_x
        } else {
            base_x.min(geom.right - 2.0 - width / 2.0)
        };
        flags.push(FlagGeom {
            index,
            done,
            base_x,
            base_y,
            pole_top,
            text,
            width,
        });
    }

    for flag in &flags {
        draw_flag_shapes(canvas, graph, palette, flag);
    }

    for flag in &flags {
        let size = 13.0;
        let baselines = flag_label_baselines(flag.pole_top);
        let preferred = &baselines[..2];
        let mut placed = try_place_flag_label(
            canvas,
            chart,
            geom,
            flag,
            occupied.as_slice(),
            preferred,
            8.0,
        )?;
        if placed.is_none() {
            placed = try_place_flag_label(
                canvas,
                chart,
                geom,
                flag,
                occupied.as_slice(),
                &baselines,
                4.0,
            )?;
        }
        let Some((x, baseline, box_)) = placed else {
            bail!(
                "commit-graph milestone label `{}` cannot be placed directly above its pole; \
                 shorten its `short` label or widen the card",
                flag.text
            );
        };
        let fill = if flag.done {
            &palette.ink
        } else {
            &palette.muted
        };
        canvas.text_group(
            &format!("milestone-{}", flag.index + 1),
            Typeface::Mono,
            size,
            fill,
            &[run(flag.text.clone(), x, baseline)],
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

/// Baselines to try for a milestone label, in preference order.
///
/// The first two are the staggered levels the card was designed around. The
/// rest fill the legal band from four to just under thirty pixels above the
/// pole. As the plotted date range grows the staircase slides under a label
/// whose width is fixed, so a pair of fixed levels can stop fitting even
/// though the card still has room; searching the band keeps generation
/// deterministic and inside the documented gap rule.
fn flag_label_baselines(pole_top: f64) -> Vec<f64> {
    let mut baselines = vec![pole_top - 8.0, pole_top - 26.0];
    let mut gap = 4.0;
    while gap < 45.0 {
        let baseline = pole_top - gap;
        if !baselines
            .iter()
            .any(|existing| (existing - baseline).abs() < 0.25)
        {
            baselines.push(baseline);
        }
        gap += 0.5;
    }
    baselines
}

/// Try each baseline in order, returning the first label box that clears the
/// card margins, the already-placed labels and every data mark.
fn try_place_flag_label(
    canvas: &Canvas,
    chart: &ChartLayout,
    geom: &ChartGeom,
    flag: &FlagGeom,
    occupied: &[TextBox],
    baselines: &[f64],
    minimum_gap: f64,
) -> Result<Option<(f64, f64, TextBox)>> {
    let size = 13.0;
    let x = (flag.base_x - flag.width / 2.0).clamp(
        canvas_margin(chart, geom) + 2.0,
        geom.right - flag.width - 2.0,
    );
    for &baseline in baselines {
        let box_ = canvas
            .font
            .line_box(Typeface::Mono, &flag.text, x, baseline, size, 0.0)?;
        if box_.x < canvas_margin(chart, geom) - 0.01 || box_.right() > geom.right + 0.01 {
            continue;
        }
        if occupied
            .iter()
            .any(|other| box_.intersects(other) || box_.gap(other) < minimum_gap)
        {
            continue;
        }
        if canvas.marks.iter().any(|mark| {
            if mark.kind == "pole" {
                // Milestone labels need an 8 px clearance from every pole.
                mark.gap_to(&box_) < 8.0
            } else {
                mark.intersects(&box_)
            }
        }) {
            continue;
        }
        return Ok(Some((x, baseline, box_)));
    }
    Ok(None)
}

fn draw_flag_shapes(canvas: &mut Canvas, graph: &CommitGraph, palette: &Palette, flag: &FlagGeom) {
    let milestone = &graph.milestones[flag.index];
    let colour = if flag.done {
        palette.teal.clone()
    } else {
        palette.muted.clone()
    };
    let mut shapes = String::new();
    let _ = write!(
        shapes,
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{colour}\" \
         stroke-width=\"1\"{} />",
        num(flag.base_x),
        num(flag.base_y),
        num(flag.base_x),
        num(flag.pole_top),
        if flag.done {
            String::new()
        } else {
            " stroke-dasharray=\"3 3\"".to_string()
        }
    );
    let pennant = format!(
        "M{} {}L{} {}L{} {}Z",
        num(flag.base_x),
        num(flag.pole_top),
        num(flag.base_x + 12.0),
        num(flag.pole_top + 4.0),
        num(flag.base_x),
        num(flag.pole_top + 8.0)
    );
    let _ = write!(
        shapes,
        "<path d=\"{pennant}\" fill=\"{}\" stroke=\"{colour}\" stroke-width=\"1\"{} />",
        if flag.done {
            colour.clone()
        } else {
            "none".to_string()
        },
        if flag.done {
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
        num(flag.base_x),
        num(flag.base_y),
        if flag.done {
            colour.clone()
        } else {
            "none".to_string()
        }
    );
    let _ = write!(canvas.body, "<g class=\"cg-flag\">{shapes}</g>");
    canvas.marks.push(Mark::segment(
        "pole",
        flag.base_x,
        flag.base_y,
        flag.base_x,
        flag.pole_top,
    ));
    canvas.marks.push(Mark::segment(
        "pennant",
        flag.base_x,
        flag.pole_top,
        flag.base_x + 12.0,
        flag.pole_top + 4.0,
    ));
    canvas.marks.push(Mark::segment(
        "pennant",
        flag.base_x + 12.0,
        flag.pole_top + 4.0,
        flag.base_x,
        flag.pole_top + 8.0,
    ));
    canvas.marks.push(Mark::boxed(
        "milestone-dot",
        flag.base_x - 2.5,
        flag.base_y - 2.5,
        5.0,
        5.0,
    ));
}

/// The narrow card turns each milestone into a numbered dot on the line and
/// lists the numbers underneath; one text group carries every number so the
/// group budget stays below the narrow cap.
// The narrow placer needs the canvas, the document, the palette and the chart
// geometry; a wrapper struct would only rename the same arguments.
#[allow(clippy::too_many_arguments)]
fn draw_narrow_flag(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    geom: &ChartGeom,
    occupied: &[TextBox],
    index: usize,
    base_x: f64,
    base_y: f64,
) -> Result<(Run, TextBox)> {
    let milestone = &graph.milestones[index];
    let done = milestone.status == MilestoneStatus::Done;
    let colour = if done {
        palette.teal.as_str()
    } else {
        palette.muted.as_str()
    };
    canvas.circle_mark(
        base_x,
        base_y,
        4.0,
        if done { colour } else { "none" },
        colour,
        !done,
        "dm dm-milestone",
        Mark::boxed("milestone-dot", base_x - 4.0, base_y - 4.0, 8.0, 8.0),
    );
    let _ = write!(
        canvas.body,
        "<circle class=\"dm dm-milestone\" data-date=\"{}\" data-status=\"{}\" \
         cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\" stroke=\"{colour}\" stroke-width=\"1\"{} />",
        milestone.date,
        milestone.status.as_str(),
        num(base_x),
        num(base_y),
        if done { colour } else { "none" },
        if done {
            ""
        } else {
            " stroke-dasharray=\"2 3\""
        }
    );
    let number = (index + 1).to_string();
    let size = 12.0;
    let width = canvas.width(Typeface::Mono, &number, size, 0.0)?;
    let x = (base_x - width / 2.0).clamp(24.0, geom.right - width - 2.0);
    let mut placed = None;
    for baseline in [base_y - 7.0, base_y + 18.0, base_y - 24.0, base_y + 32.0] {
        let box_ = canvas
            .font
            .line_box(Typeface::Mono, &number, x, baseline, size, 0.0)?;
        if box_.y < 190.0 || box_.bottom() > 348.0 {
            continue;
        }
        // These are single digits that label adjacent points; they may sit
        // close, so the sweep only has to keep them from touching.
        if occupied
            .iter()
            .any(|other| box_.intersects(other) || box_.gap(other) < 2.0)
        {
            continue;
        }
        if canvas.marks.iter().any(|mark| mark.intersects(&box_)) {
            continue;
        }
        placed = Some((baseline, box_));
        break;
    }
    let Some((baseline, box_)) = placed else {
        bail!(
            "commit-graph milestone number `{number}` cannot be placed on the narrow card \
             without overlapping another label"
        );
    };
    Ok((
        tinted(
            number,
            x,
            baseline,
            if done { &palette.ink } else { &palette.muted },
        ),
        box_,
    ))
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
    occupied: &mut Vec<TextBox>,
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
    occupied.push(canvas.text_group(
        "kpi-captions",
        Typeface::Mono,
        kpi.caption_size,
        &palette.muted,
        &caption_runs,
        0.08,
    )?);
    occupied.push(canvas.text_group(
        "kpi-values",
        display,
        kpi.value_size,
        &palette.ink,
        &value_runs,
        0.0,
    )?);
    if !detail_runs.is_empty() {
        occupied.push(canvas.text_group(
            "kpi-detail",
            display,
            kpi.detail_size,
            &palette.ink,
            &detail_runs,
            0.0,
        )?);
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

/// Footer wording for the refresh count. Absent when the history has none.
fn draw_footer(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    layout: &Layout,
    footer: &FooterLayout,
) -> Result<()> {
    let width = layout.width;
    let margin = layout.margin;
    // Left: the public-history source. Right: the as-of stamp. One line when
    // there is room, two at most.
    let full_source = format!("from the public history of {}", repository_slug(graph));
    let short_source = format!("public history of {}", repository_name(graph));
    // The narrow card cannot fit the full source string on one line, so it uses
    // the short form there; the wide card shows the full source.
    let source_text = if footer.stacked {
        short_source.as_str()
    } else {
        full_source.as_str()
    };
    let sha = &graph.as_of_sha[..7.min(graph.as_of_sha.len())];
    let date = long_date(&graph.as_of_date)?;
    let as_of_text = format!("as of {sha} - {date}");
    let source_size = footer.source_size;
    let baseline = footer.baseline;

    let source_x = margin;
    let as_of_x = canvas.right_x(
        Typeface::Mono,
        &as_of_text,
        width - margin,
        source_size,
        0.0,
    )?;

    let mut runs = vec![run(source_text, source_x, baseline)];
    if footer.stacked {
        // Narrow card: the as-of stamp drops to a second line so the source
        // line is not squeezed off the card.
        runs.push(run(as_of_text, as_of_x, baseline + source_size + 5.0));
    } else {
        runs.push(run(as_of_text, as_of_x, baseline));
    }
    canvas.text_group(
        "footer-source",
        Typeface::Mono,
        source_size,
        &palette.muted,
        &runs,
        0.0,
    )?;
    Ok(())
}

/// The graph-refresh note. It lives in the legend only, and only when the
/// history carries at least one graph refresh.
fn refresh_note(count: u64) -> Option<String> {
    match count {
        0 => None,
        1 => Some("includes 1 graph refresh".to_string()),
        count => Some(format!("includes {count} graph refreshes")),
    }
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
        return canvas.finish(
            layout.width,
            layout.height,
            layout.variant.title(),
            &desc_text(graph),
            palette,
            false,
        );
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
        draw_kpi(&mut canvas, graph, palette, kpi, display, &mut occupied)?;
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
        // Flags place their labels before the `now` label so the two cannot
        // claim the same space above the end dot.
        draw_flags(
            &mut canvas,
            graph,
            palette,
            chart,
            &geom,
            style,
            &mut occupied,
        )?;
        draw_chart_labels(&mut canvas, graph, palette, chart, &geom, &mut occupied)?;
        if layout.variant == Variant::Wide {
            draw_annotations(
                &mut canvas,
                graph,
                palette,
                chart,
                bars,
                &geom,
                &mut occupied,
            )?;
        }
        if layout.variant == Variant::Narrow {
            draw_narrow_legend(&mut canvas, graph, palette)?;
        }
    }

    if let Some(footer) = &layout.footer {
        draw_footer(&mut canvas, graph, palette, layout, footer)?;
    }

    canvas.finish(
        layout.width,
        layout.height,
        layout.variant.title(),
        &desc_text(graph),
        palette,
        true,
    )
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
    // The graph-refresh note lives in the legend only, and only when the
    // history carries at least one graph refresh.
    if let Some(note) = refresh_note(graph.totals.refresh_commits) {
        let note_baseline = 360.0 + graph.milestones.len() as f64 * 24.0 + 8.0;
        canvas.text_group(
            "refresh-note",
            Typeface::Mono,
            12.0,
            &palette.muted,
            &[run(note, 52.0, note_baseline)],
            0.0,
        )?;
    }
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
