use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;

use climb_report::charts::generate_charts;
use climb_report::html::{check_report, relative_href, render_html};
use climb_report::{parse_group_id, render_text_summary, scan_criterion, summarize, ClimbResults};

mod charts;
mod check_pins;
mod commit_graph;
mod consolidate;
mod deck_delta;
mod deck_facts;
mod dist_tui;
mod dns_demo;
mod docs_check;
mod groups;
mod ladder_verify;
mod memory;
mod package;
mod pin_env;
mod readme_badges;
mod reproduce;
mod source_hygiene;
mod stability;
mod verify_consolidated;
mod wire;
use charts::{cmd_charts, ChartsArgs};
use check_pins::CheckPinsArgs;
use commit_graph::{cmd_commit_graph, CommitGraphArgs};
use consolidate::{cmd_consolidate, ConsolidateArgs};
use deck_delta::{cmd_deck_delta, DeckDeltaArgs};
use deck_facts::{cmd_deck_facts, DeckFactsArgs};
use dist_tui::{cmd_dist_tui, DistTuiArgs};
use dns_demo::{cmd_dns_demo, DnsDemoArgs};
use docs_check::{cmd_docs_check, DocsCheckArgs};
use groups::{BenchTarget, DatasetSize, GROUPS};
use ladder_verify::{cmd_ladder_verify, LadderVerifyArgs};
use memory::{cmd_memory_report, MemoryReportArgs};
use package::{cmd_package, PackageArgs};
use pin_env::cmd_pin_env;
use readme_badges::{cmd_readme_badges, ReadmeBadgesArgs};
use reproduce::{cmd_reproduce, ReproduceArgs};
use source_hygiene::SourceHygieneArgs;
use stability::{cmd_stability, StabilityArgs};
use verify_consolidated::{cmd_verify_consolidated, VerifyConsolidatedArgs};
use wire::{cmd_wire_report, WireReportArgs};

