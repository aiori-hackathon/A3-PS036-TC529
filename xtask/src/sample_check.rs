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
//! 4. the recorded content identity matches the current manifest;
//! 5. the provenance commit exists in this history *when it is present*.
//!
//! The public repository is a rewritten copy of the development history: the
//! publishing sync removes private-only paths and redacts references from every
//! commit, so every commit hash changes. A commit recorded by the development
//! checkout therefore cannot exist in a public copy, and a shallow clone may
//! not contain it either. The check is content-based: it always verifies the
//! per-file digests and a content identity computed from the manifest, and it
//! treats a missing commit as an expected condition with a printed reason. A
//! strict mode exists for the development checkout's own tooling
//! (`--require-commit`, or `CLIMB_REPO_ROLE=private`); CI never enables it.
//!
//! The README's claims badge reads the same pair: the badge generator refuses
//! to publish a count the report does not carry, so the badge, the report and
//! the results document cannot disagree.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

    /// Require the recorded commit to exist in this repository. Only the
    /// development checkout's tooling sets this (or `CLIMB_REPO_ROLE=private`);
    /// CI runs without it, because a rewritten public copy cannot carry the
    /// development commit.
    #[arg(long)]
    pub(crate) require_commit: bool,
}

/// The provenance line naming the commit the sample was recorded at.
const COMMIT_PREFIX: &str = "commit SHA:";
/// The provenance line naming the history-independent content identity.
const CONTENT_IDENTITY_PREFIX: &str = "content identity:";

/// Whether the recorded commit can be resolved in this checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitState {
    /// `git cat-file -e <sha>^{commit}` succeeded.
    Present,
    /// The checkout is a git repository but does not contain the commit.
    Absent,
    /// There is no git repository to ask (a source tarball).
    Unavailable,
}

/// One `<sha256>  <file>` row of the checksum manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestEntry {
    pub(crate) digest: String,
    pub(crate) name: String,
}

/// The outcome of verifying the recorded provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvenanceOutcome {
    pub(crate) commit: String,
    pub(crate) content_identity: String,
    pub(crate) commit_state: CommitState,
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

/// Parse and validate the manifest without touching the payload files.
pub(crate) fn parse_manifest(manifest_path: &Path) -> Result<Vec<ManifestEntry>> {
    let text = std::fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut entries = Vec::new();
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
        entries.push(ManifestEntry {
            digest: digest.to_ascii_lowercase(),
            name: name.to_string(),
        });
    }
    if entries.is_empty() {
        bail!("{}: manifest is empty", manifest_path.display());
    }
    Ok(entries)
}

/// Canonical manifest text: one row per file, sorted by file name, two spaces
/// between digest and name, one trailing newline. This is the byte string the
/// content identity hashes, so the identity does not depend on write order.
pub(crate) fn canonical_manifest(entries: &mut [ManifestEntry]) -> String {
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    let mut text = String::new();
    for entry in entries.iter() {
        text.push_str(&entry.digest);
        text.push_str("  ");
        text.push_str(&entry.name);
        text.push('\n');
    }
    text
}

/// The history-independent identity of a sample set: the SHA-256 of the
/// canonical manifest. Any changed file, added file, removed file or stale
/// digest changes it.
pub(crate) fn content_identity_from_manifest(manifest_path: &Path) -> Result<String> {
    let mut entries = parse_manifest(manifest_path)?;
    let canonical = canonical_manifest(&mut entries);
    Ok(format!(
        "sha256:{}",
        crate::package::sha256_hex(canonical.as_bytes())
    ))
}

/// Verify every digest in the manifest against the file beside it.
pub(crate) fn check_manifest(manifest_path: &Path) -> Result<usize> {
    let entries = parse_manifest(manifest_path)?;
    let dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    for (index, entry) in entries.iter().enumerate() {
        let bytes = std::fs::read(dir.join(&entry.name))
            .with_context(|| format!("cannot read {}", dir.join(&entry.name).display()))?;
        let actual = crate::package::sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(&entry.digest) {
            bail!(
                "{}:{}: `{}` hashes to {actual}, not the recorded {}; regenerate the manifest with scripts/make-sample-output.sh",
                manifest_path.display(),
                index + 1,
                entry.name,
                entry.digest
            );
        }
    }
    Ok(entries.len())
}

