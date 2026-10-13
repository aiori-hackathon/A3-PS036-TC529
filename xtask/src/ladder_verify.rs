//! The `ladder-verify` subcommand: measured MTL ladder shape for small N.
//!
//! ```text
//! cargo xtask ladder-verify
//! cargo xtask ladder-verify --sizes 1,2,3,5,13 --out target/ladder-verification.json
//! cargo xtask ladder-verify --record data/ladder-verification.recorded.json
//! ```
//!
//! It signs a deterministic dataset at each requested leaf count, measures the
//! real ladder shape via [`climb_bench::measure_ladder_rows`], writes the
//! [`LadderVerification`] JSON document, and prints an aligned table. The
//! closed-form model in `climb-analytical` is checked against this document by
//! a test, so the artifact and the model can never silently disagree.

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use climb_analytical::HASH_SIZE;
use climb_bench::{measure_ladder_rows, DEFAULT_LADDER_SEED, DEFAULT_LADDER_SIZES};
use climb_mtl::DEFAULT_ALGORITHM;
use climb_report::{
    LadderProvenance, LadderVerification, SubmoduleRef, LADDER_VERIFICATION_SCHEMA_VERSION,
};

use crate::pin_env;

/// Arguments for `cargo xtask ladder-verify`.
#[derive(Args)]
pub(crate) struct LadderVerifyArgs {
    /// Leaf counts to measure (comma-separated). Defaults to a small-N sweep
    /// covering odds, exact powers of two, and powers of two ± 1.
    #[arg(long, value_delimiter = ',')]
    sizes: Option<Vec<usize>>,

    /// Seed for deterministic dataset generation.
    #[arg(long, default_value_t = DEFAULT_LADDER_SEED)]
    seed: u64,

    /// Where to write the ladder-verification JSON document.
    #[arg(long, default_value = "target/ladder-verification.json")]
    out: PathBuf,

    /// Also copy the document to this path (used to refresh the committed
    /// fixture). The measurement is never fabricated; only a real run writes it.
    #[arg(long)]
    record: Option<PathBuf>,
}

/// Measure the ladder shape for every requested size and record the result.
pub(crate) fn cmd_ladder_verify(args: LadderVerifyArgs) -> Result<()> {
    let sizes = resolve_sizes(args.sizes.as_deref());

    let rows = measure_ladder_rows(args.seed, &sizes).context("ladder measurement failed")?;

    let root = pin_env::workspace_root()?;
    let document = LadderVerification {
        schema_version: LADDER_VERIFICATION_SCHEMA_VERSION,
        recorded_at: pin_env::iso_timestamp(),
        algorithm: DEFAULT_ALGORITHM.to_string(),
        hash_size: HASH_SIZE,
        seed: args.seed,
        provenance: collect_provenance(&root),
        sizes: rows,
    };

    let json = serde_json::to_string_pretty(&document)
        .context("failed to serialize ladder-verification document")?;
    write_document(&args.out, &json)?;
    eprintln!("wrote ladder-verification to {}", args.out.display());

    if let Some(record) = &args.record {
        write_document(record, &json)?;
        eprintln!(
            "recorded ladder-verification fixture to {}",
            record.display()
        );
    }

    print_table(&document);
    Ok(())
}

/// Resolve the requested sizes, defaulting to [`DEFAULT_LADDER_SIZES`], and
/// drop duplicates while preserving order.
fn resolve_sizes(requested: Option<&[usize]>) -> Vec<usize> {
    let source = requested.unwrap_or(DEFAULT_LADDER_SIZES);
    let mut sizes = Vec::with_capacity(source.len());
    for &n in source {
        if !sizes.contains(&n) {
            sizes.push(n);
        }
    }
    sizes
}

/// Resolve submodule pins from `git submodule status`, mapping to the artifact's
/// provenance type. An unreadable git state yields an empty provenance list
/// rather than failing the run.
fn collect_provenance(root: &Path) -> LadderProvenance {
    let submodules = pin_env::capture_submodules(root)
        .into_iter()
        .map(|s| SubmoduleRef {
            path: s.path,
            sha: s.sha,
            describe: s.describe,
        })
        .collect();
    LadderProvenance { submodules }
}

fn write_document(path: &Path, json: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

/// Print an aligned table of the measured per-size rows.
fn print_table(document: &LadderVerification) {
    let header = format!(
        "{:>6} {:>6} {:>11} {:>13} {:>9} {:>9} {:>10} {:>9} {:>11} {:>11}",
        "N",
        "Rungs",
        "LadderBuf",
        "SignedLadder",
        "SigLen",
        "CondMin",
        "CondMean",
        "CondMax",
        "FullMin",
        "FullMax",
    );
    let separator = "-".repeat(header.len());

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{separator}");
    for row in &document.sizes {
        let _ = writeln!(
            handle,
            "{n:>6} {rungs:>6} {buf:>11} {signed:>13} {sig:>9} {cmin:>9} {cmean:>10.1} {cmax:>9} {fmin:>11} {fmax:>11}",
            n = row.leaf_count,
            rungs = row.rung_count,
            buf = row.ladder_buffer_len,
            signed = row.signed_ladder_len,
            sig = row.signature_len,
            cmin = row.condensed.min,
            cmean = row.condensed.mean,
            cmax = row.condensed.max,
            fmin = row.full_len_min,
            fmax = row.full_len_max,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sizes_are_used_when_none_requested() {
        let sizes = resolve_sizes(None);
        assert_eq!(sizes, DEFAULT_LADDER_SIZES.to_vec());
    }

    #[test]
    fn explicit_sizes_dedupe_and_preserve_order() {
        let sizes = resolve_sizes(Some(&[5, 1, 5, 3, 1]));
        assert_eq!(sizes, vec![5, 1, 3]);
    }
}
