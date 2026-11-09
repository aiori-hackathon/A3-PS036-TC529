//! The `reproduce` subcommand: run the whole artifact chain, in order, from
//! a single command.
//!
//! The individual subcommands are each correct in isolation, but the
//! consolidated results only agree with the rendered report when they are run
//! in one specific order — pin the environment, measure wire sizes, measure
//! memory, (optionally) re-run the benchmarks, verify the ladder shape, capture
//! the DNS pipeline, then consolidate, chart, render and self-check.  Run them
//! by hand and it is easy to skip a measurement or consolidate without its
//! inputs, which leaves the report claiming numbers the results no longer carry
//! (or vice versa).
//!
//! ```text
//! cargo xtask reproduce --report-only
//! cargo xtask reproduce --remeasure --quick --skip-dns
//! cargo xtask reproduce --remeasure --force
//! ```
//!
//! Two modes are deliberately distinct. `--report-only` reuses the measurement
//! artifacts already on disk and regenerates only the derived chain
//! (consolidation, charts, report), so it must never be described as a
//! re-measurement. `--remeasure` re-runs the benchmarks and measurements before
//! regenerating. The chosen mode is printed as a banner and recorded in the
//! consolidated artifact, where the report reads it back.
//!
//! The command is planned then executed.  [`plan`] is a pure function from
//! [`ReproduceOpts`] to an ordered list of [`Step`]s: it spawns no process and
//! performs no I/O, so every ordering and skip/force combination is unit
//! testable.  Execution walks the plan, times each step, and stops at the first
//! failure with that step named.  The last step is always a report self-check,
//! so a successful exit means the artifacts agree.

use anyhow::{bail, Context, Result};
use clap::Args;
use climb_report::ReproductionMode;
use std::process::Command;

use crate::proc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Canonical artifact paths — the single set of files the chain writes and the
// later steps read.  Written explicitly into every argv so the plan is
// self-documenting and does not silently follow a changed upstream default.
// ---------------------------------------------------------------------------

/// Wire-size measurements consumed by `consolidate`.
const WIRE_REPORT: &str = "target/wire-report.json";
/// Memory measurements consumed by `consolidate`.
const MEMORY_REPORT: &str = "target/memory-report.json";
/// Consolidated results consumed by `charts` and both report invocations.
const RESULTS: &str = "target/climb-results.json";
/// Rendered report validated by the final self-check step.
const REPORT_HTML: &str = "target/climb-report/report.html";
/// Directory holding the rendered SVG charts.
const CHARTS_DIR: &str = "target/climb-report/charts";
/// Live DNS capture written by `dns-demo --record`; `consolidate` prefers it
/// and falls back to the committed recorded fixture when it is absent.
const DNS_LIVE: &str = "target/dns-capture.json";
/// Ladder verification artifact written by `ladder-verify`; consumed by `consolidate`.
const LADDER_VERIFICATION: &str = "target/ladder-verification.json";

/// Blueprint experiment sizes, in canonical order.
const SIZES_FULL: &str = "small,medium,large";
/// Scope for `--quick`: the two sizes that finish in seconds.
const SIZES_QUICK: &str = "small,medium";
/// Leaf counts for `--quick` ladder verification: a short sweep that still
/// covers a single leaf, exact powers of two, and a non-power of two.  The full
/// run omits `--sizes` and measures the command's default 17-size sweep.
const LADDER_SIZES_QUICK: &str = "1,2,4,5,13";

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// Arguments for `cargo xtask reproduce`.
#[derive(Args)]
pub(crate) struct ReproduceArgs {
    /// Limit every measurement to Small/Medium and use the short ladder sweep.
    /// Has no effect in report-only mode.
    #[arg(long)]
    quick: bool,

    /// Reuse the measurement artifacts already on disk and regenerate only the
    /// derived artifacts (consolidation, charts, report). This is the default.
    #[arg(long, conflicts_with_all = ["remeasure", "bench"])]
    report_only: bool,

