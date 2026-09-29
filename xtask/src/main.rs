use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;

use climb_report::{parse_group_id, render_text_summary, scan_criterion, summarize};

mod groups;
mod memory;
mod pin_env;
mod wire;
use groups::{BenchTarget, DatasetSize, GROUPS};
use memory::{cmd_memory_report, MemoryReportArgs};
use pin_env::cmd_pin_env;
use wire::{cmd_wire_report, WireReportArgs};

/// The logical CPU to pin benchmarks to.
///
/// Host: Intel i3-1115G4, 2 physical cores / 4 logical CPUs.
/// Physical core 0 = {cpu0, cpu2}, physical core 1 = {cpu1, cpu3}.
/// Interrupt totals measured at Phase 6 start:
///   cpu1=1,428,276  cpu3=140,767  cpu0=65,862  cpu2=53,645
/// CPU 2 is the quietest logical CPU and the quieter sibling of physical core 0,
/// making it the least-contended choice for a single-threaded benchmark.
const PIN_CPU: u32 = 2;

/// CLIMB task automation — pin the environment, run benchmarks, generate reports.
#[derive(Parser)]
#[command(name = "xtask", about = "CLIMB automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Capture the current build environment and write it to environment.json.
    PinEnv,
    /// Run the Phase 5 signing and Phase 6 verification benchmark suites.
    BenchAll(BenchAllArgs),
    /// Print a plain-text summary of Criterion's latest results.
    Report(ReportArgs),
    /// Measure and display permanent wire-size numbers (MTL vs non-MTL).
    ///
    /// Calls climb_bench::measure_wire_sizes for each requested dataset size
    /// and prints a human-readable table.  When --out is given, also writes a
    /// JSON array of WireSizeReport values.
    WireReport(WireReportArgs),
    /// Measure RSS delta before/after sign_batch and cross-validate against
    /// climb-analytical memory model.
    ///
    /// Runs `repeat` measurements per size (default 2), prints measured vs
    /// analytical comparison table.  Separates libMTL internal footprint from
    /// dataset/harness overhead by sampling RSS immediately before and after
    /// sign_batch with the dataset already held in memory.
    MemoryReport(MemoryReportArgs),
}

#[derive(Args)]
struct BenchAllArgs {
    /// Dataset sizes to include (comma-separated, or `all`).
    #[arg(long, value_delimiter = ',', default_value = "all")]
    sizes: Vec<SizeArg>,

    /// Save results under this Criterion baseline name.
    #[arg(long)]
    baseline: Option<String>,

    /// Also run the without-MTL signing groups that take hours to days per run
    /// on the reference hardware (medium ~18 h, large ~78 days).
    #[arg(long)]
    include_slow: bool,

    /// Run unpinned, for machines without `taskset` (e.g. minimal CI containers).
    #[arg(long)]
    no_pin: bool,
}

#[derive(Args)]
struct ReportArgs {
    /// Criterion baseline directory to read (`new` is the latest run).
    #[arg(long, default_value = "new")]
    baseline: String,

    /// Criterion output root.
    #[arg(long, default_value = "target/criterion")]
    criterion_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SizeArg {
    All,
    Small,
    OneK,
    Medium,
    Large,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::PinEnv => cmd_pin_env(),
        Commands::BenchAll(args) => cmd_bench_all(args),
        Commands::Report(args) => cmd_report(args),
        Commands::WireReport(args) => cmd_wire_report(args),
        Commands::MemoryReport(args) => cmd_memory_report(args),
    }
}

fn cmd_bench_all(args: BenchAllArgs) -> Result<()> {
    let sizes = selected_sizes(&args);

    let mut signing = Vec::new();
    let mut verifying = Vec::new();
    let mut skipped = Vec::new();

    for group in GROUPS {
        if !sizes.contains(&group.size) {
            continue;
        }
        if group.slow && !args.include_slow {
            skipped.push(group.name);
            continue;
        }
        match group.target {
            BenchTarget::Signing => signing.push(group.name),
            BenchTarget::Verifying => verifying.push(group.name),
        }
    }

    if !skipped.is_empty() {
        eprintln!("skipping analytically-infeasible groups (pass --include-slow to run them):");
        for name in &skipped {
            eprintln!("  {name}");
        }
    }

    let pin_cpu = resolve_pin_cpu(!args.no_pin);

    run_bench_target("signing", &signing, args.baseline.as_deref(), pin_cpu)?;
    run_bench_target("verifying", &verifying, args.baseline.as_deref(), pin_cpu)?;

    eprintln!("bench-all complete");
    Ok(())
}

fn selected_sizes(args: &BenchAllArgs) -> Vec<DatasetSize> {
    let mut sizes = Vec::new();
    for arg in &args.sizes {
        let size = match arg {
            SizeArg::All => {
                return vec![
                    DatasetSize::Small,
                    DatasetSize::OneK,
                    DatasetSize::Medium,
                    DatasetSize::Large,
                ]
            }
            SizeArg::Small => DatasetSize::Small,
            SizeArg::OneK => DatasetSize::OneK,
            SizeArg::Medium => DatasetSize::Medium,
            SizeArg::Large => DatasetSize::Large,
        };
        if !sizes.contains(&size) {
            sizes.push(size);
        }
    }
    sizes
}

