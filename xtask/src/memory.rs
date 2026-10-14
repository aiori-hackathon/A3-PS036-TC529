//! The `memory-report` subcommand: measured RSS delta vs analytical estimate.
//!
//! ```text
//! cargo xtask memory-report --sizes all
//! cargo xtask memory-report --samples 7 --out results/memory.json
//! ```

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use climb_bench::{median_iqr, DEFAULT_WIRE_SEED};
use climb_dns::DatasetSize as DnsDatasetSize;
use climb_report::memory::{MemoryReportJson, ProbeLine, SizeReport, ISOLATED_PROCESS_METHOD};

use crate::SizeArg;

#[derive(Args)]
pub(crate) struct MemoryReportArgs {
    #[arg(long, value_delimiter = ',', default_value = "all")]
    sizes: Vec<SizeArg>,

    #[arg(long, default_value_t = DEFAULT_WIRE_SEED)]
    seed: u64,

    #[arg(long, default_value_t = 7)]
    samples: usize,

    #[arg(long)]
    out: Option<PathBuf>,
}

pub(crate) fn cmd_memory_report(args: MemoryReportArgs) -> Result<()> {
    let sizes = selected_memory_sizes(&args.sizes);
    let mut runs: Vec<SizeReport> = Vec::with_capacity(sizes.len());

    for &size in &sizes {
        let size_str = dns_size_to_str(size);
        let message_count = size.count();
        let mut samples = Vec::new();
        let mut deltas = Vec::new();
        let samples_n = if size == DnsDatasetSize::Large && args.samples > 3 {
            3
        } else {
            args.samples
        };
        for i in 0..samples_n {
            let out = run_probe(&size_str, args.seed + (i as u64))?;
            let line: ProbeLine = serde_json::from_str(&out)
                .with_context(|| format!("failed to parse probe output for {size_str}: {out}"))?;
            let delta = line.after_kb.saturating_sub(line.baseline_kb);
            samples.push(line);
            deltas.push(delta);
        }
        let (median_f, q1, q3, iqr) = median_iqr(&deltas);
        let minv = *deltas.iter().min().unwrap_or(&0);
        let maxv = *deltas.iter().max().unwrap_or(&0);
        runs.push(SizeReport {
            size: size_str.clone(),
            message_count,
            method: ISOLATED_PROCESS_METHOD.to_string(),
            samples,
            deltas_kb: deltas.clone(),
            median_kb: median_f,
            q1_kb: q1,
            q3_kb: q3,
            iqr_kb: iqr,
            min_kb: minv,
            max_kb: maxv,
            median_bytes: (median_f as u64) * 1024,
            min_bytes: minv * 1024,
            max_bytes: maxv * 1024,
            iqr_bytes: iqr * 1024,
        });
    }

    if let Some(path) = &args.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dirs for {}", path.display()))?;
        }
        let full = MemoryReportJson {
            method: ISOLATED_PROCESS_METHOD.to_string(),
            seed: args.seed,
            runs: runs.clone(),
        };
        let json = serde_json::to_string_pretty(&full)
            .context("failed to serialize memory report to JSON")?;
        fs::write(path, json)
            .with_context(|| format!("failed to write JSON to {}", path.display()))?;
        eprintln!("wrote memory-report to {}", path.display());
    }

    print_comparison_table_iso(&runs);
    Ok(())
}

fn run_probe(size_str: &str, seed: u64) -> Result<String> {
    let probe_path = probe_binary_path();
    let mut cmd = Command::new(probe_path);
    cmd.arg(size_str).arg(seed.to_string());
    let out = cmd.output().context("failed to spawn mem_probe")?;
    if !out.status.success() {
        anyhow::bail!("mem_probe failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    String::from_utf8(out.stdout).context("mem_probe produced non-utf8")
}

fn probe_binary_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("target");
    p.push("debug");
    p.push("mem_probe");
    if !p.exists() {
        p.pop();
        p.push("release");
        p.push("mem_probe");
    }
    p
}

fn dns_size_to_str(size: DnsDatasetSize) -> String {
    match size {
        DnsDatasetSize::Small => "small".to_string(),
        DnsDatasetSize::OneK => "onek".to_string(),
        DnsDatasetSize::Medium => "medium".to_string(),
        DnsDatasetSize::Large => "large".to_string(),
    }
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

fn print_comparison_table_iso(runs: &[SizeReport]) {
    use std::io::{self, Write};
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle);
    let _ = writeln!(handle, "RSS delta (isolated-process probe) — median (IQR)");
    let _ = writeln!(handle);
    let header = format!(
        "{:<6} {:>10} {:>16} {:>16} {:>12} {:>14}",
        "Size", "N", "Median (KB)", "IQR (KB)", "Min (KB)", "Max (KB)",
    );
    let sep = "-".repeat(header.len());
    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{sep}");
    for r in runs {
        let _ = writeln!(
            handle,
            "{:<6} {:>10} {:>16.1} {:>16} {:>12} {:>14}",
            r.size, r.message_count, r.median_kb, r.iqr_kb, r.min_kb, r.max_kb,
        );
    }
    let _ = writeln!(handle);
}
