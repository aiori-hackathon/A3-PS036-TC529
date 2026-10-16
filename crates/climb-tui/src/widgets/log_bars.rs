//! Log-scale comparison bars shared by the Signing and Verifying tabs (20.1).
//!
//! Bars are laid out on a log10 scale over the value range of the group set,
//! so a 1,000x difference stays readable: the shortest bar is always at least
//! one cell and the longest fills the bar column. Layout ([`bar_len`],
//! [`decade_ticks`]) is pure and unit-tested; rendering only draws what layout
//! computed. Measured, extrapolated and analytical bars get distinct glyphs
//! and the theme's semantic styles, and a ruler labels the decades in time
//! units via the shared `climb_report` formatter — no number literal lives
//! here.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use climb_report::charts::fmt_time_ns;

use super::ellipsize;
use crate::theme::{Glyphs, Theme};

/// Where a bar's value came from; drives its glyph and style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    /// Directly measured by the benchmark.
    Measured,
    /// Extrapolated from measured per-message cost.
    Extrapolated,
    /// Closed-form analytical model.
    Analytical,
}

/// One bar: a named value on the shared log scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub name: String,
    pub value_ns: f64,
    pub kind: BarKind,
    /// 95% CI bounds (ns), present for measured values.
    pub ci: Option<(f64, f64)>,
}

/// One labelled group of bars (a dataset size).
#[derive(Debug, Clone, PartialEq)]
pub struct BarGroup {
    pub label: String,
    pub bars: Vec<Bar>,
}

/// Bar length in cells for `value` on the log10 scale `min..=max`.
///
/// `value == min` still draws one cell (a bar never vanishes) and
/// `value == max` fills `width`. A degenerate range (non-positive bounds,
/// `max <= min`, or a zero `width`) returns `width` so a single point is drawn
/// full-length and the widget never panics on one-size or zero-size data.
pub fn bar_len(value: f64, min: f64, max: f64, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let valid_range = min.is_finite() && max.is_finite() && min > 0.0 && max > min;
    if !valid_range {
        return width;
    }
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    let lo = min.log10();
    let span = max.log10() - lo;
    if span <= 0.0 {
        return width;
    }
    let frac = ((value.log10() - lo) / span).clamp(0.0, 1.0);
    ((frac * f64::from(width)).round() as u16).clamp(1, width)
}

/// Decade tick marks on the log10 scale `min..=max`: `(column, label)` pairs
/// for every power of ten inside the range, labelled in time units.
pub fn decade_ticks(min: f64, max: f64, width: u16) -> Vec<(u16, String)> {
    let valid_range = min.is_finite() && max.is_finite() && min > 0.0 && max > min;
    if width == 0 || !valid_range {
        return Vec::new();
    }
    let lo = min.log10();
    let hi = max.log10();
    if hi <= lo {
        return Vec::new();
    }
    let span = hi - lo;
    let mut out = Vec::new();
    for k in lo.ceil() as i64..=hi.floor() as i64 {
        let v = 10f64.powi(k as i32);
        let frac = ((v.log10() - lo) / span).clamp(0.0, 1.0);
        let column = (frac * f64::from(width.saturating_sub(1))).round() as u16;
        out.push((column, fmt_time_ns(v)));
    }
    out
}

/// A grouped, log-scale bar comparison inside a bordered block, with a decade
/// ruler and a measured / extrapolated / analytical legend.
pub struct LogBars<'a> {
    pub groups: &'a [BarGroup],
    pub theme: &'a Theme,
    pub glyphs: &'a Glyphs,
    pub title: String,
    /// Width of the name column, including the two-cell indent.
    pub name_width: u16,
    /// Width reserved for the trailing value text.
    pub value_width: u16,
    pub show_legend: bool,
}

