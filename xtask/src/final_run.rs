//! `cargo xtask final-run`: the one resumable command that produces every
//! recorded artifact from a single pinned environment.
//!
//! The individual subcommands are each correct in isolation, but the published
//! numbers are only trustworthy when one run measures the wire sizes, memory,
//! benchmarks, stability and DNS pipeline, then consolidates, renders and
//! checks the result. Doing that by hand is how a stale measurement gets
//! published under a fresh timestamp.
//!
//! The run is planned then executed. [`plan`] is a pure function: it spawns no
//! process and touches no file, so the step order, the skip decisions and the
//! resume behaviour are unit-testable. Execution walks the plan, records every
//! finished step in `target/final-run/state.json`, and logs one progress line
//! per step to `target/final-run/run.log`.
//!
//! `--resume` restarts at the first step that is not recorded as complete and
//! invalidates every later record, so a re-measured input can never leave a
//! stale downstream artifact behind. `--quick` is a reduced smoke run (Small
//! and Medium measurements, no DNS) that never refreshes the committed sample
//! output or the README; `--dry-run` prints the plan and exits.
//!
//! ```text
//! cargo xtask final-run --dry-run
//! cargo xtask final-run --quick
//! cargo xtask final-run --resume
//! ```

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::proc;
use crate::{build_bench_command, resolve_pin_cpu, PIN_CPU};

// ---------------------------------------------------------------------------
// Canonical artifact paths
// ---------------------------------------------------------------------------

const WIRE_REPORT: &str = "target/wire-report.json";
const MEMORY_REPORT: &str = "target/memory-report.json";
const LADDER_VERIFICATION: &str = "target/ladder-verification.json";
const RESULTS: &str = "target/climb-results.json";
const REPORT_HTML: &str = "target/climb-report/report.html";
const CHARTS_DIR: &str = "target/climb-report/charts";
const DNS_LIVE: &str = "target/dns-capture.json";
const REPLICATION_SUMMARY: &str = "target/replication-summary.json";
const CLASSICAL_LENGTHS: &str = "target/climb-classical-signatures.json";

const STATE_DIR: &str = "target/final-run";
const STATE_FILE: &str = "target/final-run/state.json";
const LOG_FILE: &str = "target/final-run/run.log";
const QUICK_SHEET: &str = "target/final-run/deck-facts.quick.md";
const QUICK_DELTA: &str = "target/final-run/deck-delta.quick.md";

const SIZES_FULL: &str = "small,medium,large";
const SIZES_QUICK: &str = "small,medium";
const LADDER_SIZES_QUICK: &str = "1,2,4,5,13";
const STABILITY_SIZES: &str = "small,onek,medium";
const STABILITY_RUNS: usize = 5;

/// The governor every benchmark core must be pinned to before a full run.
const TARGET_GOVERNOR: &str = "performance";
/// Refuse a full run when another process keeps the pinned core this busy.
const CORE_USAGE_LIMIT_PERCENT: f64 = 20.0;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Args)]
pub(crate) struct FinalRunArgs {
    /// Reduced smoke run: Small/Medium measurements, no DNS pipeline. The
    /// benchmark suites still cover every size because the signing bound is
    /// only checkable where the non-amortised baseline was measured.
    #[arg(long)]
    quick: bool,

    /// Restart from the first step not recorded in target/final-run/state.json.
    /// Later records are invalidated so no stale artifact survives a re-run.
    #[arg(long)]
    resume: bool,

    /// Print the plan and the expected durations without running anything.
    #[arg(long)]
    dry_run: bool,

    /// Skip the live DNS pipeline and use the committed recorded fixtures.
    #[arg(long)]
    skip_dns: bool,

    /// Advisory preflight: print refusals as warnings instead of stopping.
    /// A smoke run should not need the production governor.
    #[arg(long)]
    allow_unpinned: bool,

    /// Plain-text dump of the slide deck (for example from `pdftotext`). When
    /// supplied, the run regenerates `docs/deck-delta.md` against the refreshed
    /// quote sheet; without it the deck-delta step is skipped with a reason.
    #[arg(long, value_name = "PATH")]
    deck_text: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Planning — pure, no process spawning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StepId {
    PinEnv,
    WireReport,
    MemoryReport,
    LadderVerify,
    BenchSigning,
    BenchVerifying,
    BenchPrimitives,
    BenchFull,
    Stability,
    ClassicalBaselines,
    DnsDemo,
    DnsRtt,
    DnsCache,
    ReplicateSummary,
    ReplicateCompare,
    Consolidate,
    VerifyConsolidated,
    Charts,
    ReportHtml,
    ReportCheck,
    SampleOutput,
    DeckFacts,
    DeckDelta,
    Headline,
    VerifyNumbers,
    DocsCheck,
    AuditPack,
}

impl StepId {
    /// Stable key written into the state file. Renaming a label must not make
    /// an older state file look like a fresh run, so the key is explicit.
    pub(crate) fn key(self) -> &'static str {
        match self {
            StepId::PinEnv => "pin-env",
            StepId::WireReport => "wire-report",
            StepId::MemoryReport => "memory-report",
            StepId::LadderVerify => "ladder-verify",
            StepId::BenchSigning => "bench-signing",
            StepId::BenchVerifying => "bench-verifying",
            StepId::BenchPrimitives => "bench-primitives",
            StepId::BenchFull => "bench-full",
            StepId::Stability => "stability",
            StepId::ClassicalBaselines => "classical-baselines",
            StepId::DnsDemo => "dns-demo",
            StepId::DnsRtt => "dns-rtt",
            StepId::DnsCache => "dns-cache",
            StepId::ReplicateSummary => "replicate-summary",
            StepId::ReplicateCompare => "replicate-compare",
            StepId::Consolidate => "consolidate",
            StepId::VerifyConsolidated => "verify-consolidated",
            StepId::Charts => "charts",
            StepId::ReportHtml => "report-html",
            StepId::ReportCheck => "report-check",
            StepId::SampleOutput => "sample-output",
            StepId::DeckFacts => "deck-facts",
            StepId::DeckDelta => "deck-delta",
            StepId::Headline => "headline",
            StepId::VerifyNumbers => "verify-numbers",
            StepId::DocsCheck => "docs-check",
            StepId::AuditPack => "audit-pack",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            StepId::PinEnv => "pin-env",
            StepId::WireReport => "wire-report",
            StepId::MemoryReport => "memory-report",
            StepId::LadderVerify => "ladder-verify",
            StepId::BenchSigning => "bench-signing",
            StepId::BenchVerifying => "bench-verifying",
            StepId::BenchPrimitives => "bench-primitives",
            StepId::BenchFull => "bench-full",
            StepId::Stability => "stability",
            StepId::ClassicalBaselines => "classical-baselines",
            StepId::DnsDemo => "dns-demo",
            StepId::DnsRtt => "dns-rtt",
            StepId::DnsCache => "dns-cache",
            StepId::ReplicateSummary => "replicate-summary",
            StepId::ReplicateCompare => "replicate-compare",
            StepId::Consolidate => "consolidate",
            StepId::VerifyConsolidated => "verify-consolidated",
            StepId::Charts => "charts",
            StepId::ReportHtml => "report --format html",
            StepId::ReportCheck => "report --check",
            StepId::SampleOutput => "sample-output",
            StepId::DeckFacts => "deck-facts",
            StepId::DeckDelta => "deck-delta",
            StepId::Headline => "headline",
            StepId::VerifyNumbers => "verify-numbers",
            StepId::DocsCheck => "docs-check",
            StepId::AuditPack => "audit-pack",
        }
    }

