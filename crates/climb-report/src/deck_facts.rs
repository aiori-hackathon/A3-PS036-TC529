//! The deck quote sheet: every number a presentation or README may quote,
//! rendered as one Markdown page with the evidence behind each value.
//!
//! `cargo xtask deck-facts` reads the recorded artifacts — the consolidated
//! results document plus the raw artifacts behind it (wire report, memory
//! report, ladder verification, DNS capture, environment fingerprint) — and
//! collects each quotable number as a [`Fact`] carrying an evidence class, a
//! JSON pointer, and the source file it was read from. Values that appear in
//! more than one artifact are cross-checked: matching copies merge into one
//! row with both sources listed, disagreeing copies are reported as
//! [`Inconsistency`] values and fail the command. An artifact that is simply
//! absent renders an "artifact absent" note for its section instead; an
//! artifact that is present but unparseable is a hard error naming the path.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::ReportError;
use crate::ladder::LadderVerification;
use crate::results::{DnsCapture, EvidenceClass};
use crate::summary::BenchKind;

/// Sections of the quote sheet, in render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Transmitted bytes per dataset size, every configuration present.
    WireBytes,
    /// Batch and per-message signing times, with extrapolation flags.
    SigningTime,
    /// Batch and per-message verification times (trust-cached and cold).
    VerifyingTime,
    /// Measured footprint as stored, plus the analytical model behind it.
    Memory,
    /// Measured ladder shape per leaf count and the model-agreement verdict.
    LadderVerification,
    /// Response byte counts captured from the live MTL DNS pipeline.
    DnsLive,
    /// Toolchain, kernel, CPU, pinning, submodule pins and commit SHA.
    Environment,
    /// Workspace test counts from the recorded test run.
    Quality,
}

impl Section {
    /// Section render order.
    pub const ORDER: [Section; 8] = [
        Section::WireBytes,
        Section::SigningTime,
        Section::VerifyingTime,
        Section::Memory,
        Section::LadderVerification,
        Section::DnsLive,
        Section::Environment,
        Section::Quality,
    ];

    /// Heading text for the section.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Section::WireBytes => "Wire bytes",
            Section::SigningTime => "Signing time",
            Section::VerifyingTime => "Verifying time",
            Section::Memory => "Memory",
            Section::LadderVerification => "Ladder verification",
            Section::DnsLive => "DNS live",
            Section::Environment => "Environment",
            Section::Quality => "Quality",
        }
    }
}

/// The A/B/C comparison a value is quoted against, or `n/a` when the value is
/// not an A/B/C comparison (structural counts, method notes, environment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    /// Plain SLH-DSA: one full signature per message, no ladder.
    A,
    /// MTL without amortisation: a full MTL signature per message.
    B,
    /// MTL amortised: one signed ladder per batch plus condensed paths.
    C,
    /// Not an A/B/C comparison.
    Na,
}

impl Baseline {
    /// Table-cell label for the baseline.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Baseline::A => "A",
            Baseline::B => "B",
            Baseline::C => "C",
            Baseline::Na => "n/a",
        }
    }
}

/// One quotable number, with everything needed to audit it: where it lives
/// (JSON pointer + source file), how it was obtained (evidence class, `n`/`k`)
/// and what it is compared against ([`Baseline`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    /// Section the row is rendered under.
    pub section: Section,
    /// Human-readable row label.
    pub label: String,
    /// Rendered value (formatted at collection time; `≈` is added at render
    /// time for [`EvidenceClass::Extrapolated`]).
    pub value: String,
    /// Unit of the value (`B`, `%`, `×`, a time unit, or `—`).
    pub unit: String,
    /// Evidence class; `None` renders as `UNCLASSIFIED`.
    pub class: Option<EvidenceClass>,
    /// `n = …` or `k = …` sample count, or `None` (renders as `—`).
    pub nk: Option<String>,
    /// Baseline the value is quoted against.
    pub baseline: Baseline,
    /// RFC 6901 JSON Pointer into the source artifact (or `n/a` for values
    /// not read from JSON).
    pub pointer: String,
    /// Source artifact (file path, or the command that produced the value).
    pub source: String,
}

/// The repeated attributes of a row, bundled so fact construction stays
/// within reasonable argument counts.
struct Row {
    section: Section,
    label: String,
    unit: String,
    class: Option<EvidenceClass>,
    nk: Option<String>,
    baseline: Baseline,
}

fn fact(row: Row, value: String, pointer: &str, source: &str) -> Fact {
    Fact {
        section: row.section,
        label: row.label,
        value,
        unit: row.unit,
        class: row.class,
        nk: row.nk,
        baseline: row.baseline,
        pointer: pointer.to_string(),
        source: source.to_string(),
    }
}

/// An explanatory line rendered above a section's table (never a number row).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Section the note belongs to.
    pub section: Section,
    /// Note body, rendered as a blockquote line.
    pub text: String,
}

impl Note {
    /// A note recording that one artifact is absent.
    #[must_use]
    pub fn absent(section: Section, path: &Path) -> Self {
        Note {
            section,
            text: format!("artifact absent — {}", path.display()),
        }
    }

    /// A note recording that several candidate artifacts are all absent.
    #[must_use]
    pub fn absent_paths(section: Section, paths: &[&Path]) -> Self {
        let joined = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Note {
            section,
            text: format!("artifact absent — {joined}"),
        }
    }

    /// A free-form note.
    #[must_use]
    pub fn custom(section: Section, text: impl Into<String>) -> Self {
        Note {
            section,
            text: text.into(),
        }
    }
}

/// Everything one collection pass gathered: raw facts in encounter order and
/// section notes.
#[derive(Debug, Default)]
pub struct Collected {
    /// Collected facts, before duplicate-label grouping.
    pub facts: Vec<Fact>,
    /// Section notes.
    pub notes: Vec<Note>,
}

/// One disagreement between two copies of the same label, reported in the
/// generated footer and by the command's exit status.
#[derive(Debug, Clone, PartialEq)]
pub struct Inconsistency {
    /// Section both copies were collected into.
    pub section: Section,
    /// The label whose copies disagree.
    pub label: String,
    /// Which field differs and what each source said.
    pub detail: String,
}

/// Workspace test counts summed over the `test result:` summary lines of a
/// `cargo test --workspace` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestSummary {
    /// Sum of `passed` across every test target.
    pub passed: u64,
    /// Sum of `failed` across every test target.
    pub failed: u64,
    /// Sum of `ignored` across every test target.
    pub ignored: u64,
    /// Number of summary lines (test targets, including doc-tests).
    pub targets: u64,
}

/// Inputs to [`generate`]: artifact paths plus the two values collected by
/// the caller (commit SHA and test counts).
#[derive(Debug, Clone)]
pub struct DeckFactsInput {
    /// Consolidated results document.
    pub results_path: PathBuf,
    /// Raw wire-size report (`cargo xtask wire-report --out`).
    pub wire_report_path: PathBuf,
    /// Raw memory report (`cargo xtask memory-report --out`).
    pub memory_report_path: PathBuf,
    /// Live ladder-verification artifact, preferred when present.
    pub ladder_verification_path: PathBuf,
    /// Committed ladder-verification fixture, used when no live run exists.
    pub ladder_fixture_path: PathBuf,
    /// Live DNS capture, preferred when present.
    pub dns_capture_path: PathBuf,
    /// Committed DNS capture fixture, used when no live capture exists.
    pub dns_fixture_path: PathBuf,
    /// Environment fingerprint (`cargo xtask pin-env`).
    pub environment_path: PathBuf,
    /// Commit SHA from `git rev-parse HEAD` at generation time, or `None`
    /// when git produced no SHA.
    pub commit_sha: Option<String>,
    /// Workspace test counts, or `None` when no summary lines were produced.
    pub test_summary: Option<TestSummary>,
}

/// The generated page plus the grouped rows and inconsistencies behind it.
#[derive(Debug)]
pub struct DeckFactsOutput {
    /// Complete Markdown document.
    pub markdown: String,
    /// Rendered rows (duplicates merged, disagreements kept separate).
    pub rows: Vec<Fact>,
    /// Labels whose copies disagreed across artifacts.
    pub inconsistencies: Vec<Inconsistency>,
}

// ---------------------------------------------------------------------------
// Artifact loading
// ---------------------------------------------------------------------------

/// Read a JSON document; `Ok(None)` when the file does not exist. A file that
/// exists but does not parse is a hard error naming the path.
fn load_json(path: &Path) -> Result<Option<Value>, ReportError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ReportError::Io {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };
    let value: Value = serde_json::from_str(&text).map_err(|e| ReportError::JsonDecode {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(Some(value))
}

/// Load the ladder-verification document, preferring a live run over the
/// committed fixture; `None` when neither exists. A present-but-unparseable
/// artifact is a hard error naming the path.
fn load_ladder(
    live: &Path,
    fixture: &Path,
) -> Result<Option<(PathBuf, LadderVerification)>, ReportError> {
    let path = if live.exists() {
        live
    } else if fixture.exists() {
        fixture
    } else {
        return Ok(None);
    };
    let text = fs::read_to_string(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let document: LadderVerification =
        serde_json::from_str(&text).map_err(|e| ReportError::JsonDecode {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(Some((path.to_path_buf(), document)))
}

/// Load the DNS capture, preferring a live recording over the committed
/// fixture; `None` when neither exists. A present-but-unparseable artifact is
/// a hard error naming the path.
fn load_dns(live: &Path, fixture: &Path) -> Result<Option<(PathBuf, DnsCapture)>, ReportError> {
    let path = if live.exists() {
        live
    } else if fixture.exists() {
        fixture
    } else {
        return Ok(None);
    };
    let text = fs::read_to_string(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let capture: DnsCapture = serde_json::from_str(&text).map_err(|e| ReportError::JsonDecode {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(Some((path.to_path_buf(), capture)))
}

// ---------------------------------------------------------------------------
// Field extraction and formatting
// ---------------------------------------------------------------------------

/// Read an unsigned integer at `pointer`; missing (or null) fields are `None`
/// so older schemas stay readable, and a present-but-wrong-typed field is a
/// hard error naming the artifact.
fn opt_u64(root: &Value, pointer: &str, path: &Path) -> Result<Option<u64>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| wrong_type(path, pointer, "an unsigned integer", v)),
    }
}

/// Read a signed integer at `pointer` (memory residuals can be negative).
fn opt_i64(root: &Value, pointer: &str, path: &Path) -> Result<Option<i64>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| wrong_type(path, pointer, "an integer", v)),
    }
}

/// Read a floating-point number at `pointer`.
fn opt_f64(root: &Value, pointer: &str, path: &Path) -> Result<Option<f64>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .map(Some)
            .ok_or_else(|| wrong_type(path, pointer, "a number", v)),
    }
}

