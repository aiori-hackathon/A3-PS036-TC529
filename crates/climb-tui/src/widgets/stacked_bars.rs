//! Linear stacked bars for the Wire and Memory tabs (phases 21.1, 21.4).
//!
//! Unlike the log-scale [`crate::widgets::log_bars`] comparison, these bars are
//! drawn on a **linear** scale normalised per group, because the Wire and
//! Memory tabs compare shares of a single total (ladder vs per-message bytes,
//! modelled pages vs measured RSS) rather than values spanning decades. Layout
//! ([`scale_len`], [`segment_widths`]) is pure and unit-tested; rendering only
//! draws what layout computed. Every segment carries a caller-supplied label
//! and byte count, so no number literal and no byte formatting lives here — the
//! tab modules format their own trailing notes through `climb_report`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::ellipsize;
use crate::theme::{Glyphs, Theme};

/// The visual role of a stacked segment; drives its glyph and style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentStyle {
    /// Solid fill — a directly measured bar.
    Solid,
    /// Hatched fill — the shared / amortised component.
    Hatched,
    /// Dotted fill — the per-message / harness component.
    Dotted,
}

impl SegmentStyle {
    /// The glyph for this segment in the active glyph set.
    fn glyph(self, glyphs: &Glyphs) -> char {
        match self {
            SegmentStyle::Solid => glyphs.bar_filled,
            SegmentStyle::Hatched => glyphs.bar_empty,
            // No analytical glyph exists in the sets; keep the dotted segment
            // visually distinct in both modes without inventing a new category.
            SegmentStyle::Dotted if glyphs.bar_filled == '#' => ':',
            SegmentStyle::Dotted => '\u{2592}',
        }
    }
}

/// One stacked segment: a labelled byte count drawn with a [`SegmentStyle`].
#[derive(Debug, Clone, PartialEq)]
pub struct StackSegment {
    pub label: String,
    pub bytes: u64,
    pub style: SegmentStyle,
}

/// One labelled stacked bar with a caller-formatted trailing note.
#[derive(Debug, Clone, PartialEq)]
pub struct StackBar {
    pub name: String,
    pub segments: Vec<StackSegment>,
    /// Trailing text rendered after the bar (formatted by the caller).
    pub note: String,
}

impl StackBar {
    /// Sum of the segment byte counts.
    pub fn total(&self) -> u64 {
        self.segments.iter().map(|segment| segment.bytes).sum()
    }
}

/// One group of stacked bars sharing a linear scale (`scale_max`).
#[derive(Debug, Clone, PartialEq)]
pub struct StackGroup {
    pub label: String,
    pub bars: Vec<StackBar>,
    /// Value mapped to the full bar column for every bar in this group.
    pub scale_max: u64,
    /// Optional muted caption drawn under the group's bars.
    pub caption: Option<String>,
}

/// Linear fill length of `value` on the range `0..=max` across `width` cells.
///
/// A zero `value`, a zero `max` or a zero `width` draws nothing; any non-zero
/// `value` below `max` draws at least one cell so it never vanishes; `value >=
/// max` fills the whole column.
pub fn scale_len(value: u64, max: u64, width: u16) -> u16 {
    if width == 0 || max == 0 || value == 0 {
        return 0;
    }
    if value >= max {
        return width;
    }
    let len = (u128::from(value) * u128::from(width) / u128::from(max)) as u16;
    len.max(1)
}

