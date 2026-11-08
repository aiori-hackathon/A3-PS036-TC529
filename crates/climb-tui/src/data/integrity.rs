//! The Integrity tab's view model: the claims drift check, the artifact
//! inventory, and the provenance block.
//!
//! One check runs per results load ([`IntegrityView::run`]) and drives both
//! the Overview tab's report-health badge and this tab; the comparison
//! itself lives in `climb_report::check` and is never re-implemented here.
//!
//! Artifact roles: the results file is the *reference*; the
//! `climb-report/` *outputs* are stale when older than the results file;
//! the consolidation *inputs* are never compared against the results
//! mtime (the results legitimately postdate them) — they are listed
//! present/missing with their modification time. The reference row turns
//! stale when the results file itself is older than 24 hours.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use climb_report::{CheckReport, CheckStatus, ClimbResults, CONSOLIDATE_COMMAND};

use super::{mtime, report_path, MAX_FRESH_AGE};

/// The command that refreshes every artifact the inventory lists.
pub const REFRESH_COMMAND: &str = "cargo xtask reproduce --report-only --quick --skip-dns";

/// Role of an inventoried artifact relative to the results file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactRole {
    /// The consolidated results file itself — the reference every check
    /// compares against.
    Reference,
    /// Generated from the results (`cargo xtask report`); stale when older
    /// than the results file.
    Output,
    /// Consumed by consolidation; present or missing, never stale.
    Input,
}

impl ArtifactRole {
    /// Fixed-width tag shown before the path.
    pub fn tag(self) -> &'static str {
        match self {
            ArtifactRole::Reference => "ref",
            ArtifactRole::Output => "out",
            ArtifactRole::Input => "in ",
        }
    }
}

/// Health of one inventoried artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactState {
    /// The file exists (and, for outputs, is at least as new as the results).
    Present,
    /// An output older than the results file, or a reference older than the
    /// freshness threshold.
    Stale,
    /// The file does not exist at the expected path.
    Missing,
}

impl ArtifactState {
    /// Lowercase label shown after the modification time.
    pub fn label(self) -> &'static str {
        match self {
            ArtifactState::Present => "present",
            ArtifactState::Stale => "stale",
            ArtifactState::Missing => "missing",
        }
    }
}

/// One line of the artifact inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactRow {
    /// Path relative to the results file's directory.
    pub path: String,
    /// Role relative to the results file.
    pub role: ArtifactRole,
    /// Last modification time; `None` when absent or unknown.
    pub modified: Option<SystemTime>,
    /// Present / stale / missing.
    pub state: ArtifactState,
}

/// Outcome of the report-vs-results check carried into the tab.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckState {
    /// No check has run (view built before a load).
    NotRun,
    /// The report file did not exist; the carried report lists every claim
    /// the results imply as missing.
    ReportMissing { path: PathBuf, report: CheckReport },
    /// The report file exists but could not be read — a hard error.
    Failed { path: PathBuf, error: String },
    /// The check ran against the report file.
    Ran { path: PathBuf, report: CheckReport },
}

/// Colour class for the banner line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerTone {
    /// Every claim agrees.
    Ok,
    /// Something is absent but nothing contradicts the results.
    Warn,
    /// The report contradicts the results, or the check itself failed.
    Bad,
}

/// State of the signing speed-up bound inside the loaded results document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundState {
    /// The bound was checked and holds.
    Ok,
    /// The bound was checked and violated.
    Violated,
    /// The document carries no bound check at all — the bound was never
    /// checked, which is not a pass.
    Unchecked,
    /// No results document is loaded, so there is no bound to read.
    NotApplicable,
}

impl BoundState {
    /// Read the bound state out of a results document.
    ///
    /// A document without the bound field reports [`BoundState::Unchecked`]:
    /// absent means "never checked", never "ok".
    pub fn of(results: &ClimbResults) -> Self {
        match results.derived.bound_ok {
            Some(true) => BoundState::Ok,
            Some(false) => BoundState::Violated,
            None => BoundState::Unchecked,
        }
    }
}

