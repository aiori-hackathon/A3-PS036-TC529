//! `cargo xtask audit-pack`: write a short, deterministic batch-gate audit pack.
//!
//! The pack is a single markdown file (`target/audit-pack.md` by default, at
//! most [`LINE_CAP`] lines) that a reviewer can audit instead of reading long
//! logs. It records gate exit codes and durations, test counts, code-hygiene
//! counts, README/quote-sheet agreement, large modules, open questions, README
//! markers, repository and media sizes, the site/ summary, the link-check
//! summary, the date and a host fingerprint. It never records raw command
//! output and never prints secrets or absolute home paths.
//!
//! With `--reuse` the previous pack's gate results are reused instead of
//! re-running the gates; otherwise every gate runs. Output ordering is fixed
//! and deterministic, so the pack diffs cleanly between runs.

use anyhow::{bail, Context, Result};
use clap::Args;
use regex_lite::Regex;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::check_pins;
use crate::docs_check;
use crate::link_check::{self, LinkCheck};
use crate::proc;
use crate::source_hygiene;

/// Hard cap on the pack's line count.
const LINE_CAP: usize = 160;

/// Default output path, relative to the workspace root.
const DEFAULT_OUT: &str = "target/audit-pack.md";

/// The gate names, in the fixed order they are reported.  This order never
/// changes, so the pack's gate table diffs cleanly between runs.
const GATES: &[&str] = &[
    "build",
    "fmt",
    "clippy",
    "test",
    "docs-check",
    "verify-numbers",
    "source-hygiene",
    "check-pins",
    "viewer build",
    "mermaid lint",
];

/// Markers bracketing managed sections of the README, in report order.
const README_MARKERS: &[(&str, &str, &str)] = &[
    ("logo", "<!-- logo:begin -->", "<!-- logo:end -->"),
    ("badges", "<!-- badges:begin -->", "<!-- badges:end -->"),
    ("live", "<!-- live:begin -->", "<!-- live:end -->"),
    (
        "headline",
        "<!-- headline:begin -->",
        "<!-- headline:end -->",
    ),
    (
        "classical",
        "<!-- classical:begin -->",
        "<!-- classical:end -->",
    ),
    (
        "quickstart",
        "<!-- quickstart:begin -->",
        "<!-- quickstart:end -->",
    ),
    ("footer", "<!-- footer:begin -->", "<!-- footer:end -->"),
    (
        "commit-graph",
        "<!-- commit-graph:begin -->",
        "<!-- commit-graph:end -->",
    ),
    (
        "commit-graph-strip",
        "<!-- commit-graph-strip:begin -->",
        "<!-- commit-graph-strip:end -->",
    ),
];

/// Headings of the markdown tables the pack re-parses when reusing.
const TESTS_SECTION: &str = "## Tests";
const GATES_SECTION: &str = "## Gates";

/// A `#[cfg(test)]` module region is skipped once its opening brace is seen;
/// this is the brace depth at which the region closes.
const TEST_REGION_CLOSE_DEPTH: i64 = 0;

/// The outcome of one gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GateResult {
    pub(crate) name: String,
    /// Exit code, or `None` when the gate could not be run.
    pub(crate) exit: Option<i32>,
    pub(crate) duration: Duration,
    pub(crate) reused: bool,
}

impl GateResult {
    fn passed(&self) -> bool {
        self.exit == Some(0)
    }
}

/// Test totals for one test target (crate or the workspace aggregate).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TestTargetCounts {
    pub(crate) name: String,
    pub(crate) passed: u64,
    pub(crate) failed: u64,
    pub(crate) ignored: u64,
    pub(crate) targets: Option<u64>,
    pub(crate) doctests: Option<u64>,
}

/// One README headline row and whether it matches a quote-sheet row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeadlineComparison {
    pub(crate) line: usize,
    pub(crate) value: String,
    pub(crate) class: String,
    pub(crate) baseline: String,
    /// The matching quote-sheet row's label, when one exists.
    pub(crate) match_label: Option<String>,
}

/// Code-hygiene counts for one crate (non-test code).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CrateHygiene {
    pub(crate) name: String,
    pub(crate) unsafe_blocks: u64,
    pub(crate) unwrap: u64,
    pub(crate) expect: u64,
    pub(crate) panic: u64,
}

/// The site/ summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SiteInfo {
    pub(crate) exists: bool,
    pub(crate) has_last_run: bool,
    pub(crate) build: Option<String>,
    pub(crate) lighthouse: Option<String>,
    pub(crate) wasm_size: Option<String>,
}

/// All inputs to [`build_pack`], gathered from the workspace or fabricated in
/// tests so the pack writer can be unit-tested without running the gates.
pub(crate) struct PackInputs {
    pub(crate) head: String,
    pub(crate) commits: Vec<String>,
    pub(crate) git_dirty: bool,
    pub(crate) tests_workspace: TestTargetCounts,
    pub(crate) tests_per_crate: Vec<TestTargetCounts>,
    pub(crate) gates: Vec<GateResult>,
    pub(crate) hygiene_per_crate: Vec<CrateHygiene>,
    pub(crate) headline: Vec<HeadlineComparison>,
    pub(crate) headline_mismatch: usize,
    pub(crate) big_modules: Vec<(String, u64)>,
    pub(crate) open_questions: Vec<String>,
    pub(crate) markers: Vec<(String, bool)>,
    pub(crate) repo_size: String,
    pub(crate) media_size: String,
    pub(crate) site: SiteInfo,
    pub(crate) link_check: LinkCheck,
    pub(crate) date: String,
    pub(crate) host_fingerprint: String,
}

/// CLI arguments for `cargo xtask audit-pack`.
#[derive(Args)]
pub(crate) struct AuditPackArgs {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Output path (default `target/audit-pack.md`).
    #[arg(long, default_value = DEFAULT_OUT)]
    pub(crate) out: PathBuf,

    /// Reuse the previous pack's gate results instead of re-running the gates.
    #[arg(long)]
    pub(crate) reuse: bool,
}

