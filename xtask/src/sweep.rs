//! `cargo xtask sweep`: batch sizes from 1 to the largest feasible `N`.
//!
//! ```text
//! cargo xtask sweep --n 1,2,4,8,...,1048576            # full power-of-two sweep
//! cargo xtask sweep --quick                            # short resumable smoke run
//! cargo xtask sweep --resume --record data/sweep.recorded.json
//! ```
//!
//! Each measured size signs one real batch with libMTL and records the ladder
//! bytes, every condensed-signature length and the wall-clock time to sign the
//! whole batch. Sizes above `--measured-max` keep the same fields but are
//! labelled EXTRAPOLATED: their ladder shape comes from the closed-form model
//! validated against real bytes at every measured size and their cost from the
//! per-operation timings taken in the same pass.
//!
//! # Resumable
//!
//! `--resume` reads the existing output document, keeps its measured rows for
//! the requested sizes and only measures the missing ones. A document whose
//! seed or algorithm differs is a hard error, never silently mixed.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use climb_bench::{measure_sweep, SweepModelInputs};
use climb_mtl::DEFAULT_ALGORITHM;
use climb_report::sweep::{
    analyze, extrapolated_row, measured_row, SweepCostModel, SweepRow, SweepSection, DEFAULT_MAX_N,
    QUICK_SIZES, SWEEP_SCHEMA_VERSION,
};
use climb_report::EvidenceClass;

use crate::pin_env;

/// The run-time budget the sweep is expected to stay inside.
const SWEEP_BUDGET: Duration = Duration::from_secs(20 * 60);

#[derive(Args)]
pub(crate) struct SweepArgs {
    /// Batch sizes to record (comma-separated). Defaults to every power of two
    /// from 1 through `--max-n`.
    #[arg(long)]
    n: Option<String>,

    /// Short smoke sweep: 1, 2, 4, 8, 16, 32, 64, 256 and 1,024 messages.
    #[arg(long)]
    quick: bool,

    /// Largest power-of-two size in the default sweep.
    #[arg(long, default_value_t = DEFAULT_MAX_N)]
    max_n: u64,

    /// Largest size measured with the real library. Larger requested sizes are
    /// recorded as EXTRAPOLATED rows.
    #[arg(long, default_value_t = DEFAULT_MAX_N)]
    measured_max: u64,

    /// Timing samples per measured batch (the median is recorded).
    #[arg(long, default_value_t = 3)]
    samples: usize,

    /// Deterministic dataset seed.
    #[arg(long, default_value_t = climb_bench::DEFAULT_WIRE_SEED)]
    seed: u64,

    /// Where to write the sweep document.
    #[arg(long, default_value = "target/sweep.json")]
    out: PathBuf,

    /// Also copy the document to this path (used to refresh the committed
    /// fixture). The measurement is never fabricated; only a real run writes it.
    #[arg(long)]
    record: Option<PathBuf>,

    /// Reuse measured rows already present in `--out`.
    #[arg(long)]
    resume: bool,
}

fn parse_n_list(text: &str) -> Result<Vec<u64>> {
    let mut sizes = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let n: u64 = part
            .parse()
            .with_context(|| format!("'{part}' is not a batch size"))?;
        if n == 0 {
            bail!("a batch size must be at least 1");
        }
        if !sizes.contains(&n) {
            sizes.push(n);
        }
    }
    sizes.sort_unstable();
    if sizes.is_empty() {
        bail!("no batch sizes given");
    }
    Ok(sizes)
}

fn powers_of_two(max_n: u64) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut n = 1u64;
    while n <= max_n {
        sizes.push(n);
        let Some(next) = n.checked_mul(2) else {
            break;
        };
        n = next;
    }
    sizes
}

fn requested_sizes(args: &SweepArgs) -> Result<Vec<u64>> {
    if args.quick {
        return Ok(QUICK_SIZES.to_vec());
    }
    if let Some(ref list) = args.n {
        return parse_n_list(list);
    }
    if args.max_n == 0 {
        bail!("--max-n must be at least 1");
    }
    Ok(powers_of_two(args.max_n))
}

/// Keep only measured rows for requested sizes from a previous document.
fn resumable_rows(
    previous: Option<&SweepSection>,
    requested: &[u64],
    measured_max: u64,
    seed: u64,
) -> Result<Vec<SweepRow>> {
    let Some(previous) = previous else {
        return Ok(Vec::new());
    };
    if previous.seed != seed {
        bail!(
            "existing sweep was recorded with seed {} but this run uses {seed}; rerun without --resume",
            previous.seed
        );
    }
    if previous.algorithm != DEFAULT_ALGORITHM {
        bail!(
            "existing sweep used algorithm {:?} but this build uses {DEFAULT_ALGORITHM:?}; rerun without --resume",
            previous.algorithm
        );
    }
    Ok(previous
        .rows
        .iter()
        .filter(|row| row.class == EvidenceClass::Measured)
        .filter(|row| row.n <= measured_max)
        .filter(|row| requested.contains(&row.n))
        .cloned()
        .collect())
}

