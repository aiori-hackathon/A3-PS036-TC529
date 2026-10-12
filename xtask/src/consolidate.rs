use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use climb_report::{consolidate_results, ConsolidateOptions};

#[derive(Args)]
pub(crate) struct ConsolidateArgs {
    /// Force re-running even if existing files appear fresh
    #[arg(long)]
    force: bool,

    /// Baseline to read from Criterion output
    #[arg(long, default_value = "new")]
    baseline: String,

    /// Criterion output directory
    #[arg(long, default_value = "target/criterion")]
    criterion_dir: PathBuf,

    /// Path to wire report JSON (from `cargo xtask wire-report --out`)
    #[arg(long)]
    wire_report: Option<PathBuf>,

    /// Path to memory report JSON (from `cargo xtask memory-report --out`)
    #[arg(long)]
    memory_report: Option<PathBuf>,

    /// Live DNS capture (from `cargo xtask dns-demo --record`); preferred
    #[arg(long, default_value = "target/dns-capture.json")]
    dns_capture: PathBuf,

    /// Committed recorded DNS capture fixture (fallback when no live capture)
    #[arg(long, default_value = "data/dns-capture.recorded.json")]
    dns_fixture: PathBuf,

    /// Path to environment JSON (from `cargo xtask pin-env`)
    #[arg(long, default_value = "environment.json")]
    environment: PathBuf,

    /// Output path for consolidated results
    #[arg(long, default_value = "target/climb-results.json")]
    output: PathBuf,
}

pub(crate) fn cmd_consolidate(args: ConsolidateArgs) -> Result<()> {
    let opts = ConsolidateOptions {
        force: args.force,
        criterion_dir: args.criterion_dir,
        wire_report_path: args.wire_report,
        memory_report_path: args.memory_report,
        dns_capture_path: args.dns_capture,
        dns_fixture_path: args.dns_fixture,
        environment_path: args.environment,
        baseline: args.baseline,
        output_path: args.output.clone(),
    };
    let _results = consolidate_results(opts).context("failed to consolidate results")?;
    println!("wrote consolidated results to {}", args.output.display());
    Ok(())
}
