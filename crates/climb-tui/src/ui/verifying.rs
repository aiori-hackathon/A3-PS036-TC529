//! The Verifying tab (phase 20.4): trust-cached / full-ladder / without-MTL
//! bars on the shared log scale plus the numbers table. Values come from
//! [`crate::data::ResultsView`], which mirrors the report charts exactly.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use climb_report::charts::fmt_time_ns;

use crate::app::App;
use crate::data::{PerfCell, ResultsView, ReuseRow, ValueMode, VerifyingRow};
use crate::theme::{Glyphs, Theme};
use crate::widgets::log_bars::{BarKind, LogBars};

/// Render the Verifying tab into `area`.
pub fn render_verifying(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let mode = app.value_mode();
    let groups = view.verifying_groups(mode);
    let rows = view.verifying_rows(mode);
    let reuse = view.reuse_rows();
    let sizes = view.verifying_sizes();
    let selected = app.selected_size().min(sizes.len().saturating_sub(1));

    let chart = LogBars {
        groups: &groups,
        theme,
        glyphs,
        title: format!(
            "Verifying{} {}",
            if theme.ascii { " - " } else { " \u{00b7} " },
            mode.label()
        ),
        name_width: 14,
        value_width: 28,
        show_legend: true,
    };

    let [chart_area, table_area, reuse_area] = Layout::vertical([
        Constraint::Min(7),
        Constraint::Min(4),
        Constraint::Length(6),
    ])
    .areas(area);
    frame.render_widget(chart, chart_area);
    render_table(frame, table_area, &rows, selected, mode, theme, glyphs);
    render_reuse(frame, reuse_area, &reuse, theme, glyphs);
}

/// The cold/warm reuse panel: smallest break-even reuse count per size.
fn render_reuse(frame: &mut Frame, area: Rect, rows: &[ReuseRow], theme: &Theme, glyphs: &Glyphs) {
    if rows.is_empty() {
        let placeholder = Paragraph::new(Span::styled(
            " no reuse rows recorded",
            Style::new().fg(theme.muted),
        ))
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    " Reuse ",
                    Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                )),
        );
        frame.render_widget(placeholder, area);
        return;
    }

    let header = ["Size", "Time r*", "Bytes r*", "Class"]
        .into_iter()
        .map(|text| {
            Cell::from(Span::styled(
                text,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ))
        });
    let header_row = Row::new(header);

    let body = rows.iter().map(|row| {
        Row::new([
            Cell::from(Span::styled(
                row.size.clone(),
                Style::new().fg(theme.accent2),
            )),
            Cell::from(reuse_r(row.time_r, theme)),
            Cell::from(reuse_r(row.bytes_r, theme)),
            Cell::from(Span::styled(
                row.class.clone(),
                Style::new().fg(theme.muted),
            )),
        ])
    });

    let table = Table::new(
        body,
        [
            Constraint::Length(12),
            Constraint::Min(10),
            Constraint::Min(10),
            Constraint::Min(12),
        ],
    )
    .column_spacing(1)
    .header(header_row)
    .block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Reuse \u{00b7} cold vs warm ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(table, area);
}

/// Render a break-even reuse count, or an em dash when none was reached.
fn reuse_r(r: Option<u64>, theme: &Theme) -> Span<'static> {
    match r {
        None => Span::styled(dash(theme), Style::new().fg(theme.muted)),
        Some(r) => Span::styled(format!("r={r}"), theme.measured),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    rows: &[VerifyingRow],
    selected: usize,
    mode: ValueMode,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let header = ["Size", "Trust-cached", "Full ladder", "Without MTL"]
        .into_iter()
        .map(|text| {
            Cell::from(Span::styled(
                text,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ))
        });
    let header_row = Row::new(header);

    let body = rows.iter().enumerate().map(|(i, row)| {
        let cells = [
            Cell::from(Span::styled(
                row.size.clone(),
                Style::new().fg(theme.accent2),
            )),
            Cell::from(cell_text(row.trust.as_ref(), theme)),
            Cell::from(cell_text(row.full.as_ref(), theme)),
            Cell::from(cell_text(row.without.as_ref(), theme)),
        ];
        let style = if i == selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        Row::new(cells).style(style)
    });

    let table = Table::new(
        body,
        [
            Constraint::Length(12),
            Constraint::Min(12),
            Constraint::Min(12),
            Constraint::Min(12),
        ],
    )
    .column_spacing(1)
    .header(header_row)
    .block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                format!(
                    " Tables{} {}",
                    if theme.ascii { " - " } else { " \u{00b7} " },
                    mode.label()
                ),
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(table, area);
}

