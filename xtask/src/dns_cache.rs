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
    check_docker, check_images, compose_cmd, compose_cmd_with_extra, git_head, hostname,
    run_cmd_inherit, run_in_signer, uname, wait_for_nsd_ready, workspace_root, ALGORITHM,
    SIGTAG_SCRIPT, UNBOUND_IP,
};
use crate::SizeArg;

use climb_report::cache::{parse_unbound_stats, CacheArtifact, CacheRun, LadderRun, UnboundStats};
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
/// Flag that makes the SigTag helper print the signed-ladder length instead of
/// the SigTag.
const LADDER_LENGTH_FLAG: &str = "--ladder-length";
/// Config path inside the Unbound container.
const UNBOUND_CONF_IN_CONTAINER: &str = "/etc/unbound/unbound.conf";

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
    let mut ladder_runs: Vec<LadderRun> = Vec::new();
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

        // ── Ladder cache, the MTL-specific structure ─────────────────
        // The answer-cache cells above use the resolver exactly as the demo
        // pipeline configures it (iterator only), so they cache the full
        // responses the signed zone stores. The ladder cache is populated by
        // the validator when it verifies an MTL signature, so this measurement
        // uses a measurement-only config that enables validation for the
        // zone's own trust anchor. It is generated under target/; the
        // committed pipeline config is untouched.
        let ladder = measure_ladder_cache(&root, size, r, repeats, &staged_zone, &mut warnings)?;
        ladder_runs.extend(ladder);
        eprintln!("  {size}: ladder-cache measurement complete");
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
        ladder_runs,
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