/// `cargo xtask audit-pack`: write the audit pack and print its path and line
/// count.  The pack is written even when gates fail; the process exits
/// non-zero when any gate fails.
pub(crate) fn cmd_audit_pack(args: AuditPackArgs) -> Result<()> {
    let root = &args.root;
    let out = if args.out.is_absolute() {
        args.out.clone()
    } else {
        root.join(&args.out)
    };

    let (tests, gates) = if args.reuse && out.exists() {
        let previous = fs::read_to_string(&out)
            .with_context(|| format!("failed to read previous pack {}", out.display()))?;
        eprintln!("audit-pack: reusing gate results from previous pack");
        parse_previous_pack(&previous)
    } else {
        if args.reuse {
            eprintln!("audit-pack: no previous pack to reuse; running gates");
        }
        let mut gates = Vec::with_capacity(GATES.len());
        let mut workspace = TestTargetCounts {
            name: "workspace".to_string(),
            ..Default::default()
        };
        let mut per_crate = Vec::new();
        for name in GATES {
            let (gate, ws, pc) = run_gate(name, root)?;
            if let Some(ws) = ws {
                workspace = ws;
            }
            if let Some(pc) = pc {
                per_crate = pc;
            }
            gates.push(gate);
        }
        (
            TestCounts {
                workspace,
                per_crate,
            },
            gates,
        )
    };

    let inputs = gather_inputs(root, &tests, gates)?;
    let content = build_pack(&inputs);
    let content = redact(&content);
    let lines = content.lines().count();

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&out, &content).with_context(|| format!("failed to write {}", out.display()))?;

    let failed: Vec<String> = inputs
        .gates
        .iter()
        .filter(|g| !g.passed())
        .map(|g| g.name.clone())
        .collect();
    println!(
        "wrote {} ({} lines)",
        redact(&out.display().to_string()),
        lines
    );
    if !failed.is_empty() {
        eprintln!("audit-pack: FAIL gate(s): {}", failed.join(", "));
        std::process::exit(1);
    }
    Ok(())
}

/// Test counts gathered from a `cargo test --workspace` run.
struct TestCounts {
    workspace: TestTargetCounts,
    per_crate: Vec<TestTargetCounts>,
}