    /// Expected wall-clock time, printed before the step starts. These are
    /// order-of-magnitude estimates for the reference laptop, not promises.
    pub(crate) fn expected(self, quick: bool) -> &'static str {
        match self {
            StepId::PinEnv => "1 s",
            StepId::WireReport => {
                if quick {
                    "30 s"
                } else {
                    "2 min"
                }
            }
            StepId::MemoryReport => {
                if quick {
                    "1 min"
                } else {
                    "4 min"
                }
            }
            StepId::LadderVerify => {
                if quick {
                    "30 s"
                } else {
                    "1 min"
                }
            }
            StepId::BenchSigning => "6 min",
            StepId::BenchVerifying => "6 min",
            StepId::BenchPrimitives => {
                if quick {
                    "1 min"
                } else {
                    "3 min"
                }
            }
            StepId::BenchFull => "1 min",
            StepId::Stability => "2 min",
            StepId::ClassicalBaselines => "5 s",
            StepId::DnsDemo => "1 min",
            StepId::DnsRtt => "10 min",
            StepId::DnsCache => "10 min",
            StepId::ReplicateSummary => "1 s",
            StepId::ReplicateCompare => "2 s",
            StepId::Consolidate => "30 s",
            StepId::VerifyConsolidated => "1 s",
            StepId::Charts => "1 s",
            StepId::ReportHtml => "5 s",
            StepId::ReportCheck => "5 s",
            StepId::SampleOutput => "30 s",
            StepId::DeckFacts => "5 min",
            StepId::DeckDelta => "5 s",
            StepId::Headline => "1 s",
            StepId::VerifyNumbers => "1 min",
            StepId::DocsCheck => "1 min",
            StepId::AuditPack => "15 min",
        }
    }
}

/// What the step actually invokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invocation {
    /// A subcommand of the running `xtask` binary.
    Xtask(Vec<String>),
    /// A Criterion benchmark target, run pinned like `bench-all` runs it.
    Bench(String),
    /// A shell script from the repository.
    Script(Vec<String>),
    /// The in-process guard over the classical signature-length document.
    ClassicalCheck(String),
    /// Compare the primary results document against the first replica
    /// `climb-results.json` found under a replication directory. The replica
    /// path is resolved at execution time, so the plan itself stays pure.
    ReplicaCompare { primary: String, directory: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub id: StepId,
    pub invocation: Invocation,
    /// Extra environment variables for the child.
    pub env: Vec<(String, String)>,
    /// `Some` when the step is deliberately not run in this mode.
    pub skip: Option<&'static str>,
}

impl Step {
    fn xtask(id: StepId, args: &[&str]) -> Self {
        Step {
            id,
            invocation: Invocation::Xtask(args.iter().map(|a| (*a).to_string()).collect()),
            env: Vec::new(),
            skip: None,
        }
    }

    fn bench(id: StepId, target: &str) -> Self {
        Step {
            id,
            invocation: Invocation::Bench(target.to_string()),
            env: Vec::new(),
            skip: None,
        }
    }

    fn script(id: StepId, args: &[&str]) -> Self {
        Step {
            id,
            invocation: Invocation::Script(args.iter().map(|a| (*a).to_string()).collect()),
            env: Vec::new(),
            skip: None,
        }
    }

    fn classical_check(id: StepId, path: &str) -> Self {
        Step {
            id,
            invocation: Invocation::ClassicalCheck(path.to_string()),
            env: Vec::new(),
            skip: None,
        }
    }

    fn replica_compare(id: StepId, primary: &str, directory: &str) -> Self {
        Step {
            id,
            invocation: Invocation::ReplicaCompare {
                primary: primary.to_string(),
                directory: directory.to_string(),
            },
            env: Vec::new(),
            skip: None,
        }
    }

    fn with_env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.to_string(), value.into()));
        self
    }

    fn skipped(mut self, reason: &'static str) -> Self {
        self.skip = Some(reason);
        self
    }
}

/// Everything the plan needs to know about the host, resolved by the caller so
/// [`plan`] stays pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanInputs {
    pub quick: bool,
    pub skip_dns: bool,
    pub docker_available: bool,
    pub second_host_present: bool,
    /// Plain-text deck dump to diff against the quote sheet; `None` skips the
    /// deck-delta step.
    pub deck_text: Option<String>,
}