/// Provenance block: where the checked numbers came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    /// Commit SHA of the workspace checkout, read from `.git` in-process;
    /// `None` when the checkout layout is not recognized.
    pub commit: Option<String>,
    /// `rustc` version recorded in the environment fingerprint.
    pub rustc: Option<String>,
    /// Recorded submodule `(path, sha)` pairs.
    pub submodules: Vec<(String, String)>,
    /// First 16 hex characters of a SHA-256 over the serialized environment
    /// fingerprint — a short handle for "which machine produced this".
    pub host_fingerprint: String,
    /// The results document's own `generated_at` timestamp.
    pub generated_at: String,
}

/// Integrity view over one loaded results document.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityView {
    check: CheckState,
    artifacts: Vec<ArtifactRow>,
    provenance: Provenance,
    bound: BoundState,
}

impl IntegrityView {
    /// A placeholder with no check, no artifacts and empty provenance;
    /// used by tests and before the first real load.
    pub fn not_run() -> Self {
        Self {
            check: CheckState::NotRun,
            artifacts: Vec::new(),
            provenance: Provenance {
                commit: None,
                rustc: None,
                submodules: Vec::new(),
                host_fingerprint: String::new(),
                generated_at: String::new(),
            },
            // No document is loaded, so there is no bound to read — this is
            // not an unchecked bound and must not read as one.
            bound: BoundState::NotApplicable,
        }
    }

    /// Run the claims check against the report next to `results_path`,
    /// inventory the artifacts, and collect the provenance block.
    ///
    /// An absent report is a warning-shaped outcome (see
    /// `climb_report::check`), never an error; a report that exists but
    /// cannot be read becomes [`CheckState::Failed`].
    pub fn run(results_path: &Path, results: &ClimbResults) -> Self {
        let path = report_path(results_path);
        let check = match climb_report::check(results, &path) {
            Ok(report) if report.report_found => CheckState::Ran { path, report },
            Ok(report) => CheckState::ReportMissing { path, report },
            Err(error) => CheckState::Failed {
                path,
                error: error.to_string(),
            },
        };
        Self {
            check,
            artifacts: inventory(results_path, Path::new(&results.workspace_root)),
            provenance: Provenance::of(results),
            bound: BoundState::of(results),
        }
    }

    /// The state of the signing speed-up bound in the loaded document.
    pub fn bound_state(&self) -> BoundState {
        self.bound
    }

    /// The outcome of the check.
    pub fn check_state(&self) -> &CheckState {
        &self.check
    }

    /// The report file the check read, when a check has run.
    pub fn report_path(&self) -> Option<&Path> {
        match &self.check {
            CheckState::NotRun => None,
            CheckState::ReportMissing { path, .. }
            | CheckState::Failed { path, .. }
            | CheckState::Ran { path, .. } => Some(path),
        }
    }

    /// Number of drifted claims, when the check ran to completion.
    pub fn drift_count(&self) -> Option<usize> {
        match &self.check {
            CheckState::Ran { report, .. } => Some(report.problems.len()),
            _ => None,
        }
    }

    /// The check's claim rows: every results-implied claim once the check
    /// ran, and — for an absent report — the same rows listed as missing.
    pub fn claims(&self) -> &[climb_report::ClaimCheck] {
        match &self.check {
            CheckState::Ran { report, .. } | CheckState::ReportMissing { report, .. } => {
                &report.claims
            }
            _ => &[],
        }
    }

    /// The artifact inventory.
    pub fn artifacts(&self) -> &[ArtifactRow] {
        &self.artifacts
    }

