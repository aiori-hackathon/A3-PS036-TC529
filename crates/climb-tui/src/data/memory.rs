//! Memory tab view-model: measured vs analytical stacked bars.

use climb_report::charts::fmt_bytes;

use crate::widgets::stacked_bars::{SegmentStyle, StackBar, StackGroup, StackSegment};

use climb_report::results::size_display;

use super::ResultsView;

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
                    label: size_display(&row.size),
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

    /// Resolver-cache rows for the Memory tab, kept separate from the
    /// benchmark-process memory above: each row pairs the measured Unbound
    /// cache bytes for one (size, configuration) cell with the analytical
    /// prediction for the same RRset count, plus the median container RSS
    /// growth observed over the runs.
    pub fn resolver_cache_rows(&self) -> Vec<ResolverCacheRow> {
        let Some(cache) = self.results.cache_use.as_ref() else {
            return Vec::new();
        };
        cache
            .analytical
            .iter()
            .map(|row| {
                let size = self
                    .results
                    .wire_size
                    .iter()
                    .find(|wire| wire.message_count == row.r)
                    .map(|wire| size_display(&wire.size))
                    .unwrap_or_else(|| format!("r={}", row.r));
                let mut deltas: Vec<u64> = cache
                    .measured
                    .iter()
                    .filter(|run| run.r == row.r && run.config == row.config)
                    .map(|run| {
                        run.container_rss_after_bytes
                            .saturating_sub(run.container_rss_before_bytes)
                    })
                    .collect();
                deltas.sort_unstable();
                let rss_delta_bytes = if deltas.is_empty() {
                    0
                } else {
                    deltas.get(deltas.len() / 2).copied().unwrap_or(0)
                };
                ResolverCacheRow {
                    size,
                    config: row.config.clone(),
                    r: row.r,
                    measured_bytes: row.measured_bytes,
                    analytical_bytes: row.analytical_bytes,
                    ratio: row.measured_over_analytical,
                    rss_delta_bytes,
                }
            })
            .collect()
    }

    /// Whether the artifact carries a resolver-cache section to render.
    pub fn has_resolver_cache(&self) -> bool {
        self.results
            .cache_use
            .as_ref()
            .is_some_and(|cache| !cache.analytical.is_empty())
    }
}

/// One resolver-cache cell for the Memory tab: measured Unbound cache bytes,
/// the analytical prediction for the same RRset count, and the median container
/// RSS growth over the runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolverCacheRow {
    /// Dataset label the RRset count maps to (`Small`, `Medium`, or `r=<n>`).
    pub size: String,
    /// Resolver configuration (`plain` or `mtl-condensed`).
    pub config: String,
    /// Number of distinct RRset owner names loaded.
    pub r: usize,
    /// Measured cache bytes (median of the runs for this cell).
    pub measured_bytes: u64,
    /// Analytical predicted cache bytes for the same RRset count.
    pub analytical_bytes: u64,
    /// `measured_bytes / analytical_bytes`, or `0.0` when no prediction.
    pub ratio: f64,
    /// Median container resident growth over the runs for this cell.
    pub rss_delta_bytes: u64,
}

#[cfg(test)]
mod tests {
    use crate::data::test_support::{
        memory_result, view_with_cache_section, view_with_memory, wire_row,
    };
    use crate::data::ResultsView;
    use climb_report::cache::{CacheAnalyticalRow, CacheResults, CacheRun, UnboundStats};

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

    fn cache_section(runs: Vec<(u64, u64)>, analytical: Vec<(usize, u64, u64)>) -> CacheResults {
        CacheResults {
            schema_version: 1,
            class: "MEASURED".to_string(),
            method: "unbound-control stats_noreset + container RSS".to_string(),
            measured: runs
                .into_iter()
                .map(|(rrset, rss_delta)| CacheRun {
                    config: "plain".to_string(),
                    r: 10_000,
                    container_rss_before_bytes: 1_000,
                    container_rss_after_bytes: 1_000 + rss_delta,
                    stats: UnboundStats {
                        rrset_cache_bytes: rrset,
                        rrset_cache_entries: 1,
                        message_cache_bytes: 0,
                        message_cache_entries: 1,
                        ladder_cache_bytes: 0,
                    },
                })
                .collect(),
            analytical: analytical
                .into_iter()
                .map(|(r, measured, predicted)| CacheAnalyticalRow {
                    config: "plain".to_string(),
                    r,
                    measured_bytes: measured,
                    analytical_bytes: predicted,
                    measured_over_analytical: if predicted == 0 {
                        0.0
                    } else {
                        measured as f64 / predicted as f64
                    },
                })
                .collect(),
        }
    }

    fn view_with_cache(cache: CacheResults) -> ResultsView {
        view_with_cache_section(vec![wire_row("medium", 10_000, 90.0)], cache)
    }

    #[test]
    fn resolver_cache_rows_pair_measured_with_analytical() {
        let view = view_with_cache(cache_section(
            vec![(10, 100), (20, 200), (30, 300), (40, 400), (50, 500)],
            vec![(10_000, 30, 60)],
        ));
        let rows = view.resolver_cache_rows();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.size, "Medium", "the RRset count maps to its wire size");
        assert_eq!(row.config, "plain");
        assert_eq!(row.measured_bytes, 30);
        assert_eq!(row.analytical_bytes, 60);
        assert!((row.ratio - 0.5).abs() < 1e-9);
        assert_eq!(row.rss_delta_bytes, 300, "median container growth");
        assert!(view.has_resolver_cache());
    }

    #[test]
    fn resolver_cache_rows_fall_back_to_the_rrset_count() {
        let view = view_with_cache(cache_section(Vec::new(), vec![(7, 10, 20)]));
        let rows = view.resolver_cache_rows();
        assert_eq!(rows[0].size, "r=7");
        assert_eq!(rows[0].rss_delta_bytes, 0, "no runs, no RSS figure");
    }

    #[test]
    fn resolver_cache_rows_are_empty_without_the_section() {
        let view = view_with_memory(Vec::new());
        assert!(view.resolver_cache_rows().is_empty());
        assert!(!view.has_resolver_cache());
    }

    #[test]
    fn committed_sample_cache_section_survives_the_tui_view() {
        let results: climb_report::ClimbResults = serde_json::from_str(include_str!(
            "../../../../docs/sample-output/climb-results.json"
        ))
        .expect("committed sample parses");
        let view = ResultsView::new(results, None, crate::data::ReportHealth::Agree);
        let rows = view.resolver_cache_rows();
        assert_eq!(rows.len(), 4, "two sizes x two configurations");
        assert!(view.has_resolver_cache());
        for row in &rows {
            assert!(
                matches!(row.size.as_str(), "Small" | "Medium"),
                "the RRset count maps to its dataset label: {}",
                row.size
            );
            assert!(row.measured_bytes > 0);
            assert!(row.analytical_bytes > 0);
            assert!(row.rss_delta_bytes > 0);
            assert!(row.ratio > 0.0);
        }
    }
}
