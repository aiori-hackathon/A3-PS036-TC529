//! Build-time site data: every number the website renders is read from a
//! recorded artifact, formatted once, sealed with a content hash and written
//! under `site/src/data/generated/`.
//!
//! The module is native-free: it reads JSON documents and text artifacts and
//! never touches libMTL, liboqs or a C library. The six generated documents
//! are
//!
//! | File | Source |
//! |---|---|
//! | `facts.json` | the committed results artifact, the quote sheet, the claims registry |
//! | `claims.json` | the claims registry derived from the results artifact |
//! | `provenance.json` | the recorded provenance file and the environment fingerprint |
//! | `series.json` | the wire, signing, verifying and memory rows of the results artifact |
//! | `ladder.json` | the recorded ladder-verification fixture, re-checked against the model |
//! | `commits.json` | the committed commit-graph document |
//!
//! Every document is deterministic: it contains no wall-clock timestamp, no
//! hash-map iteration order and no absolute path. A `content_sha256` field is
//! the SHA-256 of the canonical JSON with that field blanked, so a reader can
//! verify the file without trusting the writer.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::error::ReportError;
use crate::ladder::LadderVerification;

/// Schema version shared by every generated document.
pub const SITE_DATA_SCHEMA_VERSION: u32 = 1;

/// Committed results artifact every site fact is read from.
pub const DEFAULT_RESULTS: &str = "docs/sample-output/climb-results.json";
/// Committed ladder-verification fixture.
pub const DEFAULT_LADDER: &str = "docs/sample-output/ladder-verification.recorded.json";
/// Recorded provenance of the sample output.
pub const DEFAULT_PROVENANCE: &str = "docs/sample-output/PROVENANCE.txt";
/// Committed environment fingerprint.
pub const DEFAULT_ENVIRONMENT: &str = "environment.json";
/// Committed commit-graph document.
pub const DEFAULT_COMMITS: &str = "docs/media/commit-graph.json";
/// Generated quote sheet, read to cross-check unit, class and baseline.
pub const DEFAULT_QUOTE_SHEET: &str = "docs/deck-facts.md";
/// Default output directory, relative to the repository root.
pub const DEFAULT_OUT_DIR: &str = "site/src/data/generated";

/// The six generated files, in write order.
pub const OUTPUT_FILES: [&str; 6] = [
    "facts.json",
    "claims.json",
    "provenance.json",
    "series.json",
    "ladder.json",
    "commits.json",
];

/// How a fact value is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactKind {
    /// A ratio: four significant digits, grouped integer part.
    Ratio,
    /// A percentage: three decimals, matching the quote sheet.
    Percent,
    /// A whole count or byte total.
    Count,
    /// A recorded string, copied verbatim.
    Text,
    /// `k of n`, e.g. `11 of 12`.
    CountOf,
}

impl FactKind {
    /// Artifact-stable kind name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            FactKind::Ratio => "ratio",
            FactKind::Percent => "percent",
            FactKind::Count => "count",
            FactKind::Text => "text",
            FactKind::CountOf => "count_of",
        }
    }
}