    /// Re-run the benchmarks and measurements before regenerating the derived
    /// artifacts. This is the only mode that may be called a re-measurement.
    #[arg(long, conflicts_with = "report_only")]
    remeasure: bool,

    /// Hidden alias for `--remeasure`, kept for scripts that used the old flag.
    #[arg(long, hide = true, conflicts_with = "report_only")]
    bench: bool,

    /// Skip the live DNS pipeline and use the committed recorded capture.
    /// Also implied when Docker is unavailable.
    #[arg(long)]
    skip_dns: bool,

    /// Forward `--force` to `consolidate`, accepting stale optional inputs
    /// instead of recording freshness warnings for them.
    #[arg(long)]
    force: bool,
}

/// Fail before any step when a report-only run is missing a required input.
///
/// Each error names the path and the command that produces it, so the fix is
/// one line rather than a consolidation warning buried in a successful run.
fn check_report_inputs() -> Result<()> {
    let required = [
        (
            WIRE_REPORT,
            "cargo xtask wire-report --out target/wire-report.json",
        ),
        (
            MEMORY_REPORT,
            "cargo xtask memory-report --out target/memory-report.json",
        ),
        ("environment.json", "cargo xtask pin-env"),
    ];
    for (path, fix) in required {
        let p = std::path::Path::new(path);
        if !p.exists() {
            bail!("required input missing: {path} (produce it with `{fix}` or run --remeasure)");
        }
    }
    let criterion = std::path::Path::new("target/criterion");
    match std::fs::read_dir(criterion) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                bail!(
                    "required input empty: target/criterion (run `cargo bench` or `cargo xtask reproduce --remeasure`)"
                );
            }
        }
        Err(error) => bail!(
            "required input missing: target/criterion ({error}; run `cargo bench` or `cargo xtask reproduce --remeasure`)"
        ),
    }
    let ladder = [
        "target/ladder-verification.json",
        "data/ladder-verification.recorded.json",
    ];
    if !ladder
        .iter()
        .any(|path| std::path::Path::new(path).exists())
    {
        bail!(
            "required input missing: target/ladder-verification.json or data/ladder-verification.recorded.json (run `cargo xtask ladder-verify`)"
        );
    }
    let capture = ["target/dns-capture.json", "data/dns-capture.recorded.json"];
    if !capture
        .iter()
        .any(|path| std::path::Path::new(path).exists())
    {
        bail!(
            "required input missing: target/dns-capture.json or data/dns-capture.recorded.json (run `cargo xtask dns-demo --record target/dns-capture.json` or commit the recorded fixture)"
        );
    }
    Ok(())
}

/// Resolve the two explicit modes plus the hidden legacy alias. Rejecting both
/// modes at once is a usage error rather than a silent preference.
pub(crate) fn resolve_mode(
    report_only: bool,
    remeasure: bool,
    bench_alias: bool,
) -> Result<ReproductionMode> {
    if report_only && (remeasure || bench_alias) {
        bail!("--report-only and --remeasure/--bench are mutually exclusive");
    }
    if remeasure || bench_alias {
        Ok(ReproductionMode::Remeasure)
    } else {
        Ok(ReproductionMode::ReportOnly)
    }
}

