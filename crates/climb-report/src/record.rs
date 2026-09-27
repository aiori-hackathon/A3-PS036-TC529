//! Criterion on-disk output parsing.
//!
//! Reads `benchmark.json` + `estimates.json` from Criterion's leaf directories
//! and produces a flat [`CriterionRecord`] per benchmark.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::ReportError;

// ---------------------------------------------------------------------------
// Serde types — private, only used during deserialization
// ---------------------------------------------------------------------------

/// A single entry from Criterion's `benchmark.json`.
#[derive(Debug, Deserialize)]
struct BenchmarkJson {
    group_id: String,
    function_id: String,
    value_str: String,
    #[serde(default)]
    throughput: Option<Throughput>,
    full_id: String,
}

/// Throughput in `benchmark.json` — either element count or byte count.
///
/// Field names match Criterion's JSON keys exactly (PascalCase).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
// Criterion's JSON uses PascalCase keys; renaming them would break deserialization.
#[allow(non_snake_case)]
enum Throughput {
    /// Throughput measured in elements (messages, iterations, etc.).
    Elements { Elements: u64 },
    /// Throughput measured in bytes.
    Bytes { Bytes: u64 },
}

impl Throughput {
    fn count(&self) -> u64 {
        match self {
            Throughput::Elements { Elements } => *Elements,
            Throughput::Bytes { Bytes } => *Bytes,
        }
    }
}

/// Top-level structure of Criterion's `estimates.json`.
#[derive(Debug, Deserialize)]
struct EstimatesJson {
    mean: MeanEstimate,
}

/// Statistical estimate for a single metric (e.g. mean, median).
#[derive(Debug, Deserialize)]
struct MeanEstimate {
    confidence_interval: ConfidenceInterval,
    point_estimate: f64,
}

/// A bootstrap confidence interval.
#[derive(Debug, Deserialize)]
struct ConfidenceInterval {
    confidence_level: f64,
    lower_bound: f64,
    upper_bound: f64,
}

// ---------------------------------------------------------------------------
// Public value types
// ---------------------------------------------------------------------------

/// A single parsed Criterion benchmark record.
///
/// Contains the identifying metadata (group, function, parameter value) and
/// the mean iteration time with its 95% confidence interval (all in nanoseconds).
#[derive(Debug, Clone, PartialEq)]
pub struct CriterionRecord {
    /// Criterion benchmark group id (e.g. `"signing_small_with_mtl"`).
    pub group_id: String,
    /// Criterion benchmark function id (e.g. `"sign_batch"`).
    pub function_id: String,
    /// The parameter-string value (e.g. `"100"`).
    pub value_str: String,
    /// Fully-qualified Criterion benchmark id.
    pub full_id: String,
    /// Number of elements (messages), if reported via throughput.
    pub elements: Option<u64>,
    /// Mean iteration time in nanoseconds.
    pub mean_ns: f64,
    /// Lower bound of the 95% confidence interval (nanoseconds).
    pub mean_ci_lower_ns: f64,
    /// Upper bound of the 95% confidence interval (nanoseconds).
    pub mean_ci_upper_ns: f64,
    /// Confidence level (nominally 0.95).
    pub confidence_level: f64,
}

// ---------------------------------------------------------------------------
// Parsing functions
// ---------------------------------------------------------------------------

const BENCHMARK_JSON: &str = "benchmark.json";
const ESTIMATES_JSON: &str = "estimates.json";

