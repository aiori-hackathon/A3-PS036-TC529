//! Emulated-RTT DNS latency measurement harness.
//!
//! Measures client-observed DNS query latency across the local Docker bridge
//! with a kernel `tc netem` delay applied to both ends of the client exchange:
//! half the target RTT on the client's egress and half on the queried server's
//! egress, filtered to the client address. Every packet in both directions is
//! therefore delayed, so a TCP fallback pays for its extra round trips.
//!
//! Before measuring, the harness validates the configured delay with `ping`
//! at 0, 20, 50 and 100 ms. A delay that misses the 10 percent tolerance stops
//! the run; there is no userspace-sleep fallback and no path that writes an
//! artifact without a validated kernel delay.
//!
//! Evidence class: MEASURED. Label: "emulated RTT, local bridge". No number
//! from this harness may be presented as Internet latency.

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::dns_demo::{
    check_docker, check_images, compose_cmd, git_head, hostname, run_cmd_inherit, run_in_signer,
    uname, workspace_root, NSD_IP, UNBOUND_IP,
};

/// Schema version for the dns-rtt artifact.
pub const DNS_RTT_SCHEMA_VERSION: u32 = 1;

/// Evidence label applied to every RTT cell and surface.
pub const DNS_RTT_LABEL: &str = "emulated RTT, local bridge";

/// Delay levels (milliseconds of client-observed round-trip time).
pub const DNS_RTT_DELAYS_MS: [f64; 4] = [0.0, 20.0, 50.0, 100.0];

/// Minimum samples per cell for a full measurement.
pub const DNS_RTT_FULL_SAMPLES: u64 = 200;

/// Samples per cell in quick (smoke-test) mode.
pub const DNS_RTT_QUICK_SAMPLES: u64 = 20;

/// Queried zone name.
pub const RTT_ZONE: &str = "climb.example.";

/// Query name used for measurements.
pub const RTT_QUERY_NAME: &str = "www.climb.example.";

/// Query type used for measurements.
pub const RTT_QUERY_TYPE: &str = "A";

/// Fixed address of the dedicated measurement client on the internal bridge.
pub const RTT_CLIENT_IP: &str = "192.168.13.4";

/// Container name of the dedicated measurement client.
pub const RTT_CLIENT_NAME: &str = "climb-rtt-client";

/// ICMP echo requests used to validate each configured delay.
pub const RTT_PING_SAMPLES: u32 = 20;

/// Relative tolerance between the configured and measured RTT.
pub const RTT_TOLERANCE: f64 = 0.10;

/// Compose service name of the recursive resolver whose in-memory message
/// cache is flushed before every cell it is measured in.
pub const RTT_UNBOUND_SERVICE: &str = "unbound";

/// How long to wait for Unbound to answer on port 53 after a cache-flushing
/// restart, in tenths of a second (50 * 0.2 s = 10 s max).
pub const RTT_UNBOUND_READY_POLLS: u32 = 50;

/// Script inside the client container that performs one measurement cell.
const RTT_PROBE_SCRIPT: &str = "/scripts/mtl_rtt_probe.py";

/// Docker image used to run the RTT probe and the netem sidecars.
const DIG_IMAGE: &str = "climb-dig:phase14";

/// One per-query record: what the client actually sent and waited for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RttQuery {
    /// UDP messages sent for this query (normally 1).
    #[serde(default)]
    pub udp_attempts: u32,
    /// TCP messages sent for this query (0, or 1 after a TC=1 reply).
    #[serde(default)]
    pub tcp_attempts: u32,
    /// Bytes in the UDP response.
    #[serde(default)]
    pub udp_response_bytes: u64,
    /// Bytes in the TCP response, or 0 when no TCP retry happened.
    #[serde(default)]
    pub tcp_response_bytes: u64,
    /// Wall time for the whole exchange in milliseconds.
    #[serde(default)]
    pub total_ms: f64,
    /// `total_ms / target RTT`, or `None` at a 0 ms target.
    #[serde(default)]
    pub rtt_multiple: Option<f64>,
}