enum FactValue {
    /// One JSON pointer into the results artifact.
    Pointer(&'static str),
    /// The length of the array at one JSON pointer.
    ArrayLen(&'static str),
    /// `fit of total` from two JSON pointers.
    CountOf {
        fit: &'static str,
        total: &'static str,
    },
    /// A field of one recorded ladder size in the ladder-verification fixture.
    Ladder {
        leaf_count: u64,
        pointer_suffix: &'static str,
    },
    /// A constant read from a model source file, not from JSON.
    Constant { value: f64, pointer: &'static str },
    /// The `passed of total` count of the claims registry.
    ClaimsCount,
}

struct FactSpec {
    id: &'static str,
    kind: FactKind,
    value: FactValue,
    unit: &'static str,
    class: &'static str,
    baseline: &'static str,
}

/// The fact ids the copy deck uses. Each id names evidence, not presentation.
const FACT_SPECS: [FactSpec; 18] = [
    FactSpec {
        id: "F-wire-medium-c-vs-a",
        kind: FactKind::Percent,
        value: FactValue::Pointer("/comparisons/2/wire_reduction_c_vs_a_percent"),
        unit: "%",
        class: "derived",
        baseline: "A",
    },
    FactSpec {
        id: "F-sign-medium-c-vs-a",
        kind: FactKind::Ratio,
        value: FactValue::Pointer("/comparisons/2/signing_speedup_c_vs_a"),
        unit: "×",
        class: "derived",
        baseline: "A",
    },
    FactSpec {
        id: "F-verify-medium-c-vs-a",
        kind: FactKind::Ratio,
        value: FactValue::Pointer("/comparisons/2/verifying_speedup_cached_vs_a"),
        unit: "×",
        class: "derived",
        baseline: "A",
    },
    FactSpec {
        id: "F-udp-c-fit",
        kind: FactKind::CountOf,
        value: FactValue::CountOf {
            fit: "/derived/udp_fit/aggregate/2/fit_rows",
            total: "/derived/udp_fit/aggregate/2/total_rows",
        },
        unit: "rows",
        class: "analytical",
        baseline: "C",
    },
    FactSpec {
        id: "F-udp-c-total",
        kind: FactKind::Count,
        value: FactValue::Pointer("/derived/udp_fit/aggregate/2/total_rows"),
        unit: "rows",
        class: "analytical",
        baseline: "C",
    },
    FactSpec {
        id: "F-wire-medium-a",
        kind: FactKind::Count,
        value: FactValue::Pointer("/wire_size/1/plain_total_bytes"),
        unit: "B",
        class: "measured",
        baseline: "A",
    },
    FactSpec {
        id: "F-wire-medium-b",
        kind: FactKind::Count,
        value: FactValue::Pointer("/wire_size/1/without_mtl_total_bytes"),
        unit: "B",
        class: "measured",
        baseline: "B",
    },
    FactSpec {
        id: "F-wire-medium-c",
        kind: FactKind::Count,
        value: FactValue::Pointer("/wire_size/1/with_mtl_total_bytes"),
        unit: "B",
        class: "measured",
        baseline: "C",
    },
    FactSpec {
        id: "F-plain-signature-bytes",
        kind: FactKind::Count,
        value: FactValue::Ladder {
            leaf_count: 8,
            pointer_suffix: "signature_len",
        },
        unit: "B",
        class: "measured",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-condensed-path-bytes",
        kind: FactKind::Count,
        value: FactValue::Ladder {
            leaf_count: 8,
            pointer_suffix: "condensed/min",
        },
        unit: "B",
        class: "measured",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-wire-medium-c-vs-b",
        kind: FactKind::Percent,
        value: FactValue::Pointer("/comparisons/2/wire_reduction_c_vs_b_percent"),
        unit: "%",
        class: "derived",
        baseline: "B",
    },
    FactSpec {
        id: "F-sign-batch-size",
        kind: FactKind::Count,
        value: FactValue::Pointer("/wire_size/1/message_count"),
        unit: "messages",
        class: "measured",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-mem-small-ratio",
        kind: FactKind::Ratio,
        value: FactValue::Pointer("/memory/measured/0/measured_analytical_total_ratio"),
        unit: "×",
        class: "derived",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-dns-www-a",
        kind: FactKind::Percent,
        value: FactValue::Pointer("/dns_pipeline/reduction_percent_www_a"),
        unit: "%",
        class: "derived",
        baseline: "B",
    },
    FactSpec {
        id: "F-wire-sizes",
        kind: FactKind::Count,
        value: FactValue::ArrayLen("/wire_size"),
        unit: "sizes",
        class: "measured",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-parameter-set",
        kind: FactKind::Text,
        value: FactValue::Pointer("/dns_pipeline/algorithm"),
        unit: "—",
        class: "measured",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-udp-payload",
        kind: FactKind::Count,
        value: FactValue::Constant {
            value: climb_analytical::UDP_MTU_1232 as f64,
            pointer: "UDP_MTU_1232",
        },
        unit: "B",
        class: "constant",
        baseline: "n/a",
    },
    FactSpec {
        id: "F-claims-count",
        kind: FactKind::CountOf,
        value: FactValue::ClaimsCount,
        unit: "claims",
        class: "generated",
        baseline: "n/a",
    },
];

/// Where a fact value came from, for the `source` field.
fn fact_source(value: &FactValue) -> &'static str {
    match value {
        FactValue::ClaimsCount => "docs/sample-output/climb-results.json (claims registry)",
        FactValue::Ladder { .. } => DEFAULT_LADDER,
        FactValue::Constant { .. } => "crates/climb-analytical/src/udp_fit.rs",
        _ => DEFAULT_RESULTS,
    }
}

/// Format a percentage the way the quote sheet records it.
#[must_use]
pub fn format_percent(value: f64) -> String {
    format!("{value:.3}")
}

/// Format a whole count with thousands separators.
#[must_use]
pub fn format_count(value: f64) -> String {
    crate::charts::fmt_count(value)
}

/// Format a ratio with the shared four-significant-digit convention.
#[must_use]
pub fn format_ratio(value: f64) -> String {
    crate::numbers::fmt_ratio(value)
}

/// Prefix an extrapolated value with the approximation mark.
#[must_use]
pub fn with_approximation(class: &str, text: String) -> String {
    if class == "extrapolated" {
        format!("\u{2248}{text}")
    } else {
        text
    }
}

fn malformed(path: &Path, reason: impl Into<String>) -> ReportError {
    ReportError::MalformedData {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn read_text(path: &Path) -> Result<Option<String>, ReportError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ReportError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_json(path: &Path) -> Result<Value, ReportError> {
    let text = read_text(path)?.ok_or_else(|| ReportError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(ErrorKind::NotFound, "artifact not found"),
    })?;
    serde_json::from_str(&text).map_err(|source| ReportError::JsonDecode {
        path: path.to_path_buf(),
        source,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Add the `content_sha256` seal to a JSON object.
///
/// The hash covers the canonical JSON with the seal field set to the empty
/// string, so the document can be verified by blanking the field and hashing
/// again.
pub fn seal(document: Value) -> Result<Value, ReportError> {
    // Normalise float spelling before hashing. `json!` keeps a 17-digit
    // rendering for some f64 values while a re-parsed document keeps the
    // shortest round-trip form; normalising here makes the seal stable across
    // a write-then-read cycle.
    let mut document: Value =
        serde_json::from_str(&serde_json::to_string(&document).map_err(|source| {
            ReportError::JsonDecode {
                path: PathBuf::from("<generated>"),
                source,
            }
        })?)
        .map_err(|source| ReportError::JsonDecode {
            path: PathBuf::from("<generated>"),
            source,
        })?;
    {
        let Some(object) = document.as_object_mut() else {
            return Err(malformed(
                Path::new("<generated>"),
                "generated document must be a JSON object",
            ));
        };
        object.insert("content_sha256".to_string(), Value::String(String::new()));
    }
    let canonical = serde_json::to_vec(&document).map_err(|source| ReportError::JsonDecode {
        path: PathBuf::from("<generated>"),
        source,
    })?;
    let hash = sha256_hex(&canonical);
    if let Some(object) = document.as_object_mut() {
        object.insert("content_sha256".to_string(), Value::String(hash));
    }
    Ok(document)
}

/// Whether a generated document's seal matches its contents.
#[must_use]
pub fn seal_is_valid(document: &Value) -> bool {
    let Some(expected) = document.get("content_sha256").and_then(Value::as_str) else {
        return false;
    };
    let mut copy = document.clone();
    let Some(object) = copy.as_object_mut() else {
        return false;
    };
    object.insert("content_sha256".to_string(), Value::String(String::new()));
    serde_json::to_vec(&copy)
        .map(|canonical| sha256_hex(&canonical) == expected)
        .unwrap_or(false)
}

/// One row of the generated quote sheet.
struct QuoteRow {
    unit: String,
    class: String,
    baseline: String,
    pointers: Vec<String>,
}

/// Read the unit, class, baseline and pointer cells of every quote-sheet row.
fn parse_quote_sheet(text: &str) -> Vec<QuoteRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.starts_with("| ---") {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        // Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source
        if cells.len() < 8 {
            continue;
        }
        let Some(unit) = cells.get(2) else { continue };
        let Some(class) = cells.get(3) else { continue };
        let Some(baseline) = cells.get(5) else {
            continue;
        };
        let Some(pointer_cell) = cells.get(6) else {
            continue;
        };
        let pointers = pointer_cell
            .split(" + ")
            .map(str::trim)
            .filter(|token| token.starts_with('/'))
            .map(str::to_string)
            .collect();
        rows.push(QuoteRow {
            unit: (*unit).to_string(),
            class: (*class).to_string(),
            baseline: (*baseline).to_string(),
            pointers,
        });
    }
    rows
}

fn quote_row_for<'a>(rows: &'a [QuoteRow], pointer: &str) -> Option<&'a QuoteRow> {
    rows.iter()
        .find(|row| row.pointers.iter().any(|token| token == pointer))
}

fn number_at(results: &Value, pointer: &str) -> Result<f64, ReportError> {
    results
        .pointer(pointer)
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_RESULTS),
                format!("{pointer} is not a number"),
            )
        })
}