/// Read a single Criterion leaf directory.
///
/// Expects `benchmark.json` and `estimates.json` to both exist in `dir`.
/// Returns an error if either is missing or cannot be parsed.
///
/// # Errors
///
/// * [`ReportError::MissingFile`] — if either file is absent.
/// * [`ReportError::JsonDecode`] — if either file cannot be deserialized.
pub fn read_leaf(dir: &Path) -> Result<CriterionRecord, ReportError> {
    let benchmark_path = dir.join(BENCHMARK_JSON);
    let estimates_path = dir.join(ESTIMATES_JSON);

    if !benchmark_path.is_file() {
        return Err(ReportError::MissingFile {
            file: BENCHMARK_JSON,
            dir: dir.to_path_buf(),
        });
    }
    if !estimates_path.is_file() {
        return Err(ReportError::MissingFile {
            file: ESTIMATES_JSON,
            dir: dir.to_path_buf(),
        });
    }

    let bench_raw = std::fs::read_to_string(&benchmark_path).map_err(|e| ReportError::Io {
        path: benchmark_path.clone(),
        source: e,
    })?;
    let bench: BenchmarkJson =
        serde_json::from_str(&bench_raw).map_err(|e| ReportError::JsonDecode {
            path: benchmark_path,
            source: e,
        })?;

    let est_raw = std::fs::read_to_string(&estimates_path).map_err(|e| ReportError::Io {
        path: estimates_path.clone(),
        source: e,
    })?;
    let est: EstimatesJson =
        serde_json::from_str(&est_raw).map_err(|e| ReportError::JsonDecode {
            path: estimates_path,
            source: e,
        })?;

    Ok(CriterionRecord {
        group_id: bench.group_id,
        function_id: bench.function_id,
        value_str: bench.value_str,
        full_id: bench.full_id,
        elements: bench.throughput.as_ref().map(Throughput::count),
        mean_ns: est.mean.point_estimate,
        mean_ci_lower_ns: est.mean.confidence_interval.lower_bound,
        mean_ci_upper_ns: est.mean.confidence_interval.upper_bound,
        confidence_level: est.mean.confidence_interval.confidence_level,
    })
}

/// Recursively walk `root` and collect records from every directory
/// whose basename equals `baseline` and that contains both
/// `benchmark.json` and `estimates.json`.
///
/// Output is sorted deterministically by `(group_id, function_id, value_str)`.
///
/// Returns [`ReportError::MissingRoot`] if `root` does not exist.
pub fn scan_criterion(root: &Path, baseline: &str) -> Result<Vec<CriterionRecord>, ReportError> {
    if !root.is_dir() {
        return Err(ReportError::MissingRoot {
            path: root.to_path_buf(),
        });
    }

    let mut records: Vec<CriterionRecord> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    // Seed the queue with the root's immediate children.
    match std::fs::read_dir(root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| ReportError::Io {
                    path: root.to_path_buf(),
                    source: e,
                })?;
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                }
            }
        }
        Err(e) => {
            return Err(ReportError::Io {
                path: root.to_path_buf(),
                source: e,
            });
        }
    };

    // Breadth-first traversal — avoid unbounded recursion.
    let mut idx = 0;
    while idx < dirs.len() {
        let dir = &dirs[idx];

        // Is this directory the target baseline?
        let is_baseline = dir.file_name().and_then(|n| n.to_str()) == Some(baseline);

        if is_baseline {
            // Check for the expected files.
            let bench_file = dir.join(BENCHMARK_JSON);
            let est_file = dir.join(ESTIMATES_JSON);
            if bench_file.is_file() && est_file.is_file() {
                let record = read_leaf(dir)?;
                records.push(record);
            }
            // If the baseline dir lacks the files, it is not a benchmark
            // leaf — just skip it (it may be a Criterion ancillary dir).
        }

        // Push children of this directory onto the queue.
        let child_dirs = match std::fs::read_dir(dir) {
            Ok(children) => {
                let mut subdirs = Vec::new();
                for child in children {
                    let child = child.map_err(|e| ReportError::Io {
                        path: dir.clone(),
                        source: e,
                    })?;
                    let child_path = child.path();
                    if child_path.is_dir() {
                        subdirs.push(child_path);
                    }
                }
                subdirs
            }
            Err(e) => {
                return Err(ReportError::Io {
                    path: dir.clone(),
                    source: e,
                });
            }
        };
        dirs.extend(child_dirs);

        idx += 1;
    }

    // Deterministic sort.
    records.sort_by(|a, b| {
        a.group_id
            .cmp(&b.group_id)
            .then_with(|| a.function_id.cmp(&b.function_id))
            .then_with(|| a.value_str.cmp(&b.value_str))
    });

    Ok(records)
}