impl LogBars<'_> {
    fn bar_glyph(&self, kind: BarKind) -> char {
        match kind {
            BarKind::Measured => self.glyphs.bar_filled,
            BarKind::Extrapolated => self.glyphs.bar_empty,
            // No analytical glyph in the sets; keep it visually distinct in
            // both modes without inventing a new glyph category.
            BarKind::Analytical if self.glyphs.bar_filled == '#' => ':',
            BarKind::Analytical => '\u{2592}',
        }
    }

    fn bar_style(&self, kind: BarKind) -> Style {
        match kind {
            BarKind::Measured => self.theme.measured,
            BarKind::Extrapolated => self.theme.extrapolated,
            BarKind::Analytical => self.theme.analytical,
        }
    }

    /// The body lines: one header per group, one row per bar, then the decade
    /// ruler and the legend. `width` is the block's inner width.
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let name_width = self.name_width.min(width);
        let bar_width = width
            .saturating_sub(name_width)
            .saturating_sub(self.value_width)
            .max(1);

        let values: Vec<f64> = self
            .groups
            .iter()
            .flat_map(|group| group.bars.iter().map(|bar| bar.value_ns))
            .filter(|v| v.is_finite() && *v > 0.0)
            .collect();
        if values.is_empty() {
            return vec![Line::from(Span::styled(
                " no rows recorded",
                Style::new().fg(self.theme.muted),
            ))];
        }
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let ascii = self.theme.ascii;
        let mut lines = Vec::new();
        for group in self.groups {
            // The header carries a leading indent, so its text may occupy one
            // column less than the panel.
            let label = ellipsize(&group.label, width.saturating_sub(1) as usize, ascii);
            lines.push(Line::from(Span::styled(
                format!(" {label}"),
                Style::new()
                    .fg(self.theme.accent2)
                    .add_modifier(Modifier::BOLD),
            )));
            for bar in &group.bars {
                let len = bar_len(bar.value_ns, min, max, bar_width);
                let glyph = self.bar_glyph(bar.kind);
                let bar_cells: String = std::iter::repeat_n(glyph, len as usize).collect();
                let name = format!(
                    " {:<pad$}",
                    ellipsize(&bar.name, name_width.saturating_sub(1) as usize, ascii),
                    pad = (name_width.saturating_sub(1)) as usize
                );
                let style = self.bar_style(bar.kind);
                // The value sits after the name column, the bar column and one
                // separating space, so it may occupy one column less than its
                // reserved width without overflowing the panel.
                let value = ellipsize(
                    &value_text(bar),
                    self.value_width.saturating_sub(1) as usize,
                    ascii,
                );
                lines.push(Line::from(vec![
                    Span::styled(name, Style::new().fg(self.theme.fg)),
                    Span::styled(bar_cells, style),
                    Span::raw(" ".repeat((bar_width - len) as usize)),
                    Span::raw(" "),
                    Span::styled(value, style),
                ]));
            }
        }

        lines.push(ruler_line(
            &decade_ticks(min, max, bar_width),
            bar_width,
            name_width,
            self.theme,
        ));

        if self.show_legend {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    self.bar_glyph(BarKind::Measured).to_string(),
                    self.bar_style(BarKind::Measured),
                ),
                Span::raw(" measured  "),
                Span::styled(
                    self.bar_glyph(BarKind::Extrapolated).to_string(),
                    self.bar_style(BarKind::Extrapolated),
                ),
                Span::raw(" extrapolated  "),
                Span::styled(
                    self.bar_glyph(BarKind::Analytical).to_string(),
                    self.bar_style(BarKind::Analytical),
                ),
                Span::raw(" analytical"),
            ]));
        }
        lines
    }
}

/// The value label rendered at the bar end: scaled time, a ±% CI span for
/// measured values, and a source suffix for non-measured ones.
fn value_text(bar: &Bar) -> String {
    let mut text = fmt_time_ns(bar.value_ns);
    if let Some((lo, hi)) = bar.ci {
        if bar.value_ns > 0.0 && hi >= lo {
            let pct = (hi - lo) / 2.0 / bar.value_ns * 100.0;
            text.push_str(&format!(" \u{00b1}{pct:.0}%"));
        }
    }
    match bar.kind {
        BarKind::Measured => {}
        BarKind::Extrapolated => text.push_str(" (extrapolated)"),
        BarKind::Analytical => text.push_str(" (model)"),
    }
    text
}

/// The ruler under the bars: stamps each decade label, skipping any that would
/// overlap the previous one, and indents to sit under the bar column.
fn ruler_line(
    ticks: &[(u16, String)],
    bar_width: u16,
    name_width: u16,
    theme: &Theme,
) -> Line<'static> {
    let mut cells: Vec<char> = vec![' '; bar_width as usize];
    let mut last_end: u16 = 0;
    for (column, label) in ticks {
        let chars: Vec<char> = label.chars().collect();
        let end = column + chars.len() as u16;
        if *column < last_end || end > bar_width {
            continue;
        }
        for (i, ch) in chars.iter().enumerate() {
            cells[*column as usize + i] = *ch;
        }
        last_end = end + 1;
    }
    let ruler: String = cells.into_iter().collect();
    Line::from(vec![
        Span::raw(" ".repeat(name_width as usize)),
        Span::styled(ruler, Style::new().fg(theme.muted)),
    ])
}