/// Read the recorded commit SHA out of a provenance record.
pub(crate) fn provenance_commit(provenance_path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(provenance_path)
        .with_context(|| format!("failed to read {}", provenance_path.display()))?;
    let sha = text
        .lines()
        .find_map(|line| line.strip_prefix(COMMIT_PREFIX))
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

/// Read the recorded content identity out of a provenance record.
pub(crate) fn provenance_content_identity(provenance_path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(provenance_path)
        .with_context(|| format!("failed to read {}", provenance_path.display()))?;
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix(CONTENT_IDENTITY_PREFIX))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| {
            format!(
                "{}: no `content identity:` line; regenerate it with scripts/make-sample-output.sh",
                provenance_path.display()
            )
        })?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!(
            "{}: `{value}` is not a `sha256:<64 hex>` content identity",
            provenance_path.display()
        );
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!(
            "{}: `{value}` is not a `sha256:<64 hex>` content identity",
            provenance_path.display()
        );
    }
    Ok(format!("sha256:{}", hex.to_ascii_lowercase()))
}

/// Run one git command in `root`; `None` when git is missing or cannot start.
fn git_status(root: &Path, args: &[&str]) -> Option<bool> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.status().ok().map(|status| status.success())
}

/// True when `root` is inside a git repository (a worktree or a bare
/// repository). A source tarball without `.git` is not.
pub(crate) fn git_repository_present(root: &Path) -> bool {
    git_status(root, &["rev-parse", "--git-dir"]).unwrap_or(false)
}

