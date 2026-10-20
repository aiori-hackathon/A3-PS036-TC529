//! The Signing tab: with-MTL vs without-MTL bars on the shared
//! log scale, the numbers table with its speed-up column, and a detail line
//! for the currently selected size. Bar and table values come from
//! [`crate::data::ResultsView`], which mirrors the report charts exactly —
//! nothing here hardcodes a result number.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use climb_report::charts::fmt_time_ns;
use climb_report::condition_short_label;

use super::amortization;
use crate::app::App;
use crate::data::{DetailLine, PerfCell, ResultsView, ValueMode};
use crate::theme::{Glyphs, Theme};
use crate::widgets::ellipsize;
use crate::widgets::log_bars::{BarKind, LogBars};

/// Render the Signing tab into `area`.
pub fn render_signing(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let mode = app.value_mode();
    let groups = view.signing_groups(mode);
    let rows = view.signing_rows(mode);
    let sizes = view.signing_sizes();
    let selected = app.selected_size().min(sizes.len().saturating_sub(1));
    let detail = sizes
        .get(selected)
        .map(|size| view.signing_detail(size))
        .unwrap_or_default();
    // The detail panel is a plain block-less paragraph, so its available width
    // is the area it is given.
    let detail_width = area.width as usize;

    let chart = LogBars {
        groups: &groups,
        theme,
        glyphs,
        title: format!(
            "Signing{} {}",
            if theme.ascii { " - " } else { " \u{00b7} " },
            mode.label()
        ),
        name_width: 16,
        value_width: 24,
        show_legend: true,
    };

    let [chart_area, detail_area, amort_area, table_area] = Layout::vertical([
        Constraint::Min(7),
        Constraint::Length(if detail.is_empty() { 0 } else { 2 }),
        Constraint::Min(6),
        Constraint::Min(4),
    ])
    .areas(area);
    frame.render_widget(chart, chart_area);
    if !detail.is_empty() {
        frame.render_widget(
            Paragraph::new(detail_lines(&detail, detail_width, theme)),
            detail_area,
        );
    }
    amortization::render_amortization(frame, amort_area, &view.amortization_view(), theme, glyphs);
    render_table(frame, table_area, &rows, selected, mode, theme, glyphs);
}

/// Columns reserved for the dataset-size column of the numbers table.
const SIZE_COLUMN_WIDTH: u16 = 12;

fn render_table(
    frame: &mut Frame,
    area: Rect,
    rows: &[crate::data::SigningRow],
    selected: usize,
    mode: ValueMode,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let header = ["Size", "MTL amortised", "No amortisation", "Speed-up"]
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
                ellipsize(&row.size, SIZE_COLUMN_WIDTH as usize, theme.ascii),
                Style::new().fg(theme.accent2),
            )),
            Cell::from(cell_text(row.with.as_ref(), theme)),
            Cell::from(cell_text(row.without.as_ref(), theme)),
            Cell::from(Span::styled(
                speedup_text(row).unwrap_or_else(|| dash(theme).to_string()),
                theme.measured,
            )),
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
            Constraint::Length(SIZE_COLUMN_WIDTH),
            Constraint::Min(12),
            Constraint::Min(12),
            Constraint::Length(10),
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

/// One detail line per condition for the selected size.
///
/// `width` is the columns the paragraph has available: the line is composed of
/// a padded condition label, the timing with its CI span and the source note,
/// and the source is shortened with a marker when the whole line would run past
/// `width`, so the panel clips nothing mid-word.
fn detail_lines(detail: &[DetailLine], width: usize, theme: &Theme) -> Vec<Line<'static>> {
    detail
        .iter()
        .map(|line| {
            let ci = match line.ci {
                Some((lo, hi)) if line.mean_ns > 0.0 && hi >= lo => {
                    let pct = (hi - lo) / 2.0 / line.mean_ns * 100.0;
                    format!(" \u{00b1}{pct:.0}%")
                }
                _ => String::new(),
            };
            let label = ellipsize(condition_short_label(&line.condition), 16, theme.ascii);
            let head = format!(" {label:<16}");
            let timing = format!("{}{ci}", fmt_time_ns(line.mean_ns));
            let used = head.chars().count() + timing.chars().count() + 2;
            let source = ellipsize(&line.source, width.saturating_sub(used), theme.ascii);
            Line::from(vec![
                Span::styled(head, Style::new().fg(theme.accent)),
                Span::styled(timing, theme.measured),
                Span::styled(format!("  {source}"), Style::new().fg(theme.muted)),
            ])
        })
        .collect()
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

/// The speed-up column, reusing the Overview KPI string so both stay in sync.
fn speedup_text(row: &crate::data::SigningRow) -> Option<String> {
    row.speedup.clone().map(|kpi| kpi.value)
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

    /// The composed detail line the paragraph would draw, so its width and its
    /// marker can be asserted directly.
    fn plain(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn detail_line_shortens_the_source_before_the_panel_edge_at_80x24() {
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        // At 80x24 the detail paragraph spans the whole tab body, so its width
        // is the full terminal width. The source note below is deliberately far
        // longer than the columns left after the label and the timing.
        let width = 80;
        let detail = vec![DetailLine {
            condition: "Without MTL".to_string(),
            mean_ns: 1.0e9,
            ci: None,
            per_message_ns: 1.0,
            samples: 1,
            source: "extrapolated: 18,446,744,073,709,551,616 \u{00d7} measured per-message cost"
                .to_string(),
        }];
        let text = plain(&detail_lines(&detail, width, &theme)[0]);
        assert!(
            text.chars().count() <= width,
            "the detail line overruns the panel: {text:?}"
        );
        assert!(
            text.ends_with('\u{2026}'),
            "the dropped tail must be marked: {text:?}"
        );
        assert!(
            !text.contains("per-message cost"),
            "the source tail must be dropped rather than clipped: {text:?}"
        );
    }

    #[test]
    fn detail_line_stays_intact_when_it_fits() {
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        let detail = vec![DetailLine {
            condition: "With MTL".to_string(),
            mean_ns: 1.0e9,
            ci: None,
            per_message_ns: 1.0,
            samples: 100,
            source: "measured, 100 messages".to_string(),
        }];
        let text = plain(&detail_lines(&detail, 80, &theme)[0]);
        assert!(
            text.contains("measured, 100 messages"),
            "a fitting line must be untouched: {text:?}"
        );
        assert!(!text.contains('\u{2026}'), "no marker needed: {text:?}");
    }

    #[test]
    fn size_column_ellipsizes_a_long_label_at_80x24() {
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        let glyphs = theme.glyphs();
        let rows = vec![crate::data::SigningRow {
            size: "a size label far longer than the column".to_string(),
            n: 100,
            with: None,
            without: None,
            speedup: None,
        }];

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                render_table(
                    frame,
                    frame.area(),
                    &rows,
                    0,
                    ValueMode::Total,
                    &theme,
                    &glyphs,
                )
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
        assert!(
            out.contains('\u{2026}'),
            "the size cell must be ellipsized:\n{out}"
        );
        assert!(
            !out.contains("far longer than"),
            "the size label must be cut, not wrapped:\n{out}"
        );
    }

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
}