/// Build the ordered step list. Pure: no I/O, no subprocesses.
pub(crate) fn plan(input: &PlanInputs) -> Vec<Step> {
    let sizes = if input.quick { SIZES_QUICK } else { SIZES_FULL };
    let mut steps = Vec::with_capacity(27);

    steps.push(Step::xtask(StepId::PinEnv, &["pin-env"]));

    steps.push(Step::xtask(
        StepId::WireReport,
        &["wire-report", "--sizes", sizes, "--out", WIRE_REPORT],
    ));
    steps.push(Step::xtask(
        StepId::MemoryReport,
        &["memory-report", "--sizes", sizes, "--out", MEMORY_REPORT],
    ));

    let mut ladder = vec!["ladder-verify".to_string()];
    if input.quick {
        ladder.push("--sizes".to_string());
        ladder.push(LADDER_SIZES_QUICK.to_string());
    }
    ladder.push("--out".to_string());
    ladder.push(LADDER_VERIFICATION.to_string());
    steps.push(Step {
        id: StepId::LadderVerify,
        invocation: Invocation::Xtask(ladder),
        env: Vec::new(),
        skip: None,
    });

    // Benchmarks run every dataset size in both modes. The signing speed-up
    // bound is only checkable at a size where the non-amortised baseline was
    // measured, and that baseline is one signature per iteration whatever the
    // dataset size, so the Large groups are cheap enough for a smoke run.
    steps.push(Step::bench(StepId::BenchSigning, "signing"));
    steps.push(Step::bench(StepId::BenchVerifying, "verifying"));
    let primitives = if input.quick {
        Step::bench(StepId::BenchPrimitives, "primitives")
            .with_env("CLIMB_PRIMITIVE_SAMPLES", "10")
            .with_env("CLIMB_PRIMITIVES_SIZES", LADDER_SIZES_QUICK)
    } else {
        Step::bench(StepId::BenchPrimitives, "primitives")
    };
    steps.push(primitives);
    // The full-signature (classical) baseline suite. Its run writes the
    // signature-length document the next step checks.
    steps.push(Step::bench(StepId::BenchFull, "classical"));

    steps.push(Step::xtask(
        StepId::Stability,
        &[
            "stability",
            "--sizes",
            STABILITY_SIZES,
            "--runs",
            &STABILITY_RUNS.to_string(),
            "--results",
            RESULTS,
        ],
    ));

    steps.push(Step::classical_check(
        StepId::ClassicalBaselines,
        CLASSICAL_LENGTHS,
    ));

    let dns = Step::xtask(StepId::DnsDemo, &["dns-demo", "--record", DNS_LIVE]);
    steps.push(if input.quick {
        dns.skipped("skipped: quick run, using the recorded fixture")
    } else if input.skip_dns {
        dns.skipped("skipped: --skip-dns, using the recorded fixture")
    } else if !input.docker_available {
        dns.skipped("skipped: Docker unavailable, using the recorded fixture")
    } else {
        dns
    });

    let dns_rtt = Step::xtask(StepId::DnsRtt, &["dns-rtt", "--out", "target/dns-rtt.json"]);
    steps.push(if input.quick {
        dns_rtt.skipped("skipped: quick run, using the recorded fixture")
    } else if input.skip_dns || !input.docker_available {
        dns_rtt.skipped("skipped: DNS pipeline disabled, using the recorded fixture")
    } else {
        dns_rtt
    });

    let dns_cache = Step::xtask(
        StepId::DnsCache,
        &["dns-cache", "--out", "target/dns-cache.json"],
    );
    steps.push(if input.quick {
        dns_cache.skipped("skipped: quick run, using the recorded fixture")
    } else if input.skip_dns || !input.docker_available {
        dns_cache.skipped("skipped: DNS pipeline disabled, using the recorded fixture")
    } else {
        dns_cache
    });

    steps.push(Step::xtask(
        StepId::ReplicateSummary,
        &["replicate-summary", "--out", REPLICATION_SUMMARY],
    ));
    let compare = Step::replica_compare(StepId::ReplicateCompare, RESULTS, "data/replication");
    steps.push(if input.second_host_present {
        compare
    } else {
        compare.skipped("skipped: no second host's results in data/replication")
    });

    steps.push(Step::xtask(
        StepId::Consolidate,
        &[
            "consolidate",
            "--force",
            "--wire-report",
            WIRE_REPORT,
            "--memory-report",
            MEMORY_REPORT,
            "--ladder-verification",
            LADDER_VERIFICATION,
            "--output",
            RESULTS,
            "--reproduction-mode",
            "remeasure",
        ],
    ));
    steps.push(Step::xtask(
        StepId::VerifyConsolidated,
        &[
            "verify-consolidated",
            "--memory-report",
            MEMORY_REPORT,
            "--results",
            RESULTS,
        ],
    ));

    steps.push(Step::xtask(StepId::Charts, &["charts"]));
    steps.push(Step::xtask(
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
    steps.push(Step::xtask(
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

    // A smoke run must not overwrite the committed sample output or the
    // README with reduced-scope numbers. The full run refreshes both, then
    // the documentation gates check them.
    let sample = Step::script(StepId::SampleOutput, &["scripts/make-sample-output.sh"]);
    steps.push(if input.quick {
        sample.skipped("skipped: quick run does not refresh the committed sample output")
    } else {
        sample
    });

    let deck_facts = if input.quick {
        Step::xtask(StepId::DeckFacts, &["deck-facts", "--out", QUICK_SHEET])
    } else {
        Step::xtask(StepId::DeckFacts, &["deck-facts"])
    };
    steps.push(deck_facts);

    let deck_delta = match (&input.deck_text, input.quick) {
        (Some(deck), true) => Step::xtask(
            StepId::DeckDelta,
            &[
                "deck-delta",
                "--deck-text",
                deck,
                "--quote-sheet",
                QUICK_SHEET,
                "--out",
                QUICK_DELTA,
            ],
        ),
        (Some(deck), false) => Step::xtask(
            StepId::DeckDelta,
            &[
                "deck-delta",
                "--deck-text",
                deck,
                "--quote-sheet",
                "docs/deck-facts.md",
                "--out",
                "docs/deck-delta.md",
            ],
        ),
        (None, _) => Step::xtask(StepId::DeckDelta, &["deck-delta"])
            .skipped("skipped: no deck text supplied (--deck-text <path>)"),
    };
    steps.push(deck_delta);

    let headline = Step::xtask(StepId::Headline, &["headline"]);
    steps.push(if input.quick {
        headline.skipped("skipped: quick run does not rewrite the README")
    } else {
        headline
    });

    // The spread requirement belongs to the run of record: a smoke run does
    // not refresh the committed artifact that carries the spreads.
    let verify_numbers = if input.quick {
        Step::xtask(StepId::VerifyNumbers, &["verify-numbers"])
    } else {
        Step::xtask(
            StepId::VerifyNumbers,
            &["verify-numbers", "--require-spread"],
        )
    };
    steps.push(verify_numbers);
    steps.push(Step::xtask(StepId::DocsCheck, &["docs-check"]));
    steps.push(Step::xtask(StepId::AuditPack, &["audit-pack"]));

    steps
}

/// Render the plan as the dry-run output. Pure so tests can assert it.
pub(crate) fn render_plan(steps: &[Step], quick: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "final-run plan: {} step(s), mode={}\n",
        steps.len(),
        if quick { "quick" } else { "full" }
    ));
    for (index, step) in steps.iter().enumerate() {
        let expected = step.id.expected(quick);
        match step.skip {
            Some(reason) => out.push_str(&format!(
                "  {:>2}. [{}] skipped — {} (expected {})\n",
                index + 1,
                step.id.label(),
                reason,
                expected
            )),
            None => out.push_str(&format!(
                "  {:>2}. [{}] {} (expected {})\n",
                index + 1,
                step.id.label(),
                describe(&step.invocation),
                expected
            )),
        }
    }
    out
}

fn describe(invocation: &Invocation) -> String {
    match invocation {
        Invocation::Xtask(args) => format!("xtask {}", args.join(" ")),
        Invocation::Bench(target) => format!("cargo bench --bench {target}"),
        Invocation::Script(args) => format!("bash {}", args.join(" ")),
        Invocation::ClassicalCheck(path) => format!("check classical signature lengths in {path}"),
        Invocation::ReplicaCompare { primary, directory } => {
            format!("xtask replicate-compare --a {primary} --b <first replica under {directory}>")
        }
    }
}

// ---------------------------------------------------------------------------
// Preflight — pure decision, fakeable readings
// ---------------------------------------------------------------------------

/// One snapshot of the inputs the production preflight refuses on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreflightReadings {
    pub governor: String,
    /// `None` when the AC adapter state could not be read.
    pub ac_online: Option<bool>,
    /// `Some(true)` when a battery reports it is discharging.
    pub battery_discharging: Option<bool>,
    /// `None` when the per-core usage could not be sampled.
    pub pinned_core_usage_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Refusal {
    GovernorNotPinned {
        found: String,
    },
    OnBattery,
    CoreBusy {
        percent: f64,
    },
    /// Docker daemon is not available.
    DockerNotAvailable,
    /// The docker compose resolver could not be found or verified.
    ComposeResolver(String),
    /// The kernel netem module is not available (only in non-quick mode).
    NetemNotAvailable,
}

impl Refusal {
    pub(crate) fn message(&self) -> String {
        match self {
            Refusal::GovernorNotPinned { found } => format!(
                "CPU governor is `{found}`, not `{TARGET_GOVERNOR}` — pin it with \
                 `sudo scripts/pin-perf-governor.sh {TARGET_GOVERNOR}`"
            ),
            Refusal::OnBattery => {
                "the machine is running on battery power — plug in the mains adapter".to_string()
            }
            Refusal::CoreBusy { percent } => format!(
                "CPU {PIN_CPU} is {percent:.0}% busy (limit {CORE_USAGE_LIMIT_PERCENT:.0}%) — \
                 stop the other workload"
            ),
            Refusal::DockerNotAvailable => "Docker daemon is not running. Start the Docker daemon \
                 and try again."
                .to_string(),
            Refusal::ComposeResolver(msg) => format!(
                "Docker compose resolver not available: {}. \
                 Run: pacman -S docker-compose || apt-get install docker-compose-v2",
                msg
            ),
            Refusal::NetemNotAvailable => {
                "Kernel netem qdisc is not available. This is required for \
                 dns-rtt measurements. Install: sudo modprobe sch_netem"
                    .to_string()
            }
        }
    }
}

/// Every reason a full run must not start. Pure so the refusals can be tested
/// with fake readings instead of a real battery or a busy machine.
pub(crate) fn preflight_refusals(readings: &PreflightReadings) -> Vec<Refusal> {
    let mut refusals = Vec::new();
    if readings.governor != TARGET_GOVERNOR {
        refusals.push(Refusal::GovernorNotPinned {
            found: readings.governor.clone(),
        });
    }
    if readings.battery_discharging == Some(true) && readings.ac_online != Some(true) {
        refusals.push(Refusal::OnBattery);
    }
    if let Some(percent) = readings.pinned_core_usage_percent {
        if percent > CORE_USAGE_LIMIT_PERCENT {
            refusals.push(Refusal::CoreBusy { percent });
        }
    }
    refusals
}

/// Read the governor of the first CPU that exposes a cpufreq interface.
fn read_governor() -> String {
    let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") else {
        return "unknown".to_string();
    };
    let mut cpus: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("cpu") && n[3..].chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    cpus.sort();
    for cpu in cpus {
        let path = cpu.join("cpufreq/scaling_governor");
        if let Ok(value) = fs::read_to_string(&path) {
            return value.trim().to_string();
        }
    }
    "unknown".to_string()
}

fn read_ac_online() -> Option<bool> {
    for name in ["AC", "ACAD", "ADP1"] {
        let path = format!("/sys/class/power_supply/{name}/online");
        if let Ok(value) = fs::read_to_string(path) {
            return Some(value.trim() == "1");
        }
    }
    None
}

fn read_battery_discharging() -> Option<bool> {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return None;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") {
            continue;
        }
        if let Ok(status) = fs::read_to_string(entry.path().join("status")) {
            let status = status.trim();
            return Some(status == "Discharging");
        }
        if let Ok(online) = fs::read_to_string(entry.path().join("online")) {
            return Some(online.trim() == "1");
        }
    }
    None
}

