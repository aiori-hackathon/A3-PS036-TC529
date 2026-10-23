//! Rendering of the two self-contained commit-graph cards.
//!
//! The layout is deliberately fixed (880 points wide) so the cards keep their
//! proportions inside a README column. Every string is drawn as outlines, no
//! element points at a file or a URL, and the one animation is CSS inside the
//! document. The final state, with animation disabled, is the complete card.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::text::{glyph_defs, text_markup, text_width, Font, TextBox};
use super::{parse_iso_date, CommitGraph};

/// Colours sampled from the artwork and recorded in `docs/media/brand.toml`.
pub(crate) struct Palette {
    background: String,
    panel: String,
    panel_stroke: String,
    border: String,
    ink: String,
    muted: String,
    orange: String,
    teal: String,
    heat: [String; 5],
}

impl Palette {
    pub(crate) fn load(root: &std::path::Path) -> Result<Self> {
        let mut palette = Self::default();
        let path = root.join("docs/media/brand.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(palette);
        };
        let value: toml::Value = toml::from_str(&text)
            .with_context(|| format!("{} is not valid TOML", path.display()))?;
        if let Some(hex) = hex_at(&value, &["dark_theme", "background"]) {
            palette.background = hex;
        }
        if let Some(hex) = hex_at(&value, &["brand", "orange", "hex"]) {
            palette.orange = hex;
        }
        if let Some(hex) = hex_at(&value, &["brand", "teal", "hex"]) {
            palette.teal = hex;
        }
        if let Some(hex) = hex_at(&value, &["dark_theme", "slate", "replacement"]) {
            palette.ink = hex;
        }
        palette.heat = [
            "#241A17".to_string(),
            "#43271D".to_string(),
            "#673524".to_string(),
            "#8F452C".to_string(),
            palette.orange.clone(),
        ];
        Ok(palette)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: "#0D1117".to_string(),
            panel: "#161B22".to_string(),
            panel_stroke: "#21262D".to_string(),
            border: "#30363D".to_string(),
            ink: "#E6EDF3".to_string(),
            muted: "#8B949E".to_string(),
            orange: "#C55E33".to_string(),
            teal: "#5C8E8D".to_string(),
            heat: [
                "#241A17".to_string(),
                "#43271D".to_string(),
                "#673524".to_string(),
                "#8F452C".to_string(),
                "#C55E33".to_string(),
            ],
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

/// Machine-readable numbers carried by the SVG `<desc>`; the documentation
/// check compares these with the JSON document.
pub(crate) fn desc_text(graph: &CommitGraph) -> String {
    let totals = &graph.totals;
    let milestones = graph
        .milestones
        .iter()
        .map(|milestone| {
            format!(
                "{} {}{}",
                milestone.date,
                milestone.label,
                if milestone.future { " (upcoming)" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "CLIMB commit graph. commits={} refresh_commits={} active_days={} longest_streak={} \
         current_streak={} lines_added={} lines_removed={} files_touched={} as_of={} date={} \
         authors={} Milestones: {milestones}",
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
        graph.per_author.len()
    )
}

fn style_block(stair_length: f64) -> String {
    let template = "\
.climb-stair{animation:climb-draw 1.7s cubic-bezier(.22,.85,.24,1) both}\
.climb-area{animation:climb-fade 1.2s ease-out both}\
.climb-now{transform-box:fill-box;transform-origin:center;animation:climb-pulse 2.7s ease-in-out 1.1s infinite}\
@keyframes climb-draw{from{stroke-dashoffset:{LEN}}to{stroke-dashoffset:0}}\
@keyframes climb-fade{from{opacity:0}to{opacity:1}}\
@keyframes climb-pulse{0%,100%{opacity:1;transform:scale(1)}50%{opacity:.35;transform:scale(2)}}\
@media (prefers-reduced-motion:reduce){.climb-stair,.climb-area,.climb-now{animation:none}}";
    template.replace("{LEN}", &num(stair_length))
}

struct Canvas<'a> {
    font: &'a Font,
    body: String,
    used: BTreeSet<char>,
    stair_length: f64,
}

impl<'a> Canvas<'a> {
    fn new(font: &'a Font) -> Self {
        Self {
            font,
            body: String::new(),
            used: BTreeSet::new(),
            stair_length: 0.0,
        }
    }

    fn raw(&mut self, markup: &str) {
        self.body.push_str(markup);
    }

    // A drawing primitive spells its geometry out: wrapping it in a struct
    // would turn every call site into a constructor invocation.
    #[allow(clippy::too_many_arguments)]
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: Option<&str>, rx: f64) {
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
            let _ = write!(self.body, " stroke=\"{stroke}\"");
        }
        self.body.push_str("/>");
    }

    // As with `rect`: the coordinates are the clearest possible signature.
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
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{stroke}\"",
            num(x1),
            num(y1),
            num(x2),
            num(y2)
        );
        if (width - 1.0).abs() > f64::EPSILON {
            let _ = write!(self.body, " stroke-width=\"{}\"", num(width));
        }
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
        dash_length: Option<f64>,
    ) {
        let _ = write!(self.body, "<path d=\"{d}\"");
        if let Some(fill) = fill {
            let _ = write!(self.body, " fill=\"{fill}\"");
        } else {
            self.body.push_str(" fill=\"none\"");
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
            if let Some(length) = dash_length {
                let _ = write!(
                    self.body,
                    " stroke-dasharray=\"{}\" stroke-dashoffset=\"0\"",
                    num(length)
                );
            }
        }
        self.body.push_str("/>");
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str, stroke: Option<&str>) {
        let _ = write!(
            self.body,
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\"",
            num(cx),
            num(cy),
            num(r)
        );
        if let Some(stroke) = stroke {
            let _ = write!(self.body, " stroke=\"{stroke}\"");
        }
        self.body.push_str("/>");
    }

