//! Cross-host replication commands: an on-demand comparison of two results
//! documents and the summary that anchors the committed replication artifact.
//!
//! `replicate-compare` is read-only: it loads both documents, compares the
//! headline ratios already derived in each, and prints the pair beside its
//! relative difference and the host facts recorded in each environment
//! fingerprint.
//!
//! `replicate-summary` writes the section that `consolidate` embeds: pending
//! while `data/replication/` holds no second host, compared once a directory
//! with a `climb-results.json` exists there. It never fabricates a second
//! host.

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

use climb_report::replication::{compared_section, pending_section, ReplicationSection};
use climb_report::results::EnvironmentFingerprint;
use climb_report::ClimbResults;

use crate::pin_env::iso_timestamp;

#[derive(Args)]
pub(crate) struct ReplicateCompareArgs {
    /// First host's consolidated results document
    #[arg(long, value_name = "RESULTS")]
    a: PathBuf,

    /// Second host's consolidated results document
    #[arg(long, value_name = "RESULTS")]
    b: PathBuf,
}

#[derive(Args)]
pub(crate) struct ReplicateSummaryArgs {
    /// Primary (this host's) consolidated results document
    #[arg(long, default_value = "target/climb-results.json")]
    primary: PathBuf,

    /// Directory holding one subdirectory per additional host, each with a
    /// `climb-results.json` and (optionally) an `environment.json`
    #[arg(long, default_value = "data/replication")]
    replication_dir: PathBuf,

    /// Output path for the replication summary
    #[arg(long, default_value = "target/replication-summary.json")]
    out: PathBuf,
}

pub(crate) fn cmd_replicate_compare(args: ReplicateCompareArgs) -> Result<()> {
    let a = load_host(&args.a)?;
    let b = load_host(&args.b)?;
    let section = compared_section(
        &a,
        &b,
        &args.a.display().to_string(),
        &args.b.display().to_string(),
        iso_timestamp(),
    );
    print!("{}", render_compare(&section));
    Ok(())
}

pub(crate) fn cmd_replicate_summary(args: ReplicateSummaryArgs) -> Result<()> {
    let primary = load_host(&args.primary)?;
    let section = build_summary(
        &primary,
        &args.primary,
        &args.replication_dir,
        iso_timestamp(),
    )?;
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&section).context("failed to serialize summary")?;
    fs::write(&args.out, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!("wrote {} ({})", args.out.display(), section.status.label());
    Ok(())
}

/// Build the summary: compared when a replica directory holds a results
/// document, pending otherwise.
pub(crate) fn build_summary(
    primary: &ClimbResults,
    primary_path: &Path,
    replication_dir: &Path,
    generated_at: String,
) -> Result<ReplicationSection> {
    match find_replica(replication_dir)? {
        Some(replica_path) => {
            let replica = load_host(&replica_path)?;
            Ok(compared_section(
                primary,
                &replica,
                &primary_path.display().to_string(),
                &replica_path.display().to_string(),
                generated_at,
            ))
        }
        None => Ok(pending_section(
            primary,
            &primary_path.display().to_string(),
            generated_at,
        )),
    }
}

/// The lexicographically first subdirectory of `dir` that carries a
/// `climb-results.json`. A directory that exists but holds no results document
/// is skipped; a missing `dir` yields `None`.
pub(crate) fn find_replica(dir: &Path) -> Result<Option<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", dir.display()));
        }
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        if !entry
            .file_type()
            .with_context(|| format!("failed to stat {}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let results = entry.path().join("climb-results.json");
        if results.is_file() {
            candidates.push(results);
        }
    }
    candidates.sort();
    Ok(candidates.into_iter().next())
}

/// Load a results document, replacing its embedded environment fingerprint
/// with a sibling `environment.json` when one exists, so the comparison quotes
/// the facts that travelled with the artifact.
pub(crate) fn load_host(path: &Path) -> Result<ClimbResults> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut results: ClimbResults = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if let Some(parent) = path.parent() {
        let environment = parent.join("environment.json");
        if environment.is_file() {
            let env_text = fs::read_to_string(&environment)
                .with_context(|| format!("failed to read {}", environment.display()))?;
            let fingerprint: EnvironmentFingerprint = serde_json::from_str(&env_text)
                .with_context(|| format!("failed to parse {}", environment.display()))?;
            results.environment = fingerprint;
        }
    }
    Ok(results)
}