fn string_at(results: &Value, pointer: &str) -> Result<String, ReportError> {
    results
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_RESULTS),
                format!("{pointer} is not a string"),
            )
        })
}

fn class_label(lower: &str) -> String {
    match lower {
        "measured" => "MEASURED".to_string(),
        "derived" => "DERIVED".to_string(),
        "analytical" => "ANALYTICAL".to_string(),
        "extrapolated" => "EXTRAPOLATED".to_string(),
        "constant" => "CONSTANT".to_string(),
        "generated" => "GENERATED".to_string(),
        other => other.to_uppercase(),
    }
}

/// Read one field of the recorded ladder row for a leaf count.
///
/// The fixture is ordered by size, so the row is located by `leaf_count`
/// rather than by array position and the returned pointer names the row that
/// actually supplied the value.
fn ladder_size_value(
    ladder: &Value,
    leaf_count: u64,
    pointer_suffix: &str,
) -> Result<(usize, f64), ReportError> {
    let sizes = ladder
        .get("sizes")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(Path::new(DEFAULT_LADDER), "`sizes` is missing"))?;
    let index = sizes
        .iter()
        .position(|row| row.get("leaf_count").and_then(Value::as_u64) == Some(leaf_count))
        .ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_LADDER),
                format!("no recorded row for N={leaf_count}"),
            )
        })?;
    let value = sizes
        .get(index)
        .and_then(|row| row.pointer(&format!("/{pointer_suffix}")))
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_LADDER),
                format!("/sizes/{index}/{pointer_suffix} is not a number"),
            )
        })?;
    Ok((index, value))
}

/// Build `facts.json` from the results artifact, the recorded ladder fixture,
/// the quote sheet and the claims registry.
pub fn build_facts(
    results: &Value,
    ladder: &Value,
    quote_sheet: Option<&str>,
    claims_passed: usize,
    claims_total: usize,
) -> Result<Value, ReportError> {
    let rows = quote_sheet.map(parse_quote_sheet).unwrap_or_default();
    let mut facts = Vec::new();
    for spec in FACT_SPECS {
        let (raw, denominator, pointer, source) = match &spec.value {
            FactValue::Pointer(pointer) => {
                let pointer = *pointer;
                let display = if spec.kind == FactKind::Text {
                    Value::String(string_at(results, pointer)?)
                } else {
                    Value::Number(
                        serde_json::Number::from_f64(number_at(results, pointer)?)
                            .ok_or_else(|| malformed(Path::new(DEFAULT_RESULTS), "non-finite"))?,
                    )
                };
                (display, None, pointer.to_string(), fact_source(&spec.value))
            }
            FactValue::ArrayLen(pointer) => {
                let length = results
                    .pointer(pointer)
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .ok_or_else(|| {
                        malformed(Path::new(DEFAULT_RESULTS), "array pointer missing")
                    })?;
                (
                    json!(length),
                    None,
                    (*pointer).to_string(),
                    fact_source(&spec.value),
                )
            }
            FactValue::CountOf {
                fit: fit_pointer,
                total: total_pointer,
            } => {
                let fit = number_at(results, fit_pointer)?;
                let total = number_at(results, total_pointer)?;
                (
                    json!(fit),
                    Some(total),
                    (*fit_pointer).to_string(),
                    fact_source(&spec.value),
                )
            }
            FactValue::Ladder {
                leaf_count,
                pointer_suffix,
            } => {
                let (index, value) = ladder_size_value(ladder, *leaf_count, pointer_suffix)?;
                let pointer = format!("/sizes/{index}/{pointer_suffix}");
                (json!(value), None, pointer, fact_source(&spec.value))
            }
            FactValue::Constant { value, pointer } => (
                json!(value),
                None,
                (*pointer).to_string(),
                fact_source(&spec.value),
            ),
            FactValue::ClaimsCount => (
                json!(claims_passed),
                Some(claims_total as f64),
                "html::claims".to_string(),
                fact_source(&spec.value),
            ),
        };

        let mut unit = spec.unit.to_string();
        let mut class = spec.class.to_string();
        let mut baseline = spec.baseline.to_string();
        // The quote sheet is the rendered record of unit/class/baseline; when
        // a row names this pointer, the two must agree or the site would label
        // a value differently from the record it quotes.
        if let Some(row) = quote_row_for(&rows, &pointer) {
            if !unit_is_compatible(&unit, &row.unit) {
                return Err(malformed(
                    Path::new(DEFAULT_QUOTE_SHEET),
                    format!(
                        "{}: quote sheet unit `{}` disagrees with `{unit}`",
                        spec.id, row.unit
                    ),
                ));
            }
            if class_label(&class) != row.class {
                return Err(malformed(
                    Path::new(DEFAULT_QUOTE_SHEET),
                    format!(
                        "{}: quote sheet class `{}` disagrees with `{}`",
                        spec.id,
                        row.class,
                        class_label(&class)
                    ),
                ));
            }
            if baseline != row.baseline && row.baseline != "n/a" {
                return Err(malformed(
                    Path::new(DEFAULT_QUOTE_SHEET),
                    format!(
                        "{}: quote sheet baseline `{}` disagrees with `{baseline}`",
                        spec.id, row.baseline
                    ),
                ));
            }
            unit = row.unit.clone();
            class = row.class.to_lowercase();
            baseline = row.baseline.clone();
        }

        let raw_number = raw.as_f64();
        let display = match spec.kind {
            FactKind::Ratio => raw_number.map(format_ratio),
            FactKind::Percent => raw_number.map(format_percent),
            FactKind::Count => raw_number.map(format_count),
            FactKind::Text => raw.as_str().map(str::to_string),
            FactKind::CountOf => match (raw_number, denominator) {
                (Some(fit), Some(total)) => {
                    Some(format!("{} of {}", format_count(fit), format_count(total)))
                }
                _ => None,
            },
        }
        .ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_RESULTS),
                format!("{} has no value", spec.id),
            )
        })?;
        let display = with_approximation(&class, display);

        facts.push(json!({
            "id": spec.id,
            "display": display,
            "unit": unit,
            "class": class,
            "baseline": baseline,
            "pointer": pointer,
            "source": source,
            "kind": spec.kind.as_str(),
            "raw": raw,
            "denominator": denominator,
        }));
    }

    Ok(json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "facts": facts,
    }))
}

