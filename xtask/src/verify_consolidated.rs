//! The `verify-consolidated` subcommand: assert the consolidated results carry
//! a memory row for every size the memory report describes.
//!
//! Consolidation treats a missing memory report as a warning and an
//! unparseable one as a hard error.  This step closes the remaining gap: it
//! compares the row count the report implies against the row count the
//! consolidated results actually contain, so a report that is present and
//! valid but silently ignored fails the chain instead of yielding a report
//! with an empty memory section.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

use climb_report::{parse_memory_artifact, ClimbResults, MemoryArtifact};

#[derive(Args)]
pub(crate) struct VerifyConsolidatedArgs {
    /// Memory report JSON whose sizes the consolidated results must cover.
    #[arg(long, default_value = "target/memory-report.json")]
    memory_report: PathBuf,

    /// Consolidated results document to check.
    #[arg(long, default_value = "target/climb-results.json")]
    results: PathBuf,
}

/// The number of memory rows a memory artifact implies in the consolidated
/// results, matching how consolidation maps each artifact shape.
pub(crate) fn expected_row_count(artifact: &MemoryArtifact) -> usize {
    match artifact {
        MemoryArtifact::Isolated(report) => report.runs.len(),
        MemoryArtifact::LegacyRssDelta(report) => report
            .runs
            .iter()
            .filter(|run| !run.runs.is_empty())
            .count(),
        MemoryArtifact::LegacyComparison(rows) => rows.len(),
    }
}

/// Fail when the consolidated results do not carry exactly `expected` memory
/// rows.  Pure so the decision is unit-testable without touching the disk.
fn verify_row_count(expected: usize, actual: usize, results_path: &Path) -> Result<()> {
    if expected != actual {
        bail!(
            "{} carries {actual} memory row(s) but the memory report supplies {expected}",
            results_path.display()
        );
    }
    Ok(())
}

pub(crate) fn cmd_verify_consolidated(args: VerifyConsolidatedArgs) -> Result<()> {
    let memory_text = std::fs::read_to_string(&args.memory_report)
        .with_context(|| format!("failed to read {}", args.memory_report.display()))?;
    let artifact = parse_memory_artifact(&memory_text)
        .with_context(|| format!("failed to parse {}", args.memory_report.display()))?;
    let expected = expected_row_count(&artifact);

    let results_text = std::fs::read_to_string(&args.results)
        .with_context(|| format!("failed to read {}", args.results.display()))?;
    let results: ClimbResults = serde_json::from_str(&results_text)
        .with_context(|| format!("failed to parse {}", args.results.display()))?;

    verify_row_count(expected, results.memory.measured.len(), &args.results)?;
    println!(
        "verified {} memory row(s) in {}",
        results.memory.measured.len(),
        args.results.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISOLATED: &str = r#"{"method":"isolated-process","seed":7,"runs":[
      {"size":"small","message_count":100,"method":"isolated-process","samples":[],
       "deltas_kb":[],"median_kb":0.0,"q1_kb":0,"q3_kb":0,"iqr_kb":0,
       "min_kb":0,"max_kb":0,"median_bytes":0,"min_bytes":0,"max_bytes":0,"iqr_bytes":0},
      {"size":"medium","message_count":10000,"method":"isolated-process","samples":[],
       "deltas_kb":[],"median_kb":0.0,"q1_kb":0,"q3_kb":0,"iqr_kb":0,
       "min_kb":0,"max_kb":0,"median_bytes":0,"min_bytes":0,"max_bytes":0,"iqr_bytes":0}]}"#;

    const LEGACY_DELTA: &str = r#"{"seed":1,"runs":[
      {"size":"Small","runs":[
        {"rss_before_bytes":1,"rss_after_bytes":3,"delta_bytes":2,"delta_mib":0.0}],
       "repeat_count":1},
      {"size":"Medium","runs":[],"repeat_count":0}]}"#;

    const LEGACY_COMPARISON: &str = r#"[
      {"size":"Small","message_count":100,"measured_mean_bytes":1,"analytical_bytes":2,
       "ratio":0.5,"measured_range_bytes":[1,1],"measured_stddev_bytes":0.0}]"#;

    #[test]
    fn counts_every_isolated_size() {
        let artifact = parse_memory_artifact(ISOLATED).unwrap();
        assert_eq!(expected_row_count(&artifact), 2);
    }

    #[test]
    fn legacy_delta_counts_only_sizes_with_runs() {
        let artifact = parse_memory_artifact(LEGACY_DELTA).unwrap();
        assert_eq!(expected_row_count(&artifact), 1);
    }

    #[test]
    fn legacy_comparison_counts_every_row() {
        let artifact = parse_memory_artifact(LEGACY_COMPARISON).unwrap();
        assert_eq!(expected_row_count(&artifact), 1);
    }

    #[test]
    fn matching_row_count_passes() {
        let path = Path::new("target/climb-results.json");
        assert!(verify_row_count(3, 3, path).is_ok());
    }

    #[test]
    fn mismatched_row_count_names_the_path_and_numbers() {
        let path = Path::new("target/climb-results.json");
        let message = verify_row_count(3, 0, path).unwrap_err().to_string();
        assert!(message.contains("target/climb-results.json"), "{message}");
        assert!(message.contains('0') && message.contains('3'), "{message}");
    }
}