    /// The provenance block.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The banner text and its colour class.
    pub fn banner(&self) -> (BannerTone, String) {
        // A document that never checked the signing speed-up bound cannot back
        // any claim, whatever the report says. The banner says so and names
        // the command that produces a document that does carry the check.
        if self.bound == BoundState::Unchecked {
            return (
                BannerTone::Bad,
                format!("BOUND NOT CHECKED \u{2014} regenerate with `{CONSOLIDATE_COMMAND}`"),
            );
        }
        match &self.check {
            CheckState::NotRun => (BannerTone::Warn, "CHECK NOT RUN".to_string()),
            CheckState::ReportMissing { path, .. } => (
                BannerTone::Warn,
                format!("REPORT NOT FOUND \u{2014} {}", crate::paths::display(path)),
            ),
            CheckState::Failed { error, .. } => {
                (BannerTone::Bad, format!("CHECK FAILED \u{2014} {error}"))
            }
            CheckState::Ran { report, .. } => {
                // A violated signing speed-up bound is a failing claim even when
                // the report and the results agree that it is violated.
                let bound_bad = report.claims.iter().any(|c| {
                    c.id == "signing-speedup-bound" && c.value_in_results == Value::Bool(false)
                });
                if bound_bad {
                    return (BannerTone::Bad, "BOUND VIOLATION".to_string());
                }
                let (agree, drift, missing) = report.counts();
                if drift > 0 {
                    let ids: Vec<&str> = report.drifts().map(|row| row.id.as_str()).collect();
                    (
                        BannerTone::Bad,
                        format!(
                            "{drift} DRIFT{} \u{2014} {}",
                            if drift == 1 { "" } else { "S" },
                            ids.join(" \u{b7} ")
                        ),
                    )
                } else if missing > 0 {
                    let ids: Vec<&str> = report
                        .claims
                        .iter()
                        .filter(|row| row.status == CheckStatus::Missing)
                        .map(|row| row.id.as_str())
                        .collect();
                    (
                        BannerTone::Warn,
                        format!("{missing} MISSING \u{2014} {}", ids.join(" \u{b7} ")),
                    )
                } else {
                    (BannerTone::Ok, format!("ALL CLAIMS AGREE ({agree})"))
                }
            }
        }
    }
}

impl Provenance {
    /// Collect provenance from a parsed results document.
    fn of(results: &ClimbResults) -> Self {
        let environment = &results.environment;
        let rustc = environment
            .toolchain
            .get("rustc_version")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let submodules = environment
            .submodules
            .iter()
            .filter_map(|entry| {
                let path = entry.get("path")?.as_str()?.to_string();
                let sha = entry.get("sha")?.as_str()?.to_string();
                Some((path, sha))
            })
            .collect();
        let host_fingerprint = environment_fingerprint_hash(environment);
        Self {
            commit: git_commit_sha(&results.workspace_root),
            rustc,
            submodules,
            host_fingerprint,
            generated_at: results.generated_at.clone(),
        }
    }
}

/// First 16 hex characters of a SHA-256 over the serialized environment
/// fingerprint. Deterministic for a given fingerprint document.
fn environment_fingerprint_hash(
    environment: &climb_report::results::EnvironmentFingerprint,
) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(environment).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Best-effort read of the workspace checkout's commit SHA from `.git`,
/// without shelling out. A symbolic `HEAD` is followed into the ref file
/// and then `packed-refs`; `None` when the layout is not recognized.
fn git_commit_sha(workspace_root: &str) -> Option<String> {
    let git_dir = Path::new(workspace_root).join(".git");
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        if let Ok(content) = std::fs::read_to_string(git_dir.join(reference)) {
            let sha = content.trim();
            if !sha.is_empty() {
                return Some(sha.to_string());
            }
        }
        let packed = std::fs::read_to_string(git_dir.join("packed-refs")).ok()?;
        for line in packed.lines() {
            if let Some((sha, reference_in_file)) = line.split_once(' ') {
                if reference_in_file.trim() == reference
                    && !sha.starts_with('#')
                    && !sha.starts_with('^')
                {
                    return Some(sha.to_string());
                }
            }
        }
        None
    } else if head.len() >= 40 && head.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(head.to_string())
    } else {
        None
    }
}