/// Whether a quote-sheet unit and the registry unit describe the same thing.
fn unit_is_compatible(registry: &str, sheet: &str) -> bool {
    registry == sheet || sheet == "—" || registry == "—"
}

/// Build `claims.json` from the claims registry.
pub fn build_claims(results: &Value, registry: &[crate::html::Claim]) -> Value {
    let mut claims = Vec::with_capacity(registry.len());
    let mut passed = 0usize;
    for claim in registry {
        let verified = results
            .pointer(&claim.pointer)
            .is_some_and(|found| found == &claim.value);
        if verified {
            passed += 1;
        }
        claims.push(json!({
            "id": claim.id,
            "pointer": claim.pointer,
            "value": claim.value,
            "formatted": claim.formatted,
            "verified": verified,
        }));
    }
    json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "source": DEFAULT_RESULTS,
        "total": claims.len(),
        "passed": passed,
        "claims": claims,
    })
}

/// Build `provenance.json` from the recorded provenance file and the
/// environment fingerprint.
pub fn build_provenance(provenance_text: &str, environment: &Value) -> Result<Value, ReportError> {
    let mut fields = BTreeMap::new();
    for line in provenance_text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let get = |key: &str| -> Result<String, ReportError> {
        fields.get(key).cloned().ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_PROVENANCE),
                format!("missing `{key}` line"),
            )
        })
    };
    let commit = get("commit sha")?;
    let rustc = get("rustc")?;
    let date = get("date")?;
    let submodule_text = get("submodule shas")?;
    let mut submodules = Vec::new();
    for entry in submodule_text.split(',') {
        if let Some((path, sha)) = entry.trim().split_once('=') {
            submodules.push(json!({
                "path": path.trim(),
                "sha": sha.trim(),
            }));
        }
    }
    if submodules.is_empty() {
        return Err(malformed(
            Path::new(DEFAULT_PROVENANCE),
            "no submodule SHAs recorded",
        ));
    }

    let host = host_fingerprint(environment)?;
    let short = commit.chars().take(12).collect::<String>();
    Ok(json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "source": DEFAULT_PROVENANCE,
        "commit": commit,
        "commit_short": short,
        "submodules": submodules,
        "rustc": rustc,
        "host_fingerprint_sha256": host,
        "date": date,
    }))
}

