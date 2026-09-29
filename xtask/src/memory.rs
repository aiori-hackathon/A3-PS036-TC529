//! The `memory-report` subcommand: measured RSS delta vs analytical estimate.
//!
//! ```text
//! cargo xtask memory-report --sizes all
//! cargo xtask memory-report --sizes small --repeat 3 --out results/memory.json
//! ```

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;

use climb_analytical::estimate_memory_footprint;
use climb_analytical::DatasetSize as AnalyticalSize;
use climb_bench::{
    compare_to_analytical, measure_rss_delta_repeated, RssComparison, RssDeltaReport,
    RssSizeReport, DEFAULT_WIRE_SEED,
};
use climb_dns::DatasetSize as DnsDatasetSize;

use crate::SizeArg;

#[derive(Args)]
pub(crate) struct MemoryReportArgs {
    #[arg(long, value_delimiter = ',', default_value = "all")]
    sizes: Vec<SizeArg>,

    #[arg(long, default_value_t = DEFAULT_WIRE_SEED)]
    seed: u64,

    #[arg(long, default_value_t = 2)]
    repeat: usize,

    #[arg(long)]
    out: Option<PathBuf>,
}

pub(crate) fn cmd_memory_report(args: MemoryReportArgs) -> Result<()> {
    let sizes = selected_memory_sizes(&args.sizes);
    let mut reports: Vec<RssSizeReport> = Vec::with_capacity(sizes.len());
    let mut comparisons: Vec<RssComparison> = Vec::with_capacity(sizes.len());

    let analytical_size = |dns_size: DnsDatasetSize| -> AnalyticalSize {
        match dns_size {
            DnsDatasetSize::Small => AnalyticalSize::Small,
            DnsDatasetSize::OneK => AnalyticalSize::OneK,
            DnsDatasetSize::Medium => AnalyticalSize::Medium,
            DnsDatasetSize::Large => AnalyticalSize::Large,
        }
    };

    for &size in &sizes {
        let report = measure_rss_delta_repeated(size, args.seed, args.repeat)
            .with_context(|| format!("RSS measurement failed for {size:?}"))?;
        let analytical = estimate_memory_footprint(analytical_size(size).count());
        let cmp = compare_to_analytical(&report, analytical);
        comparisons.push(cmp);
        reports.push(report);
    }

    if let Some(path) = &args.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dirs for {}", path.display()))?;
        }
        let full_report = RssDeltaReport {
            seed: args.seed,
            runs: reports,
        };
        let json = serde_json::to_string_pretty(&full_report)
            .context("failed to serialize RssDeltaReport to JSON")?;
        fs::write(path, json)
            .with_context(|| format!("failed to write JSON to {}", path.display()))?;
        eprintln!("wrote memory-report to {}", path.display());
    }

    // Print cross-validation table.
    print_comparison_table(&comparisons, args.repeat);
    Ok(())
}

/// Blueprint sizes: Small, Medium, Large.
fn memory_sizes_default() -> Vec<DnsDatasetSize> {
    vec![
        DnsDatasetSize::Small,
        DnsDatasetSize::Medium,
        DnsDatasetSize::Large,
    ]
}

fn selected_memory_sizes(args: &[SizeArg]) -> Vec<DnsDatasetSize> {
    let mut sizes = Vec::new();
    for arg in args {
        match arg {
            SizeArg::All => {
                for size in memory_sizes_default() {
                    if !sizes.contains(&size) {
                        sizes.push(size);
                    }
                }
            }
            _ => {
                let size = arg_to_dns_size(*arg);
                if !sizes.contains(&size) {
                    sizes.push(size);
                }
            }
        }
    }
    sizes
}

fn arg_to_dns_size(arg: SizeArg) -> DnsDatasetSize {
    match arg {
        SizeArg::Small => DnsDatasetSize::Small,
        SizeArg::OneK => DnsDatasetSize::OneK,
        SizeArg::Medium => DnsDatasetSize::Medium,
        SizeArg::Large => DnsDatasetSize::Large,
        SizeArg::All => unreachable!("SizeArg::All handled in selected_memory_sizes"),
    }
}

fn print_comparison_table(comparisons: &[RssComparison], repeat_count: usize) {
    use std::io::{self, Write};

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    let _ = writeln!(handle);
    let _ = writeln!(
        handle,
        "RSS delta (measured) vs analytical estimate — {repeat_count} runs per size"
    );
    let _ = writeln!(handle);

    let header = format!(
        "{:<6} {:>10} {:>16} {:>15} {:>12} {:>14} {:>16}",
        "Size", "N", "Measured (mean)", "Analytical (B)", "Ratio", "Range (min–max)", "StdDev (B)",
    );
    let sep = "-".repeat(header.len());
    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{sep}");

    for row in comparisons {
        let size_label = match row.size {
            DnsDatasetSize::Small => "small",
            DnsDatasetSize::OneK => "onek",
            DnsDatasetSize::Medium => "medium",
            DnsDatasetSize::Large => "large",
        };

        let (lo, hi) = row.measured_range_bytes;
        let _ = writeln!(
            handle,
            "{size_label:<6} {n:>10} {measured:>16} {analytical:>15} {ratio:>11.3}x {lo:>14}–{hi:<14} {stddev:>15.0}",
            size_label = size_label,
            n = row.message_count,
            measured = row.measured_mean_bytes,
            analytical = row.analytical_bytes,
            ratio = row.ratio,
            lo = lo,
            hi = hi,
            stddev = row.measured_stddev_bytes,
        );
    }

    let _ = writeln!(handle);
    let _ = writeln!(handle, "Interpretation:");
    let _ = writeln!(
        handle,
        "  Ratio < 1.0 → measured delta is smaller than analytical (lazy page allocation"
    );
    let _ = writeln!(
        handle,
        "    may not allocate all theoretical pages for smaller trees)."
    );
    let _ = writeln!(
        handle,
        "  Ratio > 1.0 → SignOutput bytes (condensed sigs + ladder) are included in the"
    );
    let _ = writeln!(
        handle,
        "    measured delta but not in the analytical tree-page estimate."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_selects_three_blueprint_sizes() {
        let result = selected_memory_sizes(&[SizeArg::All]);
        assert_eq!(result.len(), 3);
        assert_eq!(
            result,
            vec![
                DnsDatasetSize::Small,
                DnsDatasetSize::Medium,
                DnsDatasetSize::Large,
            ]
        );
    }

    #[test]
    fn explicit_sizes_dedupe() {
        let result = selected_memory_sizes(&[SizeArg::Small, SizeArg::Medium, SizeArg::Small]);
        assert_eq!(result.len(), 2);
        assert_eq!(result, vec![DnsDatasetSize::Small, DnsDatasetSize::Medium]);
    }

    #[test]
    fn all_excludes_onek() {
        let result = selected_memory_sizes(&[SizeArg::All]);
        for size in &result {
            assert_ne!(*size, DnsDatasetSize::OneK);
        }
    }
}