/// The logical CPU to pin benchmarks to.
///
/// Host: Intel i3-1115G4, 2 physical cores / 4 logical CPUs.
/// Physical core 0 = {cpu0, cpu2}, physical core 1 = {cpu1, cpu3}.
/// Interrupt totals measured on this host:
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
    /// Run the signing and verification benchmark suites.
    BenchAll(BenchAllArgs),
    /// Print a plain-text summary of Criterion's latest results, or render
    /// the self-contained HTML report with `--format html`.
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
    /// Measure the real MTL ladder shape across a range of leaf counts.
    ///
    /// Signs a deterministic dataset of exactly N messages per size, parses the
    /// signed ladder and condensed-signature bytes back into their structural
    /// fields, and records the result as a `LadderVerification` document. This
    /// is the measured evidence the closed-form ladder model is checked against.
    LadderVerify(LadderVerifyArgs),
    /// Bring up the live MTL DNS pipeline, sign a zone, issue scripted
    /// queries, capture raw wire bytes, and tear down.
    ///
    /// Demonstrates the ~37x bandwidth collapse (full signed ladder vs
    /// condensed Merkle path) using NSD-MTL, Unbound-MTL, and the
    /// ldns zone signer — all inside internal-only Docker bridge networks.
    /// Captures are written under target/dns-demo/.
    DnsDemo(DnsDemoArgs),
    /// Consolidate all existing metric outputs into a single coherent artifact.
    Consolidate(ConsolidateArgs),
    /// Verify the consolidated results carry one memory row per size described
    /// by the memory report, so a silently-empty memory section fails the chain.
    VerifyConsolidated(VerifyConsolidatedArgs),
    /// Render the seven SVG report charts from consolidated results.
    ///
    /// Reads `target/climb-results.json` and writes
    /// `target/climb-report/charts/*.svg`.
    Charts(ChartsArgs),
    /// Render the deck quote sheet: every number a slide deck or README may
    /// quote, with its evidence class, baseline, JSON pointer and source file.
    ///
    /// Reads the recorded artifacts only. Two artifacts that disagree about the
    /// same number are reported in the footer and fail the command.
    DeckFacts(DeckFactsArgs),
    /// Diff a plain-text deck dump against the quote sheet.
    DeckDelta(DeckDeltaArgs),
    /// Check the documentation against the real workspace and the quote sheet.
    ///
    /// Fails when a Mermaid node id in README.md or docs/*.md looks like a
    /// crate or binary name but is not one, when a numeric result (a number
    /// plus a result unit) is absent from docs/deck-facts.md, or when a
    /// headline-table row's value, unit, evidence class or baseline does not
    /// match a quote-sheet row.
    DocsCheck(DocsCheckArgs),
    /// Regenerate the README badge block from one configuration file.
    ///
    /// Rewrites the block between the `<!-- badges:begin -->` and
    /// `<!-- badges:end -->` markers with two centred rows of anchored badge
    /// images: an identity row (hackathon, problem statement, track) and a
    /// proof row (language, introduction video, claims count). The claims
    /// count is read from the claims registry of the consolidated results
    /// artifact, never typed in. `--check` compares the committed block with a
    /// fresh generation instead of writing.
    ReadmeBadges(ReadmeBadgesArgs),
    /// Refresh the commit graph: one JSON document and four SVG cards.
    ///
    /// Reads the history reachable from `--as-of` (default HEAD) with a pinned
    /// log format and writes `docs/media/commit-graph.json` and its wide,
    /// narrow and compact cards. Dates are bucketed in the configured
    /// fixed-offset zone, so rerunning the command for the same commit produces
    /// identical bytes. `--auto` refreshes only when the public tip has moved
    /// and exits 10 when it staged a change.
    CommitGraph(CommitGraphArgs),
    /// Scan the shipped source tree for process-history references.
    ///
    /// A comment that cites a planning/process document by name, or the phase
    /// numbering those documents use, is a dangling reference for every
    /// downstream reader of the mirror. Comments must describe behaviour.
    SourceHygiene(SourceHygieneArgs),
    /// Check that the pinned-dependency record agrees across its three sources.
    ///
    /// Fails when the commits named in the toolchain Dockerfile comment,
    /// `environment.json` and `git submodule status` disagree.
    CheckPins(CheckPinsArgs),
    /// Bundle the recorded artifacts into one release tarball under `dist/`.
    ///
    /// Copies the HTML report, every report chart, the consolidated results,
    /// the quote sheet, the environment fingerprint and the recorded fixtures
    /// into `dist/climb-<short-sha>.tar.gz` beside a `SHA256SUMS` manifest and
    /// a `PROVENANCE.txt` naming the commit, submodule SHAs, toolchain, host
    /// fingerprint and date.  Fails when an input is missing or the report
    /// disagrees with the results it ships with.
    Package(PackageArgs),
    /// Build and package the viewer-only dashboard binary.
    ///
    /// Builds `climb-tui` in release mode with no default features (no native
    /// crypto stack), prefers the static musl target and falls back to gnu
    /// with a note, strips the binary, and writes
    /// `dist/climb-tui-<short-sha>-<target>.tar.gz` beside a `SHA256SUMS`
    /// manifest and a `PROVENANCE.txt`. Unless `--skip-smoke` is given, the
    /// packaged binary is opened in `debian:stable-slim` under a
    /// pseudo-terminal with `--tour` and must exit 0 after two Esc presses.
    DistTui(DistTuiArgs),
    /// Run the whole artifact chain in order, from one command.
    ///
    /// Pins the environment, measures wire sizes and memory, optionally
    /// re-runs the benchmarks, captures (or falls back to) the DNS pipeline,
    /// then consolidates, charts, renders and self-checks the report — so the
    /// artifacts always agree.  Use `--quick` for Small/Medium only.
    Reproduce(ReproduceArgs),
    /// Run stability check (repeat signing/verify benches under pinned governor).
    Stability(StabilityArgs),
}

#[derive(Args)]
struct BenchAllArgs {
    /// Dataset sizes to include (comma-separated, or `all`).
    #[arg(long, value_delimiter = ',', default_value = "all")]
    sizes: Vec<SizeArg>,

    /// Save results under this Criterion baseline name.
    #[arg(long)]
    baseline: Option<String>,

    /// Also run the benchmark groups that are skipped by default because a
    /// single iteration is infeasibly slow on the reference hardware.
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

    /// Output format: Criterion text tables or the self-contained HTML report.
    #[arg(long, value_enum, default_value = "text")]
    format: ReportFormat,

    /// HTML report output path (`--format html`).
    #[arg(long, default_value = "target/climb-report/report.html")]
    out: PathBuf,

