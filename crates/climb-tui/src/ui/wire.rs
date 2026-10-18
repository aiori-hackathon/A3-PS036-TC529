//! The Wire tab: per-size stacked bars showing the shared
//! ladder and per-message condensed bytes against the full signatures they
//! replace, beside a single-answer UDP-fit gauge. On wide terminals the two
//! panels sit side by side; below [`WIDE_WIDTH`] the gauge collapses beneath
//! the bars so neither is clipped, and on terminals at least [`FIT_MIN_HEIGHT`]
//! rows tall a third band shows the modelled UDP-fit table. All numbers come
//! from [`crate::data::ResultsView`]; nothing here hardcodes a result.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::data::{ResultsView, UdpFitTableData, UdpGaugeData};
use crate::theme::{Glyphs, Theme};
use crate::widgets::stacked_bars::{SegmentStyle, StackGroup, StackedBars};
use crate::widgets::udp_gauge::UdpGauge;

/// Terminal width at or above which the bars and gauge share one row.
const WIDE_WIDTH: u16 = 100;
/// Height reserved for the gauge when the panels stack vertically.
const GAUGE_STACK_HEIGHT: u16 = 9;
/// Terminal height at or above which the modelled UDP-fit table gets its own
/// band; below it the band is dropped so the bars and gauge keep the screen.
const FIT_MIN_HEIGHT: u16 = 30;
/// Height of the UDP-fit band: bordered table plus a one-line note.
const FIT_HEIGHT: u16 = 14;

/// Render the Wire tab into `area`.
pub fn render_wire(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let stacks = view.wire_stacks(app.baseline());
    let gauge = view.udp_gauge_data();
    let fit = view.udp_fit_table();
    render_panels(frame, area, &stacks, &gauge, &fit, theme, glyphs);
}

/// Place the bars, the gauge and (when tall enough) the UDP-fit band:
/// side by side when wide, stacked when narrow.
fn layout(area: Rect) -> (Rect, Rect, Option<Rect>) {
    let (main, fit) = if area.height >= FIT_MIN_HEIGHT {
        let [main, fit] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(FIT_HEIGHT)]).areas(area);
        (main, Some(fit))
    } else {
        (area, None)
    };
    if main.width >= WIDE_WIDTH {
        let [bars, gauge] =
            Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                .areas(main);
        (bars, gauge, fit)
    } else {
        let gauge_height = GAUGE_STACK_HEIGHT.min(main.height);
        let [bars, gauge] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(gauge_height)]).areas(main);
        (bars, gauge, fit)
    }
}

fn render_panels(
    frame: &mut Frame,
    area: Rect,
    stacks: &[StackGroup],
    gauge: &UdpGaugeData,
    fit: &UdpFitTableData,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let (bars_area, gauge_area, fit_area) = layout(area);

    let bars = StackedBars {
        groups: stacks,
        theme,
        glyphs,
        title: format!(
            "Wire{} by size",
            if theme.ascii { " -" } else { " \u{00b7}" }
        ),
        name_width: 16,
        note_width: 24,
        legend: vec![
            ("ladder".to_string(), SegmentStyle::Hatched),
            ("condensed".to_string(), SegmentStyle::Dotted),
            ("full signatures".to_string(), SegmentStyle::Solid),
        ],
        scale_note: Some("linear scale, normalised per size".to_string()),
    };
    frame.render_widget(bars, bars_area);

    let gauge_widget = UdpGauge {
        theme,
        glyphs,
        title: "Single-answer UDP fit".to_string(),
        axis_min: gauge.axis_min,
        axis_max: gauge.axis_max,
        markers: gauge.markers.clone(),
        source_note: gauge.source_note.clone(),
        empty_note: "no capture or wire data recorded".to_string(),
    };
    frame.render_widget(gauge_widget, gauge_area);

    if let Some(fit_area) = fit_area {
        render_udp_fit_table(frame, fit_area, fit, theme, glyphs);
    }
}

