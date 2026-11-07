//! Artifact and baseline-document reads for the Method tab.
//!
//! Every sibling artifact goes through the parser consolidation uses; an absent
//! file is a neutral state, a present but unreadable or unparseable one is
//! recorded with its path and error.

use std::path::{Path, PathBuf};

/// The deterministic dataset seed plus a human-readable detail, as a seed
/// artifact parser reads them: either may be absent.
type SeedParse = (Option<u64>, Option<String>);

/// A parser for one sibling seed artifact: the file text in, the seed parse or
/// an error message out.
type SeedParser = fn(&str) -> Result<SeedParse, String>;

/// The baseline document embedded at build time; used when no copy is readable
/// next to the loaded results.
const EMBEDDED_BASELINES: &str = include_str!("../../../../../docs/BASELINES.md");

/// The baseline document, as the tab renders it.
///
/// `text` is the document the tab shows (the copy found next to the results, or
/// the embedded one); `source` names where that copy came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineDocument {
    /// The markup, rendered line by line by the tab.
    pub text: String,
    /// Where the rendered copy came from.
    pub source: BaselineSource,
}

impl BaselineDocument {
    /// The markup the tab renders.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the rendered copy came from.
    pub fn source(&self) -> &BaselineSource {
        &self.source
    }
}

/// The sibling artifacts whose deterministic dataset seed the tab reports, in
/// display order, with the path they live at relative to the results file.
pub(super) const SEED_ARTIFACTS: &[(&str, &str)] = &[
    ("wire report", "wire-report.json"),
    ("memory report", "memory-report.json"),
    ("ladder verification", "ladder-verification.json"),
];

/// Where the baseline document rendered on the tab came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaselineSource {
    /// A copy readable next to the loaded results.
    File(PathBuf),
    /// The copy compiled into the binary.
    Embedded,
}

impl BaselineSource {
    /// Short label naming the origin, shown in the panel.
    pub fn label(&self) -> String {
        match self {
            BaselineSource::File(path) => format!("read from {}", crate::paths::display(path)),
            BaselineSource::Embedded => "embedded copy of the baseline document".to_string(),
        }
    }
}

/// How one sibling artifact read went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactRead {
    /// The artifact parsed; `seed` is its dataset seed when it records one.
    Read {
        /// The deterministic dataset seed, when the artifact records one.
        seed: Option<u64>,
        /// Any extra context worth showing next to the seed (row counts,
        /// measurement method, algorithm).
        detail: Option<String>,
    },
    /// The artifact is not on disk next to the results.
    Missing,
    /// The artifact exists but could not be read or parsed.
    Invalid {
        /// The path that failed.
        path: PathBuf,
        /// The read or parse error, verbatim.
        error: String,
    },
    /// No results load has happened, so no sibling artifact was looked for.
    Unread,
}

/// One artifact's seed row, from [`SEED_ARTIFACTS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedRow {
    /// Display name of the artifact.
    pub artifact: &'static str,
    /// What reading it produced.
    pub read: ArtifactRead,
}

/// Resolve the baseline document: a readable copy next to the results file
/// first (`<root>/docs/BASELINES.md`, then `<results dir>/../docs/BASELINES.md`),
/// then the embedded copy. Only a genuinely absent or unreadable candidate
/// falls through; the working directory is never consulted, so a test's render
/// cannot depend on where the test binary was started.
pub(super) fn resolve_baselines(results_path: &Path) -> BaselineSource {
    for candidate in baseline_candidates(results_path) {
        if std::fs::metadata(&candidate).is_ok() {
            return BaselineSource::File(candidate);
        }
    }
    BaselineSource::Embedded
}

/// The paths a copy of the baseline document may live at, in priority order:
/// the workspace root recorded in the results, then the results directory's
/// parent. Deterministic and independent of the process working directory.
fn baseline_candidates(results_path: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(source) = std::fs::read_to_string(results_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&source) {
            if let Some(root) = value.get("workspace_root").and_then(|v| v.as_str()) {
                candidates.push(Path::new(root).join("docs").join("BASELINES.md"));
            }
        }
    }
    if let Some(dir) = results_path.parent() {
        candidates.push(dir.join("..").join("docs").join("BASELINES.md"));
    }
    candidates
}

/// The text the baseline section shows: the resolved copy when it is readable,
/// otherwise the embedded copy.
pub(super) fn baseline_text(source: &BaselineSource) -> String {
    match source {
        BaselineSource::File(path) => {
            std::fs::read_to_string(path).unwrap_or_else(|_| EMBEDDED_BASELINES.to_string())
        }
        BaselineSource::Embedded => EMBEDDED_BASELINES.to_string(),
    }
}

