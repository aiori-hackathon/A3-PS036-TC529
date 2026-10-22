//! Rendering of the two self-contained commit-graph cards.
//!
//! The layout is deliberately fixed (880 points wide) so the cards keep their
//! proportions inside a README column. Every string is drawn as outlines, no
//! element points at a file or a URL, and the one animation is CSS inside the
//! document. The final state, with animation disabled, is the complete card.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::text::{glyph_defs, text_markup, text_width, Font};
use super::{parse_iso_date, CommitGraph, GraphMilestone};

/// Colours sampled from the artwork and recorded in `docs/media/brand.toml`.
pub(crate) struct Palette {
    background: String,
    panel: String,
    panel_stroke: String,
    border: String,
    ink: String,
    muted: String,
    faint: String,
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
            faint: "#6E7681".to_string(),
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
}

impl<'a> Canvas<'a> {
    fn new(font: &'a Font) -> Self {
        Self {
            font,
            body: String::new(),
            used: BTreeSet::new(),
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

    fn text(&mut self, text: &str, x: f64, y: f64, size: f64, fill: &str, tracking: f64) {
        let markup = text_markup(self.font, text, x, y, size, fill, tracking, &mut self.used);
        self.body.push_str(&markup);
    }

    fn text_right(&mut self, text: &str, right: f64, y: f64, size: f64, fill: &str) {
        let width = text_width(self.font, text, size, 0.0);
        self.text(text, right - width, y, size, fill, 0.0);
    }

    fn text_center(&mut self, text: &str, center: f64, y: f64, size: f64, fill: &str) {
        let width = text_width(self.font, text, size, 0.0);
        self.text(text, center - width / 2.0, y, size, fill, 0.0);
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

fn initials(display: &str) -> String {
    let mut out = String::new();
    for word in display.split_whitespace().take(2) {
        if let Some(character) = word.chars().next() {
            out.extend(character.to_uppercase());
        }
    }
    if out.is_empty() {
        "?".to_string()
    } else {
        out
    }
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
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
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

/// Full card: staircase, heatmap, KPIs, milestones, member split and stamp.
pub(crate) fn render_full(graph: &CommitGraph, palette: &Palette) -> Result<String> {
    let font = Font::load()?;
    let mut canvas = Canvas::new(&font);
    let width = 880.0;
    let height = 360.0;
    let start = parse_iso_date(&graph.range.start)?;
    let end = parse_iso_date(&graph.range.end)?;
    let milestone_days: Vec<(i64, &GraphMilestone)> = graph
        .milestones
        .iter()
        .map(|milestone| Ok((parse_iso_date(&milestone.date)?, milestone)))
        .collect::<Result<Vec<_>>>()?;
    let domain_end = milestone_days
        .iter()
        .map(|(day, _)| *day)
        .max()
        .unwrap_or(end)
        .max(end);

    canvas.rect(
        1.0,
        1.0,
        width - 2.0,
        height - 2.0,
        &palette.background,
        Some(&palette.border),
        16.0,
    );

    // Header: title, subtitle and a small ascending-rung motif.
    canvas.text("THE CLIMB", 24.0, 42.0, 22.0, &palette.ink, 0.03);
    canvas.text(
        "COMMIT GRAPH / CUMULATIVE CLIMB PER ACTIVE DAY",
        24.0,
        58.0,
        8.0,
        &palette.muted,
        0.07,
    );
    for (index, bar_height) in [5.0, 8.0, 11.0, 15.0, 20.0].into_iter().enumerate() {
        let x = 796.0 + index as f64 * 13.0;
        let colour = if index < 2 {
            palette.orange.clone()
        } else {
            palette.teal.clone()
        };
        canvas.rect(x, 40.0 - bar_height, 6.0, bar_height, &colour, None, 1.5);
    }

    let chips = [
        ("COMMITS", thousands(graph.totals.commits)),
        ("ACTIVE DAYS", thousands(graph.totals.active_days)),
        (
            "LONGEST STREAK",
            format!("{}d", graph.totals.longest_streak),
        ),
    ];
    let chip_width = 200.5;
    for (index, (label, value)) in chips.iter().enumerate() {
        let x = 24.0 + index as f64 * (chip_width + 10.0);
        canvas.rect(
            x,
            68.0,
            chip_width,
            44.0,
            &palette.panel,
            Some(&palette.panel_stroke),
            8.0,
        );
        canvas.text(label, x + 12.0, 84.0, 7.2, &palette.muted, 0.08);
        canvas.text(value, x + 12.0, 102.0, 17.0, &palette.ink, 0.0);
    }
    let x = 24.0 + 3.0 * (chip_width + 10.0);
    canvas.rect(
        x,
        68.0,
        chip_width,
        44.0,
        &palette.panel,
        Some(&palette.panel_stroke),
        8.0,
    );
    canvas.text(
        "LINES ADDED / REMOVED",
        x + 12.0,
        84.0,
        7.2,
        &palette.muted,
        0.08,
    );
    let added = format!("+{}", thousands(graph.totals.lines_added));
    let removed = format!("-{}", thousands(graph.totals.lines_removed));
    canvas.text(&added, x + 12.0, 102.0, 11.5, &palette.orange, 0.0);
    let added_width = text_width(&font, &added, 11.5, 0.0);
    canvas.text(
        &removed,
        x + 18.0 + added_width,
        102.0,
        11.5,
        &palette.teal,
        0.0,
    );

    // Staircase panel.
    let panel_x = 24.0;
    let panel_y = 126.0;
    let panel_w = 576.0;
    let panel_h = 170.0;
    canvas.rect(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        &palette.panel,
        Some(&palette.panel_stroke),
        10.0,
    );
    canvas.text(
        "CUMULATIVE COMMITS",
        panel_x + 14.0,
        panel_y + 18.0,
        7.2,
        &palette.muted,
        0.08,
    );
    canvas.text_right(
        &format!("as of {}", &graph.as_of_date[5..]),
        panel_x + panel_w - 14.0,
        panel_y + 18.0,
        7.0,
        &palette.faint,
    );

    let plot_left = panel_x + 44.0;
    let plot_right = panel_x + panel_w - 14.0;
    let plot_top = panel_y + 32.0;
    let plot_bottom = panel_y + panel_h - 50.0;
    let plot_height = plot_bottom - plot_top;
    let max = nice_max(graph.totals.commits);
    let day_span = (end - start).max(1) as f64 + 1.0;
    let day_width = (plot_right - plot_left) / day_span;
    let x_of = |day: i64| plot_left + (day - start) as f64 * day_width;
    let y_of = |value: u64| plot_bottom - (value as f64 / max as f64) * plot_height;

    for step in 0..=2 {
        let value = max / 2 * step as u64;
        let y = y_of(value);
        canvas.line(
            plot_left,
            y,
            plot_right,
            y,
            &palette.border,
            0.8,
            if step == 0 { None } else { Some("2 4") },
            if step == 0 { 0.9 } else { 0.45 },
        );
        canvas.text_right(
            &thousands(value),
            plot_left - 8.0,
            y + 2.5,
            6.8,
            &palette.faint,
        );
    }

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
        num(plot_bottom),
        num(x_of(start)),
        num(plot_bottom)
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
        Some("climb-stair"),
        Some(stair_length),
    );

    // Final point: a soft ring and a pulsing dot.
    canvas.circle(
        data_end,
        y_of(cumulative),
        6.0,
        "none",
        Some(&palette.orange),
    );
    canvas.raw(&format!(
        "<circle class=\"climb-now\" cx=\"{}\" cy=\"{}\" r=\"3.4\" fill=\"{}\"/>",
        num(data_end),
        num(y_of(cumulative)),
        palette.orange
    ));

    canvas.text(
        &graph.range.start,
        plot_left,
        plot_bottom + 14.0,
        6.5,
        &palette.faint,
        0.0,
    );
    canvas.text_right(
        &graph.range.end,
        data_end,
        plot_bottom + 14.0,
        6.5,
        &palette.faint,
    );

    // Roadmap rail: the four milestones use their own full-width date scale,
    // so the staircase above keeps one step per calendar day rather than being
    // squeezed to make room for dates that have not happened yet.
    let rail_y = panel_y + panel_h - 16.0;
    let rail_left = plot_left + 38.0;
    let rail_right = plot_right - 6.0;
    let rail_end = domain_end.max(end);
    let rail_span = (rail_end - start).max(1) as f64 + 1.0;
    let rail_width = (rail_right - rail_left) / rail_span;
    canvas.line(
        plot_left + 32.0,
        rail_y,
        rail_right,
        rail_y,
        &palette.border,
        0.8,
        None,
        0.8,
    );
    canvas.text("ROADMAP", plot_left, rail_y + 2.2, 5.4, &palette.faint, 0.1);
    for (number, (day, milestone)) in milestone_days.iter().enumerate() {
        if *day < start {
            continue;
        }
        let centre = rail_left + (*day - start) as f64 * rail_width + rail_width / 2.0;
        let colour = if milestone.future {
            palette.faint.clone()
        } else {
            palette.orange.clone()
        };
        canvas.line(
            centre,
            rail_y - 3.0,
            centre,
            rail_y + 3.0,
            &colour,
            0.9,
            Some(if milestone.future { "1 2" } else { "2 2" }),
            if milestone.future { 0.45 } else { 0.8 },
        );
        if milestone.future {
            canvas.raw(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"4.4\" fill=\"{}\" stroke=\"{}\" \
                 stroke-width=\"1\" stroke-dasharray=\"1.5 1.5\"/>",
                num(centre),
                num(rail_y),
                palette.panel,
                palette.faint
            ));
        } else {
            canvas.circle(centre, rail_y, 4.4, &palette.panel, Some(&colour));
        }
        canvas.text_center(
            &format!("{}", number + 1),
            centre,
            rail_y + 1.9,
            5.6,
            &colour,
        );
    }

    // Calendar heatmap panel.
    let activity_x = 612.0;
    let activity_y = 126.0;
    let activity_w = 244.0;
    let activity_h = 170.0;
    canvas.rect(
        activity_x,
        activity_y,
        activity_w,
        activity_h,
        &palette.panel,
        Some(&palette.panel_stroke),
        10.0,
    );
    canvas.text(
        "ACTIVITY",
        activity_x + 14.0,
        activity_y + 18.0,
        7.2,
        &palette.muted,
        0.08,
    );
    canvas.text_right(
        &format!("{} / {}", graph.totals.active_days, graph.totals.commits),
        activity_x + activity_w - 14.0,
        activity_y + 18.0,
        7.0,
        &palette.faint,
    );

    let (first_monday, weeks) = heatmap_weeks(start, end);
    let cell = (14.0f64).min((activity_w - 58.0 - (weeks as f64 - 1.0) * 2.5) / weeks as f64);
    let visible_weeks = if cell < 4.0 {
        ((activity_w - 58.0) / 6.5).floor() as usize
    } else {
        weeks
    };
    let cell = cell.max(4.0);
    let gap = 2.5;
    let columns = visible_weeks as f64;
    let grid_w = columns * cell + (columns - 1.0) * gap;
    // Centre the weekday gutter and the grid together, so the block does not
    // drift to one side of the panel.
    let gutter = 18.0;
    let block = gutter + grid_w;
    let free = (activity_w - block).max(0.0);
    let grid_x = activity_x + free / 2.0 + gutter;
    let grid_y = activity_y + 34.0;
    let hidden_weeks = weeks.saturating_sub(visible_weeks);
    let first_column_day = first_monday + hidden_weeks as i64 * 7;

    for row in 0..7 {
        let label = match row {
            0 => "MON",
            2 => "WED",
            4 => "FRI",
            6 => "SUN",
            _ => "",
        };
        if !label.is_empty() {
            canvas.text_right(
                label,
                grid_x - 6.0,
                grid_y + row as f64 * (cell + gap) + cell * 0.75,
                5.8,
                &palette.faint,
            );
        }
    }

    for week in 0..visible_weeks {
        let monday = first_column_day + week as i64 * 7;
        let month_start = (monday..monday + 7).find(|day| super::civil_from_days(*day).2 == 1);
        if week == 0 || month_start.is_some() {
            let month = super::civil_from_days(month_start.unwrap_or(monday)).1;
            canvas.text(
                month_name(month),
                grid_x + week as f64 * (cell + gap),
                grid_y - 5.0,
                5.8,
                &palette.faint,
                0.06,
            );
        }
        for row in 0..7 {
            let day = monday + row as i64;
            if day < start || day > end {
                continue;
            }
            let index = (day - start) as usize;
            let Some(entry) = graph.per_day.get(index) else {
                continue;
            };
            let level = heat_level(entry.commits);
            let x = grid_x + week as f64 * (cell + gap);
            let y = grid_y + row as f64 * (cell + gap);
            if level == 0 {
                canvas.rect(
                    x,
                    y,
                    cell,
                    cell,
                    &palette.panel,
                    Some(&palette.panel_stroke),
                    2.0,
                );
            } else {
                canvas.rect(x, y, cell, cell, &palette.heat[level - 1], None, 2.0);
            }
        }
    }

    // Heat legend, centred under the grid.
    let legend_y = grid_y + 7.0 * (cell + gap) + 2.0;
    let size = 5.8;
    let less = text_width(&font, "LESS", size, 0.05);
    let more = text_width(&font, "MORE", size, 0.05);
    let swatches = (palette.heat.len() as f64 - 1.0) * 10.0 + 7.0;
    let legend_w = less + 5.0 + swatches + 5.0 + more;
    let legend_x = grid_x + (grid_w - legend_w) / 2.0;
    canvas.text("LESS", legend_x, legend_y + 4.0, size, &palette.faint, 0.05);
    let swatch_x = legend_x + less + 5.0;
    for (index, colour) in palette.heat.iter().enumerate() {
        canvas.rect(
            swatch_x + index as f64 * 10.0,
            legend_y,
            7.0,
            7.0,
            colour,
            None,
            1.5,
        );
    }
    canvas.text(
        "MORE",
        swatch_x + swatches + 5.0,
        legend_y + 4.0,
        size,
        &palette.faint,
        0.05,
    );
    if hidden_weeks > 0 {
        let note = format!("last {visible_weeks}w");
        let note_w = text_width(&font, &note, size, 0.0);
        canvas.text(
            &note,
            grid_x + (grid_w - note_w) / 2.0,
            legend_y + 14.0,
            size,
            &palette.faint,
            0.0,
        );
    }

    // Member split: thin bars, one per author, with initials.
    canvas.text("CONTRIBUTORS", 24.0, 310.0, 7.0, &palette.muted, 0.08);
    let bar_x = 122.0;
    let bar_width = 300.0;
    let bar_y = 303.0;
    let total = graph.totals.commits.max(1) as f64;
    let mut cursor = bar_x;
    let mut legend_x = bar_x + bar_width + 14.0;
    for (index, author) in graph.per_author.iter().enumerate() {
        let share = author.commits as f64 / total;
        let segment = (bar_width * share).max(2.0);
        let colour = if index % 2 == 0 {
            palette.orange.clone()
        } else {
            palette.teal.clone()
        };
        canvas.rect(cursor, bar_y, segment, 4.5, &colour, None, 2.0);
        canvas.text(
            &format!("{} {}", initials(&author.display), author.commits),
            legend_x,
            310.0,
            7.0,
            &palette.muted,
            0.0,
        );
        legend_x += text_width(
            &font,
            &format!("{} {}", initials(&author.display), author.commits),
            7.0,
            0.0,
        ) + 12.0;
        cursor += segment + 1.5;
    }

    // Milestone legend.
    let mut legend_x = 24.0;
    for (number, (_, milestone)) in milestone_days.iter().enumerate() {
        let colour = if milestone.future {
            palette.faint.clone()
        } else {
            palette.muted.clone()
        };
        canvas.circle(
            legend_x + 4.5,
            323.0,
            4.5,
            &palette.panel,
            Some(if milestone.future {
                palette.faint.as_str()
            } else {
                palette.orange.as_str()
            }),
        );
        canvas.text_center(
            &format!("{}", number + 1),
            legend_x + 4.5,
            325.2,
            5.6,
            if milestone.future {
                &palette.faint
            } else {
                &palette.orange
            },
        );
        let label = format!("{} {}", milestone.date, milestone.label);
        canvas.text(&label, legend_x + 13.0, 325.0, 7.0, &colour, 0.0);
        legend_x += 13.0 + text_width(&font, &label, 7.0, 0.0) + 16.0;
    }

    canvas.line(
        24.0,
        334.0,
        width - 24.0,
        334.0,
        &palette.panel_stroke,
        0.8,
        None,
        0.8,
    );
    canvas.text(
        &format!(
            "as of {} - {} - of which {} graph refreshes",
            &graph.as_of_sha[..7.min(graph.as_of_sha.len())],
            graph.as_of_date,
            graph.totals.refresh_commits
        ),
        24.0,
        347.0,
        7.5,
        &palette.faint,
        0.0,
    );
    canvas.text_right(
        "generated from git history",
        width - 24.0,
        347.0,
        7.5,
        &palette.faint,
    );

    let style = style_block(stair_length);
    Ok(canvas.finish(
        880,
        360,
        "CLIMB commit graph",
        &desc_text(graph),
        &style,
        palette,
    ))
}

/// Compact card: a one-glance strip with the staircase, heatmap, KPIs and split.
pub(crate) fn render_compact(graph: &CommitGraph, palette: &Palette) -> Result<String> {
    let font = Font::load()?;
    let mut canvas = Canvas::new(&font);
    let start = parse_iso_date(&graph.range.start)?;
    let end = parse_iso_date(&graph.range.end)?;

    canvas.rect(
        1.0,
        1.0,
        878.0,
        118.0,
        &palette.background,
        Some(&palette.border),
        14.0,
    );

    canvas.text("THE CLIMB", 22.0, 27.0, 15.0, &palette.ink, 0.03);
    canvas.text("COMMIT GRAPH", 22.0, 39.0, 6.4, &palette.muted, 0.08);

    let totals = &graph.totals;
    let kpi = format!(
        "{} COMMITS / {} ACTIVE DAYS / {}d STREAK / LINES +{} -{}",
        thousands(totals.commits),
        thousands(totals.active_days),
        totals.longest_streak,
        thousands(totals.lines_added),
        thousands(totals.lines_removed)
    );
    canvas.text(&kpi, 138.0, 27.0, 7.4, &palette.muted, 0.03);

    let plot_left = 22.0;
    let plot_right = 566.0;
    let plot_top = 54.0;
    let plot_bottom = 96.0;
    let plot_height = plot_bottom - plot_top;
    let max = nice_max(totals.commits);
    let day_span = (end - start).max(1) as f64 + 1.0;
    let day_width = (plot_right - plot_left) / day_span;
    let x_of = |day: i64| plot_left + (day - start) as f64 * day_width;
    let y_of = |value: u64| plot_bottom - (value as f64 / max as f64) * plot_height;

    canvas.line(
        plot_left,
        plot_bottom,
        plot_right,
        plot_bottom,
        &palette.border,
        0.8,
        None,
        0.8,
    );
    canvas.line(
        plot_left,
        plot_top,
        plot_right,
        plot_top,
        &palette.border,
        0.8,
        Some("2 4"),
        0.4,
    );

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
        num(plot_bottom),
        num(x_of(start)),
        num(plot_bottom)
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
        2.0,
        Some("climb-stair"),
        Some(stair_length),
    );

    canvas.circle(
        data_end,
        y_of(cumulative),
        4.6,
        "none",
        Some(&palette.orange),
    );
    canvas.raw(&format!(
        "<circle class=\"climb-now\" cx=\"{}\" cy=\"{}\" r=\"2.8\" fill=\"{}\"/>",
        num(data_end),
        num(y_of(cumulative)),
        palette.orange
    ));

    canvas.text(
        &graph.range.start,
        plot_left,
        103.0,
        6.0,
        &palette.faint,
        0.0,
    );
    canvas.text_right(&graph.range.end, data_end, 103.0, 6.0, &palette.faint);

    // Heatmap.
    let (first_monday, weeks) = heatmap_weeks(start, end);
    let cell = 5.2f64;
    let gap = 1.6f64;
    let grid_x = 606.0;
    let grid_y = 54.0;
    let fitting_weeks = ((90.0 + gap) / (cell + gap)).floor() as usize;
    let visible_weeks = weeks.min(fitting_weeks.max(1));
    let hidden_weeks = weeks.saturating_sub(visible_weeks);
    let first_column_day = first_monday + hidden_weeks as i64 * 7;
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
            let x = grid_x + week as f64 * (cell + gap);
            let y = grid_y + row as f64 * (cell + gap);
            if level == 0 {
                canvas.rect(
                    x,
                    y,
                    cell,
                    cell,
                    &palette.panel,
                    Some(&palette.panel_stroke),
                    1.2,
                );
            } else {
                canvas.rect(x, y, cell, cell, &palette.heat[level - 1], None, 1.2);
            }
        }
    }
    canvas.text("ACTIVITY", grid_x, grid_y - 4.0, 5.6, &palette.muted, 0.08);

