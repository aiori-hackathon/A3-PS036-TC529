//! The Signing tab (phase 20.3): with-MTL vs without-MTL bars on the shared
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

use super::amortization;
use crate::app::App;
use crate::data::{DetailLine, PerfCell, ResultsView, ValueMode};
use crate::theme::{Glyphs, Theme};
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

    let chart = LogBars {
        groups: &groups,
        theme,
        glyphs,
        title: format!(
            "Signing{} {}",
            if theme.ascii { " - " } else { " \u{00b7} " },
            mode.label()
        ),
        name_width: 14,
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
        frame.render_widget(Paragraph::new(detail_lines(&detail, theme)), detail_area);
    }
    amortization::render_amortization(frame, amort_area, &view.amortization_view(), theme, glyphs);
    render_table(frame, table_area, &rows, selected, mode, theme, glyphs);
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    rows: &[crate::data::SigningRow],
    selected: usize,
    mode: ValueMode,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let header = ["Size", "With MTL", "Without MTL", "Speed-up"]
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
            Constraint::Length(12),
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
fn detail_lines(detail: &[DetailLine], theme: &Theme) -> Vec<Line<'static>> {
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
            Line::from(vec![
                Span::styled(
                    format!(" {:<14}", truncated(&line.condition, 14)),
                    Style::new().fg(theme.accent),
                ),
                Span::styled(format!("{}{ci}", fmt_time_ns(line.mean_ns)), theme.measured),
                Span::styled(format!("  {}", line.source), Style::new().fg(theme.muted)),
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

fn truncated(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
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
    fn truncated_keeps_leftmost_chars() {
        assert_eq!(truncated("Trust-cached ladder", 7), "Trust-c");
        assert_eq!(truncated("With MTL", 20), "With MTL");
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
