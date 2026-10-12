//! UDP-fit gauge (phases 21.2, 21.3).
//!
//! Answers one question: does an answer fit in a single UDP datagram? Three
//! DNS reference sizes anchor the axis — the classic 512-byte limit, the
//! 1232-byte EDNS buffer mandated by DNS Flag Day 2020
//! (<https://dnsflagday.net/2020/>), and 4096, the largest EDNS buffer a
//! resolver may advertise. The fit decision is a comparison ([`verdict`]) and
//! the marker column is a linear projection ([`position`]); both are pure and
//! unit-tested, and every displayed label is derived from them rather than
//! hard-coded.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use climb_report::charts::fmt_bytes;

use crate::theme::{Glyphs, Theme};

/// The classic minimum UDP payload every DNS server must accept, in bytes.
pub const CLASSIC_UDP_LIMIT: u64 = 512;
/// The EDNS buffer size mandated by DNS Flag Day 2020, in bytes.
pub const EDNS_UDP_LIMIT: u64 = 1232;
/// The largest EDNS buffer a resolver may advertise, in bytes.
pub const EDNS_MAX_LIMIT: u64 = 4096;

/// The reference sizes drawn on the gauge, smallest first.
pub const REFERENCE_SIZES: [u64; 3] = [CLASSIC_UDP_LIMIT, EDNS_UDP_LIMIT, EDNS_MAX_LIMIT];

/// Whether a payload fits within a datagram of the given size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The payload fits: `bytes <= limit`.
    Fits,
    /// The payload is larger than the limit: `bytes > limit`.
    Exceeds,
}

/// Decide whether `bytes` fits within a datagram of `limit` bytes.
///
/// Equality fits: a payload exactly filling the buffer needs no fragmentation.
pub fn verdict(bytes: u64, limit: u64) -> Verdict {
    if bytes <= limit {
        Verdict::Fits
    } else {
        Verdict::Exceeds
    }
}

/// The smallest reference size that accepts `bytes`, or `None` if it exceeds
/// every reference (i.e. it needs TCP).
pub fn fitting_limit(bytes: u64) -> Option<u64> {
    REFERENCE_SIZES
        .into_iter()
        .find(|&limit| verdict(bytes, limit) == Verdict::Fits)
}

/// Project `bytes` onto a `width`-cell column on the range `axis_min..=axis_max`.
///
/// `axis_min` maps to column 0 and `axis_max` to column `width - 1`; values
/// outside the range clamp to those endpoints. A zero `width` or a degenerate
/// range (`axis_max <= axis_min`) returns 0 so a caller can never index out of
/// bounds.
pub fn position(bytes: u64, axis_min: u64, axis_max: u64, width: u16) -> u16 {
    if width == 0 || axis_max <= axis_min {
        return 0;
    }
    let clamped = bytes.clamp(axis_min, axis_max);
    let span = (axis_max - axis_min) as f64;
    let frac = (clamped - axis_min) as f64 / span;
    (frac * f64::from(width - 1)).round() as u16
}

/// Round `value` up to a readable axis maximum: the next half-decade on or
/// above it, so `24_438` becomes `25_000` and `700` stays `700`.
pub fn nice_axis_max(value: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    let magnitude = 10f64.powi((value as f64).log10().floor() as i32);
    let step = (magnitude / 2.0).max(1.0);
    ((value as f64 / step).ceil() * step) as u64
}

/// Which side of the comparison a marker represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// The full (un-amortised) signed answer.
    Full,
    /// The condensed MTL proof.
    Condensed,
}

/// One labelled payload drawn as a marker on the gauge.
#[derive(Debug, Clone, PartialEq)]
pub struct GaugeMarker {
    pub label: String,
    pub bytes: u64,
    pub kind: MarkerKind,
}

/// A linear UDP-fit gauge: reference-size ruler, a marker track, and one
/// computed verdict line per marker.
pub struct UdpGauge<'a> {
    pub theme: &'a Theme,
    pub glyphs: &'a Glyphs,
    pub title: String,
    pub axis_min: u64,
    pub axis_max: u64,
    pub markers: Vec<GaugeMarker>,
    /// Where the numbers came from, e.g. the live capture or a wire fallback.
    pub source_note: String,
    /// Shown instead of the verdict lines when there are no markers.
    pub empty_note: String,
}

