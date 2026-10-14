//! The Memory tab: per-size stacked bars comparing the itemised analytical
//! model (libMTL page model, harness buffers, dataset buffers, keypair image)
//! against the measured resident RSS delta, beside a plain-language panel
//! explaining what each bar and the residual mean. On wide terminals the panels
//! sit side by side; below [`WIDE_WIDTH`] the explainer collapses beneath the bars.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use crate::data::ResultsView;
use crate::theme::{Glyphs, Theme};
use crate::widgets::stacked_bars::{SegmentStyle, StackGroup, StackedBars};

/// Terminal width at or above which the bars and explainer share one row.
const WIDE_WIDTH: u16 = 100;
/// Height reserved for the explainer when the panels stack vertically.
const EXPLAINER_STACK_HEIGHT: u16 = 8;

/// Render the Memory tab into `area`.
pub fn render_memory(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let stacks = view.memory_stacks();
    let has_harness = view.memory_has_harness();
    render_panels(frame, area, &stacks, has_harness, theme, glyphs);
}

/// Place the bars and the explainer: side by side when wide, stacked when narrow.
fn layout(area: Rect) -> (Rect, Rect) {
    if area.width >= WIDE_WIDTH {
        let [bars, explainer] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
                .areas(area);
        (bars, explainer)
    } else {
        let explainer_height = EXPLAINER_STACK_HEIGHT.min(area.height);
        let [bars, explainer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(explainer_height)])
                .areas(area);
        (bars, explainer)
    }
}

fn render_panels(
    frame: &mut Frame,
    area: Rect,
    stacks: &[StackGroup],
    has_harness: bool,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let (bars_area, explainer_area) = layout(area);

    let bars = StackedBars {
        groups: stacks,
        theme,
        glyphs,
        title: format!(
            "Memory{} measured vs modelled",
            if theme.ascii { " -" } else { " \u{00b7}" }
        ),
        name_width: 14,
        note_width: 20,
        legend: vec![
            ("libMTL pages".to_string(), SegmentStyle::Hatched),
            ("harness buffers".to_string(), SegmentStyle::Dotted),
            ("dataset buffers".to_string(), SegmentStyle::Dotted),
            ("keypair image".to_string(), SegmentStyle::Dotted),
            ("measured RSS".to_string(), SegmentStyle::Solid),
        ],
        scale_note: Some("linear scale, normalised per size".to_string()),
    };
    frame.render_widget(bars, bars_area);

    let explainer = Paragraph::new(explainer_lines(has_harness, theme))
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    " Reading these numbers ",
                    Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(explainer, explainer_area);
}

fn explainer_lines(has_harness: bool, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  analytical ", theme.extrapolated),
            Span::styled("itemised model total", Style::new().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled("  measured   ", theme.measured),
            Span::styled("resident median (n runs)", Style::new().fg(theme.fg)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  peak     = high-water \u{2212} baseline",
            Style::new().fg(theme.muted),
        )),
        Line::from(Span::styled(
            "  residual = resident median \u{2212} model",
            Style::new().fg(theme.muted),
        )),
        Line::from(Span::styled(
            "  ratio    = measured \u{00f7} analytical",
            Style::new().fg(theme.muted),
        )),
    ];
    if has_harness {
        lines.push(Line::from(Span::styled(
            "  model = libMTL + harness + dataset",
            Style::new().fg(theme.muted),
        )));
        lines.push(Line::from(Span::styled(
            "          + keypair image",
            Style::new().fg(theme.muted),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  harness buffers not modelled (pre-F8)",
            Style::new().fg(theme.muted),
        )));
    }
    lines.push(Line::from(""));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};
    use crate::widgets::stacked_bars::{StackBar, StackSegment};

    fn stacks() -> Vec<StackGroup> {
        vec![StackGroup {
            label: "Small".to_string(),
            scale_max: 2_100_384,
            caption: Some(
                "peak 852 kB \u{00b7} ratio 0.40\u{00d7} \u{00b7} resid -1.3 MB (-60.5%) \u{00b7} \
                 n=3, IQR 213 kB"
                    .to_string(),
            ),
            bars: vec![
                StackBar {
                    name: "Analytical".to_string(),
                    segments: vec![
                        StackSegment {
                            label: "libMTL pages".to_string(),
                            bytes: 2_097_152,
                            style: SegmentStyle::Hatched,
                        },
                        StackSegment {
                            label: "harness buffers".to_string(),
                            bytes: 3232,
                            style: SegmentStyle::Dotted,
                        },
                    ],
                    note: "2.1 MB".to_string(),
                },
                StackBar {
                    name: "Measured".to_string(),
                    segments: vec![StackSegment {
                        label: "measured RSS".to_string(),
                        bytes: 851_968,
                        style: SegmentStyle::Solid,
                    }],
                    note: "852.0 kB".to_string(),
                },
            ],
        }]
    }

    fn render_to_string(
        stacks: &[StackGroup],
        has_harness: bool,
        width: u16,
        height: u16,
    ) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                render_panels(frame, frame.area(), stacks, has_harness, &theme, &glyphs);
            })
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
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
    fn layout_splits_side_by_side_when_wide() {
        let (bars, explainer) = layout(Rect::new(0, 0, 120, 40));
        assert_eq!(bars.y, explainer.y, "wide layout shares a row");
        assert!(explainer.x > 0, "explainer starts to the right");
        assert_eq!(bars.height, 40);
        assert_eq!(explainer.height, 40);
    }

    #[test]
    fn layout_stacks_vertically_when_narrow() {
        let (bars, explainer) = layout(Rect::new(0, 0, 80, 24));
        assert_eq!(bars.x, explainer.x, "narrow layout shares a column");
        assert!(explainer.y > bars.y, "explainer sits below the bars");
        assert!(bars.height > 0 && explainer.height > 0);
    }

    #[test]
    fn renders_bars_and_explainer_at_120x40() {
        let out = render_to_string(&stacks(), true, 120, 40);
        for needle in [
            "Memory",
            "Analytical",
            "Measured",
            "itemised model total",
            "resident median",
            "Reading these numbers",
            "dataset buffers",
        ] {
            assert!(
                out.contains(needle),
                "memory tab misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn explainer_names_the_missing_harness_buffers() {
        let out = render_to_string(&stacks(), false, 120, 40);
        assert!(
            out.contains("pre-F8"),
            "explainer must flag the missing harness model:\n{out}"
        );
    }

    #[test]
    fn empty_data_renders_the_bars_placeholder() {
        let out = render_to_string(&[], true, 80, 24);
        assert!(out.contains("no rows recorded"), "bars placeholder:\n{out}");
    }

    #[test]
    fn renders_at_80x24_without_clipping() {
        let out = render_to_string(&stacks(), true, 80, 24);
        assert!(out.contains("Memory"), "bars panel:\n{out}");
        assert!(out.contains("Reading these numbers"), "explainer:\n{out}");
    }
}