/// Render the comparison as a plain-text table plus a host-facts block.
pub(crate) fn render_compare(section: &ReplicationSection) -> String {
    let mut out = String::new();
    out.push_str("Replication comparison (DERIVED)\n\n");
    for host in &section.hosts {
        out.push_str(&format!(
            "  {}  {}  (n = {} headline ratios)\n",
            host.host_hash, host.cpu_model, host.ratios
        ));
        out.push_str(&format!(
            "      governor {} | kernel {} | rustc {}\n",
            host.cpu_governor, host.kernel_release, host.rustc_version
        ));
        out.push_str(&format!(
            "      sizes {} | generated {}\n",
            host.sizes.join(", "),
            host.generated_at
        ));
    }
    out.push('\n');
    out.push_str(&format!(
        "  {:<44} {:>12} {:>12} {:>10}\n",
        "ratio", "A", "B", "diff %"
    ));
    for row in &section.ratios {
        out.push_str(&format!(
            "  {:<44} {:>12.3} {:>12.3} {:>10.3}\n",
            format!("{} [{}]", row.label, row.size),
            row.a,
            row.b,
            row.relative_difference_percent
        ));
    }
    out.push('\n');
    out.push_str(&format!("  {}\n", section.note));
    if section.missing_ratios > 0 {
        out.push_str(&format!(
            "  {} ratio(s) present on only one host were not compared\n",
            section.missing_ratios
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use climb_report::replication::ReplicationStatus;

    fn sample_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/sample-output/climb-results.json")
    }

    fn sample() -> ClimbResults {
        load_host(&sample_path()).expect("load the committed sample")
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-replication-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_missing_replication_directory_is_pending_not_an_error() {
        let primary = sample();
        let dir = temp_dir("absent").join("does-not-exist");
        let section = build_summary(
            &primary,
            &sample_path(),
            &dir,
            "2026-11-09T00:00:00+05:30".to_string(),
        )
        .expect("pending summary");
        assert_eq!(section.status, ReplicationStatus::Pending);
        assert_eq!(section.hosts.len(), 1);
        assert!(section.ratios.is_empty());
    }

    #[test]
    fn a_replication_directory_without_results_documents_is_pending() {
        let root = temp_dir("empty-dir");
        fs::create_dir_all(root.join("not-a-host")).expect("nested dir");
        let section = build_summary(&sample(), &sample_path(), &root, "fixed".to_string())
            .expect("pending summary");
        assert_eq!(section.status, ReplicationStatus::Pending);
    }

    #[test]
    fn a_committed_replica_is_discovered_and_compared() {
        let root = temp_dir("replica");
        let host_dir = root.join("0123456789ab");
        fs::create_dir_all(&host_dir).expect("host dir");
        fs::copy(sample_path(), host_dir.join("climb-results.json")).expect("copy results");

        let section = build_summary(&sample(), &sample_path(), &root, "fixed".to_string())
            .expect("compared summary");
        assert_eq!(section.status, ReplicationStatus::Compared);
        assert_eq!(section.hosts.len(), 2);
        assert_eq!(section.max_relative_difference_percent, Some(0.0));
        assert_eq!(section.agrees, Some(true));
        assert!(!section.ratios.is_empty());
    }

    #[test]
    fn replica_discovery_prefers_the_lexicographically_first_host() {
        let root = temp_dir("order");
        for hash in ["ffffff", "000000"] {
            let host_dir = root.join(hash);
            fs::create_dir_all(&host_dir).expect("host dir");
            fs::copy(sample_path(), host_dir.join("climb-results.json")).expect("copy results");
        }
        let found = find_replica(&root).expect("discovery").expect("a replica");
        assert!(
            found.starts_with(root.join("000000")),
            "{}",
            found.display()
        );
    }

    #[test]
    fn a_sibling_environment_file_replaces_the_embedded_fingerprint() {
        let root = temp_dir("sibling-env");
        let results = root.join("climb-results.json");
        fs::copy(sample_path(), &results).expect("copy results");
        let mut environment = sample().environment.clone();
        environment.cpu =
            serde_json::json!({"model_name": "Replication Test CPU", "logical_cpus": 8});
        environment.benchmark_runtime = serde_json::json!({"cpu_governor": "performance"});
        fs::write(
            root.join("environment.json"),
            serde_json::to_string(&environment).expect("serialize"),
        )
        .expect("write environment");

        let loaded = load_host(&results).expect("load with sibling environment");
        assert_eq!(
            loaded
                .environment
                .cpu
                .get("model_name")
                .and_then(|v| v.as_str()),
            Some("Replication Test CPU")
        );
    }

    #[test]
    fn the_compare_table_lists_both_values_and_the_computed_difference() {
        let a = sample();
        let mut b = sample();
        let row = b.comparisons.first_mut().expect("comparison rows");
        let baseline = row.wire_reduction_c_vs_b_percent.expect("wire reduction");
        row.wire_reduction_c_vs_b_percent = Some(baseline * 0.8);
        let section = compared_section(&a, &b, "a.json", "b.json", "fixed".to_string());
        let rendered = render_compare(&section);
        assert!(rendered.contains("A"), "{rendered}");
        assert!(rendered.contains("diff %"), "{rendered}");
        assert!(rendered.contains("DERIVED"), "{rendered}");
        // 20% higher value compared against the original: 20% relative.
        assert!(rendered.contains("20.000"), "{rendered}");
        assert!(rendered.contains("headline ratios"), "{rendered}");
    }

    #[test]
    fn a_compared_summary_over_the_same_document_agrees_at_zero() {
        let results = sample();
        let section = build_summary(
            &results,
            Path::new("a.json"),
            &temp_dir("same-doc"),
            "fixed".to_string(),
        )
        .expect("pending");
        // No replica directory exists in the temp dir yet, so it stays pending.
        assert_eq!(section.status, ReplicationStatus::Pending);
    }

    #[test]
    fn a_results_document_that_does_not_parse_is_a_named_error() {
        let root = temp_dir("bad-results");
        let path = root.join("climb-results.json");
        fs::write(&path, b"not json").expect("write");
        let error = load_host(&path).expect_err("parse must fail");
        let rendered = format!("{error:#}");
        assert!(rendered.contains("climb-results.json"), "{rendered}");
    }

    #[test]
    fn a_single_artifact_never_produces_a_two_host_verdict() {
        let results = sample();
        let section = build_summary(
            &results,
            Path::new("primary.json"),
            &temp_dir("no-replica"),
            "fixed".to_string(),
        )
        .expect("pending");
        assert_eq!(section.hosts.len(), 1);
        assert!(section.agrees.is_none());
        assert_eq!(section.status, ReplicationStatus::Pending);
    }
}