/// One row of the `ping` validation of the kernel delay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetemCheck {
    /// Requested client-observed round-trip time in milliseconds.
    #[serde(default)]
    pub target_ms: f64,
    /// Mean `ping` round-trip time in milliseconds.
    #[serde(default)]
    pub ping_avg_ms: f64,
    /// `ping_avg_ms` minus the 0 ms baseline, in milliseconds.
    #[serde(default)]
    pub corrected_ms: f64,
    /// Whether the corrected value is within [`RTT_TOLERANCE`] of the target.
    #[serde(default)]
    pub within_tolerance: bool,
    /// ICMP echo requests sent.
    #[serde(default)]
    pub samples: u32,
}

/// One measurement cell: (server, mode, delay).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RttCell {
    /// Server queried: `nsd` or `unbound`.
    pub server: String,
    /// Response mode: `plain` or `mtl-condensed`.
    pub mode: String,
    /// Client-observed emulated round-trip time in milliseconds.
    pub delay_ms: f64,
    /// Successful samples in the cell.
    pub samples: u64,
    /// Samples whose first UDP response had the TC bit set.
    pub truncated: u64,
    /// Transport actually used: `udp` or `udp-then-tcp`.
    pub transport: String,
    /// RRSIG kind observed: `full` or `condensed`.
    pub rrsig_kind: String,
    /// Median client latency in milliseconds.
    pub median_ms: f64,
    /// 95th-percentile client latency in milliseconds.
    pub p95_ms: f64,
    /// How the delay was applied; always `tc-netem`.
    pub mechanism: String,
    /// Median UDP messages sent per query.
    #[serde(default)]
    pub udp_attempts: u32,
    /// Median TCP messages sent per query.
    #[serde(default)]
    pub tcp_attempts: u32,
    /// Median bytes in the UDP response.
    #[serde(default)]
    pub udp_response_bytes: u64,
    /// Median bytes in the TCP response.
    #[serde(default)]
    pub tcp_response_bytes: u64,
    /// Median wall time for the whole exchange in milliseconds.
    #[serde(default)]
    pub median_total_ms: f64,
    /// Median `total_ms / target RTT`; `None` at a 0 ms target.
    #[serde(default)]
    pub median_rtt_multiple: Option<f64>,
    /// Per-query records.
    #[serde(default)]
    pub queries: Vec<RttQuery>,
    /// Non-fatal warnings for this cell.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// The complete RTT measurement artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RttArtifact {
    /// Artifact schema version.
    pub schema_version: u32,
    /// `quick` or `full`.
    pub mode: String,
    /// When the measurement was recorded.
    pub recorded_at: String,
    /// Git commit at recording time, if resolvable.
    pub git_commit: Option<String>,
    /// Host name the pipeline ran on.
    pub hostname: String,
    /// Kernel/userspace identification.
    pub uname: String,
    /// Fixed evidence label.
    pub label: String,
    /// Set when the recorded cells may not be shown; the value states why.
    #[serde(default)]
    pub superseded: Option<String>,
    /// How delay was applied globally; always `tc-netem`.
    pub mechanism: String,
    /// Netem description (human-readable).
    pub netem: String,
    /// Zone queried.
    pub zone: String,
    /// Query name and type.
    pub query: String,
    /// One row per configuration cell.
    pub cells: Vec<RttCell>,
    /// Minimum samples per cell for full evidence.
    pub min_samples_per_cell: u64,
    /// Evidence class.
    pub class: String,
    /// Non-fatal measurement warnings.
    pub warnings: Vec<String>,
    /// `ping` validation of every configured delay.
    #[serde(default)]
    pub netem_validation: Vec<NetemCheck>,
}

impl RttArtifact {
    /// Whether this is a full measurement with at least the required samples
    /// in every cell. A quick smoke test or a superseded artifact never
    /// qualifies.
    #[allow(dead_code)]
    pub fn is_full_evidence(&self) -> bool {
        self.superseded.is_none()
            && self.mode == "full"
            && !self.cells.is_empty()
            && self
                .cells
                .iter()
                .all(|cell| cell.samples >= self.min_samples_per_cell)
    }
}

/// CLI arguments for `cargo xtask dns-rtt`.
#[derive(Args)]
pub(crate) struct DnsRttArgs {
    /// Output path for the RTT artifact.
    #[arg(long, default_value = "target/dns-rtt.json")]
    pub(crate) out: PathBuf,

    /// Quick smoke-test mode: fewer samples per cell, never qualifies as full
    /// evidence. Intended to validate the harness end-to-end.
    #[arg(long)]
    pub(crate) quick: bool,

