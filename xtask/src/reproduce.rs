//! The `reproduce` subcommand: run the whole artifact chain, in order, from
//! a single command.
//!
//! The individual subcommands are each correct in isolation, but the
//! consolidated results only agree with the rendered report when they are run
//! in one specific order — pin the environment, measure wire sizes, measure
//! memory, (optionally) re-run the benchmarks, capture the DNS pipeline, then
//! consolidate, chart, render and self-check.  Run them by hand and it is easy
//! to skip a measurement or consolidate without its inputs, which leaves the
//! report claiming numbers the results no longer carry (or vice versa).
//!
//! ```text
//! cargo xtask reproduce
//! cargo xtask reproduce --quick --skip-dns
//! cargo xtask reproduce --bench --force
//! ```
//!
//! The command is planned then executed.  [`plan`] is a pure function from
//! [`ReproduceOpts`] to an ordered list of [`Step`]s: it spawns no process and
//! performs no I/O, so every ordering and skip/force combination is unit
//! testable.  Execution walks the plan, times each step, and stops at the first
//! failure with that step named.  The last step is always a report self-check,
//! so a successful exit means the artifacts agree.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::process::{Command, Stdio};
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

/// Blueprint experiment sizes, in canonical order.
const SIZES_FULL: &str = "small,medium,large";
/// Scope for `--quick`: the two sizes that finish in seconds.
const SIZES_QUICK: &str = "small,medium";

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// Arguments for `cargo xtask reproduce`.
#[derive(Args)]
pub(crate) struct ReproduceArgs {
    /// Limit every measurement to Small/Medium and reuse the existing
    /// Criterion output for those sizes.  Skips nothing else.
    #[arg(long)]
    quick: bool,

    /// Re-run the Criterion benchmark suite instead of reusing the last
    /// output.  Off by default: the suite is slow, and the existing output is
    /// the common case when re-rendering artifacts.
    #[arg(long)]
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
    DnsDemo,
    Consolidate,
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
            StepId::DnsDemo => "dns-demo",
            StepId::Consolidate => "consolidate",
            StepId::Charts => "charts",
            StepId::ReportHtml => "report --format html",
            StepId::ReportCheck => "report --check",
        }
    }
}

/// Why a planned step will not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// `--bench` was absent; the last Criterion output is reused as-is.
    ReuseCriterion,
    /// `--skip-dns` was passed.
    SkipDnsFlag,
    /// The Docker daemon is unreachable.
    DockerUnavailable,
}