/// Render the modelled UDP-fit table: one row per (profile, configuration)
/// pair, one column per dataset size, cells carrying the share of responses
/// that fit 1,232 bytes. Every cell comes from the artifact's
/// `derived.udp_fit` section, styled as an analytical value.
fn render_udp_fit_table(
    frame: &mut Frame,
    area: Rect,
    data: &UdpFitTableData,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let title = Span::styled(
        " Modelled UDP fit ",
        Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
    );
    let block = Block::bordered()
        .border_set(glyphs.border)
        .border_style(Style::new().fg(theme.border))
        .title(title);
    if data.rows.is_empty() {
        frame.render_widget(Paragraph::new(data.note.clone()).block(block), area);
        return;
    }

    let [table_area, note_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let header_style = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
    let mut header_cells: Vec<Cell> = ["Profile", "Cfg"]
        .into_iter()
        .map(|text| Cell::from(Span::styled(text, header_style)))
        .collect();
    for head in &data.size_headers {
        header_cells.push(Cell::from(Span::styled(head.clone(), header_style)));
    }

    let body = data.rows.iter().map(|row| {
        let mut cells: Vec<Cell> = vec![
            Cell::from(Span::styled(
                row.profile.clone(),
                Style::new().fg(theme.accent2),
            )),
            Cell::from(Span::styled(
                row.config.clone(),
                Style::new().fg(theme.accent),
            )),
        ];
        for cell in &row.cells {
            cells.push(Cell::from(Span::styled(cell.clone(), theme.analytical)));
        }
        Row::new(cells)
    });

    let mut widths = vec![Constraint::Min(20), Constraint::Length(3)];
    for _ in &data.size_headers {
        widths.push(Constraint::Min(8));
    }
    let table = Table::new(body, widths)
        .column_spacing(1)
        .header(Row::new(header_cells))
        .block(block);
    frame.render_widget(table, table_area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            data.note.clone(),
            Style::new().fg(theme.muted),
        )),
        note_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};
    use crate::widgets::stacked_bars::{StackBar, StackSegment};
    use crate::widgets::udp_gauge::{nice_axis_max, GaugeMarker, MarkerKind};

    use crate::data::UdpFitTableRow;

    fn stacks() -> Vec<StackGroup> {
        vec![StackGroup {
            label: "Small".to_string(),
            scale_max: 815_632,
            caption: Some("condensed signature: min 108 B / mean 164 B / max 172 B".to_string()),
            bars: vec![
                StackBar {
                    name: "MTL amortised".to_string(),
                    segments: vec![
                        StackSegment {
                            label: "ladder".to_string(),
                            bytes: 7992,
                            style: SegmentStyle::Hatched,
                        },
                        StackSegment {
                            label: "condensed".to_string(),
                            bytes: 16_432,
                            style: SegmentStyle::Dotted,
                        },
                    ],
                    note: "24,424 B \u{00b7} \u{2212}97.0%".to_string(),
                },
                StackBar {
                    name: "No amortisation".to_string(),
                    segments: vec![StackSegment {
                        label: "full".to_string(),
                        bytes: 815_632,
                        style: SegmentStyle::Solid,
                    }],
                    note: "815,632 B".to_string(),
                },
            ],
        }]
    }

    fn gauge() -> UdpGaugeData {
        UdpGaugeData {
            axis_min: 0,
            axis_max: nice_axis_max(24_438),
            markers: vec![
                GaugeMarker {
                    label: "full signature".to_string(),
                    bytes: 24_438,
                    kind: MarkerKind::Full,
                },
                GaugeMarker {
                    label: "condensed proof".to_string(),
                    bytes: 658,
                    kind: MarkerKind::Condensed,
                },
            ],
            source_note: "from recorded DNS capture".to_string(),
        }
    }

    /// Modelled UDP-fit table as test data: two response profiles x three
    /// configurations x two dataset sizes, with a matching capture
    /// cross-check and an RTT assumption for the modelled delay.
    fn fit_data() -> UdpFitTableData {
        UdpFitTableData {
            size_headers: vec!["100".to_string(), "1,000,000".to_string()],
            rows: vec![
                UdpFitTableRow {
                    profile: "Positive A (www)".to_string(),
                    config: "A".to_string(),
                    cells: vec!["100.0%".to_string(), "75.0%".to_string()],
                },
                UdpFitTableRow {
                    profile: "Positive A (www)".to_string(),
                    config: "C".to_string(),
                    cells: vec!["100.0%".to_string(), "100.0%".to_string()],
                },
            ],
            note: "share fitting 1,232 B \u{b7} analytical \u{b7} cross-check exact at 0 B \u{b7} delay MODELED @ RTT 15.0 ms"
                .to_string(),
        }
    }

    fn render_to_string(
        stacks: &[StackGroup],
        gauge: &UdpGaugeData,
        fit: &UdpFitTableData,
        width: u16,
        height: u16,
    ) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                render_panels(frame, frame.area(), stacks, gauge, fit, &theme, &glyphs);
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
        let (bars, gauge, fit) = layout(Rect::new(0, 0, 120, 40));
        assert_eq!(bars.y, gauge.y, "wide layout shares a row");
        assert_eq!(bars.x, 0);
        assert!(gauge.x > 0, "gauge starts to the right of the bars");
        assert_eq!(bars.height, 40 - FIT_HEIGHT, "the fit band keeps its rows");
        assert_eq!(gauge.height, 40 - FIT_HEIGHT);
        let fit = fit.expect("40 rows is tall enough for the fit band");
        assert_eq!(fit.y + fit.height, 40, "fit band sits at the bottom");
        assert_eq!(fit.height, FIT_HEIGHT);
    }

    #[test]
    fn layout_stacks_vertically_when_narrow() {
        let (bars, gauge, fit) = layout(Rect::new(0, 0, 80, 24));
        assert_eq!(bars.x, gauge.x, "narrow layout shares a column");
        assert_eq!(bars.width, 80);
        assert_eq!(gauge.width, 80);
        assert!(gauge.y > bars.y, "gauge sits below the bars");
        assert!(bars.height > 0 && gauge.height > 0, "both panels get space");
        assert!(fit.is_none(), "24 rows is below the fit band's height gate");
    }

    #[test]
    fn layout_drops_the_fit_band_below_the_height_gate() {
        let (_, _, fit) = layout(Rect::new(0, 0, 120, FIT_MIN_HEIGHT - 1));
        assert!(fit.is_none(), "just below the gate the band is dropped");
        let (_, _, fit) = layout(Rect::new(0, 0, 120, FIT_MIN_HEIGHT));
        assert!(fit.is_some(), "at the gate the band is rendered");
    }

    #[test]
    fn renders_bars_and_gauge_at_120x40() {
        let out = render_to_string(&stacks(), &gauge(), &fit_data(), 120, 40);
        for needle in [
            "Wire",
            "MTL amortised",
            "No amortisation",
            "Single-answer UDP fit",
            "full signature",
            "condensed proof",
            "fits within",
            "exceeds",
        ] {
            assert!(
                out.contains(needle),
                "wire tab misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn renders_the_modelled_udp_fit_table_at_120x40() {
        let out = render_to_string(&stacks(), &gauge(), &fit_data(), 120, 40);
        for needle in [
            "Modelled UDP fit",
            "Profile",
            "1,000,000",
            "Positive A (www)",
            "100.0%",
            "cross-check exact at 0 B",
            "delay MODELED @ RTT 15.0 ms",
        ] {
            assert!(
                out.contains(needle),
                "fit band misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn renders_bars_and_gauge_at_80x24_without_clipping() {
        let out = render_to_string(&stacks(), &gauge(), &fit_data(), 80, 24);
        assert!(out.contains("Wire"), "bars panel:\n{out}");
        assert!(out.contains("Single-answer UDP fit"), "gauge panel:\n{out}");
        assert!(
            !out.contains("Modelled UDP fit"),
            "the fit band must stay gated off at 24 rows:\n{out}"
        );
    }

    #[test]
    fn renders_the_fit_placeholder_when_the_artifact_has_no_model() {
        let placeholder = UdpFitTableData {
            size_headers: Vec::new(),
            rows: Vec::new(),
            note: "no UDP-fit model in this artifact".to_string(),
        };
        let out = render_to_string(&stacks(), &gauge(), &placeholder, 120, 40);
        assert!(
            out.contains("no UDP-fit model in this artifact"),
            "fit placeholder:\n{out}"
        );
    }

    #[test]
    fn over_long_labels_are_ellipsized_rather_than_split_at_80x24() {
        let filler = " that could never show up in a real benchmark run and only exists";
        let long_name = format!("an extremely long bar name{filler}{filler}");
        let long_group = format!("an extremely long group label{filler}{filler}");
        let long_note = format!("an extremely long note{filler}{filler}");
        let mut group = stacks().remove(0);
        group.label = long_group.clone();
        group.caption = Some(long_note.clone());
        group.bars[0].name = long_name.clone();
        group.bars[0].note = long_note.clone();
        let out = render_to_string(&[group], &gauge(), &fit_data(), 80, 24);
        for needle in [&long_name, &long_group, &long_note] {
            assert!(
                !out.contains(needle.as_str()),
                "a long label was rendered unshortened:\n{out}"
            );
        }
        assert!(out.contains('\u{2026}'), "no ellipsis marker:\n{out}");
        // Short labels must survive untouched next to the shortened ones.
        assert!(out.contains("No amortisation"), "bars:\n{out}");
    }

    #[test]
    fn empty_data_renders_both_placeholders() {
        let empty = UdpGaugeData {
            axis_min: 0,
            axis_max: 0,
            markers: Vec::new(),
            source_note: String::new(),
        };
        let out = render_to_string(&[], &empty, &fit_data(), 80, 24);
        assert!(out.contains("no rows recorded"), "bars placeholder:\n{out}");
        assert!(
            out.contains("no capture or wire data recorded"),
            "gauge placeholder:\n{out}"
        );
    }
}