/// Derive the per-message tree cost from the largest measured row.
fn tree_per_message_ns(model: &SweepModelInputs, rows: &[SweepRow]) -> f64 {
    let largest = rows
        .iter()
        .filter(|row| row.class == EvidenceClass::Measured)
        .max_by_key(|row| row.n);
    let Some(row) = largest else {
        return 0.0;
    };
    let Some(batch_sign_ns) = row.batch_sign_ns else {
        return 0.0;
    };
    if row.n == 0 {
        return 0.0;
    }
    ((batch_sign_ns - model.t_plain_sign_ns) / row.n as f64).max(0.0)
}

fn cost_model(model: &SweepModelInputs, rows: &[SweepRow]) -> SweepCostModel {
    SweepCostModel {
        t_plain_sign_ns: model.t_plain_sign_ns,
        t_plain_verify_ns: model.t_plain_verify_ns,
        t_mtl_single_sign_ns: model.t_mtl_single_sign_ns,
        t_verify_condensed_cached_ns: model.t_verify_condensed_cached_ns,
        t_verify_condensed_cold_ns: model.t_verify_condensed_cold_ns,
        plain_signature_len_bytes: model.plain_signature_len_bytes as u64,
        sign_samples: model.sign_samples,
        verify_samples: model.verify_samples,
        tree_per_message_ns: tree_per_message_ns(model, rows),
        formula: "C = t_plain_sign + N x tree_per_message".to_string(),
    }
}

fn read_previous(path: &Path) -> Result<Option<SweepSection>> {
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let section: SweepSection = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a sweep document", path.display()))?;
    Ok(Some(section))
}