/// The banner printed before any step runs. Naming the mode up front prevents a
/// cheap re-render from being reported as a re-measurement.
pub(crate) fn mode_banner(mode: ReproductionMode) -> String {
    match mode {
        ReproductionMode::Remeasure => {
            "reproduce mode: remeasure — re-running benchmarks and measurements, then regenerating derived artifacts".to_string()
        }
        ReproductionMode::ReportOnly => {
            "reproduce mode: report-only — reusing existing measurements; regenerating derived artifacts only (not a re-measurement)".to_string()
        }
        ReproductionMode::Unknown => {
            "reproduce mode: unknown — refusing to guess; pass --report-only or --remeasure".to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Planning — pure, no process spawning
// ---------------------------------------------------------------------------

/// One step of the reproduction chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepId {
    PinEnv,
    WireReport,
    MemoryReport,
    BenchAll,
    LadderVerify,
    DnsDemo,
    Consolidate,
    VerifyConsolidated,
    Charts,
    ReportHtml,
    ReportCheck,
}

impl StepId {
    /// Stable, human-readable label used in progress lines and the summary
    /// table.  These are process-invocation labels, not measured results.
    pub(crate) fn label(self) -> &'static str {
        match self {
            StepId::PinEnv => "pin-env",
            StepId::WireReport => "wire-report",
            StepId::MemoryReport => "memory-report",
            StepId::BenchAll => "bench-all",
            StepId::LadderVerify => "ladder-verify",
            StepId::DnsDemo => "dns-demo",
            StepId::Consolidate => "consolidate",
            StepId::VerifyConsolidated => "verify-consolidated",
            StepId::Charts => "charts",
            StepId::ReportHtml => "report --format html",
            StepId::ReportCheck => "report --check",
        }
    }
}

/// Why a planned step will not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// `--skip-dns` was passed.
    SkipDnsFlag,
    /// The Docker daemon is unreachable.
    DockerUnavailable,
}

impl SkipReason {
    /// The status text shown in the summary table for this skip.
    pub(crate) fn summary(self) -> &'static str {
        match self {
            SkipReason::SkipDnsFlag => "skipped: using recorded fixture (--skip-dns)",
            SkipReason::DockerUnavailable => "skipped: using recorded fixture (Docker unavailable)",
        }
    }
}

/// A single planned step: an `xtask` subcommand invocation, or a documented
/// skip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub id: StepId,
    /// Arguments passed to the `xtask` binary (the subcommand and its flags).
    pub argv: Vec<String>,
    /// `Some` when the step is deliberately not run.
    pub skip: Option<SkipReason>,
}

impl Step {
    fn run(id: StepId, argv: &[&str]) -> Self {
        Step {
            id,
            argv: argv.iter().map(|a| (*a).to_string()).collect(),
            skip: None,
        }
    }

    fn skipped(self, reason: SkipReason) -> Self {
        Step {
            skip: Some(reason),
            ..self
        }
    }
}

/// The inputs that shape the plan.
///
/// `docker_available` is resolved by the caller and injected here so [`plan`]
/// stays pure and its tests never touch Docker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReproduceOpts {
    pub quick: bool,
    pub skip_dns: bool,
    pub force: bool,
    pub docker_available: bool,
    /// `report-only` or `remeasure`; the mode also travels into `consolidate`.
    pub mode: ReproductionMode,
}