    /// Consolidated results document to validate against (`--format html`).
    #[arg(long, default_value = "target/climb-results.json")]
    results: PathBuf,

    /// Directory holding the seven SVG charts (`--format html`).
    #[arg(long, default_value = "target/climb-report/charts")]
    charts: PathBuf,

    /// Validate an existing report against the results document instead of writing one.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    /// Criterion-derived plain-text tables.
    Text,
    /// Self-contained HTML with embedded charts and claims registry.
    Html,
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
        Commands::LadderVerify(args) => cmd_ladder_verify(args),
        Commands::DnsDemo(args) => cmd_dns_demo(args),
        Commands::Consolidate(args) => cmd_consolidate(args),
        Commands::VerifyConsolidated(args) => cmd_verify_consolidated(args),
        Commands::Charts(args) => cmd_charts(args),
        Commands::DeckFacts(args) => cmd_deck_facts(args),
        Commands::DeckDelta(args) => cmd_deck_delta(args),
        Commands::DocsCheck(args) => cmd_docs_check(args),
        Commands::ReadmeBadges(args) => cmd_readme_badges(args),
        Commands::CommitGraph(args) => cmd_commit_graph(args),
        Commands::SourceHygiene(args) => cmd_source_hygiene(args),
        Commands::CheckPins(args) => check_pins::run(&args.root),
        Commands::Package(args) => cmd_package(args),
        Commands::DistTui(args) => cmd_dist_tui(args),
        Commands::Reproduce(args) => cmd_reproduce(args),
        Commands::Stability(args) => cmd_stability(args),
    }
}

fn cmd_bench_all(args: BenchAllArgs) -> Result<()> {
    let sizes = selected_sizes(&args);

    let mut signing = Vec::new();
    let mut verifying = Vec::new();
    let mut classical = Vec::new();
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
            BenchTarget::Classical => classical.push(group.name),
            BenchTarget::Primitives => {}
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
    run_bench_target("classical", &classical, args.baseline.as_deref(), pin_cpu)?;

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
    match args.format {
        ReportFormat::Text => {
            if args.check {
                bail!("--check is only supported with --format html");
            }
            cmd_report_text(args)
        }
        ReportFormat::Html if args.check => cmd_report_check(&args),
        ReportFormat::Html => cmd_report_html(&args),
    }
}

fn load_results(path: &Path) -> Result<ClimbResults> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("failed to parse {}", path.display()))
}

fn cmd_report_html(args: &ReportArgs) -> Result<()> {
    let results = load_results(&args.results)?;

    let written =
        generate_charts(&results, &args.charts).with_context(|| "failed to render charts")?;
    for path in &written {
        println!("wrote {}", path.display());
    }

    let out_dir = args.out.parent().unwrap_or_else(|| Path::new("."));
    let href = relative_href(out_dir, &args.results);
    let html = render_html(&results, &args.charts, &href)
        .with_context(|| "failed to render HTML report")?;

    let failures = check_report(&html, &results);
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("self-check: {failure}");
        }
        bail!(
            "freshly rendered report failed self-check ({} problem(s))",
            failures.len()
        );
    }

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&args.out, &html)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!("wrote {} (self-check clean)", args.out.display());
    Ok(())
}

fn cmd_report_check(args: &ReportArgs) -> Result<()> {
    let results = load_results(&args.results)?;

    let report = climb_report::check(&results, &args.out)?;
    let failures = report.messages();
    if failures.is_empty() {
        eprintln!("{}: report and results agree", args.out.display());
        return Ok(());
    }
    for failure in &failures {
        eprintln!("check failed: {failure}");
    }
    bail!(
        "{}: {} check failure(s)",
        args.out.display(),
        failures.len()
    );
}

fn cmd_source_hygiene(args: SourceHygieneArgs) -> Result<()> {
    source_hygiene::run(&args.root)
}