/// Run one gate by name, in the fixed [`GATES`] order, timing it and
/// recording the exit code without propagating a non-zero status (the pack
/// records the outcome instead).  For the `test` gate it also returns the
/// parsed per-crate test counts.
fn run_gate(
    name: &str,
    root: &Path,
) -> Result<(
    GateResult,
    Option<TestTargetCounts>,
    Option<Vec<TestTargetCounts>>,
)> {
    match name {
        "build" => Ok((
            run_cargo_gate(
                "build",
                root,
                &["build", "--workspace"],
                Duration::from_secs(900),
            ),
            None,
            None,
        )),
        "fmt" => Ok((
            run_cargo_gate(
                "fmt",
                root,
                &["fmt", "--all", "--", "--check"],
                Duration::from_secs(180),
            ),
            None,
            None,
        )),
        "clippy" => Ok((
            run_cargo_gate(
                "clippy",
                root,
                &[
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
                Duration::from_secs(900),
            ),
            None,
            None,
        )),
        "test" => {
            let (gate, workspace, per_crate) = run_test_gate(root)?;
            Ok((gate, Some(workspace), Some(per_crate)))
        }
        "docs-check" => Ok((
            run_silent_gate("docs-check", || {
                docs_check::cmd_docs_check(docs_check::DocsCheckArgs {
                    root: root.to_path_buf(),
                    quote_sheet: PathBuf::from("docs/deck-facts.md"),
                    results: PathBuf::from("docs/sample-output/climb-results.json"),
                    skip_side_checks: true,
                    require_spread: false,
                })
            }),
            None,
            None,
        )),
        "verify-numbers" => Ok((
            run_cargo_gate(
                "verify-numbers",
                root,
                &["xtask", "verify-numbers"],
                Duration::from_secs(300),
            ),
            None,
            None,
        )),
        "source-hygiene" => Ok((
            run_silent_gate("source-hygiene", || source_hygiene::run(root)),
            None,
            None,
        )),
        "check-pins" => Ok((
            run_silent_gate("check-pins", || check_pins::run(root)),
            None,
            None,
        )),
        "viewer build" => Ok((
            run_silent_gate("viewer build", || docs_check::check_viewer_build(root)),
            None,
            None,
        )),
        "mermaid lint" => Ok((run_mermaid_lint_gate(root)?, None, None)),
        _ => bail!("unknown gate: {name}"),
    }
}

/// Run one cargo gate, timing it and recording the exit code without
/// propagating a non-zero status (the pack records the outcome instead).
fn run_cargo_gate(name: &str, root: &Path, args: &[&str], timeout: Duration) -> GateResult {
    let started = Instant::now();
    let mut command = Command::new("cargo");
    command.args(args).current_dir(root);
    let result = proc::run_raw(&mut command, timeout, name);
    let duration = started.elapsed();
    let exit = result.as_ref().ok().and_then(|output| output.status.code());
    eprintln!(
        "audit-pack: gate {name} -> exit {} in {:.2}s",
        exit.map(|c| c.to_string())
            .unwrap_or_else(|| "error".into()),
        duration.as_secs_f64()
    );
    GateResult {
        name: name.to_string(),
        exit,
        duration,
        reused: false,
    }
}

/// Run the test gate and parse its output for per-crate test counts.
fn run_test_gate(root: &Path) -> Result<(GateResult, TestTargetCounts, Vec<TestTargetCounts>)> {
    let started = Instant::now();
    let mut command = Command::new("cargo");
    command
        .args(["test", "--workspace", "--no-fail-fast"])
        .current_dir(root);
    let output = proc::run_raw(&mut command, Duration::from_secs(1800), "test")?;
    let duration = started.elapsed();
    let exit = output.status.code();
    let text = String::from_utf8_lossy(&output.stdout);
    let (workspace, per_crate) = parse_test_output(&text);
    eprintln!(
        "audit-pack: gate test -> exit {} in {:.2}s ({} passed, {} failed, {} ignored)",
        exit.map(|c| c.to_string())
            .unwrap_or_else(|| "error".into()),
        duration.as_secs_f64(),
        workspace.passed,
        workspace.failed,
        workspace.ignored
    );
    let gate = GateResult {
        name: "test".to_string(),
        exit,
        duration,
        reused: false,
    };
    Ok((gate, workspace, per_crate))
}

/// Run one gate that is a pure function call, timing it and recording success
/// (exit 0) or failure (exit 1) without propagating the error.
fn run_silent_gate<F>(name: &str, f: F) -> GateResult
where
    F: FnOnce() -> Result<()>,
{
    let started = Instant::now();
    let result = f();
    let duration = started.elapsed();
    let exit = match result {
        Ok(()) => Some(0),
        Err(_) => Some(1),
    };
    eprintln!(
        "audit-pack: gate {name} -> exit {} in {:.2}s",
        exit.unwrap(),
        duration.as_secs_f64()
    );
    GateResult {
        name: name.to_string(),
        exit,
        duration,
        reused: false,
    }
}

/// Run the mermaid lint gate in-process over `README.md` and `docs/*.md`.
fn run_mermaid_lint_gate(root: &Path) -> Result<GateResult> {
    let started = Instant::now();
    let violations = docs_check::mermaid_violations(root)?.len();
    let duration = started.elapsed();
    let exit = if violations > 0 {
        eprintln!(
            "audit-pack: gate mermaid lint -> exit 1 in {:.2}s ({violations} violation(s))",
            duration.as_secs_f64()
        );
        Some(1)
    } else {
        eprintln!(
            "audit-pack: gate mermaid lint -> exit 0 in {:.2}s",
            duration.as_secs_f64()
        );
        Some(0)
    };
    Ok(GateResult {
        name: "mermaid lint".to_string(),
        exit,
        duration,
        reused: false,
    })
}

/// Parse `cargo test --workspace` output into workspace totals and per-crate
/// totals.  Each `test result:` line is attributed to the most recent test
/// target named by a preceding `Running` or `Doc-tests` line.
fn parse_test_output(text: &str) -> (TestTargetCounts, Vec<TestTargetCounts>) {
    let mut workspace = TestTargetCounts {
        name: "workspace".to_string(),
        ..Default::default()
    };
    let mut per_crate: BTreeMap<String, TestTargetCounts> = BTreeMap::new();
    let mut current: Option<String> = None;
    let running_re = Regex::new(r"deps/([A-Za-z0-9_]+)-[0-9a-f]{16,}").unwrap();
    let doc_re = Regex::new(r"Doc-tests\s+([A-Za-z0-9_]+)").unwrap();
    let result_re =
        Regex::new(r"test result:\s*(?:ok|FAILED).*?(\d+)\s*passed(?:;\s*(\d+)\s*failed)?(?:;\s*(\d+)\s*ignored)?")
            .unwrap();
    for line in text.lines() {
        if let Some(caps) = running_re.captures(line) {
            current = Some(caps[1].replace('_', "-"));
            continue;
        }
        if let Some(caps) = doc_re.captures(line) {
            current = Some(caps[1].replace('_', "-"));
            continue;
        }
        if let Some(caps) = result_re.captures(line) {
            let passed: u64 = caps[1].parse().unwrap_or(0);
            let failed: u64 = caps
                .get(2)
                .map(|m| m.as_str().parse().unwrap_or(0))
                .unwrap_or(0);
            let ignored: u64 = caps
                .get(3)
                .map(|m| m.as_str().parse().unwrap_or(0))
                .unwrap_or(0);
            workspace.passed += passed;
            workspace.failed += failed;
            workspace.ignored += ignored;
            let Some(name) = current.as_deref() else {
                continue;
            };
            let entry = per_crate.entry(name.to_string()).or_default();
            entry.name = name.to_string();
            entry.passed += passed;
            entry.failed += failed;
            entry.ignored += ignored;
        }
    }
    let mut per_crate: Vec<TestTargetCounts> = per_crate.into_values().collect();
    per_crate.sort_by(|a, b| a.name.cmp(&b.name));
    (workspace, per_crate)
}

/// Parse the gate table and tests section of a previously written pack, for
/// `--reuse`.
fn parse_previous_pack(text: &str) -> (TestCounts, Vec<GateResult>) {
    let mut workspace = TestTargetCounts::default();
    let mut per_crate: BTreeMap<String, TestTargetCounts> = BTreeMap::new();
    let mut gates: Vec<GateResult> = Vec::new();
    let mut section: Option<&str> = None;
    let test_re = Regex::new(
        r"^(\S+):\s*passed=(\d+)\s+failed=(\d+)\s+ignored=(\d+)(?:\s+targets=(\d+))?(?:\s+doctests=(\d+))?$",
    )
    .unwrap();
    let gate_re =
        Regex::new(r"^\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]*?)\s*\|\s*([^|]*?)\s*\|$")
            .unwrap();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == TESTS_SECTION {
            section = Some(TESTS_SECTION);
            continue;
        }
        if trimmed == GATES_SECTION {
            section = Some(GATES_SECTION);
            continue;
        }
        if !trimmed.is_empty() && !trimmed.starts_with('|') && !trimmed.starts_with('-') {
            section = None;
        }
        match section {
            Some(TESTS_SECTION) => {
                if let Some(caps) = test_re.captures(trimmed) {
                    let name = caps[1].to_string();
                    let counts = TestTargetCounts {
                        name: name.clone(),
                        passed: caps[2].parse().unwrap_or(0),
                        failed: caps[3].parse().unwrap_or(0),
                        ignored: caps[4].parse().unwrap_or(0),
                        targets: caps.get(5).and_then(|m| m.as_str().parse().ok()),
                        doctests: caps.get(6).and_then(|m| m.as_str().parse().ok()),
                    };
                    if name == "workspace" {
                        workspace = counts;
                    } else {
                        per_crate.insert(name, counts);
                    }
                }
            }
            Some(GATES_SECTION) => {
                if trimmed.starts_with("| gate |") || trimmed.starts_with("|---") {
                    continue;
                }
                if let Some(caps) = gate_re.captures(trimmed) {
                    let name = caps[1].trim().to_string();
                    let exit = match caps[2].trim() {
                        "-" => None,
                        s => Some(s.parse().unwrap_or(-1)),
                    };
                    let duration = match caps[3].trim() {
                        "-" => Duration::ZERO,
                        s => s
                            .strip_suffix('s')
                            .and_then(|s| s.parse::<f64>().ok())
                            .map(Duration::from_secs_f64)
                            .unwrap_or(Duration::ZERO),
                    };
                    gates.push(GateResult {
                        name,
                        exit,
                        duration,
                        reused: true,
                    });
                }
            }
            _ => {}
        }
    }
    let mut per_crate: Vec<TestTargetCounts> = per_crate.into_values().collect();
    per_crate.sort_by(|a, b| a.name.cmp(&b.name));
    (
        TestCounts {
            workspace,
            per_crate,
        },
        gates,
    )
}

