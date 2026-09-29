//! The `pin-env` subcommand: capture a complete build + runtime fingerprint.
//!
//! ```text
//! cargo xtask pin-env
//! ```
//!
//! Writes `environment.json` at the workspace root.  This file is the single
//! machine-readable fingerprint PS-036 asks for under *"record benchmark
//! configuration"* — host identity, toolchain, submodule commit SHAs (read
//! from `git submodule status`, never hand-typed), the Docker images used to
//! produce the pinned native artifacts, and the Phase-F3 CPU governor /
//! `taskset` pinning state.
//!
//! # Single-source-of-truth
//!
//! Before this module existed, resolved submodule SHAs and the Docker digest
//! lived only in the hand-maintained `ENVIRONMENT.md`, split away from
//! `environment.json`.  That split meant two files could silently disagree.
//! `environment.json` is now the authoritative fingerprint; `ENVIRONMENT.md`
//! retains the *provenance and build-recipe* narrative (repo URLs, branch,
//! compile flags, verification status) and points here for the resolved values.
//!
//! # Failure philosophy
//!
//! Fingerprinting must never panic or abort a run.  Every probe (git, docker,
//! sysfs, procfs) degrades to an explicit empty/`None`/`false` value with the
//! reason recorded, so a partial fingerprint is still written rather than no
//! fingerprint at all.

use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// The current `environment.json` layout version.
///
/// Bump when a field is removed or its meaning changes; additive fields do not
/// require a bump.
pub const SCHEMA_VERSION: u32 = 1;

/// A complete build + runtime environment fingerprint.
#[derive(Debug, Serialize)]
pub(crate) struct Environment {
    pub schema_version: u32,
    pub timestamp: String,
    pub system: SystemInfo,
    pub toolchain: ToolchainInfo,
    pub cpu: CpuInfo,
    pub submodules: Vec<SubmoduleInfo>,
    pub docker: DockerInfo,
    pub benchmark_runtime: BenchmarkRuntime,
}