/// Read a string at `pointer`.
fn opt_str(root: &Value, pointer: &str, path: &Path) -> Result<Option<String>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(str::to_string)
            .map(Some)
            .ok_or_else(|| wrong_type(path, pointer, "a string", v)),
    }
}

/// Read a boolean at `pointer`.
fn opt_bool(root: &Value, pointer: &str, path: &Path) -> Result<Option<bool>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| wrong_type(path, pointer, "a boolean", v)),
    }
}

fn wrong_type(path: &Path, pointer: &str, expected: &str, found: &Value) -> ReportError {
    ReportError::MalformedData {
        path: path.to_path_buf(),
        reason: format!("expected {expected} at {pointer}, found {found}"),
    }
}

/// Parse an artifact-recorded evidence class; an unrecognized class string
/// (or a missing field) yields `None`, which renders as `UNCLASSIFIED`.
fn opt_class(
    root: &Value,
    pointer: &str,
    path: &Path,
) -> Result<Option<EvidenceClass>, ReportError> {
    match opt_str(root, pointer, path)? {
        None => Ok(None),
        Some(s) => Ok(match s.as_str() {
            "measured" => Some(EvidenceClass::Measured),
            "extrapolated" => Some(EvidenceClass::Extrapolated),
            "analytical" => Some(EvidenceClass::Analytical),
            "derived" => Some(EvidenceClass::Derived),
            _ => None,
        }),
    }
}

/// Byte counts render exactly as stored.
fn fmt_bytes(v: u64) -> String {
    v.to_string()
}

/// Percentages render with three decimals.
fn fmt_percent(v: f64) -> String {
    format!("{v:.3}")
}

/// Ratios render coarsely for large values and precisely for small ones,
/// mirroring the report tables' speed-up formatting.
fn fmt_ratio(v: f64) -> String {
    if v >= 1000.0 {
        format!("{v:.0}")
    } else if v >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

/// Format a duration and split it into its numeric value and unit, so the
/// value and unit land in separate table columns.
fn fmt_duration(ns: f64) -> (String, String) {
    let rendered = crate::render::format_duration(ns);
    match rendered.rsplit_once(' ') {
        Some((value, unit)) => (value.to_string(), unit.to_string()),
        None => (rendered, String::new()),
    }
}

/// Capitalize a size key from an artifact (`small` → `Small`) so labels built
/// from differently-cased copies of the same size group together.
fn size_display(size: &str) -> String {
    let mut chars = size.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Baseline gloss for one benchmark condition; `None` for a condition the
/// mapping does not know (the collector then quotes it verbatim against
/// `n/a` rather than dropping the row).
fn condition_gloss(kind: BenchKind, condition: &str) -> Option<(&'static str, Baseline)> {
    match kind {
        BenchKind::Signing => match condition {
            "With MTL" => Some(("C (MTL amortised)", Baseline::C)),
            "Without MTL" => Some(("B (MTL, no amortisation)", Baseline::B)),
            _ => None,
        },
        BenchKind::Verifying => match condition {
            "MTL (trust cached)" => Some(("C (trust cached ladder)", Baseline::C)),
            "With MTL" => Some(("C (ladder re-validated)", Baseline::C)),
            "Without MTL" => Some(("B (MTL, no amortisation)", Baseline::B)),
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------
// Collectors
// ---------------------------------------------------------------------------

/// Read the array at `pointer`; a present-but-non-array section is a hard
/// error naming the artifact, an absent (or null) one is `None`.
fn array_at<'a>(
    root: &'a Value,
    path: &Path,
    pointer: &str,
) -> Result<Option<&'a Vec<Value>>, ReportError> {
    match root.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(rows)) => Ok(Some(rows)),
        Some(v) => Err(wrong_type(path, pointer, "an array", v)),
    }
}

/// Pointers for one wire-size row. The raw report nests the condensed and
/// plain statistics (`/condensed/min`, `/plain/total`) while the consolidated
/// results flatten them (`/condensed_min`, `/plain_total_bytes`), so each
/// source supplies its own suffixes relative to the row base.
struct WirePointers {
    total_a: String,
    total_b: String,
    total_c: String,
    plain_sig_len: String,
    ladder: String,
    cond_min: String,
    cond_mean: String,
    cond_max: String,
    reduction: String,
}

/// Push every wire-byte fact for one row of either wire-size artifact.
fn wire_rows(
    out: &mut Collected,
    root: &Value,
    path: &Path,
    base: &str,
    size: &str,
    nk: Option<String>,
    p: &WirePointers,
) -> Result<(), ReportError> {
    let source = path.display().to_string();
    let byte_specs: [(&str, &str, Baseline); 7] = [
        (
            "wire total, A (plain SLH-DSA)",
            p.total_a.as_str(),
            Baseline::A,
        ),
        (
            "wire total, B (MTL, no amortisation)",
            p.total_b.as_str(),
            Baseline::B,
        ),
        (
            "wire total, C (MTL amortised)",
            p.total_c.as_str(),
            Baseline::C,
        ),
        (
            "plain SLH-DSA signature length",
            p.plain_sig_len.as_str(),
            Baseline::A,
        ),
        ("ladder bytes", p.ladder.as_str(), Baseline::C),
        ("condensed signature, min", p.cond_min.as_str(), Baseline::C),
        ("condensed signature, max", p.cond_max.as_str(), Baseline::C),
    ];
    for (label, suffix, baseline) in byte_specs {
        if let Some(v) = opt_u64(root, suffix, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::WireBytes,
                    label: format!("{size} — {label}"),
                    unit: "B".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: nk.clone(),
                    baseline,
                },
                fmt_bytes(v),
                &format!("{base}{suffix}"),
                &source,
            ));
        }
    }

    if let Some(v) = opt_f64(root, p.cond_mean.as_str(), path)? {
        out.facts.push(fact(
            Row {
                section: Section::WireBytes,
                label: format!("{size} — condensed signature, mean"),
                unit: "B".to_string(),
                class: Some(EvidenceClass::Measured),
                nk: nk.clone(),
                baseline: Baseline::C,
            },
            format!("{v:.2}"),
            &format!("{base}{}", p.cond_mean),
            &source,
        ));
    }

    // The reduction is byte arithmetic on measured counts, so it is DERIVED
    // regardless of what class any comparison row records for it.
    if let Some(v) = opt_f64(root, p.reduction.as_str(), path)? {
        out.facts.push(fact(
            Row {
                section: Section::WireBytes,
                label: format!("{size} — wire reduction, C vs B"),
                unit: "%".to_string(),
                class: Some(EvidenceClass::Derived),
                nk: nk.clone(),
                baseline: Baseline::B,
            },
            fmt_percent(v),
            &format!("{base}{}", p.reduction),
            &source,
        ));
    }
    Ok(())
}

/// Collect the wire-byte section from the raw report and the results copy.
fn collect_wire(
    out: &mut Collected,
    raw: Option<(&Path, &Value)>,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    let mut sizes_seen: Vec<String> = Vec::new();

    if let Some((path, root)) = raw {
        // The raw wire report's root is the array itself.
        let rows = root.as_array().ok_or_else(|| ReportError::MalformedData {
            path: path.to_path_buf(),
            reason: "wire report root is not a JSON array".to_string(),
        })?;
        for (i, row) in rows.iter().enumerate() {
            let base = format!("/{i}");
            let Some(size_raw) = opt_str(row, "/size", path)? else {
                continue;
            };
            let size = size_display(&size_raw);
            sizes_seen.push(size.to_lowercase());
            let message_count = opt_u64(row, "/message_count", path)?.unwrap_or(0);
            let nk = (message_count > 0).then(|| format!("n = {message_count}"));
            let pointers = WirePointers {
                total_a: "/plain/total".to_string(),
                total_b: "/without_mtl_total".to_string(),
                total_c: "/with_mtl_total".to_string(),
                plain_sig_len: "/plain/signature_len".to_string(),
                ladder: "/ladder_len".to_string(),
                cond_min: "/condensed/min".to_string(),
                cond_mean: "/condensed/mean".to_string(),
                cond_max: "/condensed/max".to_string(),
                reduction: "/reduction_percent".to_string(),
            };
            wire_rows(out, row, path, &base, &size, nk, &pointers)?;
        }
    }

    if let Some((path, root)) = results {
        let rows = array_at(root, path, "/wire_size")?;
        if let Some(rows) = rows {
            for (i, row) in rows.iter().enumerate() {
                let base = format!("/wire_size/{i}");
                let Some(size_raw) = opt_str(row, "/size", path)? else {
                    continue;
                };
                let size = size_display(&size_raw);
                sizes_seen.push(size.to_lowercase());
                let message_count = opt_u64(row, "/message_count", path)?.unwrap_or(0);
                let nk = (message_count > 0).then(|| format!("n = {message_count}"));
                let pointers = WirePointers {
                    total_a: "/plain_total_bytes".to_string(),
                    total_b: "/without_mtl_total_bytes".to_string(),
                    total_c: "/with_mtl_total_bytes".to_string(),
                    plain_sig_len: "/plain_signature_len_bytes".to_string(),
                    ladder: "/ladder_len".to_string(),
                    cond_min: "/condensed_min".to_string(),
                    cond_mean: "/condensed_mean".to_string(),
                    cond_max: "/condensed_max".to_string(),
                    reduction: "/reduction_percent".to_string(),
                };
                wire_rows(out, row, path, &base, &size, nk, &pointers)?;
            }
        }

        // Sizes with a plain-timing row but no wire row are a coverage gap the
        // deck must not paper over.
        let missing: Vec<String> = array_at(root, path, "/plain")?
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|row| {
                let size = row.get("size")?.as_str()?;
                (!sizes_seen.iter().any(|s| s == &size.to_lowercase())).then(|| size_display(size))
            })
            .collect();
        if !missing.is_empty() {
            out.notes.push(Note::custom(
                Section::WireBytes,
                format!("no wire rows recorded for: {}", missing.join(", ")),
            ));
        }
    }

    Ok(())
}

