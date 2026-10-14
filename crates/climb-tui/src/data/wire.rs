//! Wire tab view-model: per-size stacked bars and the UDP-fit gauge.
//!
//! Every number comes from the parsed results carried by
//! [`crate::data::ResultsView`]; nothing here hardcodes a result.

use climb_report::charts::fmt_count;
use climb_report::condition_short_label;

use crate::widgets::stacked_bars::{SegmentStyle, StackBar, StackGroup, StackSegment};
use crate::widgets::udp_gauge::{nice_axis_max, GaugeMarker, MarkerKind, REFERENCE_SIZES};

use super::{title_size, ResultsView};

/// Render-ready payloads and axis for the UDP-fit gauge.
#[derive(Debug, Clone, PartialEq)]
pub struct UdpGaugeData {
    pub axis_min: u64,
    pub axis_max: u64,
    pub markers: Vec<GaugeMarker>,
    /// Where the numbers came from: the DNS capture or the wire fallback.
    pub source_note: String,
}

impl ResultsView {
    /// Per-size wire breakdown for the stacked bars.
    ///
    /// Each size gets an `MTL amortised` bar split into the shared ladder and
    /// the per-message condensed bytes, and a `No amortisation` bar of full
    /// signatures. When baseline == 0, a `Plain SLH-DSA` bar (N × plain
    /// signature length) is added as the leftmost bar. Bars normalise per size
    /// on the rightmost bar's total, so the amortisation reads directly as bar
    /// length.
    pub fn wire_stacks(&self, baseline: usize) -> Vec<StackGroup> {
        let mut rows: Vec<_> = self.results.wire_size.iter().collect();
        rows.sort_by_key(|row| row.message_count);
        rows.into_iter()
            .map(|row| {
                let condensed_total = row
                    .with_mtl_total_bytes
                    .saturating_sub(row.ladder_len as u64);
                let uniform = if row.condensed_uniform {
                    " (uniform)"
                } else {
                    ""
                };
                let caption = format!(
                    "condensed signature: min {} B / mean {} B / max {} B{}",
                    fmt_count(row.condensed_min as f64),
                    fmt_count(row.condensed_mean),
                    fmt_count(row.condensed_max as f64),
                    uniform,
                );
                let mut bars = Vec::new();
                if baseline == 0 {
                    // Config A: Plain SLH-DSA (N × plain signature length)
                    if row.plain_total_bytes > 0 {
                        bars.push(StackBar {
                            name: "Plain SLH-DSA".to_string(),
                            segments: vec![StackSegment {
                                label: format!(
                                    "{} \u{00d7} plain",
                                    fmt_count(row.message_count as f64)
                                ),
                                bytes: row.plain_total_bytes,
                                style: SegmentStyle::Solid,
                            }],
                            note: format!("{} B", fmt_count(row.plain_total_bytes as f64)),
                        });
                    }
                }
                // Config C: MTL amortised
                bars.push(StackBar {
                    name: condition_short_label("With MTL").to_string(),
                    segments: vec![
                        StackSegment {
                            label: "shared ladder".to_string(),
                            bytes: row.ladder_len as u64,
                            style: SegmentStyle::Hatched,
                        },
                        StackSegment {
                            label: format!(
                                "{} \u{00d7} condensed",
                                fmt_count(row.message_count as f64)
                            ),
                            bytes: condensed_total,
                            style: SegmentStyle::Dotted,
                        },
                    ],
                    note: format!(
                        "{} B \u{00b7} \u{2212}{:.1}%",
                        fmt_count(row.with_mtl_total_bytes as f64),
                        row.reduction_percent,
                    ),
                });
                // Config B: MTL non-amortised (baseline 1 or 2)
                bars.push(StackBar {
                    name: condition_short_label("Without MTL").to_string(),
                    segments: vec![StackSegment {
                        label: format!("{} \u{00d7} full", fmt_count(row.message_count as f64)),
                        bytes: row.without_mtl_total_bytes,
                        style: SegmentStyle::Solid,
                    }],
                    note: format!("{} B", fmt_count(row.without_mtl_total_bytes as f64)),
                });
                StackGroup {
                    label: title_size(&row.size),
                    scale_max: row.without_mtl_total_bytes.max(1),
                    caption: Some(caption),
                    bars,
                }
            })
            .collect()
    }

    /// Payloads, axis and provenance for the UDP-fit gauge.
    ///
    /// Prefers the DNS capture's full-signature and condensed-response sizes;
    /// with no capture present it falls back to the largest wire-size report's
    /// per-record sizes and names the source in the note.
    pub fn udp_gauge_data(&self) -> UdpGaugeData {
        if let Some((full, condensed, note)) = self.udp_from_capture() {
            return Self::gauge_from_markers(
                vec![
                    GaugeMarker {
                        label: "full signature".to_string(),
                        bytes: full,
                        kind: MarkerKind::Full,
                    },
                    GaugeMarker {
                        label: "condensed proof".to_string(),
                        bytes: condensed,
                        kind: MarkerKind::Condensed,
                    },
                ],
                note,
            );
        }
        if let Some((full, condensed)) = self.udp_from_wire() {
            return Self::gauge_from_markers(
                vec![
                    GaugeMarker {
                        label: "full signature".to_string(),
                        bytes: full,
                        kind: MarkerKind::Full,
                    },
                    GaugeMarker {
                        label: "condensed proof".to_string(),
                        bytes: condensed,
                        kind: MarkerKind::Condensed,
                    },
                ],
                "no DNS capture \u{2014} per-record sizes from the wire report".to_string(),
            );
        }
        UdpGaugeData {
            axis_min: 0,
            axis_max: 0,
            markers: Vec::new(),
            source_note: "no wire or capture data recorded".to_string(),
        }
    }