    // Member split, right of the heatmap.
    let members_x = 720.0;
    canvas.text("CONTRIBUTORS", members_x, 48.0, 5.8, &palette.muted, 0.08);
    let bar_width = 138.0;
    let total = totals.commits.max(1) as f64;
    let mut cursor = members_x;
    let mut label = String::new();
    for (index, author) in graph.per_author.iter().enumerate() {
        let segment = (bar_width * author.commits as f64 / total).max(1.5);
        let colour = if index % 2 == 0 {
            palette.orange.clone()
        } else {
            palette.teal.clone()
        };
        canvas.rect(cursor, 54.0, segment, 3.5, &colour, None, 1.5);
        cursor += segment + 1.0;
        if !label.is_empty() {
            label.push_str("  ");
        }
        let _ = write!(
            &mut label,
            "{} {}",
            initials(&author.display),
            author.commits
        );
    }
    canvas.text(&label, members_x, 70.0, 6.2, &palette.muted, 0.0);

    canvas.text(
        &format!(
            "as of {} - {}",
            &graph.as_of_sha[..7.min(graph.as_of_sha.len())],
            graph.as_of_date
        ),
        22.0,
        115.0,
        6.6,
        &palette.faint,
        0.0,
    );
    canvas.text_right(
        &format!("of which {} graph refreshes", graph.totals.refresh_commits),
        858.0,
        115.0,
        6.6,
        &palette.faint,
    );

    let style = style_block(stair_length);
    Ok(canvas.finish(
        880,
        120,
        "CLIMB commit graph (compact)",
        &desc_text(graph),
        &style,
        palette,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit_graph::{
        format_date, AuthorEntry, CommitGraphConfig, DayEntry, GraphRange, GraphTotals,
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
    fn initials_take_the_first_letter_of_two_words() {
        assert_eq!(initials("Pratyay"), "P");
        assert_eq!(initials("Pratyay Mukherjee"), "PM");
        assert_eq!(initials("  "), "?");
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
            render_compact(&graph, &palette).expect("compact card"),
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