/// Collect one benchmark kind (signing or verifying): batch and per-message
/// rows for the MTL conditions, plus the configuration-A plain rows.
fn collect_bench(
    out: &mut Collected,
    results: Option<(&Path, &Value)>,
    kind: BenchKind,
) -> Result<(), ReportError> {
    let Some((path, root)) = results else {
        return Ok(());
    };
    let source = path.display().to_string();
    let (section, kind_str) = match kind {
        BenchKind::Signing => (Section::SigningTime, "signing"),
        BenchKind::Verifying => (Section::VerifyingTime, "verifying"),
    };

    if let Some(rows) = array_at(root, path, &format!("/benchmark/{kind_str}"))? {
        for (i, row) in rows.iter().enumerate() {
            let base = format!("/benchmark/{kind_str}/{i}");
            let Some(size_raw) = opt_str(row, "/size", path)? else {
                continue;
            };
            let size = size_display(&size_raw);
            let Some(condition) = opt_str(row, "/condition", path)? else {
                continue;
            };
            let messages = opt_u64(row, "/messages", path)?.unwrap_or(0);
            let nk = (messages > 0).then(|| format!("n = {messages}"));
            let (gloss, baseline) = match condition_gloss(kind, &condition) {
                Some((gloss, baseline)) => (gloss.to_string(), baseline),
                None => (condition.clone(), Baseline::Na),
            };
            let pointer = format!("{base}/mean_ns");
            let Some(mean_ns) = opt_f64(row, "/mean_ns", path)? else {
                continue;
            };

            let (value, unit) = fmt_duration(mean_ns);
            out.facts.push(fact(
                Row {
                    section,
                    label: format!("{size} — {kind_str} per batch, {gloss}"),
                    unit,
                    class: Some(EvidenceClass::Measured),
                    nk: nk.clone(),
                    baseline,
                },
                value,
                &pointer,
                &source,
            ));

            if messages > 0 {
                let (value, unit) = fmt_duration(mean_ns / messages as f64);
                out.facts.push(fact(
                    Row {
                        section,
                        label: format!("{size} — {kind_str} per message, {gloss}"),
                        unit,
                        class: Some(EvidenceClass::Derived),
                        nk: nk.clone(),
                        baseline,
                    },
                    value,
                    &pointer,
                    &source,
                ));
            }
        }
    }

    // Configuration A: a plain row is a per-message cost; its batch figure is
    // the per-message identity, never a measurement over the batch.
    let plain_field = match kind {
        BenchKind::Signing => "signing_mean_ns",
        BenchKind::Verifying => "verifying_mean_ns",
    };
    if let Some(rows) = array_at(root, path, "/plain")? {
        for (i, row) in rows.iter().enumerate() {
            let base = format!("/plain/{i}");
            let Some(size_raw) = opt_str(row, "/size", path)? else {
                continue;
            };
            let size = size_display(&size_raw);
            let pointer = format!("{base}/{plain_field}");
            let Some(mean_ns) = opt_f64(row, &format!("/{plain_field}"), path)? else {
                continue;
            };
            let messages = opt_u64(row, "/messages", path)?.unwrap_or(0);
            let class = opt_class(row, "/class", path)?;
            let samples = opt_u64(row, "/samples", path)?;
            let nk = Some(match samples {
                Some(k) => format!("k = {k}"),
                None => "n = 1".to_string(),
            });

            let (value, unit) = fmt_duration(mean_ns);
            out.facts.push(fact(
                Row {
                    section,
                    label: format!("{size} — {kind_str} per message, A (plain SLH-DSA)"),
                    unit,
                    class,
                    nk: nk.clone(),
                    baseline: Baseline::A,
                },
                value,
                &pointer,
                &source,
            ));

            if messages > 0 {
                let (value, unit) = fmt_duration(mean_ns * messages as f64);
                let k = samples.unwrap_or(1);
                out.facts.push(fact(
                    Row {
                        section,
                        label: format!("{size} — {kind_str} per batch, A (plain SLH-DSA)"),
                        unit,
                        class: Some(EvidenceClass::Extrapolated),
                        nk: Some(format!("k = {k}")),
                        baseline: Baseline::A,
                    },
                    value,
                    &pointer,
                    &source,
                ));
            }
        }
    }

    Ok(())
}

/// Message count of the plain row for `size`, when one exists.
fn plain_messages(root: &Value, size: &str) -> Option<u64> {
    let rows = root.pointer("/plain")?.as_array()?;
    rows.iter().find_map(|row| {
        let row_size = row.get("size")?.as_str()?;
        if !row_size.eq_ignore_ascii_case(size) {
            return None;
        }
        row.get("messages")?.as_u64()
    })
}

/// Collect the three-way comparison rows: wire reductions (byte arithmetic,
/// always DERIVED) and speed-ups (whose class comes from the artifact row).
fn collect_comparisons(
    out: &mut Collected,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    let Some((path, root)) = results else {
        return Ok(());
    };
    let source = path.display().to_string();
    let Some(rows) = array_at(root, path, "/comparisons")? else {
        return Ok(());
    };
    for (i, row) in rows.iter().enumerate() {
        let base = format!("/comparisons/{i}");
        let Some(size_raw) = opt_str(row, "/size", path)? else {
            continue;
        };
        let size = size_display(&size_raw);
        let nk = plain_messages(root, &size).map(|m| format!("n = {m}"));
        let class = opt_class(row, "/class", path)?;

        let numeric_specs: [(&str, &str, Section, EvidenceClass, Baseline); 2] = [
            (
                "wire_reduction_c_vs_a_percent",
                "wire reduction, C vs A",
                Section::WireBytes,
                EvidenceClass::Derived,
                Baseline::A,
            ),
            (
                "wire_reduction_c_vs_b_percent",
                "wire reduction, C vs B",
                Section::WireBytes,
                EvidenceClass::Derived,
                Baseline::B,
            ),
        ];
        for (field, label, section, class, baseline) in numeric_specs {
            if let Some(v) = opt_f64(row, &format!("/{field}"), path)? {
                out.facts.push(fact(
                    Row {
                        section,
                        label: format!("{size} — {label}"),
                        unit: "%".to_string(),
                        class: Some(class),
                        nk: nk.clone(),
                        baseline,
                    },
                    fmt_percent(v),
                    &format!("{base}/{field}"),
                    &source,
                ));
            }
        }

        let ratio_specs: [(&str, &str, Section, Baseline); 4] = [
            (
                "signing_speedup_c_vs_a",
                "signing speed-up, C vs A",
                Section::SigningTime,
                Baseline::A,
            ),
            (
                "signing_speedup_c_vs_b",
                "signing speed-up, C vs B",
                Section::SigningTime,
                Baseline::B,
            ),
            (
                "verifying_speedup_cached_vs_a",
                "verifying speed-up (trust cached), C vs A",
                Section::VerifyingTime,
                Baseline::A,
            ),
            (
                "verifying_speedup_cold_vs_a",
                "verifying speed-up (ladder re-validated), C vs A",
                Section::VerifyingTime,
                Baseline::A,
            ),
        ];
        for (field, label, section, baseline) in ratio_specs {
            if let Some(v) = opt_f64(row, &format!("/{field}"), path)? {
                out.facts.push(fact(
                    Row {
                        section,
                        label: format!("{size} — {label}"),
                        unit: "×".to_string(),
                        class,
                        nk: nk.clone(),
                        baseline,
                    },
                    fmt_ratio(v),
                    &format!("{base}/{field}"),
                    &source,
                ));
            }
        }
    }
    Ok(())
}

/// Collect the derived summary rows: each is computed across the rows it
/// summarizes, so its `n` is the number of contributing rows.
fn collect_summaries(
    out: &mut Collected,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    let Some((path, root)) = results else {
        return Ok(());
    };
    let source = path.display().to_string();
    let wire_rows = array_at(root, path, "/wire_size")?
        .map(Vec::len)
        .unwrap_or(0);
    let wire_nk = (wire_rows > 0).then(|| format!("n = {wire_rows}"));

    let wire_specs = [
        (
            "/derived/overall_wire_reduction_percent",
            "Overall wire reduction (mean across sizes)",
        ),
        (
            "/derived/largest_wire_reduction_percent",
            "Largest wire reduction (across sizes)",
        ),
    ];
    for (pointer, label) in wire_specs {
        if let Some(v) = opt_f64(root, pointer, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::WireBytes,
                    label: label.to_string(),
                    unit: "%".to_string(),
                    class: Some(EvidenceClass::Derived),
                    nk: wire_nk.clone(),
                    baseline: Baseline::B,
                },
                fmt_percent(v),
                pointer,
                &source,
            ));
        }
    }

    let cached_rows = array_at(root, path, "/benchmark/verifying")?
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|row| {
            row.get("condition")
                .and_then(Value::as_str)
                .is_some_and(|c| c.contains("trust cached"))
        })
        .count();
    let cached_nk = (cached_rows > 0).then(|| format!("n = {cached_rows}"));
    if let Some(v) = opt_f64(
        root,
        "/derived/overall_verifying_trust_cached_speedup_avg",
        path,
    )? {
        out.facts.push(fact(
            Row {
                section: Section::VerifyingTime,
                label: "Average trust-cached verify speed-up".to_string(),
                unit: "×".to_string(),
                class: Some(EvidenceClass::Derived),
                nk: cached_nk,
                baseline: Baseline::Na,
            },
            fmt_ratio(v),
            "/derived/overall_verifying_trust_cached_speedup_avg",
            &source,
        ));
    }
    Ok(())
}

/// Record a measurement method for the section note, without duplicates.
fn record_method(methods: &mut Vec<String>, method: String) {
    if !methods.contains(&method) {
        methods.push(method);
    }
}

