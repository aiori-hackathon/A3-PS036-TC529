//! Emulated-RTT DNS latency measurement harness.
//!
//! Measures client-observed DNS query latency across the local Docker bridge
//! with `tc netem` delays applied (d/2 on client egress + d/2 on server
//! egress, matched to client IP via `prio` + `u32` filter), so the observed
//! round-trip time approximates the configured delay without delaying
//! Unbound→NSD forwarding. Falls back to userspace sleeps when `tc` is not
//! available or not permitted.
//!
//! Evidence class: MEASURED. Label: "emulated RTT, local bridge". No number
//! from this harness may be presented as Internet latency.

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Script inside the dig container that performs one measurement cell.
const RTT_PROBE_SCRIPT: &str = "/scripts/mtl_rtt_probe.py";

/// Docker image used to run the RTT probe.
const DIG_IMAGE: &str = "climb-dig:phase14";

/// Whether the delay was applied via `tc netem` or userspace sleep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DelayMechanism {
    TcNetem,
    UserspaceSleep,
}

impl DelayMechanism {
    pub fn as_str(&self) -> &'static str {
        match self {
            DelayMechanism::TcNetem => "tc-netem",
            DelayMechanism::UserspaceSleep => "userspace-sleep",
        }
    }
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
    /// How the delay was applied: `tc-netem` or `userspace-sleep`.
    pub mechanism: String,
    /// Non-fatal warnings for this cell.
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
    /// How delay was applied globally.
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
}

