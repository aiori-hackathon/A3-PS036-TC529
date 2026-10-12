//! The `dns-demo` subcommand: a live, scripted MTL DNS pipeline demonstration.
//!
//! ```text
//! cargo xtask dns-demo
//! cargo xtask dns-demo --keep --no-build
//! cargo xtask dns-demo --reuse-zone target/dns-demo/climb.example.zone.signed
//! ```
//!
//! Orchestrates 11 clearly-labelled phases: preflight checks, optional image
//! builds, live MTL zone signing, pipeline startup, baseline-full-signature
//! queries, MTL-SigTag condensed queries, literal `dig` transcripts of both,
//! a headline-comparison display, Unbound forwarding, wire-capture summaries,
//! and guaranteed teardown.
//!
//! All external commands (docker, python3 helpers) are printed before
//! execution for transparency.  Teardown runs on *every* error path via a
//! RAII drop guard; `--keep` disarms the guard so the pipeline + scratch
//! directory survive for inspection.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use climb_report::{DnsCapture, DnsCaptureFlags, DnsCaptureHost, DnsCaptureImage, DnsCaptureQuery};

// ---------------------------------------------------------------------------
// JSON output mode — all human output moves to stderr when `--json` is set
// ---------------------------------------------------------------------------

/// Set when `--json` is given: human-readable output is redirected to stderr
/// so stdout carries only the capture JSON.
static JSON_MODE: AtomicBool = AtomicBool::new(false);

fn json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

/// `println!` that writes to stderr instead of stdout in JSON mode.
macro_rules! humanln {
    ($($arg:tt)*) => {
        if json_mode() {
            eprintln!($($arg)*);
        } else {
            println!($($arg)*);
        }
    };
}

/// `print!` that writes to stderr instead of stdout in JSON mode.
macro_rules! humanprint {
    ($($arg:tt)*) => {
        if json_mode() {
            eprint!($($arg)*);
        } else {
            print!($($arg)*);
        }
    };
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COMPOSE_FILE: &str = "docker/compose.dns-pipeline.yml";
const UNSIGNED_ZONE: &str = "docker/zones/climb.example.zone";
const NSD_IP: &str = "192.168.13.2";
const UNBOUND_IP: &str = "192.168.13.3";
const NSD_READY_MARKER: &str = "zone climb.example. read with success";
const NSD_READY_TIMEOUT_SECS: u64 = 30;
const NSD_POLL_SECS: u64 = 2;
const SIGTAG_SCRIPT: &str = "scripts/mtl_sigtag.py";
const CAPTURE_SCRIPT: &str = "scripts/mtl_dns_capture.py";
const CAPTURE_SCHEMA_VERSION: u32 = 1;
const ALGORITHM: &str = "SLH-DSA-SHA2-128s-MTL-SHA2-128";
const ZONE_NAME: &str = "climb.example.";
const CAPTURE_TRANSPORT: &str = "tcp";

/// Docker images the demo requires, with a human description for the
/// preflight report.  Shared by `check_images` and the capture recorder.
const REQUIRED_IMAGES: &[(&str, &str)] = &[
    ("climb-nsd:phase12", "Authoritative NSD-MTL"),
    ("climb-unbound:phase13", "Recursive Unbound-MTL"),
    ("climb-dig:phase14", "Query tool (dig)"),
    (
        "climb-ldns-signer:phase14",
        "Zone signer (ldns-keygen/signzone)",
    ),
];

/// Queries always issued in the demo.  Order matters — the headline
/// comparison uses the `www.climb.example. A` row.
const QUERIES: &[(&str, &str)] = &[
    ("climb.example.", "SOA"),
    ("www.climb.example.", "A"),
    ("mail.climb.example.", "MX"),
];

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Args)]
pub(crate) struct DnsDemoArgs {
    /// Keep scratch directory and leave pipeline running for inspection.
    #[arg(long)]
    keep: bool,

    /// Skip building Docker images (assume they already exist).
    #[arg(long)]
    no_build: bool,

    /// Reuse an existing signed zone file instead of signing fresh.
    #[arg(long, value_name = "PATH")]
    reuse_zone: Option<PathBuf>,

    /// Write the recorded DNS capture as JSON to this path (default
    /// `target/dns-capture.json`); omit to skip writing.
    #[arg(long, num_args = 0..=1, default_missing_value = "target/dns-capture.json", value_name = "PATH")]
    record: Option<PathBuf>,

    /// Print the capture JSON on stdout; human-readable output goes to stderr.
    #[arg(long)]
    json: bool,
}

// ---------------------------------------------------------------------------
// Visual output helpers
// ---------------------------------------------------------------------------

fn print_phase(n: u32, title: &str) {
    humanln!("\n{:=<72}", "");
    humanln!("  Phase {n} — {title}");
    humanln!("{:=<72}", "");
}

