//! The `package` subcommand: bundle the recorded artifacts into one release
//! tarball under `dist/`.
//!
//! ```text
//! cargo xtask package
//! cargo xtask package --out dist --skip-checks
//! ```
//!
//! The bundle is what a judge or mentor unpacks to inspect the numbers without
//! building anything: the rendered HTML report, every report chart, the
//! consolidated results document, the quote sheet, the recorded fixtures, a
//! `SHA256SUMS` manifest over the payload, and a `PROVENANCE.txt` naming the
//! commit, the submodule SHAs, the toolchain, a host-fingerprint hash and the
//! time of packing.
//!
//! Packing is deliberately strict. Before writing anything it verifies that
//! every required input exists, re-runs the renderer's own self-check
//! (`climb_report::check`) against the report it is about to ship, and hashes
//! each payload file the moment it copies it — so the manifest cannot describe
//! bytes the tarball does not contain. A missing input is a hard error naming
//! the path, never a bundle that quietly omits a section.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use climb_report::charts::CHART_FILES;
use climb_report::{check, ClimbResults};

/// Consolidated results: the source of every quoted number.
const RESULTS: &str = "target/climb-results.json";
/// Rendered self-contained HTML report.
const REPORT_HTML: &str = "target/climb-report/report.html";
/// Directory holding the rendered SVG charts.
const CHARTS_DIR: &str = "target/climb-report/charts";
/// The quote sheet: every number the README/report/deck may quote.
const DECK_FACTS: &str = "docs/deck-facts.md";
/// Environment fingerprint captured by `pin-env`.
const ENVIRONMENT: &str = "environment.json";
/// Recorded fixtures, with the filenames they keep inside the bundle.
const FIXTURES: [&str; 2] = [
    "data/dns-capture.recorded.json",
    "data/ladder-verification.recorded.json",
];

/// Arguments for `cargo xtask package`.
#[derive(Args)]
pub(crate) struct PackageArgs {
    /// Directory the tarball is written into (created when absent).
    #[arg(long, default_value = "dist")]
    pub(crate) out: PathBuf,

    /// Path to the consolidated results document to bundle and validate.
    #[arg(long, default_value = RESULTS)]
    pub(crate) results: PathBuf,

    /// Path to the rendered HTML report to bundle and self-check.
    #[arg(long, default_value = REPORT_HTML)]
    pub(crate) report: PathBuf,

    /// Directory holding the SVG charts to bundle.
    #[arg(long, default_value = CHARTS_DIR)]
    pub(crate) charts: PathBuf,

    /// Skip the report self-check (only for debugging a bundle whose report is
    /// already known to be stale).
    #[arg(long)]
    pub(crate) skip_checks: bool,
}

/// One file that goes into the bundle: where it is read from, and the name it
/// carries inside the tarball.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) source: PathBuf,
    pub(crate) archive_name: String,
}

/// Build the ordered member list for a bundle. Pure: path arithmetic only, so
/// the layout is unit-testable without touching the disk.
///
/// `charts` is the directory holding the SVGs; the member names are flattened
/// to the bundle root so the tarball has a single flat level beside the
/// directory entry.
pub(crate) fn plan(results: &Path, report: &Path, charts: &Path) -> Vec<Member> {
    let mut members = Vec::with_capacity(4 + CHART_FILES.len() + FIXTURES.len());
    members.push(Member {
        source: report.to_path_buf(),
        archive_name: "report.html".to_string(),
    });
    for chart in CHART_FILES {
        members.push(Member {
            source: charts.join(chart),
            archive_name: chart.to_string(),
        });
    }
    members.push(Member {
        source: results.to_path_buf(),
        archive_name: "climb-results.json".to_string(),
    });
    members.push(Member {
        source: PathBuf::from(DECK_FACTS),
        archive_name: "deck-facts.md".to_string(),
    });
    members.push(Member {
        source: PathBuf::from(ENVIRONMENT),
        archive_name: "environment.json".to_string(),
    });
    for fixture in FIXTURES {
        members.push(Member {
            source: PathBuf::from(fixture),
            archive_name: fixture.rsplit('/').next().unwrap_or(fixture).to_string(),
        });
    }
    members
}

/// The bundle's top-level directory name, derived from the short commit SHA.
pub(crate) fn bundle_name(short_sha: &str) -> String {
    format!("climb-{short_sha}")
}

/// The tarball filename for a given short SHA.
pub(crate) fn tarball_name(short_sha: &str) -> String {
    format!("{}.tar.gz", bundle_name(short_sha))
}

/// SHA-256 hex digest of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Render the `SHA256SUMS` manifest: one `<digest>  <name>` line per file,
/// sorted by name so the manifest is byte-stable across runs.
pub(crate) fn render_sums(entries: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (name, digest) in entries {
        out.push_str(digest);
        out.push_str("  ");
        out.push_str(name);
        out.push('\n');
    }
    out
}

/// The provenance fields recorded beside the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Provenance {
    pub(crate) commit: String,
    pub(crate) submodules: Vec<(String, String)>,
    pub(crate) rustc: String,
    pub(crate) host_fingerprint: String,
    pub(crate) date: String,
}

