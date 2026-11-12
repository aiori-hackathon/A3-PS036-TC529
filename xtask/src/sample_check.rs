//! `cargo xtask sample-check`: prove the committed sample pair is one run.
//!
//! The committed `docs/sample-output/` directory holds a results document and
//! the HTML report rendered from it. Those two files are evidence only when
//! they describe the same claims: a report left behind by an earlier render
//! can silently omit claims that the results document gained, and every
//! dashboard tab that reads both then reports missing claims.
//!
//! This check is native-free on purpose, so the viewer CI job can run it
//! before the tests without a libMTL install. It verifies:
//!
//! 1. the report exists and carries exactly the claims the results imply;
//! 2. `report --check` reports no drift, missing claim or stray registry entry;
//! 3. every digest in `SHA256SUMS` still describes the committed file;
//! 4. the provenance record names a commit that exists in this history.
//!
//! The README's claims badge reads the same pair: the badge generator refuses
//! to publish a count the report does not carry, so the badge, the report and
//! the results document cannot disagree.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use climb_report::ClimbResults;

/// The committed results document.
pub(crate) const DEFAULT_RESULTS: &str = "docs/sample-output/climb-results.json";
/// The committed report rendered from that document.
pub(crate) const DEFAULT_REPORT: &str = "docs/sample-output/report.html";
/// The checksum manifest covering the committed artifacts.
pub(crate) const DEFAULT_MANIFEST: &str = "docs/sample-output/SHA256SUMS";
/// The provenance record for the committed artifacts.
pub(crate) const DEFAULT_PROVENANCE: &str = "docs/sample-output/PROVENANCE.txt";

#[derive(Args)]
pub(crate) struct SampleCheckArgs {
    /// Workspace root holding the committed sample.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Results document to check.
    #[arg(long, default_value = DEFAULT_RESULTS)]
    pub(crate) results: PathBuf,

    /// Report rendered from the results document.
    #[arg(long, default_value = DEFAULT_REPORT)]
    pub(crate) report: PathBuf,

    /// Checksum manifest covering the committed artifacts.
    #[arg(long, default_value = DEFAULT_MANIFEST)]
    pub(crate) manifest: PathBuf,

    /// Provenance record naming the commit the sample was recorded at.
    #[arg(long, default_value = DEFAULT_PROVENANCE)]
    pub(crate) provenance: PathBuf,

    /// Check only the results/report pair (used by the sample generator
    /// before it writes the manifest and provenance record).
    #[arg(long)]
    pub(crate) pair_only: bool,
}

/// Claim counts read from the committed pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PairCounts {
    /// Claims implied by the results document.
    pub(crate) expected: usize,
    /// Registry entries embedded in the report.
    pub(crate) in_report: usize,
    /// Rows that agree.
    pub(crate) agree: usize,
    /// Rows that drift.
    pub(crate) drift: usize,
    /// Rows missing from the report.
    pub(crate) missing: usize,
}

impl PairCounts {
    /// The pair is consistent only when every count describes the same set.
    pub(crate) fn consistent(self) -> bool {
        self.expected == self.in_report
            && self.agree == self.expected
            && self.drift == 0
            && self.missing == 0
    }
}

/// Resolve a path against the workspace root unless it is already absolute.
fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Parse the results document.
fn load_results(path: &Path) -> Result<ClimbResults> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

