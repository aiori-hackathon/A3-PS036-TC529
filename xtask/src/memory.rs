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
use std::time::Instant;

use climb_bench::{median_iqr, DEFAULT_WIRE_SEED};
use climb_dns::DatasetSize as DnsDatasetSize;
use climb_report::memory::{
    MemoryReportJson, ProbeLine, SizeReport, StageDeltas, StageStats, ISOLATED_PROCESS_METHOD,
};

use crate::SizeArg;

/// One Large probe sample must finish within this many seconds for the run to
/// use the larger sample count.
const LARGE_SAMPLE_BUDGET_SECS: f64 = 90.0;
/// Minimum Large sample count when a single sample fits the budget.
const LARGE_MIN_SAMPLES: usize = 5;
/// Large sample count used when a single sample exceeds the budget.
const LARGE_SLOW_SAMPLES: usize = 3;

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
        let mut note = None;
        let mut samples_n = args.samples;

        if size == DnsDatasetSize::Large {
            // Time one sample so the run count tracks this machine rather than a
            // hardcoded cap.
            let started = Instant::now();
            let line = run_probe(&size_str, args.seed)?;
            let elapsed = started.elapsed().as_secs_f64();
            if elapsed < LARGE_SAMPLE_BUDGET_SECS {
                samples_n = args.samples.max(LARGE_MIN_SAMPLES);
                note = Some(format!(
                    "large measured with n={samples_n} (one sample took {elapsed:.1} s, \
                     within the {LARGE_SAMPLE_BUDGET_SECS:.0} s budget)"
                ));
            } else {
                samples_n = LARGE_SLOW_SAMPLES;
                note = Some(format!(
                    "large measured with n={LARGE_SLOW_SAMPLES} (one sample took \
                     {elapsed:.1} s, over the {LARGE_SAMPLE_BUDGET_SECS:.0} s budget)"
                ));
            }
            deltas.push(line.after_kb.saturating_sub(line.baseline_kb));
            samples.push(line);
        }

        while samples.len() < samples_n {
            let i = samples.len() as u64;
            let line = run_probe(&size_str, args.seed + i)?;
            deltas.push(line.after_kb.saturating_sub(line.baseline_kb));
            samples.push(line);
        }

        let (median_f, q1, q3, iqr) = median_iqr(&deltas);
        let minv = *deltas.iter().min().unwrap_or(&0);
        let maxv = *deltas.iter().max().unwrap_or(&0);
        let stages = stage_deltas(&samples);
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
            stages,
            note,
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

/// Per-stage deltas from process start to each sample point, or `None` when any
/// sample predates stage attribution and lacks an intermediate reading.
fn stage_deltas(samples: &[ProbeLine]) -> Option<StageDeltas> {
    let mut dataset = Vec::with_capacity(samples.len());
    let mut keypair = Vec::with_capacity(samples.len());
    let mut sign = Vec::with_capacity(samples.len());
    let mut resident = Vec::with_capacity(samples.len());
    let mut peak = Vec::with_capacity(samples.len());
    for s in samples {
        let after_dataset = s.after_dataset_kb?;
        let after_keypair = s.after_keypair_kb?;
        dataset.push(after_dataset.saturating_sub(s.baseline_kb));
        keypair.push(after_keypair.saturating_sub(after_dataset));
        sign.push(s.after_kb.saturating_sub(after_keypair));
        resident.push(s.after_kb.saturating_sub(s.baseline_kb));
        peak.push(s.hwm_kb.saturating_sub(s.baseline_kb));
    }
    Some(StageDeltas {
        dataset: stage_stats(&dataset),
        keypair: stage_stats(&keypair),
        sign: stage_stats(&sign),
        resident: stage_stats(&resident),
        peak: stage_stats(&peak),
    })
}

/// Median and interquartile range of one stage's per-sample deltas.
fn stage_stats(deltas: &[u64]) -> StageStats {
    let (median_kb, _q1, _q3, iqr_kb) = median_iqr(deltas);
    StageStats { median_kb, iqr_kb }
}

fn run_probe(size_str: &str, seed: u64) -> Result<ProbeLine> {
    let probe_path = probe_binary_path();
    let mut cmd = Command::new(probe_path);
    cmd.arg(size_str).arg(seed.to_string());
    let out = cmd.output().context("failed to spawn mem_probe")?;
    if !out.status.success() {
        anyhow::bail!("mem_probe failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let stdout = String::from_utf8(out.stdout).context("mem_probe produced non-utf8")?;
    serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse probe output for {size_str}: {stdout}"))
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
    let _ = writeln!(
        handle,
        "RSS delta (isolated-process probe) — median (IQR), KB"
    );
    let _ = writeln!(handle);
    let header = format!(
        "{:<6} {:>6} {:>18} {:>18}",
        "Size", "N", "Resident (IQR)", "Peak (IQR)",
    );
    let sep = "-".repeat(header.len());
    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{sep}");
    for r in runs {
        let (resident, peak) = match &r.stages {
            Some(s) => (
                format!("{:.1} ({})", s.resident.median_kb, s.resident.iqr_kb),
                format!("{:.1} ({})", s.peak.median_kb, s.peak.iqr_kb),
            ),
            None => (
                format!("{:.1} ({})", r.median_kb, r.iqr_kb),
                "-".to_string(),
            ),
        };
        let _ = writeln!(
            handle,
            "{:<6} {:>6} {:>18} {:>18}",
            r.size,
            r.samples.len(),
            resident,
            peak,
        );
    }

    if runs.iter().any(|r| r.stages.is_some()) {
        let _ = writeln!(handle);
        let _ = writeln!(handle, "Per-stage delta — median (IQR), KB");
        let _ = writeln!(handle);
        let header = format!(
            "{:<6} {:>18} {:>18} {:>18}",
            "Size", "Dataset (IQR)", "Keypair (IQR)", "Sign (IQR)",
        );
        let sep = "-".repeat(header.len());
        let _ = writeln!(handle, "{header}");
        let _ = writeln!(handle, "{sep}");
        for r in runs {
            if let Some(s) = &r.stages {
                let _ = writeln!(
                    handle,
                    "{:<6} {:>18} {:>18} {:>18}",
                    r.size,
                    format!("{:.1} ({})", s.dataset.median_kb, s.dataset.iqr_kb),
                    format!("{:.1} ({})", s.keypair.median_kb, s.keypair.iqr_kb),
                    format!("{:.1} ({})", s.sign.median_kb, s.sign.iqr_kb),
                );
            }
        }
    }
    let _ = writeln!(handle);
}