/// Collect the memory section: raw report rows (medians, ranges) that the
/// results copy repeats, plus the results-only peak, sample count, itemised
/// analytical model and residual.
fn collect_memory(
    out: &mut Collected,
    raw: Option<(&Path, &Value)>,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    let mut methods: Vec<String> = Vec::new();

    if let Some((path, root)) = raw {
        if let Some(m) = opt_str(root, "/method", path)? {
            record_method(&mut methods, m);
        }
        if let Some(rows) = array_at(root, path, "/runs")? {
            for (i, row) in rows.iter().enumerate() {
                let base = format!("/runs/{i}");
                let Some(size_raw) = opt_str(row, "/size", path)? else {
                    continue;
                };
                let size = size_display(&size_raw);
                if let Some(m) = opt_str(row, "/method", path)? {
                    record_method(&mut methods, m);
                }
                let samples = array_at(row, path, "/samples")?.map(Vec::len).unwrap_or(0);
                let nk = (samples > 0).then(|| format!("n = {samples}"));
                let measured_specs = [
                    ("/median_bytes", "resident median (measured)"),
                    ("/min_bytes", "resident min (measured)"),
                    ("/max_bytes", "resident max (measured)"),
                    ("/iqr_bytes", "resident IQR (measured)"),
                ];
                for (suffix, label) in measured_specs {
                    if let Some(v) = opt_u64(row, suffix, path)? {
                        out.facts.push(fact(
                            Row {
                                section: Section::Memory,
                                label: format!("{size} — {label}"),
                                unit: "B".to_string(),
                                class: Some(EvidenceClass::Measured),
                                nk: nk.clone(),
                                baseline: Baseline::Na,
                            },
                            fmt_bytes(v),
                            &format!("{base}{suffix}"),
                            &path.display().to_string(),
                        ));
                    }
                }
            }
        }
    }

    if let Some((path, root)) = results {
        let source = path.display().to_string();
        if let Some(rows) = array_at(root, path, "/memory/measured")? {
            for (i, row) in rows.iter().enumerate() {
                let base = format!("/memory/measured/{i}");
                let Some(size_raw) = opt_str(row, "/size", path)? else {
                    continue;
                };
                let size = size_display(&size_raw);
                if let Some(m) = opt_str(row, "/method", path)? {
                    record_method(&mut methods, m);
                }
                let samples = opt_u64(row, "/samples", path)?.unwrap_or(0);
                let nk = (samples > 0).then(|| format!("n = {samples}"));

                // Same labels as the raw-report copies so duplicates merge.
                let measured_specs = [
                    ("/median_bytes", "resident median (measured)"),
                    ("/measured_range_min_bytes", "resident min (measured)"),
                    ("/measured_range_max_bytes", "resident max (measured)"),
                    ("/iqr_bytes", "resident IQR (measured)"),
                ];
                for (suffix, label) in measured_specs {
                    if let Some(v) = opt_u64(row, suffix, path)? {
                        out.facts.push(fact(
                            Row {
                                section: Section::Memory,
                                label: format!("{size} — {label}"),
                                unit: "B".to_string(),
                                class: Some(EvidenceClass::Measured),
                                nk: nk.clone(),
                                baseline: Baseline::Na,
                            },
                            fmt_bytes(v),
                            &format!("{base}{suffix}"),
                            &source,
                        ));
                    }
                }

                if let Some(v) = opt_u64(row, "/measured_peak_bytes", path)? {
                    out.facts.push(fact(
                        Row {
                            section: Section::Memory,
                            label: format!("{size} — peak median (measured)"),
                            unit: "B".to_string(),
                            class: Some(EvidenceClass::Measured),
                            nk: nk.clone(),
                            baseline: Baseline::Na,
                        },
                        fmt_bytes(v),
                        &format!("{base}/measured_peak_bytes"),
                        &source,
                    ));
                }

                if samples > 0 {
                    out.facts.push(fact(
                        Row {
                            section: Section::Memory,
                            label: format!("{size} — measurement samples"),
                            unit: "—".to_string(),
                            class: Some(EvidenceClass::Measured),
                            nk: Some(format!("n = {samples}")),
                            baseline: Baseline::Na,
                        },
                        samples.to_string(),
                        &format!("{base}/samples"),
                        &source,
                    ));
                }

                let analytical_specs = [
                    ("/libmtl_analytical_bytes", "analytical, libMTL pages"),
                    ("/harness_buffers_bytes", "analytical, harness buffers"),
                    ("/dataset_buffers_bytes", "analytical, dataset buffers"),
                    ("/keypair_bytes", "analytical, keypair"),
                    ("/analytical_total_bytes", "analytical total"),
                ];
                for (suffix, label) in analytical_specs {
                    if let Some(v) = opt_u64(row, suffix, path)? {
                        out.facts.push(fact(
                            Row {
                                section: Section::Memory,
                                label: format!("{size} — {label}"),
                                unit: "B".to_string(),
                                class: Some(EvidenceClass::Analytical),
                                nk: None,
                                baseline: Baseline::Na,
                            },
                            fmt_bytes(v),
                            &format!("{base}{suffix}"),
                            &source,
                        ));
                    }
                }

                if let Some(v) = opt_i64(row, "/residual_bytes", path)? {
                    out.facts.push(fact(
                        Row {
                            section: Section::Memory,
                            label: format!("{size} — residual (measured − analytical)"),
                            unit: "B".to_string(),
                            class: Some(EvidenceClass::Derived),
                            nk: nk.clone(),
                            baseline: Baseline::Na,
                        },
                        v.to_string(),
                        &format!("{base}/residual_bytes"),
                        &source,
                    ));
                }

                if let Some(v) = opt_f64(row, "/measured_analytical_total_ratio", path)? {
                    out.facts.push(fact(
                        Row {
                            section: Section::Memory,
                            label: format!("{size} — measured / analytical ratio"),
                            unit: "×".to_string(),
                            class: Some(EvidenceClass::Derived),
                            nk: nk.clone(),
                            baseline: Baseline::Na,
                        },
                        fmt_ratio(v),
                        &format!("{base}/measured_analytical_total_ratio"),
                        &source,
                    ));
                }
            }
        }

        let model_specs = [
            (
                "/memory/analytical_model/tree_page_size_bytes",
                "Analytical model — tree page size",
                "B",
            ),
            (
                "/memory/analytical_model/tree_max_pages",
                "Analytical model — tree max pages",
                "—",
            ),
            (
                "/memory/analytical_model/randomizer_pages",
                "Analytical model — randomizer pages",
                "—",
            ),
            (
                "/memory/analytical_model/hash_size_bytes",
                "Analytical model — hash size",
                "B",
            ),
        ];
        for (pointer, label, unit) in model_specs {
            if let Some(v) = opt_u64(root, pointer, path)? {
                out.facts.push(fact(
                    Row {
                        section: Section::Memory,
                        label: label.to_string(),
                        unit: unit.to_string(),
                        class: Some(EvidenceClass::Analytical),
                        nk: None,
                        baseline: Baseline::Na,
                    },
                    v.to_string(),
                    pointer,
                    &source,
                ));
            }
        }
    }

    if !methods.is_empty() {
        out.notes.push(Note::custom(
            Section::Memory,
            format!("measurement method: {}", methods.join(", ")),
        ));
    }
    Ok(())
}

/// Collect the ladder-verification section from the recorded document and the
/// results copy, plus the results-only model-agreement verdicts.
fn collect_ladder(
    out: &mut Collected,
    doc: Option<(&Path, &LadderVerification)>,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    if let Some((path, ladder)) = doc {
        let source = path.display().to_string();
        for (i, row) in ladder.sizes.iter().enumerate() {
            let base = format!("/sizes/{i}");
            let nk = format!("n = {}", row.leaf_count);
            let value_specs: [(u64, &str, &str, &str); 5] = [
                (row.rung_count as u64, "rung count", "—", "rung_count"),
                (
                    row.ladder_buffer_len as u64,
                    "ladder buffer bytes",
                    "B",
                    "ladder_buffer_len",
                ),
                (
                    row.signed_ladder_len as u64,
                    "signed ladder bytes",
                    "B",
                    "signed_ladder_len",
                ),
                (
                    row.condensed.min as u64,
                    "condensed min",
                    "B",
                    "condensed/min",
                ),
                (
                    row.condensed.max as u64,
                    "condensed max",
                    "B",
                    "condensed/max",
                ),
            ];
            for (value, label, unit, suffix) in value_specs {
                let pointer = format!("{base}/{suffix}");
                out.facts.push(fact(
                    Row {
                        section: Section::LadderVerification,
                        label: format!("Ladder, N={} — {label}", row.leaf_count),
                        unit: unit.to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: Some(nk.clone()),
                        baseline: Baseline::Na,
                    },
                    value.to_string(),
                    &pointer,
                    &source,
                ));
            }
            let pointer = format!("{base}/condensed/mean");
            out.facts.push(fact(
                Row {
                    section: Section::LadderVerification,
                    label: format!("Ladder, N={} — condensed mean", row.leaf_count),
                    unit: "B".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: Some(nk.clone()),
                    baseline: Baseline::Na,
                },
                format!("{:.2}", row.condensed.mean),
                &pointer,
                &source,
            ));
        }
    }

    if let Some((path, root)) = results {
        let source = path.display().to_string();
        let base = "/ladder/sizes";
        if let Some(rows) = array_at(root, path, base)? {
            for (i, row) in rows.iter().enumerate() {
                let row_base = format!("{base}/{i}");
                let leaf = opt_u64(row, "/leaf_count", path)?.unwrap_or(0);
                let nk = format!("n = {leaf}");
                let value_specs = [
                    ("/rung_count", "rung count", "—"),
                    ("/ladder_buffer_len", "ladder buffer bytes", "B"),
                    ("/signed_ladder_len", "signed ladder bytes", "B"),
                    ("/condensed_min", "condensed min", "B"),
                    ("/condensed_max", "condensed max", "B"),
                ];
                for (suffix, label, unit) in value_specs {
                    if let Some(v) = opt_u64(row, suffix, path)? {
                        out.facts.push(fact(
                            Row {
                                section: Section::LadderVerification,
                                label: format!("Ladder, N={leaf} — {label}"),
                                unit: unit.to_string(),
                                class: Some(EvidenceClass::Measured),
                                nk: Some(nk.clone()),
                                baseline: Baseline::Na,
                            },
                            v.to_string(),
                            &format!("{row_base}{suffix}"),
                            &source,
                        ));
                    }
                }
                if let Some(v) = opt_f64(row, "/condensed_mean", path)? {
                    out.facts.push(fact(
                        Row {
                            section: Section::LadderVerification,
                            label: format!("Ladder, N={leaf} — condensed mean"),
                            unit: "B".to_string(),
                            class: Some(EvidenceClass::Measured),
                            nk: Some(nk.clone()),
                            baseline: Baseline::Na,
                        },
                        format!("{v:.2}"),
                        &format!("{row_base}/condensed_mean"),
                        &source,
                    ));
                }
                if let Some(v) = opt_bool(row, "/model_matches", path)? {
                    out.facts.push(fact(
                        Row {
                            section: Section::LadderVerification,
                            label: format!("Ladder, N={leaf} — matches the closed-form model"),
                            unit: "—".to_string(),
                            class: Some(EvidenceClass::Derived),
                            nk: Some(nk.clone()),
                            baseline: Baseline::Na,
                        },
                        if v { "yes" } else { "no" }.to_string(),
                        &format!("{row_base}/model_matches"),
                        &source,
                    ));
                }
            }
        }

        let all_pointer = "/ladder/all_match";
        if let Some(v) = opt_bool(root, all_pointer, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::LadderVerification,
                    label: "Ladder — model matches every measured size".to_string(),
                    unit: "—".to_string(),
                    class: Some(EvidenceClass::Derived),
                    nk: None,
                    baseline: Baseline::Na,
                },
                if v { "yes" } else { "no" }.to_string(),
                all_pointer,
                &source,
            ));
        }
        for (pointer, label) in [
            ("/ladder/algorithm", "Ladder — algorithm"),
            ("/ladder/recorded_at", "Ladder — recorded at"),
        ] {
            if let Some(v) = opt_str(root, pointer, path)? {
                out.facts.push(fact(
                    Row {
                        section: Section::LadderVerification,
                        label: label.to_string(),
                        unit: "—".to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: None,
                        baseline: Baseline::Na,
                    },
                    v,
                    pointer,
                    &source,
                ));
            }
        }
    }

    Ok(())
}