/// Split `width` cells among `values` in proportion to their sizes.
///
/// The returned widths sum **exactly** to `width` (largest-remainder rounding,
/// ties broken by index order), which keeps a stacked bar's total length equal
/// to the bar column it was allocated. A zero `width`, an empty slice, or an
/// all-zero input yields all-zero widths.
pub fn segment_widths(values: &[u64], width: u16) -> Vec<u16> {
    let total: u64 = values.iter().copied().sum();
    if width == 0 || total == 0 {
        return vec![0; values.len()];
    }
    let w = u128::from(width);
    let t = u128::from(total);
    let mut out = vec![0u16; values.len()];
    let mut assigned: u128 = 0;
    let mut remainders: Vec<(u128, usize)> = Vec::with_capacity(values.len());
    for (i, &value) in values.iter().enumerate() {
        let exact = u128::from(value) * w;
        let floor = exact / t;
        // Narrowing is lossless: floor <= width, and width is a u16.
        out[i] = floor as u16;
        assigned += floor;
        remainders.push((exact % t, i));
    }
    // Leftover cells are strictly fewer than the segment count, but the modulo
    // keeps the loop terminating even if that invariant were ever violated.
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let len = remainders.len();
    let mut left = (w - assigned) as usize;
    let mut k = 0;
    while left > 0 {
        out[remainders[k % len].1] += 1;
        left -= 1;
        k += 1;
    }
    out
}

/// A grouped, linear stacked-bar comparison inside a bordered block, with an
/// optional scale note and segment legend.
pub struct StackedBars<'a> {
    pub groups: &'a [StackGroup],
    pub theme: &'a Theme,
    pub glyphs: &'a Glyphs,
    pub title: String,
    /// Width of the name column, including the leading indent.
    pub name_width: u16,
    /// Width reserved for the trailing note text.
    pub note_width: u16,
    /// Legend entries: `(label, style)` pairs drawn under the bars.
    pub legend: Vec<(String, SegmentStyle)>,
    /// Optional caption describing the shared linear scale.
    pub scale_note: Option<String>,
}

impl StackedBars<'_> {
    fn seg_style(&self, style: SegmentStyle) -> Style {
        match style {
            SegmentStyle::Solid => self.theme.measured,
            SegmentStyle::Hatched => self.theme.extrapolated,
            SegmentStyle::Dotted => self.theme.analytical,
        }
    }

    /// The body lines: a header per group, one row per bar, then any scale note
    /// and legend. `width` is the block's inner width.
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if self.groups.is_empty() {
            return vec![Line::from(Span::styled(
                " no rows recorded",
                Style::new().fg(self.theme.muted),
            ))];
        }
        let name_width = self.name_width.min(width);
        let bar_width = width
            .saturating_sub(name_width)
            .saturating_sub(self.note_width)
            .max(1);

        let ascii = self.theme.ascii;
        let mut lines = Vec::new();
        for group in self.groups {
            let label = ellipsize(&group.label, width.saturating_sub(1) as usize, ascii);
            lines.push(Line::from(Span::styled(
                format!(" {label}"),
                Style::new()
                    .fg(self.theme.accent2)
                    .add_modifier(Modifier::BOLD),
            )));
            for bar in &group.bars {
                let filled = scale_len(bar.total(), group.scale_max, bar_width);
                let values: Vec<u64> = bar.segments.iter().map(|seg| seg.bytes).collect();
                let widths = segment_widths(&values, filled);

                let mut spans = vec![Span::styled(
                    format!(
                        " {:<pad$}",
                        ellipsize(&bar.name, name_width.saturating_sub(1) as usize, ascii),
                        pad = name_width.saturating_sub(1) as usize
                    ),
                    Style::new().fg(self.theme.fg),
                )];
                let mut drawn: u16 = 0;
                for (seg, cell_width) in bar.segments.iter().zip(widths.iter()) {
                    if *cell_width == 0 {
                        continue;
                    }
                    let cells: String =
                        std::iter::repeat_n(seg.style.glyph(self.glyphs), *cell_width as usize)
                            .collect();
                    spans.push(Span::styled(cells, self.seg_style(seg.style)));
                    drawn += *cell_width;
                }
                spans.push(Span::raw(" ".repeat((bar_width - drawn) as usize)));
                spans.push(Span::raw(" "));
                // The note follows the name column, the bar column and one
                // separating space, so it may occupy one column less than its
                // reserved width.
                spans.push(Span::styled(
                    ellipsize(&bar.note, self.note_width.saturating_sub(1) as usize, ascii),
                    Style::new().fg(self.theme.muted),
                ));
                lines.push(Line::from(spans));
            }
            if let Some(caption) = &group.caption {
                let caption = ellipsize(caption, width.saturating_sub(3) as usize, ascii);
                lines.push(Line::from(Span::styled(
                    format!("   {caption}"),
                    Style::new().fg(self.theme.muted),
                )));
            }
        }

        if let Some(note) = &self.scale_note {
            let note = ellipsize(note, width.saturating_sub(1) as usize, ascii);
            lines.push(Line::from(Span::styled(
                format!(" {note}"),
                Style::new().fg(self.theme.muted),
            )));
        }

        if !self.legend.is_empty() {
            let mut spans = vec![Span::raw(" ")];
            for (i, (label, style)) in self.legend.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled(
                    style.glyph(self.glyphs).to_string(),
                    self.seg_style(*style),
                ));
                spans.push(Span::raw(format!(" {label}")));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