/// Build the ordered step list for `opts`.  Pure: no I/O, no subprocesses.
pub(crate) fn plan(opts: &ReproduceOpts) -> Vec<Step> {
    let mut steps = Vec::with_capacity(11);
    let remeasure = opts.mode == ReproductionMode::Remeasure;

    // Measurements belong only to a re-measurement. A report-only run reuses
    // whatever measurement artifacts are already on disk (or the committed
    // fixtures) and starts at consolidation.
    if remeasure {
        let sizes = if opts.quick { SIZES_QUICK } else { SIZES_FULL };
        steps.push(Step::run(StepId::PinEnv, &["pin-env"]));

        steps.push(Step::run(
            StepId::WireReport,
            &["wire-report", "--sizes", sizes, "--out", WIRE_REPORT],
        ));

        steps.push(Step::run(
            StepId::MemoryReport,
            &["memory-report", "--sizes", sizes, "--out", MEMORY_REPORT],
        ));

        // A re-measurement always re-runs the benchmarks; that is the claim the
        // mode makes.
        steps.push(Step::run(
            StepId::BenchAll,
            &["bench-all", "--sizes", sizes],
        ));

        // Ladder verification is a fast measurement and always runs.  The
        // `--quick` sweep is shorter than the command's default 17-size run.
        let mut ladder = vec!["ladder-verify".to_string()];
        if opts.quick {
            ladder.push("--sizes".to_string());
            ladder.push(LADDER_SIZES_QUICK.to_string());
        }
        ladder.push("--out".to_string());
        ladder.push(LADDER_VERIFICATION.to_string());
        steps.push(Step {
            id: StepId::LadderVerify,
            argv: ladder,
            skip: None,
        });

        // The live DNS pipeline requires Docker; either gate falls back to the
        // committed recorded fixture, which `consolidate` picks up by default.
        let dns = Step::run(StepId::DnsDemo, &["dns-demo", "--record", DNS_LIVE]);
        steps.push(if opts.skip_dns {
            dns.skipped(SkipReason::SkipDnsFlag)
        } else if !opts.docker_available {
            dns.skipped(SkipReason::DockerUnavailable)
        } else {
            dns
        });
    }

    let mut consolidate = vec![
        "consolidate".to_string(),
        "--wire-report".to_string(),
        WIRE_REPORT.to_string(),
        "--memory-report".to_string(),
        MEMORY_REPORT.to_string(),
        "--ladder-verification".to_string(),
        LADDER_VERIFICATION.to_string(),
        "--output".to_string(),
        RESULTS.to_string(),
        "--reproduction-mode".to_string(),
        opts.mode.as_str().to_string(),
    ];
    if opts.force {
        consolidate.push("--force".to_string());
    }
    steps.push(Step {
        id: StepId::Consolidate,
        argv: consolidate,
        skip: None,
    });

    // Consolidation warns on an absent memory report and errors on an
    // unparseable one, but silently accepts a valid report it fails to map.
    // This step fails the chain when the consolidated rows do not cover every
    // size the report describes.
    steps.push(Step::run(
        StepId::VerifyConsolidated,
        &[
            "verify-consolidated",
            "--memory-report",
            MEMORY_REPORT,
            "--results",
            RESULTS,
        ],
    ));

    steps.push(Step::run(StepId::Charts, &["charts"]));

    steps.push(Step::run(
        StepId::ReportHtml,
        &[
            "report",
            "--format",
            "html",
            "--out",
            REPORT_HTML,
            "--results",
            RESULTS,
            "--charts",
            CHARTS_DIR,
        ],
    ));

    // Always last: a clean report self-check is the success criterion.
    steps.push(Step::run(
        StepId::ReportCheck,
        &[
            "report",
            "--format",
            "html",
            "--check",
            "--out",
            REPORT_HTML,
            "--results",
            RESULTS,
        ],
    ));

    steps
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// One row of the final summary table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepResult {
    pub label: &'static str,
    pub status: String,
    pub seconds: Option<f64>,
}

/// The `xtask` binary to re-invoke for each step.
fn xtask_executable() -> Result<std::path::PathBuf> {
    std::env::current_exe().context("failed to locate the running xtask binary")
}

/// Whether the Docker daemon is reachable.  A missing CLI or a down daemon
/// both report `false`; the caller turns that into the recorded-fixture skip.
fn docker_available() -> bool {
    let mut command = Command::new("docker");
    command.args(["info", "--format", "{{.ServerVersion}}"]);
    proc::run(
        &mut command,
        std::time::Duration::from_secs(15),
        "docker info",
    )
    .is_ok()
}

/// Run one planned step as a child `xtask` process.
fn run_step(exe: &std::path::Path, step: &Step) -> Result<()> {
    let label = step.id.label();
    let mut command = Command::new(exe);
    command.args(&step.argv);
    proc::run(&mut command, std::time::Duration::from_secs(3600), label)?;
    Ok(())
}

/// Format a step duration for the summary table; skipped steps show an em
/// dash rather than a zero that might read as a measurement.
fn format_seconds(seconds: Option<f64>) -> String {
    match seconds {
        Some(s) => format!("{s:.1}"),
        None => "—".to_string(),
    }
}

/// Render the summary table (header + rows) without printing it, so the
/// formatting is unit-testable without capturing stdout.
fn summary_lines(results: &[StepResult]) -> Vec<String> {
    let mut lines = Vec::with_capacity(results.len() + 2);
    let header = format!("{:<26} {:<49} {:>8}", "step", "status", "seconds");
    lines.push(header.clone());
    lines.push("-".repeat(header.len()));
    for r in results {
        lines.push(format!(
            "{:<26} {:<49} {:>8}",
            r.label,
            r.status,
            format_seconds(r.seconds)
        ));
    }
    lines
}