/// Baseline a DNS capture pass is quoted against: a condensed (amortised) proof
/// replaces the full signature it was captured beside, while the full and
/// unbound passes both carry a full MTL signature.
fn dns_baseline(label: &str) -> Baseline {
    match label {
        "condensed" => Baseline::C,
        "full" | "unbound" => Baseline::B,
        _ => Baseline::Na,
    }
}

/// Collect the DNS-live section: per-query response and answer-RRSIG bytes from
/// the capture, the consolidated headline (whose two byte counts duplicate the
/// `www.climb.example. A` capture rows and therefore cross-check them), and the
/// capture provenance the headline repeats.
fn collect_dns(
    out: &mut Collected,
    capture: Option<(&Path, &DnsCapture)>,
    results: Option<(&Path, &Value)>,
) -> Result<(), ReportError> {
    let present = capture.is_some() || results.is_some();

    if let Some((path, capture)) = capture {
        let source = path.display().to_string();
        for (i, query) in capture.queries.iter().enumerate() {
            let base = format!("/queries/{i}");
            let baseline = dns_baseline(&query.label);
            out.facts.push(fact(
                Row {
                    section: Section::DnsLive,
                    label: format!(
                        "DNS, {} {} [{}] — response bytes",
                        query.qname, query.qtype, query.label
                    ),
                    unit: "B".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: None,
                    baseline,
                },
                fmt_bytes(query.response_bytes),
                &format!("{base}/response_bytes"),
                &source,
            ));
            if let Some(rrsig) = query.answer_rrsig_bytes {
                out.facts.push(fact(
                    Row {
                        section: Section::DnsLive,
                        label: format!(
                            "DNS, {} {} [{}] — answer RRSIG bytes",
                            query.qname, query.qtype, query.label
                        ),
                        unit: "B".to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: None,
                        baseline,
                    },
                    fmt_bytes(rrsig),
                    &format!("{base}/answer_rrsig_bytes"),
                    &source,
                ));
            }
        }
        for (pointer, label, value) in [
            ("/algorithm", "DNS — algorithm", capture.algorithm.clone()),
            (
                "/recorded_at",
                "DNS — recorded at",
                capture.recorded_at.clone(),
            ),
        ] {
            out.facts.push(fact(
                Row {
                    section: Section::DnsLive,
                    label: label.to_string(),
                    unit: "—".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: None,
                    baseline: Baseline::Na,
                },
                value,
                pointer,
                &source,
            ));
        }
    }

    if let Some((path, root)) = results {
        let source = path.display().to_string();
        let www = "www.climb.example.";
        let headline_specs = [
            ("full_signature_bytes_www_a", "full", Baseline::B),
            ("condensed_proof_bytes_www_a", "condensed", Baseline::C),
        ];
        for (field, label, baseline) in headline_specs {
            let pointer = format!("/dns_pipeline/{field}");
            if let Some(v) = opt_u64(root, &pointer, path)? {
                out.facts.push(fact(
                    Row {
                        section: Section::DnsLive,
                        label: format!("DNS, {www} A [{label}] — response bytes"),
                        unit: "B".to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: None,
                        baseline,
                    },
                    fmt_bytes(v),
                    &pointer,
                    &source,
                ));
            }
        }

        let reduction = "/dns_pipeline/reduction_percent_www_a";
        if let Some(v) = opt_f64(root, reduction, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::DnsLive,
                    label: format!("DNS, {www} A — response reduction, condensed vs full"),
                    unit: "%".to_string(),
                    class: Some(EvidenceClass::Derived),
                    nk: None,
                    baseline: Baseline::B,
                },
                fmt_percent(v),
                reduction,
                &source,
            ));
        }

        for (pointer, label) in [
            ("/dns_pipeline/source", "DNS — capture source"),
            ("/dns_pipeline/recorded_at", "DNS — recorded at"),
            ("/dns_pipeline/algorithm", "DNS — algorithm"),
        ] {
            if let Some(v) = opt_str(root, pointer, path)? {
                out.facts.push(fact(
                    Row {
                        section: Section::DnsLive,
                        label: label.to_string(),
                        unit: "—".to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: None,
                        baseline: Baseline::Na,
                    },
                    v,
                    pointer,
                    &source,
                ));
            }
        }
    }

    if present {
        out.notes.push(Note::custom(
            Section::DnsLive,
            "per-section response anatomy: not recorded in this capture artifact",
        ));
    }
    Ok(())
}

/// Push the environment rows for one fingerprint document, reading the same
/// field set from the raw `environment.json` and the results copy (which nests
/// the identical document under `/environment`).
fn environment_rows(
    out: &mut Collected,
    root: &Value,
    path: &Path,
    base: &str,
) -> Result<(), ReportError> {
    let source = path.display().to_string();
    let string_specs = [
        ("/timestamp", "Environment — timestamp"),
        ("/toolchain/rustc_version", "Environment — rustc version"),
        ("/toolchain/cargo_version", "Environment — cargo version"),
        ("/system/kernel_release", "Environment — kernel release"),
        ("/cpu/model_name", "Environment — CPU model"),
        (
            "/benchmark_runtime/cpu_governor",
            "Environment — CPU governor",
        ),
        (
            "/benchmark_runtime/pin_expression",
            "Environment — pin expression",
        ),
    ];
    for (suffix, label) in string_specs {
        let pointer = format!("{base}{suffix}");
        if let Some(v) = opt_str(root, &pointer, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::Environment,
                    label: label.to_string(),
                    unit: "—".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: None,
                    baseline: Baseline::Na,
                },
                v,
                &pointer,
                &source,
            ));
        }
    }

    let uint_specs = [
        ("/cpu/logical_cpus", "Environment — logical CPUs"),
        ("/cpu/physical_cores", "Environment — physical cores"),
        ("/benchmark_runtime/pin_cpu", "Environment — pinned CPU"),
    ];
    for (suffix, label) in uint_specs {
        let pointer = format!("{base}{suffix}");
        if let Some(v) = opt_u64(root, &pointer, path)? {
            out.facts.push(fact(
                Row {
                    section: Section::Environment,
                    label: label.to_string(),
                    unit: "—".to_string(),
                    class: Some(EvidenceClass::Measured),
                    nk: None,
                    baseline: Baseline::Na,
                },
                v.to_string(),
                &pointer,
                &source,
            ));
        }
    }

    let submodules = format!("{base}/submodules");
    if let Some(rows) = array_at(root, path, &submodules)? {
        for (i, row) in rows.iter().enumerate() {
            let row_base = format!("{base}/submodules/{i}");
            let Some(submodule) = opt_str(row, "/path", path)? else {
                continue;
            };
            let pointer = format!("{row_base}/sha");
            if let Some(sha) = opt_str(row, "/sha", path)? {
                out.facts.push(fact(
                    Row {
                        section: Section::Environment,
                        label: format!("Submodule {submodule} — commit SHA"),
                        unit: "—".to_string(),
                        class: Some(EvidenceClass::Measured),
                        nk: None,
                        baseline: Baseline::Na,
                    },
                    sha,
                    &pointer,
                    &source,
                ));
            }
        }
    }
    Ok(())
}

/// Collect the environment section from the raw fingerprint and the results
/// copy, plus the repository commit SHA supplied by the caller (which has no
/// recorded evidence class of its own and therefore renders UNCLASSIFIED).
fn collect_environment(
    out: &mut Collected,
    raw: Option<(&Path, &Value)>,
    results: Option<(&Path, &Value)>,
    commit_sha: Option<&str>,
) -> Result<(), ReportError> {
    if let Some((path, root)) = raw {
        environment_rows(out, root, path, "")?;
    }
    if let Some((path, root)) = results {
        environment_rows(out, root, path, "/environment")?;
    }
    if let Some(sha) = commit_sha {
        out.facts.push(fact(
            Row {
                section: Section::Environment,
                label: "Repository — commit SHA".to_string(),
                unit: "—".to_string(),
                class: None,
                nk: None,
                baseline: Baseline::Na,
            },
            sha.to_string(),
            "n/a",
            "git rev-parse HEAD",
        ));
    }
    Ok(())
}