/// A stable fingerprint of the machine that recorded the artifacts.
///
/// The hash covers the kernel string, CPU model and core counts only; paths,
/// container IDs and timestamps would make it change for no measurement
/// reason.
fn host_fingerprint(environment: &Value) -> Result<String, ReportError> {
    let field = |pointer: &str| -> Result<String, ReportError> {
        environment
            .pointer(pointer)
            .map(|value| match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .ok_or_else(|| {
                malformed(
                    Path::new(DEFAULT_ENVIRONMENT),
                    format!("{pointer} is missing"),
                )
            })
    };
    let parts = [
        field("/system/uname")?,
        field("/system/kernel_release")?,
        field("/cpu/model_name")?,
        field("/cpu/logical_cpus")?,
        field("/cpu/physical_cores")?,
    ];
    Ok(sha256_hex(parts.join("\n").as_bytes()))
}

#[derive(Serialize)]
struct WirePoint {
    size: String,
    messages: usize,
    plain_bytes: u64,
    without_mtl_bytes: u64,
    with_mtl_bytes: u64,
    reduction_c_vs_a_percent: f64,
    reduction_c_vs_b_percent: f64,
}

#[derive(Serialize)]
struct BenchPoint {
    size: String,
    condition: String,
    messages: u64,
    mean_ns: f64,
    per_message_ns: f64,
}

#[derive(Serialize)]
struct MemoryPoint {
    size: String,
    messages: usize,
    measured_mean_bytes: u64,
    analytical_total_bytes: u64,
    measured_analytical_total_ratio: f64,
    samples: usize,
}

/// Build `series.json` from the typed results document.
pub fn build_series(results: &crate::results::ClimbResults) -> Result<Value, ReportError> {
    let wire: Vec<WirePoint> = results
        .wire_size
        .iter()
        .map(|row| WirePoint {
            size: row.size.clone(),
            messages: row.message_count,
            plain_bytes: row.plain_total_bytes,
            without_mtl_bytes: row.without_mtl_total_bytes,
            with_mtl_bytes: row.with_mtl_total_bytes,
            reduction_c_vs_a_percent: wire_reduction_c_vs_a(results, &row.size),
            reduction_c_vs_b_percent: row.reduction_percent,
        })
        .collect();
    let signing: Vec<BenchPoint> = results
        .benchmark
        .signing
        .iter()
        .map(|row| BenchPoint {
            size: row.size.clone(),
            condition: row.condition.clone(),
            messages: row.messages,
            mean_ns: row.mean_ns,
            per_message_ns: per_message(row.mean_ns, row.messages),
        })
        .collect();
    let verifying: Vec<BenchPoint> = results
        .benchmark
        .verifying
        .iter()
        .map(|row| BenchPoint {
            size: row.size.clone(),
            condition: row.condition.clone(),
            messages: row.messages,
            mean_ns: row.mean_ns,
            per_message_ns: per_message(row.mean_ns, row.messages),
        })
        .collect();
    let memory: Vec<MemoryPoint> = results
        .memory
        .measured
        .iter()
        .map(|row| MemoryPoint {
            size: row.size.clone(),
            messages: row.message_count,
            measured_mean_bytes: row.measured_mean_bytes,
            analytical_total_bytes: row.analytical_total_bytes,
            measured_analytical_total_ratio: row.measured_analytical_total_ratio,
            samples: row.samples,
        })
        .collect();
    serde_json::to_value(json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "source": DEFAULT_RESULTS,
        "wire": wire,
        "signing": signing,
        "verifying": verifying,
        "memory": memory,
    }))
    .map_err(|source| ReportError::JsonDecode {
        path: PathBuf::from("<generated>"),
        source,
    })
}

fn per_message(total_ns: f64, messages: u64) -> f64 {
    if messages == 0 {
        0.0
    } else {
        total_ns / messages as f64
    }
}

fn wire_reduction_c_vs_a(results: &crate::results::ClimbResults, size: &str) -> f64 {
    results
        .comparisons
        .iter()
        .find(|row| row.size.eq_ignore_ascii_case(size))
        .and_then(|row| row.wire_reduction_c_vs_a_percent)
        .unwrap_or(0.0)
}

#[derive(Serialize)]
struct LadderNode {
    level: u32,
    index: u64,
    first_leaf: u64,
    last_leaf: u64,
}

#[derive(Serialize)]
struct AuthStep {
    level: u32,
    index: u64,
    side: &'static str,
}

#[derive(Serialize)]
struct LadderLeaf {
    index: u64,
    condensed_bytes: usize,
    auth_path: Vec<AuthStep>,
}

#[derive(Serialize)]
struct LadderRungOut {
    left: u64,
    right: u64,
    size: u64,
}

#[derive(Serialize)]
struct LadderTree {
    leaf_count: u64,
    levels: u32,
    rung_count: usize,
    ladder_bytes: usize,
    model_ladder_bytes: usize,
    signed_ladder_bytes: usize,
    plain_signature_bytes: usize,
    condensed_min: usize,
    condensed_max: usize,
    condensed_total: u64,
    rungs: Vec<LadderRungOut>,
    nodes: Vec<LadderNode>,
    leaves: Vec<LadderLeaf>,
}