impl Widget for StackedBars<'_> {
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

    fn sample_groups() -> Vec<StackGroup> {
        vec![StackGroup {
            label: "Small".to_string(),
            scale_max: 1000,
            caption: Some("condensed 100/120/140 B".to_string()),
            bars: vec![
                StackBar {
                    name: "With MTL".to_string(),
                    segments: vec![
                        StackSegment {
                            label: "ladder".to_string(),
                            bytes: 100,
                            style: SegmentStyle::Hatched,
                        },
                        StackSegment {
                            label: "condensed".to_string(),
                            bytes: 200,
                            style: SegmentStyle::Dotted,
                        },
                    ],
                    note: "300 B".to_string(),
                },
                StackBar {
                    name: "Without MTL".to_string(),
                    segments: vec![StackSegment {
                        label: "full".to_string(),
                        bytes: 1000,
                        style: SegmentStyle::Solid,
                    }],
                    note: "1,000 B".to_string(),
                },
            ],
        }]
    }

    fn render_text(groups: &[StackGroup], width: u16, height: u16, ascii: bool) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, ascii);
        let glyphs = theme.glyphs();
        let widget = StackedBars {
            groups,
            theme: &theme,
            glyphs: &glyphs,
            title: "Wire".to_string(),
            name_width: 14,
            note_width: 20,
            legend: vec![
                ("ladder".to_string(), SegmentStyle::Hatched),
                ("condensed".to_string(), SegmentStyle::Dotted),
                ("full".to_string(), SegmentStyle::Solid),
            ],
            scale_note: Some("linear scale, normalised per size".to_string()),
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
    fn segment_widths_sum_exactly_to_width() {
        let cases: &[(&[u64], u16)] = &[
            (&[1, 1, 1], 7),
            (&[1, 2, 3], 10),
            (&[100, 200, 700], 33),
            (&[1, 1, 1, 1, 1, 1, 1], 5),
            (&[999_999, 1], 40),
            (&[7], 13),
            (&[5, 0, 5], 9),
        ];
        for (values, width) in cases {
            let widths = segment_widths(values, *width);
            let sum: u16 = widths.iter().sum();
            assert_eq!(sum, *width, "values={values:?} width={width} -> {widths:?}");
        }
    }

    #[test]
    fn segment_widths_degenerate_inputs() {
        assert_eq!(segment_widths(&[], 10), Vec::<u16>::new());
        assert_eq!(segment_widths(&[1, 2, 3], 0), vec![0, 0, 0]);
        assert_eq!(segment_widths(&[0, 0, 0], 10), vec![0, 0, 0]);
    }

    #[test]
    fn segment_widths_largest_remainder_breaks_ties_by_index() {
        // 5 cells over three equal parts: 2,2,1 (first indices win the leftover).
        assert_eq!(segment_widths(&[1, 1, 1], 5), vec![2, 2, 1]);
    }

    #[test]
    fn scale_len_clamps_endpoints_and_keeps_small_values_visible() {
        assert_eq!(scale_len(0, 1000, 20), 0, "zero draws nothing");
        assert_eq!(scale_len(1000, 1000, 20), 20, "max fills the column");
        assert_eq!(scale_len(3000, 1000, 20), 20, "above max clamps to full");
        assert_eq!(scale_len(500, 1000, 20), 10, "linear midpoint");
        assert_eq!(scale_len(1, 1_000_000, 20), 1, "a tiny value stays visible");
        assert_eq!(scale_len(10, 0, 20), 0, "zero max draws nothing");
        assert_eq!(scale_len(10, 100, 0), 0, "zero width draws nothing");
    }

    #[test]
    fn renders_groups_segments_notes_and_legend() {
        let out = render_text(&sample_groups(), 60, 10, false);
        assert!(out.contains("Wire"), "block title:\n{out}");
        assert!(out.contains("Small"), "group label:\n{out}");
        assert!(out.contains("With MTL"));
        assert!(out.contains("Without MTL"));
        assert!(out.contains('\u{2588}'), "solid full bar:\n{out}");
        assert!(out.contains('\u{2591}'), "hatched ladder segment:\n{out}");
        assert!(out.contains('\u{2592}'), "dotted condensed segment:\n{out}");
        assert!(out.contains("normalised per size"), "scale note:\n{out}");
        assert!(out.contains("condensed 100/120/140 B"), "caption:\n{out}");
        assert!(out.contains("ladder"), "legend:\n{out}");
    }

    #[test]
    fn renders_ascii_glyphs_in_ascii_mode() {
        let out = render_text(&sample_groups(), 60, 10, true);
        assert!(out.contains('#'), "ascii solid bar:\n{out}");
        assert!(out.contains('-'), "ascii hatched bar:\n{out}");
        assert!(out.contains(':'), "ascii dotted bar:\n{out}");
        assert!(!out.contains('\u{2588}'));
    }

    #[test]
    fn empty_groups_render_a_placeholder_line() {
        let out = render_text(&[], 60, 4, false);
        assert!(out.contains("no rows recorded"), "empty:\n{out}");
    }

    #[test]
    fn over_long_labels_are_ellipsized_rather_than_split() {
        let groups = vec![StackGroup {
            label: "An extremely long dataset label that cannot fit".to_string(),
            scale_max: 1000,
            caption: Some("a caption far longer than the panel could ever show".to_string()),
            bars: vec![StackBar {
                name: "a name that is far too long for the column".to_string(),
                segments: vec![StackSegment {
                    label: "full".to_string(),
                    bytes: 1000,
                    style: SegmentStyle::Solid,
                }],
                note: "a trailing note far too long for its reserved column".to_string(),
            }],
        }];
        let out = render_text(&groups, 44, 6, false);
        assert!(
            out.contains('\u{2026}'),
            "over-long text must end in an ellipsis:\n{out}"
        );
        for needle in ["far too long", "could ever show", "reserved column"] {
            assert!(
                !out.contains(needle),
                "{needle:?} must be truncated, not wrapped:\n{out}"
            );
        }
        for line in out.lines() {
            assert!(
                line.chars().count() <= 44,
                "a line exceeded the panel width: {line:?}"
            );
        }
    }

    #[test]
    fn ascii_mode_uses_an_ascii_ellipsis_marker() {
        let groups = vec![StackGroup {
            label: "An extremely long dataset label that cannot fit".to_string(),
            scale_max: 1000,
            caption: None,
            bars: vec![StackBar {
                name: "With MTL".to_string(),
                segments: vec![StackSegment {
                    label: "full".to_string(),
                    bytes: 1000,
                    style: SegmentStyle::Solid,
                }],
                note: "1,000 B".to_string(),
            }],
        }];
        let out = render_text(&groups, 24, 4, true);
        assert!(out.contains('~'), "ascii truncation marker:\n{out}");
        assert!(
            !out.contains('\u{2026}'),
            "no unicode marker in ascii:\n{out}"
        );
    }
}