/// Gather every static input of the pack from the workspace.
fn gather_inputs(root: &Path, tests: &TestCounts, gates: Vec<GateResult>) -> Result<PackInputs> {
    let (head, commits) = git_commits(root);
    let git_dirty = git_status_dirty(root);
    let hygiene = crate_hygiene(root)?;
    let (headline, headline_mismatch) = headline_comparison(root);
    let big_modules = big_modules(root);
    let open_questions = collect_open_questions(root);
    let markers = readme_markers(root);
    let repo_size = human_size(dir_size(root, &[".git", "target"]));
    let media_size = human_size(dir_size(&root.join("docs/media"), &[]));
    let site = site_info(root);
    let link_check = link_check::scan(root).unwrap_or_default();
    let date = current_date_utc();
    let host_fingerprint = host_fingerprint(root);
    Ok(PackInputs {
        head,
        commits,
        git_dirty,
        tests_workspace: tests.workspace.clone(),
        tests_per_crate: tests.per_crate.clone(),
        gates,
        hygiene_per_crate: hygiene,
        headline,
        headline_mismatch,
        big_modules,
        open_questions,
        markers,
        repo_size,
        media_size,
        site,
        link_check,
        date,
        host_fingerprint,
    })
}

/// `HEAD <sha> <subject>` plus the last 15 commit subjects.
fn git_commits(root: &Path) -> (String, Vec<String>) {
    let output = Command::new("git")
        .args(["log", "--format=%h %s", "-16"])
        .current_dir(root)
        .output();
    let Ok(output) = output else {
        return ("unknown".into(), Vec::new());
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    let head = entries.first().copied().unwrap_or("unknown").to_string();
    let commits = entries
        .into_iter()
        .skip(1)
        .take(15)
        .map(|s| s.to_string())
        .collect();
    (head, commits)
}

/// `true` when `git status --porcelain` is non-empty.
fn git_status_dirty(root: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map(|output| !output.stdout.is_empty())
        .unwrap_or(false)
}

/// Count `unsafe` blocks and `unwrap`/`expect`/`panic!` calls in non-test code,
/// per crate.  Test files under `tests/` directories and `#[cfg(test)] mod
/// tests` / `#[test]` regions are excluded by a brace-depth heuristic.
fn crate_hygiene(root: &Path) -> Result<Vec<CrateHygiene>> {
    let mut files = Vec::new();
    for dir in ["crates", "xtask", "benches"] {
        let path = root.join(dir);
        if path.is_dir() {
            collect_rs(&path, &mut files);
        }
    }
    files.sort();
    let mut hygiene: BTreeMap<String, CrateHygiene> = BTreeMap::new();
    for path in files {
        let Some(crate_name) = crate_of(&path) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let counts = count_hygiene(&text);
        let entry = hygiene.entry(crate_name.clone()).or_default();
        entry.name = crate_name;
        entry.unsafe_blocks += counts.0;
        entry.unwrap += counts.1;
        entry.expect += counts.2;
        entry.panic += counts.3;
    }
    Ok(hygiene.into_values().collect())
}

/// The crate name a `.rs` file belongs to: `crates/<name>/...`, `xtask/...` or
/// `benches/...`; `None` for anything else.
fn crate_of(path: &Path) -> Option<String> {
    let mut components = path.components().map(|c| c.as_os_str().to_string_lossy());
    while let Some(first) = components.next() {
        match first.as_ref() {
            "crates" => return components.next().map(|c| c.into_owned()),
            "xtask" | "benches" => return Some(first.into_owned()),
            _ => {}
        }
    }
    None
}

/// Count hygiene patterns in one file's text.
fn count_hygiene(text: &str) -> (u64, u64, u64, u64) {
    let mut unsafe_blocks = 0u64;
    let mut unwrap = 0u64;
    let mut expect = 0u64;
    let mut panic = 0u64;
    let mut skip_depth = 0i64; // >0 while inside a skipped test region
    let mut pending_cfg_test = false;
    let mut pending_test_fn = false;
    let mut in_block_comment = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if skip_depth > 0 {
            skip_depth += brace_delta(line);
            if skip_depth <= TEST_REGION_CLOSE_DEPTH {
                skip_depth = 0;
            }
            continue;
        }
        if pending_cfg_test {
            pending_cfg_test = false;
            if trimmed.starts_with("mod tests") {
                skip_depth = brace_delta(line);
                if skip_depth <= 0 {
                    skip_depth = 1;
                }
            }
            continue;
        }
        if pending_test_fn {
            pending_test_fn = false;
            if line.contains('{') {
                skip_depth = brace_delta(line);
                if skip_depth <= 0 {
                    skip_depth = 1;
                }
            }
            continue;
        }
        if line.contains("#[cfg(test)]") {
            if line.contains("mod tests") {
                skip_depth = brace_delta(line);
                if skip_depth <= 0 {
                    skip_depth = 1;
                }
            } else {
                pending_cfg_test = true;
            }
            continue;
        }
        if line.contains("#[test]") {
            if line.contains('{') {
                skip_depth = brace_delta(line);
                if skip_depth <= 0 {
                    skip_depth = 1;
                }
            } else {
                pending_test_fn = true;
            }
            continue;
        }
        // Track block comments crudely so commented-out code is not counted.
        let mut remaining = line;
        while !remaining.is_empty() {
            if in_block_comment {
                if let Some(end) = remaining.find("*/") {
                    in_block_comment = false;
                    remaining = &remaining[end + 2..];
                } else {
                    remaining = "";
                }
                continue;
            }
            if remaining.starts_with("//")
                || remaining.starts_with("///")
                || remaining.starts_with("//!")
            {
                remaining = "";
            } else if let Some(start) = remaining.find("/*") {
                // Count code before the comment.
                count_line(
                    &remaining[..start],
                    &mut unsafe_blocks,
                    &mut unwrap,
                    &mut expect,
                    &mut panic,
                );
                if let Some(end) = remaining[start + 2..].find("*/") {
                    remaining = &remaining[start + 2 + end + 2..];
                } else {
                    in_block_comment = true;
                    remaining = "";
                }
            } else {
                count_line(
                    remaining,
                    &mut unsafe_blocks,
                    &mut unwrap,
                    &mut expect,
                    &mut panic,
                );
                remaining = "";
            }
        }
    }
    (unsafe_blocks, unwrap, expect, panic)
}