    /// Skip building Docker images (assume they already exist).
    #[arg(long)]
    pub(crate) no_build: bool,

    /// Reuse an existing signed zone file instead of signing fresh.
    #[arg(long, value_name = "PATH")]
    pub(crate) reuse_zone: Option<PathBuf>,

    /// Measure against `nsd`, `unbound` or `both`.
    #[arg(long, default_value = "both")]
    pub(crate) server: String,
}

pub(crate) fn cmd_dns_rtt(args: DnsRttArgs) -> Result<()> {
    let root = workspace_root()?;
    check_docker()?;
    check_images()?;
    let servers = selected_servers(&args.server)?;

    if !args.no_build {
        print_stage("Building Docker images");
        run_cmd_inherit(compose_cmd(&root, &["build"]))?;
    }

    let scratch = root.join("target").join("dns-rtt");
    std::fs::create_dir_all(&scratch).context("failed to create scratch directory")?;

    // Sign or reuse a zone.
    let signed_zone = if let Some(reuse) = args.reuse_zone {
        if !reuse.exists() {
            bail!("--reuse-zone path does not exist: {}", reuse.display());
        }
        fs_copy_to_scratch(&scratch, reuse.as_path(), "climb.example.zone")?;
        scratch.join("climb.example.zone")
    } else {
        print_stage("Signing MTL zone");
        let unsigned_dest = scratch.join("climb.example.zone");
        std::fs::copy(root.join("docker/zones/climb.example.zone"), &unsigned_dest)
            .context("failed to copy unsigned zone")?;

        let zsk = run_in_signer(
            &scratch,
            &[
                "ldns-keygen",
                "-a",
                "SLH-DSA-SHA2-128s-MTL-SHA2-128",
                "climb.example",
            ],
        )?;
        eprintln!("  ZSK basename: {zsk}");

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
        eprintln!("  KSK basename: {ksk}");

        run_in_signer(
            &scratch,
            &["ldns-signzone", "climb.example.zone", &zsk, &ksk],
        )?;

        // Replace unsigned zone with signed copy, keep .signed backup for SigTag.
        let intermediate = scratch.join("climb.example.zone.signed");
        let signed_copy = scratch.join("climb.example.zone.signed.backup");
        std::fs::copy(&intermediate, &signed_copy)
            .context("failed to create signed zone backup")?;
        std::fs::remove_file(&unsigned_dest)
            .context("failed to remove unsigned zone before replacing")?;
        std::fs::rename(&intermediate, &unsigned_dest)
            .context("failed to rename signed zone to climb.example.zone")?;
        signed_copy
    };

    // Bring up pipeline.
    print_stage("Starting DNS pipeline");
    let zone_dir = signed_zone
        .parent()
        .context("signed zone path has no parent directory")?;
    {
        let mut cmd = compose_cmd(&root, &["up", "-d", "nsd", "unbound"]);
        cmd.env("CLIMB_ZONE_DIR", zone_dir);
        run_cmd_inherit(cmd)?;
    }
    crate::dns_demo::wait_for_nsd_ready(&root)?;
    eprintln!("  NSD ready — zone loaded");

    let _guard = PipelineGuard::new(root.clone());
    let nsd_id = container_id(&root, "nsd")?;
    let unbound_id = container_id(&root, "unbound")?;
    let server_ids: Vec<&str> = servers
        .iter()
        .map(|server| {
            if *server == NSD_IP {
                nsd_id.as_str()
            } else {
                unbound_id.as_str()
            }
        })
        .collect();
    for id in &server_ids {
        if id.is_empty() {
            bail!("could not resolve a pipeline server container id");
        }
    }

    print_stage("Starting measurement client");
    let _client = ClientGuard::start(&root)?;

    let samples = if args.quick {
        DNS_RTT_QUICK_SAMPLES
    } else {
        DNS_RTT_FULL_SAMPLES
    };

    // Compute SigTag for MTL condensed mode.
    let sigtag = compute_sigtag(&root, &signed_zone)?;

    let mut validation = Vec::new();
    let mut cells = Vec::new();
    let warnings = Vec::new();
    let mut baseline_ms = 0.0;

    for &delay_ms in &DNS_RTT_DELAYS_MS {
        let check = apply_delay_and_validate(&_client.name, &server_ids, delay_ms, baseline_ms)?;
        if delay_ms == 0.0 {
            baseline_ms = check.ping_avg_ms;
        }
        if !check.within_tolerance {
            bail!(
                "netem validation failed at {delay_ms} ms: ping average {:.3} ms, corrected {:.3} ms \
                 (tolerance {:.0} %)",
                check.ping_avg_ms,
                check.corrected_ms,
                RTT_TOLERANCE * 100.0
            );
        }
        validation.push(check);

        for server in &servers {
            for mode in ["plain", "mtl-condensed"] {
                // Unbound caches answers in memory: a plain cell's full
                // response would otherwise be served to the next cell's
                // condensed query. Flush before every Unbound cell so no cell
                // can be served another cell's cached reply. `docker compose
                // restart` keeps the container (and its network namespace, so
                // the netem qdisc on its egress survives); NSD is
                // authoritative and keeps no answer cache, so nothing to flush
                // there.
                if server_needs_cache_flush(server) {
                    flush_unbound_cache(&root, &unbound_id, delay_ms)?;
                    eprintln!("  flushed Unbound cache before {server}/{mode} cell");
                }
                let cell = measure_cell(&_client.name, server, mode, delay_ms, samples, &sigtag)?;
                cells.push(cell);
            }
        }
    }

    // Tear down pipeline.
    print_stage("Tearing down DNS pipeline");

    // Build artifact.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let recorded_at = format!("{now}");

    let artifact = RttArtifact {
        schema_version: DNS_RTT_SCHEMA_VERSION,
        mode: if args.quick {
            "quick".to_string()
        } else {
            "full".to_string()
        },
        recorded_at,
        git_commit: git_head(),
        hostname: hostname(),
        uname: uname(),
        label: DNS_RTT_LABEL.to_string(),
        superseded: None,
        mechanism: "tc-netem".to_string(),
        netem: "kernel netem: half the target RTT on client egress and half on the queried server's egress, filtered to the client address".to_string(),
        zone: RTT_ZONE.to_string(),
        query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
        cells,
        min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
        class: "measured".to_string(),
        warnings,
        netem_validation: validation,
    };

    print_probe_table(&artifact);

    // Write artifact.
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(&artifact).context("failed to serialize RTT artifact")?;
    std::fs::write(&args.out, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", args.out.display()))?;

    eprintln!(
        "wrote {} ({} cells, mode={}, mechanism={})",
        args.out.display(),
        artifact.cells.len(),
        artifact.mode,
        artifact.mechanism
    );
    Ok(())
}

fn print_stage(title: &str) {
    eprintln!("\n{:=<72}", "");
    eprintln!("  dns-rtt: {title}");
    eprintln!("{:=<72}", "");
}

/// Print the smoke/full table. Quick mode is labelled as a smoke test.
fn print_probe_table(artifact: &RttArtifact) {
    let kind = if artifact.mode == "quick" {
        "smoke, quick mode"
    } else {
        "full measurement"
    };
    println!(
        "\nRTT probe ({kind}): {} cell(s), {} samples/cell",
        artifact.cells.len(),
        artifact.cells.first().map_or(0, |cell| cell.samples)
    );
    println!(
        "{:<7} {:<14} {:>7} {:>10} {:>10} {:>4} {:>4} {:>9} {:>9} {:>8}",
        "server", "mode", "target", "median ms", "p95 ms", "udp", "tcp", "udp B", "tcp B", "x RTT"
    );
    for cell in &artifact.cells {
        let multiple = cell
            .median_rtt_multiple
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<7} {:<14} {:>7.0} {:>10.3} {:>10.3} {:>4} {:>4} {:>9} {:>9} {:>8}",
            cell.server,
            cell.mode,
            cell.delay_ms,
            cell.median_ms,
            cell.p95_ms,
            cell.udp_attempts,
            cell.tcp_attempts,
            cell.udp_response_bytes,
            cell.tcp_response_bytes,
            multiple,
        );
    }
    println!(
        "\nping validation (tolerance {:.0} %):",
        RTT_TOLERANCE * 100.0
    );
    println!(
        "{:>7} {:>10} {:>10} {:>8}",
        "target", "ping avg", "corrected", "within"
    );
    for check in &artifact.netem_validation {
        println!(
            "{:>7.0} {:>10.3} {:>10.3} {:>8}",
            check.target_ms,
            check.ping_avg_ms,
            check.corrected_ms,
            if check.within_tolerance { "yes" } else { "NO" }
        );
    }
}