/// Read the seed rows, pairing each artifact with its parser.
pub(super) fn read_seed_rows(results_path: &Path) -> Vec<SeedRow> {
    let dir = results_path.parent().unwrap_or_else(|| Path::new("."));
    SEED_ARTIFACTS
        .iter()
        .zip(SEED_PARSERS)
        .map(|((artifact, relative), parse)| SeedRow {
            artifact,
            read: read_artifact(&dir.join(relative), parse),
        })
        .collect()
}

/// Read one artifact and parse it with `parse`; absent is a neutral state,
/// while a read or parse failure is recorded with the path and the error.
fn read_artifact(path: &Path, parse: SeedParser) -> ArtifactRead {
    if std::fs::metadata(path).is_err() {
        return ArtifactRead::Missing;
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return ArtifactRead::Invalid {
                path: path.to_path_buf(),
                error: error.to_string(),
            };
        }
    };
    match parse(&text) {
        Ok((seed, detail)) => ArtifactRead::Read { seed, detail },
        Err(error) => ArtifactRead::Invalid {
            path: path.to_path_buf(),
            error,
        },
    }
}

/// Parse a wire report (a bare array of per-size rows) for its dataset seed.
///
/// The wire report has no schema module of its own, so the minimal shape is
/// read here: the seed is consistent across the rows, and the row count is
/// worth showing beside it.
fn parse_seed(text: &str) -> Result<(Option<u64>, Option<String>), String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    match value {
        serde_json::Value::Array(rows) => {
            let seeds: Vec<u64> = rows
                .iter()
                .filter_map(|row| row.get("seed").and_then(|v| v.as_u64()))
                .collect();
            let seed = distinct_seed(&seeds);
            Ok((seed, Some(format!("{} size rows", rows.len()))))
        }
        other => Err(format!(
            "expected the per-size row array, found {}",
            artifact_kind(&other)
        )),
    }
}

/// The JSON kind of a value, for a parse error that names what was found.
fn artifact_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Array(_) => "per-size row",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Null => "null",
    }
}

/// The seed shared by every row, or `None` when the rows disagree or carry
/// none — a disagreement is never resolved by picking one.
pub(super) fn distinct_seed(seeds: &[u64]) -> Option<u64> {
    let first = *seeds.first()?;
    seeds.iter().all(|s| *s == first).then_some(first)
}

/// Parse the memory report with the shared artifact parser, so the Method tab
/// and consolidation can never disagree about what the file is.
fn parse_memory(text: &str) -> Result<(Option<u64>, Option<String>), String> {
    match climb_report::parse_memory_artifact(text).map_err(|e| e.to_string())? {
        climb_report::MemoryArtifact::Isolated(report) => {
            let method = report.method.clone();
            Ok((Some(report.seed), Some(method)))
        }
        climb_report::MemoryArtifact::LegacyRssDelta(report) => {
            Ok((Some(report.seed), Some("legacy rss-delta".to_string())))
        }
        climb_report::MemoryArtifact::LegacyComparison(rows) => Ok((
            None,
            Some(format!("legacy comparison rows ({})", rows.len())),
        )),
    }
}

/// Parse the ladder-verification document for its seed and algorithm.
fn parse_ladder(text: &str) -> Result<(Option<u64>, Option<String>), String> {
    let doc: climb_report::LadderVerification =
        serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok((Some(doc.seed), Some(doc.algorithm)))
}

/// The parser matching each entry of [`SEED_ARTIFACTS`], in the same order.
const SEED_PARSERS: [SeedParser; 3] = [parse_seed, parse_memory, parse_ladder];

/// The value shown for one artifact read.
pub(super) fn seed_text(read: &ArtifactRead) -> String {
    match read {
        ArtifactRead::Read { seed, detail } => match (seed, detail) {
            (Some(seed), Some(detail)) => format!("seed {seed} · {detail}"),
            (Some(seed), None) => format!("seed {seed}"),
            (None, Some(detail)) => detail.clone(),
            (None, None) => "seed not recorded".to_string(),
        },
        ArtifactRead::Missing => "not on disk — this run did not produce it".to_string(),
        ArtifactRead::Invalid { path, error } => {
            format!("unreadable — {}: {error}", crate::paths::display(path))
        }
        ArtifactRead::Unread => "not read — no results load has happened yet".to_string(),
    }
}
