//! `cargo xtask deck-facts`: render the quote sheet from the recorded
//! artifacts.
//!
//! Reads `target/climb-results.json` plus the raw artifacts behind it, asks
//! `climb-report` to collect every quotable number, writes the Markdown sheet,
//! and fails when two artifacts disagree about the same number. The workspace
//! test counts for the quality section come from a `cargo test --workspace`
//! run started here; the commit SHA comes from `git rev-parse HEAD`.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

use climb_report::deck_facts::{generate, parse_test_summary, DeckFactsInput, TestSummary};

#[derive(Args)]
pub(crate) struct DeckFactsArgs {
    /// Consolidated results document to read.
    #[arg(long, default_value = "target/climb-results.json")]
    pub(crate) results: PathBuf,

    /// Output Markdown path.
    #[arg(long, default_value = "docs/deck-facts.md")]
    pub(crate) out: PathBuf,
}

pub(crate) fn cmd_deck_facts(args: DeckFactsArgs) -> Result<()> {
    let test_summary = run_workspace_tests();
    let input = DeckFactsInput {
        results_path: args.results,
        wire_report_path: PathBuf::from("target/wire-report.json"),
        memory_report_path: PathBuf::from("target/memory-report.json"),
        ladder_verification_path: PathBuf::from("target/ladder-verification.json"),
        ladder_fixture_path: PathBuf::from("data/ladder-verification.recorded.json"),
        dns_capture_path: PathBuf::from("target/dns-capture.json"),
        dns_fixture_path: PathBuf::from("data/dns-capture.recorded.json"),
        environment_path: PathBuf::from("environment.json"),
        commit_sha: head_commit(),
        test_summary,
    };

    let output = generate(&input).context("failed to collect the deck quote sheet")?;

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&args.out, &output.markdown)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!(
        "wrote {} ({} rows, {} inconsistency(ies))",
        args.out.display(),
        output.rows.len(),
        output.inconsistencies.len()
    );

    if output.inconsistencies.is_empty() {
        return Ok(());
    }
    for inconsistency in &output.inconsistencies {
        eprintln!(
            "inconsistency: [{}] {}: {}",
            inconsistency.section.title(),
            inconsistency.label,
            inconsistency.detail
        );
    }
    bail!(
        "{} recorded value(s) disagree across artifacts — see the footer of {}",
        output.inconsistencies.len(),
        args.out.display()
    );
}

/// Run the workspace test suite and sum its `test result:` lines. A failing run
/// only warns: the sheet records the counts (and the failure count) either way.
fn run_workspace_tests() -> Option<TestSummary> {
    let output = Command::new("cargo")
        .args(["test", "--workspace"])
        .output()
        .ok()?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        eprintln!(
            "warning: `cargo test --workspace` exited with {}; the quality section records the failed count",
            output.status
        );
    }
    parse_test_summary(&combined)
}

/// The current commit SHA, or `None` when git is unavailable or the repository
/// has no commits yet.
fn head_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}
