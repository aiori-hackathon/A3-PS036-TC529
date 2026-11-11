//! The `dns-cache` subcommand: directly measured resolver cache use.
//!
//! ```text
//! cargo xtask dns-cache
//! cargo xtask dns-cache --quick
//! cargo xtask dns-cache --sizes small,medium --repeats 5 --out target/dns-cache.json
//! ```
//!
//! Starts NSD-MTL and Unbound-MTL on the internal Docker bridge, loads `R`
//! distinct signed RRsets into Unbound's cache for each configuration (plain
//! full-signature responses, and MTL-condensed responses backed by one shared
//! ladder), reads Unbound's own cache accounting through
//! `unbound-control stats_noreset` (`mem.cache.rrset`, `mem.cache.message`,
//! entry counts) plus the container's resident memory before and after, and
//! repeats. Everything runs on the internal bridge; nothing is published to
//! the host. Evidence class: `MEASURED`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::dns_demo::{
    check_docker, check_images, compose_cmd, git_head, hostname, run_cmd_inherit, run_in_signer,
    uname, wait_for_nsd_ready, workspace_root, ALGORITHM, SIGTAG_SCRIPT, UNBOUND_IP,
};
use crate::SizeArg;

use climb_report::cache::{parse_unbound_stats, CacheArtifact, CacheRun, UnboundStats};
use climb_report::{CACHE_USE_CLASS, CACHE_USE_METHOD, CACHE_USE_SCHEMA_VERSION};

/// Container name of the recursive resolver whose cache is measured.
const UNBOUND_CONTAINER: &str = "climb-dns-unbound-1";
/// Compose service name of the recursive resolver.
const UNBOUND_SERVICE: &str = "unbound";
/// Path to the `unbound-control` binary inside the Unbound container.
const UNBOUND_CONTROL: &str = "/usr/local/unbound/sbin/unbound-control";
/// Unbound config path inside the container (mounted from the host).
const UNBOUND_CONF: &str = "/etc/unbound/unbound.conf";
/// Cache-loading probe script, run on the host against the resolver's bridge
/// address.
const CACHE_PROBE_SCRIPT: &str = "scripts/mtl_dns_cache_probe.py";

/// RAII guard: tears down the DNS pipeline on drop.
struct PipelineGuard {
    root: PathBuf,
}

impl PipelineGuard {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let _ = compose_cmd(&self.root, &["down", "--remove-orphans"]).status();
    }
}

/// Distinct owner names queried per dataset size. Large is deliberately not in
/// the default set because a million queries would not finish in minutes.
fn size_to_r(size: SizeArg) -> Option<usize> {
    match size {
        SizeArg::Small => Some(100),
        SizeArg::OneK => None,
        SizeArg::Medium => Some(10_000),
        SizeArg::Large => None,
        SizeArg::All => None,
    }
}

/// CLI arguments for `cargo xtask dns-cache`.
#[derive(Args)]
pub(crate) struct DnsCacheArgs {
    /// Output path for the cache-use artifact.
    #[arg(long, default_value = "target/dns-cache.json")]
    pub(crate) out: PathBuf,

    /// Quick smoke-test mode: Small only, three repeats, never published as
    /// full evidence. Intended to validate the harness end-to-end.
    #[arg(long)]
    pub(crate) quick: bool,

    /// Skip building Docker images (assume they already exist).
    #[arg(long)]
    pub(crate) no_build: bool,

    /// Dataset sizes to measure (comma-separated). Defaults to Small and
    /// Medium; Large is measured only when explicitly requested and only if
    /// it finishes in time.
    #[arg(long, value_delimiter = ',', default_value = "small,medium")]
    pub(crate) sizes: Vec<SizeArg>,

    /// How many times to repeat each (size, configuration) cell. The cache is
    /// flushed between every repetition so each run measures cold loading.
    #[arg(long, default_value_t = 5)]
    pub(crate) repeats: u64,

    /// Reuse an existing signed zone file instead of signing fresh.
    #[arg(long, value_name = "PATH")]
    pub(crate) reuse_zone: Option<PathBuf>,
}