#[derive(Debug, Serialize)]
pub(crate) struct SystemInfo {
    pub uname: String,
    pub kernel_release: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolchainInfo {
    pub rustc_version: String,
    pub cargo_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CpuInfo {
    pub model_name: String,
    /// Number of logical CPUs (hyperthreads included).
    pub logical_cpus: usize,
    /// Number of distinct physical cores, when `/proc/cpuinfo` exposes
    /// `physical id` / `core id`.  `None` on platforms that do not.
    pub physical_cores: Option<usize>,
}

/// One git submodule's resolved state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubmoduleInfo {
    pub path: String,
    /// Resolved commit SHA from `git submodule status`.
    pub sha: String,
    /// `git describe`-style label in parentheses when available
    /// (e.g. `0.16.0` or `MTLLIB_1_2_1-3-g53ce25d`).
    pub describe: Option<String>,
    /// `up-to-date` | `initialized-different` | `not-initialized` | `conflict`.
    pub state: String,
}

/// Docker images involved in producing the pinned native artifacts.
#[derive(Debug, Serialize)]
pub(crate) struct DockerInfo {
    /// `docker` binary found on `PATH`.
    pub cli_available: bool,
    /// The daemon answered (`docker images` succeeded).
    pub daemon_reachable: bool,
    /// Base image reference parsed from `docker/Dockerfile.build` (e.g.
    /// `debian:bookworm-slim`).
    pub base_image_ref: Option<String>,
    /// Base image digest parsed from `docker/Dockerfile.build`
    /// (e.g. `sha256:3783…`).  This is the reproducible pin.
    pub base_image_digest: Option<String>,
    /// CLIMB-related images found locally, sorted by reference.
    pub images: Vec<DockerImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DockerImage {
    pub reference: String,
    /// Local image ID (`sha256:…`).
    pub image_id: String,
    /// Registry repo digest when the image was pulled (`None` for a purely
    /// local build, which never has one).
    pub repo_digest: Option<String>,
}

/// Phase-F3 CPU governor + `taskset` pinning state.
#[derive(Debug, Serialize)]
pub(crate) struct BenchmarkRuntime {
    /// Unified governor name, or `"mixed"` when CPUs disagree, or
    /// `"unavailable"` when cpufreq sysfs is not present.
    pub cpu_governor: String,
    /// Per-CPU governor readings, sorted by CPU index.
    pub cpu_governor_per_cpu: Vec<CpuGovernor>,
    /// Where the governor readings came from.
    pub governor_source: String,
    /// The logical CPU `xtask bench-all` pins to.
    pub pin_cpu: u32,
    /// The pinning expression applied by `xtask bench-all`.
    pub pin_expression: String,
    /// `taskset` found on `PATH`.
    pub taskset_available: bool,
    /// `Cpus_allowed_list` from `/proc/self/status` at capture time — the
    /// actual affinity of the process running `pin-env`.
    pub current_cpus_allowed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CpuGovernor {
    pub cpu: u32,
    pub governor: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Capture the environment and write `environment.json` at the workspace root.
pub(crate) fn cmd_pin_env() -> Result<()> {
    let root = workspace_root()?;
    let env = capture(&root)?;

    let json = serde_json::to_string_pretty(&env).context("failed to serialize environment")?;
    let output_path = root.join("environment.json");
    fs::write(&output_path, json).context("failed to write environment.json")?;

    eprintln!("wrote environment snapshot to {}", output_path.display());
    Ok(())
}

/// Workspace root = the parent of the `xtask` crate directory.
fn workspace_root() -> Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = dir
        .parent()
        .context("xtask is expected to live directly inside the workspace root")?;
    Ok(root.to_path_buf())
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// Collect every fingerprint section.  Pure-probe failures degrade to
/// documented empties, never errors (see module docs).
fn capture(root: &Path) -> Result<Environment> {
    let uname = run_cmd("uname", &["-a"]).unwrap_or_else(|_| "unknown".to_string());
    let kernel_release = run_cmd("uname", &["-r"]).unwrap_or_else(|_| "unknown".to_string());
    let rustc_version = run_cmd("rustc", &["--version"]).unwrap_or_else(|_| "unknown".to_string());
    let cargo_version = run_cmd("cargo", &["--version"]).unwrap_or_else(|_| "unknown".to_string());
    let timestamp = iso_timestamp();

    let cpu = capture_cpu();
    let submodules = capture_submodules(root);
    let docker = capture_docker(root);
    let benchmark_runtime = capture_benchmark_runtime();

    Ok(Environment {
        schema_version: SCHEMA_VERSION,
        timestamp,
        system: SystemInfo {
            uname,
            kernel_release,
        },
        toolchain: ToolchainInfo {
            rustc_version,
            cargo_version,
        },
        cpu,
        submodules,
        docker,
        benchmark_runtime,
    })
}

fn capture_cpu() -> CpuInfo {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model_name = parse_cpu_model(&cpuinfo).unwrap_or_else(|| "unknown".to_string());
    let logical_cpus = parse_logical_cpu_count(&cpuinfo);
    let physical_cores = parse_physical_core_count(&cpuinfo);
    CpuInfo {
        model_name,
        logical_cpus,
        physical_cores,
    }
}

/// Read submodule SHAs from `git submodule status` in the workspace root.
fn capture_submodules(root: &Path) -> Vec<SubmoduleInfo> {
    match run_cmd_in(root, "git", &["submodule", "status"]) {
        Ok(out) => parse_submodule_status(&out),
        Err(_) => Vec::new(),
    }
}

fn capture_docker(root: &Path) -> DockerInfo {
    let cli_available = crate::binary_on_path("docker");

    // Parse the pinned base image out of the Dockerfile regardless of whether
    // a daemon is reachable — it is a repo fact, not a daemon fact.
    let dockerfile = root.join("docker").join("Dockerfile.build");
    let (base_image_ref, base_image_digest) = fs::read_to_string(&dockerfile)
        .ok()
        .and_then(|text| parse_dockerfile_base_image(&text))
        .map_or((None, None), |(r, d)| (Some(r), Some(d)));

    let mut images = Vec::new();
    let mut daemon_reachable = false;

    if cli_available {
        // `--digests` yields the Digest column (empty for local-only builds).
        if let Ok(out) = run_cmd(
            "docker",
            &[
                "images",
                "--no-trunc",
                "--format",
                "{{.Repository}}|{{.Tag}}|{{.ID}}|{{.Digest}}",
            ],
        ) {
            daemon_reachable = true;
            images = parse_docker_images(&out);
        }
    }

    DockerInfo {
        cli_available,
        daemon_reachable,
        base_image_ref,
        base_image_digest,
        images,
    }
}

fn capture_benchmark_runtime() -> BenchmarkRuntime {
    let per_cpu = read_governors();
    let (governor, source) = summarize_governors(&per_cpu);
    BenchmarkRuntime {
        cpu_governor: governor,
        cpu_governor_per_cpu: per_cpu,
        governor_source: source,
        pin_cpu: crate::PIN_CPU,
        pin_expression: format!("taskset -c {}", crate::PIN_CPU),
        taskset_available: crate::binary_on_path("taskset"),
        current_cpus_allowed: read_cpus_allowed(),
    }
}

/// Read `scaling_governor` for every `cpuN` present, sorted by index.
fn read_governors() -> Vec<CpuGovernor> {
    let mut governors = Vec::new();
    let base = Path::new("/sys/devices/system/cpu");
    let entries = match fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return governors,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(index) = name.strip_prefix("cpu").and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let path = entry.path().join("cpufreq").join("scaling_governor");
        if let Ok(governor) = fs::read_to_string(&path) {
            governors.push(CpuGovernor {
                cpu: index,
                governor: governor.trim().to_string(),
            });
        }
    }

    governors.sort_by_key(|g| g.cpu);
    governors
}

// ---------------------------------------------------------------------------
// Pure parsers — unit-testable without git / docker / sysfs
// ---------------------------------------------------------------------------

/// Parse the output of `git submodule status`.
///
/// Each line is `[ +-U]<sha> <path>[ (<describe>)]` where the leading marker is
/// a space for an up-to-date submodule, `-` for one that was never
/// initialized, `+` for one checked out at a different commit than the index
/// records, and `U` for a merge conflict.
pub(crate) fn parse_submodule_status(output: &str) -> Vec<SubmoduleInfo> {
    let mut out = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (marker, rest) = match line.as_bytes().first() {
            Some(b' ') => (' ', &line[1..]),
            Some(b'-') => ('-', &line[1..]),
            Some(b'+') => ('+', &line[1..]),
            Some(b'U') => ('U', &line[1..]),
            _ => (' ', line),
        };
        let Some((sha, remainder)) = rest.trim_start().split_once(' ') else {
            continue;
        };
        let remainder = remainder.trim();
        // Split an optional trailing "(describe)".
        let (path, describe) = match remainder.rfind(" (") {
            Some(idx) if remainder.ends_with(')') => {
                let path = remainder[..idx].trim();
                let desc = remainder[idx + 2..remainder.len() - 1].trim();
                (path, Some(desc.to_string()))
            }
            _ => (remainder, None),
        };
        if path.is_empty() {
            continue;
        }
        let state = match marker {
            '-' => "not-initialized",
            '+' => "initialized-different",
            'U' => "conflict",
            _ => "up-to-date",
        };
        out.push(SubmoduleInfo {
            path: path.to_string(),
            sha: sha.to_string(),
            describe,
            state: state.to_string(),
        });
    }
    out
}

/// Parse the first pinned `FROM <image>@sha256:<digest>` line of a Dockerfile.
///
/// Returns `(reference, digest)` where `digest` includes the `sha256:` prefix.
pub(crate) fn parse_dockerfile_base_image(dockerfile: &str) -> Option<(String, String)> {
    for line in dockerfile.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("FROM ") else {
            continue;
        };
        // Drop an optional ` AS <stage>` suffix.
        let token = rest.split_whitespace().next()?;
        // Look for an explicit digest pin.
        let (reference, digest) = token.split_once('@')?;
        if digest.starts_with("sha256:") {
            return Some((reference.to_string(), digest.to_string()));
        }
        return None; // A `FROM` without a digest is not a pin.
    }
    None
}

/// Parse `docker images --format '{{.Repository}}|{{.Tag}}|{{.ID}}|{{.Digest}}'`,
/// keeping only images whose repository name references CLIMB.
///
/// Sorted by reference for deterministic output.
pub(crate) fn parse_docker_images(output: &str) -> Vec<DockerImage> {
    let mut images = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 4 {
            continue;
        }
        let (repo, tag, id, digest) = (parts[0], parts[1], parts[2], parts[3]);
        if !repo.to_ascii_lowercase().contains("climb") {
            continue;
        }
        let reference = if tag.is_empty() || tag == "<none>" {
            repo.to_string()
        } else {
            format!("{repo}:{tag}")
        };
        images.push(DockerImage {
            reference,
            image_id: id.to_string(),
            repo_digest: if digest.is_empty() {
                None
            } else {
                Some(digest.to_string())
            },
        });
    }
    images.sort_by(|a, b| a.reference.cmp(&b.reference));
    images
}

