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

use std::path::Path;

use climb_report::results::EvidenceClass;

use super::{LoadState, ResultsView};

mod artifacts;

use artifacts::{baseline_text, read_seed_rows, resolve_baselines, seed_text, SEED_ARTIFACTS};
pub use artifacts::{ArtifactRead, BaselineDocument, BaselineSource, SeedRow};

/// One environment row: its label and the reader that pulls its value out of a
/// loaded results view.
type EnvironmentReader = fn(&ResultsView) -> Option<String>;

/// One labelled row of the environment block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRow {
    /// Fixed label.
    pub label: &'static str,
    /// The value from the artifact; `None` when the run did not record it.
    pub value: Option<String>,
}

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
    pub const SECTION_TITLES: [&'static str; 7] = [
        "Scope",
        "Baselines A/B/C",
        "Evidence classes",
        "Environment and pinning",
        "Sampling and extrapolation",
        "Known limits",
        "Accessibility",
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

/// The method sections for a loaded view.
fn sections_for(view: &ResultsView, seeds: &[SeedRow]) -> Vec<MethodSection> {
    vec![
        scope_section(view),
        baselines_section(),
        classes_section(),
        environment_section(view),
        sampling_section(view, seeds),
        limits_section(view),
        accessibility_section(),
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
        MethodLine::text(
            "The benchmark matrix runs three evaluated dataset sizes (Small, Medium, Large) plus one intermediate validation size used only to validate the extrapolation identity.",
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
        MethodLine::code("cargo xtask reproduce --report-only"),
        MethodLine::code("cargo xtask reproduce --remeasure"),
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
        let label = climb_report::results::size_display(&row.size);
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
            size: climb_report::results::size_display(&row.size),
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
            size: climb_report::results::size_display(&row.size),
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
            size: climb_report::results::size_display(&row.size),
            class: row.class,
        })
        .collect()
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

/// How to read the dashboard without relying on colour alone.
fn accessibility_section() -> MethodSection {
    let lines = vec![
        MethodLine::text(
            "Colour is never the only signal. Every status message carries a leading text or \
             glyph marker in addition to its colour, and the measured / extrapolated / \
             analytical legend is spelled out in words in the help overlay (press ?).",
        ),
        MethodLine::field("themes", "T cycles dark -> light -> contrast -> brand"),
        MethodLine::field("glyphs", "a toggles ASCII-only output"),
        MethodLine::field("colour", "NO_COLOR=1 forces monochrome"),
        MethodLine::note(
            "In monochrome mode the three evidence classes are distinguished by bold, dim and \
             reverse video rather than by hue, so the tables stay readable on a terminal with \
             no colour support.",
        ),
        MethodLine::note(
            "The contrast theme is tuned so every semantic colour keeps enough contrast against \
             its background to remain legible on a projector. The accessibility tests compute \
             the ratios from the palette the terminal actually resolves, not from the palette \
             definition, so a mapping that collapses two colours is caught.",
        ),
        MethodLine::note(
            "The frame is reflowed from the terminal size each draw. Below the minimum size the \
             dashboard states the size it needs and the size it was given, instead of clipping \
             a partial table.",
        ),
    ];
    MethodSection::new("Accessibility", lines)
}

#[cfg(test)]
mod tests;
