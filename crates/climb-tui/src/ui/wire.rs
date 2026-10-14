//! The Wire tab (phases 21.1-21.3): per-size stacked bars showing the shared
//! ladder and per-message condensed bytes against the full signatures they
//! replace, beside a single-answer UDP-fit gauge. On wide terminals the two
//! panels sit side by side; below [`WIDE_WIDTH`] the gauge collapses beneath
//! the bars so neither is clipped. All numbers come from
//! [`crate::data::ResultsView`]; nothing here hardcodes a result.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use crate::data::{ResultsView, UdpGaugeData};
use crate::theme::{Glyphs, Theme};
use crate::widgets::stacked_bars::{SegmentStyle, StackGroup, StackedBars};
use crate::widgets::udp_gauge::UdpGauge;

/// Terminal width at or above which the bars and gauge share one row.
const WIDE_WIDTH: u16 = 100;
/// Height reserved for the gauge when the panels stack vertically.
const GAUGE_STACK_HEIGHT: u16 = 9;

/// Render the Wire tab into `area`.
pub fn render_wire(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let stacks = view.wire_stacks();
    let gauge = view.udp_gauge_data();
    render_panels(frame, area, &stacks, &gauge, theme, glyphs);
}

/// Place the bars and the gauge: side by side when wide, stacked when narrow.
fn layout(area: Rect) -> (Rect, Rect) {
    if area.width >= WIDE_WIDTH {
        let [bars, gauge] =
            Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                .areas(area);
        (bars, gauge)
    } else {
        let gauge_height = GAUGE_STACK_HEIGHT.min(area.height);
        let [bars, gauge] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(gauge_height)]).areas(area);
        (bars, gauge)
    }
}

fn render_panels(
    frame: &mut Frame,
    area: Rect,
    stacks: &[StackGroup],
    gauge: &UdpGaugeData,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let (bars_area, gauge_area) = layout(area);

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};
    use crate::widgets::stacked_bars::{StackBar, StackSegment};
    use crate::widgets::udp_gauge::{nice_axis_max, GaugeMarker, MarkerKind};

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

    fn render_to_string(
        stacks: &[StackGroup],
        gauge: &UdpGaugeData,
        width: u16,
        height: u16,
    ) -> String {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                render_panels(frame, frame.area(), stacks, gauge, &theme, &glyphs);
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
        let (bars, gauge) = layout(Rect::new(0, 0, 120, 40));
        assert_eq!(bars.y, gauge.y, "wide layout shares a row");
        assert_eq!(bars.x, 0);
        assert!(gauge.x > 0, "gauge starts to the right of the bars");
        assert_eq!(bars.height, 40);
        assert_eq!(gauge.height, 40);
    }

    #[test]
    fn layout_stacks_vertically_when_narrow() {
        let (bars, gauge) = layout(Rect::new(0, 0, 80, 24));
        assert_eq!(bars.x, gauge.x, "narrow layout shares a column");
        assert_eq!(bars.width, 80);
        assert_eq!(gauge.width, 80);
        assert!(gauge.y > bars.y, "gauge sits below the bars");
        assert!(bars.height > 0 && gauge.height > 0, "both panels get space");
    }

    #[test]
    fn renders_bars_and_gauge_at_120x40() {
        let out = render_to_string(&stacks(), &gauge(), 120, 40);
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
    fn renders_bars_and_gauge_at_80x24_without_clipping() {
        let out = render_to_string(&stacks(), &gauge(), 80, 24);
        assert!(out.contains("Wire"), "bars panel:\n{out}");
        assert!(out.contains("Single-answer UDP fit"), "gauge panel:\n{out}");
    }

    #[test]
    fn empty_data_renders_both_placeholders() {
        let empty = UdpGaugeData {
            axis_min: 0,
            axis_max: 0,
            markers: Vec::new(),
            source_note: String::new(),
        };
        let out = render_to_string(&[], &empty, 80, 24);
        assert!(out.contains("no rows recorded"), "bars placeholder:\n{out}");
        assert!(
            out.contains("no capture or wire data recorded"),
            "gauge placeholder:\n{out}"
        );
    }
}
