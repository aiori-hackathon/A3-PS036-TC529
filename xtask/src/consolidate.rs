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

    /// Live ladder-verification artifact (from `cargo xtask ladder-verify`);
    /// preferred when present
    #[arg(long, default_value = "target/ladder-verification.json")]
    ladder_verification: PathBuf,

    /// Committed recorded ladder-verification fixture (fallback when no live
    /// run artifact exists)
    #[arg(long, default_value = "data/ladder-verification.recorded.json")]
    ladder_fixture: PathBuf,

    /// Path to environment JSON (from `cargo xtask pin-env`)
    #[arg(long, default_value = "environment.json")]
    environment: PathBuf,

    /// Classical signature lengths recorded by the classical benchmark run
    /// (written beside the Criterion output by the classical bench); its
    /// absence while classical records exist drops every classical context
    /// row with a warning rather than guessing lengths
    #[arg(long, default_value = "target/climb-classical-signatures.json")]
    classical_signatures: PathBuf,

    /// Output path for consolidated results
    #[arg(long, default_value = "target/climb-results.json")]
    output: PathBuf,

    /// Assumed network round-trip time in milliseconds, used only to model
    /// the TCP fallback delay for truncated responses; omitted, the model
    /// reports round trips and bytes but no latency
    #[arg(long)]
    rtt_ms: Option<f64>,
}

pub(crate) fn cmd_consolidate(args: ConsolidateArgs) -> Result<()> {
    let opts = ConsolidateOptions {
        force: args.force,
        criterion_dir: args.criterion_dir,
        wire_report_path: args.wire_report,
        memory_report_path: args.memory_report,
        dns_capture_path: args.dns_capture,
        dns_fixture_path: args.dns_fixture,
        ladder_verification_path: args.ladder_verification,
        ladder_fixture_path: args.ladder_fixture,
        environment_path: args.environment,
        baseline: args.baseline,
        rtt_ms: args.rtt_ms,
        classical_signatures_path: args.classical_signatures,
        output_path: args.output.clone(),
    };
    let _results = consolidate_results(opts).context("failed to consolidate results")?;
    println!("wrote consolidated results to {}", args.output.display());
    Ok(())
}