/// Collect the quality section from a `cargo test --workspace` summary; the
/// counts are read back from the run's own `test result:` lines, never typed in.
fn collect_quality(out: &mut Collected, summary: Option<TestSummary>) {
    let Some(summary) = summary else {
        return;
    };
    let source = "cargo test --workspace";
    let nk = Some(format!("n = {}", summary.targets));
    let specs = [
        ("Workspace tests — passed", summary.passed),
        ("Workspace tests — failed", summary.failed),
        ("Workspace tests — ignored", summary.ignored),
        ("Workspace test targets (summary lines)", summary.targets),
    ];
    for (label, value) in specs {
        out.facts.push(fact(
            Row {
                section: Section::Quality,
                label: label.to_string(),
                unit: "—".to_string(),
                class: Some(EvidenceClass::Measured),
                nk: nk.clone(),
                baseline: Baseline::Na,
            },
            value.to_string(),
            "n/a",
            source,
        ));
    }
}

/// Join items with `" + "`, keeping the first occurrence of each value.
fn unique_join<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen.join(" + ")
}

/// Describe the first field on which copies of one label disagree, naming each
/// copy's source so the reader can open the artifacts directly.
fn describe_disagreement(copies: &[&Fact]) -> String {
    let fields: [(&str, Vec<String>); 5] = [
        ("value", copies.iter().map(|f| f.value.clone()).collect()),
        ("unit", copies.iter().map(|f| f.unit.clone()).collect()),
        (
            "class",
            copies
                .iter()
                .map(|f| {
                    f.class
                        .map_or("UNCLASSIFIED", EvidenceClass::label)
                        .to_string()
                })
                .collect(),
        ),
        (
            "n / k",
            copies
                .iter()
                .map(|f| f.nk.clone().unwrap_or_else(|| "—".to_string()))
                .collect(),
        ),
        (
            "baseline",
            copies
                .iter()
                .map(|f| f.baseline.label().to_string())
                .collect(),
        ),
    ];
    for (name, values) in fields {
        if values.iter().any(|value| value != &values[0]) {
            let parts: Vec<String> = copies
                .iter()
                .zip(&values)
                .map(|(f, value)| format!("{value} ({})", f.source))
                .collect();
            return format!("field \"{name}\": {}", parts.join(" vs "));
        }
    }
    "copies differ".to_string()
}

/// Group facts by `(section, label)`. Copies that agree on every quoted
/// attribute merge into one row listing each pointer and source; copies that
/// disagree are each rendered and reported as an [`Inconsistency`].
fn group_facts(facts: &[Fact]) -> (Vec<Fact>, Vec<Inconsistency>) {
    let mut groups: Vec<(Section, String, Vec<usize>)> = Vec::new();
    for (i, f) in facts.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(section, label, _)| *section == f.section && *label == f.label)
        {
            Some((_, _, indices)) => indices.push(i),
            None => groups.push((f.section, f.label.clone(), vec![i])),
        }
    }

    let mut rows = Vec::new();
    let mut inconsistencies = Vec::new();
    for (section, label, indices) in groups {
        let first = &facts[indices[0]];
        let agrees = indices.iter().all(|&i| {
            let f = &facts[i];
            f.value == first.value
                && f.unit == first.unit
                && f.class == first.class
                && f.nk == first.nk
                && f.baseline == first.baseline
        });
        if agrees {
            let mut merged = first.clone();
            merged.pointer = unique_join(indices.iter().map(|&i| facts[i].pointer.as_str()));
            merged.source = unique_join(indices.iter().map(|&i| facts[i].source.as_str()));
            rows.push(merged);
        } else {
            let copies: Vec<&Fact> = indices.iter().map(|&i| &facts[i]).collect();
            for &i in &indices {
                rows.push(facts[i].clone());
            }
            inconsistencies.push(Inconsistency {
                section,
                label,
                detail: describe_disagreement(&copies),
            });
        }
    }
    (rows, inconsistencies)
}

/// Preamble: what the sheet is, what the columns mean, and the one thing the
/// numbers are not.
const PREAMBLE: &str = "\
# CLIMB deck quote sheet

Every number a slide deck or README may quote, read back from the recorded
artifacts at generation time. Each row carries where the value lives (JSON
pointer and source file), how it was obtained (evidence class, `n`/`k`) and
what it is compared against (baseline). Nothing here is typed in by hand.

- **Evidence classes.** `MEASURED` was read from a measurement artifact;
  `DERIVED` was computed from other recorded values (ratios, byte arithmetic);
  `ANALYTICAL` comes from a closed-form model; `EXTRAPOLATED` comes from the
  `N × per-message` identity applied to a measured per-message cost rather than
  a measurement over the whole batch, and is rendered with `≈`;
  `UNCLASSIFIED` carries no recorded class.
- **Baselines.** `A` plain SLH-DSA (one full signature per message, no ladder);
  `B` MTL without amortisation (one full MTL signature per message); `C` MTL
  amortised (one signed ladder per batch plus condensed paths); `n/a` is not an
  A/B/C comparison. Definitions are maintained in `docs/BASELINES.md`.
- **Scope.** This sheet reports empirical measurements only. It is not a
  security proof, a formal verification, or a certification of any kind.
";

/// Table header for the fact rows.
const TABLE_HEADER: &str =
    "Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source";

/// Markdown alignment row for [`TABLE_HEADER`].
const TABLE_SEPARATOR: &str = "--- | --- | --- | --- | --- | --- | --- | ---";

/// Escape a table cell so embedded pipes do not break the row.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// Render one fact as a Markdown table row. `EXTRAPOLATED` values take the `≈`
/// prefix; a missing class renders `UNCLASSIFIED`.
fn render_row(row: &Fact) -> String {
    let value = if row.class == Some(EvidenceClass::Extrapolated) && !row.value.starts_with('≈') {
        format!("≈{}", row.value)
    } else {
        row.value.clone()
    };
    let class = row.class.map_or("UNCLASSIFIED", EvidenceClass::label);
    let nk = row.nk.clone().unwrap_or_else(|| "—".to_string());
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
        cell(&row.label),
        cell(&value),
        cell(&row.unit),
        class,
        cell(&nk),
        row.baseline.label(),
        cell(&row.pointer),
        cell(&row.source),
    )
}

/// Render the footer: rendered/collected/merged counts, per-class counts, the
/// UNCLASSIFIED labels, and any inconsistencies.
fn render_footer(rows: &[Fact], inconsistencies: &[Inconsistency], collected: usize) -> String {
    let mut out = String::from("\n---\n\n## Footer\n\n");
    out.push_str(&format!(
        "- Rows rendered: {} (from {} collected facts; {} duplicate copies merged).\n",
        rows.len(),
        collected,
        collected - rows.len()
    ));

    let classes = [
        EvidenceClass::Measured,
        EvidenceClass::Analytical,
        EvidenceClass::Extrapolated,
        EvidenceClass::Derived,
    ];
    let counts: Vec<String> = classes
        .iter()
        .map(|class| {
            format!(
                "{}: {}",
                class.label(),
                rows.iter().filter(|r| r.class == Some(*class)).count()
            )
        })
        .collect();
    let unclassified = rows.iter().filter(|r| r.class.is_none()).count();
    out.push_str(&format!(
        "- Evidence classes (rendered rows): {}; UNCLASSIFIED: {}.\n",
        counts.join(", "),
        unclassified
    ));

    let unclassified_labels: Vec<&str> = rows
        .iter()
        .filter(|r| r.class.is_none())
        .map(|r| r.label.as_str())
        .collect();
    if unclassified_labels.is_empty() {
        out.push_str("- UNCLASSIFIED rows: none.\n");
    } else {
        out.push_str(&format!(
            "- UNCLASSIFIED rows: {}.\n",
            unclassified_labels.join(", ")
        ));
    }

    if inconsistencies.is_empty() {
        out.push_str("- Inconsistencies: none.\n");
    } else {
        out.push_str(&format!("- Inconsistencies ({}):\n", inconsistencies.len()));
        for inconsistency in inconsistencies {
            out.push_str(&format!(
                "  - [{}] {}: {}\n",
                inconsistency.section.title(),
                inconsistency.label,
                inconsistency.detail
            ));
        }
    }
    out
}

/// Render the whole sheet: preamble, one section per [`Section::ORDER`] entry
/// (notes first, then the fact table), and the footer.
fn render_markdown(
    rows: &[Fact],
    notes: &[Note],
    inconsistencies: &[Inconsistency],
    collected: usize,
) -> String {
    let mut out = String::from(PREAMBLE);
    for section in Section::ORDER {
        out.push_str(&format!("\n## {}\n", section.title()));
        for note in notes.iter().filter(|n| n.section == section) {
            out.push_str(&format!("\n> {}\n", note.text));
        }
        let section_rows: Vec<&Fact> = rows.iter().filter(|r| r.section == section).collect();
        if !section_rows.is_empty() {
            out.push_str(&format!("\n| {TABLE_HEADER} |\n"));
            out.push_str(&format!("| {TABLE_SEPARATOR} |\n"));
            for row in section_rows {
                out.push_str(&render_row(row));
            }
        }
    }
    out.push_str(&render_footer(rows, inconsistencies, collected));
    out
}