/// Read the per-CPU tick counters from `/proc/stat`.
fn read_cpu_ticks(cpu: u32) -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    let prefix = format!("cpu{cpu} ");
    let line = text.lines().find(|l| l.starts_with(&prefix))?;
    let fields: Vec<u64> = line[prefix.len()..]
        .split_whitespace()
        .filter_map(|f| f.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    let total: u64 = fields.iter().sum();
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    Some((total, idle))
}

/// Sample the pinned core's busy share over `interval`.
pub(crate) fn sample_core_usage(cpu: u32, interval: Duration) -> Option<f64> {
    let (total0, idle0) = read_cpu_ticks(cpu)?;
    std::thread::sleep(interval);
    let (total1, idle1) = read_cpu_ticks(cpu)?;
    let total = total1.saturating_sub(total0);
    if total == 0 {
        return None;
    }
    let idle = idle1.saturating_sub(idle0);
    let busy = total.saturating_sub(idle);
    Some(busy as f64 / total as f64 * 100.0)
}

/// Real readings, used by the command itself.
fn real_readings() -> PreflightReadings {
    PreflightReadings {
        governor: read_governor(),
        ac_online: read_ac_online(),
        battery_discharging: read_battery_discharging(),
        pinned_core_usage_percent: sample_core_usage(PIN_CPU, Duration::from_secs(1)),
    }
}

/// DNS-steps preflight (FR1.3): checks prerequisites for the DNS pipeline
/// steps (dns-demo, dns-rtt, dns-cache). Each failure prints the exact fix
/// command in a table and stops early.
///
/// Checks: docker daemon reachable, compose resolver works, needed ports free,
/// kernel module sch_netem available (for dns-rtt), images present or buildable.
pub(crate) fn dns_preflight(docker_available: bool, quick: bool) -> Vec<Refusal> {
    let mut refusals = Vec::new();

    // 1. Docker daemon reachable
    if !docker_available {
        refusals.push(Refusal::DockerNotAvailable);
    }

    // 2. Compose resolver works
    if docker_available {
        // Check if `docker compose version` (plugin) works.
        let plugin_check = Command::new("docker").args(["compose", "version"]).output();
        if plugin_check.map(|o| o.status.success()).unwrap_or(false) {
            // Plugin is available, nothing more to check.
        } else {
            // Check for standalone docker-compose binary.
            let standalone_check = Command::new("docker-compose").output();
            if standalone_check
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                // standalone docker-compose available - OK.
            } else {
                // Check for vendored binary at target/tools/docker-compose.
                let w = std::env::current_dir().unwrap_or_default();
                let vendored = w.join("target/tools/docker-compose");
                if !vendored.is_file() {
                    refusals.push(Refusal::ComposeResolver(
                        "docker compose plugin not available, \
                         standalone docker-compose not found, \
                         and vendored binary missing; \
                         install with: pacman -S docker-compose || apt-get install docker-compose-v2".to_string(),
                    ));
                }
                // Note: SHA-256 verification is handled by compose_binary() in dns_demo.rs
            }
        }
    }

    // 3. Kernel module sch_netem available (for dns-rtt)
    // Check /proc/net/netem or tc availability
    if !quick {
        let netem_output = Command::new("tc")
            .arg("qdisc")
            .arg("show")
            .output()
            .unwrap_or_else(|_| std::process::Output {
                stdout: b"".to_vec(),
                stderr: b"".to_vec(),
                status: std::process::ExitStatus::default(),
            });
        if netem_output.status.success() {
            // netem is available, nothing to refuse
        } else {
            refusals.push(Refusal::NetemNotAvailable);
        }
    }

    refusals
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StepRecord {
    /// `ok`, `failed` or `skipped`.
    pub status: String,
    pub seconds_millis: u64,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RunState {
    pub version: u32,
    /// `quick` or `full`; a state file from another mode is not resumable.
    pub mode: String,
    pub started_at: String,
    #[serde(default)]
    pub steps: BTreeMap<String, StepRecord>,
}

impl RunState {
    fn new(mode: &str) -> Self {
        RunState {
            version: 1,
            mode: mode.to_string(),
            started_at: timestamp(),
            steps: BTreeMap::new(),
        }
    }
}

fn mode_name(quick: bool) -> &'static str {
    if quick {
        "quick"
    } else {
        "full"
    }
}

