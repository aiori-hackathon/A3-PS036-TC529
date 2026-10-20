//! The Method tab's view model: the experimental method written out for a
//! reader who has to judge the numbers on the other tabs — scope, baselines,
//! evidence classes, environment, sampling and the limits of what was measured.
//!
//! The tab is a document, not a measurement: the only I/O here is reading the
//! documents and artifacts that already describe the run. Baselines come from
//! the repository's baseline document when one is readable next to the results
//! (falling back to the copy embedded at build time), the environment rows and
//! the sampling table come from the loaded results, and the per-artifact seed
//! rows come from a read of the sibling artifacts — reusing the same parsers
//! consolidation uses, never a second parser that could disagree with them.
//!
//! A sibling artifact that is absent is a neutral "not on disk" state; one that
//! is present but unreadable or unparseable is reported with its path and the
//! error, never silently dropped.

use std::path::{Path, PathBuf};

use climb_report::results::EvidenceClass;

use super::{LoadState, ResultsView};

/// The deterministic dataset seed plus a human-readable detail, as a seed
/// artifact parser reads them: either may be absent.
type SeedParse = (Option<u64>, Option<String>);

/// A parser for one sibling seed artifact: the file text in, the seed parse or
/// an error message out.
type SeedParser = fn(&str) -> Result<SeedParse, String>;

/// One environment row: its label and the reader that pulls its value out of a
/// loaded results view.
type EnvironmentReader = fn(&ResultsView) -> Option<String>;

/// The baseline document embedded at build time; used when no copy is readable
/// next to the loaded results.
const EMBEDDED_BASELINES: &str = include_str!("../../../../docs/BASELINES.md");

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
const SEED_ARTIFACTS: &[(&str, &str)] = &[
    ("wire report", "wire-report.json"),
    ("memory report", "memory-report.json"),
    ("ladder verification", "ladder-verification.json"),
];

/// One line of a method section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodLine {
    /// A plain paragraph line, wrapped to the panel width by the renderer.
    Text(String),
    /// A labelled value, shown as `label  value` in an aligned column.
    Field {
        /// Left-hand label; may carry a dynamic suffix (e.g. `memory Small`).
        label: String,
        /// Right-hand value, already formatted.
        value: String,
    },
    /// A monospaced-ish fragment (a command, an identity) shown indented.
    Code(String),
    /// A caveat or qualification, rendered in the muted style.
    Note(String),
}

impl MethodLine {
    /// A labelled value line.
    fn field(label: impl Into<String>, value: impl Into<String>) -> Self {
        MethodLine::Field {
            label: label.into(),
            value: value.into(),
        }
    }

    /// A plain text line.
    fn text(text: impl Into<String>) -> Self {
        MethodLine::Text(text.into())
    }

    /// A code fragment line.
    fn code(text: impl Into<String>) -> Self {
        MethodLine::Code(text.into())
    }

    /// A muted note line.
    fn note(text: impl Into<String>) -> Self {
        MethodLine::Note(text.into())
    }
}

/// One titled section of the method document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSection {
    /// Section heading, used for the contents list and the panel title.
    pub title: String,
    /// The section body, in display order.
    pub lines: Vec<MethodLine>,
}

impl MethodSection {
    fn new(title: &str, lines: Vec<MethodLine>) -> Self {
        Self {
            title: title.to_string(),
            lines,
        }
    }
}

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
            BaselineSource::File(path) => format!("read from {}", path.display()),
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

/// One labelled row of the environment block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRow {
    /// Fixed label.
    pub label: &'static str,
    /// The value from the artifact; `None` when the run did not record it.
    pub value: Option<String>,
}

/// The Method tab's view over one loaded results document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodView {
    sections: Vec<MethodSection>,
    baselines: BaselineDocument,
    environment: Vec<EnvironmentRow>,
    seeds: Vec<SeedRow>,
}

impl MethodView {
    /// The sections' titles in display order, always the same regardless of
    /// whether results loaded — a section with no results-dependent body
    /// explains why, rather than disappearing from the contents list.
    pub const SECTION_TITLES: [&'static str; 6] = [
        "Scope",
        "Baselines A/B/C",
        "Evidence classes",
        "Environment and pinning",
        "Sampling and extrapolation",
        "Known limits",
    ];