/// Build the quote sheet from the recorded artifacts. Absent artifacts produce
/// "artifact absent" notes; a present-but-unparseable artifact is a hard error
/// naming the path.
pub fn generate(input: &DeckFactsInput) -> Result<DeckFactsOutput, ReportError> {
    let results = load_json(&input.results_path)?;
    let wire_raw = load_json(&input.wire_report_path)?;
    let memory_raw = load_json(&input.memory_report_path)?;
    let ladder = load_ladder(&input.ladder_verification_path, &input.ladder_fixture_path)?;
    let dns = load_dns(&input.dns_capture_path, &input.dns_fixture_path)?;
    let environment = load_json(&input.environment_path)?;

    let results_ref = results
        .as_ref()
        .map(|value| (input.results_path.as_path(), value));
    let mut collected = Collected::default();

    // Every results-driven section names the results document when it is
    // missing, so a missing consolidated artifact is visible per section
    // rather than as one silent swallow.
    if results.is_none() {
        for section in [
            Section::WireBytes,
            Section::SigningTime,
            Section::VerifyingTime,
            Section::Memory,
            Section::LadderVerification,
            Section::DnsLive,
            Section::Environment,
        ] {
            collected
                .notes
                .push(Note::absent(section, &input.results_path));
        }
    }
    if wire_raw.is_none() {
        collected
            .notes
            .push(Note::absent(Section::WireBytes, &input.wire_report_path));
    }
    if memory_raw.is_none() {
        collected
            .notes
            .push(Note::absent(Section::Memory, &input.memory_report_path));
    }
    if environment.is_none() {
        collected
            .notes
            .push(Note::absent(Section::Environment, &input.environment_path));
    }
    if ladder.is_none() {
        collected.notes.push(Note::absent_paths(
            Section::LadderVerification,
            &[
                input.ladder_verification_path.as_path(),
                input.ladder_fixture_path.as_path(),
            ],
        ));
    }
    if dns.is_none() {
        collected.notes.push(Note::absent_paths(
            Section::DnsLive,
            &[
                input.dns_capture_path.as_path(),
                input.dns_fixture_path.as_path(),
            ],
        ));
    }

    collect_wire(
        &mut collected,
        wire_raw
            .as_ref()
            .map(|value| (input.wire_report_path.as_path(), value)),
        results_ref,
    )?;
    collect_bench(&mut collected, results_ref, BenchKind::Signing)?;
    collect_bench(&mut collected, results_ref, BenchKind::Verifying)?;
    collect_comparisons(&mut collected, results_ref)?;
    collect_summaries(&mut collected, results_ref)?;
    collect_memory(
        &mut collected,
        memory_raw
            .as_ref()
            .map(|value| (input.memory_report_path.as_path(), value)),
        results_ref,
    )?;
    collect_ladder(
        &mut collected,
        ladder.as_ref().map(|(path, doc)| (path.as_path(), doc)),
        results_ref,
    )?;
    collect_dns(
        &mut collected,
        dns.as_ref().map(|(path, doc)| (path.as_path(), doc)),
        results_ref,
    )?;
    collect_environment(
        &mut collected,
        environment
            .as_ref()
            .map(|value| (input.environment_path.as_path(), value)),
        results_ref,
        input.commit_sha.as_deref(),
    )?;
    collect_quality(&mut collected, input.test_summary);

    if input.commit_sha.is_none() {
        collected.notes.push(Note::custom(
            Section::Environment,
            "commit SHA unavailable — `git rev-parse HEAD` produced no SHA",
        ));
    }
    if input.test_summary.is_none() {
        collected.notes.push(Note::custom(
            Section::Quality,
            "no `cargo test --workspace` summary lines were produced for this sheet",
        ));
    }

    // A section that neither an artifact nor a note mentions would silently
    // vanish from the sheet; say so instead.
    for section in Section::ORDER {
        let has_facts = collected.facts.iter().any(|f| f.section == section);
        let has_notes = collected.notes.iter().any(|n| n.section == section);
        if !has_facts && !has_notes {
            collected.notes.push(Note::custom(
                section,
                "no recorded artifact supplied a value for this section",
            ));
        }
    }

    let collected_count = collected.facts.len();
    let (rows, inconsistencies) = group_facts(&collected.facts);
    let markdown = render_markdown(&rows, &collected.notes, &inconsistencies, collected_count);
    Ok(DeckFactsOutput {
        markdown,
        rows,
        inconsistencies,
    })
}