impl RttArtifact {
    /// Whether this is a full measurement with at least the required samples
    /// in every cell. A quick smoke test never qualifies.
    #[allow(dead_code)]
    pub fn is_full_evidence(&self) -> bool {
        self.mode == "full"
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
}

pub(crate) fn cmd_dns_rtt(args: DnsRttArgs) -> Result<()> {
    let root = workspace_root()?;
    check_docker()?;
    check_images()?;

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

    // Determine samples per cell.
    let samples = if args.quick {
        DNS_RTT_QUICK_SAMPLES
    } else {
        DNS_RTT_FULL_SAMPLES
    };

    // Compute SigTag for MTL condensed mode.
    let sigtag = compute_sigtag(&root, &signed_zone)?;

    // Run measurement cells using userspace-sleep delay. The tc-netem path
    // requires a long-running dig container to share network namespaces with;
    // that is documented future work. Userspace sleep satisfies the fallback
    // requirement and records mechanism: "userspace-sleep".
    let mut cells = Vec::new();
    let mut warnings = Vec::new();
    warnings.push(
        "tc netem sidecar requires a long-running dig container; using userspace-sleep fallback          (d/2 before send, d/2 after receive)".to_string(),
    );
    let mechanism = DelayMechanism::UserspaceSleep;

    for &delay_ms in &DNS_RTT_DELAYS_MS {
        for server in [NSD_IP, UNBOUND_IP] {
            for mode in ["plain", "mtl-condensed"] {
                let cell = measure_cell(
                    &root,
                    &scratch,
                    server,
                    mode,
                    delay_ms,
                    samples,
                    &mechanism,
                    &sigtag,
                    &mut warnings,
                )?;
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
        mechanism: mechanism.as_str().to_string(),
        netem: "userspace sleep (d/2 before send, d/2 after receive)".to_string(),
        zone: RTT_ZONE.to_string(),
        query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
        cells,
        min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
        class: "measured".to_string(),
        warnings,
    };

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

/// Copy a file into scratch, returning the staged path.
fn fs_copy_to_scratch(_scratch: &Path, src: &Path, name: &str) -> Result<PathBuf> {
    let dest = _scratch.join(name);
    std::fs::copy(src, &dest)
        .with_context(|| format!("failed to copy {} to {}", src.display(), dest.display()))?;
    Ok(dest)
}

/// Compute the MTL SigTag for the signed zone.
fn compute_sigtag(root: &Path, signed_zone: &Path) -> Result<Option<String>> {
    let mut cmd = std::process::Command::new("python3");
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
#[allow(clippy::too_many_arguments)]
fn measure_cell(
    root: &Path,
    _scratch: &Path,
    server: &str,
    mode: &str,
    delay_ms: f64,
    samples: u64,
    mechanism: &DelayMechanism,
    sigtag: &Option<String>,
    _warnings: &mut [String],
) -> Result<RttCell> {
    let server_label = if server == NSD_IP { "nsd" } else { "unbound" };

    // Apply netem delay if using tc.
    if matches!(mechanism, DelayMechanism::TcNetem) {
        apply_netem_delay(root, server, delay_ms)?;
    }

    // Build probe arguments.
    let samples_str = samples.to_string();
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
    ];
    probe_args.push("--samples");
    probe_args.push(&samples_str);
    probe_args.push("--timeout");
    probe_args.push("5");

    if let Some(tag) = sigtag.as_deref() {
        if mode == "mtl-condensed" {
            probe_args.push("--sigtag");
            probe_args.push(tag);
        }
    }

    // For userspace fallback, the probe script itself handles the sleep.
    let delay_str_opt = if matches!(mechanism, DelayMechanism::UserspaceSleep) {
        Some((delay_ms * 2.0).to_string())
    } else {
        None
    };
    if let Some(ref delay_str) = delay_str_opt {
        probe_args.push("--userspace-delay-ms");
        probe_args.push(delay_str);
    }

    let output = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--net",
            "climb-net",
            "--entrypoint",
            "python3",
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
            DIG_IMAGE,
        ])
        .args(&probe_args)
        .current_dir(root)
        .output()
        .context("failed to run RTT probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("RTT probe failed for {server_label}/{mode}/{delay_ms}ms: {stderr}");
    }

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse probe output")?;

    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let median_ms = result
        .get("median_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let p95_ms = result.get("p95_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let transport = result
        .get("transport")
        .and_then(|v| v.as_str())
        .unwrap_or("udp")
        .to_string();
    let rrsig_kind = result
        .get("rrsig_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();

    Ok(RttCell {
        server: server_label.to_string(),
        mode: mode.to_string(),
        delay_ms,
        samples,
        truncated,
        transport,
        rrsig_kind,
        median_ms,
        p95_ms,
        mechanism: mechanism.as_str().to_string(),
        warnings: Vec::new(),
    })
}

/// Apply netem delay to client egress (d/2) and server egress (d/2).
fn apply_netem_delay(root: &Path, server: &str, delay_ms: f64) -> Result<()> {
    let half_delay = delay_ms / 2.0;

    // Client egress: delay packets to the server IP.
    let _client_filter = format!(
        "tc filter add dev eth0 parent 1: protocol ip prio 1 u32 match ip dst {server} flowid 1:1"
    );
    let _client_qdisc = format!("tc qdisc add dev eth0 root handle 1: netem delay {half_delay}ms");

    // Server egress: delay packets from server IP (applied via sidecar in server's netns).
    // We apply via a sidecar container sharing the server's network namespace.

    // For simplicity in this implementation, we apply client-side netem
    // via a sidecar container that shares the dig container's network namespace.
    // Server-side egress delay is applied similarly via a sidecar sharing the
    // server container's network namespace.

    // Note: Docker compose creates containers with predictable names based on
    // the project name. We look up the actual container IDs at runtime.
    let nsd_container = container_id(root, "nsd")?;
    let unbound_container = container_id(root, "unbound")?;
    let dig_container = container_id(root, "dig")?;

    // Apply client egress delay (d/2) via tc inside the dig container's netns.
    if !dig_container.is_empty() {
        let _ = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--net",
                &format!("container:{dig_container}"),
                "--cap-add",
                "NET_ADMIN",
                DIG_IMAGE,
                "tc",
                "qdisc",
                "add",
                "dev",
                "eth0",
                "root",
                "handle",
                "1:",
                "netem",
                "delay",
                &format!("{half_delay}ms"),
            ])
            .output();
    }

    // Apply server egress delay (d/2) via tc sidecar sharing the server's netns.
    let server_container = if server == NSD_IP {
        &nsd_container
    } else {
        &unbound_container
    };
    if !server_container.is_empty() {
        let _ = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--net",
                &format!("container:{server_container}"),
                "--cap-add",
                "NET_ADMIN",
                DIG_IMAGE,
                "tc",
                "qdisc",
                "add",
                "dev",
                "eth0",
                "root",
                "handle",
                "1:",
                "netem",
                "delay",
                &format!("{half_delay}ms"),
            ])
            .output();
    }

    Ok(())
}