    /// A view with no results-dependent content: the sections that describe
    /// the method itself and the embedded baseline document.
    pub fn embedded() -> Self {
        Self::build_with(Path::new(""), None, BaselineSource::Embedded)
    }

    /// Build the view for one load outcome.
    ///
    /// The sibling artifacts are read once, here, so rendering never touches
    /// the filesystem. An unreadable sibling is recorded with its path and
    /// error rather than dropped.
    pub fn build(results_path: &Path, state: &LoadState) -> Self {
        let source = resolve_baselines(results_path);
        let view = match state {
            LoadState::Loaded(view) => Some(view.as_ref()),
            _ => None,
        };
        Self::build_with(results_path, view, source)
    }

    fn build_with(results_path: &Path, view: Option<&ResultsView>, source: BaselineSource) -> Self {
        let baselines = BaselineDocument {
            text: baseline_text(&source),
            source,
        };
        let seeds = match view {
            Some(_) => read_seed_rows(results_path),
            None => SEED_ARTIFACTS
                .iter()
                .map(|(artifact, _)| SeedRow {
                    artifact,
                    read: ArtifactRead::Unread,
                })
                .collect(),
        };
        let (sections, environment) = match view {
            Some(view) => (sections_for(view, &seeds), environment_rows(view)),
            None => (unloaded_sections(), Vec::new()),
        };
        Self {
            sections,
            baselines,
            environment,
            seeds,
        }
    }

    /// The method sections, in display order.
    pub fn sections(&self) -> &[MethodSection] {
        &self.sections
    }

    /// The baseline document the tab renders.
    pub fn baselines(&self) -> &BaselineDocument {
        &self.baselines
    }

    /// The environment rows read from the results.
    pub fn environment(&self) -> &[EnvironmentRow] {
        &self.environment
    }

    /// The per-artifact seed rows.
    pub fn seeds(&self) -> &[SeedRow] {
        &self.seeds
    }
}

