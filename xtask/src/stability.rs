//! `cargo xtask stability`: repeat the signing-primitives benchmark several
//! times under a pinned core and record the run-to-run spread of the derived
//! signing speed-up model.
//!
//! Each run measures the per-message MTL signing primitive `t_mtl_single` and
//! the per-batch amortised signing cost `t_tree(N)` for every selected dataset
//! size.
//!
//! # The signing speed-up model
//!
//! Signing a batch of `N` messages costs:
//!
//! * **C — with amortisation:** one MTL ladder over all `N` messages, i.e.
//!   `t_tree(N)`. The underlying signature is paid once.
//! * **B — without amortisation:** `N` independent MTL signatures, i.e.
//!   `N × t_mtl_single`. The underlying signature is paid `N` times.
//!
//! so the speed-up is
//!
//! ```text
//! B/C = (N × t_mtl_single) / t_tree(N)
//! ```
//!
//! Because `t_tree(N)` is dominated by that one underlying signature and is
//! nearly independent of `N`, the model ratio is close to `N`: the amortised
//! path signs `N` messages for roughly the price of one.
//!
//! The model deliberately does **not** add `t_mtl_single` to `t_tree(N)`.
//! `t_tree(N)` already contains the underlying signature — it *is* the
//! signature over the ladder — so adding it again would count the same
//! signature twice and halve the modelled ratio.
//!
//! Across runs the median, min, max, coefficient of variation and run count
//! are computed per size and merged additively into `derived.stability` of the
//! consolidated results artifact, so a run-to-run instability in a headline
//! ratio is visible without re-deriving it by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::Value;

use climb_report::{ClimbResults, StabilityRatioStats, StabilitySection};

#[derive(Args)]
pub(crate) struct StabilityArgs {
    /// Number of repeated bench runs to aggregate over.
    #[arg(long, default_value = "5")]
    runs: usize,
    /// Comma list of dataset sizes (small, onek, medium) to measure.
    #[arg(long, default_value = "small,onek,medium")]
    sizes: String,
    /// Consolidated results artifact to merge `derived.stability` into.
    #[arg(long, default_value = "target/climb-results.json")]
    results: PathBuf,
    /// Criterion output directory the runs write into.
    #[arg(long, default_value = "target/criterion")]
    criterion_dir: PathBuf,
    /// CPU core to pin each run to (the recorded governor's pinned core).
    #[arg(long, default_value = "2")]
    pin_cpu: u32,
}

/// Map a dataset-size label to its message count.
fn size_to_n(label: &str) -> Option<usize> {
    match label.trim().to_ascii_lowercase().as_str() {
        "small" => Some(100),
        "onek" => Some(1_000),
        "medium" => Some(10_000),
        _ => None,
    }
}

/// Parse a `--sizes` list into ordered `(label, N)` pairs, rejecting unknowns.
fn parse_sizes(s: &str) -> Result<Vec<(String, usize)>> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let label = part.to_ascii_lowercase();
        match size_to_n(&label) {
            Some(n) => {
                if !out.iter().any(|(l, _)| *l == label) {
                    out.push((label, n));
                }
            }
            None => bail!("unknown dataset size '{part}' (expected small, onek or medium)"),
        }
    }
    if out.is_empty() {
        bail!("no dataset sizes given");
    }
    Ok(out)
}

/// Build the pinned `cargo bench --bench primitives` command for one run.
fn build_cmd(args: &StabilityArgs, sizes: &[(String, usize)]) -> Command {
    let n_list = sizes
        .iter()
        .map(|(_, n)| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut cmd = Command::new("taskset");
    cmd.args(["-c", &args.pin_cpu.to_string(), "cargo"]);
    cmd.args(["bench", "-p", "climb-bench", "--bench", "primitives"]);
    // A modest per-run sample count keeps each repeat fast; the cross-run
    // spread is what stability records, not the within-run error bars.
    cmd.env("CLIMB_PRIMITIVE_SAMPLES", "10");
    cmd.env("CLIMB_PRIMITIVES_SIZES", &n_list);
    cmd.env("CLIMB_PIN_CPU", args.pin_cpu.to_string());
    cmd
}

/// Read the mean point estimate (nanoseconds) for one Criterion group by
/// walking its output directory for the `new` baseline's `estimates.json`.
fn read_criterion_mean(criterion_dir: &Path, group: &str) -> Option<f64> {
    let mut stack = vec![criterion_dir.join(group)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.file_name().and_then(|s| s.to_str()) == Some("new") {
                let Ok(txt) = std::fs::read_to_string(path.join("estimates.json")) else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(&txt) else {
                    continue;
                };
                if let Some(m) = v
                    .get("mean")
                    .and_then(|m| m.get("point_estimate"))
                    .and_then(|x| x.as_f64())
                {
                    return Some(m);
                }
            } else {
                stack.push(path);
            }
        }
    }
    None
}