/// Collapse per-CPU governor readings into a single label plus its source.
pub(crate) fn summarize_governors(per_cpu: &[CpuGovernor]) -> (String, String) {
    if per_cpu.is_empty() {
        return (
            "unavailable".to_string(),
            "cpufreq sysfs absent".to_string(),
        );
    }
    let first = per_cpu[0].governor.clone();
    if per_cpu.iter().all(|g| g.governor == first) {
        (first, "cpufreq sysfs".to_string())
    } else {
        ("mixed".to_string(), "cpufreq sysfs".to_string())
    }
}

fn parse_cpu_model(cpuinfo: &str) -> Option<String> {
    for line in cpuinfo.lines() {
        if let Some(model) = line.strip_prefix("model name") {
            if let Some((_, value)) = model.split_once(':') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn parse_logical_cpu_count(cpuinfo: &str) -> usize {
    cpuinfo
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count()
}

/// Count distinct `(physical id, core id)` pairs, if both fields are present.
///
/// Falls back to `None` when the kernel does not expose them (e.g. some ARM
/// and virtualized environments).
fn parse_physical_core_count(cpuinfo: &str) -> Option<usize> {
    let mut pairs = std::collections::BTreeSet::new();
    let mut physical: Option<u32> = None;
    let mut core: Option<u32> = None;

    let flush = |physical: &mut Option<u32>,
                 core: &mut Option<u32>,
                 pairs: &mut std::collections::BTreeSet<(u32, u32)>| {
        if let (Some(p), Some(c)) = (physical.take(), core.take()) {
            pairs.insert((p, c));
        }
    };

    for line in cpuinfo.lines() {
        if line.starts_with("processor") {
            // A new CPU block: flush the previous one.
            flush(&mut physical, &mut core, &mut pairs);
        } else if let Some((_, v)) = line.split_once(':') {
            let key = line.split_once(':').map(|(k, _)| k.trim()).unwrap_or("");
            let value = v.trim();
            match key {
                "physical id" => physical = value.parse().ok(),
                "core id" => core = value.parse().ok(),
                _ => {}
            }
        }
    }
    flush(&mut physical, &mut core, &mut pairs);

    if pairs.is_empty() {
        None
    } else {
        Some(pairs.len())
    }
}

fn read_cpus_allowed() -> Option<String> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Cpus_allowed_list:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Subprocess helpers
// ---------------------------------------------------------------------------

/// Run a command and capture its stdout, trimmed.
fn run_cmd(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        anyhow::bail!("{program} exited with status {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a command with a working directory, capturing trimmed stdout.
fn run_cmd_in(dir: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("failed to run {program} in {}", dir.display()))?;
    if !output.status.success() {
        anyhow::bail!("{program} exited with status {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Return an ISO 8601 timestamp without pulling in the `chrono` crate.
fn iso_timestamp() -> String {
    run_cmd("date", &["--iso-8601=seconds"]).unwrap_or_else(|_| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_submodule_status_lines() {
        let sample = "\
 53ce25dcc35c15e051c5263e41aaeaede18a8aa7 third_party/MTL (MTLLIB_1_2_1-3-g53ce25d)
 5a1a854b0dc9f2141bdc771c555ee60c37950183 third_party/liboqs (0.16.0)
";
        let subs = parse_submodule_status(sample);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].path, "third_party/MTL");
        assert_eq!(subs[0].sha, "53ce25dcc35c15e051c5263e41aaeaede18a8aa7");
        assert_eq!(subs[0].describe.as_deref(), Some("MTLLIB_1_2_1-3-g53ce25d"));
        assert_eq!(subs[0].state, "up-to-date");
        assert_eq!(subs[1].path, "third_party/liboqs");
        assert_eq!(subs[1].describe.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn parses_uninitialized_and_modified_markers() {
        let sample = "\
-1111111111111111111111111111111111111111 third_party/liboqs
+2222222222222222222222222222222222222222 third_party/MTL (v1)
";
        let subs = parse_submodule_status(sample);
        assert_eq!(subs[0].state, "not-initialized");
        assert_eq!(subs[1].state, "initialized-different");
    }

    #[test]
    fn submodule_status_without_describe() {
        let subs = parse_submodule_status("  abc123 third_party/thing\n");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].sha, "abc123");
        assert_eq!(subs[0].path, "third_party/thing");
        assert_eq!(subs[0].describe, None);
    }

    #[test]
    fn parses_dockerfile_pinned_base_image() {
        let df = "\
# comment
FROM debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251 AS builder
RUN true
";
        let (reference, digest) = parse_dockerfile_base_image(df).expect("pin");
        assert_eq!(reference, "debian:bookworm-slim");
        assert_eq!(
            digest,
            "sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251"
        );
    }

    #[test]
    fn dockerfile_without_digest_is_not_a_pin() {
        assert!(parse_dockerfile_base_image("FROM debian:bookworm-slim AS b\n").is_none());
        assert!(parse_dockerfile_base_image("RUN echo hi\n").is_none());
    }

    #[test]
    fn parses_and_filters_docker_images() {
        let sample = "\
docker-climb-smoke|latest|sha256:dac97e73d810f9b66e02a8973e04a4845545524501f57c7f2b6ded2e9dfa802e|docker-climb-smoke@sha256:dac97e73d810f9b66e02a8973e04a4845545524501f57c7f2b6ded2e9dfa802e
climb-builder|phase1|sha256:084d30888a6dc7d97bea5cbe28da1bc2286a1bb86940a12c9d2088d0e732d81d|
nginx|latest|sha256:aaaa|
";
        let images = parse_docker_images(sample);
        // nginx is filtered out.
        assert_eq!(images.len(), 2);
        // Sorted by reference: "climb-builder:phase1" < "docker-climb-smoke:latest".
        assert_eq!(images[0].reference, "climb-builder:phase1");
        assert_eq!(images[0].repo_digest, None);
        assert_eq!(images[1].reference, "docker-climb-smoke:latest");
        assert!(images[1].repo_digest.is_some());
    }

    #[test]
    fn summarize_governors_uniform_mixed_and_empty() {
        let uniform = vec![
            CpuGovernor {
                cpu: 0,
                governor: "performance".into(),
            },
            CpuGovernor {
                cpu: 1,
                governor: "performance".into(),
            },
        ];
        assert_eq!(
            summarize_governors(&uniform),
            ("performance".to_string(), "cpufreq sysfs".to_string())
        );

        let mixed = vec![
            CpuGovernor {
                cpu: 0,
                governor: "performance".into(),
            },
            CpuGovernor {
                cpu: 1,
                governor: "powersave".into(),
            },
        ];
        assert_eq!(summarize_governors(&mixed).0, "mixed");

        assert_eq!(summarize_governors(&[]).0, "unavailable");
    }

    #[test]
    fn parses_physical_core_count_from_cpuinfo() {
        // Two physical packages, two cores each, two hyperthreads per core.
        let cpuinfo = "\
processor\t: 0
physical id\t: 0
core id\t\t: 0

processor\t: 1
physical id\t: 0
core id\t\t: 1

processor\t: 2
physical id\t: 1
core id\t\t: 0

processor\t: 3
physical id\t: 1
core id\t\t: 1
";
        assert_eq!(parse_logical_cpu_count(cpuinfo), 4);
        assert_eq!(parse_physical_core_count(cpuinfo), Some(4));
    }

    #[test]
    fn physical_core_count_none_when_fields_absent() {
        let cpuinfo = "processor\t: 0\nmodel name\t: ARM\n\nprocessor\t: 1\n";
        assert_eq!(parse_physical_core_count(cpuinfo), None);
    }

    #[test]
    fn parses_cpu_model() {
        let cpuinfo =
            "processor\t: 0\nmodel name\t: 11th Gen Intel(R) Core(TM) i3-1115G4 @ 3.00GHz\n";
        assert_eq!(
            parse_cpu_model(cpuinfo).as_deref(),
            Some("11th Gen Intel(R) Core(TM) i3-1115G4 @ 3.00GHz")
        );
    }
}