/// Build the artifact inventory: the reference row, then the outputs and
/// inputs in display order.
///
/// Artifacts normally sit beside the results file. The environment fingerprint
/// is the exception: it is written at the repository root and, in older runs,
/// under `target/`. Every candidate is checked in turn while the row keeps its
/// stable display name (`environment.json`).
fn inventory(results_path: &Path, workspace_root: &Path) -> Vec<ArtifactRow> {
    let results_mtime = mtime(results_path);
    let dir = results_path.parent().unwrap_or_else(|| Path::new("."));
    let mut rows = vec![reference_row(results_path)];
    for (relative, role) in ARTIFACTS {
        let path = artifact_path(dir, workspace_root, relative);
        let modified = mtime(&path);
        let state = match (role, modified) {
            (_, None) => ArtifactState::Missing,
            (ArtifactRole::Output, Some(stamp))
                if results_mtime.is_some_and(|results| stamp < results) =>
            {
                ArtifactState::Stale
            }
            _ => ArtifactState::Present,
        };
        rows.push(ArtifactRow {
            path: (*relative).to_string(),
            role: *role,
            modified,
            state,
        });
    }
    debug_assert_eq!(
        rows.len(),
        ARTIFACTS.len() + 1,
        "one reference row plus every inventoried artifact"
    );
    rows
}

/// Resolve one inventoried artifact to the file actually on disk. Most live
/// beside the results file; `environment.json` is also accepted at the
/// workspace root and under `target/`, which are the two places different runs
/// have written it.
fn artifact_path(dir: &Path, workspace_root: &Path, relative: &str) -> PathBuf {
    let beside = dir.join(relative);
    if relative != "environment.json" {
        return beside;
    }
    let candidates = [
        beside.clone(),
        workspace_root.join("target").join(relative),
        workspace_root.join(relative),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .unwrap_or(beside)
}

/// The reference row: the results file itself, stale once older than the
/// freshness threshold.
fn reference_row(results_path: &Path) -> ArtifactRow {
    let modified = mtime(results_path);
    let age = modified.and_then(|stamp| stamp.elapsed().ok());
    let state = match age {
        Some(age) if age > MAX_FRESH_AGE => ArtifactState::Stale,
        _ => ArtifactState::Present,
    };
    ArtifactRow {
        path: results_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "climb-results.json".to_string()),
        role: ArtifactRole::Reference,
        modified,
        state,
    }
}

/// The inventoried artifacts, in display order, relative to the results
/// file's directory.
const ARTIFACTS: &[(&str, ArtifactRole)] = &[
    ("climb-report/report.html", ArtifactRole::Output),
    ("climb-report/charts", ArtifactRole::Output),
    ("wire-report.json", ArtifactRole::Input),
    ("memory-report.json", ArtifactRole::Input),
    ("dns-capture.json", ArtifactRole::Input),
    ("ladder-verification.json", ArtifactRole::Input),
    ("climb-classical-signatures.json", ArtifactRole::Input),
    ("environment.json", ArtifactRole::Input),
];

/// Format a modification time as `YYYY-MM-DD HH:MM UTC`; `None` for times
/// before the Unix epoch or with an unknown clock.
fn format_utc(time: SystemTime) -> Option<String> {
    let seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        (rest % 3600) / 60
    ))
}

/// Gregorian civil date from a count of days since 1970-01-01, using the
/// standard days-to-civil conversion (valid for non-negative day counts).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Render one artifact row as a single display line: role tag, path,
/// modification time, state.
pub fn artifact_line(row: &ArtifactRow, width: usize, ascii: bool) -> String {
    let modified = row.modified.and_then(format_utc);
    let placeholder = if ascii { "--" } else { "\u{2014}" };
    let state = match (row.role, row.state) {
        (ArtifactRole::Reference, ArtifactState::Stale) => "older than 24 hours",
        _ => row.state.label(),
    };
    let line = format!(
        "[{}] {}  {}  {}",
        row.role.tag(),
        row.path,
        modified.unwrap_or_else(|| placeholder.to_string()),
        state
    );
    crate::widgets::ellipsize(&line, width, ascii)
}

#[cfg(test)]
mod tests;