fn cmd_report_text(args: ReportArgs) -> Result<()> {
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
    use crate::commit_graph::HistorySource;

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

    #[test]
    fn deck_facts_defaults_to_the_recorded_artifact_paths() {
        let cli = Cli::try_parse_from(["xtask", "deck-facts"]).expect("deck-facts parses");
        match cli.command {
            Commands::DeckFacts(args) => {
                assert_eq!(args.results, PathBuf::from("target/climb-results.json"));
                assert_eq!(args.out, PathBuf::from("docs/deck-facts.md"));
            }
            _ => panic!("expected the deck-facts subcommand"),
        }
    }

    #[test]
    fn docs_check_defaults_to_the_repository_paths() {
        let cli = Cli::try_parse_from(["xtask", "docs-check"]).expect("docs-check parses");
        match cli.command {
            Commands::DocsCheck(args) => {
                assert_eq!(args.root, PathBuf::from("."));
                assert_eq!(args.quote_sheet, PathBuf::from("docs/deck-facts.md"));
            }
            _ => panic!("expected the docs-check subcommand"),
        }
    }

    #[test]
    fn commit_graph_defaults_to_the_media_directory_and_off_check() {
        let cli = Cli::try_parse_from(["xtask", "commit-graph"]).expect("commit-graph parses");
        match cli.command {
            Commands::CommitGraph(args) => {
                assert_eq!(args.out_dir, PathBuf::from("docs/media"));
                assert!(!args.check);
                assert!(!args.auto);
                assert!(args.as_of.is_none());
                assert_eq!(args.source, HistorySource::Auto);
            }
            _ => panic!("expected the commit-graph subcommand"),
        }
    }

    #[test]
    fn commit_graph_auto_flag_parses_and_conflicts_with_check() {
        let cli = Cli::try_parse_from(["xtask", "commit-graph", "--auto"])
            .expect("commit-graph --auto parses");
        match cli.command {
            Commands::CommitGraph(args) => assert!(args.auto),
            _ => panic!("expected the commit-graph subcommand"),
        }
        assert!(
            Cli::try_parse_from(["xtask", "commit-graph", "--auto", "--check"]).is_err(),
            "--auto and --check are mutually exclusive"
        );
    }

    #[test]
    fn commit_graph_source_flag_parses() {
        let cli = Cli::try_parse_from(["xtask", "commit-graph", "--source", "public"])
            .expect("commit-graph --source public parses");
        match cli.command {
            Commands::CommitGraph(args) => assert_eq!(args.source, HistorySource::Public),
            _ => panic!("expected the commit-graph subcommand"),
        }
        let cli = Cli::try_parse_from(["xtask", "commit-graph", "--source", "local"])
            .expect("commit-graph --source local parses");
        match cli.command {
            Commands::CommitGraph(args) => assert_eq!(args.source, HistorySource::Local),
            _ => panic!("expected the commit-graph subcommand"),
        }
    }

    #[test]
    fn commit_graph_check_flag_parses() {
        let cli = Cli::try_parse_from(["xtask", "commit-graph", "--check"])
            .expect("commit-graph --check parses");
        match cli.command {
            Commands::CommitGraph(args) => assert!(args.check),
            _ => panic!("expected the commit-graph subcommand"),
        }
    }

    #[test]
    fn package_defaults_to_the_recorded_artifact_paths() {
        let cli = Cli::try_parse_from(["xtask", "package"]).expect("package parses");
        match cli.command {
            Commands::Package(args) => {
                assert_eq!(args.out, PathBuf::from("dist"));
                assert_eq!(args.results, PathBuf::from("target/climb-results.json"));
                assert_eq!(
                    args.report,
                    PathBuf::from("target/climb-report/report.html")
                );
                assert_eq!(args.charts, PathBuf::from("target/climb-report/charts"));
                assert!(!args.skip_checks);
            }
            _ => panic!("expected the package subcommand"),
        }
    }

    #[test]
    fn dist_tui_defaults_to_dist_and_runs_the_smoke_test() {
        let cli = Cli::try_parse_from(["xtask", "dist-tui"]).expect("dist-tui parses");
        match cli.command {
            Commands::DistTui(args) => {
                assert_eq!(args.out, PathBuf::from("dist"));
                assert!(args.target.is_none());
                assert!(!args.skip_smoke);
            }
            _ => panic!("expected the dist-tui subcommand"),
        }
    }

    #[test]
    fn dist_tui_accepts_a_target_and_skip_smoke() {
        let cli = Cli::try_parse_from([
            "xtask",
            "dist-tui",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--skip-smoke",
        ])
        .expect("dist-tui flags parse");
        match cli.command {
            Commands::DistTui(args) => {
                assert_eq!(args.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
                assert!(args.skip_smoke);
            }
            _ => panic!("expected the dist-tui subcommand"),
        }
    }
}