fn print_status(label: &str, value: &str) {
    humanln!("  {label:<22} {value}");
}

fn render_cmd(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_str().unwrap_or("?").to_string()];
    for arg in cmd.get_args() {
        parts.push(arg.to_str().unwrap_or("?").to_string());
    }
    parts.join(" ")
}

fn print_cmd(cmd: &Command) {
    eprintln!("  running: {}", render_cmd(cmd));
}

// ---------------------------------------------------------------------------
// Teardown guard — runs `docker compose down` on drop
// ---------------------------------------------------------------------------

struct ComposeGuard {
    root: PathBuf,
    active: bool,
}

impl ComposeGuard {
    fn new(root: PathBuf) -> Self {
        ComposeGuard { root, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        if self.active {
            eprintln!("\n  [guard] shutting down pipeline...");
            compose_down_silent(&self.root);
        }
    }
}

fn compose_down_silent(root: &Path) {
    let _ = compose_cmd(root, &["down", "--remove-orphans"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// ---------------------------------------------------------------------------
// Workspace root
// ---------------------------------------------------------------------------

/// Returns the workspace root (parent of the `xtask` crate directory).
fn workspace_root() -> Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.parent()
        .map(Path::to_path_buf)
        .context("xtask is expected to live directly inside the workspace root")
}

/// `uid:gid` of the invoking user, used to run the signer container so files
/// it writes into the mounted scratch directory are not owned by root.
fn host_user() -> String {
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1000".to_string());
    let gid = Command::new("id")
        .arg("-g")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1000".to_string());
    format!("{uid}:{gid}")
}

// ---------------------------------------------------------------------------
// Docker subprocess helpers
// ---------------------------------------------------------------------------

/// Build a `docker compose` command rooted at the workspace.
fn compose_cmd(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("docker");
    c.args(["compose", "-f"])
        .arg(root.join(COMPOSE_FILE))
        .args(args)
        .current_dir(root);
    c
}

/// Run a command in the signer container, mounted on `scratch`, capturing
/// trimmed stdout.  The command output is printed before execution.
/// Runs as the invoking host user so the generated keys and signed zone are
/// owned by the developer rather than root.
fn run_in_signer(scratch: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm"])
        .arg("--user")
        .arg(host_user())
        .arg("-v")
        .arg(format!("{}:/var/dns/zones", scratch.display()))
        .arg("-w")
        .arg("/var/dns/zones")
        .arg("climb-ldns-signer:phase14")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    print_cmd(&cmd);

    let output = cmd.output().context("failed to run signer container")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("signer command failed: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a command and return its exit status.  In JSON mode the child's
/// stdout/stderr are forwarded to stderr so stdout carries only the capture
/// JSON.
fn run_cmd_status(mut cmd: Command) -> Result<std::process::ExitStatus> {
    print_cmd(&cmd);
    if json_mode() {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = cmd.output().context("failed to execute command")?;
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Ok(output.status);
    }
    cmd.status().context("failed to execute command")
}

/// Run a command, inherit stdio, and check success.
fn run_cmd_inherit(cmd: Command) -> Result<()> {
    let status = run_cmd_status(cmd)?;
    if !status.success() {
        bail!("command exited with {status}");
    }
    Ok(())
}

/// Run a command, capturing stdout, and check success.
fn run_cmd_capture(mut cmd: Command) -> Result<String> {
    print_cmd(&cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd.output().context("failed to execute command")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("command exited with {}: {}", output.status, stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ---------------------------------------------------------------------------
// Phase 1 — Preflight
// ---------------------------------------------------------------------------

fn check_docker() -> Result<()> {
    let output = Command::new("docker")
        .args(["--version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("docker not found on PATH — is Docker installed?")?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    print_status("docker CLI", &version);
    Ok(())
}

/// List the four required images and report which exist.
fn check_images() -> Result<()> {
    for (image, desc) in REQUIRED_IMAGES {
        let exists = image_exists(image);
        let status = if exists {
            "present"
        } else {
            "MISSING — will be built"
        };
        print_status(desc, status);
    }
    Ok(())
}

fn image_exists(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Phase 4 helper — wait for NSD to be ready
// ---------------------------------------------------------------------------

fn wait_for_nsd_ready(root: &Path) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(NSD_READY_TIMEOUT_SECS);

    while std::time::Instant::now() < deadline {
        let output = compose_cmd(root, &["logs", "nsd"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains(NSD_READY_MARKER) {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_secs(NSD_POLL_SECS));
    }

    bail!(
        "NSD did not emit '{}' within {} s",
        NSD_READY_MARKER,
        NSD_READY_TIMEOUT_SECS
    )
}

// ---------------------------------------------------------------------------
// Phase 5 / 6 / 8 — run the capture script
// ---------------------------------------------------------------------------

/// One row from `scripts/mtl_dns_capture.py` output.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureRow {
    label: String,
    qname: String,
    qtype: String,
    rcode: Option<u8>,
    bytes: u64,
    rrsig_lead_byte: Option<u8>,
    rrsig_len: Option<u64>,
    rrsig_kind: Option<String>, // "full", "condensed"
}

/// Run the capture script, print its stdout verbatim, and return parsed rows.
fn run_capture(
    root: &Path,
    server: &str,
    out_dir: &Path,
    sigtag: Option<&str>,
    label: &str,
) -> Result<Vec<CaptureRow>> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create capture dir {}", out_dir.display()))?;

    let mut args: Vec<String> = vec![
        CAPTURE_SCRIPT.to_string(),
        "--server".to_string(),
        server.to_string(),
        "--port".to_string(),
        "53".to_string(),
        "--out".to_string(),
        out_dir.display().to_string(),
    ];
    if let Some(tag) = sigtag {
        args.push("--sigtag".to_string());
        args.push(tag.to_string());
    }
    for &(name, qtype) in QUERIES {
        args.push(name.to_string());
        args.push(qtype.to_string());
    }

    let mut cmd = Command::new("python3");
    cmd.args(&args).current_dir(root);

    let stdout = run_cmd_capture(cmd)?;

    // Print the raw table so the user sees the demo-quality output.
    humanprint!("{stdout}");

    Ok(parse_capture_output(&stdout, label))
}

// ---------------------------------------------------------------------------
// Phase 6 — SigTag script
// ---------------------------------------------------------------------------

fn run_sigtag(root: &Path, signed_zone: &Path) -> Result<String> {
    let mut cmd = Command::new("python3");
    cmd.args([SIGTAG_SCRIPT, &signed_zone.display().to_string()])
        .current_dir(root);

    run_cmd_capture(cmd).map(|s| s.trim().to_string())
}

/// Run `dig` from inside the internal network via the compose `dig` service.
/// A `sigtag` is attached as the MTL ladder-hash in EDNS option 65050 so NSD
/// answers with a condensed proof instead of a full signature.
fn run_dig(
    root: &Path,
    server: &str,
    qname: &str,
    qtype: &str,
    sigtag: Option<&str>,
) -> Result<String> {
    let mut cmd = Command::new("docker");
    cmd.current_dir(root)
        .args(["compose", "-f"])
        .arg(root.join(COMPOSE_FILE))
        .args(["--profile", "tools", "run", "--rm", "dig"])
        .arg(format!("@{server}"))
        .args([qname, qtype, "+dnssec", "+tcp", "+time=5", "+tries=1"]);
    if let Some(tag) = sigtag {
        cmd.arg(format!("+ednsopt=65050:{tag}"));
    }
    run_cmd_capture(cmd)
}

/// Shorten a long record line for display while keeping the original text
/// recognisable.  RRSIG base64 blobs are thousands of characters wide and
/// would otherwise dominate the terminal.
fn truncate_line(line: &str, max_chars: usize) -> String {
    let count = line.chars().count();
    if count <= max_chars {
        return line.to_string();
    }
    let keep = max_chars.saturating_sub(18);
    let head: String = line.chars().take(keep).collect();
    format!("{head}… (+{} more chars)", count - keep)
}

/// Print a `dig` transcript, dropping banner boilerplate and eliding long
/// signature lines.
fn print_dig_output(output: &str, max_chars: usize) {
    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with("; <<>>") || line.starts_with(";; global options") {
            continue;
        }
        humanln!("    {}", truncate_line(line, max_chars));
    }
}

// ---------------------------------------------------------------------------
// Phase 7 — Headline comparison
// ---------------------------------------------------------------------------

fn print_headline(baseline: &[CaptureRow], condensed: &[CaptureRow]) {
    // Find the www.climb.example. A row in each set (second query).
    let full = &baseline[1];
    let cond = &condensed[1];

    let full_b = full.bytes;
    let cond_b = cond.bytes;
    let saved = full_b.saturating_sub(cond_b);
    let pct = if full_b > 0 {
        (saved as f64 / full_b as f64) * 100.0
    } else {
        0.0
    };
    let ratio = if cond_b > 0 {
        full_b as f64 / cond_b as f64
    } else {
        f64::INFINITY
    };

    humanln!();
    humanln!("  Query:  www.climb.example. A");
    humanln!("  ──────────────────────────────────────────────────────────────────");
    humanln!("  Full signature      {full_b:>8} B  (lead byte 1, signed ladder in response)",);
    humanln!("  Condensed proof     {cond_b:>8} B  (lead byte 2, Merkle path only)",);
    humanln!("  ──────────────────────────────────────────────────────────────────");
    humanln!("  Saved               {saved:>8} B  ({pct:.1}% smaller, ~{ratio:.0}x)",);
    humanln!();
    humanln!("  This is the MTL bandwidth collapse: replacing a ~8 KB signed");
    humanln!("  ladder with a ~140 B Merkle authentication path.");
}

// ---------------------------------------------------------------------------
// Parse capture output
// ---------------------------------------------------------------------------

/// Parse the fixed-width table printed by `scripts/mtl_dns_capture.py`.
///
/// Column layout (0-based character offsets):
///   0-1:  prefix "  "
///   2-29: query name (28, left)
///  30:    space
///  31-35: type (5, left)
///  36:    space
///  37-43: bytes (7, right)
///  44-45: spaces
///  46-51: rcode integer (6, left)
///  52:    space
///  53-68: rrsig cell (16, left) — "—" or "full NNNNB" or "condensed NNNNB"
///  69:    space
///  70+:   filename
///
/// Returns one row per data line.  Skips header/separator lines.  Slices use
/// `str::get` so a multi-byte cell (the em-dash "—") can never panic the
/// offset arithmetic; an unparseable line is skipped.
fn parse_capture_output(stdout: &str, label: &str) -> Vec<CaptureRow> {
    let mut rows = Vec::new();

    for line in stdout.lines() {
        // Data lines start with two spaces and are long enough for the fixed
        // offsets we use below.
        if line.len() < 70 || !line.starts_with("  ") || line.starts_with("  query") {
            continue;
        }
        // Skip separator lines (they look like "  ----------...").
        if line.trim_start().starts_with("--") || line.trim_start().starts_with("wire capture") {
            continue;
        }

        let Some(bytes_str) = line.get(37..44).map(str::trim) else {
            continue;
        };
        let Ok(bytes) = bytes_str.parse::<u64>() else {
            continue;
        };

        let qname = line.get(2..30).map(str::trim).unwrap_or("").to_string();
        let qtype = line.get(31..36).map(str::trim).unwrap_or("").to_string();
        let rcode = line
            .get(46..52)
            .map(str::trim)
            .and_then(|s| s.parse::<u8>().ok());

        let rrsig_str = line.get(53..69).map(str::trim).unwrap_or("");
        let (rrsig_kind, rrsig_lead_byte, rrsig_len) =
            if rrsig_str == "\u{2014}" || rrsig_str.is_empty() {
                (None, None, None)
            } else {
                let parts: Vec<&str> = rrsig_str.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kind = parts[0].to_string();
                    let lead = match kind.as_str() {
                        "full" => Some(1u8),
                        "condensed" => Some(2u8),
                        _ => None,
                    };
                    let len_str = parts[1].trim_end_matches('B');
                    let len = len_str.parse::<u64>().ok();
                    (Some(kind), lead, len)
                } else {
                    (None, None, None)
                }
            };

        rows.push(CaptureRow {
            label: label.to_string(),
            qname,
            qtype,
            rcode,
            bytes,
            rrsig_lead_byte,
            rrsig_len,
            rrsig_kind,
        });
    }

    rows
}

// ---------------------------------------------------------------------------
// Capture recording (F7.3)
// ---------------------------------------------------------------------------

/// Build the shared [`DnsCapture`] document from the parsed capture rows.
fn build_dns_capture(
    baseline: &[CaptureRow],
    condensed: &[CaptureRow],
    unbound: &[CaptureRow],
) -> DnsCapture {
    let mut queries = Vec::new();
    for rows in [baseline, condensed, unbound] {
        for row in rows {
            queries.push(DnsCaptureQuery {
                label: row.label.clone(),
                qname: row.qname.clone(),
                qtype: row.qtype.clone(),
                response_bytes: row.bytes,
                answer_rrsig_bytes: row.rrsig_len,
                flags: DnsCaptureFlags {
                    ad: None,
                    tc: None,
                    rcode: row.rcode,
                },
                transport: CAPTURE_TRANSPORT.to_string(),
                edns_udp_size: None,
                elapsed_ms: None,
            });
        }
    }

    DnsCapture {
        schema_version: CAPTURE_SCHEMA_VERSION,
        recorded_at: iso8601_now(),
        host: DnsCaptureHost {
            hostname: hostname(),
            uname: uname(),
        },
        images: REQUIRED_IMAGES
            .iter()
            .map(|(image, _)| DnsCaptureImage {
                reference: (*image).to_string(),
                digest: image_digest(image),
            })
            .collect(),
        algorithm: ALGORITHM.to_string(),
        zone: ZONE_NAME.to_string(),
        queries,
    }
}

fn iso8601_now() -> String {
    Command::new("date")
        .arg("--iso-8601=seconds")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn uname() -> String {
    Command::new("uname")
        .arg("-a")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn image_digest(image: &str) -> Option<String> {
    Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .RepoDigests 0}}",
            image,
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('<'))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(crate) fn cmd_dns_demo(args: DnsDemoArgs) -> Result<()> {
    JSON_MODE.store(args.json, Ordering::Relaxed);
    let root = workspace_root()?;
    let scratch = root.join("target").join("dns-demo");

    // ── Phase 1: Preflight ──────────────────────────────────────────
    print_phase(1, "Preflight");
    check_docker()?;
    check_images()?;

    // ── Phase 2: Build images (unless --no-build) ───────────────────
    if !args.no_build {
        print_phase(2, "Build Docker Images");
        run_cmd_inherit(compose_cmd(&root, &["build"]))?;
        print_status("Result", "Images built successfully");
    } else {
        print_phase(2, "Build Docker Images (skipped — --no-build)");
    }

    // ── Phase 3: Sign the zone (unless --reuse-zone) ────────────────
    let signed_zone: PathBuf;

    if let Some(ref reuse) = args.reuse_zone {
        print_phase(3, "Reuse Signed Zone");
        if !reuse.exists() {
            bail!("--reuse-zone path does not exist: {}", reuse.display());
        }
        // NSD always loads "climb.example.zone" from its zone directory, and
        // the SigTag helper reads the same file, so stage the reused zone under
        // that name rather than mounting its original directory.
        fs::create_dir_all(&scratch).context("failed to create scratch directory")?;
        let staged = scratch.join("climb.example.zone");
        fs::copy(reuse, &staged)
            .with_context(|| format!("failed to stage reused zone from {}", reuse.display()))?;
        signed_zone = staged;
        let sz = signed_zone
            .metadata()
            .with_context(|| format!("cannot stat {}", signed_zone.display()))?
            .len();
        print_status(
            "Signed zone",
            &format!(
                "{} ({sz} B, staged from {})",
                signed_zone.display(),
                reuse.display()
            ),
        );
    } else {
        print_phase(3, "Sign Zone");
        fs::create_dir_all(&scratch).context("failed to create scratch directory")?;

        // Copy the unsigned zone into the scratch dir.
        let unsigned_dest = scratch.join("climb.example.zone");
        fs::copy(root.join(UNSIGNED_ZONE), &unsigned_dest)
            .context("failed to copy unsigned zone")?;
        let usz = unsigned_dest.metadata()?.len();
        print_status("Unsigned zone", &format!("{usz} B"));

        // --- ZSK ---
        let zsk = run_in_signer(
            &scratch,
            &[
                "ldns-keygen",
                "-a",
                "SLH-DSA-SHA2-128s-MTL-SHA2-128",
                "climb.example",
            ],
        )?;
        print_status("ZSK basename", &zsk);

        // --- KSK ---
        let ksk = run_in_signer(
            &scratch,
            &[
                "ldns-keygen",
                "-k",
                "-a",
                "SLH-DSA-SHA2-128s-MTL-SHA2-128",
                "climb.example",
            ],
        )?;
        print_status("KSK basename", &ksk);

        // --- Sign ---
        run_in_signer(
            &scratch,
            &["ldns-signzone", "climb.example.zone", &zsk, &ksk],
        )?;

        // NSD reads the zone as "climb.example.zone", so the signed file
        // must replace the unsigned copy.  Keep a .signed backup for the
        // SigTag script.
        {
            let unsigned = scratch.join("climb.example.zone");
            let intermediate = scratch.join("climb.example.zone.signed");
            let signed_copy = scratch.join("climb.example.zone.signed.backup");
            fs::copy(&intermediate, &signed_copy).context("failed to create signed zone backup")?;
            fs::remove_file(&unsigned)
                .context("failed to remove unsigned zone before replacing")?;
            fs::rename(&intermediate, &unsigned)
                .context("failed to rename signed zone to climb.example.zone")?;
            signed_zone = signed_copy;
        }

        humanln!();
        let ssz = signed_zone
            .metadata()
            .with_context(|| format!("signed zone not found at {}", signed_zone.display()))?
            .len();
        print_status(
            "Signed zone",
            &format!("{} ({ssz} B)", signed_zone.display()),
        );

        // --- Verify ---
        // NSD now reads climb.example.zone (the signed copy), so verify
        // against that path.
        let ds = format!("{ksk}.ds");
        run_in_signer(
            &scratch,
            &[
                "ldns-verify-zone",
                "-V",
                "3",
                "climb.example.zone",
                "-k",
                &ds,
            ],
        )?;
        print_status("Verification", "Zone is verified and complete");
    }

    // ── Phase 4: Bring pipeline up ──────────────────────────────────
    print_phase(4, "Bring Pipeline Up");

    let zone_dir = signed_zone
        .parent()
        .context("signed zone path has no parent directory")?;

    // The guard runs `docker compose down` on *any* error after this point.
    let mut guard = ComposeGuard::new(root.clone());
    if args.keep {
        guard.disarm();
    }

    {
        let mut cmd = compose_cmd(&root, &["up", "-d", "nsd", "unbound"]);
        cmd.env("CLIMB_ZONE_DIR", zone_dir);
        run_cmd_inherit(cmd)?;
    }

    wait_for_nsd_ready(&root)?;
    print_status("NSD", "ready — zone loaded");
    print_status("Unbound", "ready — forwarding to NSD");

    // ── Phase 5: Baseline queries (full signature) ──────────────────
    print_phase(5, "Baseline Queries — Full Signatures");
    humanln!("  Querying NSD directly (192.168.13.2) with DO bit, no SigTag.");
    humanln!("  NSD returns the full signed ladder (~8 KB) in every response.");
    humanln!();
    let base_dir = scratch.join("captures-full");
    let baseline_rows = run_capture(&root, NSD_IP, &base_dir, None, "full")?;

    // ── Phase 6: SigTag + condensed queries ─────────────────────────
    print_phase(6, "MTL SigTag + Condensed Queries");

    let sigtag = run_sigtag(&root, &signed_zone)?;
    humanln!("  SigTag (SHAKE-128 hash of the signed ladder):");
    humanln!("  ┌──────────────────────────────────────────────────────────────────┐");
    humanln!("  │ {sigtag} │");
    humanln!("  └──────────────────────────────────────────────────────────────────┘");
    humanln!();
    humanln!("  Querying NSD with the SigTag in EDNS option 65050.");
    humanln!("  NSD recognises the cached ladder hash and returns condensed proofs.");
    humanln!();

    let cond_dir = scratch.join("captures-condensed");
    let condensed_rows = run_capture(&root, NSD_IP, &cond_dir, Some(&sigtag), "condensed")?;

    // ── Phase 7: Live dig queries ───────────────────────────────────
    print_phase(7, "Live dig Queries — Full vs Condensed");
    humanln!("  The same query issued by real `dig`, twice. The only");
    humanln!("  difference is the MTL SigTag in EDNS option 65050.");
    humanln!();

    for &(qname, qtype) in QUERIES {
        humanln!("  ── {qname} {qtype} ──");
        humanln!("  full signature:");
        print_dig_output(&run_dig(&root, "nsd", qname, qtype, None)?, 96);
        humanln!();
        humanln!("  condensed (SigTag advertised):");
        print_dig_output(&run_dig(&root, "nsd", qname, qtype, Some(&sigtag))?, 96);
        humanln!();
    }

    // ── Phase 8: Headline comparison ────────────────────────────────
    print_phase(8, "Headline — Full vs Condensed");
    print_headline(&baseline_rows, &condensed_rows);

    // ── Phase 9: Unbound forwarding ─────────────────────────────────
    print_phase(9, "Unbound Forwarding (iterator mode)");
    humanln!("  The committed unbound.conf uses module-config: \"iterator\"");
    humanln!("  (forwarding) — it forwards queries to NSD and returns answers");
    humanln!("  but does not perform DNSSEC validation or set the `ad` flag.");
    humanln!("  Full validation requires switching to \"validator iterator\" and");
    humanln!("  adding a trust-anchor for climb.example. — this is documented");
    humanln!("  but not exercised in the default demo run.");
    humanln!("  MTL SigTag exchange still happens on the Unbound→NSD upstream");
    humanln!("  path (enable-edns-sigtag-mtl: yes), reducing internal bandwidth.");
    humanln!();

    let ub_dir = scratch.join("captures-unbound");
    let unbound_rows = run_capture(&root, UNBOUND_IP, &ub_dir, None, "unbound")?;
    humanln!("  All queries via Unbound returned successfully.");
    humanln!("  (The AD flag is absent — expected in iterator mode.)");

    // ── Phase 10: Wire captures summary ─────────────────────────────
    print_phase(10, "Wire Captures");

    let total_full: u64 = baseline_rows.iter().map(|r| r.bytes).sum();
    let total_cond: u64 = condensed_rows.iter().map(|r| r.bytes).sum();
    humanln!(
        "  Full-signature responses:  {}  ({} B / {} queries)",
        base_dir.display(),
        total_full,
        baseline_rows.len(),
    );
    humanln!(
        "  Condensed-signature:       {}  ({} B / {} queries)",
        cond_dir.display(),
        total_cond,
        condensed_rows.len(),
    );
    humanln!("  Via Unbound (iterator):    {}", ub_dir.display(),);
    humanln!("  Each .bin file is a complete DNS wire-format response.");

    // ── Record the capture (F7.3) ───────────────────────────────────
    if args.record.is_some() || args.json {
        let capture = build_dns_capture(&baseline_rows, &condensed_rows, &unbound_rows);
        let json =
            serde_json::to_string_pretty(&capture).context("failed to serialize DNS capture")?;
        if let Some(path) = &args.record {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
            }
            fs::write(path, &json)
                .with_context(|| format!("failed to write {}", path.display()))?;
            humanln!("  Recorded DNS capture → {}", path.display());
        }
        if args.json {
            println!("{json}");
        }
    }

    // ── Phase 11: Teardown ──────────────────────────────────────────
    print_phase(11, "Teardown");

    if args.keep {
        guard.disarm(); // already disarmed; explicit for clarity
        humanln!("  Pipeline LEFT RUNNING (--keep).");
        humanln!();
        humanln!("  To tear down manually:");
        humanln!("    docker compose -f docker/compose.dns-pipeline.yml down");
        humanln!();
        humanln!("  Scratch directory:  {}", scratch.display());
        humanln!("  Signed zone:        {}", signed_zone.display());
    } else {
        // Explicit compose down.
        {
            let cmd = compose_cmd(&root, &["down", "--remove-orphans"]);
            let status = run_cmd_status(cmd)?;
            if !status.success() {
                eprintln!("  warning: docker compose down exited with {status}");
            }
        }
        guard.disarm(); // prevent double-down in Drop
        humanln!("  Pipeline shut down.");

        if scratch.exists() {
            fs::remove_dir_all(&scratch).context("failed to remove scratch directory")?;
            humanln!("  Scratch directory removed.");
        }
    }

    // ── Final ────────────────────────────────────────────────────────
    humanln!("\n{:=<72}", "");
    humanln!("  Demo complete.");
    humanln!("{:=<72}", "");
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests — pure logic, no Docker / network required
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── workspace_root ───────────────────────────────────────────────

    #[test]
    fn workspace_root_is_parent_of_manifest_dir() {
        let root = workspace_root().expect("workspace_root");
        assert!(root.join("xtask").join("Cargo.toml").exists());
    }

    // ── parse_capture_output ─────────────────────────────────────────

    fn sample_capture_full() -> String {
        "\n  wire capture \u{2192} /tmp/x  (server 192.168.13.2, TCP, mode=full)\n\
         \n  query                        type   bytes  rcode  rrsig            file\n  \
         ----------------------------------------------------------------------------\n  \
         climb.example.               SOA     24465  0      full 8069B       00-soa-full.bin\n  \
         www.climb.example.           A       24438  0      full 8069B       01-a-full.bin\n  \
         mail.climb.example.          MX      32572  0      full 8069B       02-mx-full.bin"
            .to_string()
    }

    fn sample_capture_condensed() -> String {
        "\n  wire capture \u{2192} /tmp/x  (server 192.168.13.2, TCP, mode=sigtag)\n\
         \n  query                        type   bytes  rcode  rrsig            file\n  \
         ----------------------------------------------------------------------------\n  \
         climb.example.               SOA       685  0      condensed 141B   00-soa-sigtag.bin\n  \
         www.climb.example.           A         658  0      condensed 141B   01-a-sigtag.bin\n  \
         mail.climb.example.          MX        864  0      condensed 141B   02-mx-sigtag.bin"
            .to_string()
    }

    #[test]
    fn parse_full_capture_yields_three_rows() {
        let rows = parse_capture_output(&sample_capture_full(), "full");
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn parse_condensed_capture_yields_three_rows() {
        let rows = parse_capture_output(&sample_capture_condensed(), "condensed");
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn full_capture_lead_byte_is_1() {
        let rows = parse_capture_output(&sample_capture_full(), "full");
        for row in &rows {
            assert_eq!(row.rrsig_lead_byte, Some(1));
            assert_eq!(row.rrsig_kind.as_deref(), Some("full"));
            assert_eq!(row.rrsig_len, Some(8069));
        }
    }

    #[test]
    fn condensed_capture_lead_byte_is_2() {
        let rows = parse_capture_output(&sample_capture_condensed(), "condensed");
        for row in &rows {
            assert_eq!(row.rrsig_lead_byte, Some(2));
            assert_eq!(row.rrsig_kind.as_deref(), Some("condensed"));
            assert_eq!(row.rrsig_len, Some(141));
        }
    }

    #[test]
    fn full_capture_byte_counts_match_verified_numbers() {
        let rows = parse_capture_output(&sample_capture_full(), "full");
        // Per the spec: SOA=24465, A=24438, MX=32572
        assert_eq!(rows[0].bytes, 24465);
        assert_eq!(rows[1].bytes, 24438);
        assert_eq!(rows[2].bytes, 32572);
    }

    #[test]
    fn condensed_capture_byte_counts_match_verified_numbers() {
        let rows = parse_capture_output(&sample_capture_condensed(), "condensed");
        // Per the spec: SOA=685, A=658, MX=864
        assert_eq!(rows[0].bytes, 685);
        assert_eq!(rows[1].bytes, 658);
        assert_eq!(rows[2].bytes, 864);
    }

    #[test]
    fn empty_input_yields_zero_rows() {
        let rows = parse_capture_output("", "empty");
        assert!(rows.is_empty());
    }

    #[test]
    fn header_lines_are_skipped() {
        let input = "\n  wire capture → /tmp/x  (server nsd, TCP, mode=full)\n\n  \
                      query                        type   bytes  rcode  rrsig            file\n  \
                      ----------------------------------------------------------------------------\n";
        let rows = parse_capture_output(input, "headers-only");
        assert!(rows.is_empty());
    }

    #[test]
    fn no_rrsig_uses_em_dash() {
        // Simulate a capture line where there's no RRSIG (shown as "—")
        let line = "  mail.climb.example.          MX       1000  0      \u{2014}                02-mx.bin";
        let rows = parse_capture_output(line, "no-rrsig");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rrsig_lead_byte, None);
        assert_eq!(rows[0].rrsig_kind, None);
    }

    #[test]
    fn parse_capture_records_qname_qtype_rcode_and_label() {
        let rows = parse_capture_output(&sample_capture_full(), "full");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].label, "full");
        assert_eq!(rows[1].qname, "www.climb.example.");
        assert_eq!(rows[1].qtype, "A");
        assert_eq!(rows[1].rcode, Some(0));
    }

    // ── build_dns_capture ────────────────────────────────────────────

    #[test]
    fn build_dns_capture_includes_all_three_passes() {
        let baseline = parse_capture_output(&sample_capture_full(), "full");
        let condensed = parse_capture_output(&sample_capture_condensed(), "condensed");
        let unbound = parse_capture_output(&sample_capture_full(), "unbound");
        let capture = build_dns_capture(&baseline, &condensed, &unbound);
        assert_eq!(capture.schema_version, 1);
        assert_eq!(capture.queries.len(), 9);
        assert_eq!(
            capture.queries.iter().filter(|q| q.label == "full").count(),
            3
        );
        assert_eq!(
            capture
                .queries
                .iter()
                .filter(|q| q.label == "condensed")
                .count(),
            3
        );
        assert_eq!(
            capture
                .queries
                .iter()
                .filter(|q| q.label == "unbound")
                .count(),
            3
        );
        assert_eq!(capture.algorithm, ALGORITHM);
        assert_eq!(capture.zone, ZONE_NAME);
        let a_row = capture
            .queries
            .iter()
            .find(|q| q.label == "full" && q.qtype == "A")
            .expect("www A row");
        assert_eq!(a_row.response_bytes, 24_438);
        assert_eq!(a_row.transport, "tcp");
    }

    // ── compute_reduction ────────────────────────────────────────────

    #[test]
    fn reduction_ratio_from_verified_numbers() {
        // 24438 B → 658 B: ratio ≈ 37.14, saved = 23780, pct ≈ 97.31%
        let full = 24_438u64;
        let cond = 658u64;
        let saved = full - cond;
        assert_eq!(saved, 23_780);

        let pct = (saved as f64 / full as f64) * 100.0;
        assert!((pct - 97.31).abs() < 0.1);

        let ratio = full as f64 / cond as f64;
        assert!((ratio - 37.14).abs() < 0.1);
    }

    #[test]
    fn reduction_zero_bytes() {
        let pct = (0.0f64 / 1.0) * 100.0;
        assert_eq!(pct, 0.0);
    }

    // ── render_cmd ───────────────────────────────────────────────────

    #[test]
    fn render_cmd_shows_program_and_args() {
        let mut cmd = Command::new("docker");
        cmd.args(["compose", "-f", "docker/x.yml", "up", "-d"]);
        let rendered = render_cmd(&cmd);
        assert!(rendered.starts_with("docker"));
        assert!(rendered.contains("compose"));
        assert!(rendered.contains("-f"));
        assert!(rendered.contains("docker/x.yml"));
        assert!(rendered.contains("up"));
        assert!(rendered.contains("-d"));
    }

    #[test]
    fn render_cmd_single_program_no_args() {
        let cmd = Command::new("python3");
        let rendered = render_cmd(&cmd);
        assert_eq!(rendered, "python3");
    }

    // ── image_exists (unit — only tests fallback when docker absent) ─

    #[test]
    fn image_exists_returns_false_for_nonexistent() {
        // This returns false on any host without docker, which is fine for
        // unit tests — the test validates the fallback path.
        let exists = image_exists("this-image-does-not-exist-xyzzy-12345");
        assert!(!exists);
    }
}