fn brace_delta(line: &str) -> i64 {
    line.chars().filter(|c| *c == '{').count() as i64
        - line.chars().filter(|c| *c == '}').count() as i64
}

fn count_line(
    line: &str,
    unsafe_blocks: &mut u64,
    unwrap: &mut u64,
    expect: &mut u64,
    panic: &mut u64,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if trimmed.starts_with("//")
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("*")
        || trimmed.starts_with("*/")
    {
        return;
    }
    *unsafe_blocks += line.matches("unsafe {").count() as u64;
    *unwrap += line.matches(".unwrap()").count() as u64;
    *expect += line.matches(".expect(").count() as u64;
    *panic += line.matches("panic!(").count() as u64;
}

/// Recursively collect `.rs` files under `dir`.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Sum the bytes of every file under `dir`, skipping directories named in
/// `skip`.
fn dir_size(dir: &Path, skip: &[&str]) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(dir) else {
        return total;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if skip.iter().any(|s| *s == name) {
                continue;
            }
            total += dir_size(&path, skip);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// Format a byte count as a human-readable size.
fn human_size(bytes: u64) -> String {
    let mut size = bytes as f64;
    let mut unit = "B";
    for next in ["KB", "MB", "GB", "TB"] {
        if size >= 1024.0 {
            size /= 1024.0;
            unit = next;
        } else {
            break;
        }
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{size:.2} {unit}")
    }
}

/// Every README headline row beside its matching quote-sheet row.
fn headline_comparison(root: &Path) -> (Vec<HeadlineComparison>, usize) {
    let readme = fs::read_to_string(root.join("README.md")).unwrap_or_default();
    let sheet = fs::read_to_string(root.join("docs/deck-facts.md")).unwrap_or_default();
    let facts = docs_check::parse_quote_sheet(&sheet);
    let mut rows = Vec::new();
    if let Some(headline_rows) = parse_headline_block(&readme) {
        for (line, _metric, value, class, baseline) in headline_rows {
            let match_label = if value.contains(" of ") {
                let normalized = normalize_cell(&value);
                facts
                    .iter()
                    .find(|f| {
                        normalize_cell(&f.value) == normalized
                            && f.class == class
                            && f.baseline == baseline
                    })
                    .map(|f| f.label.clone())
            } else {
                let pairs = docs_check::scan_result_numbers(&value);
                match pairs.first() {
                    Some((number, unit)) => facts
                        .iter()
                        .find(|f| {
                            f.value == *number
                                && f.unit == *unit
                                && f.class == class
                                && f.baseline == baseline
                        })
                        .map(|f| f.label.clone()),
                    None => None,
                }
            };
            rows.push(HeadlineComparison {
                line,
                value,
                class,
                baseline,
                match_label,
            });
        }
    }
    let mismatch = rows.iter().filter(|r| r.match_label.is_none()).count();
    (rows, mismatch)
}

/// Parse the marker-bracketed headline block: (line, metric, value, class,
/// baseline).  Mirrors `docs_check::parse_headline_rows` but also returns the
/// metric name.
fn parse_headline_block(text: &str) -> Option<Vec<HeadlineRow>> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines
        .iter()
        .position(|l| l.trim() == "<!-- headline:begin -->")?;
    let end = lines
        .iter()
        .position(|l| l.trim() == "<!-- headline:end -->")?;
    if end <= begin {
        return None;
    }
    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(begin + 1) {
        if idx >= end {
            break;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 5 {
            continue;
        }
        if cells[1] == "Metric" || cells[2] == "Value" || cells[2].starts_with("---") {
            continue;
        }
        rows.push((
            idx + 1,
            cells[1].to_string(),
            cells[2].to_string(),
            cells[3].to_string(),
            cells[4].to_string(),
        ));
    }
    Some(rows)
}

/// One parsed headline row: (line number, metric, value, evidence class,
/// baseline).  A named type keeps `parse_headline_block`'s signature simple.
type HeadlineRow = (usize, String, String, String, String);

/// Strip whitespace, thousands separators and the extrapolation marker from a
/// value cell.
fn normalize_cell(value: &str) -> String {
    let filtered: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '\u{2248}')
        .collect();
    if filtered.is_empty() {
        value.trim().to_string()
    } else {
        filtered
    }
}