/// `(median, min, max, cv)` over a non-empty sample. `cv` is the standard
/// deviation over the mean (population form); a zero mean yields a zero `cv`.
fn compute_stats(vals: &[f64]) -> (f64, f64, f64, f64) {
    let mut sorted = vals.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let min = sorted[0];
    let max = sorted[n - 1];
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    let mean = vals.iter().sum::<f64>() / n as f64;
    let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let cv = if mean != 0.0 { var.sqrt() / mean } else { 0.0 };
    (median, min, max, cv)
}

/// The modelled signing speed-up `B/C` for a batch of `n` messages.
///
/// `C` is the amortised cost of signing the whole batch under one ladder
/// (`t_tree`); `B` is the non-amortised cost (`n` independent MTL
/// signatures, each `t_mtl_single`). `t_tree` already contains the single
/// underlying signature, so it is used as-is.
///
/// Returns `None` when `t_tree` is not positive — a degenerate measurement
/// must not become a ratio.
fn model_speedup(n: usize, t_mtl_single: f64, t_tree: f64) -> Option<f64> {
    if t_tree <= 0.0 {
        None
    } else {
        Some((n as f64) * t_mtl_single / t_tree)
    }
}

pub(crate) fn cmd_stability(args: StabilityArgs) -> Result<()> {
    let sizes = parse_sizes(&args.sizes)?;
    if args.runs == 0 {
        bail!("--runs must be at least 1");
    }

    // ratio name -> one speed-up value per run
    let mut per_ratio: BTreeMap<String, Vec<f64>> = BTreeMap::new();

    for run in 1..=args.runs {
        eprintln!("stability run {run}/{} (sizes: {})", args.runs, args.sizes);
        let status = build_cmd(&args, &sizes)
            .status()
            .with_context(|| "failed to launch the pinned primitives benchmark")?;
        if !status.success() {
            bail!("primitives benchmark failed on run {run} ({status})");
        }

        let t_mtl_single = read_criterion_mean(&args.criterion_dir, "primitives_t_mtl_single")
            .with_context(|| "missing primitives_t_mtl_single estimate after the run")?;

        for (label, n) in &sizes {
            let group = format!("primitives_t_tree_{n}");
            let t_tree = read_criterion_mean(&args.criterion_dir, &group)
                .with_context(|| format!("missing {group} estimate after the run"))?;
            // The amortised cost of signing the whole batch is the ladder
            // signing itself; the non-amortised cost is one independent
            // signature per message. `t_tree(N)` already contains the
            // underlying signature, so nothing is added to it.
            let Some(speedup) = model_speedup(*n, t_mtl_single, t_tree) else {
                bail!("{group} reported a non-positive ladder signing cost ({t_tree} ns)");
            };
            per_ratio
                .entry(format!("signing_speedup_model_{label}"))
                .or_default()
                .push(speedup);
        }
    }

    let runs: Vec<StabilityRatioStats> = per_ratio
        .into_iter()
        .map(|(ratio_name, vals)| {
            let (median, min, max, cv) = compute_stats(&vals);
            StabilityRatioStats {
                ratio_name,
                median,
                min,
                max,
                cv,
                n_runs: vals.len(),
            }
        })
        .collect();

    let n_ratios = runs.len();
    for row in &runs {
        eprintln!(
            "{}: median {:.4} (min {:.4}, max {:.4}, cv {:.4}, n={})",
            row.ratio_name, row.median, row.min, row.max, row.cv, row.n_runs
        );
    }
    merge_stability(&args.results, StabilitySection { runs }).with_context(|| {
        format!(
            "failed to merge derived.stability into {}",
            args.results.display()
        )
    })?;

    eprintln!(
        "wrote derived.stability ({} ratio(s)) into {}",
        n_ratios,
        args.results.display()
    );
    Ok(())
}