impl UdpGauge<'_> {
    fn marker_glyph(&self, kind: MarkerKind) -> char {
        match kind {
            // Reuse the solid/hollow convention: full = solid, condensed = hollow.
            MarkerKind::Full => self.glyphs.bar_filled,
            MarkerKind::Condensed => self.glyphs.bar_empty,
        }
    }

    fn marker_style(&self, kind: MarkerKind) -> Style {
        match kind {
            MarkerKind::Full => self.theme.measured,
            MarkerKind::Condensed => self.theme.extrapolated,
        }
    }

    /// One line per marker: `glyph label — bytes, computed verdict`.
    fn verdict_lines(&self) -> Vec<Line<'static>> {
        self.markers
            .iter()
            .map(|marker| {
                let text = match fitting_limit(marker.bytes) {
                    Some(limit) => format!("fits within {}", fmt_bytes(limit as f64),),
                    None => format!("exceeds {} (TCP)", fmt_bytes(EDNS_MAX_LIMIT as f64),),
                };
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        self.marker_glyph(marker.kind).to_string(),
                        self.marker_style(marker.kind),
                    ),
                    Span::raw(" "),
                    Span::styled(marker.label.clone(), Style::new().fg(self.theme.fg)),
                    Span::styled(
                        format!("  {} \u{00b7} ", fmt_bytes(marker.bytes as f64)),
                        Style::new().fg(self.theme.muted),
                    ),
                    Span::styled(text, self.marker_style(marker.kind)),
                ])
            })
            .collect()
    }

    /// The body lines within `width` cells: the marker track, the reference
    /// ruler, the source note, then the verdict lines.
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if self.markers.is_empty() {
            return vec![Line::from(Span::styled(
                format!(" {}", self.empty_note),
                Style::new().fg(self.theme.muted),
            ))];
        }
        let track_width = width.max(1);

        let mut track: Vec<char> = vec![' '; track_width as usize];
        for &reference in &REFERENCE_SIZES {
            if reference < self.axis_min || reference > self.axis_max {
                continue;
            }
            let column = position(reference, self.axis_min, self.axis_max, track_width);
            track[column as usize] = self.glyphs.rail;
        }
        for marker in &self.markers {
            let column = position(marker.bytes, self.axis_min, self.axis_max, track_width);
            track[column as usize] = self.marker_glyph(marker.kind);
        }
        let track_line: String = track.into_iter().collect();

        let mut lines = vec![
            Line::from(Span::styled(
                format!(" {track_line}"),
                Style::new().fg(self.theme.muted),
            )),
            ruler_line(self, track_width),
            Line::from(Span::styled(
                format!(" {}", self.source_note),
                Style::new().fg(self.theme.muted),
            )),
        ];
        lines.extend(self.verdict_lines());
        lines
    }
}