/// Index of the first step a resumed run must execute. Skipped steps never
/// block resume; a recorded `ok` step before the first gap is reused. Records
/// at and after the returned index are discarded, so a re-measured input
/// invalidates every downstream artifact.
pub(crate) fn resume_start(steps: &[Step], state: &RunState, resume: bool) -> usize {
    if !resume {
        return 0;
    }
    for (index, step) in steps.iter().enumerate() {
        if step.skip.is_some() {
            continue;
        }
        match state.steps.get(step.id.key()) {
            Some(record) if record.status == "ok" => {}
            _ => return index,
        }
    }
    steps.len()
}

fn load_state(path: &Path) -> Result<Option<RunState>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read final-run state {}", path.display()))?;
    let state = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse final-run state {}", path.display()))?;
    Ok(Some(state))
}

fn save_state(path: &Path, state: &RunState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(state).context("failed to serialize final-run state")?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

fn execute_step(step: &Step) -> Result<()> {
    let mut command = match &step.invocation {
        Invocation::Xtask(args) => {
            let exe =
                std::env::current_exe().context("failed to locate the running xtask binary")?;
            let mut c = Command::new(exe);
            c.args(args);
            c
        }
        Invocation::Bench(target) => build_bench_command(target, "", None, resolve_pin_cpu(true)),
        Invocation::Script(args) => {
            let mut c = Command::new("bash");
            c.args(args);
            c
        }
        Invocation::ClassicalCheck(path) => {
            check_classical_lengths(Path::new(path))?;
            return Ok(());
        }
        Invocation::ReplicaCompare { primary, directory } => {
            let replica =
                crate::replication::find_replica(Path::new(directory))?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "no replica climb-results.json found under {directory}; \
                     remove the step from the plan or add a second host's results"
                    )
                })?;
            let exe =
                std::env::current_exe().context("failed to locate the running xtask binary")?;
            let mut c = Command::new(exe);
            c.args([
                "replicate-compare",
                "--a",
                primary,
                "--b",
                replica
                    .to_str()
                    .context("replica path is not valid UTF-8")?,
            ]);
            c
        }
    };
    for (key, value) in &step.env {
        command.env(key, value);
    }
    proc::run(
        &mut command,
        Duration::from_secs(7200),
        &format!("final-run step {}", step.id.label()),
    )?;
    Ok(())
}

/// Run the plan from `start`, recording every outcome. `run` is injected so a
/// test can fail a step without spawning a process.
pub(crate) fn execute_plan(
    steps: &[Step],
    start: usize,
    state: &mut RunState,
    state_path: &Path,
    mut run: impl FnMut(&Step) -> Result<()>,
    mut report: impl FnMut(&str),
) -> Result<()> {
    // A resumed run drops every record from the restart point onward: the
    // step about to run may change its inputs, so nothing after it is fresh.
    for step in &steps[start..] {
        state.steps.remove(step.id.key());
    }

    let total = steps.len();
    for (offset, step) in steps[start..].iter().enumerate() {
        let index = start + offset + 1;
        if let Some(reason) = step.skip {
            report(&format!("[{index}/{total}] {} {reason}", step.id.label()));
            state.steps.insert(
                step.id.key().to_string(),
                StepRecord {
                    status: "skipped".to_string(),
                    seconds_millis: 0,
                    finished_at: timestamp(),
                },
            );
            save_state(state_path, state)?;
            continue;
        }

        report(&format!(
            "[{index}/{total}] {} starting (expected {})",
            step.id.label(),
            step.id.expected(state.mode == "quick")
        ));
        let started = Instant::now();
        match run(step) {
            Ok(()) => {
                let elapsed = started.elapsed();
                report(&format!(
                    "[{index}/{total}] {} ok ({:.1}s)",
                    step.id.label(),
                    elapsed.as_secs_f64()
                ));
                state.steps.insert(
                    step.id.key().to_string(),
                    StepRecord {
                        status: "ok".to_string(),
                        seconds_millis: elapsed.as_millis() as u64,
                        finished_at: timestamp(),
                    },
                );
                save_state(state_path, state)?;
            }
            Err(error) => {
                let elapsed = started.elapsed();
                report(&format!(
                    "[{index}/{total}] {} FAILED ({:.1}s)",
                    step.id.label(),
                    elapsed.as_secs_f64()
                ));
                state.steps.insert(
                    step.id.key().to_string(),
                    StepRecord {
                        status: "failed".to_string(),
                        seconds_millis: elapsed.as_millis() as u64,
                        finished_at: timestamp(),
                    },
                );
                save_state(state_path, state)?;
                return Err(error).with_context(|| {
                    format!(
                        "final-run stopped at `{}`; rerun with --resume to continue",
                        step.id.label()
                    )
                });
            }
        }
    }
    Ok(())
}

/// Render the final table from the recorded state, in plan order.
pub(crate) fn summary_lines(steps: &[Step], state: &RunState) -> Vec<String> {
    let header = format!("{:<26} {:<12} {:>10}", "step", "status", "seconds");
    let mut lines = vec![header.clone(), "-".repeat(header.len())];
    for step in steps {
        let (status, seconds) = match step.skip {
            Some(_) => ("skipped", "—".to_string()),
            None => match state.steps.get(step.id.key()) {
                Some(record) => (
                    record.status.as_str(),
                    format!("{:.1}", record.seconds_millis as f64 / 1000.0),
                ),
                None => ("not run", "—".to_string()),
            },
        };
        lines.push(format!(
            "{:<26} {:<12} {:>10}",
            step.id.label(),
            status,
            seconds
        ));
    }
    lines
}

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

/// Civil date from days since the Unix epoch (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DDTHH:MM:SSZ`, from the system clock.
pub(crate) fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3600;
    let minute = rem % 3600 / 60;
    let second = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// ---------------------------------------------------------------------------
// Classical-baseline guard
// ---------------------------------------------------------------------------