    /// A filled rectangle that carries data rather than surface. The
    /// readability contract checks that no text sits on top of one. Heat cells
    /// and contributor segments share the same 2 px corner radius, so it is
    /// fixed here rather than threaded through every call.
    fn mark_rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, kind: &str) {
        const DATA_MARK_RX: f64 = 2.0;
        let _ = write!(
            self.body,
            "<rect class=\"dm dm-{kind}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
             rx=\"{}\" fill=\"{fill}\"/>",
            num(x),
            num(y),
            num(w),
            num(h),
            num(DATA_MARK_RX)
        );
    }

    fn mark_circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str, stroke: &str, kind: &str) {
        let _ = write!(
            self.body,
            "<circle class=\"dm dm-{kind}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\" \
             stroke=\"{stroke}\"/>",
            num(cx),
            num(cy),
            num(r)
        );
    }

    fn width(&self, text: &str, size: f64) -> f64 {
        text_width(self.font, text, size, 0.0)
    }

    fn right_x(&self, text: &str, right: f64, size: f64) -> f64 {
        right - self.width(text, size)
    }

    /// Draw one semantic text group. Every run shares the group's size and
    /// fill; the group carries the size and ink colour the readability
    /// contract reads, and the run boxes let it check spacing and data marks.
    fn text_group(
        &mut self,
        name: &str,
        size: f64,
        fill: &str,
        runs: &[(String, f64, f64)],
    ) -> TextBox {
        let mut union: Option<TextBox> = None;
        let mut inner = String::new();
        for (text, x, baseline) in runs {
            let box_ = self.font.line_box(text, *x, *baseline, size, 0.0);
            union = Some(match union {
                Some(current) => current.union(box_),
                None => box_,
            });
            inner.push_str(&text_markup(
                self.font,
                text,
                *x,
                *baseline,
                size,
                fill,
                0.0,
                &mut self.used,
            ));
        }
        let union = union.expect("a text group carries at least one run");
        let _ = write!(
            self.body,
            "<g class=\"cg-text-group\" data-name=\"{name}\" data-size=\"{}\" fill=\"{fill}\" \
             data-box=\"{} {} {} {}\">{inner}</g>",
            num(size),
            num(union.x),
            num(union.y),
            num(union.width),
            num(union.height)
        );
        union
    }

    fn finish(
        self,
        width: u32,
        height: u32,
        title: &str,
        desc: &str,
        style: &str,
        palette: &Palette,
    ) -> String {
        let glyphs = glyph_defs(self.font, &self.used);
        let mut out = String::new();
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" \
             width=\"{width}\" height=\"{height}\" role=\"img\" \
             aria-labelledby=\"cg-title cg-desc\">\
             <title id=\"cg-title\">{}</title><desc id=\"cg-desc\">{}</desc>\
             <style>{style}</style><defs>\
             <linearGradient id=\"climb-stroke\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\">\
             <stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient>\
             <linearGradient id=\"climb-fill\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\">\
             <stop offset=\"0\" stop-color=\"{}\" stop-opacity=\"0.24\"/>\
             <stop offset=\"1\" stop-color=\"{}\" stop-opacity=\"0.10\"/></linearGradient>{glyphs}</defs>",
            escape_xml(title),
            escape_xml(desc),
            palette.orange,
            palette.teal,
            palette.orange,
            palette.teal
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

fn heat_level(commits: u64) -> usize {
    match commits {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4..=5 => 4,
        _ => 5,
    }
    .min(5)
}

/// The columns of the calendar heatmap: Monday of the first week, number of
/// weeks, and the day index of the first Monday.
fn heatmap_weeks(start: i64, end: i64) -> (i64, usize) {
    // 1970-01-01 was a Thursday: Monday is day -3 modulo 7.
    let offset = (start + 3).rem_euclid(7);
    let first_monday = start - offset;
    let last_sunday = end + (6 - (end + 3).rem_euclid(7));
    let weeks = ((last_sunday - first_monday) / 7 + 1) as usize;
    (first_monday, weeks)
}

/// Geometry of one cumulative-commit staircase.
struct Staircase {
    panel: (f64, f64, f64, f64),
    plot_left: f64,
    plot_right: f64,
    plot_top: f64,
    plot_bottom: f64,
    date_baseline: f64,
    tick_size: f64,
    date_size: f64,
    total_size: f64,
    radius: f64,
    stroke: bool,
}

/// Draw the staircase row and return the boxes of its three text groups.
fn draw_staircase(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    staircase: &Staircase,
) -> Result<(TextBox, TextBox, TextBox)> {
    let (panel_x, panel_y, panel_w, panel_h) = staircase.panel;
    canvas.rect(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        &palette.panel,
        if staircase.stroke {
            Some(&palette.panel_stroke)
        } else {
            None
        },
        staircase.radius,
    );

    let start = parse_iso_date(&graph.range.start)?;
    let end = parse_iso_date(&graph.range.end)?;
    let max = nice_max(graph.totals.commits);
    let day_span = (end - start).max(1) as f64 + 1.0;
    let day_width = (staircase.plot_right - staircase.plot_left) / day_span;
    let x_of = |day: i64| staircase.plot_left + (day - start) as f64 * day_width;
    let y_of = |value: u64| {
        staircase.plot_bottom
            - (value as f64 / max as f64) * (staircase.plot_bottom - staircase.plot_top)
    };

    let mut tick_runs = Vec::new();
    for step in 0..=2 {
        let value = max / 2 * step as u64;
        let y = y_of(value);
        canvas.line(
            staircase.plot_left,
            y,
            staircase.plot_right,
            y,
            &palette.border,
            0.8,
            if step == 0 { None } else { Some("2 4") },
            if step == 0 { 0.9 } else { 0.45 },
        );
        let label = thousands(value);
        let x = staircase.plot_left - 10.0 - canvas.width(&label, staircase.tick_size);
        tick_runs.push((label, x, y + staircase.tick_size * 0.22));
    }
    let ticks = canvas.text_group("y-ticks", staircase.tick_size, &palette.muted, &tick_runs);

    let mut cumulative = 0u64;
    let mut stair = String::new();
    let mut stair_length = 0.0f64;
    let mut previous_point = (x_of(start), y_of(0));
    let _ = write!(
        stair,
        "M{} {}",
        num(previous_point.0),
        num(previous_point.1)
    );
    let mut previous = 0u64;
    for (index, day) in graph.per_day.iter().enumerate() {
        let day_index = start + index as i64;
        cumulative += day.commits;
        if cumulative != previous {
            let point = (x_of(day_index), y_of(cumulative));
            stair_length += (point.0 - previous_point.0).abs() + (point.1 - previous_point.1).abs();
            let _ = write!(stair, "L{} {}", num(point.0), num(point.1));
            previous_point = point;
        }
        let point = (x_of(day_index + 1), y_of(cumulative));
        stair_length += (point.0 - previous_point.0).abs() + (point.1 - previous_point.1).abs();
        let _ = write!(stair, "L{} {}", num(point.0), num(point.1));
        previous_point = point;
        previous = cumulative;
    }
    let data_end = x_of(end + 1);
    let mut area = stair.clone();
    let _ = write!(
        area,
        "L{} {}L{} {}Z",
        num(data_end),
        num(staircase.plot_bottom),
        num(x_of(start)),
        num(staircase.plot_bottom)
    );
    canvas.path(
        &area,
        Some("url(#climb-fill)"),
        None,
        0.0,
        Some("climb-area"),
        None,
    );
    canvas.path(
        &stair,
        None,
        Some("url(#climb-stroke)"),
        2.2,
        Some("climb-stair dm-line"),
        Some(stair_length),
    );
    canvas.stair_length = stair_length;

    canvas.circle(
        data_end,
        y_of(cumulative),
        6.0,
        "none",
        Some(&palette.orange),
    );
    canvas.raw(&format!(
        "<circle class=\"climb-now dm dm-now\" cx=\"{}\" cy=\"{}\" r=\"3.4\" fill=\"{}\"/>",
        num(data_end),
        num(y_of(cumulative)),
        palette.orange
    ));

    let start_label = graph.range.start.clone();
    let end_label = graph.range.end.clone();
    let dates = canvas.text_group(
        "dates",
        staircase.date_size,
        &palette.muted,
        &[
            (start_label, staircase.plot_left, staircase.date_baseline),
            (
                end_label,
                canvas.right_x(&graph.range.end, data_end, staircase.date_size),
                staircase.date_baseline,
            ),
        ],
    );

    let total = thousands(graph.totals.commits);
    let total_box = canvas.text_group(
        "last-total",
        staircase.total_size,
        &palette.ink,
        &[(
            total.clone(),
            canvas.right_x(&total, data_end - 10.0, staircase.total_size),
            y_of(graph.totals.commits) - staircase.total_size * 0.75,
        )],
    );
    Ok((ticks, dates, total_box))
}

/// Geometry of one calendar heatmap.
struct Heatmap {
    panel: (f64, f64, f64, f64),
    grid_x: f64,
    grid_y: f64,
    cell: f64,
    gap: f64,
    weeks_limit: usize,
    radius: f64,
    label_size: f64,
}

/// Draw the heatmap row and return the boxes of its text groups.
fn draw_heatmap(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    heatmap: &Heatmap,
) -> Result<(TextBox, TextBox, TextBox)> {
    let (panel_x, panel_y, panel_w, panel_h) = heatmap.panel;
    canvas.rect(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        &palette.panel,
        Some(&palette.panel_stroke),
        heatmap.radius,
    );
    let start = parse_iso_date(&graph.range.start)?;
    let end = parse_iso_date(&graph.range.end)?;
    let (first_monday, weeks) = heatmap_weeks(start, end);
    let visible_weeks = weeks.min(heatmap.weeks_limit).max(1);
    let hidden_weeks = weeks.saturating_sub(visible_weeks);
    let first_column_day = first_monday + hidden_weeks as i64 * 7;

    let mut month_runs = Vec::new();
    for week in 0..visible_weeks {
        let monday = first_column_day + week as i64 * 7;
        let month = super::civil_from_days(monday).1;
        let month_changed = week == 0 || super::civil_from_days(monday - 7).1 != month;
        if month_changed {
            month_runs.push((
                month_name(month).to_string(),
                heatmap.grid_x + week as f64 * (heatmap.cell + heatmap.gap),
                heatmap.grid_y - 6.0,
            ));
        }
    }
    let months = canvas.text_group("months", heatmap.label_size, &palette.muted, &month_runs);

    let mut weekday_runs = Vec::new();
    for (row, label) in [(0usize, "Mon"), (2, "Wed"), (4, "Fri")] {
        let x = heatmap.grid_x - 10.0 - canvas.width(label, heatmap.label_size);
        weekday_runs.push((
            label.to_string(),
            x,
            heatmap.grid_y + row as f64 * (heatmap.cell + heatmap.gap) + heatmap.cell * 0.72,
        ));
    }
    let weekdays = canvas.text_group(
        "weekdays",
        heatmap.label_size,
        &palette.muted,
        &weekday_runs,
    );

    for week in 0..visible_weeks {
        for row in 0..7 {
            let day = first_column_day + week as i64 * 7 + row as i64;
            if day < start || day > end {
                continue;
            }
            let index = (day - start) as usize;
            let Some(entry) = graph.per_day.get(index) else {
                continue;
            };
            let level = heat_level(entry.commits);
            let x = heatmap.grid_x + week as f64 * (heatmap.cell + heatmap.gap);
            let y = heatmap.grid_y + row as f64 * (heatmap.cell + heatmap.gap);
            if level == 0 {
                canvas.mark_rect(x, y, heatmap.cell, heatmap.cell, &palette.panel, "heat");
            } else {
                canvas.mark_rect(
                    x,
                    y,
                    heatmap.cell,
                    heatmap.cell,
                    &palette.heat[level - 1],
                    "heat",
                );
            }
        }
    }

    let legend_size = heatmap.label_size;
    let less = "less".to_string();
    let more = "more".to_string();
    let less_w = canvas.width(&less, legend_size);
    let more_w = canvas.width(&more, legend_size);
    let swatch = 10.0;
    let swatch_gap = 2.0;
    let swatch_count = (palette.heat.len() - 1) as f64;
    let swatches_w = swatch_count * swatch + (swatch_count - 1.0) * swatch_gap;
    let legend_w = less_w + 6.0 + swatches_w + 6.0 + more_w;
    let grid_w = visible_weeks as f64 * heatmap.cell + (visible_weeks as f64 - 1.0) * heatmap.gap;
    let preferred = heatmap.grid_x + grid_w + 36.0;
    let max_x = panel_x + panel_w - 14.0 - legend_w;
    let legend_x = if preferred <= max_x {
        preferred
    } else {
        (heatmap.grid_x + grid_w + 12.0).max(panel_x + 14.0)
    };
    let legend_baseline = heatmap.grid_y + heatmap.cell * 0.72;
    let mut legend_runs = vec![(less, legend_x, legend_baseline)];
    let swatch_x = legend_x + less_w + 6.0;
    for index in 0..(palette.heat.len() - 1) {
        canvas.rect(
            swatch_x + index as f64 * (swatch + swatch_gap),
            legend_baseline - swatch * 0.75,
            swatch,
            swatch,
            &palette.heat[index + 1],
            None,
            1.5,
        );
    }
    legend_runs.push((more, swatch_x + swatches_w + 6.0, legend_baseline));
    let legend = canvas.text_group("legend", legend_size, &palette.muted, &legend_runs);

    Ok((months, weekdays, legend))
}

/// Geometry of the contributor row: the stacked bar and the name/count line
/// beneath it.
struct ContributorLayout {
    x: f64,
    max_right: f64,
    bar_y: f64,
    bar_height: f64,
    text_baseline: f64,
    size: f64,
}

/// Draw the contributor bar and its name/count row. Authors with no commits
/// are not drawn at all. Returns the text group's box.
fn draw_contributors(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    layout: ContributorLayout,
) -> Option<TextBox> {
    let ContributorLayout {
        x,
        max_right,
        bar_y,
        bar_height,
        text_baseline,
        size,
    } = layout;
    let authors: Vec<&super::AuthorEntry> = graph
        .per_author
        .iter()
        .filter(|author| author.commits > 0)
        .collect();
    if authors.is_empty() {
        return None;
    }
    let visible_total: u64 = authors.iter().map(|author| author.commits).sum();
    let bar_width = max_right - x;
    let mut cursor = x;
    for (index, author) in authors.iter().enumerate() {
        let share = author.commits as f64 / visible_total.max(1) as f64;
        let segment = (bar_width * share).max(2.0);
        let colour = if index % 2 == 0 {
            palette.orange.clone()
        } else {
            palette.teal.clone()
        };
        canvas.mark_rect(cursor, bar_y, segment, bar_height, &colour, "bar");
        cursor += segment;
        if index + 1 < authors.len() {
            cursor += 2.0;
        }
    }

    let mut runs = Vec::new();
    let mut pen_x = x;
    let mut baseline = text_baseline;
    for author in authors {
        let text = format!("{} {}", author.display, thousands(author.commits));
        let width = canvas.width(&text, size);
        if pen_x + width > max_right && pen_x > x {
            pen_x = x;
            baseline += size + 5.0;
        }
        runs.push((text, pen_x, baseline));
        pen_x += width + 18.0;
    }
    Some(canvas.text_group("contributors", size, &palette.ink, &runs))
}

/// Draw up to four milestones in the supplied column x positions and row
/// baselines, with a filled dot for a done milestone and a hollow one for an
/// upcoming milestone. Returns the text group's box.
fn draw_milestones(
    canvas: &mut Canvas,
    graph: &CommitGraph,
    palette: &Palette,
    columns: &[f64],
    row_baselines: &[f64],
    size: f64,
) -> Option<TextBox> {
    if columns.is_empty() || row_baselines.is_empty() || graph.milestones.is_empty() {
        return None;
    }
    let mut runs = Vec::new();
    for (index, milestone) in graph.milestones.iter().take(4).enumerate() {
        let column = columns[index % columns.len()];
        let baseline = row_baselines[(index / columns.len()).min(row_baselines.len() - 1)];
        let cy = baseline - 4.5;
        if milestone.future {
            canvas.mark_circle(
                column + 7.0,
                cy,
                5.0,
                &palette.panel,
                &palette.muted,
                "milestone",
            );
        } else {
            canvas.mark_circle(
                column + 7.0,
                cy,
                5.0,
                &palette.orange,
                &palette.orange,
                "milestone",
            );
        }
        runs.push((
            format!("{}  {}", milestone.date, milestone.label),
            column + 20.0,
            baseline,
        ));
    }
    Some(canvas.text_group("milestones", size, &palette.ink, &runs))
}

/// The four KPI tile labels and values. The fourth tile keeps the removed-line
/// count visible without needing a second 36 px number.
fn kpi_tiles(graph: &CommitGraph, narrow: bool) -> (Vec<String>, Vec<String>) {
    let labels = vec![
        "Commits".to_string(),
        "Active days".to_string(),
        "Longest streak".to_string(),
        if narrow {
            let removed = graph.totals.lines_removed;
            if removed >= 1_000 {
                format!("Lines changed -{:.1}k", removed as f64 / 1000.0)
            } else {
                format!("Lines changed -{}", thousands(removed))
            }
        } else {
            format!("Lines changed -{}", thousands(graph.totals.lines_removed))
        },
    ];
    let values = vec![
        thousands(graph.totals.commits),
        thousands(graph.totals.active_days),
        format!("{}d", graph.totals.longest_streak),
        format!("+{}", thousands(graph.totals.lines_added)),
    ];
    (labels, values)
}

/// Wide card: 840 x 520, shown at 840 px so one user unit is one pixel.
pub(crate) fn render_full(graph: &CommitGraph, palette: &Palette) -> Result<String> {
    let font = Font::load()?;
    let mut canvas = Canvas::new(&font);
    let (width, height) = (840.0, 520.0);
    let margin = 22.0;
    canvas.rect(
        1.0,
        1.0,
        width - 2.0,
        height - 2.0,
        &palette.background,
        Some(&palette.border),
        18.0,
    );

    // Row 1: title and stamp.
    canvas.text_group(
        "title",
        24.0,
        &palette.ink,
        &[("The Climb".to_string(), margin, 33.0)],
    );
    let sha = &graph.as_of_sha[..7.min(graph.as_of_sha.len())];
    let stamp = format!("as of {sha} - {}", graph.as_of_date);
    let refresh = format!("of which {} graph refreshes", graph.totals.refresh_commits);
    let refresh_x = width - margin - canvas.width(&refresh, 13.0);
    let stamp_x = refresh_x - 14.0 - canvas.width(&stamp, 13.0);
    canvas.text_group(
        "stamp",
        13.0,
        &palette.muted,
        &[(stamp, stamp_x, 33.0), (refresh, refresh_x, 33.0)],
    );

    // Row 2: four KPI tiles.
    let tile_y = 44.0;
    let tile_h = 80.0;
    let gap = 12.0;
    let tile_w = (width - 2.0 * margin - 3.0 * gap) / 4.0;
    let (labels, values) = kpi_tiles(graph, false);
    let mut label_runs = Vec::new();
    let mut value_runs = Vec::new();
    for index in 0..4 {
        let x = margin + index as f64 * (tile_w + gap);
        canvas.rect(
            x,
            tile_y,
            tile_w,
            tile_h,
            &palette.panel,
            Some(&palette.panel_stroke),
            10.0,
        );
        label_runs.push((labels[index].clone(), x + 12.0, 62.0));
        value_runs.push((values[index].clone(), x + 12.0, 110.0));
    }
    canvas.text_group("kpi-labels", 13.0, &palette.muted, &label_runs);
    canvas.text_group("kpi-values", 36.0, &palette.ink, &value_runs);

    // Row 3: the cumulative staircase, 840 x 190.
    draw_staircase(
        &mut canvas,
        graph,
        palette,
        &Staircase {
            panel: (0.0, 128.0, 840.0, 190.0),
            plot_left: 64.0,
            plot_right: 818.0,
            plot_top: 166.0,
            plot_bottom: 294.0,
            date_baseline: 311.0,
            tick_size: 12.0,
            date_size: 13.0,
            total_size: 16.0,
            radius: 0.0,
            stroke: false,
        },
    )?;

    // Row 4: calendar heatmap.
    draw_heatmap(
        &mut canvas,
        graph,
        palette,
        &Heatmap {
            panel: (margin, 322.0, width - 2.0 * margin, 132.0),
            grid_x: 96.0,
            grid_y: 338.0,
            cell: 14.0,
            gap: 3.0,
            weeks_limit: 40,
            radius: 10.0,
            label_size: 12.0,
        },
    )?;

    // Row 5: up to four milestones in two columns.
    draw_milestones(
        &mut canvas,
        graph,
        palette,
        &[margin, 430.0],
        &[472.0, 490.0],
        13.0,
    );

    // Row 6: contributors, names and counts beneath one stacked bar.
    draw_contributors(
        &mut canvas,
        graph,
        palette,
        ContributorLayout {
            x: margin,
            max_right: width - margin,
            bar_y: 494.0,
            bar_height: 8.0,
            text_baseline: 515.0,
            size: 13.0,
        },
    );

    let style = style_block(canvas.stair_length);
    Ok(canvas.finish(
        width as u32,
        height as u32,
        "CLIMB commit graph",
        &desc_text(graph),
        &style,
        palette,
    ))
}

/// Narrow card: 400 x 640, shown at 390 px on a phone.
pub(crate) fn render_narrow(graph: &CommitGraph, palette: &Palette) -> Result<String> {
    let font = Font::load()?;
    let mut canvas = Canvas::new(&font);
    let (width, height) = (400.0, 640.0);
    let margin = 20.0;
    canvas.rect(
        1.0,
        1.0,
        width - 2.0,
        height - 2.0,
        &palette.background,
        Some(&palette.border),
        18.0,
    );

    canvas.text_group(
        "title",
        22.0,
        &palette.ink,
        &[("The Climb".to_string(), margin, 34.0)],
    );
    let sha = &graph.as_of_sha[..7.min(graph.as_of_sha.len())];
    let stamp = format!("as of {sha} - {}", graph.as_of_date);
    canvas.text_group(
        "stamp",
        14.0,
        &palette.muted,
        &[(
            stamp.clone(),
            width - margin - canvas.width(&stamp, 14.0),
            33.0,
        )],
    );

    // 2 x 2 KPI tiles. Each row is its own pair of text groups, so a label
    // never shares a bounding box with the numbers below it.
    let tile_w = (width - 2.0 * margin - 8.0) / 2.0;
    let tile_h = 76.0;
    let (labels, values) = kpi_tiles(graph, true);
    for index in 0..4 {
        let column = index % 2;
        let row = index / 2;
        let x = margin + column as f64 * (tile_w + 8.0);
        let y = 50.0 + row as f64 * (tile_h + 10.0);
        canvas.rect(
            x,
            y,
            tile_w,
            tile_h,
            &palette.panel,
            Some(&palette.panel_stroke),
            10.0,
        );
    }
    for row in 0..2 {
        let mut label_runs = Vec::new();
        let mut value_runs = Vec::new();
        for column in 0..2 {
            let index = row * 2 + column;
            let x = margin + column as f64 * (tile_w + 8.0);
            let y = 50.0 + row as f64 * (tile_h + 10.0);
            label_runs.push((labels[index].clone(), x + 8.0, y + 18.0));
            value_runs.push((values[index].clone(), x + 8.0, y + 56.0));
        }
        canvas.text_group("kpi-labels", 14.0, &palette.muted, &label_runs);
        canvas.text_group("kpi-values", 34.0, &palette.ink, &value_runs);
    }

    draw_staircase(
        &mut canvas,
        graph,
        palette,
        &Staircase {
            panel: (margin, 222.0, width - 2.0 * margin, 150.0),
            plot_left: 48.0,
            plot_right: 380.0,
            plot_top: 252.0,
            plot_bottom: 342.0,
            date_baseline: 362.0,
            tick_size: 13.0,
            date_size: 13.0,
            total_size: 14.0,
            radius: 10.0,
            stroke: true,
        },
    )?;

    draw_heatmap(
        &mut canvas,
        graph,
        palette,
        &Heatmap {
            panel: (margin, 382.0, width - 2.0 * margin, 142.0),
            grid_x: 58.0,
            grid_y: 402.0,
            cell: 14.0,
            gap: 3.0,
            weeks_limit: 10,
            radius: 10.0,
            label_size: 13.0,
        },
    )?;

    draw_milestones(
        &mut canvas,
        graph,
        palette,
        &[margin],
        &[543.0, 563.0, 583.0, 603.0],
        14.0,
    );

    draw_contributors(
        &mut canvas,
        graph,
        palette,
        ContributorLayout {
            x: margin,
            max_right: width - margin,
            bar_y: 612.0,
            bar_height: 7.0,
            text_baseline: 633.0,
            size: 14.0,
        },
    );

    let style = style_block(canvas.stair_length);
    Ok(canvas.finish(
        width as u32,
        height as u32,
        "CLIMB commit graph (narrow)",
        &desc_text(graph),
        &style,
        palette,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit_graph::{
        format_date, AuthorEntry, CommitGraphConfig, DayEntry, GraphMilestone, GraphRange,
        GraphTotals,
    };

    #[test]
    fn axis_maximum_is_readable_and_covers_every_commit() {
        assert_eq!(nice_max(74), 80);
        assert_eq!(nice_max(100), 100);
        assert_eq!(nice_max(1), 1);
        assert_eq!(nice_max(0), 1);
        assert_eq!(nice_max(5), 6);
        assert_eq!(nice_max(1234), 1500);
        for value in 0..500 {
            assert!(nice_max(value) >= value, "value {value}");
            // One doubling is the most slack a rounded axis should add.
            assert!(nice_max(value) <= (value * 2).max(2), "value {value}");
        }
    }

    #[test]
    fn thousands_separator_groups_from_the_right() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn heat_levels_saturate() {
        assert_eq!(heat_level(0), 0);
        assert_eq!(heat_level(1), 1);
        assert_eq!(heat_level(3), 3);
        assert_eq!(heat_level(4), 4);
        assert_eq!(heat_level(5), 4);
        assert_eq!(heat_level(6), 5);
        assert_eq!(heat_level(60), 5);
    }

    #[test]
    fn heatmap_weeks_align_to_monday_and_sunday() {
        let start = parse_iso_date("2026-09-26").unwrap();
        let end = parse_iso_date("2026-10-22").unwrap();
        let (first, weeks) = heatmap_weeks(start, end);
        assert_eq!(format_date(first), "2026-09-21");
        // 2026-09-21 .. 2026-10-25 is five Monday-to-Sunday columns.
        assert_eq!(weeks, 5);
    }

    fn sample_graph() -> CommitGraph {
        let start = parse_iso_date("2026-10-10").unwrap();
        let per_day = (0..3)
            .map(|offset| DayEntry {
                date: format_date(start + offset),
                commits: offset as u64,
                added: 10,
                removed: 2,
                authors: if offset == 0 {
                    vec![]
                } else {
                    vec!["Pratyay".to_string()]
                },
            })
            .collect();
        CommitGraph {
            schema_version: 1,
            as_of_sha: "0123456789abcdef0123456789abcdef01234567".to_string(),
            as_of_date: "2026-10-12".to_string(),
            timezone: "Asia/Kolkata".to_string(),
            range: GraphRange {
                start: "2026-10-10".to_string(),
                end: "2026-10-12".to_string(),
            },
            totals: GraphTotals {
                commits: 3,
                refresh_commits: 1,
                active_days: 2,
                longest_streak: 2,
                current_streak: 2,
                lines_added: 30,
                lines_removed: 6,
                files_touched: 3,
            },
            per_day,
            per_author: vec![AuthorEntry {
                display: "Pratyay".to_string(),
                commits: 3,
            }],
            milestones: vec![GraphMilestone {
                date: "2026-10-20".to_string(),
                label: "Level 2 qualified".to_string(),
                future: true,
            }],
            config_hash: "0".repeat(64),
        }
    }

    #[test]
    fn animation_degrades_to_a_complete_static_card() {
        let palette = Palette::default();
        let graph = sample_graph();
        for card in [
            render_full(&graph, &palette).expect("full card"),
            render_narrow(&graph, &palette).expect("narrow card"),
        ] {
            assert!(card.contains("@keyframes climb-draw"));
            assert!(card.contains("@keyframes climb-pulse"));
            assert!(card.contains("prefers-reduced-motion:reduce"));
            // The attribute, not the keyframes, draws the finished line in a
            // viewer that ignores CSS animations.
            assert!(card.contains("stroke-dashoffset=\"0\""));
            assert!(!card.contains("<script"));
        }
    }

    #[test]
    fn desc_carries_every_headline_number_as_a_token() {
        let desc = desc_text(&sample_graph());
        for token in [
            "commits=3",
            "refresh_commits=1",
            "active_days=2",
            "longest_streak=2",
            "current_streak=2",
            "lines_added=30",
            "lines_removed=6",
            "files_touched=3",
            "as_of=0123456",
            "date=2026-10-12",
        ] {
            assert!(desc.contains(token), "missing {token} in {desc}");
        }
    }

    /// One text group parsed from a rendered card.
    struct Group {
        name: String,
        size: f64,
        fill: String,
        runs: Vec<TextBox>,
    }

    /// One data mark: a rectangle, a circle or a polyline segment.
    struct Mark {
        kind: String,
        rect: Option<TextBox>,
        segment: Option<((f64, f64), (f64, f64))>,
    }

    struct ParsedCard {
        view_box: (f64, f64, f64, f64),
        groups: Vec<Group>,
        rects: Vec<(TextBox, String)>,
        marks: Vec<Mark>,
    }

    fn attr_box(node: roxmltree::Node) -> TextBox {
        let values: Vec<f64> = node
            .attribute("data-box")
            .expect("text carries a data-box")
            .split_whitespace()
            .map(|value| value.parse().expect("a numeric data-box"))
            .collect();
        TextBox {
            x: values[0],
            y: values[1],
            width: values[2],
            height: values[3],
        }
    }

    fn node_box(node: roxmltree::Node) -> TextBox {
        match node.tag_name().name() {
            "circle" => {
                let cx: f64 = node.attribute("cx").unwrap_or("0").parse().unwrap();
                let cy: f64 = node.attribute("cy").unwrap_or("0").parse().unwrap();
                let r: f64 = node.attribute("r").unwrap_or("0").parse().unwrap();
                TextBox {
                    x: cx - r,
                    y: cy - r,
                    width: 2.0 * r,
                    height: 2.0 * r,
                }
            }
            _ => TextBox {
                x: node.attribute("x").unwrap_or("0").parse().unwrap(),
                y: node.attribute("y").unwrap_or("0").parse().unwrap(),
                width: node.attribute("width").unwrap_or("0").parse().unwrap(),
                height: node.attribute("height").unwrap_or("0").parse().unwrap(),
            },
        }
    }

    /// Split an absolute `M`/`L` path into its line segments.
    fn line_segments(path: &str) -> Vec<((f64, f64), (f64, f64))> {
        let normalized = path.replace('M', " M ").replace('L', " L ");
        let mut command = "";
        let mut pending = Vec::new();
        let mut last: Option<(f64, f64)> = None;
        let mut segments = Vec::new();
        for token in normalized.split_whitespace() {
            if token == "M" || token == "L" {
                command = token;
                continue;
            }
            pending.push(token.parse::<f64>().expect("a path coordinate"));
            if pending.len() == 2 {
                let point = (pending[0], pending[1]);
                if command == "L" {
                    if let Some(previous) = last {
                        segments.push((previous, point));
                    }
                }
                last = Some(point);
                pending.clear();
            }
        }
        segments
    }

    fn parse_card(card: &str) -> ParsedCard {
        let document = roxmltree::Document::parse(card).expect("card is XML");
        let root = document.root_element();
        let view: Vec<f64> = root
            .attribute("viewBox")
            .expect("a viewBox")
            .split_whitespace()
            .map(|value| value.parse().expect("a numeric viewBox"))
            .collect();
        let mut groups = Vec::new();
        let mut rects = Vec::new();
        let mut marks = Vec::new();
        for node in document.descendants() {
            let classes: Vec<&str> = node
                .attribute("class")
                .unwrap_or("")
                .split_whitespace()
                .collect();
            if classes.contains(&"cg-text-group") {
                let runs = node
                    .descendants()
                    .filter(|child| {
                        child
                            .attribute("class")
                            .unwrap_or("")
                            .split_whitespace()
                            .any(|class| class == "cg-run")
                    })
                    .map(attr_box)
                    .collect();
                groups.push(Group {
                    name: node.attribute("data-name").unwrap_or("").to_string(),
                    size: node
                        .attribute("data-size")
                        .expect("a text group carries its size")
                        .parse()
                        .expect("a numeric text size"),
                    fill: node.attribute("fill").unwrap_or("").to_string(),
                    runs,
                });
            }
            if node.has_tag_name("rect") {
                let fill = node.attribute("fill").unwrap_or("").to_string();
                let rect = node_box(node);
                if !fill.is_empty() && fill.starts_with('#') {
                    rects.push((rect, fill));
                }
                if classes.contains(&"dm") {
                    marks.push(Mark {
                        kind: classes
                            .iter()
                            .find(|class| class.starts_with("dm-") && **class != "dm-line")
                            .copied()
                            .unwrap_or("dm")
                            .to_string(),
                        rect: Some(rect),
                        segment: None,
                    });
                }
            }
            if node.has_tag_name("circle") && classes.contains(&"dm") {
                marks.push(Mark {
                    kind: "dm-circle".to_string(),
                    rect: Some(node_box(node)),
                    segment: None,
                });
            }
            if node.has_tag_name("path") && classes.contains(&"dm-line") {
                for segment in line_segments(node.attribute("d").unwrap_or("")) {
                    marks.push(Mark {
                        kind: "dm-line".to_string(),
                        rect: None,
                        segment: Some(segment),
                    });
                }
            }
        }
        ParsedCard {
            view_box: (view[0], view[1], view[2], view[3]),
            groups,
            rects,
            marks,
        }
    }

    fn parse_hex(colour: &str) -> (f64, f64, f64) {
        let digits = colour.trim_start_matches('#');
        assert_eq!(digits.len(), 6, "expected a six-digit hex colour: {colour}");
        let channel = |start: usize| {
            let value = u8::from_str_radix(&digits[start..start + 2], 16).expect("hex");
            f64::from(value) / 255.0
        };
        (channel(0), channel(2), channel(4))
    }

    fn luminance(colour: &str) -> f64 {
        let (r, g, b) = parse_hex(colour);
        let linear = |value: f64| {
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (first, second) = (luminance(a), luminance(b));
        let (high, low) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (high + 0.05) / (low + 0.05)
    }

    fn background_at(card: &ParsedCard, point: (f64, f64)) -> String {
        let mut background = "#000000".to_string();
        for (rect, fill) in &card.rects {
            if point.0 >= rect.x
                && point.0 <= rect.right()
                && point.1 >= rect.y
                && point.1 <= rect.bottom()
            {
                background = fill.clone();
            }
        }
        background
    }

    fn segment_hits_box(segment: ((f64, f64), (f64, f64)), box_: &TextBox) -> bool {
        let ((x0, y0), (x1, y1)) = segment;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let p = [-dx, dx, -dy, dy];
        let q = [
            x0 - box_.x,
            box_.right() - x0,
            y0 - box_.y,
            box_.bottom() - y0,
        ];
        let mut enter = 0.0f64;
        let mut leave = 1.0f64;
        for index in 0..4 {
            if p[index].abs() < f64::EPSILON {
                if q[index] < 0.0 {
                    return false;
                }
            } else {
                let ratio = q[index] / p[index];
                if p[index] < 0.0 {
                    enter = enter.max(ratio);
                } else {
                    leave = leave.min(ratio);
                }
                if enter > leave {
                    return false;
                }
            }
        }
        true
    }

    /// The contract from the phase brief, computed from the card itself:
    /// effective sizes at the display width, WCAG contrast against the surface
    /// under each run, the group cap, the body/title/KPI floors, no text on a
    /// data mark and at least eight pixels between groups.
    fn check_readability(
        card: &str,
        view_box: (f64, f64),
        display_width: f64,
        body_min: f64,
    ) -> Vec<String> {
        let parsed = parse_card(card);
        let mut problems = Vec::new();
        if (parsed.view_box.2, parsed.view_box.3) != view_box {
            problems.push(format!(
                "viewBox is {}x{}, expected {}x{}",
                parsed.view_box.2, parsed.view_box.3, view_box.0, view_box.1
            ));
        }
        if parsed.groups.len() > 14 {
            problems.push(format!(
                "{} text groups, at most 14 allowed",
                parsed.groups.len()
            ));
        }
        let scale = display_width / parsed.view_box.2;
        let body = ["stamp", "kpi-labels", "milestones", "contributors"];
        for group in &parsed.groups {
            let effective = group.size * scale;
            if effective < 12.0 - 1e-6 {
                problems.push(format!(
                    "{}: effective {effective:.2} px is below 12",
                    group.name
                ));
            }
            if body.contains(&group.name.as_str()) && effective < body_min - 1e-6 {
                problems.push(format!(
                    "{}: effective {effective:.2} px is below the body floor {body_min}",
                    group.name
                ));
            }
            if group.name == "title" && effective < 20.0 - 1e-6 {
                problems.push(format!("title: effective {effective:.2} px is below 20"));
            }
            if group.name == "kpi-values" && effective < 32.0 - 1e-6 {
                problems.push(format!(
                    "kpi-values: effective {effective:.2} px is below 32"
                ));
            }
            for run in &group.runs {
                let centre = (run.x + run.width / 2.0, run.y + run.height / 2.0);
                let background = background_at(&parsed, centre);
                let ratio = contrast(&group.fill, &background);
                if ratio < 4.5 - 1e-6 {
                    problems.push(format!(
                        "{}: {:.2}:1 contrast against {background} is below 4.5:1",
                        group.name, ratio
                    ));
                }
                for mark in &parsed.marks {
                    let hit = match &mark.rect {
                        Some(rect) => run.intersects(rect),
                        None => mark
                            .segment
                            .is_some_and(|segment| segment_hits_box(segment, run)),
                    };
                    if hit {
                        problems.push(format!(
                            "{}: text overlaps a {} mark",
                            group.name, mark.kind
                        ));
                    }
                }
            }
        }
        for (index, first) in parsed.groups.iter().enumerate() {
            let first_box = union_of(&first.runs);
            for second in parsed.groups.iter().skip(index + 1) {
                let second_box = union_of(&second.runs);
                let gap = first_box.gap(&second_box);
                if gap < 8.0 - 1e-6 {
                    problems.push(format!(
                        "{} and {} are {gap:.2} px apart, below 8",
                        first.name, second.name
                    ));
                }
            }
        }
        problems
    }

    fn union_of(runs: &[TextBox]) -> TextBox {
        runs.iter()
            .copied()
            .reduce(TextBox::union)
            .expect("a group has at least one run")
    }

    #[test]
    fn the_wide_card_meets_the_readability_contract() {
        let card = render_full(&sample_graph(), &Palette::default()).expect("full card");
        let problems = check_readability(&card, (840.0, 520.0), 840.0, 13.0);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn the_narrow_card_meets_the_readability_contract() {
        let card = render_narrow(&sample_graph(), &Palette::default()).expect("narrow card");
        let problems = check_readability(&card, (400.0, 640.0), 390.0, 13.0);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn the_committed_graph_cards_meet_the_readability_contract() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives inside the workspace");
        let text = std::fs::read_to_string(root.join("docs/media/commit-graph.json"))
            .expect("the committed graph");
        let graph: CommitGraph = serde_json::from_str(&text).expect("valid graph");
        let palette = Palette::default();
        let committed_wide =
            std::fs::read_to_string(root.join("docs/media/commit-graph.svg")).expect("wide card");
        let committed_narrow =
            std::fs::read_to_string(root.join("docs/media/commit-graph-narrow.svg"))
                .expect("narrow card");
        let wide = render_full(&graph, &palette).expect("full card");
        let narrow = render_narrow(&graph, &palette).expect("narrow card");
        assert_eq!(committed_wide, wide, "the committed wide card has drifted");
        assert_eq!(
            committed_narrow, narrow,
            "the committed narrow card has drifted"
        );
        let wide_problems = check_readability(&committed_wide, (840.0, 520.0), 840.0, 13.0);
        let narrow_problems = check_readability(&committed_narrow, (400.0, 640.0), 390.0, 13.0);
        assert!(wide_problems.is_empty(), "{wide_problems:#?}");
        assert!(narrow_problems.is_empty(), "{narrow_problems:#?}");
        assert_eq!(parse_card(&committed_wide).groups.len(), 12);
        assert_eq!(parse_card(&committed_narrow).groups.len(), 14);
    }

    #[test]
    fn both_cards_describe_the_same_committed_numbers() {
        let graph = sample_graph();
        let palette = Palette::default();
        let full = render_full(&graph, &palette).expect("full card");
        let narrow = render_narrow(&graph, &palette).expect("narrow card");
        let desc = desc_text(&graph);
        assert!(full.contains(&desc) && narrow.contains(&desc));
    }

    #[test]
    fn a_missing_brand_file_falls_back_to_the_sampled_palette() {
        let palette = Palette::load(std::path::Path::new("/definitely/not/a/repo")).unwrap();
        assert_eq!(palette.orange, "#C55E33");
        assert_eq!(palette.teal, "#5C8E8D");
    }

    #[test]
    fn the_config_defaults_keep_staleness_bounded() {
        let config: CommitGraphConfig =
            toml::from_str("timezone = \"Asia/Kolkata\"\nbudget_kb = 50\n").unwrap();
        assert_eq!(config.staleness_commits, 20);
        assert_eq!(config.staleness_days, 7);
        assert_eq!(
            config.source_url,
            "https://github.com/aiori-hackathon/A3-PS036-TC529.git"
        );
        assert_eq!(config.source_ref, "main");
        assert_eq!(config.source_cache, "target/commit-graph-source.git");
        assert_eq!(
            config.repo_web_url,
            "https://github.com/aiori-hackathon/A3-PS036-TC529"
        );
    }
}