    fn udp_from_capture(&self) -> Option<(u64, u64, String)> {
        let dns = self.results.dns_pipeline.as_ref()?;
        let full = dns.full_signature_bytes_www_a?;
        let condensed = dns.condensed_proof_bytes_www_a?;
        let note = self
            .dns_provenance()
            .map(|provenance| format!("from {provenance}"))
            .unwrap_or_else(|| "from the DNS capture".to_string());
        Some((full, condensed, note))
    }

    fn udp_from_wire(&self) -> Option<(u64, u64)> {
        let row = self
            .results
            .wire_size
            .iter()
            .max_by_key(|row| row.message_count)?;
        let condensed = row.condensed_max as u64;
        let full = row.ladder_len as u64 + condensed;
        Some((full, condensed))
    }

    fn gauge_from_markers(markers: Vec<GaugeMarker>, source_note: String) -> UdpGaugeData {
        let peak = markers
            .iter()
            .map(|marker| marker.bytes)
            .chain(REFERENCE_SIZES)
            .max()
            .unwrap_or(0);
        UdpGaugeData {
            axis_min: 0,
            axis_max: nice_axis_max(peak),
            markers,
            source_note,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data::test_support::{dns_headline, empty_derived, view_with, wire_size_row};

    #[test]
    fn wire_stacks_split_ladder_and_condensed_with_per_size_scale() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            vec![
                wire_size_row(
                    "large",
                    1_000_000,
                    8120,
                    367.06,
                    367_067_832,
                    8_487_059_712,
                    95.675,
                ),
                wire_size_row("small", 100, 7992, 164.32, 24_424, 815_632, 97.005),
            ],
            None,
            empty_derived(),
        );
        let stacks = view.wire_stacks(1);
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[0].label, "Small", "sizes sort ascending by messages");
        assert_eq!(stacks[1].label, "Large");
        assert_eq!(stacks[0].scale_max, 815_632);

        let with = &stacks[0].bars[0];
        let segment_sum: u64 = with.segments.iter().map(|s| s.bytes).sum();
        assert_eq!(
            segment_sum, 24_424,
            "ladder + N*condensed == with-MTL total"
        );
        assert_eq!(with.segments[0].bytes, 7992, "shared ladder segment");
        assert_eq!(
            with.segments[1].bytes,
            24_424 - 7992,
            "condensed segment is total minus ladder"
        );
        assert!(with.note.contains("97.0%"), "reduction %: {}", with.note);

        let without = &stacks[0].bars[1];
        assert_eq!(without.segments[0].bytes, 815_632);
        assert_eq!(without.total(), 815_632);

        let caption = stacks[0].caption.as_deref().unwrap();
        assert!(caption.contains("min 164 B"), "{caption}");
        assert!(caption.contains("max 164 B"), "{caption}");
    }

    #[test]
    fn wire_stacks_is_empty_without_wire_rows() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        assert!(view.wire_stacks(1).is_empty());
    }

    #[test]
    fn udp_gauge_prefers_the_dns_capture() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            vec![wire_size_row(
                "small", 100, 7992, 164.32, 24_424, 815_632, 97.005,
            )],
            Some(dns_headline(24_438, 658)),
            empty_derived(),
        );
        let gauge = view.udp_gauge_data();
        assert_eq!(gauge.markers.len(), 2);
        assert_eq!(gauge.markers[0].bytes, 24_438);
        assert_eq!(gauge.markers[1].bytes, 658);
        assert!(gauge.axis_max >= 24_438);
        assert!(
            gauge.source_note.contains("recorded"),
            "{}",
            gauge.source_note
        );
    }

    #[test]
    fn udp_gauge_falls_back_to_per_record_wire_sizes() {
        let view = view_with(
            Vec::new(),
            Vec::new(),
            vec![wire_size_row(
                "small", 100, 7992, 164.32, 24_424, 815_632, 97.005,
            )],
            None,
            empty_derived(),
        );
        let gauge = view.udp_gauge_data();
        assert_eq!(gauge.markers[0].bytes, 7992 + 164, "ladder + one condensed");
        assert_eq!(gauge.markers[1].bytes, 164);
        assert!(
            gauge.source_note.contains("no DNS capture"),
            "{}",
            gauge.source_note
        );
    }

    #[test]
    fn udp_gauge_is_empty_without_any_source() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        let gauge = view.udp_gauge_data();
        assert!(gauge.markers.is_empty());
        assert_eq!(gauge.axis_max, 0);
    }
}