/// Resolve the baseline document: a readable copy next to the results file
/// first (`<root>/docs/BASELINES.md`, then `<results dir>/../docs/BASELINES.md`),
/// then the embedded copy. Only a genuinely absent or unreadable candidate
/// falls through; the working directory is never consulted, so a test's render
/// cannot depend on where the test binary was started.
fn resolve_baselines(results_path: &Path) -> BaselineSource {
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
fn baseline_text(source: &BaselineSource) -> String {
    match source {
        BaselineSource::File(path) => {
            std::fs::read_to_string(path).unwrap_or_else(|_| EMBEDDED_BASELINES.to_string())
        }
        BaselineSource::Embedded => EMBEDDED_BASELINES.to_string(),
    }
}

/// Read the seed rows, pairing each artifact with its parser.
fn read_seed_rows(results_path: &Path) -> Vec<SeedRow> {
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
fn distinct_seed(seeds: &[u64]) -> Option<u64> {
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

/// The method sections for a loaded view.
fn sections_for(view: &ResultsView, seeds: &[SeedRow]) -> Vec<MethodSection> {
    vec![
        scope_section(view),
        baselines_section(),
        classes_section(),
        environment_section(view),
        sampling_section(view, seeds),
        limits_section(view),
    ]
}

/// The same sections when no results are loaded: every section is still listed
/// (so the contents list matches the loaded case) but carries a line saying
/// what would fill it.
fn unloaded_sections() -> Vec<MethodSection> {
    MethodView::SECTION_TITLES
        .iter()
        .map(|title| {
            MethodSection::new(
                title,
                vec![MethodLine::note(
                    "no results loaded — the sections that read the run's artifact are empty \
                     until a results file loads (press r to reload)",
                )],
            )
        })
        .collect()
}

/// Scope: what was benchmarked and how to reproduce it.
fn scope_section(view: &ResultsView) -> MethodSection {
    let sizes = signing_sizes(view);
    let lines = vec![
        MethodLine::text(
            "This suite measures Merkle Tree Ladder amortisation for post-quantum DNSSEC \
             signing. The same deterministic message sets are signed with MTL amortisation \
             and without it, and the difference is reported per dataset size.",
        ),
        MethodLine::field(
            "algorithm",
            view.algorithm()
                .unwrap_or_else(|| "not recorded".to_string()),
        ),
        MethodLine::field(
            "dataset sizes",
            if sizes.is_empty() {
                "not recorded".to_string()
            } else {
                sizes.join(", ")
            },
        ),
        MethodLine::field("results generated", view.results().generated_at.clone()),
        MethodLine::text("Reproduce every artifact from a clean checkout:"),
        MethodLine::code("cargo xtask reproduce"),
        MethodLine::code("cargo xtask report --check"),
        MethodLine::code("cargo run -p climb-tui"),
        MethodLine::note(
            "Empirical measurements only — no formal security guarantee or proof is claimed.",
        ),
    ];
    MethodSection::new("Scope", lines)
}

/// The baseline document, rendered verbatim by the renderer.
fn baselines_section() -> MethodSection {
    MethodSection::new("Baselines A/B/C", Vec::new())
}

/// The evidence classes, with the same wording the report and legends use.
fn classes_section() -> MethodSection {
    let mut lines = Vec::new();
    for class in [
        EvidenceClass::Measured,
        EvidenceClass::Extrapolated,
        EvidenceClass::Analytical,
        EvidenceClass::Derived,
    ] {
        lines.push(MethodLine::field(class.label(), class_explanation(class)));
    }
    lines.push(MethodLine::note(
        "A number is only ever reported at the class its production path earns: a total \
         scaled from a per-message measurement is EXTRAPOLATED, a model output is \
         ANALYTICAL, and a ratio of two reported numbers is DERIVED.",
    ));
    MethodSection::new("Evidence classes", lines)
}

/// The explanation shown beside one evidence class.
fn class_explanation(class: EvidenceClass) -> &'static str {
    match class {
        EvidenceClass::Measured => "read directly from a measurement artifact",
        EvidenceClass::Extrapolated => "N × per-message identity over the k measured sizes",
        EvidenceClass::Analytical => "produced by a closed-form model",
        EvidenceClass::Derived => "computed as a ratio or summary of other values",
    }
}

/// The environment fingerprint the run was taken under. The body is rendered
/// from [`MethodView::environment`], so the section itself carries no lines.
fn environment_section(_view: &ResultsView) -> MethodSection {
    MethodSection::new("Environment and pinning", Vec::new())
}

/// The labelled environment rows.
fn environment_rows(view: &ResultsView) -> Vec<EnvironmentRow> {
    let rows: [(&'static str, EnvironmentReader); 8] = [
        ("cpu", |v| v.cpu_model()),
        ("governor", |v| v.governor()),
        ("pinning", env_pinning),
        ("kernel", env_kernel),
        ("rustc", |v| v.rustc()),
        ("libMTL pin", |v| v.mtl_sha()),
        ("liboqs pin", |v| v.liboqs_sha()),
        ("recorded", env_timestamp),
    ];
    rows.iter()
        .map(|(label, read)| EnvironmentRow {
            label,
            value: read(view),
        })
        .collect()
}

/// The pinning expression: the recorded `taskset` command when the run wrote
/// one, otherwise the pinned core.
fn env_pinning(view: &ResultsView) -> Option<String> {
    super::overview::json_str(
        &view.results().environment.benchmark_runtime,
        "pin_expression",
    )
    .or_else(|| view.pinned_core())
}

/// The kernel release the run recorded.
fn env_kernel(view: &ResultsView) -> Option<String> {
    super::overview::json_str(&view.results().environment.system, "kernel_release")
}

/// The environment fingerprint's own timestamp.
fn env_timestamp(view: &ResultsView) -> Option<String> {
    let stamp = view.results().environment.timestamp.clone();
    (!stamp.is_empty()).then_some(stamp)
}

/// Sampling: how many repetitions stand behind each number, and the dataset
/// seed each sibling artifact was produced with.
fn sampling_section(view: &ResultsView, seeds: &[SeedRow]) -> MethodSection {
    let mut lines = Vec::new();
    let sizes = signing_sizes(view);
    if sizes.is_empty() {
        lines.push(MethodLine::note("no sign batches recorded"));
    } else {
        lines.push(MethodLine::text(
            "Repetitions, and the dataset seed behind each sibling artifact:",
        ));
        lines.push(MethodLine::note(
            "Criterion reports a mean over a measured sample; the k below is the number of \
             dataset sizes measured, and larger sizes are EXTRAPOLATED from those by the \
             N × per-message identity.",
        ));
        lines.push(MethodLine::code(format!(
            "{} × per-message identity over the k measured sizes",
            sizes.len()
        )));
    }

    for row in memory_rows(view) {
        lines.push(MethodLine::field(
            format!("memory {}", row.size),
            format!("{} samples · {}", row.samples, row.method),
        ));
    }
    for row in plain_rows(view) {
        lines.push(MethodLine::field(
            format!("plain {}", row.size),
            match row.samples {
                Some(k) => format!("{} (k = {k})", row.class.as_str()),
                None => format!("{} (measured)", row.class.as_str()),
            },
        ));
    }
    for row in comparison_rows(view) {
        lines.push(MethodLine::field(
            format!("comparison {}", row.size),
            row.class.as_str().to_string(),
        ));
    }

    for row in seeds {
        lines.push(MethodLine::field(row.artifact, seed_text(&row.read)));
    }
    MethodSection::new("Sampling and extrapolation", lines)
}

/// The distinct dataset sizes the signing benchmark covered, in artifact order
/// and formatted the way the other tabs format counts.
fn signing_sizes(view: &ResultsView) -> Vec<String> {
    let mut sizes: Vec<String> = Vec::new();
    for row in &view.results().benchmark.signing {
        let label = super::title_size(&row.size);
        if !sizes.contains(&label) {
            sizes.push(label);
        }
    }
    sizes
}

/// One memory row's sample count and measurement method.
struct MemoryRow {
    size: String,
    samples: usize,
    method: String,
}

fn memory_rows(view: &ResultsView) -> Vec<MemoryRow> {
    view.results()
        .memory
        .measured
        .iter()
        .map(|row| MemoryRow {
            size: super::title_size(&row.size),
            samples: row.samples,
            method: row.method.clone(),
        })
        .collect()
}

/// One plain-baseline row's size, class and sample count.
struct PlainRow {
    size: String,
    class: EvidenceClass,
    samples: Option<u32>,
}

fn plain_rows(view: &ResultsView) -> Vec<PlainRow> {
    view.results()
        .plain
        .iter()
        .map(|row| PlainRow {
            size: super::title_size(&row.size),
            class: row.class,
            samples: row.samples,
        })
        .collect()
}

/// One comparison row's size and class.
struct ComparisonRow {
    size: String,
    class: EvidenceClass,
}

fn comparison_rows(view: &ResultsView) -> Vec<ComparisonRow> {
    view.results()
        .comparisons
        .iter()
        .map(|row| ComparisonRow {
            size: super::title_size(&row.size),
            class: row.class,
        })
        .collect()
}

/// The value shown for one artifact read.
fn seed_text(read: &ArtifactRead) -> String {
    match read {
        ArtifactRead::Read { seed, detail } => match (seed, detail) {
            (Some(seed), Some(detail)) => format!("seed {seed} · {detail}"),
            (Some(seed), None) => format!("seed {seed}"),
            (None, Some(detail)) => detail.clone(),
            (None, None) => "seed not recorded".to_string(),
        },
        ArtifactRead::Missing => "not on disk — this run did not produce it".to_string(),
        ArtifactRead::Invalid { path, error } => {
            format!("unreadable — {}: {error}", path.display())
        }
        ArtifactRead::Unread => "not read — no results load has happened yet".to_string(),
    }
}

/// The limits of what these measurements support.
fn limits_section(view: &ResultsView) -> MethodSection {
    let lines = vec![
        MethodLine::field(
            "libMTL pin",
            view.mtl_sha().unwrap_or_else(|| "not recorded".to_string()),
        ),
        MethodLine::field(
            "algorithm",
            view.algorithm()
                .unwrap_or_else(|| "not recorded".to_string()),
        ),
        MethodLine::note(
            "Thermal variance: the host is a laptop under a fixed governor with CPU pinning, \
             so run-to-run variance is reduced but not eliminated; small differences between \
             sizes should be read as noise, not as signal.",
        ),
        MethodLine::note(
            "One algorithm: every number here comes from one SLH-DSA parameter set through \
             libMTL. No other post-quantum signature scheme was measured, and no SIMD-accelerated \
             SLH-DSA implementation is available in this build.",
        ),
        MethodLine::note(
            "Extrapolation: values marked EXTRAPOLATED are the N × per-message identity applied \
             to a measured per-message cost, not whole-batch measurements.",
        ),
        MethodLine::note(
            "These are empirical measurements of one implementation on one machine, not a \
             general claim about MTL or about post-quantum DNSSEC.",
        ),
    ];
    MethodSection::new("Known limits", lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::test_support::{empty_derived, memory_result, signing_row, view_with};
    use climb_report::results::{ConfigComparison, PlainTimingResult};

    /// A fresh temp directory laid out like a real run, so the baseline
    /// candidate `<dir>/../docs/BASELINES.md` is under the test's control.
    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "climb-tui-method-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// Write `results` into `<root>/target/climb-results.json` and load it
    /// through the TUI's own loader, exactly as the runtime does.
    fn loaded(root: &Path, results: &climb_report::ClimbResults) -> LoadState {
        std::fs::create_dir_all(root.join("target")).expect("target dir");
        let path = root.join("target/climb-results.json");
        std::fs::write(&path, serde_json::to_vec(results).expect("serialize")).expect("write");
        super::super::load(&path)
    }

    fn sample_results() -> climb_report::ClimbResults {
        view_with(
            vec![
                signing_row("Small", "With MTL", 1_000.0),
                signing_row("Medium", "With MTL", 2_000.0),
                signing_row("Large", "With MTL", 3_000.0),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        )
        .results()
        .clone()
    }

    #[test]
    fn sections_are_always_the_same_titles_in_order() {
        let root = temp_root("order");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let titles: Vec<&str> = view
            .sections()
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles, MethodView::SECTION_TITLES);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unloaded_view_still_lists_every_section_with_an_explanation() {
        let view = MethodView::embedded();
        assert_eq!(view.sections().len(), MethodView::SECTION_TITLES.len());
        for section in view.sections() {
            assert!(
                matches!(section.lines.as_slice(), [MethodLine::Note(_)]),
                "section {:?} must explain the empty state",
                section.title
            );
        }
        assert!(
            view.seeds()
                .iter()
                .all(|row| row.read == ArtifactRead::Unread),
            "no sibling artifact is read without a load"
        );
        assert!(view.environment().is_empty());
    }

    #[test]
    fn scope_carries_the_algorithm_sizes_and_reproduce_commands() {
        let root = temp_root("scope");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let scope = &view.sections()[0];
        assert_eq!(scope.title, "Scope");
        assert!(
            scope
                .lines
                .contains(&MethodLine::code("cargo xtask reproduce")),
            "scope must carry the reproduce command: {:?}",
            scope.lines
        );
        assert!(
            scope
                .lines
                .contains(&MethodLine::code("cargo xtask report --check")),
            "scope must carry the report check command"
        );
        assert!(
            scope.lines.contains(&MethodLine::Field {
                label: "dataset sizes".to_string(),
                value: "Small, Medium, Large".to_string(),
            }),
            "scope must list the sizes the benchmark covered: {:?}",
            scope.lines
        );
        assert!(
            scope.lines.iter().any(|line| matches!(
                line,
                MethodLine::Note(text) if text.contains("no formal security guarantee")
            )),
            "scope must carry the no-security-claim note"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn evidence_class_lines_are_the_four_classes_explained() {
        let root = temp_root("classes");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let classes = &view.sections()[2];
        assert_eq!(classes.title, "Evidence classes");
        for label in ["MEASURED", "EXTRAPOLATED", "ANALYTICAL", "DERIVED"] {
            assert!(
                classes.lines.iter().any(|line| matches!(
                    line,
                    MethodLine::Field { label: l, .. } if l == label
                )),
                "evidence classes must explain {label}: {:?}",
                classes.lines
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn environment_rows_read_every_field_from_the_artifact() {
        let root = temp_root("env");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let value = |label: &str| {
            view.environment()
                .iter()
                .find(|row| row.label == label)
                .and_then(|row| row.value.clone())
        };
        assert_eq!(value("cpu").as_deref(), Some("Test CPU Model"));
        assert_eq!(value("governor").as_deref(), Some("powersave"));
        assert_eq!(value("pinning").as_deref(), Some("cpu 2"));
        assert_eq!(
            value("rustc").as_deref(),
            Some("rustc 1.98.1 (x 2026-01-01)")
        );
        assert_eq!(value("libMTL pin").as_deref(), Some("abcdef0"));
        assert_eq!(value("liboqs pin").as_deref(), Some("0123456"));
        assert_eq!(value("recorded").as_deref(), Some("2026-10-12T00:00:00Z"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pinning_prefers_the_recorded_taskset_expression() {
        let mut results = sample_results();
        results.environment.benchmark_runtime = serde_json::json!({
            "cpu_governor": "powersave",
            "pin_cpu": 2,
            "pin_expression": "taskset -c 2",
        });
        let root = temp_root("pinning-expr");
        let state = loaded(&root, &results);
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let pinning = view
            .environment()
            .iter()
            .find(|row| row.label == "pinning")
            .and_then(|row| row.value.clone());
        assert_eq!(pinning.as_deref(), Some("taskset -c 2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn absent_environment_fields_read_as_none_rather_than_fabricated() {
        let mut results = sample_results();
        results.environment.cpu = serde_json::json!({});
        results.environment.toolchain = serde_json::json!({});
        results.environment.submodules = Vec::new();
        results.environment.timestamp = String::new();
        let root = temp_root("env-absent");
        let state = loaded(&root, &results);
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let cpu = view
            .environment()
            .iter()
            .find(|r| r.label == "cpu")
            .unwrap();
        assert_eq!(cpu.value, None);
        let recorded = view
            .environment()
            .iter()
            .find(|r| r.label == "recorded")
            .unwrap();
        assert_eq!(recorded.value, None, "an empty timestamp is not recorded");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sampling_rows_carry_the_measured_sample_counts_and_classes() {
        let mut results = sample_results();
        results.memory.measured = vec![memory_result("small", 100, 851_968, 2_097_152, 3_232)];
        results.plain = vec![PlainTimingResult {
            size: "Medium".to_string(),
            messages: 10_000,
            signing_mean_ns: Some(689_910_000.0),
            signing_ci_lower_ns: None,
            signing_ci_upper_ns: None,
            verifying_mean_ns: None,
            verifying_ci_lower_ns: None,
            verifying_ci_upper_ns: None,
            class: EvidenceClass::Extrapolated,
            samples: Some(2),
        }];
        results.comparisons = vec![ConfigComparison {
            size: "Small".to_string(),
            wire_reduction_c_vs_a_percent: Some(97.0),
            wire_reduction_c_vs_b_percent: Some(96.7),
            signing_speedup_c_vs_a: Some(100.0),
            signing_speedup_c_vs_b: Some(9_800.0),
            verifying_speedup_cached_vs_a: Some(50.0),
            verifying_speedup_cold_vs_a: Some(49.0),
            class: EvidenceClass::Derived,
        }];
        let root = temp_root("sampling");
        let state = loaded(&root, &results);
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let sampling = &view.sections()[4];
        assert!(
            sampling.lines.contains(&MethodLine::Field {
                label: "memory Small".to_string(),
                value: "3 samples · isolated-process".to_string(),
            }),
            "memory row: {:?}",
            sampling.lines
        );
        assert!(
            sampling.lines.contains(&MethodLine::Field {
                label: "plain Medium".to_string(),
                value: "extrapolated (k = 2)".to_string(),
            }),
            "plain row: {:?}",
            sampling.lines
        );
        assert!(
            sampling.lines.contains(&MethodLine::Field {
                label: "comparison Small".to_string(),
                value: "derived".to_string(),
            }),
            "comparison row: {:?}",
            sampling.lines
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sampling_states_the_extrapolation_identity() {
        let root = temp_root("identity");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let sampling = &view.sections()[4];
        assert!(
            sampling.lines.contains(&MethodLine::code(
                "3 × per-message identity over the k measured sizes".to_string()
            )),
            "the identity line names k: {:?}",
            sampling.lines
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_rows_name_an_absent_artifact_instead_of_dropping_it() {
        let root = temp_root("seeds-absent");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let names: Vec<&str> = view.seeds().iter().map(|row| row.artifact).collect();
        assert_eq!(
            names,
            vec!["wire report", "memory report", "ladder verification"]
        );
        assert!(
            view.seeds()
                .iter()
                .all(|row| row.read == ArtifactRead::Missing),
            "every sibling is absent in a bare temp dir: {:?}",
            view.seeds()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_rows_read_the_committed_artifacts() {
        let root = temp_root("seeds-present");
        let state = loaded(&root, &sample_results());
        // The committed ladder fixture next to the results exercises the real
        // artifact path (§7c: the TUI consumes the artifact, not a fake).
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/ladder-verification.recorded.json");
        std::fs::copy(&fixture, root.join("target/ladder-verification.json"))
            .expect("copy ladder fixture");
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let ladder = view
            .seeds()
            .iter()
            .find(|row| row.artifact == "ladder verification")
            .expect("ladder seed row");
        assert_eq!(
            ladder.read,
            ArtifactRead::Read {
                seed: Some(4_849_331_494_946_035_253),
                detail: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
            },
            "the seed and algorithm come from the artifact, not a literal"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_present_but_unparseable_sibling_is_a_hard_error_naming_the_path() {
        let root = temp_root("seeds-invalid");
        let state = loaded(&root, &sample_results());
        let path = root.join("target/wire-report.json");
        std::fs::write(&path, "{ not json at all").expect("write garbage");
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let wire = view
            .seeds()
            .iter()
            .find(|row| row.artifact == "wire report")
            .expect("wire seed row");
        match &wire.read {
            ArtifactRead::Invalid { path: p, error } => {
                assert_eq!(p, &path, "the invalid state names the exact path");
                assert!(!error.is_empty());
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn memory_seed_uses_the_shared_artifact_parser() {
        let root = temp_root("seeds-memory-shape");
        let state = loaded(&root, &sample_results());
        // A legacy in-process delta report: the shared parser recognizes it,
        // so the Method tab reads its seed rather than declaring it unreadable.
        std::fs::write(
            root.join("target/memory-report.json"),
            r#"{"seed":1,"runs":[{"size":"Small","runs":[
                {"rss_before_bytes":1,"rss_after_bytes":3,"delta_bytes":2,"delta_mib":0.0}],
               "repeat_count":1}]}"#,
        )
        .expect("write legacy memory report");
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        let memory = view
            .seeds()
            .iter()
            .find(|row| row.artifact == "memory report")
            .expect("memory seed row");
        assert_eq!(
            memory.read,
            ArtifactRead::Read {
                seed: Some(1),
                detail: Some("legacy rss-delta".to_string()),
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_readable_baseline_copy_next_to_the_results_is_preferred_over_embedded() {
        let root = temp_root("baselines-file");
        let state = loaded(&root, &sample_results());
        // The candidate is `<results dir>/../docs/BASELINES.md`.
        std::fs::create_dir_all(root.join("docs")).expect("docs dir");
        std::fs::write(
            root.join("docs/BASELINES.md"),
            "# Test baselines\nMTL amortised\n",
        )
        .expect("write baselines");
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        assert!(
            matches!(view.baselines().source, BaselineSource::File(_)),
            "a readable copy wins: {:?}",
            view.baselines().source
        );
        assert!(view.baselines().text.contains("MTL amortised"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn without_a_readable_copy_the_embedded_baseline_document_is_used() {
        let root = temp_root("baselines-embedded");
        let state = loaded(&root, &sample_results());
        let view = MethodView::build(&root.join("target/climb-results.json"), &state);
        assert_eq!(view.baselines().source, BaselineSource::Embedded);
        assert!(
            view.baselines().text.contains("Signing baselines"),
            "the embedded document is the repository one"
        );
    }

    #[test]
    fn seed_text_formats_every_read_outcome() {
        assert_eq!(
            seed_text(&ArtifactRead::Read {
                seed: Some(7),
                detail: Some("2 size rows".to_string())
            }),
            "seed 7 · 2 size rows"
        );
        assert_eq!(
            seed_text(&ArtifactRead::Read {
                seed: Some(7),
                detail: None
            }),
            "seed 7"
        );
        assert_eq!(
            seed_text(&ArtifactRead::Read {
                seed: None,
                detail: Some("legacy comparison rows (3)".to_string())
            }),
            "legacy comparison rows (3)"
        );
        assert_eq!(
            seed_text(&ArtifactRead::Missing),
            "not on disk — this run did not produce it"
        );
        assert!(
            seed_text(&ArtifactRead::Invalid {
                path: PathBuf::from("x.json"),
                error: "bad".to_string()
            })
            .contains("x.json"),
            "an invalid read names the path"
        );
    }

    #[test]
    fn distinct_seed_rejects_disagreeing_rows() {
        assert_eq!(distinct_seed(&[5, 5, 5]), Some(5));
        assert_eq!(
            distinct_seed(&[5, 6]),
            None,
            "disagreement is never resolved by picking one"
        );
        assert_eq!(distinct_seed(&[]), None);
    }
}