/// Check that `results` and `report` carry exactly the same claim ids.
///
/// The error names the first missing and stray claim ids, so the message is
/// actionable without opening either file.
pub(crate) fn check_pair(results_path: &Path, report_path: &Path) -> Result<PairCounts> {
    let results = load_results(results_path)?;
    let expected = climb_report::claims(&results)
        .with_context(|| format!("failed to derive claims from {}", results_path.display()))?;
    let check = climb_report::check(&results, report_path)
        .with_context(|| format!("failed to check {}", report_path.display()))?;

    if !check.report_found {
        bail!(
            "{}: committed report is missing; regenerate the pair with scripts/make-sample-output.sh",
            report_path.display()
        );
    }

    let expected_ids: BTreeSet<&str> = expected.iter().map(|claim| claim.id.as_str()).collect();
    let reported_ids: BTreeSet<&str> = check
        .claims
        .iter()
        .filter(|row| row.value_in_report.is_some())
        .map(|row| row.id.as_str())
        .collect();

    let missing: Vec<&str> = expected_ids.difference(&reported_ids).copied().collect();
    let extra: Vec<&str> = reported_ids.difference(&expected_ids).copied().collect();
    let (agree, drift, missing_count) = check.counts();

    if expected.len() != reported_ids.len() {
        bail!(
            "{}: claim count mismatch — {} claims in the results document, {} registry entries in the report{}{}",
            report_path.display(),
            expected.len(),
            reported_ids.len(),
            if missing.is_empty() {
                String::new()
            } else {
                format!("; missing: {}", missing.join(", "))
            },
            if extra.is_empty() {
                String::new()
            } else {
                format!("; not backed by results: {}", extra.join(", "))
            }
        );
    }

    if !check.problems.is_empty() {
        bail!(
            "{}: {} problem(s) against {}:\n{}",
            report_path.display(),
            check.problems.len(),
            results_path.display(),
            check.problems.join("\n")
        );
    }

    let counts = PairCounts {
        expected: expected.len(),
        in_report: reported_ids.len(),
        agree,
        drift,
        missing: missing_count,
    };
    if !counts.consistent() {
        bail!(
            "{}: claim counts do not reconcile — expected {}, in report {}, agree {}, drift {}, missing {}",
            report_path.display(),
            counts.expected,
            counts.in_report,
            counts.agree,
            counts.drift,
            counts.missing
        );
    }
    Ok(counts)
}

/// Verify every digest in the manifest against the file beside it.
pub(crate) fn check_manifest(manifest_path: &Path) -> Result<usize> {
    let text = std::fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut checked = 0usize;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some((digest, name)) = line.split_once("  ") else {
            bail!(
                "{}:{}: malformed line `{line}` (expected `<sha256>  <file>`)",
                manifest_path.display(),
                index + 1
            );
        };
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!(
                "{}:{}: `{digest}` is not a 64-character hex digest",
                manifest_path.display(),
                index + 1
            );
        }
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            bail!(
                "{}:{}: `{name}` is not a plain file name in this directory",
                manifest_path.display(),
                index + 1
            );
        }
        let bytes = std::fs::read(dir.join(name))
            .with_context(|| format!("cannot read {}", dir.join(name).display()))?;
        let actual = crate::package::sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(digest) {
            bail!(
                "{}:{}: `{name}` hashes to {actual}, not the recorded {digest}; regenerate the manifest with scripts/make-sample-output.sh",
                manifest_path.display(),
                index + 1
            );
        }
        checked += 1;
    }
    if checked == 0 {
        bail!("{}: manifest is empty", manifest_path.display());
    }
    Ok(checked)
}

/// Read the recorded commit SHA out of a provenance record.
pub(crate) fn provenance_commit(provenance_path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(provenance_path)
        .with_context(|| format!("failed to read {}", provenance_path.display()))?;
    let sha = text
        .lines()
        .find_map(|line| line.strip_prefix("commit SHA:"))
        .map(str::trim)
        .filter(|sha| !sha.is_empty())
        .with_context(|| {
            format!(
                "{}: no `commit SHA:` line; regenerate it with scripts/make-sample-output.sh",
                provenance_path.display()
            )
        })?;
    if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!(
            "{}: `{sha}` is not a full 40-character commit SHA",
            provenance_path.display()
        );
    }
    Ok(sha.to_string())
}

/// Prove the provenance commit exists in the repository at `root`.
pub(crate) fn check_provenance(root: &Path, provenance_path: &Path) -> Result<String> {
    let sha = provenance_commit(provenance_path)?;
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("cat-file")
        .arg("-e")
        .arg(format!("{sha}^{{commit}}"))
        .status()
        .with_context(|| "failed to run git cat-file")?;
    if !status.success() {
        bail!(
            "{}: recorded commit {sha} does not exist in this repository; the sample's provenance cannot be verified",
            provenance_path.display()
        );
    }
    Ok(sha)
}

