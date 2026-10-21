//! `cargo xtask stability`: repeat the signing-primitives benchmark several
//! times under a pinned core and record the run-to-run spread of the derived
//! signing speed-up model.
//!
//! Each run measures the per-message MTL signing primitive `t_mtl_single` and
//! the per-batch amortised signing cost `t_tree(N)` for every selected dataset
//! size. The signing speed-up model is then
//! `B/C = (N * t_mtl_single) / (t_mtl_single + t_tree(N))`. Across runs the
//! median, min, max, coefficient of variation and run count are computed per
//! size and merged additively into `derived.stability` of the consolidated
//! results artifact, so a run-to-run instability in a headline ratio is visible
//! without re-deriving it by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::Value;

use climb_report::{StabilityRatioStats, StabilitySection};

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
            let c_model = t_mtl_single + t_tree;
            let b_model = (*n as f64) * t_mtl_single;
            per_ratio
                .entry(format!("signing_speedup_model_{label}"))
                .or_default()
                .push(b_model / c_model);
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
fn merge_stability(results_path: &Path, section: StabilitySection) -> Result<()> {
    let text = std::fs::read_to_string(results_path)
        .with_context(|| format!("failed to read {}", results_path.display()))?;
    let mut doc: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", results_path.display()))?;
    let derived = doc
        .get_mut("derived")
        .and_then(|d| d.as_object_mut())
        .with_context(|| format!("{} has no derived object", results_path.display()))?;
    derived.insert("stability".to_string(), serde_json::to_value(&section)?);
    let out = serde_json::to_string_pretty(&doc)?;
    std::fs::write(results_path, out)
        .with_context(|| format!("failed to write {}", results_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_constant_sample_has_zero_variation() {
        let (_, min, max, cv) = compute_stats(&[5.0, 5.0, 5.0]);
        assert_eq!(min, 5.0);
        assert_eq!(max, 5.0);
        assert_eq!(cv, 0.0);
    }
}