fn write_document(path: &Path, document: &SweepSection) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let json =
        serde_json::to_string_pretty(document).context("failed to serialize the sweep document")?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn cmd_sweep(args: SweepArgs) -> Result<()> {
    if args.samples == 0 {
        bail!("--samples must be at least 1");
    }
    let requested = requested_sizes(&args)?;
    let previous = if args.resume {
        read_previous(&args.out)?
    } else {
        None
    };
    let existing = resumable_rows(previous.as_ref(), &requested, args.measured_max, args.seed)?;

    let measured_sizes: Vec<u64> = requested
        .iter()
        .copied()
        .filter(|n| *n <= args.measured_max)
        .collect();
    let missing: Vec<u64> = measured_sizes
        .iter()
        .copied()
        .filter(|n| !existing.iter().any(|row| row.n == *n))
        .collect();

    let started = Instant::now();
    let mut rows: Vec<SweepRow> = existing;
    let model = if missing.is_empty() {
        // Nothing new to measure. Reuse the previous cost model when resuming;
        // otherwise take the per-operation timings with an empty batch list.
        if let (true, Some(previous)) = (args.resume, previous.as_ref()) {
            if previous.cost_model.plain_signature_len_bytes > 0 {
                previous.cost_model.clone()
            } else {
                measure_model_only(args.seed, args.samples)?
            }
        } else {
            measure_model_only(args.seed, args.samples)?
        }
    } else {
        eprintln!(
            "sweep: measuring {} missing size(s): {}",
            missing.len(),
            missing
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let measurement = measure_sweep(args.seed, &missing, args.samples, args.samples)
            .context("sweep measurement failed")?;
        let provisional: Vec<SweepRow> = measurement
            .rows
            .iter()
            .map(|row| climb_report::sweep::measured_row(row.clone(), &SweepCostModel::default()))
            .collect();
        let model = cost_model(&measurement.model, &provisional);
        for row in &measurement.rows {
            rows.push(measured_row(row.clone(), &model));
        }
        model
    };

    // Every requested size above the measured limit becomes a model row; so
    // does a measured size the previous document did not carry (only possible
    // when the caller deliberately lowered `--measured-max` while resuming).
    for &n in &requested {
        if rows.iter().any(|row| row.n == n) {
            continue;
        }
        if n > args.measured_max {
            rows.push(extrapolated_row(n, &model));
        }
    }
    rows.sort_by_key(|row| row.n);
    rows.dedup_by_key(|row| row.n);

    let measured_max_actual = rows
        .iter()
        .filter(|row| row.class == EvidenceClass::Measured)
        .map(|row| row.n)
        .max()
        .unwrap_or(0);
    let extrapolated = rows
        .iter()
        .filter(|row| row.class == EvidenceClass::Extrapolated)
        .count();
    let elapsed = started.elapsed();
    let mut limit_reason = format!(
        "measured with the real library to N = {measured_max_actual}; sizes above use the closed-form ladder model validated against every measured row and the per-operation cost model"
    );
    if elapsed > SWEEP_BUDGET {
        limit_reason.push_str(&format!(
            "; this run took {:.1} min, above the 20-minute budget, so lowering --measured-max is advised",
            elapsed.as_secs_f64() / 60.0
        ));
    }

    let mut document = SweepSection {
        schema_version: SWEEP_SCHEMA_VERSION,
        generated_at: pin_env::iso_timestamp(),
        algorithm: DEFAULT_ALGORITHM.to_string(),
        seed: args.seed,
        measured_max_n: measured_max_actual,
        limit_reason,
        elapsed_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        cost_model: model,
        rows,
        crossovers: Vec::new(),
        udp_fit: Vec::new(),
        best_range: None,
    };
    analyze(&mut document);

    write_document(&args.out, &document)?;
    eprintln!(
        "wrote sweep to {} ({} row(s), {extrapolated} extrapolated, {:.1} s)",
        args.out.display(),
        document.rows.len(),
        elapsed.as_secs_f64()
    );
    if let Some(record) = &args.record {
        write_document(record, &document)?;
        eprintln!("recorded sweep fixture to {}", record.display());
    }

    print_table(&document);
    print_analysis(&document);
    Ok(())
}

/// Take only the per-operation timings, with no measured batch rows.
fn measure_model_only(seed: u64, samples: usize) -> Result<SweepCostModel> {
    let measurement =
        measure_sweep(seed, &[], samples, samples).context("sweep per-operation timing failed")?;
    Ok(cost_model(&measurement.model, &[]))
}

fn print_table(document: &SweepSection) {
    let header = format!(
        "{:>9} {:>12} {:>5} {:>8} {:>8} {:>8} {:>8} {:>13} {:>13} {:>13} {:>10}",
        "N",
        "Class",
        "Rung",
        "Ladder",
        "CondMin",
        "CondMean",
        "CondMax",
        "A bytes",
        "B bytes",
        "C bytes",
        "Sign ms"
    );
    let separator = "-".repeat(header.len());
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{header}");
    let _ = writeln!(handle, "{separator}");
    for row in &document.rows {
        let sign = row
            .batch_sign_ns
            .map(|ns| format!("{:.1}", ns / 1e6))
            .unwrap_or_else(|| "\u{2014}".to_string());
        let _ = writeln!(
            handle,
            "{n:>9} {class:>12} {rungs:>5} {ladder:>8} {cmin:>8} {cmean:>8.1} {cmax:>8} {a:>13} {b:>13} {c:>13} {sign:>10}",
            n = row.n,
            class = row.class.label(),
            rungs = row.rung_count,
            ladder = row.signed_ladder_len,
            cmin = row.condensed_min,
            cmean = row.condensed_mean,
            cmax = row.condensed_max,
            a = row.wire_a_bytes,
            b = row.wire_b_bytes,
            c = row.wire_c_bytes,
            sign = sign,
        );
    }
}

fn print_analysis(document: &SweepSection) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle);
    for crossover in &document.crossovers {
        let n = crossover
            .n
            .map(|n| format!("N = {n}"))
            .unwrap_or_else(|| "not reached".to_string());
        let _ = writeln!(
            handle,
            "break-even {}: {n} ({})",
            crossover.metric,
            crossover.class.label()
        );
    }
    for row in document
        .udp_fit
        .iter()
        .filter(|row| row.config == "mtl-amortised")
    {
        let _ = writeln!(
            handle,
            "UDP fit {} / C: first N = {}, last N = {}",
            row.profile_label,
            row.first_fit_n
                .map(|n| n.to_string())
                .unwrap_or_else(|| "\u{2014}".to_string()),
            row.last_fit_n
                .map(|n| n.to_string())
                .unwrap_or_else(|| "\u{2014}".to_string()),
        );
    }
    if let Some(range) = document.best_range.as_ref() {
        let _ = writeln!(
            handle,
            "best N range: {} \u{2013} {} ({})",
            range.start_n,
            range
                .end_n
                .map(|n| n.to_string())
                .unwrap_or_else(|| "\u{2014}".to_string()),
            range.class.label()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> SweepArgs {
        SweepArgs {
            n: None,
            quick: false,
            max_n: DEFAULT_MAX_N,
            measured_max: DEFAULT_MAX_N,
            samples: 3,
            seed: 1,
            out: PathBuf::from("unused.json"),
            record: None,
            resume: false,
        }
    }

    #[test]
    fn parse_n_list_sorts_and_dedupes() {
        assert_eq!(parse_n_list("8,1,4,8,2").expect("parses"), vec![1, 2, 4, 8]);
    }

    #[test]
    fn parse_n_list_rejects_zero_and_garbage() {
        assert!(parse_n_list("1,0").is_err());
        assert!(parse_n_list("one").is_err());
        assert!(parse_n_list(" , ").is_err());
    }

    #[test]
    fn the_default_sweep_is_every_power_of_two_through_the_limit() {
        let sizes = powers_of_two(1_048_576);
        assert_eq!(sizes.len(), 21);
        assert_eq!(sizes.first(), Some(&1));
        assert_eq!(sizes.last(), Some(&1_048_576));
        assert!(sizes.iter().all(|n| n.is_power_of_two()));
    }

    #[test]
    fn the_quick_list_is_a_prefix_of_the_default_sweep() {
        let full = powers_of_two(DEFAULT_MAX_N);
        assert!(QUICK_SIZES.iter().all(|n| full.contains(n)));
        assert_eq!(QUICK_SIZES.last(), Some(&1_024));
    }

    #[test]
    fn quick_overrides_an_explicit_list() {
        let mut args = args();
        args.quick = true;
        args.n = Some("7".to_string());
        assert_eq!(requested_sizes(&args).expect("sizes"), QUICK_SIZES.to_vec());
    }

    #[test]
    fn resume_keeps_only_requested_measured_rows() {
        let model = SweepCostModel {
            plain_signature_len_bytes: 7_856,
            ..SweepCostModel::default()
        };
        let measured = measured_row(
            climb_bench::SweepMeasuredRow {
                n: 4,
                rung_count: 1,
                ladder_buffer_len: 68,
                signed_ladder_len: 7_928,
                signature_len: 7_856,
                condensed_min: 108,
                condensed_mean: 108.0,
                condensed_max: 108,
                condensed_total: 432,
                batch_sign_ns: 1.0,
            },
            &model,
        );
        let extrapolated = extrapolated_row(1_048_576, &model);
        let previous = SweepSection {
            schema_version: SWEEP_SCHEMA_VERSION,
            generated_at: "2026-11-13T00:00:00Z".to_string(),
            algorithm: DEFAULT_ALGORITHM.to_string(),
            seed: 1,
            measured_max_n: 4,
            limit_reason: "test".to_string(),
            elapsed_ms: 0,
            cost_model: model,
            rows: vec![measured, extrapolated],
            crossovers: Vec::new(),
            udp_fit: Vec::new(),
            best_range: None,
        };
        let kept = resumable_rows(Some(&previous), &[4, 8], 8, 1).expect("resume");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.first().map(|row| row.n), Some(4));
    }

    #[test]
    fn resume_refuses_a_different_seed() {
        let previous = SweepSection {
            schema_version: SWEEP_SCHEMA_VERSION,
            generated_at: "2026-11-13T00:00:00Z".to_string(),
            algorithm: DEFAULT_ALGORITHM.to_string(),
            seed: 2,
            measured_max_n: 1,
            limit_reason: "test".to_string(),
            elapsed_ms: 0,
            cost_model: SweepCostModel::default(),
            rows: Vec::new(),
            crossovers: Vec::new(),
            udp_fit: Vec::new(),
            best_range: None,
        };
        assert!(resumable_rows(Some(&previous), &[1], 1, 1).is_err());
    }

    #[test]
    fn tree_cost_comes_from_the_largest_measured_row() {
        let model = SweepModelInputs {
            t_plain_sign_ns: 500_000.0,
            t_plain_verify_ns: 1.0,
            t_mtl_single_sign_ns: 500_000.0,
            t_verify_condensed_cached_ns: 1.0,
            t_verify_condensed_cold_ns: 500.0,
            plain_signature_len_bytes: 7_856,
            sign_samples: 1,
            verify_samples: 1,
        };
        let cost = cost_model(&model, &[extrapolated_row(4, &SweepCostModel::default())]);
        assert_eq!(cost.tree_per_message_ns, 0.0);

        let measured = measured_row(
            climb_bench::SweepMeasuredRow {
                n: 100,
                rung_count: 3,
                ladder_buffer_len: 132,
                signed_ladder_len: 7_992,
                signature_len: 7_856,
                condensed_min: 108,
                condensed_mean: 164.32,
                condensed_max: 172,
                condensed_total: 16_432,
                batch_sign_ns: 520_000.0,
            },
            &SweepCostModel::default(),
        );
        let cost = cost_model(&model, &[measured]);
        assert!((cost.tree_per_message_ns - 200.0).abs() < 1e-9);
    }
}