/// True when the checkout at `root` is a shallow clone. A directory without a
/// git repository is not shallow: it simply has no history to read.
pub(crate) fn is_shallow(root: &Path) -> bool {
    if !git_repository_present(root) {
        return false;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-shallow-repository"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output();
    output
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Resolve the provenance commit against the checkout. This never fails: an
/// absent commit is an expected condition in a rewritten or shallow copy.
pub(crate) fn commit_state(root: &Path, sha: &str) -> CommitState {
    if !git_repository_present(root) {
        return CommitState::Unavailable;
    }
    let revision = format!("{sha}^{{commit}}");
    if git_status(root, &["cat-file", "-e", &revision]) == Some(true) {
        CommitState::Present
    } else {
        CommitState::Absent
    }
}

/// The line `sample-check` prints for a resolved commit state. The absent
/// wording is part of the contract: a rewritten public copy is expected to
/// carry different hashes, so it is a skip with a reason, not a failure.
pub(crate) fn commit_message(state: CommitState, sha: &str) -> String {
    let short = &sha[..7.min(sha.len())];
    match state {
        CommitState::Present => format!(
            "provenance commit {short} present in this repository; content identity verified"
        ),
        CommitState::Absent => format!(
            "provenance commit {short} not present in this repository (expected in a rewritten public copy); content identity verified"
        ),
        CommitState::Unavailable => format!(
            "provenance commit {short} not checkable (no git repository); content identity verified"
        ),
    }
}

/// Verify the content identity and the manifest, then resolve the recorded
/// commit. The commit is required only when `require_commit` is set.
pub(crate) fn verify_provenance(
    root: &Path,
    provenance_path: &Path,
    manifest_path: &Path,
    require_commit: bool,
) -> Result<ProvenanceOutcome> {
    let commit = provenance_commit(provenance_path)?;
    let recorded = provenance_content_identity(provenance_path)?;
    let actual = content_identity_from_manifest(manifest_path)?;
    if recorded != actual {
        bail!(
            "{}: recorded content identity {recorded} does not match the current SHA256SUMS ({actual}); \
             regenerate the sample with scripts/make-sample-output.sh",
            provenance_path.display()
        );
    }
    let state = commit_state(root, &commit);
    if require_commit && state != CommitState::Present {
        let reason = match state {
            CommitState::Absent => "does not exist in this repository",
            CommitState::Unavailable => {
                "cannot be checked because this checkout has no git repository"
            }
            CommitState::Present => unreachable!("the present state cannot fail the requirement"),
        };
        bail!(
            "{}: recorded commit {commit} {reason}; --require-commit was requested by the private checkout",
            provenance_path.display()
        );
    }
    Ok(ProvenanceOutcome {
        commit,
        content_identity: actual,
        commit_state: state,
    })
}

/// The `sample-check` subcommand entry point.
pub(crate) fn cmd_sample_check(args: SampleCheckArgs) -> Result<()> {
    run(&args).map(|_| ())
}

/// True when the caller asked for strict commit verification: the explicit flag
/// or the development checkout's role marker. CI never sets either.
fn strict_requested(args: &SampleCheckArgs) -> bool {
    args.require_commit
        || std::env::var("CLIMB_REPO_ROLE")
            .map(|role| role.eq_ignore_ascii_case("private"))
            .unwrap_or(false)
}

/// Run every check over the committed sample and return the claim counts.
pub(crate) fn run(args: &SampleCheckArgs) -> Result<PairCounts> {
    run_inner(args, strict_requested(args))
}

/// Run every check without strict commit verification. `docs-check` uses this
/// path so the documentation gate works in a public copy and in CI.
fn run_lenient(args: &SampleCheckArgs) -> Result<PairCounts> {
    run_inner(args, false)
}

fn run_inner(args: &SampleCheckArgs, require_commit: bool) -> Result<PairCounts> {
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
        println!("sample-check: {files} manifest entries match");
        if is_shallow(&args.root) {
            println!(
                "sample-check: this checkout is a shallow clone; the recorded commit is verified only if it is present"
            );
        }
        let outcome = verify_provenance(&args.root, &provenance, &manifest, require_commit)?;
        println!(
            "sample-check: content identity {} verified",
            outcome.content_identity
        );
        println!(
            "sample-check: {}",
            commit_message(outcome.commit_state, &outcome.commit)
        );
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
        require_commit: false,
    };
    match run_lenient(&args) {
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
    fn a_provenance_record_without_a_commit_line_is_reported() {
        let dir = temp_dir("provenance-missing");
        std::fs::write(dir.join("PROVENANCE.txt"), "rustc: anything\n").expect("write");
        let error = provenance_commit(&dir.join("PROVENANCE.txt"))
            .expect_err("a missing commit line must fail");
        assert!(error.to_string().contains("commit SHA"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_provenance_record_without_a_content_identity_is_reported() {
        let dir = temp_dir("identity-missing");
        std::fs::write(
            dir.join("PROVENANCE.txt"),
            format!("commit SHA: {}\n", "0".repeat(40)),
        )
        .expect("write");
        let error = provenance_content_identity(&dir.join("PROVENANCE.txt"))
            .expect_err("a missing content identity must fail");
        assert!(error.to_string().contains("content identity"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_committed_manifest_and_provenance_are_consistent() {
        let root = workspace_root();
        let files = check_manifest(&root.join(DEFAULT_MANIFEST)).expect("manifest matches");
        assert!(files >= 10, "the manifest covers the committed sample");
        let outcome = verify_provenance(
            &root,
            &root.join(DEFAULT_PROVENANCE),
            &root.join(DEFAULT_MANIFEST),
            false,
        )
        .expect("the content identity and the manifest agree");
        assert_eq!(
            outcome.content_identity,
            provenance_content_identity(&root.join(DEFAULT_PROVENANCE)).expect("identity")
        );
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
            require_commit: false,
        })
        .expect("the committed sample passes every check");
        assert_eq!(counts.expected, counts.in_report);
    }

    // --- temporary repositories: present, absent, shallow, no .git ----------

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test Harness")
            .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test Harness")
            .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
            .env("GIT_AUTHOR_DATE", "2026-10-22T12:00:00+0530")
            .env("GIT_COMMITTER_DATE", "2026-10-22T12:00:00+0530")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run git")
            .success()
    }

    fn init_repo(dir: &Path) {
        std::fs::create_dir_all(dir).expect("create the repository directory");
        assert!(git(dir, &["init", "-q", "-b", "main"]), "git init");
        assert!(git(dir, &["config", "user.name", "Test Harness"]));
        assert!(git(
            dir,
            &["config", "user.email", "harness@example.invalid"]
        ));
        assert!(git(dir, &["config", "commit.gpgsign", "false"]));
    }

    fn commit(dir: &Path, file: &str, message: &str) -> String {
        std::fs::write(dir.join(file), format!("{message}\n")).expect("write the committed file");
        assert!(git(dir, &["add", "-A"]), "git add");
        assert!(git(dir, &["commit", "-q", "-m", message]), "git commit");
        std::fs::read_to_string(dir.join(".git/HEAD"))
            .ok()
            .and_then(|head| {
                head.trim().strip_prefix("ref: ").and_then(|reference| {
                    std::fs::read_to_string(dir.join(".git").join(reference)).ok()
                })
            })
            .map(|sha| sha.trim().to_string())
            .expect("resolve HEAD")
    }

    /// Write one-file sample output beside a provenance record. The manifest is
    /// deliberately written in a different order from the canonical one.
    fn write_sample(dir: &Path, recorded_commit: &str) -> (PathBuf, PathBuf) {
        let payload = b"recorded sample payload";
        std::fs::write(dir.join("artifact.json"), payload).expect("write the payload");
        let digest = crate::package::sha256_hex(payload);
        let manifest = dir.join("SHA256SUMS");
        std::fs::write(
            &manifest,
            format!(
                "{}  zz-other.txt\n{digest}  artifact.json\n",
                "a".repeat(64)
            ),
        )
        .expect("write the manifest");
        // The second row names a file that does not exist; content identity
        // does not read the payload, but check_manifest does. Only the identity
        // tests use this manifest directly.
        let identity = content_identity_from_manifest(&manifest).expect("identity");
        let provenance = dir.join("PROVENANCE.txt");
        std::fs::write(
            &provenance,
            format!(
                "commit SHA: {recorded_commit}\n\
                 content identity: {identity}\n\
                 pinned inputs: third_party/MTL={}, third_party/liboqs={}\n\
                 rustc: rustc test\n\
                 date: 2026-10-22T12:00:00+05:30\n",
                "c".repeat(40),
                "d".repeat(40)
            ),
        )
        .expect("write the provenance");
        (manifest, provenance)
    }

    #[test]
    fn a_present_commit_is_reported_as_present() {
        let dir = temp_dir("commit-present");
        init_repo(&dir);
        let sha = commit(&dir, "a.txt", "first");
        let (manifest, provenance) = write_sample(&dir, &sha);
        let outcome =
            verify_provenance(&dir, &provenance, &manifest, false).expect("verification runs");
        assert_eq!(outcome.commit_state, CommitState::Present);
        assert!(commit_message(outcome.commit_state, &outcome.commit)
            .contains("present in this repository"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_absent_commit_reports_the_public_copy_reason_without_failing() {
        let dir = temp_dir("commit-absent");
        init_repo(&dir);
        commit(&dir, "a.txt", "first");
        let absent = "0".repeat(40);
        let (manifest, provenance) = write_sample(&dir, &absent);
        let outcome =
            verify_provenance(&dir, &provenance, &manifest, false).expect("absence is a skip");
        assert_eq!(outcome.commit_state, CommitState::Absent);
        assert_eq!(
            commit_message(outcome.commit_state, &outcome.commit),
            "provenance commit 0000000 not present in this repository (expected in a rewritten public copy); content identity verified"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shallow_clone_reports_the_absent_commit_without_failing() {
        let root = temp_dir("shallow");
        let origin = root.join("origin");
        init_repo(&origin);
        let first = commit(&origin, "a.txt", "first");
        commit(&origin, "b.txt", "second");
        let clone = root.join("clone");
        let url = format!("file://{}", origin.display());
        assert!(Command::new("git")
            .args(["clone", "-q", "--depth", "1", &url])
            .arg(&clone)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .expect("run git clone")
            .success());
        assert!(is_shallow(&clone), "the fixture must really be shallow");
        let (manifest, provenance) = write_sample(&clone, &first);
        let outcome = verify_provenance(&clone, &provenance, &manifest, false)
            .expect("a skip, not a failure");
        assert_eq!(outcome.commit_state, CommitState::Absent);
        assert!(commit_message(outcome.commit_state, &outcome.commit)
            .contains("expected in a rewritten public copy"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tarball_without_git_reports_the_commit_as_unavailable() {
        let dir = temp_dir("tarball");
        let absent = "e".repeat(40);
        let (manifest, provenance) = write_sample(&dir, &absent);
        assert!(!git_repository_present(&dir));
        let outcome =
            verify_provenance(&dir, &provenance, &manifest, false).expect("no git is a skip");
        assert_eq!(outcome.commit_state, CommitState::Unavailable);
        assert!(commit_message(outcome.commit_state, &outcome.commit).contains("no git repository"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_mode_refuses_an_absent_commit() {
        let dir = temp_dir("strict");
        init_repo(&dir);
        commit(&dir, "a.txt", "first");
        let absent = "0".repeat(40);
        let (manifest, provenance) = write_sample(&dir, &absent);
        let error = verify_provenance(&dir, &provenance, &manifest, true)
            .expect_err("strict mode must refuse an absent commit");
        assert!(error.to_string().contains("--require-commit"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_content_identity_that_disagrees_with_the_manifest_is_reported() {
        let dir = temp_dir("identity-mismatch");
        let (manifest, provenance) = write_sample(&dir, &"0".repeat(40));
        let text = std::fs::read_to_string(&provenance).expect("read the provenance");
        let replaced = text
            .lines()
            .map(|line| {
                if line.starts_with(CONTENT_IDENTITY_PREFIX) {
                    format!("content identity: sha256:{}", "0".repeat(64))
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&provenance, replaced).expect("rewrite the provenance");
        let error = verify_provenance(&dir, &provenance, &manifest, false)
            .expect_err("a stale identity must fail");
        assert!(
            error
                .to_string()
                .contains("does not match the current SHA256SUMS"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_content_identity_ignores_manifest_row_order() {
        let dir = temp_dir("identity-order");
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        std::fs::write(
            &first,
            format!("{}  b.txt\n{}  a.txt\n", "b".repeat(64), "a".repeat(64)),
        )
        .expect("write the first manifest");
        std::fs::write(
            &second,
            format!("{}  a.txt\n{}  b.txt\n", "a".repeat(64), "b".repeat(64)),
        )
        .expect("write the second manifest");
        assert_eq!(
            content_identity_from_manifest(&first).expect("first identity"),
            content_identity_from_manifest(&second).expect("second identity")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
