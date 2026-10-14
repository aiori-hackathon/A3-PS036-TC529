//! Memory tab view-model: measured vs analytical stacked bars.

use climb_report::charts::fmt_bytes;

use crate::widgets::stacked_bars::{SegmentStyle, StackBar, StackGroup, StackSegment};

use super::{title_size, ResultsView};

impl ResultsView {
    /// Per-size measured-vs-analytical memory bars.
    ///
    /// The analytical bar is the itemised model (libMTL page model, harness
    /// buffers when present, dataset message buffers, keypair image); the
    /// measured bar is the resident RSS delta (median of n isolated-process
    /// runs). The caption carries the headline peak, the resident median and
    /// IQR, the residual, and the resident/model ratio.
    pub fn memory_stacks(&self) -> Vec<StackGroup> {
        let mut rows: Vec<_> = self.results.memory.measured.iter().collect();
        rows.sort_by_key(|row| row.message_count);
        rows.into_iter()
            .map(|row| {
                let has_harness = row.harness_buffers_bytes > 0;
                let mut analytical_segments = vec![StackSegment {
                    label: "libMTL pages".to_string(),
                    bytes: row.libmtl_analytical_bytes as u64,
                    style: SegmentStyle::Hatched,
                }];
                if has_harness {
                    analytical_segments.push(StackSegment {
                        label: "harness buffers".to_string(),
                        bytes: row.harness_buffers_bytes,
                        style: SegmentStyle::Dotted,
                    });
                }
                if row.dataset_buffers_bytes > 0 {
                    analytical_segments.push(StackSegment {
                        label: "dataset buffers".to_string(),
                        bytes: row.dataset_buffers_bytes,
                        style: SegmentStyle::Dotted,
                    });
                }
                if row.keypair_bytes > 0 {
                    analytical_segments.push(StackSegment {
                        label: "keypair image".to_string(),
                        bytes: row.keypair_bytes,
                        style: SegmentStyle::Dotted,
                    });
                }
                let model_note = if has_harness {
                    ""
                } else {
                    " (pre-F8: no harness model)"
                };
                let caption = format!(
                    "peak {} \u{00b7} ratio {:.2}\u{00d7} \u{00b7} resid {} ({:+.1}%) \u{00b7} n={}, IQR {}{}",
                    fmt_bytes(row.measured_peak_bytes as f64),
                    row.measured_analytical_total_ratio,
                    fmt_bytes(row.residual_bytes as f64),
                    row.residual_percent,
                    row.samples,
                    fmt_bytes(row.iqr_bytes as f64),
                    model_note,
                );
                StackGroup {
                    label: title_size(&row.size),
                    scale_max: row
                        .measured_mean_bytes
                        .max(row.measured_peak_bytes)
                        .max(row.analytical_total_bytes)
                        .max(1),
                    caption: Some(caption),
                    bars: vec![
                        StackBar {
                            name: "Analytical".to_string(),
                            segments: analytical_segments,
                            note: fmt_bytes(row.analytical_total_bytes as f64),
                        },
                        StackBar {
                            name: "Measured".to_string(),
                            segments: vec![StackSegment {
                                label: "measured RSS".to_string(),
                                bytes: row.median_bytes,
                                style: SegmentStyle::Solid,
                            }],
                            note: fmt_bytes(row.median_bytes as f64),
                        },
                    ],
                }
            })
            .collect()
    }

    /// Whether any memory row models harness buffers (the F8 fields are
    /// present and non-zero).
    pub fn memory_has_harness(&self) -> bool {
        self.results
            .memory
            .measured
            .iter()
            .any(|row| row.harness_buffers_bytes > 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::data::test_support::{memory_result, view_with_memory};

    #[test]
    fn memory_stacks_sum_segments_and_report_residual() {
        let view = view_with_memory(vec![
            memory_result("large", 1_000_000, 320_679_936, 49_283_072, 32_000_032),
            memory_result("small", 100, 851_968, 2_097_152, 3_232),
        ]);
        let stacks = view.memory_stacks();
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[0].label, "Small");
        let analytical = &stacks[0].bars[0];
        assert_eq!(
            analytical.segments.len(),
            4,
            "libMTL pages + harness + dataset buffers + keypair image"
        );
        let sum: u64 = analytical.segments.iter().map(|s| s.bytes).sum();
        let expected = 2_097_152u64 + 3_232 + 100 * 536 + 186;
        assert_eq!(sum, expected, "analytical segments sum to the total");
        let caption = stacks[0].caption.as_deref().unwrap();
        assert!(caption.contains("-60.5%"), "{caption}");
        assert!(caption.contains("ratio 0.40"), "{caption}");
        assert!(view.memory_has_harness());
    }

    #[test]
    fn memory_stacks_drop_harness_when_absent() {
        let view = view_with_memory(vec![memory_result("small", 100, 851_968, 2_097_152, 0)]);
        let stacks = view.memory_stacks();
        let analytical = &stacks[0].bars[0];
        assert_eq!(
            analytical.segments.len(),
            3,
            "harness dropped; libMTL + dataset buffers + keypair remain"
        );
        assert_eq!(analytical.segments[0].bytes, 2_097_152);
        let caption = stacks[0].caption.as_deref().unwrap();
        assert!(caption.contains("pre-F8"), "{caption}");
        assert!(!view.memory_has_harness());
    }

    #[test]
    fn memory_stacks_is_empty_without_rows() {
        let view = view_with_memory(Vec::new());
        assert!(view.memory_stacks().is_empty());
        assert!(!view.memory_has_harness());
    }
}