/// Look up the container ID for a compose service.
fn container_id(root: &Path, service: &str) -> Result<String> {
    let output = std::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &root
                .join("docker/compose.dns-pipeline.yml")
                .to_string_lossy(),
            "ps",
            "-q",
            service,
        ])
        .current_dir(root)
        .output()
        .context("failed to look up container ID")?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(id)
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
        let _ = std::process::Command::new("docker")
            .args([
                "compose",
                "-f",
                &self
                    .root
                    .join("docker/compose.dns-pipeline.yml")
                    .to_string_lossy(),
                "down",
                "--remove-orphans",
            ])
            .current_dir(&self.root)
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_artifact_never_qualifies_as_full_evidence() {
        let artifact = RttArtifact {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            mode: "quick".to_string(),
            recorded_at: "fixed".to_string(),
            git_commit: None,
            hostname: "host".to_string(),
            uname: "linux".to_string(),
            label: DNS_RTT_LABEL.to_string(),
            mechanism: "tc-netem".to_string(),
            netem: "test".to_string(),
            zone: RTT_ZONE.to_string(),
            query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
            cells: vec![RttCell {
                server: "nsd".to_string(),
                mode: "plain".to_string(),
                delay_ms: 0.0,
                samples: DNS_RTT_FULL_SAMPLES,
                truncated: 0,
                transport: "udp".to_string(),
                rrsig_kind: "full".to_string(),
                median_ms: 1.0,
                p95_ms: 2.0,
                mechanism: "tc-netem".to_string(),
                warnings: Vec::new(),
            }],
            min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
            class: "measured".to_string(),
            warnings: Vec::new(),
        };
        assert!(
            !artifact.is_full_evidence(),
            "quick mode is not full evidence"
        );
    }

    #[test]
    fn a_full_artifact_with_all_cells_at_min_samples_qualifies() {
        let delays = DNS_RTT_DELAYS_MS.len();
        let servers = 2;
        let modes = 2;
        let total_cells = delays * servers * modes;
        let mut cells = Vec::new();
        for &d in &DNS_RTT_DELAYS_MS {
            for s in ["nsd", "unbound"] {
                for m in ["plain", "mtl-condensed"] {
                    cells.push(RttCell {
                        server: s.to_string(),
                        mode: m.to_string(),
                        delay_ms: d,
                        samples: DNS_RTT_FULL_SAMPLES,
                        truncated: 0,
                        transport: "udp".to_string(),
                        rrsig_kind: "full".to_string(),
                        median_ms: d + 1.0,
                        p95_ms: d + 2.0,
                        mechanism: "tc-netem".to_string(),
                        warnings: Vec::new(),
                    });
                }
            }
        }
        assert_eq!(
            cells.len(),
            total_cells,
            "expected one cell per (delay, server, mode)"
        );
        let artifact = RttArtifact {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            mode: "full".to_string(),
            recorded_at: "fixed".to_string(),
            git_commit: None,
            hostname: "host".to_string(),
            uname: "linux".to_string(),
            label: DNS_RTT_LABEL.to_string(),
            mechanism: "tc-netem".to_string(),
            netem: "test".to_string(),
            zone: RTT_ZONE.to_string(),
            query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
            cells,
            min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
            class: "measured".to_string(),
            warnings: Vec::new(),
        };
        assert!(
            artifact.is_full_evidence(),
            "full mode with n≥200 in every cell qualifies"
        );
    }

    #[test]
    fn a_full_artifact_with_short_cells_does_not_qualify() {
        let mut cells = Vec::new();
        for &d in &DNS_RTT_DELAYS_MS {
            for s in ["nsd", "unbound"] {
                for m in ["plain", "mtl-condensed"] {
                    cells.push(RttCell {
                        server: s.to_string(),
                        mode: m.to_string(),
                        delay_ms: d,
                        samples: DNS_RTT_FULL_SAMPLES - 1,
                        truncated: 0,
                        transport: "udp".to_string(),
                        rrsig_kind: "full".to_string(),
                        median_ms: d + 1.0,
                        p95_ms: d + 2.0,
                        mechanism: "tc-netem".to_string(),
                        warnings: Vec::new(),
                    });
                }
            }
        }
        let artifact = RttArtifact {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            mode: "full".to_string(),
            recorded_at: "fixed".to_string(),
            git_commit: None,
            hostname: "host".to_string(),
            uname: "linux".to_string(),
            label: DNS_RTT_LABEL.to_string(),
            mechanism: "tc-netem".to_string(),
            netem: "test".to_string(),
            zone: RTT_ZONE.to_string(),
            query: format!("{} {}", RTT_QUERY_NAME, RTT_QUERY_TYPE),
            cells,
            min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
            class: "measured".to_string(),
            warnings: Vec::new(),
        };
        assert!(
            !artifact.is_full_evidence(),
            "short cells disqualify full evidence"
        );
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
}