fn cell_text(cell: Option<&PerfCell>, theme: &Theme) -> Span<'static> {
    match cell {
        None => Span::styled(dash(theme), Style::new().fg(theme.muted)),
        Some(cell) => {
            let prefix = if cell.kind == BarKind::Extrapolated {
                if theme.ascii {
                    "~"
                } else {
                    "\u{2248}"
                }
            } else {
                ""
            };
            let style = match cell.kind {
                BarKind::Extrapolated => theme.extrapolated,
                _ => theme.measured,
            };
            Span::styled(format!("{prefix}{}", fmt_time_ns(cell.value_ns)), style)
        }
    }
}

fn dash(theme: &Theme) -> &'static str {
    if theme.ascii {
        "-"
    } else {
        "\u{2014}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_is_ascii_safe() {
        let ascii = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            true,
        );
        let unicode = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        assert_eq!(dash(&ascii), "-");
        assert_eq!(dash(&unicode), "\u{2014}");
    }

    #[test]
    fn cell_text_marks_extrapolated_values() {
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        let measured = Some(PerfCell {
            value_ns: 1000.0,
            kind: BarKind::Measured,
            ci: Some((900.0, 1100.0)),
        });
        let extrapolated = Some(PerfCell {
            value_ns: 50_000.0,
            kind: BarKind::Extrapolated,
            ci: None,
        });
        assert!(cell_text(measured.as_ref(), &theme)
            .content
            .contains("\u{00b5}s"));
        let ext = cell_text(extrapolated.as_ref(), &theme);
        assert!(
            ext.content.contains('\u{2248}'),
            "extrapolated marks ≈: {}",
            ext.content
        );
        assert_eq!(cell_text(None, &theme).content, "\u{2014}");
    }

    fn theme() -> Theme {
        Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        )
    }

    fn render_reuse_to_string(rows: &[ReuseRow]) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let theme = theme();
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render_reuse(frame, frame.area(), rows, &theme, &glyphs))
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
    fn reuse_panel_renders_break_evens_and_class() {
        let rows = vec![ReuseRow {
            size: "Small".to_string(),
            time_r: Some(1),
            bytes_r: Some(2),
            class: "DERIVED".to_string(),
        }];
        let out = render_reuse_to_string(&rows);
        for needle in [
            "Reuse", "Time r*", "Bytes r*", "Small", "r=1", "r=2", "DERIVED",
        ] {
            assert!(
                out.contains(needle),
                "reuse panel misses {needle:?}:\n{out}"
            );
        }
    }

    #[test]
    fn reuse_panel_marks_missing_break_even_with_a_dash() {
        let rows = vec![ReuseRow {
            size: "Large".to_string(),
            time_r: None,
            bytes_r: None,
            class: "EXTRAPOLATED".to_string(),
        }];
        let out = render_reuse_to_string(&rows);
        assert!(
            out.contains('\u{2014}'),
            "missing r* renders an em dash:\n{out}"
        );
        assert!(out.contains("EXTRAPOLATED"), "class present:\n{out}");
    }

    #[test]
    fn reuse_panel_without_rows_shows_a_placeholder() {
        let out = render_reuse_to_string(&[]);
        assert!(
            out.contains("no reuse rows recorded"),
            "empty reuse panel placeholder:\n{out}"
        );
    }
}