pub(crate) fn cmd_dns_cache(args: DnsCacheArgs) -> Result<()> {
    let root = workspace_root()?;
    check_docker()?;
    check_images()?;

    let sizes = selected_sizes(&args.sizes, args.quick);
    if sizes.is_empty() {
        bail!("no dataset sizes selected for cache measurement");
    }

    let mut r_values: Vec<usize> = Vec::new();
    for &size in &sizes {
        if let Some(r) = size_to_r(size) {
            if !r_values.contains(&r) {
                r_values.push(r);
            }
        } else if matches!(size, SizeArg::Large) {
            eprintln!("  {size}: skipped (Large cache measurement would not finish in minutes)");
        }
    }

    let mode = if args.quick { "quick" } else { "full" };
    let repeats = if args.quick { 3 } else { args.repeats };
    let mut runs: Vec<CacheRun> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // ── Sign or reuse the zone(s) ─────────────────────────────────────
    let signed_zones = sign_zones(&root, &sizes, &args.reuse_zone, &mut warnings)?;

    // ── Measure every (size, configuration) cell, one pipeline per size ─
    let configs = [("plain", false), ("mtl-condensed", true)];
    for &size in &sizes {
        let Some(r) = size_to_r(size) else {
            continue;
        };
        let signed_zone = signed_zones
            .get(&size)
            .context(format!("missing signed zone for {size:?}"))?;

        let scratch = root.join("target").join("dns-cache");
        fs::create_dir_all(&scratch).context("failed to create scratch directory")?;
        // NSD always reads `climb.example.zone` from its zone directory, so
        // stage this size's signed zone under that stable name.
        let staged_zone = scratch.join("climb.example.zone");
        fs::copy(signed_zone, &staged_zone)
            .with_context(|| format!("failed to stage signed zone for {size:?}"))?;

        let guard = PipelineGuard::new(root.clone());
        {
            let mut cmd = compose_cmd(&root, &["up", "-d", "nsd", "unbound"]);
            cmd.env("CLIMB_ZONE_DIR", &scratch);
            run_cmd_inherit(cmd)?;
        }
        wait_for_nsd_ready(&root)?;
        eprintln!("  {size}: NSD ready — zone loaded");

        // Compute the SigTag once per size (shared ladder hash for condensed).
        let sigtag = compute_sigtag(&root, &staged_zone)?;
        eprintln!(
            "  {size}: SigTag for MTL-condensed responses: {}",
            sigtag.as_deref().unwrap_or("<none>")
        );

        for (config, use_sigtag) in configs {
            for repeat in 0..repeats {
                // Flush Unbound's in-memory cache before every run so each
                // run measures cold loading of the same `r` names.
                flush_unbound_cache(&root)?;
                eprintln!(
                    "  {size} {config} repeat {repeat}/{repeats}: flushed cache, loading {r} names"
                );

                let rss_before = container_memory_bytes(UNBOUND_CONTAINER)?;
                let _probe = load_names(
                    UNBOUND_IP,
                    r,
                    if use_sigtag { sigtag.as_deref() } else { None },
                )?;
                let rss_after = container_memory_bytes(UNBOUND_CONTAINER)?;
                let stats = read_unbound_stats()?;

                runs.push(CacheRun {
                    config: config.to_string(),
                    r,
                    container_rss_before_bytes: rss_before,
                    container_rss_after_bytes: rss_after,
                    stats: stats.clone(),
                });

                eprintln!(
                    "    rrset_cache={}B ({} entries) message_cache={}B ({} entries) \
                     rss {}→{} B",
                    stats.rrset_cache_bytes,
                    stats.rrset_cache_entries,
                    stats.message_cache_bytes,
                    stats.message_cache_entries,
                    rss_before,
                    rss_after,
                );
            }
        }

        drop(guard); // explicit compose down before the next size
        eprintln!("  {size}: pipeline torn down");
    }

    // ── Write the artifact ────────────────────────────────────────────
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let artifact = CacheArtifact {
        schema_version: CACHE_USE_SCHEMA_VERSION,
        mode: mode.to_string(),
        recorded_at: format!("{now}"),
        git_commit: git_head(),
        hostname: hostname(),
        uname: uname(),
        label: "resolver cache use, local Docker bridge".to_string(),
        class: CACHE_USE_CLASS.to_string(),
        method: CACHE_USE_METHOD.to_string(),
        resolver: "unbound".to_string(),
        r_values,
        configs: vec!["plain".to_string(), "mtl-condensed".to_string()],
        repeats,
        runs,
        warnings,
    };

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let json = serde_json::to_string_pretty(&artifact)
        .context("failed to serialize cache-use artifact")?;
    fs::write(&args.out, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    eprintln!(
        "wrote {} ({} runs, mode={}, resolver={})",
        args.out.display(),
        artifact.runs.len(),
        artifact.mode,
        artifact.resolver
    );
    Ok(())
}

/// Select dataset sizes: expand `All`, honor `--quick`, drop `Large` unless
/// explicitly requested (it is not measured by default because it would not
/// finish in minutes).
fn selected_sizes(sizes: &[SizeArg], quick: bool) -> Vec<SizeArg> {
    let mut expanded: Vec<SizeArg> = Vec::new();
    for size in sizes {
        match size {
            SizeArg::All => {
                for s in [SizeArg::Small, SizeArg::Medium, SizeArg::Large] {
                    if !expanded.contains(&s) {
                        expanded.push(s);
                    }
                }
            }
            other => {
                if !expanded.contains(other) {
                    expanded.push(*other);
                }
            }
        }
    }
    if quick {
        expanded.retain(|s| matches!(s, SizeArg::Small));
    }
    expanded
}

/// Sign a deterministic zone holding `R` distinct A records for each size,
/// staged under `target/dns-cache`.
fn sign_zones(
    root: &Path,
    sizes: &[SizeArg],
    reuse_zone: &Option<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<BTreeMap<SizeArg, PathBuf>> {
    let mut zones = BTreeMap::new();
    for &size in sizes {
        let Some(r) = size_to_r(size) else {
            continue;
        };
        if let Some(reuse) = reuse_zone {
            if !reuse.exists() {
                bail!("--reuse-zone path does not exist: {}", reuse.display());
            }
            warnings.push(format!(
                "reusing one signed zone for every size ({})",
                reuse.display()
            ));
            zones.insert(size, reuse.clone());
            continue;
        }
        let scratch = root.join("target").join("dns-cache");
        fs::create_dir_all(&scratch).context("failed to create scratch directory")?;
        let basename = format!("climb.{}.zone", size_label(size));
        let unsigned = scratch.join(&basename);
        write_unsigned_zone(&unsigned, size, r)?;
        let zsk = run_in_signer(&scratch, &["ldns-keygen", "-a", ALGORITHM, "climb.example"])?;
        eprintln!("  {size} ZSK basename: {zsk}");
        let ksk = run_in_signer(
            &scratch,
            &["ldns-keygen", "-k", "-a", ALGORITHM, "climb.example"],
        )?;
        eprintln!("  {size} KSK basename: {ksk}");
        // ldns-signzone runs inside the signer container with cwd = scratch, so
        // pass basenames. It writes `<basename>.signed`.
        run_in_signer(&scratch, &["ldns-signzone", &basename, &zsk, &ksk])?;
        let signed = scratch.join(format!("{basename}.signed"));
        if !signed.exists() {
            bail!("ldns-signzone did not produce {}", signed.display());
        }
        let signed_len = signed.metadata()?.len();
        zones.insert(size, signed);
        eprintln!("  {size} signed zone staged ({signed_len} B, {r} names)");
    }
    Ok(zones)
}

fn size_label(size: SizeArg) -> &'static str {
    match size {
        SizeArg::Small => "small",
        SizeArg::OneK => "onek",
        SizeArg::Medium => "medium",
        SizeArg::Large => "large",
        SizeArg::All => "all",
    }
}

/// Write an unsigned zone holding `r` distinct A records
/// (`r0.climb.example.` … `r{r-1}.climb.example.`).
fn write_unsigned_zone(path: &Path, size: SizeArg, r: usize) -> Result<()> {
    let mut content = String::new();
    content.push_str("$ORIGIN climb.example.\n");
    content.push_str("$TTL 3600\n");
    content.push_str("@ IN SOA ns1.climb.example. hostmaster.climb.example. (\n");
    content.push_str("                        2026111001 ; serial\n");
    content.push_str("                        3600       ; refresh\n");
    content.push_str("                        1800       ; retry\n");
    content.push_str("                        604800     ; expire\n");
    content.push_str("                        3600 )     ; minimum\n");
    content.push_str("@ IN NS ns1.climb.example.\n");
    content.push_str("ns1 IN A 127.0.0.1\n");
    for i in 0..r {
        content.push_str(&format!("r{i} IN A 10.0.0.{}\n", i % 254 + 1));
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    eprintln!("  {} unsigned zone: {} names", size_label(size), r);
    Ok(())
}

/// Compute the MTL SigTag (SHAKE-128 hash of the signed ladder) for the zone.
fn compute_sigtag(root: &Path, signed_zone: &Path) -> Result<Option<String>> {
    let mut cmd = Command::new("python3");
    cmd.arg(SIGTAG_SCRIPT).arg(signed_zone).current_dir(root);
    let output = cmd.output().context("failed to run SigTag script")?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        eprintln!("  warning: SigTag computation failed; condensed queries will use no tag");
        Ok(None)
    }
}

/// Load `r` distinct names into the resolver's cache through a single fast
/// probe process (one UDP socket, `r` sequential queries). Runs on the host
/// and addresses the resolver at its bridge address.
fn load_names(server: &str, r: usize, sigtag: Option<&str>) -> Result<serde_json::Value> {
    let count = r.to_string();
    let mut args: Vec<&str> = vec![
        CACHE_PROBE_SCRIPT,
        "--server",
        server,
        "--port",
        "53",
        "--count",
        &count,
        "--base-name",
        "r",
        "--timeout",
        "5",
    ];
    if let Some(tag) = sigtag {
        args.push("--sigtag");
        args.push(tag);
    }
    let output = Command::new("python3")
        .args(&args)
        .current_dir(std::env::current_dir()?)
        .output()
        .context("failed to run cache-loading probe")?;
    if !output.status.success() {
        bail!(
            "cache probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("failed to parse probe output")
}

/// Restart Unbound so its in-memory answer cache is empty before the next run.
///
/// A short settle after `compose restart` lets Unbound re-bind port 53 before
/// the readiness probe starts hammering it.
fn flush_unbound_cache(root: &Path) -> Result<()> {
    run_cmd_inherit(compose_cmd(root, &["restart", UNBOUND_SERVICE]))?;
    thread::sleep(Duration::from_secs(2));
    wait_for_unbound_ready()?;
    Ok(())
}

/// Wait until Unbound answers on port 53 after a restart.
fn wait_for_unbound_ready() -> Result<()> {
    let script = format!(
        r#"import socket, time
s = socket.socket()
s.settimeout(2.0)
for _ in range(50):
    if s.connect_ex(('{UNBOUND_IP}', 53)) == 0:
        raise SystemExit(0)
    time.sleep(0.2)
raise SystemExit(1)
"#,
    );
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--net",
            "climb-net",
            "--entrypoint",
            "python3",
            "climb-dig:phase14",
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

/// Read Unbound's cache accounting via `unbound-control stats_noreset`.
fn read_unbound_stats() -> Result<UnboundStats> {
    let output = Command::new("docker")
        .args([
            "exec",
            UNBOUND_CONTAINER,
            UNBOUND_CONTROL,
            "-c",
            UNBOUND_CONF,
            "stats_noreset",
        ])
        .output()
        .context("failed to run unbound-control stats_noreset")?;
    if !output.status.success() {
        bail!(
            "unbound-control stats_noreset failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let blob = String::from_utf8_lossy(&output.stdout);
    parse_unbound_stats(&blob).with_context(|| format!("failed to parse unbound stats: {blob}"))
}

/// Read the Unbound container's resident memory in bytes (`docker stats`).
fn container_memory_bytes(container: &str) -> Result<u64> {
    let output = Command::new("docker")
        .args([
            "stats",
            "--no-stream",
            "--format",
            "{{.MemUsage}}",
            container,
        ])
        .output()
        .context("failed to run docker stats")?;
    if !output.status.success() {
        bail!(
            "docker stats failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_mem_usage(&text).ok_or_else(|| anyhow::anyhow!("could not parse docker stats: {text}"))
}

/// Parse `docker stats` `{{.MemUsage}}` output like `3.664MiB / 7.497GiB`
/// into bytes (the number and unit are concatenated in the first token).
fn parse_mem_usage(text: &str) -> Option<u64> {
    let token = text.split_whitespace().next()?;
    // Split off the trailing alphabetic unit ("3.664MiB" -> "3.664" / "MiB").
    let mut unit_start = token.len();
    for (i, c) in token.char_indices().rev() {
        if c.is_ascii_alphabetic() {
            unit_start = i;
        } else {
            break;
        }
    }
    let (number, unit) = token.split_at(unit_start);
    if number.is_empty() || unit.is_empty() {
        return None;
    }
    let n: f64 = number.parse().ok()?;
    let multiplier = match unit {
        "KiB" => 1024u64,
        "MiB" => 1024u64 * 1024,
        "GiB" => 1024u64 * 1024 * 1024,
        "B" => 1,
        _ => return None,
    };
    Some((n * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mem_usage_handles_all_units() {
        assert_eq!(parse_mem_usage("12.3MiB / 2GiB"), Some(12_897_484));
        assert_eq!(parse_mem_usage("1GiB / 2GiB"), Some(1_073_741_824));
        assert_eq!(parse_mem_usage("512B / 2GiB"), Some(512));
        assert_eq!(parse_mem_usage("1024KiB / 2GiB"), Some(1_048_576));
        assert_eq!(parse_mem_usage("3.664MiB / 7.497GiB"), Some(3_841_982));
        assert_eq!(parse_mem_usage("garbage"), None);
        assert_eq!(parse_mem_usage(""), None);
    }

    #[test]
    fn size_to_r_maps_blueprint_sizes() {
        assert_eq!(size_to_r(SizeArg::Small), Some(100));
        assert_eq!(size_to_r(SizeArg::Medium), Some(10_000));
        assert_eq!(size_to_r(SizeArg::Large), None);
    }

    #[test]
    fn quick_mode_selects_only_small() {
        let sizes = selected_sizes(&[SizeArg::Small, SizeArg::Medium, SizeArg::Large], true);
        assert_eq!(sizes, vec![SizeArg::Small]);
    }

    #[test]
    fn default_sizes_are_small_and_medium() {
        let sizes = selected_sizes(&[SizeArg::Small, SizeArg::Medium], false);
        assert_eq!(sizes, vec![SizeArg::Small, SizeArg::Medium]);
    }
}