/// Every file over 600 lines, sorted by line count descending then path.
fn big_modules(root: &Path) -> Vec<(String, u64)> {
    let mut files = Vec::new();
    for dir in ["crates", "xtask", "benches"] {
        let path = root.join(dir);
        if path.is_dir() {
            collect_rs(&path, &mut files);
        }
    }
    let mut out = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lines = text.lines().count() as u64;
        if lines > 600 {
            if let Ok(relative) = path.strip_prefix(root) {
                out.push((relative.to_string_lossy().to_string(), lines));
            }
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Open questions copied from the last two phase entries of the progress log.
fn collect_open_questions(root: &Path) -> Vec<String> {
    // The progress log filename, read as data rather than cited as a process
    // document.  The literal is exempt from the source-hygiene scan below.
    const PROGRESS_FILE: &str = "the dev notes"; // source-hygiene:allow
    let path = root.join(PROGRESS_FILE);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut boundaries: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("### Phase ") {
            boundaries.push(i);
        }
    }
    let mut entries: Vec<String> = Vec::new();
    for (idx, &start) in boundaries.iter().enumerate() {
        let end = boundaries.get(idx + 1).copied().unwrap_or(lines.len());
        entries.push(lines[start..end].join("\n"));
    }
    let mut questions = Vec::new();
    for entry in entries.into_iter().rev().take(2) {
        let mut in_oq = false;
        let mut buf = String::new();
        for line in entry.lines() {
            if line.starts_with("- **Open questions") {
                in_oq = true;
                if let Some((_, rest)) = line.split_once(":**") {
                    if !buf.is_empty() {
                        buf.push(' ');
                    }
                    buf.push_str(rest.trim());
                }
                continue;
            }
            if in_oq {
                if line.starts_with("- **") {
                    break;
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(trimmed);
            }
        }
        let question = buf.trim();
        if !question.is_empty() {
            questions.push(truncate(question, 200));
        }
    }
    questions
}

/// Which README marker blocks exist (both markers present, in order).
fn readme_markers(root: &Path) -> Vec<(String, bool)> {
    let text = fs::read_to_string(root.join("README.md")).unwrap_or_default();
    README_MARKERS
        .iter()
        .map(|(name, begin, end)| {
            let has = text.contains(begin)
                && text.contains(end)
                && text.find(begin).unwrap_or(usize::MAX) < text.find(end).unwrap_or(0);
            (name.to_string(), has)
        })
        .collect()
}

/// The site/ summary: build status, Lighthouse scores and WASM size from the
/// last run, when `site/last-run.json` exists.
fn site_info(root: &Path) -> SiteInfo {
    let site_dir = root.join("site");
    if !site_dir.is_dir() {
        return SiteInfo::default();
    }
    let mut info = SiteInfo {
        exists: true,
        ..Default::default()
    };
    let last_run = site_dir.join("last-run.json");
    let Ok(text) = fs::read_to_string(&last_run) else {
        return info;
    };
    info.has_last_run = true;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return info;
    };
    info.build = value
        .get("build")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut lighthouse = String::new();
    if let Some(scores) = value.get("lighthouse").and_then(|v| v.as_object()) {
        for key in ["performance", "accessibility", "best-practices", "seo"] {
            if let Some(score) = scores.get(key).and_then(|v| v.as_u64()) {
                if !lighthouse.is_empty() {
                    lighthouse.push(' ');
                }
                lighthouse.push_str(&format!("{key}{score}"));
            }
        }
    }
    info.lighthouse = if lighthouse.is_empty() {
        None
    } else {
        Some(lighthouse)
    };
    info.wasm_size = value
        .get("wasm_size_bytes")
        .and_then(|v| v.as_u64())
        .map(human_size);
    info
}

/// The current UTC date/time.
fn current_date_utc() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let secs = duration.as_secs() as i64;
    let days = secs / 86400;
    let sod = (secs % 86400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = sod / 3600;
    let minute = (sod % 3600) / 60;
    let second = sod % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Convert days since the Unix epoch to a civil date (Howard Hinnant's
/// algorithm).
fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

/// A hash of the host fingerprint facts recorded in `environment.json`.
fn host_fingerprint(root: &Path) -> String {
    let env_path = root.join("environment.json");
    let facts = if let Ok(text) = fs::read_to_string(&env_path) {
        extract_env_facts(&text)
    } else {
        match Command::new("uname").arg("-a").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "unknown".to_string(),
        }
    };
    let digest = Sha256::digest(facts.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Canonical host facts from `environment.json`, for fingerprinting.
fn extract_env_facts(env_json: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(env_json).unwrap_or(serde_json::Value::Null);
    let system = value.get("system").and_then(|v| v.as_object());
    let cpu = value.get("cpu").and_then(|v| v.as_object());
    let toolchain = value.get("toolchain").and_then(|v| v.as_object());
    let uname = system
        .and_then(|s| s.get("uname"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let kernel = system
        .and_then(|s| s.get("kernel_release"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model = cpu
        .and_then(|c| c.get("model_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let logical = cpu
        .and_then(|c| c.get("logical_cpus"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let physical = cpu
        .and_then(|c| c.get("physical_cores"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let rustc = toolchain
        .and_then(|t| t.get("rustc_version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    format!("{uname}\0{kernel}\0{model}\0{logical}\0{physical}\0{rustc}")
}

/// Replace absolute home-directory paths and `$HOME` with `~`, so the pack
/// never prints an absolute home path.
fn redact(text: &str) -> String {
    let mut out = text.to_string();
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            out = out.replace(&home, "~");
        }
    }
    if let Ok(user) = std::env::var("USER") {
        if !user.is_empty() {
            out = out.replace(&format!("/{user}/"), "~/");
            out = out.replace(&format!("\\{user}\\"), "\\~\\");
            out = out.replace(&format!("{user}@"), "~@");
        }
    }
    for prefix in ["/home/", "/Users/", r"C:\Users\"] {
        out = out.replace(prefix, "~/");
    }
    out
}

/// Truncate `text` to at most `max` characters, appending `…` when cut.
fn truncate(text: &str, max: usize) -> String {
    let chars: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        format!("{chars}…")
    } else {
        chars
    }
}

/// Render the pack from gathered inputs, with fixed, deterministic section
/// ordering.
pub(crate) fn build_pack(inputs: &PackInputs) -> String {
    let mut lines: Vec<String> = Vec::new();
    let failed: Vec<&str> = inputs
        .gates
        .iter()
        .filter(|g| !g.passed())
        .map(|g| g.name.as_str())
        .collect();

    let head_sha = inputs
        .head
        .split_whitespace()
        .next()
        .unwrap_or(&inputs.head);
    if failed.is_empty() {
        lines.push(format!("OK: all {} gates passed", inputs.gates.len()));
    } else {
        lines.push(format!("FAIL: {}", failed.join(", ")));
    }
    lines.push(format!(
        "Generated {} · host fp {} · git {} {}",
        inputs.date,
        inputs.host_fingerprint,
        head_sha,
        if inputs.git_dirty { "dirty" } else { "clean" }
    ));
    lines.push(String::new());

    lines.push("## Repository".to_string());
    lines.push(format!("HEAD {}", inputs.head));
    lines.push("Last 15 commit subjects:".to_string());
    for (i, subject) in inputs.commits.iter().enumerate() {
        lines.push(format!("  {}. {}", i + 1, subject));
    }
    lines.push(format!(
        "git status: {}",
        if inputs.git_dirty { "dirty" } else { "clean" }
    ));
    lines.push(String::new());

    lines.push("## Tests".to_string());
    lines.push(format!(
        "workspace: passed={} failed={} ignored={} (targets={:?}, doctests={:?})",
        inputs.tests_workspace.passed,
        inputs.tests_workspace.failed,
        inputs.tests_workspace.ignored,
        inputs.tests_workspace.targets,
        inputs.tests_workspace.doctests,
    ));
    for counts in &inputs.tests_per_crate {
        lines.push(format!(
            "{}: passed={} failed={} ignored={}",
            counts.name, counts.passed, counts.failed, counts.ignored,
        ));
    }
    lines.push(String::new());

    lines.push("## Gates".to_string());
    lines.push("| gate | exit | duration | result |".to_string());
    lines.push("|---|---|---|---|".to_string());
    for gate in &inputs.gates {
        let exit = gate
            .exit
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());
        let duration = if gate.exit.is_none() {
            "-".to_string()
        } else {
            format!("{:.2}s", gate.duration.as_secs_f64())
        };
        let result = if gate.reused {
            "reused".to_string()
        } else if gate.passed() {
            "ok".to_string()
        } else {
            "FAIL".to_string()
        };
        lines.push(format!(
            "| {} | {} | {} | {} |",
            gate.name, exit, duration, result
        ));
    }
    lines.push(String::new());

    lines.push(
        "## Code hygiene per crate (unsafe blocks · unwrap / expect / panic in non-test code)"
            .to_string(),
    );
    for hygiene in &inputs.hygiene_per_crate {
        lines.push(format!(
            "{}: unsafe={} unwrap={} expect={} panic={}",
            hygiene.name, hygiene.unsafe_blocks, hygiene.unwrap, hygiene.expect, hygiene.panic
        ));
    }
    lines.push(String::new());

    lines.push("## README headline vs quote sheet".to_string());
    for row in &inputs.headline {
        let match_label = row.match_label.as_deref().unwrap_or("no match").to_string();
        lines.push(format!(
            "headline[{}]: {} | {} | {} → {}",
            row.line,
            row.value,
            row.class,
            row.baseline,
            truncate(&match_label, 70)
        ));
    }
    lines.push(format!(
        "headline mismatch count: {}",
        inputs.headline_mismatch
    ));
    lines.push(String::new());

    lines.push("## Modules over 600 lines".to_string());
    let mut shown = inputs.big_modules.clone();
    let total_big = shown.len();
    if shown.len() > 40 {
        shown.truncate(40);
    }
    for (path, count) in shown {
        lines.push(format!("{path}: {count} lines"));
    }
    if total_big > 40 {
        lines.push(format!(
            "…and {} more modules over 600 lines",
            total_big - 40
        ));
    }
    lines.push(String::new());

    lines.push("## Open questions (from the progress log)".to_string());
    let max_questions = 8;
    for (i, question) in inputs.open_questions.iter().enumerate().take(max_questions) {
        lines.push(format!("  {}. {}", i + 1, question));
    }
    if inputs.open_questions.len() > max_questions {
        lines.push(format!(
            "  …and {} more open questions",
            inputs.open_questions.len() - max_questions
        ));
    }
    lines.push(String::new());

    lines.push("## README marker blocks".to_string());
    let marker_text: String = inputs
        .markers
        .iter()
        .map(|(name, exists)| {
            if *exists {
                format!("✔ {name}")
            } else {
                format!("✖ {name}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    lines.push(marker_text);
    lines.push(String::new());

    lines.push("## Sizes".to_string());
    lines.push(format!(
        "repository: {} (excluding .git and target)",
        inputs.repo_size
    ));
    lines.push(format!("docs/media: {}", inputs.media_size));
    lines.push(String::new());

    lines.push("## site".to_string());
    lines.push(match &inputs.site {
        SiteInfo { exists: false, .. } => "site/ does not exist".to_string(),
        SiteInfo {
            exists: true,
            has_last_run: false,
            ..
        } => "site/ exists; no last-run record".to_string(),
        SiteInfo {
            exists: true,
            has_last_run: true,
            build,
            lighthouse,
            wasm_size,
            ..
        } => format!(
            "site/: build {} · Lighthouse {} · wasm {}",
            build.as_deref().unwrap_or("unknown"),
            lighthouse.as_deref().unwrap_or("-"),
            wasm_size.as_deref().unwrap_or("-")
        ),
    });
    lines.push(String::new());

    lines.push("## Link-check".to_string());
    lines.push(format!(
        "README.md and docs/*.md: {} links, {} broken, {} external (not fetched, offline)",
        inputs.link_check.total, inputs.link_check.broken, inputs.link_check.external,
    ));
    for broken in &inputs.link_check.broken_list {
        lines.push(format!("  broken: {broken}"));
    }

    let content = lines.join("\n") + "\n";
    debug_assert!(
        content.lines().count() <= LINE_CAP,
        "audit pack has {} lines, exceeding the {LINE_CAP} line cap",
        content.lines().count()
    );
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_gate(name: &str, exit: Option<i32>, reused: bool) -> GateResult {
        GateResult {
            name: name.to_string(),
            exit,
            duration: Duration::from_secs_f64(1.5),
            reused,
        }
    }

    fn default_inputs() -> PackInputs {
        PackInputs {
            head: "7d60431 replication tooling and emulated-RTT TCP fallback".to_string(),
            commits: (0..15)
                .map(|i| format!("abc{i:04} commit subject number {i}"))
                .collect(),
            git_dirty: false,
            tests_workspace: TestTargetCounts {
                name: "workspace".into(),
                passed: 1228,
                failed: 0,
                ignored: 3,
                targets: Some(48),
                doctests: Some(6),
            },
            tests_per_crate: vec![TestTargetCounts {
                name: "climb-mtl".into(),
                passed: 45,
                failed: 0,
                ignored: 0,
                targets: None,
                doctests: None,
            }],
            gates: GATES
                .iter()
                .map(|name| fake_gate(name, Some(0), false))
                .collect(),
            hygiene_per_crate: vec![CrateHygiene {
                name: "climb-mtl".into(),
                unsafe_blocks: 0,
                unwrap: 2,
                expect: 0,
                panic: 0,
            }],
            headline: vec![HeadlineComparison {
                line: 96,
                value: "97.307 %".to_string(),
                class: "DERIVED".to_string(),
                baseline: "B".to_string(),
                match_label: Some("Live DNS (`www.climb.example. A`)".to_string()),
            }],
            headline_mismatch: 0,
            big_modules: vec![("crates/climb-report/src/html.rs".to_string(), 2740)],
            open_questions: vec!["the p99 figures are from this host only".to_string()],
            markers: vec![("logo".to_string(), true), ("badges".to_string(), true)],
            repo_size: "41.00 MB".to_string(),
            media_size: "7.81 MB".to_string(),
            site: SiteInfo::default(),
            link_check: LinkCheck {
                total: 84,
                broken: 0,
                broken_list: Vec::new(),
                external: 12,
            },
            date: "2026-11-09T13:00:00Z".to_string(),
            host_fingerprint: "4a8f2c1e".to_string(),
        }
    }

    #[test]
    fn audit_pack_line_cap_under_limit() {
        let mut inputs = default_inputs();
        // Stress the pack with maximal content: every gate failing, many big
        // modules and open questions, plus a home path that must be redacted.
        inputs.gates = GATES
            .iter()
            .map(|name| fake_gate(name, Some(1), false))
            .collect();
        inputs.big_modules = (0..50)
            .map(|i| (format!("crates/long/src/module_{i}.rs"), 700 + i as u64))
            .collect();
        inputs.open_questions = (0..12)
            .map(|i| format!("open question number {i} with some detail text here"))
            .collect();
        inputs
            .open_questions
            .push("/home/alice/secret.key must not appear literally".to_string()); // path-hygiene:allow
        let content = build_pack(&inputs);
        let lines = content.lines().count();
        assert!(
            lines <= LINE_CAP,
            "pack has {lines} lines, cap is {LINE_CAP}:\n{content}"
        );
        assert!(
            !content.contains("/home/"),
            "home path must be redacted:\n{content}"
        );
        assert!(
            !content.contains("/Users/"),
            "macOS home path must be redacted:\n{content}"
        );
    }

    #[test]
    fn audit_pack_first_line_names_failed_gates() {
        let mut inputs = default_inputs();
        inputs.gates = vec![
            fake_gate("build", Some(0), false),
            fake_gate("test", Some(1), false),
            fake_gate("clippy", Some(1), false),
        ];
        let content = build_pack(&inputs);
        let mut lines = content.lines();
        let first = lines.next().expect("pack is non-empty");
        assert_eq!(
            first, "FAIL: test, clippy",
            "first line is the failure banner: {first}"
        );
        // The pack is still written in full despite the failures.
        assert!(content.contains("## Gates"));
        assert!(content.contains("## Repository"));
    }

    #[test]
    fn audit_pack_all_pass_banner() {
        let inputs = default_inputs();
        let content = build_pack(&inputs);
        let first = content.lines().next().expect("pack is non-empty");
        assert_eq!(first, "OK: all 10 gates passed");
    }

    #[test]
    fn redact_hides_home_paths() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/nobody".to_string()); // path-hygiene:allow
        let text = format!("build log: {home}/src/climb/target/out and /Users/bob/Library/x"); // path-hygiene:allow
        let redacted = redact(&text);
        assert!(
            !redacted.contains("/home/"),
            "linux home path redacted: {redacted}"
        );
        assert!(
            !redacted.contains("/Users/"),
            "macOS home path redacted: {redacted}"
        );
        assert!(
            !redacted.contains(&home),
            "literal $HOME redacted: {redacted}"
        );
    }

    #[test]
    fn gate_row_display_is_deterministic() {
        let passed = fake_gate("build", Some(0), false);
        assert_eq!(gate_row(&passed), "| build | 0 | 1.50s | ok |");
        let failed = fake_gate("clippy", Some(2), false);
        assert_eq!(gate_row(&failed), "| clippy | 2 | 1.50s | FAIL |");
        let absent = fake_gate("verify-numbers", None, false);
        assert_eq!(gate_row(&absent), "| verify-numbers | - | - | FAIL |");
        let reused = fake_gate("fmt", Some(0), true);
        assert_eq!(gate_row(&reused), "| fmt | 0 | 1.50s | reused |");
    }

    /// Render one gate table row the way `build_pack` does.
    fn gate_row(gate: &GateResult) -> String {
        let exit = gate
            .exit
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());
        let duration = if gate.exit.is_none() {
            "-".to_string()
        } else {
            format!("{:.2}s", gate.duration.as_secs_f64())
        };
        let result = if gate.reused {
            "reused".to_string()
        } else if gate.passed() {
            "ok".to_string()
        } else {
            "FAIL".to_string()
        };
        format!("| {} | {} | {} | {} |", gate.name, exit, duration, result)
    }
}