/// Read the integer immediately before `marker` on a `test result:` line, e.g.
/// the `12` in `... 12 passed; 0 failed; ...`. Commas are tolerated in case a
/// future cargo groups large counts.
fn count_before(line: &str, marker: &str) -> Option<u64> {
    let index = line.find(marker)?;
    let tail: String = line[..index]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let digits = tail.replace(',', "");
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Sum the `test result:` summary lines of a `cargo test --workspace` run.
/// `None` when the output contains no such line (e.g. the run never started).
#[must_use]
pub fn parse_test_summary(output: &str) -> Option<TestSummary> {
    let mut summary = TestSummary {
        passed: 0,
        failed: 0,
        ignored: 0,
        targets: 0,
    };
    let mut found = false;
    for line in output.lines() {
        if !line.contains("test result:") {
            continue;
        }
        let (Some(passed), Some(failed), Some(ignored)) = (
            count_before(line, " passed;"),
            count_before(line, " failed;"),
            count_before(line, " ignored;"),
        ) else {
            continue;
        };
        summary.passed += passed;
        summary.failed += failed;
        summary.ignored += ignored;
        summary.targets += 1;
        found = true;
    }
    found.then_some(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::BenchKind;

    /// Build a fact for grouping/rendering tests with a fixed unit.
    fn test_fact(
        section: Section,
        label: &str,
        value: &str,
        class: Option<EvidenceClass>,
        baseline: Baseline,
        pointer: &str,
        source: &str,
    ) -> Fact {
        Fact {
            section,
            label: label.to_string(),
            value: value.to_string(),
            unit: "B".to_string(),
            class,
            nk: Some("n = 100".to_string()),
            baseline,
            pointer: pointer.to_string(),
            source: source.to_string(),
        }
    }

    fn sample_results_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/climb-results.json")
    }

    /// The committed sample results document with every other artifact absent,
    /// so the sheet exercises results-only sections plus absent-artifact notes.
    fn sample_input() -> DeckFactsInput {
        let missing = PathBuf::from("target/deck-facts-tests/absent.json");
        DeckFactsInput {
            results_path: sample_results_path(),
            wire_report_path: missing.clone(),
            memory_report_path: missing.clone(),
            ladder_verification_path: missing.clone(),
            ladder_fixture_path: missing.clone(),
            dns_capture_path: missing.clone(),
            dns_fixture_path: missing.clone(),
            environment_path: missing,
            commit_sha: Some("c5cf3220000000000000000000000000000000000".to_string()),
            test_summary: Some(TestSummary {
                passed: 500,
                failed: 0,
                ignored: 1,
                targets: 20,
            }),
        }
    }

    #[test]
    fn renders_every_column_of_a_fact_row() {
        let row = Fact {
            section: Section::WireBytes,
            label: "Small — wire total, C (MTL amortised)".to_string(),
            value: "24424".to_string(),
            unit: "B".to_string(),
            class: Some(EvidenceClass::Measured),
            nk: Some("n = 100".to_string()),
            baseline: Baseline::C,
            pointer: "/wire_size/0/with_mtl_total_bytes".to_string(),
            source: "target/climb-results.json".to_string(),
        };
        let markdown = render_markdown(std::slice::from_ref(&row), &[], &[], 1);
        assert!(
            markdown.contains(&format!("| {TABLE_HEADER} |")),
            "{markdown}"
        );
        let expected = "| Small — wire total, C (MTL amortised) | 24424 | B | MEASURED \
                        | n = 100 | C | /wire_size/0/with_mtl_total_bytes | \
                        target/climb-results.json |";
        assert!(markdown.contains(expected), "{markdown}");
    }

    #[test]
    fn extrapolated_values_carry_a_prefix_and_unknown_classes_render_unclassified() {
        let extrapolated = test_fact(
            Section::SigningTime,
            "Small — signing per batch, C (MTL amortised)",
            "595778430.5",
            Some(EvidenceClass::Extrapolated),
            Baseline::C,
            "/plain/0/signing_mean_ns",
            "target/climb-results.json",
        );
        let unknown = test_fact(
            Section::Environment,
            "Repository — commit SHA",
            "c5cf322",
            None,
            Baseline::Na,
            "n/a",
            "git rev-parse HEAD",
        );
        let markdown = render_markdown(&[extrapolated, unknown], &[], &[], 2);
        assert!(markdown.contains("| ≈595778430.5 |"), "{markdown}");
        assert!(markdown.contains("| UNCLASSIFIED |"), "{markdown}");
        assert!(
            markdown.contains("- UNCLASSIFIED rows: Repository — commit SHA."),
            "{markdown}"
        );
        assert!(markdown.contains("- Inconsistencies: none."), "{markdown}");
    }

    #[test]
    fn missing_artifacts_render_absent_notes_instead_of_failing() {
        let missing = PathBuf::from("target/deck-facts-tests/does-not-exist.json");
        let input = DeckFactsInput {
            results_path: missing.clone(),
            wire_report_path: missing.clone(),
            memory_report_path: missing.clone(),
            ladder_verification_path: missing.clone(),
            ladder_fixture_path: missing.clone(),
            dns_capture_path: missing.clone(),
            dns_fixture_path: missing.clone(),
            environment_path: missing,
            commit_sha: None,
            test_summary: None,
        };
        let output = generate(&input).expect("absent artifacts must not fail the sheet");
        assert!(output.rows.is_empty(), "{:?}", output.rows);
        assert!(output.inconsistencies.is_empty());
        assert!(
            output.markdown.contains("artifact absent"),
            "{}",
            output.markdown
        );
        for section in Section::ORDER {
            assert!(
                output.markdown.contains(&format!("## {}", section.title())),
                "section {} missing from the sheet",
                section.title()
            );
        }
        assert!(output
            .markdown
            .contains("no `cargo test --workspace` summary lines"));
    }

    #[test]
    fn disagreeing_sources_are_reported_as_an_inconsistency() {
        let first = test_fact(
            Section::WireBytes,
            "Small — wire reduction, C vs B",
            "97.006",
            Some(EvidenceClass::Derived),
            Baseline::B,
            "/0/reduction_percent",
            "target/wire-report.json",
        );
        let second = test_fact(
            Section::WireBytes,
            "Small — wire reduction, C vs B",
            "97.007",
            Some(EvidenceClass::Derived),
            Baseline::B,
            "/wire_size/0/reduction_percent",
            "target/climb-results.json",
        );
        let (rows, inconsistencies) = group_facts(&[first, second]);
        assert_eq!(rows.len(), 2);
        assert_eq!(inconsistencies.len(), 1);
        assert!(
            inconsistencies[0].detail.contains("field \"value\""),
            "{}",
            inconsistencies[0].detail
        );
        assert!(inconsistencies[0]
            .detail
            .contains("target/wire-report.json"));
        assert!(inconsistencies[0]
            .detail
            .contains("target/climb-results.json"));

        let markdown = render_markdown(&rows, &[], &inconsistencies, 2);
        assert!(markdown.contains("- Inconsistencies (1):"), "{markdown}");
        assert!(
            markdown.contains("[Wire bytes] Small — wire reduction, C vs B"),
            "{markdown}"
        );
    }

    #[test]
    fn agreeing_sources_merge_into_one_row() {
        let first = test_fact(
            Section::WireBytes,
            "Small — wire reduction, C vs B",
            "97.006",
            Some(EvidenceClass::Derived),
            Baseline::B,
            "/0/reduction_percent",
            "target/wire-report.json",
        );
        let second = test_fact(
            Section::WireBytes,
            "Small — wire reduction, C vs B",
            "97.006",
            Some(EvidenceClass::Derived),
            Baseline::B,
            "/wire_size/0/reduction_percent",
            "target/climb-results.json",
        );
        let (rows, inconsistencies) = group_facts(&[first, second]);
        assert_eq!(rows.len(), 1);
        assert!(inconsistencies.is_empty());
        assert_eq!(
            rows[0].pointer,
            "/0/reduction_percent + /wire_size/0/reduction_percent"
        );
        assert_eq!(
            rows[0].source,
            "target/wire-report.json + target/climb-results.json"
        );
        assert!(
            render_markdown(&rows, &[], &inconsistencies, 2)
                .contains("2 collected facts; 1 duplicate copies merged"),
            "merge count not reported"
        );
    }

    #[test]
    fn generate_is_deterministic_across_two_runs() {
        let input = sample_input();
        let first = generate(&input).expect("first run");
        let second = generate(&input).expect("second run");
        assert_eq!(first.markdown, second.markdown);
        assert_eq!(first.rows, second.rows);
    }

    #[test]
    fn parse_test_summary_sums_every_result_line() {
        let output = "\
running 3 tests
test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 10 tests
test result: ok. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
";
        let summary = parse_test_summary(output).expect("summary lines must be found");
        assert_eq!(
            summary,
            TestSummary {
                passed: 13,
                failed: 1,
                ignored: 1,
                targets: 2,
            }
        );
    }

    #[test]
    fn parse_test_summary_is_none_without_result_lines() {
        assert_eq!(
            parse_test_summary("running 0 tests\n\nno tests to run\n"),
            None
        );
    }

    #[test]
    fn present_but_invalid_results_names_the_path_in_the_error() {
        let dir = PathBuf::from("target/deck-facts-tests");
        std::fs::create_dir_all(&dir).expect("create test directory");
        let invalid = dir.join("invalid-results.json");
        std::fs::write(&invalid, "{ not json").expect("write invalid artifact");
        let missing = dir.join("absent.json");
        let input = DeckFactsInput {
            results_path: invalid.clone(),
            wire_report_path: missing.clone(),
            memory_report_path: missing.clone(),
            ladder_verification_path: missing.clone(),
            ladder_fixture_path: missing.clone(),
            dns_capture_path: missing.clone(),
            dns_fixture_path: missing.clone(),
            environment_path: missing,
            commit_sha: None,
            test_summary: None,
        };
        let error = generate(&input).expect_err("a malformed consolidated artifact must fail");
        match error {
            ReportError::JsonDecode { path, .. } => assert_eq!(path, invalid),
            other => panic!("expected a JSON decode error naming the path, got {other:?}"),
        }
        let _ = std::fs::remove_file(&invalid);
    }

    #[test]
    fn committed_sample_results_produce_a_populated_sheet() {
        let output = generate(&sample_input()).expect("sample results must render");
        assert!(output.rows.len() > 20, "rows: {}", output.rows.len());
        assert!(
            output.inconsistencies.is_empty(),
            "{:?}",
            output.inconsistencies
        );
        for heading in ["## Wire bytes", "## Signing time", "## Quality"] {
            assert!(output.markdown.contains(heading), "missing {heading}");
        }
        assert!(output.markdown.contains("Workspace tests — passed"));
        assert!(output.markdown.contains("UNCLASSIFIED"));
        assert!(output.markdown.contains("artifact absent"));
    }

    #[test]
    fn row_scoped_artifacts_are_read_relative_to_their_rows() {
        // Test data: a minimal consolidated document whose rows each carry the
        // fields the collectors read by row-relative pointer. Before the rows
        // were read relative to their own element, every one of these rows was
        // silently dropped and the labels below never appeared.
        let document = r#"{
            "version": "1.0",
            "wire_size": [{"size": "small", "message_count": 10, "ladder_len": 68,
                "condensed_min": 76, "condensed_mean": 76.0, "condensed_max": 76,
                "with_mtl_total_bytes": 100, "without_mtl_total_bytes": 1000,
                "reduction_percent": 90.0, "plain_total_bytes": 1000,
                "plain_signature_len_bytes": 100}],
            "benchmark": {
                "signing": [{"size": "Small", "condition": "With MTL", "messages": 10,
                    "mean_ns": 1000000.0}],
                "verifying": []
            },
            "plain": [{"size": "Small", "messages": 10, "signing_mean_ns": 5000000.0,
                "verifying_mean_ns": 500000.0, "class": "measured", "samples": null}],
            "comparisons": [{"size": "Small", "wire_reduction_c_vs_a_percent": 90.0,
                "wire_reduction_c_vs_b_percent": 90.0, "signing_speedup_c_vs_a": 5.0,
                "signing_speedup_c_vs_b": 5.0, "verifying_speedup_cached_vs_a": 1.0,
                "verifying_speedup_cold_vs_a": 1.0, "class": "measured"}],
            "memory": {
                "measured": [{"size": "small", "median_bytes": 1000,
                    "measured_range_min_bytes": 900, "measured_range_max_bytes": 1100,
                    "iqr_bytes": 200, "measured_peak_bytes": 1000, "samples": 7,
                    "method": "isolated-process", "libmtl_analytical_bytes": 100,
                    "harness_buffers_bytes": 10, "dataset_buffers_bytes": 10,
                    "keypair_bytes": 1, "analytical_total_bytes": 121,
                    "residual_bytes": 879, "measured_analytical_total_ratio": 8.26}],
                "analytical_model": {"tree_page_size_bytes": 1, "tree_max_pages": 2,
                    "randomizer_pages": 3, "hash_size_bytes": 16}
            },
            "ladder": {"source": "recorded-fixture", "recorded_at": "t", "algorithm": "a",
                "all_match": true, "sizes": [{"leaf_count": 1, "rung_count": 1,
                    "ladder_buffer_len": 68, "signed_ladder_len": 100, "condensed_min": 76,
                    "condensed_max": 76, "condensed_mean": 76.0, "model_matches": true}]},
            "environment": {"timestamp": "t", "system": {"kernel_release": "k"},
                "toolchain": {"rustc_version": "r", "cargo_version": "c"},
                "cpu": {"model_name": "m", "logical_cpus": 4, "physical_cores": 2},
                "submodules": [{"path": "third_party/x", "sha": "abc"}],
                "benchmark_runtime": {"cpu_governor": "g", "pin_cpu": 2,
                    "pin_expression": "taskset -c 2"}},
            "dns_pipeline": {"full_signature_bytes_www_a": 24438,
                "condensed_proof_bytes_www_a": 658, "reduction_percent_www_a": 97.3,
                "source": "recorded-fixture", "recorded_at": "t", "algorithm": "a"},
            "derived": {"overall_wire_reduction_percent": 90.0,
                "largest_wire_reduction_percent": 90.0,
                "overall_verifying_trust_cached_speedup_avg": 1.0},
            "warnings": []
        }"#;
        let dir = PathBuf::from("target/deck-facts-tests");
        std::fs::create_dir_all(&dir).expect("create test directory");
        let results_path = dir.join("row-scoped-results.json");
        std::fs::write(&results_path, document).expect("write test document");
        let missing = dir.join("absent.json");
        let input = DeckFactsInput {
            results_path: results_path.clone(),
            wire_report_path: missing.clone(),
            memory_report_path: missing.clone(),
            ladder_verification_path: missing.clone(),
            ladder_fixture_path: missing.clone(),
            dns_capture_path: missing.clone(),
            dns_fixture_path: missing.clone(),
            environment_path: missing,
            commit_sha: None,
            test_summary: None,
        };
        let output = generate(&input).expect("row-scoped document must render");
        let _ = std::fs::remove_file(&results_path);

        for expected in [
            "Small — wire total, C (MTL amortised)",
            "Small — wire reduction, C vs A",
            "Small — signing per batch, C (MTL amortised)",
            "Small — signing per message, A (plain SLH-DSA)",
            "Small — resident median (measured)",
            "Small — analytical total",
            "Ladder, N=1 — rung count",
            "Submodule third_party/x — commit SHA",
            "DNS, www.climb.example. A [full] — response bytes",
            "Environment — kernel release",
            "Environment — logical CPUs",
        ] {
            assert!(
                output.rows.iter().any(|row| row.label == expected),
                "missing {expected}; rows: {:?}",
                output.rows.iter().map(|r| &r.label).collect::<Vec<_>>()
            );
        }

        let signing = output
            .rows
            .iter()
            .find(|row| row.label == "Small — signing per batch, C (MTL amortised)")
            .expect("signing row");
        assert_eq!(signing.value, "1.0");
        assert_eq!(signing.unit, "ms");
        assert_eq!(signing.nk.as_deref(), Some("n = 10"));

        let submodule = output
            .rows
            .iter()
            .find(|row| row.label == "Submodule third_party/x — commit SHA")
            .expect("submodule row");
        assert_eq!(submodule.value, "abc");
    }

    #[test]
    fn signing_and_verifying_conditions_map_to_baselines() {
        assert_eq!(
            condition_gloss(BenchKind::Signing, "With MTL"),
            Some(("C (MTL amortised)", Baseline::C))
        );
        assert_eq!(
            condition_gloss(BenchKind::Signing, "Without MTL"),
            Some(("B (MTL, no amortisation)", Baseline::B))
        );
        assert_eq!(
            condition_gloss(BenchKind::Verifying, "MTL (trust cached)"),
            Some(("C (trust cached ladder)", Baseline::C))
        );
        assert_eq!(
            condition_gloss(BenchKind::Verifying, "Without MTL"),
            Some(("B (MTL, no amortisation)", Baseline::B))
        );
        assert_eq!(condition_gloss(BenchKind::Signing, "No MTL at all"), None);
    }
}