/// Stamp each in-range reference size's byte label under its column, skipping
/// any that would overlap the previous label.
fn ruler_line(gauge: &UdpGauge<'_>, width: u16) -> Line<'static> {
    let mut cells: Vec<char> = vec![' '; width as usize];
    let mut last_end: u16 = 0;
    for &reference in &REFERENCE_SIZES {
        if reference < gauge.axis_min || reference > gauge.axis_max {
            continue;
        }
        let column = position(reference, gauge.axis_min, gauge.axis_max, width);
        let label = fmt_bytes(reference as f64);
        let chars: Vec<char> = label.chars().collect();
        let end = column + chars.len() as u16;
        if column < last_end || end > width {
            continue;
        }
        for (i, ch) in chars.iter().enumerate() {
            cells[column as usize + i] = *ch;
        }
        last_end = end + 1;
    }
    let ruler: String = cells.into_iter().collect();
    Line::from(Span::styled(
        format!(" {ruler}"),
        Style::new().fg(gauge.theme.muted),
    ))
}

impl Widget for UdpGauge<'_> {
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

    fn theme() -> Theme {
        Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false)
    }

    fn render_text(gauge: UdpGauge<'_>, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        gauge.render(area, &mut buffer);
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

    fn sample_gauge<'a>(theme: &'a Theme, glyphs: &'a Glyphs) -> UdpGauge<'a> {
        UdpGauge {
            theme,
            glyphs,
            title: "UDP fit".to_string(),
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
            empty_note: "no capture recorded".to_string(),
        }
    }

    #[test]
    fn verdict_boundary_is_inclusive() {
        assert_eq!(verdict(512, 512), Verdict::Fits, "== limit fits");
        assert_eq!(verdict(513, 512), Verdict::Exceeds, "limit + 1 does not");
        assert_eq!(verdict(0, 512), Verdict::Fits);
        assert_eq!(verdict(u64::MAX, EDNS_MAX_LIMIT), Verdict::Exceeds);
    }

    #[test]
    fn fitting_limit_picks_the_smallest_accepting_reference() {
        assert_eq!(fitting_limit(100), Some(CLASSIC_UDP_LIMIT));
        assert_eq!(fitting_limit(512), Some(CLASSIC_UDP_LIMIT));
        assert_eq!(fitting_limit(513), Some(EDNS_UDP_LIMIT));
        assert_eq!(fitting_limit(1232), Some(EDNS_UDP_LIMIT));
        assert_eq!(fitting_limit(1233), Some(EDNS_MAX_LIMIT));
        assert_eq!(fitting_limit(4096), Some(EDNS_MAX_LIMIT));
        assert_eq!(fitting_limit(4097), None);
    }

    #[test]
    fn position_maps_endpoints_and_clamps() {
        let width = 21;
        assert_eq!(position(0, 0, 1000, width), 0, "axis_min -> column 0");
        assert_eq!(
            position(1000, 0, 1000, width),
            20,
            "axis_max -> last column"
        );
        assert_eq!(position(500, 0, 1000, width), 10, "midpoint");
        assert_eq!(position(4000, 0, 1000, width), 20, "above max clamps");
        assert_eq!(position(0, 100, 1000, width), 0, "below min clamps");
        assert!(position(100, 0, 1000, width) <= position(900, 0, 1000, width));
    }

    #[test]
    fn position_handles_degenerate_inputs() {
        assert_eq!(position(5, 0, 1000, 0), 0, "zero width");
        assert_eq!(position(5, 100, 100, 10), 0, "empty range");
        assert_eq!(position(5, 100, 50, 10), 0, "inverted range");
    }

    #[test]
    fn nice_axis_max_rounds_up_to_a_half_decade() {
        assert_eq!(nice_axis_max(0), 0);
        assert_eq!(nice_axis_max(24_438), 25_000);
        assert_eq!(nice_axis_max(700), 700);
        assert_eq!(nice_axis_max(1), 1);
        assert!(nice_axis_max(4097) >= 4097);
    }

    #[test]
    fn renders_markers_ruler_and_computed_verdicts() {
        let theme = theme();
        let glyphs = theme.glyphs();
        let out = render_text(sample_gauge(&theme, &glyphs), 70, 9);
        assert!(out.contains("UDP fit"), "title:\n{out}");
        assert!(out.contains("full signature"), "marker label:\n{out}");
        assert!(out.contains("condensed proof"));
        assert!(
            out.contains(&fmt_bytes(24_438.0)),
            "formatted marker size:\n{out}"
        );
        assert!(out.contains(&fmt_bytes(658.0)));
        assert!(
            out.contains(&format!("exceeds {} (TCP)", fmt_bytes(4096.0))),
            "full exceeds every buffer:\n{out}"
        );
        assert!(
            out.contains(&format!("fits within {}", fmt_bytes(1232.0))),
            "condensed fits the EDNS limit:\n{out}"
        );
    }

    #[test]
    fn empty_gauge_renders_its_note() {
        let theme = theme();
        let glyphs = theme.glyphs();
        let gauge = UdpGauge {
            theme: &theme,
            glyphs: &glyphs,
            title: "UDP fit".to_string(),
            axis_min: 0,
            axis_max: 5000,
            markers: Vec::new(),
            source_note: String::new(),
            empty_note: "no capture recorded".to_string(),
        };
        let out = render_text(gauge, 70, 5);
        assert!(out.contains("no capture recorded"), "empty:\n{out}");
    }
}