/// Require the classical signature-length document the classical bench writes.
/// A missing or empty document means the full-signature baseline never ran,
/// so consolidation would silently drop every classical context row.
pub(crate) fn check_classical_lengths(path: &Path) -> Result<usize> {
    if !path.exists() {
        bail!(
            "missing classical signature lengths {} (run `cargo bench -p climb-bench --bench classical`)",
            path.display()
        );
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let count = match &value {
        serde_json::Value::Object(map) => map.len(),
        serde_json::Value::Array(rows) => rows.len(),
        _ => 0,
    };
    if count == 0 {
        bail!(
            "{} carries no classical signature lengths (run `cargo bench -p climb-bench --bench classical`)",
            path.display()
        );
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

fn docker_available() -> bool {
    let mut command = Command::new("docker");
    command.args(["info", "--format", "{{.ServerVersion}}"]);
    proc::run(&mut command, Duration::from_secs(15), "docker info").is_ok()
}

/// A second host's consolidated results, if data/replication holds one.
fn second_host_present() -> bool {
    let Ok(entries) = fs::read_dir("data/replication") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_dir() && path.join("climb-results.json").exists()
    })
}

pub(crate) fn cmd_final_run(args: FinalRunArgs) -> Result<()> {
    fs::create_dir_all(STATE_DIR).with_context(|| format!("failed to create {STATE_DIR}"))?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)
        .with_context(|| format!("failed to open {LOG_FILE}"))?;
    let mut report = |line: &str| {
        println!("{line}");
        let _ = writeln!(log, "{line}");
    };

    // Preflight runs before any step, including the environment fingerprint.
    let readings = real_readings();
    let refusals = preflight_refusals(&readings);
    let advisory = args.quick || args.allow_unpinned || args.dry_run;
    for refusal in &refusals {
        if advisory {
            report(&format!(
                "preflight warning (advisory): {}",
                refusal.message()
            ));
        } else {
            report(&format!("preflight refusal: {}", refusal.message()));
        }
    }
    if !refusals.is_empty() && !advisory {
        bail!(
            "final-run preflight refused to start ({} reason(s)); fix them or pass --allow-unpinned for a smoke run",
            refusals.len()
        );
    }
    report(&format!(
        "preflight: governor={} ac_online={:?} battery_discharging={:?} core_usage={:?}%",
        readings.governor,
        readings.ac_online,
        readings.battery_discharging,
        readings.pinned_core_usage_percent.map(|p| p.round())
    ));

    // Compute docker availability for plan (needed for DNS preflight).
    let docker_for_plan = if args.quick || args.skip_dns {
        false
    } else {
        docker_available()
    };

    // --- DNS-steps preflight (FR1.3) ---
    let dns_refusals = dns_preflight(docker_for_plan, args.quick);
    for refusal in &dns_refusals {
        if advisory {
            report(&format!(
                "preflight warning (advisory): {}",
                refusal.message()
            ));
        } else {
            report(&format!("preflight refusal: {}", refusal.message()));
        }
    }
    if !dns_refusals.is_empty() && !advisory {
        bail!(
            "final-run DNS preflight refused to start ({} reason(s)); fix them or pass --skip-dns to use the committed captures",
            dns_refusals.len()
        );
    }

    if !args.dry_run && !args.quick && !args.skip_dns && !docker_for_plan {
        bail!(
            "docker is unavailable; start the Docker daemon or pass --skip-dns to use the committed captures"
        );
    }
    let inputs = PlanInputs {
        quick: args.quick,
        skip_dns: args.skip_dns,
        docker_available: docker_for_plan,
        second_host_present: second_host_present(),
        deck_text: args
            .deck_text
            .as_ref()
            .map(|path| path.display().to_string()),
    };
    let steps = plan(&inputs);

    if args.dry_run {
        print!("{}", render_plan(&steps, args.quick));
        report("dry-run: no step was executed");
        return Ok(());
    }

    let mode = mode_name(args.quick);
    let mut state = match load_state(Path::new(STATE_FILE))? {
        Some(state) if args.resume && state.mode != mode => {
            bail!(
                "final-run state at {STATE_FILE} belongs to a `{}` run; rerun without --resume to start a fresh `{mode}` run",
                state.mode
            );
        }
        Some(state) => state,
        None => RunState::new(mode),
    };
    state.mode = mode.to_string();

    let start = resume_start(&steps, &state, args.resume);
    if args.resume {
        report(&format!(
            "resume: {start} of {} step(s) already complete",
            steps.len()
        ));
    }
    if start >= steps.len() {
        report("final-run: nothing to do, every step is already recorded");
        for line in summary_lines(&steps, &state) {
            report(&line);
        }
        return Ok(());
    }

    report(&format!(
        "final-run starting: mode={mode}, {} step(s), state={STATE_FILE}, log={LOG_FILE}",
        steps.len() - start
    ));

    let outcome = execute_plan(
        &steps,
        start,
        &mut state,
        Path::new(STATE_FILE),
        execute_step,
        &mut report,
    );

    for line in summary_lines(&steps, &state) {
        report(&line);
    }
    outcome
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> PlanInputs {
        PlanInputs {
            quick: false,
            skip_dns: false,
            docker_available: true,
            second_host_present: false,
            deck_text: None,
        }
    }

    fn quick() -> PlanInputs {
        PlanInputs {
            quick: true,
            ..full()
        }
    }

    fn ids(steps: &[Step]) -> Vec<StepId> {
        steps.iter().map(|s| s.id).collect()
    }

    fn find(steps: &[Step], id: StepId) -> &Step {
        steps
            .iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("{id:?} present"))
    }

    fn state_with(entries: &[(StepId, &str)]) -> RunState {
        let mut state = RunState::new("full");
        for (id, status) in entries {
            state.steps.insert(
                id.key().to_string(),
                StepRecord {
                    status: (*status).to_string(),
                    seconds_millis: 0,
                    finished_at: "t".to_string(),
                },
            );
        }
        state
    }

    fn temp_state(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-final-run-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir.join("state.json")
    }

    fn noop(_: &str) {}

    #[test]
    fn the_plan_lists_every_documented_step_in_order() {
        assert_eq!(
            ids(&plan(&full())),
            vec![
                StepId::PinEnv,
                StepId::WireReport,
                StepId::MemoryReport,
                StepId::LadderVerify,
                StepId::BenchSigning,
                StepId::BenchVerifying,
                StepId::BenchPrimitives,
                StepId::BenchFull,
                StepId::Stability,
                StepId::ClassicalBaselines,
                StepId::DnsDemo,
                StepId::DnsRtt,
                StepId::DnsCache,
                StepId::ReplicateSummary,
                StepId::ReplicateCompare,
                StepId::Consolidate,
                StepId::VerifyConsolidated,
                StepId::Charts,
                StepId::ReportHtml,
                StepId::ReportCheck,
                StepId::SampleOutput,
                StepId::DeckFacts,
                StepId::DeckDelta,
                StepId::Headline,
                StepId::VerifyNumbers,
                StepId::DocsCheck,
                StepId::AuditPack,
            ]
        );
    }

    #[test]
    fn step_keys_and_labels_are_unique_and_nonempty() {
        let steps = plan(&full());
        let mut keys: Vec<&str> = steps.iter().map(|s| s.id.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), steps.len());
        assert!(steps.iter().all(|s| !s.id.label().is_empty()));
    }

    #[test]
    fn full_mode_runs_the_dns_pipeline_and_refreshes_the_committed_docs() {
        let steps = plan(&full());
        for id in [
            StepId::DnsDemo,
            StepId::DnsRtt,
            StepId::DnsCache,
            StepId::SampleOutput,
            StepId::Headline,
        ] {
            assert_eq!(find(&steps, id).skip, None, "{id:?} must run in full mode");
        }
        let verify = find(&steps, StepId::VerifyNumbers);
        match &verify.invocation {
            Invocation::Xtask(args) => assert!(args.iter().any(|a| a == "--require-spread")),
            other => panic!("unexpected invocation {other:?}"),
        }
    }

    #[test]
    fn quick_mode_skips_dns_and_never_rewrites_committed_docs() {
        let steps = plan(&quick());
        for id in [
            StepId::DnsDemo,
            StepId::DnsRtt,
            StepId::DnsCache,
            StepId::SampleOutput,
            StepId::Headline,
        ] {
            assert!(
                find(&steps, id).skip.is_some(),
                "{id:?} must skip in quick mode"
            );
        }
        match &find(&steps, StepId::DeckFacts).invocation {
            Invocation::Xtask(args) => {
                let out = args
                    .iter()
                    .position(|a| a == "--out")
                    .expect("--out present");
                assert_eq!(args[out + 1], QUICK_SHEET);
            }
            other => panic!("unexpected invocation {other:?}"),
        }
        let verify = find(&steps, StepId::VerifyNumbers);
        match &verify.invocation {
            Invocation::Xtask(args) => assert!(!args.iter().any(|a| a == "--require-spread")),
            other => panic!("unexpected invocation {other:?}"),
        }
    }

    #[test]
    fn deck_delta_is_skipped_without_deck_text_and_wired_when_supplied() {
        let without = plan(&full());
        let skipped = find(&without, StepId::DeckDelta);
        assert!(skipped.skip.is_some());
        assert!(skipped.skip.unwrap().contains("--deck-text"));

        let with = plan(&PlanInputs {
            deck_text: Some("target/deck.txt".to_string()),
            ..full()
        });
        let step = find(&with, StepId::DeckDelta);
        assert_eq!(step.skip, None);
        match &step.invocation {
            Invocation::Xtask(args) => {
                assert_eq!(args[0], "deck-delta");
                let at = args.iter().position(|a| a == "--out").expect("--out");
                assert_eq!(args[at + 1], "docs/deck-delta.md");
                let sheet = args
                    .iter()
                    .position(|a| a == "--quote-sheet")
                    .expect("sheet");
                assert_eq!(args[sheet + 1], "docs/deck-facts.md");
            }
            other => panic!("unexpected invocation {other:?}"),
        }

        let quick_steps = plan(&PlanInputs {
            quick: true,
            deck_text: Some("target/deck.txt".to_string()),
            ..full()
        });
        match &find(&quick_steps, StepId::DeckDelta).invocation {
            Invocation::Xtask(args) => {
                let at = args.iter().position(|a| a == "--out").expect("--out");
                assert_eq!(args[at + 1], QUICK_DELTA);
                let sheet = args
                    .iter()
                    .position(|a| a == "--quote-sheet")
                    .expect("sheet");
                assert_eq!(args[sheet + 1], QUICK_SHEET);
            }
            other => panic!("unexpected invocation {other:?}"),
        }
    }

    #[test]
    fn quick_mode_measures_small_and_medium_but_benchmarks_every_size() {
        let steps = plan(&quick());
        for id in [StepId::WireReport, StepId::MemoryReport] {
            match &find(&steps, id).invocation {
                Invocation::Xtask(args) => {
                    let at = args.iter().position(|a| a == "--sizes").expect("--sizes");
                    assert_eq!(args[at + 1], SIZES_QUICK);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        // The signing bound needs the non-amortised baseline at every
        // comparison size, so the bench targets are not size-filtered.
        assert_eq!(
            find(&steps, StepId::BenchSigning).invocation,
            Invocation::Bench("signing".to_string())
        );
        assert_eq!(
            find(&steps, StepId::BenchVerifying).invocation,
            Invocation::Bench("verifying".to_string())
        );
    }

    #[test]
    fn full_mode_uses_the_three_blueprint_sizes() {
        let steps = plan(&full());
        for id in [StepId::WireReport, StepId::MemoryReport] {
            match &find(&steps, id).invocation {
                Invocation::Xtask(args) => {
                    let at = args.iter().position(|a| a == "--sizes").expect("--sizes");
                    assert_eq!(args[at + 1], SIZES_FULL);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn stability_runs_five_repeats_across_three_sizes() {
        let steps = plan(&full());
        match &find(&steps, StepId::Stability).invocation {
            Invocation::Xtask(args) => {
                let sizes = args.iter().position(|a| a == "--sizes").expect("--sizes");
                assert_eq!(args[sizes + 1], STABILITY_SIZES);
                let runs = args.iter().position(|a| a == "--runs").expect("--runs");
                assert_eq!(args[runs + 1], "5");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_second_host_turns_replicate_compare_from_a_skip_into_a_run() {
        let without = plan(&full());
        assert!(find(&without, StepId::ReplicateCompare).skip.is_some());
        let with = plan(&PlanInputs {
            second_host_present: true,
            ..full()
        });
        assert_eq!(find(&with, StepId::ReplicateCompare).skip, None);
    }

    #[test]
    fn skip_dns_uses_the_recorded_fixtures() {
        let steps = plan(&PlanInputs {
            skip_dns: true,
            ..full()
        });
        for id in [StepId::DnsDemo, StepId::DnsRtt, StepId::DnsCache] {
            let step = find(&steps, id);
            assert!(step.skip.is_some());
            assert!(step.skip.unwrap().contains("recorded fixture"));
        }
    }

    #[test]
    fn docker_unavailable_falls_back_to_fixtures_without_failing_the_plan() {
        let steps = plan(&PlanInputs {
            docker_available: false,
            ..full()
        });
        assert!(find(&steps, StepId::DnsDemo).skip.is_some());
        assert_eq!(find(&steps, StepId::Consolidate).skip, None);
    }

    #[test]
    fn dry_run_renders_every_step_and_its_expected_duration() {
        let steps = plan(&full());
        let rendered = render_plan(&steps, false);
        assert!(rendered.contains(&format!("{} step(s)", steps.len())));
        for step in &steps {
            assert!(
                rendered.contains(step.id.label()),
                "missing {}",
                step.id.label()
            );
        }
        assert!(rendered.contains("expected"));
    }

    #[test]
    fn a_fresh_run_starts_at_the_first_step() {
        let steps = plan(&full());
        let state = RunState::new("full");
        assert_eq!(resume_start(&steps, &state, false), 0);
        assert_eq!(resume_start(&steps, &state, true), 0);
    }

    #[test]
    fn resume_skips_every_step_recorded_ok() {
        let steps = plan(&full());
        let mut state = RunState::new("full");
        for step in steps.iter().take(5) {
            state.steps.insert(
                step.id.key().to_string(),
                StepRecord {
                    status: "ok".to_string(),
                    seconds_millis: 1,
                    finished_at: "t".to_string(),
                },
            );
        }
        assert_eq!(resume_start(&steps, &state, true), 5);
    }

    #[test]
    fn a_failure_in_the_middle_restarts_at_the_failed_step() {
        let steps = plan(&full());
        let state = state_with(&[
            (StepId::PinEnv, "ok"),
            (StepId::WireReport, "ok"),
            (StepId::MemoryReport, "failed"),
            // A stale downstream record must not be reused.
            (StepId::BenchSigning, "ok"),
        ]);
        assert_eq!(resume_start(&steps, &state, true), 2);
    }

    #[test]
    fn a_plan_change_reruns_a_previously_skipped_step_that_is_now_runnable() {
        // Full plan: dns-demo is runnable. A quick state marked it skipped;
        // resuming a full run must not treat that as complete.
        let steps = plan(&full());
        let state = state_with(&[
            (StepId::PinEnv, "ok"),
            (StepId::WireReport, "ok"),
            (StepId::DnsDemo, "skipped"),
        ]);
        let start = resume_start(&steps, &state, true);
        assert_eq!(steps[start].id, StepId::MemoryReport);
    }

    #[test]
    fn a_fully_recorded_run_has_nothing_to_resume() {
        let steps = plan(&full());
        let mut state = RunState::new("full");
        for step in &steps {
            state.steps.insert(
                step.id.key().to_string(),
                StepRecord {
                    status: "ok".to_string(),
                    seconds_millis: 1,
                    finished_at: "t".to_string(),
                },
            );
        }
        assert_eq!(resume_start(&steps, &state, true), steps.len());
    }

    #[test]
    fn executing_to_a_failure_records_ok_then_failed_and_resumes_there() {
        let steps = plan(&full());
        let path = temp_state("failure");
        let mut state = RunState::new("full");
        let mut attempts = 0;
        let outcome = execute_plan(
            &steps,
            0,
            &mut state,
            &path,
            |step| {
                attempts += 1;
                if step.id == StepId::MemoryReport {
                    bail!("injected failure");
                }
                Ok(())
            },
            noop,
        );
        assert!(outcome.is_err());
        assert_eq!(state.steps[StepId::PinEnv.key()].status, "ok");
        assert_eq!(state.steps[StepId::WireReport.key()].status, "ok");
        assert_eq!(state.steps[StepId::MemoryReport.key()].status, "failed");
        assert_eq!(attempts, 3);
        assert_eq!(resume_start(&steps, &state, true), 2);
        // The state file on disk matches the in-memory state.
        let reloaded = load_state(&path).expect("load").expect("present");
        assert_eq!(reloaded, state);
    }

    #[test]
    fn resuming_discards_stale_downstream_records() {
        let steps = plan(&full());
        let path = temp_state("stale");
        let mut state = state_with(&[
            (StepId::PinEnv, "ok"),
            (StepId::WireReport, "failed"),
            (StepId::MemoryReport, "ok"),
        ]);
        execute_plan(&steps, 1, &mut state, &path, |_| Ok(()), noop).expect("run");
        // Everything from the restart point is recorded afresh.
        assert_eq!(state.steps[StepId::WireReport.key()].status, "ok");
        assert_eq!(state.steps[StepId::MemoryReport.key()].status, "ok");
        assert_eq!(state.steps[StepId::PinEnv.key()].status, "ok");
    }

    #[test]
    fn a_skipped_step_is_recorded_but_does_not_block_resume() {
        let steps = plan(&quick());
        let path = temp_state("skip");
        let mut state = RunState::new("quick");
        execute_plan(&steps, 0, &mut state, &path, |_| Ok(()), noop).expect("run");
        let dns = state.steps.get(StepId::DnsDemo.key()).expect("recorded");
        assert_eq!(dns.status, "skipped");
        assert_eq!(resume_start(&steps, &state, true), steps.len());
    }

    #[test]
    fn preflight_refuses_an_unpinned_governor() {
        let readings = PreflightReadings {
            governor: "powersave".to_string(),
            ac_online: Some(true),
            battery_discharging: Some(false),
            pinned_core_usage_percent: Some(1.0),
        };
        assert_eq!(
            preflight_refusals(&readings),
            vec![Refusal::GovernorNotPinned {
                found: "powersave".to_string()
            }]
        );
    }

    #[test]
    fn preflight_refuses_battery_power() {
        let readings = PreflightReadings {
            governor: TARGET_GOVERNOR.to_string(),
            ac_online: Some(false),
            battery_discharging: Some(true),
            pinned_core_usage_percent: Some(1.0),
        };
        assert_eq!(preflight_refusals(&readings), vec![Refusal::OnBattery]);
    }

    #[test]
    fn preflight_refuses_a_busy_core() {
        let readings = PreflightReadings {
            governor: TARGET_GOVERNOR.to_string(),
            ac_online: Some(true),
            battery_discharging: Some(false),
            pinned_core_usage_percent: Some(25.0),
        };
        assert_eq!(
            preflight_refusals(&readings),
            vec![Refusal::CoreBusy { percent: 25.0 }]
        );
    }

    #[test]
    fn preflight_passes_on_a_quiet_pinned_mains_host() {
        let readings = PreflightReadings {
            governor: TARGET_GOVERNOR.to_string(),
            ac_online: Some(true),
            battery_discharging: Some(false),
            pinned_core_usage_percent: Some(3.0),
        };
        assert!(preflight_refusals(&readings).is_empty());
    }

    #[test]
    fn preflight_tolerates_unknown_readings_rather_than_guessing() {
        let readings = PreflightReadings {
            governor: TARGET_GOVERNOR.to_string(),
            ac_online: None,
            battery_discharging: None,
            pinned_core_usage_percent: None,
        };
        assert!(preflight_refusals(&readings).is_empty());
    }

    #[test]
    fn refusal_messages_name_the_fix() {
        assert!(Refusal::GovernorNotPinned {
            found: "powersave".into()
        }
        .message()
        .contains("pin-perf-governor.sh performance"));
        assert!(Refusal::OnBattery.message().contains("mains"));
        assert!(Refusal::CoreBusy { percent: 50.0 }
            .message()
            .contains("50% busy"));
    }

    #[test]
    fn classical_lengths_must_exist_and_be_nonempty() {
        let dir = std::env::temp_dir().join(format!("climb-classical-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let missing = dir.join("missing.json");
        assert!(check_classical_lengths(&missing).is_err());
        let empty = dir.join("empty.json");
        fs::write(&empty, "{}").expect("write");
        assert!(check_classical_lengths(&empty).is_err());
        let valid = dir.join("valid.json");
        fs::write(&valid, r#"{"ed25519":64,"p256":64}"#).expect("write");
        assert_eq!(check_classical_lengths(&valid).expect("valid"), 2);
    }

    #[test]
    fn timestamps_are_iso_8601_utc() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), 20);
        assert!(stamp.ends_with('Z'));
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
    }

    #[test]
    fn the_summary_table_shows_recorded_statuses_and_dashes_skips() {
        let steps = plan(&quick());
        let mut state = RunState::new("quick");
        state.steps.insert(
            StepId::PinEnv.key().to_string(),
            StepRecord {
                status: "ok".to_string(),
                seconds_millis: 1500,
                finished_at: "t".to_string(),
            },
        );
        let lines = summary_lines(&steps, &state);
        assert!(lines[0].starts_with("step"));
        assert!(lines
            .iter()
            .any(|l| l.starts_with("pin-env") && l.contains("1.5")));
        assert!(lines
            .iter()
            .any(|l| l.starts_with("dns-demo") && l.contains("skipped")));
        assert!(lines
            .iter()
            .any(|l| l.starts_with("bench-signing") && l.contains("not run")));
    }
}
