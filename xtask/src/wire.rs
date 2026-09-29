//! The `wire-report` subcommand: permanent, reproducible wire-size numbers.
//!
//! Replaces the throwaway Phase 7.2 probe (which existed only as a markdown
//! table) with a command anyone can re-run:
//!
//! ```text
//! cargo xtask wire-report --sizes all
//! cargo xtask wire-report --sizes small --out results/wire.json
//! ```
//!
//! It calls [`climb_bench::measure_wire_sizes`] for each requested dataset
//! size, prints an aligned ASCII table to stdout, and optionally writes a
//! pretty-JSON array of [`WireSizeReport`] values.

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use climb_bench::{measure_wire_sizes, WireSizeReport, DEFAULT_WIRE_SEED};
use climb_dns::DatasetSize as DnsDatasetSize;

use crate::SizeArg;

/// Arguments for `cargo xtask wire-report`.
#[derive(Args)]
pub(crate) struct WireReportArgs {
    /// Dataset sizes to measure (comma-separated).
    ///
    /// The "all" default selects exactly the three blueprint sizes:
    /// Small (100), Medium (10k), Large (1M).  OneK is NOT included by
    /// default — 1,000 messages is a validation size, not a blueprint size.
    /// Use `--sizes onek` (or `--sizes small,medium,large,onek`) to include
    /// it explicitly.
    #[arg(long, value_delimiter = ',', default_value = "all")]
    sizes: Vec<SizeArg>,

    /// Seed for deterministic dataset generation.
    ///
    /// Default equals the signing bench seed constant CLIMB_V5
    /// (0x434C494D425F5635).
    #[arg(long, default_value_t = DEFAULT_WIRE_SEED)]
    seed: u64,

    /// If set, also write a pretty JSON array of WireSizeReport values.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// Measure wire sizes for every requested size and print (and optionally
/// persist) the result.
pub(crate) fn cmd_wire_report(args: WireReportArgs) -> Result<()> {
    let sizes = selected_wire_sizes(&args.sizes);
    let mut reports: Vec<WireSizeReport> = Vec::with_capacity(sizes.len());

    for &size in &sizes {
        let report = measure_wire_sizes(size, args.seed)
            .with_context(|| format!("wire-size measurement failed for {size:?}"))?;
        reports.push(report);
    }

    if let Some(path) = &args.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dirs for {}", path.display()))?;
        }
        let json = serde_json::to_string_pretty(&reports)
            .context("failed to serialize WireSizeReport to JSON")?;
        fs::write(path, json)
            .with_context(|| format!("failed to write JSON to {}", path.display()))?;
        eprintln!("wrote wire-size report to {}", path.display());
    }

    print_wire_report_table(&reports);
    Ok(())
}

/// Select the exact three blueprint sizes: Small, Medium, Large.
///
/// OneK is deliberately excluded — it is a validation size for verifying
/// linear extrapolation, not a blueprint experiment size.  It remains
/// selectable explicitly via `--sizes onek`.
fn wire_sizes_default() -> Vec<DnsDatasetSize> {
    vec![
        DnsDatasetSize::Small,
        DnsDatasetSize::Medium,
        DnsDatasetSize::Large,
    ]
}

/// Resolve the user's `--sizes` list to an ordered, deduplicated list of
/// sizes.  When `All` appears, inline the default blueprint sizes (Small,
/// Medium, Large) immediately.  Explicit sizes preserve the user's order.
fn selected_wire_sizes(args: &[SizeArg]) -> Vec<DnsDatasetSize> {
    let mut sizes = Vec::new();
    for arg in args {
        match arg {
            SizeArg::All => {
                for size in wire_sizes_default() {
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
        SizeArg::All => unreachable!("SizeArg::All is handled in selected_wire_sizes"),
    }
}

/// Format a byte count as a plain integer (no thousands separator) for
/// deterministic, machine-parseable output.
fn human_bytes(bytes: u64) -> String {
    bytes.to_string()
}

/// Print a human-readable wire-size table to stdout.
fn print_wire_report_table(reports: &[WireSizeReport]) {
    // Column widths chosen to accommodate all reasonable values,
    // avoiding dynamic sizing for determinism.
    let header = format!(
        "{:<6} {:>8} {:>7} {:>9} {:>10} {:>9} {:>8} {:>10} {:>13} {:>10}",
        "Size",
        "N",
        "Ladder",
        "Cond-Min",
        "Cond-Mean",
        "Cond-Max",
        "Uniform",
        "With MTL",
        "Without MTL",
        "Reduction%",
    );
    let separator = "-".repeat(header.len());

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{separator}");

    for report in reports {
        let size_label = match report.size {
            DnsDatasetSize::Small => "small",
            DnsDatasetSize::OneK => "onek",
            DnsDatasetSize::Medium => "medium",
            DnsDatasetSize::Large => "large",
        };

        let uniform_label = if report.condensed.uniform {
            "yes"
        } else {
            "no"
        };

        let _ = writeln!(
            handle,
            "{size_label:<6} {n:>8} {ladder:>7} {c_min:>9} {c_mean:>10.0} {c_max:>9} {uniform:>8} {with_mtl:>10} {without_mtl:>13} {reduction:>9.2}%",
            size_label = size_label,
            n = report.message_count,
            ladder = human_bytes(report.ladder_len as u64),
            c_min = human_bytes(report.condensed.min as u64),
            c_mean = report.condensed.mean,
            c_max = human_bytes(report.condensed.max as u64),
            uniform = uniform_label,
            with_mtl = human_bytes(report.with_mtl_total),
            without_mtl = human_bytes(report.without_mtl_total),
            reduction = report.reduction_percent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_selects_three_blueprint_sizes() {
        let result = selected_wire_sizes(&[SizeArg::All]);
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
    fn explicit_sizes_preserve_order_and_dedupe() {
        let result = selected_wire_sizes(&[SizeArg::Small, SizeArg::Large, SizeArg::Small]);
        assert_eq!(result, vec![DnsDatasetSize::Small, DnsDatasetSize::Large]);
    }

    #[test]
    fn includes_onek_when_explicit() {
        let result = selected_wire_sizes(&[SizeArg::OneK]);
        assert_eq!(result, vec![DnsDatasetSize::OneK]);
    }

    #[test]
    fn all_excludes_onek_by_default() {
        let result = selected_wire_sizes(&[SizeArg::All]);
        for size in &result {
            assert_ne!(*size, DnsDatasetSize::OneK);
        }
    }

    #[test]
    fn all_plus_onek_includes_onek() {
        let result = selected_wire_sizes(&[SizeArg::All, SizeArg::OneK]);
        assert_eq!(result.len(), 4);
        assert!(result.contains(&DnsDatasetSize::OneK));
    }

    #[test]
    fn human_bytes_zero() {
        assert_eq!(human_bytes(0), "0");
    }

    #[test]
    fn human_bytes_large() {
        assert_eq!(human_bytes(1_234_567), "1234567");
    }

    #[test]
    fn human_bytes_over_1024() {
        assert_eq!(human_bytes(2_048), "2048");
    }
}