fn print_summary(results: &[StepResult]) {
    println!();
    println!("reproduce summary");
    for line in summary_lines(results) {
        println!("{line}");
    }
}

pub(crate) fn cmd_reproduce(args: ReproduceArgs) -> Result<()> {
    let mode = resolve_mode(args.report_only, args.remeasure, args.bench)?;
    println!("{}", mode_banner(mode));

    let docker_available =
        mode == ReproductionMode::Remeasure && !args.skip_dns && docker_available();
    if mode == ReproductionMode::Remeasure && !args.skip_dns && !docker_available {
        bail!(
            "docker is unavailable; start the Docker daemon, or pass --skip-dns to use the committed capture"
        );
    }
    if mode == ReproductionMode::ReportOnly {
        check_report_inputs()?;
    }

    let opts = ReproduceOpts {
        quick: args.quick,
        skip_dns: args.skip_dns,
        force: args.force,
        mode,
        // `--skip-dns` already forces the fixture path; only probe Docker when
        // it could actually change the outcome, and only a re-measurement runs
        // the live pipeline at all.
        docker_available,
    };
    let steps = plan(&opts);
    let exe = xtask_executable()?;

    let mut results: Vec<StepResult> = Vec::with_capacity(steps.len());
    for step in &steps {
        let label = step.id.label();
        if let Some(reason) = step.skip {
            println!("[{label}] {}", reason.summary());
            results.push(StepResult {
                label,
                status: reason.summary().to_string(),
                seconds: None,
            });
            continue;
        }

        println!("[{label}] running: xtask {}", step.argv.join(" "));
        let started = Instant::now();
        match run_step(&exe, step) {
            Ok(()) => {
                let seconds = started.elapsed().as_secs_f64();
                println!("[{label}] ok ({seconds:.1}s)");
                results.push(StepResult {
                    label,
                    status: "ok".to_string(),
                    seconds: Some(seconds),
                });
            }
            Err(error) => {
                let seconds = started.elapsed().as_secs_f64();
                println!("[{label}] FAILED ({seconds:.1}s)");
                results.push(StepResult {
                    label,
                    status: "failed".to_string(),
                    seconds: Some(seconds),
                });
                print_summary(&results);
                // Fail fast with the failing step named; non-zero exit.
                return Err(error);
            }
        }
    }

    print_summary(&results);
    println!("reproduce mode: {}", mode.as_str());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remeasure() -> ReproduceOpts {
        ReproduceOpts {
            quick: false,
            skip_dns: false,
            force: false,
            docker_available: true,
            mode: ReproductionMode::Remeasure,
        }
    }

    fn report_only() -> ReproduceOpts {
        ReproduceOpts {
            quick: false,
            skip_dns: false,
            force: false,
            docker_available: true,
            mode: ReproductionMode::ReportOnly,
        }
    }

    fn kinds(steps: &[Step]) -> Vec<StepId> {
        steps.iter().map(|s| s.id).collect()
    }

    fn find(steps: &[Step], id: StepId) -> &Step {
        steps
            .iter()
            .find(|s| s.id == id)
            .expect("step present in plan")
    }

    fn has_flag(step: &Step, flag: &str) -> bool {
        step.argv.iter().any(|a| a == flag)
    }

    fn value_of<'a>(step: &'a Step, flag: &str) -> &'a str {
        let idx = step
            .argv
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("flag {flag} present"));
        step.argv[idx + 1].as_str()
    }

    fn full_order() -> Vec<StepId> {
        vec![
            StepId::PinEnv,
            StepId::WireReport,
            StepId::MemoryReport,
            StepId::BenchAll,
            StepId::LadderVerify,
            StepId::DnsDemo,
            StepId::Consolidate,
            StepId::VerifyConsolidated,
            StepId::Charts,
            StepId::ReportHtml,
            StepId::ReportCheck,
        ]
    }

    fn report_order() -> Vec<StepId> {
        vec![
            StepId::Consolidate,
            StepId::VerifyConsolidated,
            StepId::Charts,
            StepId::ReportHtml,
            StepId::ReportCheck,
        ]
    }

    #[test]
    fn remeasure_runs_every_step_in_the_documented_order() {
        assert_eq!(kinds(&plan(&remeasure())), full_order());
    }

    #[test]
    fn report_only_runs_only_the_derived_chain() {
        assert_eq!(kinds(&plan(&report_only())), report_order());
    }

    #[test]
    fn report_only_reuses_measurements_without_naming_a_skip() {
        let steps = plan(&report_only());
        assert_eq!(steps.last().unwrap().id, StepId::ReportCheck);
        for step in &steps {
            assert!(step.skip.is_none(), "unexpected skip on {:?}", step.id);
        }
    }

    #[test]
    fn last_step_is_the_report_self_check_in_both_modes() {
        assert_eq!(plan(&remeasure()).last().unwrap().id, StepId::ReportCheck);
        assert_eq!(plan(&report_only()).last().unwrap().id, StepId::ReportCheck);
    }

    #[test]
    fn the_mode_is_recorded_in_the_consolidate_invocation() {
        for mode in [ReproductionMode::ReportOnly, ReproductionMode::Remeasure] {
            let steps = plan(&ReproduceOpts {
                mode,
                ..remeasure()
            });
            assert_eq!(
                value_of(find(&steps, StepId::Consolidate), "--reproduction-mode"),
                mode.as_str()
            );
        }
    }

    #[test]
    fn resolve_mode_defaults_to_report_only() {
        assert_eq!(
            resolve_mode(false, false, false).unwrap(),
            ReproductionMode::ReportOnly
        );
    }

    #[test]
    fn resolve_mode_honours_the_explicit_flags() {
        assert_eq!(
            resolve_mode(true, false, false).unwrap(),
            ReproductionMode::ReportOnly
        );
        assert_eq!(
            resolve_mode(false, true, false).unwrap(),
            ReproductionMode::Remeasure
        );
        assert_eq!(
            resolve_mode(false, false, true).unwrap(),
            ReproductionMode::Remeasure
        );
    }

    #[test]
    fn the_hidden_bench_flag_is_an_alias_for_remeasure() {
        assert_eq!(
            resolve_mode(false, false, true).unwrap(),
            ReproductionMode::Remeasure
        );
    }

    #[test]
    fn asking_for_both_modes_is_a_usage_error() {
        assert!(resolve_mode(true, true, false).is_err());
        assert!(resolve_mode(true, false, true).is_err());
    }

    #[test]
    fn the_banner_names_the_mode() {
        assert!(mode_banner(ReproductionMode::ReportOnly).contains("report-only"));
        assert!(mode_banner(ReproductionMode::ReportOnly).contains("not a re-measurement"));
        assert!(mode_banner(ReproductionMode::Remeasure).contains("remeasure"));
        assert!(mode_banner(ReproductionMode::Remeasure).contains("re-running"));
    }

    #[test]
    fn quick_restricts_measurements_to_small_and_medium() {
        let opts = ReproduceOpts {
            quick: true,
            ..remeasure()
        };
        let steps = plan(&opts);
        for id in [StepId::WireReport, StepId::MemoryReport, StepId::BenchAll] {
            let step = find(&steps, id);
            assert_eq!(value_of(step, "--sizes"), SIZES_QUICK);
            assert!(!step.argv.iter().any(|a| a == "large"));
        }
    }

    #[test]
    fn full_scope_uses_the_three_blueprint_sizes() {
        let steps = plan(&remeasure());
        for id in [StepId::WireReport, StepId::MemoryReport, StepId::BenchAll] {
            assert_eq!(value_of(find(&steps, id), "--sizes"), SIZES_FULL);
        }
    }

    #[test]
    fn report_only_ignores_quick_and_docker_gates() {
        let steps = plan(&ReproduceOpts {
            quick: true,
            skip_dns: true,
            docker_available: false,
            ..report_only()
        });
        assert_eq!(kinds(&steps), report_order());
    }

    #[test]
    fn skip_dns_flag_marks_dns_demo_skipped_with_fixture_reason() {
        let steps = plan(&ReproduceOpts {
            skip_dns: true,
            ..remeasure()
        });
        let dns = find(&steps, StepId::DnsDemo);
        assert_eq!(dns.skip, Some(SkipReason::SkipDnsFlag));
        assert!(dns
            .skip
            .unwrap()
            .summary()
            .contains("using recorded fixture"));
    }

    #[test]
    fn docker_unavailable_marks_dns_demo_skipped() {
        let steps = plan(&ReproduceOpts {
            docker_available: false,
            ..remeasure()
        });
        let dns = find(&steps, StepId::DnsDemo);
        assert_eq!(dns.skip, Some(SkipReason::DockerUnavailable));
        assert!(dns.skip.unwrap().summary().contains("Docker unavailable"));
    }

    #[test]
    fn dns_demo_records_when_allowed() {
        let steps = plan(&remeasure());
        let dns = find(&steps, StepId::DnsDemo);
        assert_eq!(dns.skip, None);
        assert_eq!(value_of(dns, "--record"), DNS_LIVE);
    }

    #[test]
    fn ladder_verify_runs_immediately_before_dns_and_before_consolidate() {
        let steps = plan(&remeasure());
        let order = kinds(&steps);
        let ladder = order
            .iter()
            .position(|&id| id == StepId::LadderVerify)
            .expect("ladder-verify present");
        let dns = order
            .iter()
            .position(|&id| id == StepId::DnsDemo)
            .expect("dns-demo present");
        let consolidate = order
            .iter()
            .position(|&id| id == StepId::Consolidate)
            .expect("consolidate present");
        assert_eq!(dns, ladder + 1, "ladder-verify precedes dns-demo");
        assert!(ladder < consolidate, "ladder-verify precedes consolidate");
    }

    #[test]
    fn ladder_verify_writes_the_canonical_artifact_path() {
        let steps = plan(&remeasure());
        let ladder = find(&steps, StepId::LadderVerify);
        assert_eq!(value_of(ladder, "--out"), LADDER_VERIFICATION);
        assert_eq!(ladder.skip, None);
    }

    #[test]
    fn full_ladder_verify_omits_sizes_for_the_default_sweep() {
        let steps = plan(&remeasure());
        let ladder = find(&steps, StepId::LadderVerify);
        assert!(
            !has_flag(ladder, "--sizes"),
            "full run uses the command's default 17-size sweep"
        );
    }

    #[test]
    fn quick_ladder_verify_uses_a_short_size_list() {
        let steps = plan(&ReproduceOpts {
            quick: true,
            ..remeasure()
        });
        let ladder = find(&steps, StepId::LadderVerify);
        assert_eq!(value_of(ladder, "--sizes"), LADDER_SIZES_QUICK);
        assert!(
            value_of(ladder, "--sizes").split(',').count() < 17,
            "quick sweep must be shorter than the default 17-size sweep"
        );
    }

    #[test]
    fn force_is_forwarded_to_consolidate() {
        for opts in [
            ReproduceOpts {
                force: true,
                ..remeasure()
            },
            ReproduceOpts {
                force: true,
                ..report_only()
            },
        ] {
            assert!(has_flag(find(&plan(&opts), StepId::Consolidate), "--force"));
        }
    }

    #[test]
    fn absent_force_is_not_forwarded() {
        let steps = plan(&remeasure());
        assert!(!has_flag(find(&steps, StepId::Consolidate), "--force"));
    }

    #[test]
    fn consolidate_receives_the_measurement_outputs() {
        let steps = plan(&remeasure());
        let consolidate = find(&steps, StepId::Consolidate);
        assert_eq!(value_of(consolidate, "--wire-report"), WIRE_REPORT);
        assert_eq!(value_of(consolidate, "--memory-report"), MEMORY_REPORT);
        assert_eq!(
            value_of(consolidate, "--ladder-verification"),
            LADDER_VERIFICATION
        );
        assert_eq!(value_of(consolidate, "--output"), RESULTS);
    }

    #[test]
    fn verify_consolidated_receives_the_memory_report_and_results() {
        let steps = plan(&remeasure());
        let verify = find(&steps, StepId::VerifyConsolidated);
        assert_eq!(value_of(verify, "--memory-report"), MEMORY_REPORT);
        assert_eq!(value_of(verify, "--results"), RESULTS);
        let order = kinds(&steps);
        let after = order
            .iter()
            .position(|&id| id == StepId::Consolidate)
            .unwrap();
        let at = order
            .iter()
            .position(|&id| id == StepId::VerifyConsolidated)
            .unwrap();
        assert_eq!(at, after + 1, "verify-consolidated follows consolidate");
    }

    #[test]
    fn wire_and_memory_write_to_the_canonical_paths() {
        let steps = plan(&remeasure());
        assert_eq!(
            value_of(find(&steps, StepId::WireReport), "--out"),
            WIRE_REPORT
        );
        assert_eq!(
            value_of(find(&steps, StepId::MemoryReport), "--out"),
            MEMORY_REPORT
        );
    }

    #[test]
    fn report_check_validates_the_rendered_html() {
        for steps in [plan(&remeasure()), plan(&report_only())] {
            let check = find(&steps, StepId::ReportCheck);
            assert_eq!(value_of(check, "--out"), REPORT_HTML);
            assert_eq!(value_of(check, "--results"), RESULTS);
            assert!(has_flag(check, "--check"));
            assert_eq!(value_of(check, "--format"), "html");
        }
    }

    #[test]
    fn every_remeasure_skip_and_force_combination_keeps_the_canonical_order() {
        let expected = full_order();
        for &quick in &[false, true] {
            for &skip_dns in &[false, true] {
                for &force in &[false, true] {
                    for &docker_available in &[false, true] {
                        let steps = plan(&ReproduceOpts {
                            quick,
                            skip_dns,
                            force,
                            docker_available,
                            mode: ReproductionMode::Remeasure,
                        });
                        assert_eq!(kinds(&steps), expected, "order is fixed");
                        assert_eq!(steps.last().unwrap().id, StepId::ReportCheck);
                        // Only dns-demo may be skipped in a remeasure; the
                        // benchmark suite always re-runs.
                        for step in &steps {
                            let skippable = step.id == StepId::DnsDemo;
                            assert!(
                                skippable || step.skip.is_none(),
                                "unexpected skip on {:?}",
                                step.id
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn skip_reasons_render_distinct_summary_text() {
        assert!(SkipReason::SkipDnsFlag.summary().contains("--skip-dns"));
        assert!(SkipReason::DockerUnavailable
            .summary()
            .contains("Docker unavailable"));
    }

    #[test]
    fn step_labels_are_unique_and_nonempty() {
        let steps = plan(&remeasure());
        let mut labels: Vec<&str> = steps.iter().map(|s| s.id.label()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count);
        assert!(labels.iter().all(|l| !l.is_empty()));
    }

    #[test]
    fn summary_table_lists_steps_in_order_and_dashes_skips() {
        let results = vec![
            StepResult {
                label: "pin-env",
                status: "ok".to_string(),
                seconds: Some(0.4),
            },
            StepResult {
                label: "dns-demo",
                status: SkipReason::SkipDnsFlag.summary().to_string(),
                seconds: None,
            },
        ];
        let lines = summary_lines(&results);
        assert_eq!(lines.len(), 4); // header, rule, two rows
        assert!(lines[0].starts_with("step"));
        assert!(lines[2].starts_with("pin-env"));
        assert!(lines[2].contains("0.4"));
        assert!(lines[3].contains("—"));
    }

    #[test]
    fn seconds_formatter_uses_one_decimal_and_dash_for_skips() {
        assert_eq!(format_seconds(Some(1.239)), "1.2");
        assert_eq!(format_seconds(Some(0.0)), "0.0");
        assert_eq!(format_seconds(None), "—");
    }
}
