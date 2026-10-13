//! Memory tab view-model: measured vs analytical stacked bars.

use climb_report::charts::fmt_bytes;

use crate::widgets::stacked_bars::{SegmentStyle, StackBar, StackGroup, StackSegment};

use super::{title_size, ResultsView};

impl ResultsView {
    /// Per-size measured-vs-analytical memory bars (phase 21.4).
    ///
    /// The analytical bar is the libMTL page model plus harness buffers (when
    /// the F8 fields are present); the measured bar is the mean RSS delta. The
    /// caption carries the residual and the measured/model ratio.
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
                let model_note = if has_harness {
                    ""
                } else {
                    " (pre-F8: no harness model)"
                };
                let caption = format!(
                    "residual {} ({:+.1}%) \u{00b7} measured/model {:.2}\u{00d7}{}",
                    fmt_bytes(row.residual_bytes as f64),
                    row.residual_percent,
                    row.measured_analytical_total_ratio,
                    model_note,
                );
                StackGroup {
                    label: title_size(&row.size),
                    scale_max: row
                        .measured_mean_bytes
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
                                bytes: row.measured_mean_bytes,
                                style: SegmentStyle::Solid,
                            }],
                            note: format!(
                                "{} \u{00b1}{}",
                                fmt_bytes(row.measured_mean_bytes as f64),
                                fmt_bytes(row.measured_stddev_bytes),
                            ),
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
            2,
            "libMTL pages + harness buffers"
        );
        let sum: u64 = analytical.segments.iter().map(|s| s.bytes).sum();
        assert_eq!(sum, 2_100_384, "analytical segments sum to the total");
        let caption = stacks[0].caption.as_deref().unwrap();
        assert!(caption.contains("-59.4%"), "{caption}");
        assert!(caption.contains("measured/model 0.41"), "{caption}");
        assert!(view.memory_has_harness());
    }

    #[test]
    fn memory_stacks_drop_harness_when_absent() {
        let view = view_with_memory(vec![memory_result("small", 100, 851_968, 2_097_152, 0)]);
        let stacks = view.memory_stacks();
        let analytical = &stacks[0].bars[0];
        assert_eq!(analytical.segments.len(), 1, "libMTL-only when F8 absent");
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