/// Measure the MTL ladder cache: flush the resolver, load `r` names from the
/// MTL-signed zone (pass 1), then load the same names again (pass 2).
///
/// The ladder cache is populated by Unbound's validator when it verifies the
/// full MTL signature carried by the first response; after that the resolver
/// advertises the cached ladder's SigTag, so later responses are condensed and
/// reuse the same cache entry. A second pass over the same zone must therefore
/// not grow the cache: growth on pass 2 would mean the "one ladder per batch"
/// assumption is wrong, and it is recorded rather than hidden.
fn measure_ladder_cache(
    root: &Path,
    size: SizeArg,
    r: usize,
    repeats: u64,
    staged_zone: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<LadderRun>> {
    let scratch = root.join("target").join("dns-cache");
    fs::create_dir_all(&scratch).context("failed to create scratch directory")?;
    let signed_ladder_bytes = signed_ladder_len(root, staged_zone)?;
    let compose_override = write_ladder_config(root, &scratch, staged_zone)?;

    let guard = PipelineGuard::new(root.to_path_buf());
    {
        let mut cmd = compose_cmd_with_extra(
            root,
            &[compose_override.as_path()],
            &["up", "-d", "nsd", "unbound"],
        );
        cmd.env("CLIMB_ZONE_DIR", &scratch);
        run_cmd_inherit(cmd)?;
    }
    wait_for_nsd_ready(root)?;
    eprintln!("  {size}: validation-enabled pipeline ready for the ladder cache");

    let mut runs = Vec::new();
    for repeat in 0..repeats {
        flush_unbound_cache(root)?;
        let before = read_unbound_stats()?;
        let first = load_names(UNBOUND_IP, r, None)?;
        let after_first = read_unbound_stats()?;
        let second = load_names(UNBOUND_IP, r, None)?;
        let after_second = read_unbound_stats()?;

        if after_first.ladder_cache_bytes < before.ladder_cache_bytes {
            warnings.push(format!(
                "{size} ladder repeat {repeat}: mem.cache.ladder shrank ({} -> {} B)",
                before.ladder_cache_bytes, after_first.ladder_cache_bytes
            ));
        }
        let second_pass_growth = after_second
            .ladder_cache_bytes
            .saturating_sub(after_first.ladder_cache_bytes);
        if second_pass_growth != 0 {
            warnings.push(format!(
                "{size} ladder repeat {repeat}: a second pass over the same zone grew \
                 mem.cache.ladder by {second_pass_growth} B instead of reusing the cached ladder"
            ));
        }

        eprintln!(
            "    ladder repeat {repeat}/{repeats}: mem.cache.ladder {} → {} → {} B \
             (pass1 {} queries, pass2 {} queries, signed ladder {} B)",
            before.ladder_cache_bytes,
            after_first.ladder_cache_bytes,
            after_second.ladder_cache_bytes,
            first
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            second
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            signed_ladder_bytes,
        );

        runs.push(LadderRun {
            size: size_label(size).to_string(),
            r,
            repeat,
            signed_ladder_bytes,
            distinct_ladders: 1,
            ladder_cache_bytes_before: before.ladder_cache_bytes,
            ladder_cache_bytes_after_first_pass: after_first.ladder_cache_bytes,
            ladder_cache_bytes_after_second_pass: after_second.ladder_cache_bytes,
            before,
            after_first_pass: after_first,
            after_second_pass: after_second,
            ladder_entry_count_exposed: false,
        });
    }
    drop(guard);
    Ok(runs)
}

/// Write a measurement-only Unbound config that validates the signed zone, and
/// the Compose overlay that mounts it.
///
/// It is the committed pipeline config with two deliberate changes: validation
/// is enabled (`validator iterator`) and the trust anchor is the zone's own KSK,
/// read from the signed zone. Nothing is hardcoded and the committed config is
/// not rewritten for a one-off measurement.
fn write_ladder_config(root: &Path, scratch: &Path, signed_zone: &Path) -> Result<PathBuf> {
    let base = fs::read_to_string(root.join("docker/configs/unbound.conf"))
        .context("failed to read the pipeline Unbound config")?;
    let zone = fs::read_to_string(signed_zone)
        .with_context(|| format!("failed to read {}", signed_zone.display()))?;
    let text = ladder_config_text(&base, &zone)?;

    let config_path = scratch.join("unbound-ladder.conf");
    fs::write(&config_path, text)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let overlay = scratch.join("compose-ladder.yml");
    let overlay_text = format!(
        "services:\n  unbound:\n    volumes:\n      - \"{}:{}:ro\"\n",
        config_path.display(),
        UNBOUND_CONF_IN_CONTAINER
    );
    fs::write(&overlay, overlay_text)
        .with_context(|| format!("failed to write {}", overlay.display()))?;
    Ok(overlay)
}

/// Rewrite the committed pipeline config into the measurement config.
///
/// Pure so a test can prove the two changes: the module list gains the
/// validator and the `domain-insecure` exclusion is removed.
fn ladder_config_text(base: &str, zone: &str) -> Result<String> {
    let anchor = trust_anchor_from_zone(zone)?;
    let mut text = String::new();
    for line in base.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("module-config:") {
            text.push_str("    module-config: \"validator iterator\"\n");
        } else if trimmed.starts_with("domain-insecure:") {
            // Validation is the point of this config, so the zone must not be
            // excluded from it.
            continue;
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    text.push_str(&format!("\nserver:\n    trust-anchor: \"{anchor}\"\n"));
    Ok(text)
}

/// Read the zone's KSK line and return it as an Unbound `trust-anchor` value.
fn trust_anchor_from_zone(zone: &str) -> Result<String> {
    for raw in zone.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(key_index) = fields.iter().position(|field| *field == "DNSKEY") else {
            continue;
        };
        if fields.get(key_index + 1) != Some(&"257") {
            continue;
        }
        let owner = fields.first().context("DNSKEY line has no owner")?;
        let ttl = fields.get(1).context("DNSKEY line has no TTL")?;
        let protocol = fields
            .get(key_index + 2)
            .context("DNSKEY line has no protocol field")?;
        let algorithm = fields
            .get(key_index + 3)
            .context("DNSKEY line has no algorithm field")?;
        let key = fields
            .get(key_index + 4)
            .context("DNSKEY line has no key material")?;
        return Ok(format!(
            "{owner} {ttl} IN DNSKEY 257 {protocol} {algorithm} {key}"
        ));
    }
    bail!("signed zone carries no DNSKEY 257 record to use as a trust anchor")
}

/// Read the signed-ladder length from the zone's own SOA RRSIG, in bytes.
fn signed_ladder_len(root: &Path, signed_zone: &Path) -> Result<u64> {
    let output = Command::new("python3")
        .arg(SIGTAG_SCRIPT)
        .arg(LADDER_LENGTH_FLAG)
        .arg(signed_zone)
        .current_dir(root)
        .output()
        .context("failed to run the ladder-length helper")?;
    if !output.status.success() {
        bail!(
            "ladder-length helper failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .context("ladder-length helper did not print a byte count")
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

    #[test]
    fn trust_anchor_is_read_from_the_zone_ksk() {
        let zone = "climb.example.\t3600\tIN\tDNSKEY\t256 3 130 AAAA ;{id = 1 (zsk)}\n\
                    climb.example.\t3600\tIN\tDNSKEY\t257 3 130 BBBB ;{id = 2 (ksk)}\n";
        assert_eq!(
            trust_anchor_from_zone(zone).expect("anchor"),
            "climb.example. 3600 IN DNSKEY 257 3 130 BBBB"
        );
    }

    #[test]
    fn a_zone_without_a_ksk_is_a_hard_error() {
        let zone = "climb.example.\t3600\tIN\tDNSKEY\t256 3 130 AAAA\n";
        assert!(trust_anchor_from_zone(zone).is_err());
    }

    #[test]
    fn the_measurement_config_enables_the_validator_and_drops_the_exclusion() {
        let base = "server:\n    module-config: \"iterator\"\n    domain-insecure: \"climb.example.\"\n    verbosity: 2\n";
        let zone = "climb.example.\t3600\tIN\tDNSKEY\t257 3 130 BBBB ;{id = 2 (ksk)}\n";
        let text = ladder_config_text(base, zone).expect("measurement config");
        assert!(text.contains("module-config: \"validator iterator\""));
        assert!(!text.contains("domain-insecure"));
        assert!(text.contains("verbosity: 2"));
        assert!(text.contains("trust-anchor: \"climb.example. 3600 IN DNSKEY 257 3 130 BBBB\""));
    }
}