/// Render `PROVENANCE.txt`. Deterministic given its inputs.
pub(crate) fn render_provenance(prov: &Provenance) -> String {
    let submodules = prov
        .submodules
        .iter()
        .map(|(path, sha)| format!("{path}={sha}"))
        .collect::<Vec<_>>()
        .join(", ");
    let submodules = if submodules.is_empty() {
        "none".to_string()
    } else {
        submodules
    };
    format!(
        "commit SHA: {commit}\n\
         submodule SHAs: {submodules}\n\
         rustc: {rustc}\n\
         host fingerprint: {host}\n\
         date: {date}\n",
        commit = prov.commit,
        submodules = submodules,
        rustc = prov.rustc,
        host = prov.host_fingerprint,
        date = prov.date,
    )
}

/// Run a command and return its trimmed stdout, or `None` when it is missing
/// or exits non-zero.
pub(crate) fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The short commit SHA, or `unknown` when git is unavailable.
fn short_sha() -> String {
    capture("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

/// The full commit SHA, or `unknown`.
pub(crate) fn full_sha() -> String {
    capture("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

/// `(path, sha)` for every submodule, from `git submodule status`.
pub(crate) fn submodule_shas() -> Vec<(String, String)> {
    let Some(out) = capture("git", &["submodule", "status"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| {
            let rest = line.trim_start_matches([' ', '-', '+', 'U']);
            let mut parts = rest.split_whitespace();
            let sha = parts.next()?;
            let path = parts.next()?;
            Some((path.to_string(), sha.to_string()))
        })
        .collect()
}

/// Read an environment fingerprint from disk, if present, and return the hash
/// of its canonical serialization. A missing or unparseable document yields
/// `None`.
fn environment_hash(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: climb_report::results::EnvironmentFingerprint = serde_json::from_str(&text).ok()?;
    let bytes = serde_json::to_vec(&value).ok()?;
    Some(sha256_hex(&bytes).chars().take(16).collect())
}

pub(crate) fn cmd_package(args: PackageArgs) -> Result<()> {
    let members = plan(&args.results, &args.report, &args.charts);

    // Every input must exist: a bundle that silently omits a section is worse
    // than no bundle, so a missing member is a hard error naming the path.
    for member in &members {
        if !member.source.is_file() {
            bail!(
                "{} is not a file — run `cargo xtask reproduce` before packaging",
                member.source.display()
            );
        }
    }

    // Re-run the renderer's own drift check against the report being shipped,
    // so the bundle never carries a report that disagrees with its results.
    if !args.skip_checks {
        let text = std::fs::read_to_string(&args.results)
            .with_context(|| format!("failed to read {}", args.results.display()))?;
        let results: ClimbResults = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", args.results.display()))?;
        let report = check(&results, &args.report).with_context(|| {
            format!(
                "failed to check {} against {}",
                args.report.display(),
                args.results.display()
            )
        })?;
        let failures = report.messages();
        if !failures.is_empty() {
            for failure in &failures {
                eprintln!("package: {failure}");
            }
            bail!(
                "{} disagrees with the results document ({} problem(s)) — re-run \
                 `cargo xtask reproduce`",
                args.report.display(),
                failures.len()
            );
        }
    }

    let short = short_sha();
    let bundle = bundle_name(&short);
    let tarball = args.out.join(tarball_name(&short));

    // Stage the payload in a scratch directory next to the tarball, hashing
    // each file as it is copied so the manifest always describes the bytes
    // actually archived.
    let staging = args.out.join(format!(".{bundle}.staging"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("failed to clear {}", staging.display()))?;
    }
    let root = staging.join(&bundle);
    std::fs::create_dir_all(&root)
        .with_context(|| format!("failed to create {}", root.display()))?;

    let mut sums: BTreeMap<String, String> = BTreeMap::new();
    for member in &members {
        let bytes = std::fs::read(&member.source)
            .with_context(|| format!("failed to read {}", member.source.display()))?;
        let dest = root.join(&member.archive_name);
        std::fs::write(&dest, &bytes)
            .with_context(|| format!("failed to write {}", dest.display()))?;
        sums.insert(member.archive_name.clone(), sha256_hex(&bytes));
    }

    // Provenance is derived from the checkout and the recorded fingerprint,
    // never hand-typed.
    let provenance = Provenance {
        commit: full_sha(),
        submodules: submodule_shas(),
        rustc: capture("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string()),
        host_fingerprint: environment_hash(Path::new(ENVIRONMENT))
            .unwrap_or_else(|| "unknown".to_string()),
        date: capture("date", &["--iso-8601=seconds"]).unwrap_or_else(|| "unknown".to_string()),
    };
    let provenance_text = render_provenance(&provenance);

    // The manifest and the provenance file describe the payload, so they are
    // written after it and then archived alongside it.
    std::fs::write(root.join("SHA256SUMS"), render_sums(&sums))
        .with_context(|| format!("failed to write {}/SHA256SUMS", root.display()))?;
    std::fs::write(root.join("PROVENANCE.txt"), &provenance_text)
        .with_context(|| format!("failed to write {}/PROVENANCE.txt", root.display()))?;

    if tarball.exists() {
        std::fs::remove_file(&tarball)
            .with_context(|| format!("failed to replace {}", tarball.display()))?;
    }
    let status = Command::new("tar")
        .args(["-czf"])
        .arg(&tarball)
        .arg("-C")
        .arg(&staging)
        .arg(&bundle)
        .status()
        .context("failed to run tar")?;
    if !status.success() {
        bail!("tar exited with {status}");
    }

    std::fs::remove_dir_all(&staging)
        .with_context(|| format!("failed to clear {}", staging.display()))?;

    let size = std::fs::metadata(&tarball)
        .map(|m| m.len())
        .unwrap_or_default();
    println!(
        "wrote {} ({} file(s) + SHA256SUMS + PROVENANCE.txt, {} B)",
        tarball.display(),
        members.len(),
        size
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member_names(out: &Path) -> Vec<String> {
        plan(
            Path::new("target/climb-results.json"),
            Path::new("target/climb-report/report.html"),
            out,
        )
        .into_iter()
        .map(|m| m.archive_name)
        .collect()
    }

    #[test]
    fn plan_bundles_report_charts_results_sheet_environment_and_fixtures() {
        let names = member_names(Path::new("target/climb-report/charts"));
        assert_eq!(names[0], "report.html");
        assert!(names.contains(&"climb-results.json".to_string()));
        assert!(names.contains(&"deck-facts.md".to_string()));
        assert!(names.contains(&"environment.json".to_string()));
        assert!(names.contains(&"dns-capture.recorded.json".to_string()));
        assert!(names.contains(&"ladder-verification.recorded.json".to_string()));
    }

    #[test]
    fn plan_bundles_every_chart_at_the_root() {
        let names = member_names(Path::new("target/climb-report/charts"));
        for chart in CHART_FILES {
            assert!(names.contains(&chart.to_string()), "missing {chart}");
        }
        // The charts are flattened, not nested under a charts/ prefix.
        let chart_count = names.iter().filter(|n| n.ends_with(".svg")).count();
        assert_eq!(chart_count, CHART_FILES.len());
    }

    #[test]
    fn plan_maps_each_source_to_its_archive_name() {
        let members = plan(
            Path::new("target/climb-results.json"),
            Path::new("target/climb-report/report.html"),
            Path::new("target/climb-report/charts"),
        );
        let signing = members
            .iter()
            .find(|m| m.archive_name == "signing_time.svg")
            .expect("signing chart present");
        assert_eq!(
            signing.source,
            PathBuf::from("target/climb-report/charts/signing_time.svg")
        );
        assert_eq!(members.len(), 4 + CHART_FILES.len() + FIXTURES.len());
    }

    #[test]
    fn bundle_and_tarball_names_carry_the_short_sha() {
        assert_eq!(bundle_name("47a4a14"), "climb-47a4a14");
        assert_eq!(tarball_name("47a4a14"), "climb-47a4a14.tar.gz");
    }

    #[test]
    fn sha256_hex_matches_a_known_vector() {
        // SHA-256 of the empty string.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sums_are_sorted_and_two_space_separated() {
        let mut entries = BTreeMap::new();
        entries.insert("b.svg".to_string(), "22".to_string());
        entries.insert("a.html".to_string(), "11".to_string());
        assert_eq!(render_sums(&entries), "11  a.html\n22  b.svg\n");
    }

    #[test]
    fn provenance_lists_every_field() {
        let prov = Provenance {
            commit: "deadbeef".to_string(),
            submodules: vec![
                ("third_party/MTL".to_string(), "aaa".to_string()),
                ("third_party/liboqs".to_string(), "bbb".to_string()),
            ],
            rustc: "rustc 1.98.1".to_string(),
            host_fingerprint: "0123456789abcdef".to_string(),
            date: "2026-10-20T00:00:00+05:30".to_string(),
        };
        let text = render_provenance(&prov);
        assert!(text.contains("commit SHA: deadbeef"));
        assert!(text.contains("third_party/MTL=aaa, third_party/liboqs=bbb"));
        assert!(text.contains("rustc: rustc 1.98.1"));
        assert!(text.contains("host fingerprint: 0123456789abcdef"));
        assert!(text.contains("date: 2026-10-20T00:00:00+05:30"));
    }

    #[test]
    fn provenance_without_submodules_says_none() {
        let prov = Provenance {
            commit: "deadbeef".to_string(),
            submodules: Vec::new(),
            rustc: "rustc 1.98.1".to_string(),
            host_fingerprint: "0123".to_string(),
            date: "2026-10-20T00:00:00+05:30".to_string(),
        };
        assert!(render_provenance(&prov).contains("submodule SHAs: none"));
    }

    #[test]
    fn fingerprint_hash_is_sixteen_hex_characters() {
        let hash: String = sha256_hex(b"{\"cpu\":\"x\"}").chars().take(16).collect();
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Deterministic for identical input.
        let again: String = sha256_hex(b"{\"cpu\":\"x\"}").chars().take(16).collect();
        assert_eq!(hash, again);
    }
}