/// Load the results artifact, set `derived.stability`, and write it back
/// without disturbing any other field (the section is purely additive).
///
/// The document goes through the same typed model the consolidator writes, so
/// the merged artifact keeps the consolidator's field order and layout: a
/// merge never reshapes the file around it. An artifact the model cannot
/// parse is a hard error naming the path, never a silently rewritten file.
fn merge_stability(results_path: &Path, section: StabilitySection) -> Result<()> {
    let text = std::fs::read_to_string(results_path)
        .with_context(|| format!("failed to read {}", results_path.display()))?;
    let mut results: ClimbResults = serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not a consolidated results artifact",
            results_path.display()
        )
    })?;
    results.derived.stability = Some(section);
    let out = serde_json::to_string_pretty(&results)?;
    std::fs::write(results_path, out)
        .with_context(|| format!("failed to write {}", results_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed sample artifact, used as a realistic input for the
    /// merge tests.
    fn committed_sample() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../docs/sample-output/climb-results.json")
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-stability-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn section() -> StabilitySection {
        StabilitySection {
            runs: vec![StabilityRatioStats {
                ratio_name: "signing_speedup_model_small".to_string(),
                median: 100.0,
                min: 99.0,
                max: 101.0,
                cv: 0.01,
                n_runs: 5,
            }],
        }
    }

    /// The merge is additive and nothing else: every field of the artifact
    /// outside `derived.stability` survives the merge unchanged, and the
    /// stability section lands where it belongs.
    #[test]
    fn the_merge_adds_the_stability_section_and_changes_nothing_else() {
        let dir = temp_dir("additive");
        let path = dir.join("climb-results.json");
        let source = std::fs::read_to_string(committed_sample()).expect("read committed sample");
        std::fs::write(&path, &source).expect("write copy");

        merge_stability(&path, section()).expect("merge");

        let mut before: serde_json::Value = serde_json::from_str(&source).expect("valid JSON");
        let mut after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read merged"))
                .expect("valid JSON");

        // The section the merge wrote is the one it was given.
        let written = after["derived"]["stability"].clone();
        assert_eq!(
            written["runs"][0]["ratio_name"],
            "signing_speedup_model_small"
        );
        assert_eq!(written["runs"][0]["n_runs"], 5);

        // Remove the section from both documents: the rest of the artifact
        // must be identical, field for field.
        for doc in [&mut before, &mut after] {
            doc["derived"]
                .as_object_mut()
                .expect("derived object")
                .remove("stability");
        }
        assert_eq!(before, after, "the merge must not disturb any other field");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The merge writes the artifact in the consolidator's field order: the
    /// merged document lists its top-level keys exactly as the results model
    /// declares them, so the two writers agree on the layout of the file and a
    /// merge never reshapes the artifact around it.
    #[test]
    fn the_merge_keeps_the_consolidators_field_order() {
        let dir = temp_dir("order");
        let path = dir.join("climb-results.json");
        let source = std::fs::read_to_string(committed_sample()).expect("read committed sample");
        std::fs::write(&path, &source).expect("write copy");

        merge_stability(&path, section()).expect("merge");
        let merged = std::fs::read_to_string(&path).expect("read merged");

        let top_level_keys = |text: &str| -> Vec<String> {
            text.lines()
                .filter_map(|line| {
                    let rest = line.strip_prefix("  \"")?;
                    let end = rest.find('"')?;
                    Some(rest[..end].to_string())
                })
                .collect()
        };
        assert_eq!(
            top_level_keys(&merged),
            vec![
                "version",
                "generated_at",
                "workspace_root",
                "benchmark",
                "wire_size",
                "memory",
                "environment",
                "dns_pipeline",
                "dns_anatomy",
                "ladder",
                "plain",
                "classical",
                "comparisons",
                "derived",
                "warnings",
            ],
            "the merge must write the artifact in the model's field order"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is not a consolidated results artifact is a hard error that
    /// names the path — the merge never rewrites a document it cannot read.
    #[test]
    fn the_merge_rejects_a_file_that_is_not_a_results_artifact() {
        let dir = temp_dir("invalid");
        let path = dir.join("climb-results.json");
        std::fs::write(&path, "{ \"not\": \"an artifact\" }").expect("write invalid file");

        let err = merge_stability(&path, section())
            .expect_err("an unreadable artifact must be a hard error")
            .to_string();
        assert!(
            err.contains("climb-results.json"),
            "the error must name the path: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_the_default_size_list() {
        let sizes = parse_sizes("small,onek,medium").expect("parses");
        assert_eq!(
            sizes,
            vec![
                ("small".to_string(), 100),
                ("onek".to_string(), 1_000),
                ("medium".to_string(), 10_000),
            ]
        );
    }

    #[test]
    fn rejects_an_unknown_size() {
        assert!(parse_sizes("small,huge").is_err());
    }

    #[test]
    fn rejects_an_empty_size_list() {
        assert!(parse_sizes(" , ").is_err());
    }

    #[test]
    fn stats_on_an_odd_sample_use_the_middle_median() {
        let (median, min, max, cv) = compute_stats(&[3.0, 1.0, 2.0]);
        assert_eq!(median, 2.0);
        assert_eq!(min, 1.0);
        assert_eq!(max, 3.0);
        assert!((cv - (2.0_f64 / 3.0).sqrt() / 2.0).abs() < 1e-12);
    }

    #[test]
    fn stats_on_an_even_sample_average_the_two_middle_values() {
        let (median, _, _, _) = compute_stats(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(median, 2.5);
    }

    #[test]
    fn model_speedup_is_close_to_the_batch_size() {
        // The ladder signing already contains the one underlying signature, so
        // the modelled ratio is the batch size (times the ratio of the two
        // signing costs) — not half of it.
        let speedup = model_speedup(1_000, 1_360_219_000.0, 1_379_698_000.0).expect("a ratio");
        assert!(
            (speedup - 985.9).abs() < 0.5,
            "unexpected model ratio: {speedup}"
        );
    }

    #[test]
    fn model_speedup_rejects_a_non_positive_ladder_cost() {
        assert!(model_speedup(100, 1_360_219_000.0, 0.0).is_none());
        assert!(model_speedup(100, 1_360_219_000.0, -1.0).is_none());
    }

    #[test]
    fn a_constant_sample_has_zero_variation() {
        let (_, min, max, cv) = compute_stats(&[5.0, 5.0, 5.0]);
        assert_eq!(min, 5.0);
        assert_eq!(max, 5.0);
        assert_eq!(cv, 0.0);
    }
}