fn resolve_pin_cpu(enabled: bool) -> Option<u32> {
    if !enabled {
        return None;
    }
    if !binary_on_path("taskset") {
        eprintln!("warning: taskset not found on PATH — running unpinned");
        return None;
    }
    let cpu_path = format!("/sys/devices/system/cpu/cpu{PIN_CPU}");
    if !Path::new(&cpu_path).exists() {
        eprintln!(
            "warning: CPU {PIN_CPU} not present ({cpu_path} does not exist) — running unpinned"
        );
        return None;
    }
    Some(PIN_CPU)
}

fn binary_on_path(name: &str) -> bool {
    let path_var = std::env::var_os("PATH");
    match path_var {
        None => false,
        Some(joined) => std::env::split_paths(&joined).any(|dir| dir.join(name).exists()),
    }
}

fn build_bench_command(
    target: &str,
    filter: &str,
    baseline: Option<&str>,
    pin_cpu: Option<u32>,
) -> Command {
    let mut cmd = if let Some(cpu) = pin_cpu {
        let mut c = Command::new("taskset");
        c.args(["-c", &cpu.to_string(), "cargo"]);
        c
    } else {
        Command::new("cargo")
    };

    cmd.args([
        "bench",
        "-p",
        "climb-bench",
        "--bench",
        target,
        "--",
        filter,
    ]);
    if let Some(name) = baseline {
        cmd.args(["--save-baseline", name]);
    }
    cmd
}

fn run_bench_target(
    target: &str,
    groups: &[&str],
    baseline: Option<&str>,
    pin_cpu: Option<u32>,
) -> Result<()> {
    if groups.is_empty() {
        return Ok(());
    }

    let filter = groups.join("|");
    let cmd = build_bench_command(target, &filter, baseline, pin_cpu);

    eprintln!("running: {}", render_cmd(&cmd));
    let mut child = cmd;
    let status = child
        .status()
        .with_context(|| format!("failed to spawn cargo bench for {target}"))?;
    if !status.success() {
        bail!("cargo bench --bench {target} exited with {status}");
    }
    Ok(())
}

fn render_cmd(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_str().unwrap_or("?").to_string()];
    for arg in cmd.get_args() {
        parts.push(arg.to_str().unwrap_or("?").to_string());
    }
    parts.join(" ")
}

fn cmd_report(args: ReportArgs) -> Result<()> {
    let records = scan_criterion(&args.criterion_dir, &args.baseline).with_context(|| {
        format!(
            "failed to read Criterion output from {} (baseline '{}')",
            args.criterion_dir.display(),
            args.baseline
        )
    })?;

    let mut known = Vec::with_capacity(records.len());
    for record in &records {
        match parse_group_id(&record.group_id) {
            Ok(_) => known.push(record.clone()),
            Err(_) => eprintln!("note: ignoring unrecognized benchmark '{}'", record.full_id),
        }
    }

    if known.is_empty() {
        bail!(
            "no recognized CLIMB benchmarks under {} (baseline '{}')",
            args.criterion_dir.display(),
            args.baseline
        );
    }

    let rows = summarize(&known)?;
    print!("{}", render_text_summary(&rows));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_command_invokes_taskset_with_cpu() {
        let cmd = build_bench_command("signing", "group_a|group_b", None, Some(2));
        let program = cmd.get_program().to_str().unwrap();
        assert_eq!(program, "taskset");
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert!(args.contains(&"-c"));
        assert!(args.contains(&"2"));
        assert!(args.contains(&"cargo"));
        assert!(args.contains(&"bench"));
        assert!(args.contains(&"signing"));
        assert!(args.contains(&"--"));
        assert!(args.contains(&"group_a|group_b"));
        assert!(!args.contains(&"--save-baseline"));
    }

    #[test]
    fn pinned_command_includes_save_baseline_when_given() {
        let cmd = build_bench_command("signing", "g1", Some("my-baseline"), Some(2));
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert!(args.contains(&"--save-baseline"));
        assert!(args.contains(&"my-baseline"));
    }

    #[test]
    fn unpinned_command_invokes_cargo_directly() {
        let cmd = build_bench_command("verifying", "v1", None, None);
        let program = cmd.get_program().to_str().unwrap();
        assert_eq!(program, "cargo");
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert!(!args.contains(&"taskset"));
    }

    #[test]
    fn unpinned_command_still_passes_save_baseline() {
        let cmd = build_bench_command("verifying", "v1", Some("bl"), None);
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert!(args.contains(&"--save-baseline"));
        assert!(args.contains(&"bl"));
    }

    #[test]
    fn binary_on_path_finds_true_for_known_binary() {
        assert!(binary_on_path("cargo"));
    }

    #[test]
    fn binary_on_path_returns_false_for_nonexistent() {
        assert!(!binary_on_path("__nonexistent_command_xyzzy__"));
    }
}