impl Widget for LogBars<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .border_set(self.glyphs.border)
            .border_style(Style::new().fg(self.theme.border))
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::new()
                    .fg(self.theme.accent2)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        let lines = self.lines(inner.width);
        Paragraph::new(lines).block(block).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ColorDepth, ThemeId};

    fn sample_groups() -> Vec<BarGroup> {
        vec![
            BarGroup {
                label: "Small".to_string(),
                bars: vec![
                    Bar {
                        name: "With MTL".to_string(),
                        value_ns: 100.0,
                        kind: BarKind::Measured,
                        ci: Some((90.0, 110.0)),
                    },
                    Bar {
                        name: "Without MTL".to_string(),
                        value_ns: 10_000.0,
                        kind: BarKind::Extrapolated,
                        ci: None,
                    },
                ],
            },
            BarGroup {
                label: "Large".to_string(),
                bars: vec![Bar {
                    name: "With MTL".to_string(),
                    value_ns: 1_000.0,
                    kind: BarKind::Measured,
                    ci: None,
                }],
            },
        ]
    }

    fn render_text(groups: &[BarGroup], width: u16, height: u16, ascii: bool) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, ascii);
        let glyphs = theme.glyphs();
        let widget = LogBars {
            groups,
            theme: &theme,
            glyphs: &glyphs,
            title: "Signing".to_string(),
            name_width: 14,
            value_width: 22,
            show_legend: true,
        };
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if let Some(cell) = buffer.cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn bar_len_is_monotonic_and_endpoints_are_clamped() {
        let width = 40;
        assert_eq!(bar_len(100.0, 100.0, 10_000.0, width), 1, "min is visible");
        assert_eq!(
            bar_len(10_000.0, 100.0, 10_000.0, width),
            width,
            "max fills the column"
        );
        let a = bar_len(200.0, 100.0, 10_000.0, width);
        let b = bar_len(2_000.0, 100.0, 10_000.0, width);
        let c = bar_len(9_000.0, 100.0, 10_000.0, width);
        assert!(a <= b && b <= c, "log scale must be monotonic: {a} {b} {c}");
    }

    #[test]
    fn bar_len_handles_degenerate_inputs_without_panic() {
        assert_eq!(bar_len(5.0, 5.0, 5.0, 30), 30, "single value fills width");
        assert_eq!(bar_len(5.0, 0.0, 10.0, 30), 30, "non-positive range");
        assert_eq!(bar_len(5.0, 1.0, 10.0, 0), 0, "zero width");
        assert_eq!(bar_len(f64::NAN, 1.0, 10.0, 30), 1, "NaN value");
    }

    #[test]
    fn decade_ticks_cover_the_range_in_order() {
        let ticks = decade_ticks(90.0, 11_000.0, 50);
        let labels: Vec<&str> = ticks.iter().map(|(_, label)| label.as_str()).collect();
        assert_eq!(labels, vec!["100 ns", "1 \u{00b5}s", "10 \u{00b5}s"]);
        let columns: Vec<u16> = ticks.iter().map(|(column, _)| *column).collect();
        assert!(
            columns.windows(2).all(|pair| pair[0] <= pair[1]),
            "columns must ascend: {columns:?}"
        );
        assert_eq!(decade_ticks(10.0, 9.0, 50), Vec::new());
        assert_eq!(decade_ticks(10.0, 20.0, 0), Vec::new());
    }

    #[test]
    fn renders_groups_ruler_and_legend() {
        let out = render_text(&sample_groups(), 60, 10, false);
        assert!(out.contains("Signing"), "block title:\n{out}");
        assert!(out.contains("Small"), "group label:\n{out}");
        assert!(out.contains("With MTL"));
        assert!(out.contains("Without MTL"));
        assert!(out.contains('\u{2588}'), "measured bars are solid:\n{out}");
        assert!(
            out.contains('\u{2591}'),
            "extrapolated bars are hatched:\n{out}"
        );
        assert!(out.contains("measured"), "legend:\n{out}");
        assert!(out.contains("extrapolated"));
        assert!(
            out.contains("\u{00b5}s"),
            "ruler labels in time units:\n{out}"
        );
    }

    #[test]
    fn renders_ascii_glyphs_in_ascii_mode() {
        let out = render_text(&sample_groups(), 60, 9, true);
        assert!(out.contains('#'), "ascii measured bar:\n{out}");
        assert!(out.contains('-'), "ascii extrapolated bar:\n{out}");
        assert!(!out.contains('\u{2588}'));
    }

    #[test]
    fn empty_groups_render_a_placeholder_line() {
        let out = render_text(&[], 60, 4, false);
        assert!(out.contains("no rows recorded"), "empty:\n{out}");
    }

    #[test]
    fn over_long_labels_are_ellipsized_rather_than_split() {
        let groups = vec![BarGroup {
            label: "An extremely long dataset label that cannot fit".to_string(),
            bars: vec![Bar {
                name: "a name that is far too long for the column".to_string(),
                value_ns: 100.0,
                kind: BarKind::Measured,
                ci: None,
            }],
        }];
        let out = render_text(&groups, 40, 4, false);
        assert!(
            out.contains('\u{2026}'),
            "an over-long label must end in an ellipsis:\n{out}"
        );
        assert!(
            !out.contains("far too long"),
            "the name must be truncated, not wrapped:\n{out}"
        );
        for line in out.lines() {
            assert!(
                line.chars().count() <= 40,
                "a line exceeded the panel width: {line:?}"
            );
        }
    }

    #[test]
    fn ascii_mode_uses_an_ascii_ellipsis_marker() {
        let groups = vec![BarGroup {
            label: "An extremely long dataset label that cannot fit".to_string(),
            bars: vec![Bar {
                name: "With MTL".to_string(),
                value_ns: 100.0,
                kind: BarKind::Measured,
                ci: None,
            }],
        }];
        let out = render_text(&groups, 24, 3, true);
        assert!(out.contains('~'), "ascii truncation marker:\n{out}");
        assert!(
            !out.contains('\u{2026}'),
            "no unicode marker in ascii:\n{out}"
        );
    }
}