/// Copy a file into scratch, returning the staged path.
fn fs_copy_to_scratch(_scratch: &Path, src: &Path, name: &str) -> Result<PathBuf> {
    let dest = _scratch.join(name);
    std::fs::copy(src, &dest)
        .with_context(|| format!("failed to copy {} to {}", src.display(), dest.display()))?;
    Ok(dest)
}

/// Compute the MTL SigTag for the signed zone.
fn compute_sigtag(root: &Path, signed_zone: &Path) -> Result<Option<String>> {
    let mut cmd = Command::new("python3");
    cmd.arg("scripts/mtl_sigtag.py")
        .arg(signed_zone)
        .current_dir(root);

    let output = cmd.output().context("failed to run SigTag script")?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        eprintln!("  warning: SigTag computation failed; MTL condensed queries will use no tag");
        Ok(None)
    }
}

/// Measure one configuration cell.
fn measure_cell(
    client: &str,
    server: &str,
    mode: &str,
    delay_ms: f64,
    samples: u64,
    sigtag: &Option<String>,
) -> Result<RttCell> {
    let server_label = if server == NSD_IP { "nsd" } else { "unbound" };
    let samples_str = samples.to_string();
    let delay_str = delay_ms.to_string();

    let mut probe_args: Vec<&str> = vec![
        RTT_PROBE_SCRIPT,
        "--server",
        server,
        "--port",
        "53",
        "--name",
        RTT_QUERY_NAME,
        "--qtype",
        RTT_QUERY_TYPE,
        "--samples",
        &samples_str,
        "--timeout",
        "5",
        "--target-rtt-ms",
        &delay_str,
    ];

    if let Some(tag) = sigtag.as_deref() {
        if mode == "mtl-condensed" {
            probe_args.push("--sigtag");
            probe_args.push(tag);
        }
    }

    let output = Command::new("docker")
        .arg("exec")
        .arg(client)
        .arg("python3")
        .args(&probe_args)
        .output()
        .context("failed to run RTT probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("RTT probe failed for {server_label}/{mode}/{delay_ms}ms: {stderr}");
    }

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse probe output")?;

    let queries = result
        .get("queries")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| RttQuery {
                    udp_attempts: u32_at(row, "udp_attempts"),
                    tcp_attempts: u32_at(row, "tcp_attempts"),
                    udp_response_bytes: u64_at(row, "udp_response_bytes"),
                    tcp_response_bytes: u64_at(row, "tcp_response_bytes"),
                    total_ms: f64_at(row, "total_ms"),
                    rtt_multiple: row.get("rtt_multiple").and_then(serde_json::Value::as_f64),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(RttCell {
        server: server_label.to_string(),
        mode: mode.to_string(),
        delay_ms,
        samples,
        truncated: u64_at(&result, "truncated"),
        transport: str_at(&result, "transport").unwrap_or("udp").to_string(),
        rrsig_kind: str_at(&result, "rrsig_kind").unwrap_or("none").to_string(),
        median_ms: f64_at(&result, "median_ms"),
        p95_ms: f64_at(&result, "p95_ms"),
        mechanism: "tc-netem".to_string(),
        udp_attempts: u32_at(&result, "median_udp_attempts"),
        tcp_attempts: u32_at(&result, "median_tcp_attempts"),
        udp_response_bytes: u64_at(&result, "udp_response_bytes"),
        tcp_response_bytes: u64_at(&result, "tcp_response_bytes"),
        median_total_ms: f64_at(&result, "median_total_ms"),
        median_rtt_multiple: result
            .get("median_rtt_multiple")
            .and_then(serde_json::Value::as_f64),
        queries,
        warnings: Vec::new(),
    })
}

fn f64_at(value: &serde_json::Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

fn u64_at(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn u32_at(value: &serde_json::Value, key: &str) -> u32 {
    u64_at(value, key).try_into().unwrap_or(u32::MAX)
}

fn str_at<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

/// Apply the delay to both ends of the client exchange, then validate it with
/// `ping`. Returns the validation row; a failed tolerance is the caller's to
/// reject before any artifact is written.
fn apply_delay_and_validate(
    client: &str,
    server_ids: &[&str],
    delay_ms: f64,
    baseline_ms: f64,
) -> Result<NetemCheck> {
    apply_client_delay(client, delay_ms)?;
    for id in server_ids {
        apply_server_delay(id, delay_ms)?;
    }
    let ping_avg_ms = ping_average(client, NSD_IP)?;
    let corrected_ms = (ping_avg_ms - baseline_ms).max(0.0);
    let within_tolerance = within_tolerance(delay_ms, corrected_ms);
    Ok(NetemCheck {
        target_ms: delay_ms,
        ping_avg_ms,
        corrected_ms,
        within_tolerance,
        samples: RTT_PING_SAMPLES,
    })
}

/// Whether a corrected measurement is inside the 10 percent tolerance. A 0 ms
/// target is satisfied by at most 1 ms, because a percentage of zero cannot
/// describe the bridge's own sub-millisecond overhead.
fn within_tolerance(target_ms: f64, corrected_ms: f64) -> bool {
    if target_ms <= f64::EPSILON {
        corrected_ms <= 1.0
    } else {
        (corrected_ms - target_ms).abs() <= target_ms * RTT_TOLERANCE
    }
}

/// Whether a server's in-memory answer cache must be flushed before each of
/// its cells. Unbound caches answers; NSD is authoritative and keeps none.
fn server_needs_cache_flush(server: &str) -> bool {
    server == UNBOUND_IP
}

/// Restart the Unbound service so its in-memory answer cache is empty before
/// the next cell is measured, then re-apply the server-side netem delay: a
/// restart drops the server's qdisc, so without this the measured RTT would
/// collapse to the one-way client delay.
fn flush_unbound_cache(root: &Path, container: &str, delay_ms: f64) -> Result<()> {
    run_cmd_inherit(compose_cmd(root, &["restart", RTT_UNBOUND_SERVICE]))?;
    wait_for_unbound_ready()?;
    apply_server_delay(container, delay_ms)
}

/// Wait until Unbound answers on port 53 after its restart, polling a TCP
/// connect through a throwaway container on the pipeline network. A failed
/// wait aborts the run rather than measuring against a cold port.
fn wait_for_unbound_ready() -> Result<()> {
    let script = format!(
        r#"import socket, time
s = socket.socket()
s.settimeout(2.0)
for _ in range({polls}):
    if s.connect_ex(("{ip}", 53)) == 0:
        raise SystemExit(0)
    time.sleep(0.2)
raise SystemExit(1)
"#,
        polls = RTT_UNBOUND_READY_POLLS,
        ip = UNBOUND_IP,
    );
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--net",
            "climb-net",
            "--entrypoint",
            "python3",
            DIG_IMAGE,
            "-c",
            &script,
        ])
        .output()
        .context("failed to poll Unbound readiness after cache flush")?;
    if !output.status.success() {
        bail!("Unbound did not answer on port 53 after cache-flush restart");
    }
    Ok(())
}

/// Apply half the target delay to the client's egress.
fn apply_client_delay(client: &str, delay_ms: f64) -> Result<()> {
    let half = delay_ms / 2.0;
    let script = if half > 0.0 {
        format!(
            "tc qdisc del dev eth0 root 2>/dev/null || true; \
             tc qdisc add dev eth0 root netem delay {half}ms"
        )
    } else {
        "tc qdisc del dev eth0 root 2>/dev/null || true".to_string()
    };
    run_checked(
        Command::new("docker")
            .args(["exec", client, "sh", "-c"])
            .arg(script),
        "apply client netem delay",
    )
}

/// Apply half the target delay to a server's egress, filtered to the client
/// address so server-to-server forwarding is not delayed.
fn apply_server_delay(container: &str, delay_ms: f64) -> Result<()> {
    let half = delay_ms / 2.0;
    let script = if half > 0.0 {
        format!(
            "set -e; tc qdisc del dev eth0 root 2>/dev/null || true; \
             tc qdisc add dev eth0 root handle 1: prio; \
             tc qdisc add dev eth0 parent 1:1 handle 10: netem delay {half}ms; \
             tc filter add dev eth0 parent 1: protocol ip prio 1 u32 match ip dst {RTT_CLIENT_IP}/32 flowid 1:1"
        )
    } else {
        "tc qdisc del dev eth0 root 2>/dev/null || true".to_string()
    };
    run_checked(
        Command::new("docker")
            .args([
                "run",
                "--rm",
                "--net",
                &format!("container:{container}"),
                "--cap-add",
                "NET_ADMIN",
                "--entrypoint",
                "sh",
                DIG_IMAGE,
                "-c",
            ])
            .arg(script),
        "apply server netem delay",
    )
}

fn run_checked(cmd: &mut Command, what: &str) -> Result<()> {
    let output = cmd.output().with_context(|| format!("failed to {what}"))?;
    if !output.status.success() {
        bail!(
            "failed to {what}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Mean ICMP round-trip time in milliseconds.
fn ping_average(client: &str, target: &str) -> Result<f64> {
    let count = RTT_PING_SAMPLES.to_string();
    let output = Command::new("docker")
        .args([
            "exec", client, "ping", "-c", &count, "-i", "0.1", "-W", "2", target,
        ])
        .output()
        .context("failed to run ping validation")?;
    if !output.status.success() {
        bail!(
            "ping validation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    ping_average_from_output(&text).with_context(|| format!("could not parse ping output: {text}"))
}

/// Parse the mean out of BusyBox or iputils `ping` output.
fn ping_average_from_output(text: &str) -> Option<f64> {
    for line in text.lines() {
        if !line.contains("min/avg/max") {
            continue;
        }
        let values = line.split('=').nth(1)?;
        let average = values.trim().split('/').nth(1)?;
        return average.trim().parse().ok();
    }
    None
}

/// Look up the container ID for a compose service.
fn container_id(root: &Path, service: &str) -> Result<String> {
    let output = compose_cmd(root, &["ps", "-q", service])
        .output()
        .context("failed to look up container ID")?;
    if !output.status.success() {
        bail!(
            "failed to look up container ID for {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The servers selected on the command line.
fn selected_servers(selector: &str) -> Result<Vec<&'static str>> {
    match selector {
        "nsd" => Ok(vec![NSD_IP]),
        "unbound" => Ok(vec![UNBOUND_IP]),
        "both" => Ok(vec![NSD_IP, UNBOUND_IP]),
        other => bail!("--server must be nsd, unbound or both, got {other:?}"),
    }
}

/// A long-running measurement client on the internal bridge.
struct ClientGuard {
    name: String,
}

impl ClientGuard {
    fn start(root: &Path) -> Result<Self> {
        let _ = Command::new("docker")
            .args(["rm", "-f", RTT_CLIENT_NAME])
            .output();
        let output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                RTT_CLIENT_NAME,
                "--net",
                "climb-net",
                "--ip",
                RTT_CLIENT_IP,
                "--cap-add",
                "NET_ADMIN",
                "-v",
                &format!(
                    "{}/scripts/mtl_rtt_probe.py:/scripts/mtl_rtt_probe.py:ro",
                    root.display()
                ),
                "-v",
                &format!(
                    "{}/scripts/mtl_dns_capture.py:/scripts/mtl_dns_capture.py:ro",
                    root.display()
                ),
                "--entrypoint",
                "sleep",
                DIG_IMAGE,
                "infinity",
            ])
            .output()
            .context("failed to start the measurement client")?;
        if !output.status.success() {
            bail!(
                "failed to start the measurement client: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let guard = Self {
            name: RTT_CLIENT_NAME.to_string(),
        };
        for _ in 0..50 {
            if guard.is_running() {
                return Ok(guard);
            }
            thread::sleep(Duration::from_millis(100));
        }
        bail!("measurement client did not start within 5 s")
    }

    fn is_running(&self) -> bool {
        Command::new("docker")
            .args(["inspect", "-f", "{{.State.Running}}", self.name.as_str()])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
            .unwrap_or(false)
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", self.name.as_str()])
            .output();
    }
}

/// RAII guard: tears down the DNS pipeline on drop.
struct PipelineGuard {
    root: std::path::PathBuf,
}

impl PipelineGuard {
    fn new(root: std::path::PathBuf) -> Self {
        Self { root }
    }
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let _ = compose_cmd(&self.root, &["down", "--remove-orphans"]).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(samples: u64) -> RttCell {
        RttCell {
            server: "nsd".to_string(),
            mode: "plain".to_string(),
            delay_ms: 0.0,
            samples,
            truncated: 0,
            transport: "udp".to_string(),
            rrsig_kind: "full".to_string(),
            median_ms: 1.0,
            p95_ms: 2.0,
            mechanism: "tc-netem".to_string(),
            udp_attempts: 1,
            tcp_attempts: 0,
            udp_response_bytes: 300,
            tcp_response_bytes: 0,
            median_total_ms: 1.0,
            median_rtt_multiple: None,
            queries: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn artifact(mode: &str, cells: Vec<RttCell>) -> RttArtifact {
        RttArtifact {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            mode: mode.to_string(),
            recorded_at: "fixed".to_string(),
            git_commit: None,
            hostname: "host".to_string(),
            uname: "linux".to_string(),
            label: DNS_RTT_LABEL.to_string(),
            superseded: None,
            mechanism: "tc-netem".to_string(),
            netem: "test".to_string(),
            zone: RTT_ZONE.to_string(),
            query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
            cells,
            min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
            class: "measured".to_string(),
            warnings: Vec::new(),
            netem_validation: Vec::new(),
        }
    }

    #[test]
    fn a_quick_artifact_never_qualifies_as_full_evidence() {
        let artifact = artifact("quick", vec![cell(DNS_RTT_FULL_SAMPLES)]);
        assert!(
            !artifact.is_full_evidence(),
            "quick mode is not full evidence"
        );
    }

    #[test]
    fn a_full_artifact_with_all_cells_at_min_samples_qualifies() {
        let mut cells = Vec::new();
        for &d in &DNS_RTT_DELAYS_MS {
            for s in ["nsd", "unbound"] {
                for m in ["plain", "mtl-condensed"] {
                    let mut row = cell(DNS_RTT_FULL_SAMPLES);
                    row.server = s.to_string();
                    row.mode = m.to_string();
                    row.delay_ms = d;
                    cells.push(row);
                }
            }
        }
        let artifact = artifact("full", cells);
        assert!(
            artifact.is_full_evidence(),
            "full mode with n≥200 in every cell qualifies"
        );
    }

    #[test]
    fn a_full_artifact_with_short_cells_does_not_qualify() {
        let artifact = artifact("full", vec![cell(DNS_RTT_FULL_SAMPLES - 1)]);
        assert!(
            !artifact.is_full_evidence(),
            "short cells disqualify full evidence"
        );
    }

    #[test]
    fn a_superseded_artifact_is_never_full_evidence() {
        let mut artifact = artifact("full", vec![cell(DNS_RTT_FULL_SAMPLES)]);
        artifact.superseded = Some("harness invalid".to_string());
        assert!(!artifact.is_full_evidence());
    }

    #[test]
    fn delays_cover_the_required_levels() {
        assert_eq!(DNS_RTT_DELAYS_MS, [0.0, 20.0, 50.0, 100.0]);
    }

    #[test]
    fn full_samples_meets_the_minimum() {
        const {
            assert!(DNS_RTT_FULL_SAMPLES >= 200);
        }
    }

    #[test]
    fn tolerance_is_ten_percent_above_zero() {
        assert!(within_tolerance(20.0, 20.5));
        assert!(within_tolerance(20.0, 18.0));
        assert!(!within_tolerance(20.0, 22.1));
        assert!(!within_tolerance(100.0, 89.9));
        assert!(within_tolerance(0.0, 0.4));
        assert!(!within_tolerance(0.0, 1.1));
    }

    #[test]
    fn ping_average_parses_busybox_and_iputils_output() {
        let busybox = "--- 192.168.13.2 ping statistics ---\n\
                       20 packets transmitted, 20 packets received, 0% packet loss\n\
                       round-trip min/avg/max = 0.079/0.120/0.186 ms";
        assert_eq!(ping_average_from_output(busybox), Some(0.120));
        let iputils = "rtt min/avg/max/mdev = 20.298/20.943/22.142/0.5 ms";
        assert_eq!(ping_average_from_output(iputils), Some(20.943));
        assert_eq!(ping_average_from_output("no statistics here"), None);
    }

    #[test]
    fn the_server_selector_rejects_an_unknown_name() {
        assert_eq!(selected_servers("nsd").expect("nsd"), vec![NSD_IP]);
        assert_eq!(
            selected_servers("both").expect("both"),
            vec![NSD_IP, UNBOUND_IP]
        );
        assert!(selected_servers("resolver").is_err());
    }

    #[test]
    fn only_unbound_needs_its_cache_flushed_before_each_cell() {
        assert!(server_needs_cache_flush(UNBOUND_IP));
        assert!(!server_needs_cache_flush(NSD_IP));
    }
}