impl SkipReason {
    /// The status text shown in the summary table for this skip.
    pub(crate) fn summary(self) -> &'static str {
        match self {
            SkipReason::ReuseCriterion => "skipped: reusing existing Criterion output",
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
    pub bench: bool,
    pub skip_dns: bool,
    pub force: bool,
    pub docker_available: bool,
}

/// Build the ordered step list for `opts`.  Pure: no I/O, no subprocesses.
pub(crate) fn plan(opts: &ReproduceOpts) -> Vec<Step> {
    let sizes = if opts.quick { SIZES_QUICK } else { SIZES_FULL };
    let mut steps = Vec::with_capacity(9);

    steps.push(Step::run(StepId::PinEnv, &["pin-env"]));

    steps.push(Step::run(
        StepId::WireReport,
        &["wire-report", "--sizes", sizes, "--out", WIRE_REPORT],
    ));

    steps.push(Step::run(
        StepId::MemoryReport,
        &["memory-report", "--sizes", sizes, "--out", MEMORY_REPORT],
    ));

    // Benchmarks are the slow step and are opt-in; without `--bench` the last
    // Criterion output is reused and the skip is stated explicitly.
    let bench = Step::run(StepId::BenchAll, &["bench-all", "--sizes", sizes]);
    steps.push(if opts.bench {
        bench
    } else {
        bench.skipped(SkipReason::ReuseCriterion)
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

    let mut consolidate = vec![
        "consolidate".to_string(),
        "--wire-report".to_string(),
        WIRE_REPORT.to_string(),
        "--memory-report".to_string(),
        MEMORY_REPORT.to_string(),
        "--output".to_string(),
        RESULTS.to_string(),
    ];
    if opts.force {
        consolidate.push("--force".to_string());
    }
    steps.push(Step {
        id: StepId::Consolidate,
        argv: consolidate,
        skip: None,
    });

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
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run one planned step as a child `xtask` process.
fn run_step(exe: &std::path::Path, step: &Step) -> Result<()> {
    let label = step.id.label();
    let status = Command::new(exe)
        .args(&step.argv)
        .status()
        .with_context(|| format!("failed to spawn '{label}'"))?;
    if !status.success() {
        bail!("'{label}' exited with {status}");
    }
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
    let opts = ReproduceOpts {
        quick: args.quick,
        bench: args.bench,
        skip_dns: args.skip_dns,
        force: args.force,
        // `--skip-dns` already forces the fixture path; only probe Docker when
        // it could actually change the outcome.
        docker_available: !args.skip_dns && docker_available(),
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> ReproduceOpts {
        ReproduceOpts {
            quick: false,
            bench: true,
            skip_dns: false,
            force: false,
            docker_available: true,
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

    #[test]
    fn plan_runs_every_step_in_the_documented_order() {
        let steps = plan(&base());
        assert_eq!(
            kinds(&steps),
            vec![
                StepId::PinEnv,
                StepId::WireReport,
                StepId::MemoryReport,
                StepId::BenchAll,
                StepId::DnsDemo,
                StepId::Consolidate,
                StepId::Charts,
                StepId::ReportHtml,
                StepId::ReportCheck,
            ]
        );
    }

    #[test]
    fn last_step_is_the_report_self_check() {
        let steps = plan(&base());
        assert_eq!(steps.last().unwrap().id, StepId::ReportCheck);
    }

    #[test]
    fn quick_restricts_measurements_to_small_and_medium() {
        let opts = ReproduceOpts {
            quick: true,
            ..base()
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
        let steps = plan(&base());
        for id in [StepId::WireReport, StepId::MemoryReport, StepId::BenchAll] {
            assert_eq!(value_of(find(&steps, id), "--sizes"), SIZES_FULL);
        }
    }

    #[test]
    fn bench_flag_controls_whether_bench_all_runs() {
        let off = plan(&ReproduceOpts {
            bench: false,
            ..base()
        });
        assert_eq!(
            find(&off, StepId::BenchAll).skip,
            Some(SkipReason::ReuseCriterion)
        );

        let on = plan(&base());
        assert_eq!(find(&on, StepId::BenchAll).skip, None);
    }

    #[test]
    fn skip_dns_flag_marks_dns_demo_skipped_with_fixture_reason() {
        let steps = plan(&ReproduceOpts {
            skip_dns: true,
            ..base()
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
            ..base()
        });
        let dns = find(&steps, StepId::DnsDemo);
        assert_eq!(dns.skip, Some(SkipReason::DockerUnavailable));
        assert!(dns
            .skip
            .unwrap()
            .summary()
            .contains("using recorded fixture"));
    }

    #[test]
    fn dns_demo_records_when_allowed() {
        let steps = plan(&base());
        let dns = find(&steps, StepId::DnsDemo);
        assert_eq!(dns.skip, None);
        assert_eq!(value_of(dns, "--record"), DNS_LIVE);
    }

    #[test]
    fn force_is_forwarded_to_consolidate() {
        let steps = plan(&ReproduceOpts {
            force: true,
            ..base()
        });
        assert!(has_flag(find(&steps, StepId::Consolidate), "--force"));
    }

    #[test]
    fn absent_force_is_not_forwarded() {
        let steps = plan(&base());
        assert!(!has_flag(find(&steps, StepId::Consolidate), "--force"));
    }

    #[test]
    fn consolidate_receives_the_measurement_outputs() {
        let steps = plan(&base());
        let consolidate = find(&steps, StepId::Consolidate);
        assert_eq!(value_of(consolidate, "--wire-report"), WIRE_REPORT);
        assert_eq!(value_of(consolidate, "--memory-report"), MEMORY_REPORT);
        assert_eq!(value_of(consolidate, "--output"), RESULTS);
    }

    #[test]
    fn wire_and_memory_write_to_the_canonical_paths() {
        let steps = plan(&base());
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
        let steps = plan(&base());
        let check = find(&steps, StepId::ReportCheck);
        assert_eq!(value_of(check, "--out"), REPORT_HTML);
        assert_eq!(value_of(check, "--results"), RESULTS);
        assert!(has_flag(check, "--check"));
        assert_eq!(value_of(check, "--format"), "html");
    }

    #[test]
    fn every_skip_and_force_combination_keeps_the_canonical_order() {
        let expected = vec![
            StepId::PinEnv,
            StepId::WireReport,
            StepId::MemoryReport,
            StepId::BenchAll,
            StepId::DnsDemo,
            StepId::Consolidate,
            StepId::Charts,
            StepId::ReportHtml,
            StepId::ReportCheck,
        ];
        for &quick in &[false, true] {
            for &bench in &[false, true] {
                for &skip_dns in &[false, true] {
                    for &force in &[false, true] {
                        for &docker_available in &[false, true] {
                            let steps = plan(&ReproduceOpts {
                                quick,
                                bench,
                                skip_dns,
                                force,
                                docker_available,
                            });
                            assert_eq!(kinds(&steps), expected, "order is fixed");
                            assert_eq!(steps.last().unwrap().id, StepId::ReportCheck);
                            // Only bench-all and dns-demo may be skipped.
                            for step in &steps {
                                let skippable =
                                    matches!(step.id, StepId::BenchAll | StepId::DnsDemo);
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
    }

    #[test]
    fn skip_reasons_render_distinct_summary_text() {
        assert!(SkipReason::ReuseCriterion.summary().contains("Criterion"));
        assert!(SkipReason::SkipDnsFlag.summary().contains("--skip-dns"));
        assert!(SkipReason::DockerUnavailable
            .summary()
            .contains("Docker unavailable"));
    }

    #[test]
    fn step_labels_are_unique_and_nonempty() {
        let steps = plan(&base());
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
                label: "bench-all",
                status: SkipReason::ReuseCriterion.summary().to_string(),
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