/// Build `ladder.json` for one recorded fixture.
///
/// Every node, rung and authentication path is derived from the closed-form
/// tree shape, then checked against the recorded row. Any disagreement — a
/// rung boundary, a condensed length, the ladder byte count or the signed
/// ladder arithmetic — is an error rather than a value the site would show.
pub fn build_ladder(document: &LadderVerification) -> Result<Value, ReportError> {
    let mut trees = Vec::new();
    for leaf_count in [8u64, 16] {
        let row = document.row(leaf_count as usize).ok_or_else(|| {
            malformed(
                Path::new(DEFAULT_LADDER),
                format!("no recorded row for N={leaf_count}"),
            )
        })?;
        let modelled_rungs = climb_analytical::rungs(leaf_count);
        if modelled_rungs.len() != row.rungs.len() {
            return Err(malformed(
                Path::new(DEFAULT_LADDER),
                format!("N={leaf_count}: rung count disagrees with the model"),
            ));
        }
        for (modelled, recorded) in modelled_rungs.iter().zip(row.rungs.iter()) {
            if modelled.left != recorded.left || modelled.right != recorded.right {
                return Err(malformed(
                    Path::new(DEFAULT_LADDER),
                    format!("N={leaf_count}: rung boundary disagrees with the model"),
                ));
            }
        }
        let model_ladder_bytes = climb_analytical::predicted_ladder_len(leaf_count);
        if model_ladder_bytes != row.ladder_buffer_len {
            return Err(malformed(
                Path::new(DEFAULT_LADDER),
                format!(
                    "N={leaf_count}: model ladder {model_ladder_bytes} B, recorded {} B",
                    row.ladder_buffer_len
                ),
            ));
        }
        let signature_bytes = row.signature_len as usize;
        let expected_signed = row
            .ladder_buffer_len
            .saturating_add(4)
            .saturating_add(signature_bytes);
        if expected_signed != row.signed_ladder_len {
            return Err(malformed(
                Path::new(DEFAULT_LADDER),
                format!(
                    "N={leaf_count}: signed ladder {} B is not ladder+4+signature ({expected_signed} B)",
                    row.signed_ladder_len
                ),
            ));
        }

        let levels = leaf_count.trailing_zeros();
        let mut nodes = Vec::new();
        for level in 0..=levels {
            let count = leaf_count >> level;
            for index in 0..count {
                let first_leaf = index << level;
                let width = 1u64 << level;
                nodes.push(LadderNode {
                    level,
                    index,
                    first_leaf,
                    last_leaf: first_leaf + width - 1,
                });
            }
        }

        let mut leaves = Vec::new();
        let mut condensed_total = 0u64;
        let mut condensed_min = usize::MAX;
        let mut condensed_max = 0usize;
        for index in 0..leaf_count {
            let Some(condensed_bytes) =
                climb_analytical::predicted_condensed_len(leaf_count, index)
            else {
                return Err(malformed(
                    Path::new(DEFAULT_LADDER),
                    format!("N={leaf_count}: no model path for leaf {index}"),
                ));
            };
            if condensed_bytes < row.condensed.min || condensed_bytes > row.condensed.max {
                return Err(malformed(
                    Path::new(DEFAULT_LADDER),
                    format!(
                        "N={leaf_count}: leaf {index} path {condensed_bytes} B outside recorded {}-{} B",
                        row.condensed.min, row.condensed.max
                    ),
                ));
            }
            condensed_total = condensed_total.saturating_add(condensed_bytes as u64);
            condensed_min = condensed_min.min(condensed_bytes);
            condensed_max = condensed_max.max(condensed_bytes);
            let mut auth_path = Vec::with_capacity(levels as usize);
            for level in 0..levels {
                let node_index = index >> level;
                auth_path.push(AuthStep {
                    level,
                    index: node_index ^ 1,
                    side: if node_index % 2 == 0 { "right" } else { "left" },
                });
            }
            leaves.push(LadderLeaf {
                index,
                condensed_bytes,
                auth_path,
            });
        }
        if condensed_total != row.condensed.total
            || condensed_min != row.condensed.min
            || condensed_max != row.condensed.max
        {
            return Err(malformed(
                Path::new(DEFAULT_LADDER),
                format!("N={leaf_count}: condensed lengths disagree with the recorded row"),
            ));
        }

        trees.push(LadderTree {
            leaf_count,
            levels,
            rung_count: modelled_rungs.len(),
            ladder_bytes: row.ladder_buffer_len,
            model_ladder_bytes,
            signed_ladder_bytes: row.signed_ladder_len,
            plain_signature_bytes: signature_bytes,
            condensed_min,
            condensed_max,
            condensed_total,
            rungs: modelled_rungs
                .iter()
                .map(|rung| LadderRungOut {
                    left: rung.left,
                    right: rung.right,
                    size: rung.size(),
                })
                .collect(),
            nodes,
            leaves,
        });
    }

    serde_json::to_value(json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "source": DEFAULT_LADDER,
        "recorded_at": document.recorded_at.clone(),
        "algorithm": document.algorithm.clone(),
        "hash_size": document.hash_size,
        "trees": trees,
    }))
    .map_err(|source| ReportError::JsonDecode {
        path: PathBuf::from("<generated>"),
        source,
    })
}

/// Build `commits.json` from the committed commit graph.
pub fn build_commits(graph: Value) -> Value {
    json!({
        "schema_version": SITE_DATA_SCHEMA_VERSION,
        "source": DEFAULT_COMMITS,
        "graph": graph,
    })
}

/// Render every generated document in memory. The result is ordered by
/// [`OUTPUT_FILES`].
pub fn render(root: &Path) -> Result<Vec<(String, String)>, ReportError> {
    let results_path = root.join(DEFAULT_RESULTS);
    let results = read_json(&results_path)?;
    let quote_sheet = read_text(&root.join(DEFAULT_QUOTE_SHEET))?;
    let typed: crate::results::ClimbResults =
        serde_json::from_value(results.clone()).map_err(|source| ReportError::JsonDecode {
            path: results_path.clone(),
            source,
        })?;
    let registry = crate::html::claims(&typed)?;
    let claims_doc = build_claims(&results, &registry);
    let passed = claims_doc
        .get("passed")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let total = claims_doc.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;

    let ladder_path = root.join(DEFAULT_LADDER);
    let ladder_raw = read_json(&ladder_path)?;
    let facts = build_facts(&results, &ladder_raw, quote_sheet.as_deref(), passed, total)?;
    let provenance_text =
        read_text(&root.join(DEFAULT_PROVENANCE))?.ok_or_else(|| ReportError::Io {
            path: root.join(DEFAULT_PROVENANCE),
            source: std::io::Error::new(ErrorKind::NotFound, "provenance file not found"),
        })?;
    let environment = read_json(&root.join(DEFAULT_ENVIRONMENT))?;
    let provenance = build_provenance(&provenance_text, &environment)?;
    let series = build_series(&typed)?;
    let ladder_text: LadderVerification =
        serde_json::from_value(ladder_raw).map_err(|source| ReportError::JsonDecode {
            path: ladder_path.clone(),
            source,
        })?;
    let ladder = build_ladder(&ladder_text)?;
    let graph = read_json(&root.join(DEFAULT_COMMITS))?;
    let commits = build_commits(graph);

    let documents = [facts, claims_doc, provenance, series, ladder, commits];
    let mut out = Vec::with_capacity(documents.len());
    for (name, document) in OUTPUT_FILES.iter().zip(documents) {
        let sealed = seal(document)?;
        let mut text =
            serde_json::to_string_pretty(&sealed).map_err(|source| ReportError::JsonDecode {
                path: PathBuf::from("<generated>"),
                source,
            })?;
        text.push('\n');
        out.push(((*name).to_string(), text));
    }
    Ok(out)
}

