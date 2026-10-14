//! The amortization curve panel (phase 20.5): measured with-MTL and
//! without-MTL per-message signing cost against dataset size on log-log
//! axes, plus the OLS fit of the with-MTL batch total. It renders inside
//! the Signing tab — the tab bar is pinned to nine tabs (keys 1-9), so the
//! curve lives with the signing data it describes instead of as a tenth tab.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::Span;
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, LegendPosition, Paragraph};
use ratatui::Frame;

use crate::data::AmortizationView;
use crate::theme::{Glyphs, Theme};
use climb_report::{SHORT_LABEL_WITHOUT_MTL, SHORT_LABEL_WITH_MTL};

/// Render the amortization panel into `area`.
pub fn render_amortization(
    frame: &mut Frame,
    area: Rect,
    amort: &AmortizationView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let block = Block::bordered()
        .border_set(glyphs.border)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " Amortization ",
            Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
        ));

    if !amort.has_data {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " no signing rows recorded",
                Style::new().fg(theme.muted),
            ))
            .block(block),
            area,
        );
        return;
    }

    let with = Dataset::default()
        .name(SHORT_LABEL_WITH_MTL)
        .marker(Marker::Dot)
        .graph_type(GraphType::Scatter)
        .style(theme.measured)
        .data(&amort.with_points);
    let without = Dataset::default()
        .name(SHORT_LABEL_WITHOUT_MTL)
        .marker(Marker::Dot)
        .graph_type(GraphType::Scatter)
        .style(Style::new().fg(theme.warn))
        .data(&amort.without_points);
    let fit = Dataset::default()
        .name("fit")
        .marker(Marker::Dot)
        .graph_type(GraphType::Line)
        .style(theme.analytical)
        .data(&amort.fit_points);

    let x_axis = Axis::default()
        .title(Span::styled(" messages", Style::new().fg(theme.muted)))
        .style(Style::new().fg(theme.muted))
        .bounds(amort.x_bounds)
        .labels(amort.x_labels.clone());
    let y_axis = Axis::default()
        .title(Span::styled(
            " per-message time",
            Style::new().fg(theme.muted),
        ))
        .style(Style::new().fg(theme.muted))
        .bounds(amort.y_bounds)
        .labels(amort.y_labels.clone());

    let chart = Chart::new(vec![with, without, fit])
        .block(block)
        .x_axis(x_axis)
        .y_axis(y_axis)
        .legend_position(Some(LegendPosition::TopLeft));
    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};

    fn render_to_string(amort: &AmortizationView, width: u16, height: u16) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render_amortization(frame, frame.area(), amort, &theme, &glyphs))
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
    fn empty_view_renders_the_placeholder() {
        let out = render_to_string(&AmortizationView::default(), 80, 10);
        assert!(out.contains("Amortization"), "panel title:\n{out}");
        assert!(
            out.contains("no signing rows recorded"),
            "placeholder:\n{out}"
        );
    }

    #[test]
    fn populated_view_renders_a_log_log_chart() {
        let amort = AmortizationView {
            has_data: true,
            with_points: vec![(2.0, 1.0), (4.0, 0.0)],
            without_points: vec![(4.0, 1.6)],
            fit_points: vec![(2.0, 1.0), (3.0, 0.5), (4.0, 0.0)],
            x_bounds: [2.0, 4.0],
            y_bounds: [0.0, 2.0],
            x_labels: vec!["100".to_string(), "1,000".to_string(), "10,000".to_string()],
            y_labels: vec!["1 ns".to_string(), "100 ns".to_string()],
        };
        let out = render_to_string(&amort, 120, 12);
        assert!(out.contains("Amortization"), "panel title:\n{out}");
        assert!(out.contains("messages"), "x-axis title:\n{out}");
        assert!(
            !out.contains("no signing rows recorded"),
            "chart replaces the placeholder:\n{out}"
        );
    }
}