/// The `sample-check` subcommand entry point.
pub(crate) fn cmd_sample_check(args: SampleCheckArgs) -> Result<()> {
    run(&args).map(|_| ())
}

/// Run every check over the committed sample and return the claim counts.
pub(crate) fn run(args: &SampleCheckArgs) -> Result<PairCounts> {
    let results = resolve(&args.root, &args.results);
    let report = resolve(&args.root, &args.report);
    let counts = check_pair(&results, &report)?;
    println!(
        "sample-check: {} claims, {} in report, {} agree, {} drift, {} missing",
        counts.expected, counts.in_report, counts.agree, counts.drift, counts.missing
    );

    if !args.pair_only {
        let manifest = resolve(&args.root, &args.manifest);
        let provenance = resolve(&args.root, &args.provenance);
        let files = check_manifest(&manifest)?;
        let sha = check_provenance(&args.root, &provenance)?;
        println!("sample-check: {files} manifest entries match; provenance commit {sha} exists");
    }
    println!("sample-check: committed sample pair is consistent");
    Ok(counts)
}

/// Collect the same checks as violation strings for `docs-check`.
pub(crate) fn violations(root: &Path) -> Vec<String> {
    let args = SampleCheckArgs {
        root: root.to_path_buf(),
        results: PathBuf::from(DEFAULT_RESULTS),
        report: PathBuf::from(DEFAULT_REPORT),
        manifest: PathBuf::from(DEFAULT_MANIFEST),
        provenance: PathBuf::from(DEFAULT_PROVENANCE),
        pair_only: false,
    };
    match run(&args) {
        Ok(_) => Vec::new(),
        Err(error) => vec![format!("sample-check failed: {error:#}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask sits under the workspace root")
            .to_path_buf()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-sample-check-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn committed_report_text() -> String {
        std::fs::read_to_string(workspace_root().join(DEFAULT_REPORT)).expect("read report")
    }

    /// Replace the embedded registry payload, leaving the rest of the page.
    fn with_registry(html: &str, claims: &[climb_report::html::Claim]) -> String {
        const MARKER: &str = "<script type=\"application/json\" id=\"climb-claims\">";
        let start = html.find(MARKER).expect("marker") + MARKER.len();
        let end = start + html[start..].find("</script>").expect("payload end");
        let payload = serde_json::to_string(claims).expect("serialize claims");
        format!("{}{}{}", &html[..start], payload, &html[end..])
    }

    fn registry(html: &str) -> Vec<climb_report::html::Claim> {
        const MARKER: &str = "<script type=\"application/json\" id=\"climb-claims\">";
        let start = html.find(MARKER).expect("marker") + MARKER.len();
        let end = start + html[start..].find("</script>").expect("payload end");
        serde_json::from_str(&html[start..end]).expect("valid registry")
    }

    #[test]
    fn the_committed_pair_has_equal_claim_counts() {
        let root = workspace_root();
        let counts = check_pair(&root.join(DEFAULT_RESULTS), &root.join(DEFAULT_REPORT))
            .expect("the committed pair is consistent");
        assert!(counts.expected > 0, "the sample carries no claims");
        assert_eq!(counts.expected, counts.in_report);
        assert_eq!(counts.agree, counts.expected);
        assert_eq!(counts.drift, 0);
        assert_eq!(counts.missing, 0);
    }

    #[test]
    fn a_report_missing_a_claim_names_it() {
        let dir = temp_dir("missing-claim");
        let root = workspace_root();
        let html = committed_report_text();
        let mut claims = registry(&html);
        let removed = claims.remove(0);
        std::fs::write(dir.join("report.html"), with_registry(&html, &claims)).expect("write");
        let error = check_pair(&root.join(DEFAULT_RESULTS), &dir.join("report.html"))
            .expect_err("a missing claim must fail");
        assert!(
            error.to_string().contains(&removed.id),
            "the error must name the missing claim {}: {error}",
            removed.id
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_report_with_a_stray_claim_is_rejected() {
        let dir = temp_dir("stray-claim");
        let root = workspace_root();
        let html = committed_report_text();
        let mut claims = registry(&html);
        claims.push(climb_report::html::Claim {
            id: "not-backed-by-results".to_string(),
            pointer: "/not/a/pointer".to_string(),
            value: serde_json::json!("invented"),
            formatted: "invented".to_string(),
        });
        std::fs::write(dir.join("report.html"), with_registry(&html, &claims)).expect("write");
        let error = check_pair(&root.join(DEFAULT_RESULTS), &dir.join("report.html"))
            .expect_err("a stray claim must fail");
        assert!(
            error.to_string().contains("not-backed-by-results"),
            "the error must name the stray claim: {error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_present_but_empty_report_is_not_a_pass() {
        let dir = temp_dir("empty-report");
        let root = workspace_root();
        std::fs::write(dir.join("report.html"), "<html></html>").expect("write");
        let error = check_pair(&root.join(DEFAULT_RESULTS), &dir.join("report.html"))
            .expect_err("an empty report must fail");
        assert!(error.to_string().contains("registry"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_manifest_digest_mismatch_is_reported() {
        let dir = temp_dir("manifest");
        std::fs::write(dir.join("artifact.json"), b"content").expect("write artifact");
        std::fs::write(
            dir.join("SHA256SUMS"),
            format!("{}  artifact.json\n", "0".repeat(64)),
        )
        .expect("write manifest");
        let error = check_manifest(&dir.join("SHA256SUMS")).expect_err("a bad digest must fail");
        assert!(error.to_string().contains("artifact.json"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_manifest_that_names_a_path_outside_the_directory_is_rejected() {
        let dir = temp_dir("manifest-path");
        std::fs::write(
            dir.join("SHA256SUMS"),
            format!("{}  ../artifact.json\n", "a".repeat(64)),
        )
        .expect("write manifest");
        let error = check_manifest(&dir.join("SHA256SUMS")).expect_err("escape must fail");
        assert!(error.to_string().contains("plain file name"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_provenance_commit_that_does_not_exist_is_reported() {
        let dir = temp_dir("provenance");
        std::fs::write(
            dir.join("PROVENANCE.txt"),
            format!("commit SHA: {}\n", "0".repeat(40)),
        )
        .expect("write provenance");
        let error = check_provenance(&workspace_root(), &dir.join("PROVENANCE.txt"))
            .expect_err("an unknown commit must fail");
        assert!(error.to_string().contains("does not exist"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_provenance_record_without_a_commit_line_is_reported() {
        let dir = temp_dir("provenance-missing");
        std::fs::write(dir.join("PROVENANCE.txt"), "rustc: anything\n").expect("write");
        let error = provenance_commit(&dir.join("PROVENANCE.txt"))
            .expect_err("a missing commit line must fail");
        assert!(error.to_string().contains("commit SHA"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_committed_manifest_and_provenance_are_consistent() {
        let root = workspace_root();
        let files = check_manifest(&root.join(DEFAULT_MANIFEST)).expect("manifest matches");
        assert!(files >= 10, "the manifest covers the committed sample");
        check_provenance(&root, &root.join(DEFAULT_PROVENANCE))
            .expect("the provenance commit exists");
    }

    #[test]
    fn run_checks_the_committed_sample_end_to_end() {
        let root = workspace_root();
        let counts = run(&SampleCheckArgs {
            root: root.clone(),
            results: PathBuf::from(DEFAULT_RESULTS),
            report: PathBuf::from(DEFAULT_REPORT),
            manifest: PathBuf::from(DEFAULT_MANIFEST),
            provenance: PathBuf::from(DEFAULT_PROVENANCE),
            pair_only: false,
        })
        .expect("the committed sample passes every check");
        assert_eq!(counts.expected, counts.in_report);
    }
}