/// Write every generated document under `out_dir`.
pub fn write(root: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, ReportError> {
    fs::create_dir_all(out_dir).map_err(|source| ReportError::Io {
        path: out_dir.to_path_buf(),
        source,
    })?;
    let mut written = Vec::new();
    for (name, text) in render(root)? {
        let path = out_dir.join(&name);
        fs::write(&path, text).map_err(|source| ReportError::Io {
            path: path.clone(),
            source,
        })?;
        written.push(path);
    }
    Ok(written)
}

/// Names of generated files whose committed bytes differ from a fresh render,
/// or that are missing entirely.
pub fn stale_files(root: &Path, out_dir: &Path) -> Result<Vec<String>, ReportError> {
    let mut stale = Vec::new();
    for (name, text) in render(root)? {
        let path = out_dir.join(&name);
        match fs::read_to_string(&path) {
            Ok(committed) if committed == text => {}
            Ok(_) => stale.push(format!("{name} (stale)")),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                stale.push(format!("{name} (missing)"));
            }
            Err(source) => {
                return Err(ReportError::Io {
                    path: path.clone(),
                    source,
                });
            }
        }
    }
    Ok(stale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root")
    }

    fn results() -> Value {
        read_json(&root().join(DEFAULT_RESULTS)).expect("results artifact")
    }

    fn fixture() -> LadderVerification {
        let text = std::fs::read_to_string(root().join(DEFAULT_LADDER)).expect("ladder fixture");
        serde_json::from_str(&text).expect("ladder fixture parses")
    }

    fn fixture_value() -> Value {
        read_json(&root().join(DEFAULT_LADDER)).expect("ladder fixture")
    }

    #[test]
    fn every_registry_fact_has_a_distinct_id() {
        let mut ids: Vec<&str> = FACT_SPECS.iter().map(|spec| spec.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FACT_SPECS.len());
        assert_eq!(ids.len(), 18);
    }

    #[test]
    fn facts_render_the_quote_sheet_values() {
        let results = results();
        let ladder = fixture_value();
        let quote = read_text(&root().join(DEFAULT_QUOTE_SHEET)).expect("quote sheet read");
        let facts = build_facts(&results, &ladder, quote.as_deref(), 852, 852).expect("facts");
        let find = |id: &str| -> &Value {
            facts
                .get("facts")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
                })
                .expect("fact present")
        };
        assert_eq!(
            find("F-wire-medium-c")
                .get("display")
                .and_then(Value::as_str),
            Some("2,743,352")
        );
        assert_eq!(
            find("F-plain-signature-bytes")
                .get("display")
                .and_then(Value::as_str),
            Some("7,856")
        );
        assert_eq!(
            find("F-condensed-path-bytes")
                .get("display")
                .and_then(Value::as_str),
            Some("124")
        );
        assert_eq!(
            find("F-sign-medium-c-vs-a")
                .get("display")
                .and_then(Value::as_str),
            Some("9,434")
        );
        assert_eq!(
            find("F-verify-medium-c-vs-a")
                .get("display")
                .and_then(Value::as_str),
            Some("77.34")
        );
        assert_eq!(
            find("F-mem-small-ratio")
                .get("display")
                .and_then(Value::as_str),
            Some("1.628")
        );
        assert_eq!(
            find("F-dns-www-a").get("display").and_then(Value::as_str),
            Some("97.307")
        );
        assert_eq!(
            find("F-wire-medium-c-vs-a")
                .get("display")
                .and_then(Value::as_str),
            Some("96.508")
        );
        assert_eq!(
            find("F-udp-c-fit").get("display").and_then(Value::as_str),
            Some("11 of 12")
        );
        assert_eq!(
            find("F-udp-payload").get("display").and_then(Value::as_str),
            Some("1,232")
        );
        assert_eq!(
            find("F-claims-count").get("class").and_then(Value::as_str),
            Some("generated")
        );
        assert_eq!(
            find("F-claims-count")
                .get("display")
                .and_then(Value::as_str),
            Some("852 of 852")
        );
    }

    #[test]
    fn hero_byte_facts_point_at_the_eight_leaf_recorded_row() {
        let results = results();
        let ladder = fixture_value();
        let facts = build_facts(&results, &ladder, None, 852, 852).expect("facts");
        let rows = facts
            .get("facts")
            .and_then(Value::as_array)
            .expect("facts array");
        let find = |id: &str| -> &Value {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
                .expect("hero byte fact present")
        };

        let eight = ladder
            .get("sizes")
            .and_then(Value::as_array)
            .and_then(|sizes| {
                sizes
                    .iter()
                    .position(|row| row.get("leaf_count").and_then(Value::as_u64) == Some(8))
            })
            .expect("recorded N=8 row");

        let plain = find("F-plain-signature-bytes");
        assert_eq!(
            plain.get("pointer").and_then(Value::as_str),
            Some(format!("/sizes/{eight}/signature_len").as_str())
        );
        assert_eq!(
            plain.get("raw").and_then(Value::as_f64),
            ladder
                .pointer(&format!("/sizes/{eight}/signature_len"))
                .and_then(Value::as_f64)
        );
        assert_eq!(
            plain.get("source").and_then(Value::as_str),
            Some(DEFAULT_LADDER)
        );

        let condensed = find("F-condensed-path-bytes");
        assert_eq!(
            condensed.get("pointer").and_then(Value::as_str),
            Some(format!("/sizes/{eight}/condensed/min").as_str())
        );
        assert_eq!(
            condensed.get("raw").and_then(Value::as_f64),
            ladder
                .pointer(&format!("/sizes/{eight}/condensed/min"))
                .and_then(Value::as_f64)
        );
        assert_eq!(condensed.get("unit").and_then(Value::as_str), Some("B"));
    }

    #[test]
    fn extrapolated_values_carry_the_approximation_mark() {
        assert_eq!(with_approximation("derived", "9,434".into()), "9,434");
        assert_eq!(
            with_approximation("extrapolated", "992.4".into()),
            "\u{2248}992.4"
        );
    }

    #[test]
    fn formatter_conventions_match_the_tui_and_the_quote_sheet() {
        assert_eq!(format_ratio(9434.44263867255), "9,434");
        assert_eq!(format_ratio(77.33947336502698), "77.34");
        assert_eq!(format_ratio(1.6281767047921734), "1.628");
        assert_eq!(format_percent(96.50795315682281), "96.508");
        assert_eq!(format_percent(97.30747196988297), "97.307");
        assert_eq!(format_count(78560000.0), "78,560,000");
        assert_eq!(format_count(2743352.0), "2,743,352");
    }

    #[test]
    fn ladder_paths_match_the_recorded_rows_for_eight_and_sixteen() {
        let document = fixture();
        let ladder = build_ladder(&document).expect("ladder builds");
        let trees = ladder
            .get("trees")
            .and_then(Value::as_array)
            .expect("trees");
        assert_eq!(trees.len(), 2);
        for tree in trees {
            let n = tree
                .get("leaf_count")
                .and_then(Value::as_u64)
                .expect("leaf count");
            let levels = n.trailing_zeros();
            let node_count = 2 * n - 1;
            assert_eq!(
                tree.get("nodes").and_then(Value::as_array).map(Vec::len),
                Some(node_count as usize)
            );
            let row = document.row(n as usize).expect("recorded row");
            assert_eq!(
                tree.get("ladder_bytes").and_then(Value::as_u64),
                Some(row.ladder_buffer_len as u64)
            );
            assert_eq!(
                tree.get("plain_signature_bytes").and_then(Value::as_u64),
                Some(row.signature_len as u64)
            );
            let leaves = tree
                .get("leaves")
                .and_then(Value::as_array)
                .expect("leaves");
            assert_eq!(leaves.len(), n as usize);
            for leaf in leaves {
                let path = leaf
                    .get("auth_path")
                    .and_then(Value::as_array)
                    .expect("path");
                assert_eq!(path.len(), levels as usize);
                let leaf_index = leaf
                    .get("index")
                    .and_then(Value::as_u64)
                    .expect("leaf index");
                let mut seen = leaf_index;
                for step in path {
                    let level = step.get("level").and_then(Value::as_u64).expect("level") as u32;
                    let index = step.get("index").and_then(Value::as_u64).expect("index");
                    // The sibling is adjacent to the node containing the leaf
                    // at this level, and it is never the leaf's own ancestor.
                    assert_eq!(index, (seen ^ 1), "N={n} leaf {leaf_index} level {level}");
                    seen >>= 1;
                }
                let expected =
                    climb_analytical::predicted_condensed_len(n, leaf_index).expect("model");
                assert_eq!(
                    leaf.get("condensed_bytes").and_then(Value::as_u64),
                    Some(expected as u64)
                );
            }
        }
    }

    #[test]
    fn ladder_build_rejects_a_tampered_fixture() {
        let mut document = fixture();
        let row = document
            .sizes
            .iter_mut()
            .find(|row| row.leaf_count == 8)
            .unwrap();
        row.ladder_buffer_len += 4;
        assert!(build_ladder(&document).is_err());
    }

    #[test]
    fn generated_documents_are_deterministic_and_sealed() {
        let first = render(&root()).expect("render");
        let second = render(&root()).expect("render");
        assert_eq!(first, second);
        assert_eq!(first.len(), OUTPUT_FILES.len());
        for ((name, text), expected) in first.iter().zip(OUTPUT_FILES) {
            assert_eq!(name, expected);
            let value: Value = serde_json::from_str(text).expect("generated JSON parses");
            assert!(seal_is_valid(&value), "{name} seal must verify");
        }
    }

    #[test]
    fn stale_check_reports_a_missing_or_changed_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let out = temp.path().join("generated");
        write(&root(), &out).expect("write");
        assert!(stale_files(&root(), &out).expect("stale").is_empty());

        std::fs::write(out.join("facts.json"), "{}\n").expect("tamper");
        let stale = stale_files(&root(), &out).expect("stale");
        assert_eq!(stale, vec!["facts.json (stale)".to_string()]);

        std::fs::remove_file(out.join("ladder.json")).expect("remove");
        let stale = stale_files(&root(), &out).expect("stale");
        assert!(stale.contains(&"ladder.json (missing)".to_string()));
    }

    #[test]
    fn claims_registry_resolves_every_claim_against_the_artifact() {
        let results = results();
        let typed: crate::results::ClimbResults =
            serde_json::from_value(results.clone()).expect("typed results");
        let registry = crate::html::claims(&typed).expect("claims");
        let document = build_claims(&results, &registry);
        assert_eq!(
            document.get("passed").and_then(Value::as_u64),
            document.get("total").and_then(Value::as_u64)
        );
        assert!(document
            .get("claims")
            .and_then(Value::as_array)
            .is_some_and(|claims| claims
                .iter()
                .all(|claim| claim.get("verified").and_then(Value::as_bool) == Some(true))));
    }

    #[test]
    fn provenance_reads_the_recorded_commit_and_hashes_the_host() {
        let provenance = read_text(&root().join(DEFAULT_PROVENANCE))
            .expect("provenance read")
            .unwrap();
        let environment = read_json(&root().join(DEFAULT_ENVIRONMENT)).expect("environment");
        let document = build_provenance(&provenance, &environment).expect("provenance");
        assert_eq!(
            document.get("commit").and_then(Value::as_str),
            Some("fea347548a3f182bb2392c6c3ecdcebed275f882")
        );
        assert_eq!(
            document
                .get("submodules")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        let hash = document
            .get("host_fingerprint_sha256")
            .and_then(Value::as_str)
            .expect("host hash");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
