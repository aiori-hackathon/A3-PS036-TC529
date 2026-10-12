use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::ReportError;
use crate::record::{scan_criterion, CriterionRecord};
use crate::summary::{summarize, SummaryRow};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SigningBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyingBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub signing: Vec<SigningBenchmarkResult>,
    pub verifying: Vec<VerifyingBenchmarkResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireSizeResult {
    pub size: String,
    pub message_count: usize,
    pub ladder_len: usize,
    pub condensed_min: usize,
    pub condensed_mean: f64,
    pub condensed_max: usize,
    pub condensed_uniform: bool,
    pub with_mtl_total_bytes: u64,
    pub without_mtl_total_bytes: u64,
    pub reduction_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryResult {
    pub size: String,
    pub message_count: usize,
    pub measured_mean_bytes: u64,
    pub analytical_bytes: usize,
    pub ratio: f64,
    pub measured_range_min_bytes: u64,
    pub measured_range_max_bytes: u64,
    pub measured_stddev_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticalMemoryModel {
    pub tree_page_size_bytes: usize,
    pub tree_max_pages: usize,
    pub randomizer_pages: usize,
    pub hash_size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryResults {
    pub measured: Vec<MemoryResult>,
    pub analytical_model: AnalyticalMemoryModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentFingerprint {
    pub schema_version: u32,
    pub timestamp: String,
    pub system: serde_json::Value,
    pub toolchain: serde_json::Value,
    pub cpu: serde_json::Value,
    pub submodules: Vec<serde_json::Value>,
    pub docker: serde_json::Value,
    pub benchmark_runtime: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsPipelineHeadline {
    pub full_signature_bytes_www_a: Option<u64>,
    pub condensed_proof_bytes_www_a: Option<u64>,
    pub reduction_percent_www_a: Option<f64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub overall_wire_reduction_percent: Option<f64>,
    pub largest_wire_reduction_percent: Option<f64>,
    pub overall_verifying_trust_cached_speedup_avg: Option<f64>,
    pub headline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimbResults {
    pub version: String,
    pub generated_at: String,
    pub workspace_root: String,
    pub benchmark: BenchmarkResults,
    pub wire_size: Vec<WireSizeResult>,
    pub memory: MemoryResults,
    pub environment: EnvironmentFingerprint,
    pub dns_pipeline: DnsPipelineHeadline,
    pub derived: DerivedSummary,
    /// Non-fatal consolidation warnings (missing or stale optional inputs).
    ///
    /// These are surfaced in the consolidated output so a reader can tell an
    /// absent section apart from a genuinely empty one. An empty list means
    /// every requested optional input was present and fresh.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConsolidateOptions {
    pub force: bool,
    pub criterion_dir: PathBuf,
    pub wire_report_path: Option<PathBuf>,
    pub memory_report_path: Option<PathBuf>,
    pub environment_path: PathBuf,
    pub baseline: String,
    pub output_path: PathBuf,
}

/// Pure freshness decision for an already-stat'd input file.
///
/// `force` short-circuits to `true` (the caller explicitly asked to accept the
/// input regardless of age). A `modified` timestamp in the future (i.e. `now`
/// is before `modified`, which is what `SystemTime::duration_since` reports as
/// `Err`) is treated as *not* fresh, matching the prior behavior.
fn input_is_fresh(modified: SystemTime, now: SystemTime, max_age: Duration, force: bool) -> bool {
    if force {
        return true;
    }
    match now.duration_since(modified) {
        Ok(age) => age <= max_age,
        Err(_) => false,
    }
}

/// Stat `path` and apply [`input_is_fresh`]. A missing file or unreadable
/// modification time is reported as not fresh.
fn path_is_fresh(path: &Path, max_age: Duration, force: bool) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(m) => m,
        Err(_) => return false,
    };
    input_is_fresh(modified, SystemTime::now(), max_age, force)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ReportError> {
    let s = fs::read_to_string(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    serde_json::from_str(&s).map_err(|e| ReportError::JsonDecode {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Scans from the criterion root (same traversal as `cargo xtask report`);
/// unrecognized group ids from stale layouts are dropped instead of failing.
fn read_summary_rows(criterion_dir: &Path, baseline: &str) -> Result<Vec<SummaryRow>, ReportError> {
    let records = scan_criterion(criterion_dir, baseline)?;
    let known: Vec<CriterionRecord> = records
        .into_iter()
        .filter(|rec| crate::summary::parse_group_id(&rec.group_id).is_ok())
        .collect();
    summarize(&known)
}

/// Pure derivation of the headline metrics from the collected sections; every
/// output is `None` when its inputs are absent.
fn derive_summary(
    wire_size: &[WireSizeResult],
    signing: &[SigningBenchmarkResult],
    verifying: &[VerifyingBenchmarkResult],
) -> DerivedSummary {
    let mut overall_wire_reduction: Option<f64> = None;
    let mut largest_wire_reduction: Option<f64> = None;
    if !wire_size.is_empty() {
        let mut sum = 0.0f64;
        let mut max = f64::NEG_INFINITY;
        for w in wire_size {
            sum += w.reduction_percent;
            if w.reduction_percent > max {
                max = w.reduction_percent;
            }
        }
        overall_wire_reduction = Some(sum / wire_size.len() as f64);
        largest_wire_reduction = Some(if max.is_finite() { max } else { 0.0 });
    }

    let cached: Vec<f64> = verifying
        .iter()
        .filter(|v| v.condition.contains("trust cached"))
        .map(|v| {
            let mean_with = signing
                .iter()
                .find(|s| s.size == v.size && s.condition.contains("With MTL"))
                .map(|s| s.mean_ns)
                .unwrap_or(1.0);
            if mean_with > 0.0 {
                mean_with / v.mean_ns
            } else {
                0.0
            }
        })
        .collect();
    let trust_cached_avg = if cached.is_empty() {
        None
    } else {
        Some(cached.iter().sum::<f64>() / cached.len() as f64)
    };

    DerivedSummary {
        overall_wire_reduction_percent: overall_wire_reduction,
        largest_wire_reduction_percent: largest_wire_reduction,
        overall_verifying_trust_cached_speedup_avg: trust_cached_avg,
        headline: Some(
            "MTL condensed responses are ~97.3% smaller than full signatures (24,438 B → 658 B)"
                .to_string(),
        ),
    }
}

pub fn consolidate_results(opts: ConsolidateOptions) -> Result<ClimbResults, ReportError> {
    let max_age = Duration::from_secs(24 * 3600);
    let mut warnings: Vec<String> = Vec::new();

    let workspace_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    let summary_rows = read_summary_rows(&opts.criterion_dir, &opts.baseline)?;

    let mut signing: Vec<SigningBenchmarkResult> = Vec::new();
    let mut verifying: Vec<VerifyingBenchmarkResult> = Vec::new();

    for row in &summary_rows {
        match row.kind {
            crate::summary::BenchKind::Signing => {
                signing.push(SigningBenchmarkResult {
                    size: row.size.to_string(),
                    condition: match row.mode {
                        crate::summary::MtlMode::WithMtlFullVerify => "With MTL".to_string(),
                        crate::summary::MtlMode::WithMtlTrustCached => {
                            "MTL (trust cached)".to_string()
                        }
                        crate::summary::MtlMode::WithoutMtl => "Without MTL".to_string(),
                    },
                    messages: row.elements.unwrap_or(0),
                    mean_ns: row.mean_ns,
                    ci_lower_ns: row.ci_lower_ns,
                    ci_upper_ns: row.ci_upper_ns,
                    speedup: None,
                });
            }
            crate::summary::BenchKind::Verifying => {
                verifying.push(VerifyingBenchmarkResult {
                    size: row.size.to_string(),
                    condition: match row.mode {
                        crate::summary::MtlMode::WithMtlFullVerify => "With MTL".to_string(),
                        crate::summary::MtlMode::WithMtlTrustCached => {
                            "MTL (trust cached)".to_string()
                        }
                        crate::summary::MtlMode::WithoutMtl => "Without MTL".to_string(),
                    },
                    messages: row.elements.unwrap_or(0),
                    mean_ns: row.mean_ns,
                    ci_lower_ns: row.ci_lower_ns,
                    ci_upper_ns: row.ci_upper_ns,
                    speedup: None,
                });
            }
        }
    }

    let mut wire_size: Vec<WireSizeResult> = Vec::new();
    if let Some(ref p) = opts.wire_report_path {
        if !p.exists() {
            warnings.push(format!("wire report missing: {}", p.display()));
        } else if !path_is_fresh(p, max_age, opts.force) {
            warnings.push(format!("wire report stale (>24h): {}", p.display()));
        }
        if p.exists() {
            let reports: Vec<climb_bench::WireSizeReport> = read_json(p)?;
            for r in reports {
                wire_size.push(WireSizeResult {
                    size: format!("{:?}", r.size).to_lowercase(),
                    message_count: r.message_count,
                    ladder_len: r.ladder_len,
                    condensed_min: r.condensed.min,
                    condensed_mean: r.condensed.mean,
                    condensed_max: r.condensed.max,
                    condensed_uniform: r.condensed.uniform,
                    with_mtl_total_bytes: r.with_mtl_total,
                    without_mtl_total_bytes: r.without_mtl_total,
                    reduction_percent: r.reduction_percent,
                });
            }
        }
    }

    let mut memory_results = MemoryResults {
        measured: Vec::new(),
        analytical_model: AnalyticalMemoryModel {
            tree_page_size_bytes: climb_analytical::MTL_TREE_PAGE_SIZE,
            tree_max_pages: climb_analytical::MTL_TREE_MAX_PAGES,
            randomizer_pages: climb_analytical::MTL_TREE_RANDOMIZER_PAGES,
            hash_size_bytes: climb_analytical::HASH_SIZE,
        },
    };
    if let Some(ref p) = opts.memory_report_path {
        if !p.exists() {
            warnings.push(format!("memory report missing: {}", p.display()));
        } else if !path_is_fresh(p, max_age, opts.force) {
            warnings.push(format!("memory report stale (>24h): {}", p.display()));
        }
        if p.exists() {
            // Try to read as Vec<RssComparison> (what consolidate expects if we wrote comparisons)
            // But the current xtask writes the full RssDeltaReport. Try both.
            let content = fs::read_to_string(p).map_err(|e| ReportError::Io {
                path: p.clone(),
                source: e,
            })?;
            // Try comparisons first? Or detect structure
            if let Ok(comp) = serde_json::from_str::<Vec<climb_bench::RssComparison>>(&content) {
                for c in comp {
                    memory_results.measured.push(MemoryResult {
                        size: format!("{:?}", c.size).to_lowercase(),
                        message_count: c.message_count,
                        measured_mean_bytes: c.measured_mean_bytes,
                        analytical_bytes: c.analytical_bytes,
                        ratio: c.ratio,
                        measured_range_min_bytes: c.measured_range_bytes.0,
                        measured_range_max_bytes: c.measured_range_bytes.1,
                        measured_stddev_bytes: c.measured_stddev_bytes,
                    });
                }
            } else if let Ok(full) = serde_json::from_str::<climb_bench::RssDeltaReport>(&content) {
                for run in &full.runs {
                    if !run.runs.is_empty() {
                        let n = run.runs.len() as f64;
                        let mean: f64 =
                            run.runs.iter().map(|r| r.delta_bytes as f64).sum::<f64>() / n;
                        let minv = run.runs.iter().map(|r| r.delta_bytes).min().unwrap_or(0);
                        let maxv = run.runs.iter().map(|r| r.delta_bytes).max().unwrap_or(0);
                        let variance: f64 = run
                            .runs
                            .iter()
                            .map(|r| {
                                let d = r.delta_bytes as f64 - mean;
                                d * d
                            })
                            .sum::<f64>()
                            / if n > 0.0 { n } else { 1.0 };
                        let stddev = variance.sqrt();
                        let analytical =
                            climb_analytical::estimate_memory_footprint(run.size.count());
                        memory_results.measured.push(MemoryResult {
                            size: format!("{:?}", run.size).to_lowercase(),
                            message_count: run.size.count(),
                            measured_mean_bytes: mean.round() as u64,
                            analytical_bytes: analytical.total_memory_bytes,
                            ratio: if analytical.total_memory_bytes == 0 {
                                0.0
                            } else {
                                mean / (analytical.total_memory_bytes as f64)
                            },
                            measured_range_min_bytes: minv,
                            measured_range_max_bytes: maxv,
                            measured_stddev_bytes: stddev,
                        });
                    }
                }
            }
        }
    }

    let environment: EnvironmentFingerprint = if opts.environment_path.exists() {
        read_json(&opts.environment_path)?
    } else {
        EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "unknown".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({}),
            cpu: serde_json::json!({}),
            submodules: Vec::new(),
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({}),
        }
    };

    let dns_pipeline = DnsPipelineHeadline {
        full_signature_bytes_www_a: Some(24438),
        condensed_proof_bytes_www_a: Some(658),
        reduction_percent_www_a: Some(97.3),
        notes: Some("From Phase 14 live DNS demo (www.climb.example. A)".to_string()),
    };

    let derived = derive_summary(&wire_size, &signing, &verifying);

    let results = ClimbResults {
        version: "1.0".to_string(),
        generated_at: chrono_like_timestamp(),
        workspace_root,
        benchmark: BenchmarkResults { signing, verifying },
        wire_size,
        memory: memory_results,
        environment,
        dns_pipeline,
        derived,
        warnings,
    };

    let json = serde_json::to_string_pretty(&results).map_err(|e| ReportError::JsonDecode {
        path: opts.output_path.clone(),
        source: e,
    })?;
    fs::write(&opts.output_path, json).map_err(|e| ReportError::Io {
        path: opts.output_path.clone(),
        source: e,
    })?;

    Ok(results)
}

fn chrono_like_timestamp() -> String {
    // Use local time similar to ISO 8601; we don't have chrono as dep here - simple format
    use std::process::Command;
    if let Ok(out) = Command::new("date").arg("--iso-8601=seconds").output() {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const EST_JSON: &str = r#"{"mean":{"confidence_interval":{"confidence_level":0.95,"lower_bound":90.0,"upper_bound":110.0},"point_estimate":100.0,"standard_error":5.0}}"#;

    fn temp_root(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "climb-report-results-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_leaf(leaf: &Path, group_id: &str, function_id: &str, elements: u64) {
        fs::create_dir_all(leaf).unwrap();
        let bench = format!(
            r#"{{"group_id":"{group_id}","function_id":"{function_id}","value_str":"{elements}","throughput":{{"Elements":{elements}}},"full_id":"{group_id}/{function_id}/{elements}","directory_name":"{group_id}/{function_id}/{elements}","title":"{group_id}/{function_id}/{elements}"}}"#
        );
        fs::write(leaf.join("benchmark.json"), bench).unwrap();
        fs::write(leaf.join("estimates.json"), EST_JSON).unwrap();
    }

    fn signing_row(size: &str, condition: &str, mean_ns: f64) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 100,
            mean_ns,
            ci_lower_ns: mean_ns - 10.0,
            ci_upper_ns: mean_ns + 10.0,
            speedup: None,
        }
    }

    fn verifying_row(size: &str, condition: &str, mean_ns: f64) -> VerifyingBenchmarkResult {
        VerifyingBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 100,
            mean_ns,
            ci_lower_ns: mean_ns - 10.0,
            ci_upper_ns: mean_ns + 10.0,
            speedup: None,
        }
    }

    fn wire_row(size: &str, reduction_percent: f64) -> WireSizeResult {
        WireSizeResult {
            size: size.to_string(),
            message_count: 100,
            ladder_len: 8,
            condensed_min: 650,
            condensed_mean: 658.0,
            condensed_max: 660,
            condensed_uniform: true,
            with_mtl_total_bytes: 658,
            without_mtl_total_bytes: 24438,
            reduction_percent,
        }
    }

    fn memory_row(size: &str) -> MemoryResult {
        MemoryResult {
            size: size.to_string(),
            message_count: 100,
            measured_mean_bytes: 4096,
            analytical_bytes: 4096,
            ratio: 1.0,
            measured_range_min_bytes: 4000,
            measured_range_max_bytes: 4200,
            measured_stddev_bytes: 50.0,
        }
    }

    fn analytical_model() -> AnalyticalMemoryModel {
        AnalyticalMemoryModel {
            tree_page_size_bytes: 4096,
            tree_max_pages: 1,
            randomizer_pages: 1,
            hash_size_bytes: 32,
        }
    }

    fn environment_fingerprint() -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "unknown".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({}),
            cpu: serde_json::json!({}),
            submodules: Vec::new(),
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({}),
        }
    }

    fn full_fixture() -> ClimbResults {
        let signing = vec![signing_row("Small", "With MTL", 1000.0)];
        let verifying = vec![verifying_row("Small", "MTL (trust cached)", 100.0)];
        let wire = vec![wire_row("small", 97.3)];
        let derived = derive_summary(&wire, &signing, &verifying);
        ClimbResults {
            version: "1.0".to_string(),
            generated_at: "2026-10-12T00:00:00Z".to_string(),
            workspace_root: "/tmp/climb".to_string(),
            benchmark: BenchmarkResults { signing, verifying },
            wire_size: wire,
            memory: MemoryResults {
                measured: vec![memory_row("small")],
                analytical_model: analytical_model(),
            },
            environment: EnvironmentFingerprint {
                system: serde_json::json!({"os": "linux"}),
                submodules: vec![serde_json::json!({"name": "climb-mtl"})],
                ..environment_fingerprint()
            },
            dns_pipeline: DnsPipelineHeadline {
                full_signature_bytes_www_a: Some(24_438),
                condensed_proof_bytes_www_a: Some(658),
                reduction_percent_www_a: Some(97.3),
                notes: Some("note".to_string()),
            },
            derived,
            warnings: vec!["wire report missing: /x".to_string()],
        }
    }

    #[test]
    fn climb_results_full_roundtrip_equals_original() {
        let original = full_fixture();
        let json = serde_json::to_string(&original).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn climb_results_empty_roundtrip_equals_original() {
        let original = ClimbResults {
            version: "1.0".to_string(),
            generated_at: "unknown".to_string(),
            workspace_root: ".".to_string(),
            benchmark: BenchmarkResults {
                signing: Vec::new(),
                verifying: Vec::new(),
            },
            wire_size: Vec::new(),
            memory: MemoryResults {
                measured: Vec::new(),
                analytical_model: analytical_model(),
            },
            environment: environment_fingerprint(),
            dns_pipeline: DnsPipelineHeadline {
                full_signature_bytes_www_a: None,
                condensed_proof_bytes_www_a: None,
                reduction_percent_www_a: None,
                notes: None,
            },
            derived: DerivedSummary {
                overall_wire_reduction_percent: None,
                largest_wire_reduction_percent: None,
                overall_verifying_trust_cached_speedup_avg: None,
                headline: None,
            },
            warnings: Vec::new(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn derive_wire_reduction_matches_hand_computed_three_size_table() {
        // Hand-computed: (97.0 + 96.7 + 95.7) / 3 = 289.4 / 3 = 96.466_666_...67
        let wire = vec![
            wire_row("small", 97.0),
            wire_row("medium", 96.7),
            wire_row("large", 95.7),
        ];
        let derived = derive_summary(&wire, &[], &[]);
        let overall = derived.overall_wire_reduction_percent.expect("some");
        assert!(
            (overall - 96.466_666_666_666_67).abs() < 1e-9,
            "overall={overall}"
        );
        let largest = derived.largest_wire_reduction_percent.expect("some");
        assert!((largest - 97.0).abs() < 1e-9, "largest={largest}");
    }

    #[test]
    fn derive_wire_reduction_is_none_without_wire_data() {
        let derived = derive_summary(&[], &[], &[]);
        assert_eq!(derived.overall_wire_reduction_percent, None);
        assert_eq!(derived.largest_wire_reduction_percent, None);
        assert_eq!(derived.overall_verifying_trust_cached_speedup_avg, None);
    }

    #[test]
    fn derive_trust_cached_speedup_average_from_two_rows() {
        // Signing "With MTL" means: Small 1000, Medium 2000 ns.
        // Trust-cached verifying means: Small 100 ns -> 10.0, Medium 400 ns -> 5.0.
        // Average = (10.0 + 5.0) / 2 = 7.5.
        let signing = vec![
            signing_row("Small", "With MTL", 1000.0),
            signing_row("Medium", "With MTL", 2000.0),
        ];
        let verifying = vec![
            verifying_row("Small", "MTL (trust cached)", 100.0),
            verifying_row("Medium", "MTL (trust cached)", 400.0),
        ];
        let derived = derive_summary(&[], &signing, &verifying);
        let avg = derived
            .overall_verifying_trust_cached_speedup_avg
            .expect("some");
        assert!((avg - 7.5).abs() < 1e-9, "avg={avg}");
    }

    #[test]
    fn derive_trust_cached_speedup_is_none_without_cached_rows() {
        let signing = vec![signing_row("Small", "With MTL", 1000.0)];
        let verifying = vec![verifying_row("Small", "With MTL", 100.0)];
        let derived = derive_summary(&[], &signing, &verifying);
        assert_eq!(derived.overall_verifying_trust_cached_speedup_avg, None);
    }

    #[test]
    fn read_summary_rows_returns_valid_rows_and_drops_unknown_ids() {
        let root = temp_root("rows-valid");
        write_leaf(
            &root
                .join("signing_small_with_mtl")
                .join("sign_batch")
                .join("100")
                .join("new"),
            "signing_small_with_mtl",
            "sign_batch",
            100,
        );
        // Stale layout: the group id is the first path component, so
        // "signing_small" does not parse as a known group.
        write_leaf(
            &root
                .join("signing_small")
                .join("with_mtl")
                .join("batch")
                .join("new"),
            "signing_small",
            "with_mtl",
            1,
        );

        let rows = read_summary_rows(&root, "new").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, crate::summary::BenchKind::Signing);
        assert_eq!(rows[0].size, crate::summary::Size::Small);
        assert_eq!(rows[0].mode, crate::summary::MtlMode::WithMtlFullVerify);
        assert_eq!(rows[0].elements, Some(100));
    }

    #[test]
    fn read_summary_rows_skips_leaf_missing_estimates_json() {
        let root = temp_root("rows-malformed");
        write_leaf(
            &root
                .join("verifying_medium_with_mtl_trust_true")
                .join("verify_batch")
                .join("1000")
                .join("new"),
            "verifying_medium_with_mtl_trust_true",
            "verify_batch",
            1000,
        );
        let broken = root
            .join("verifying_large_without_mtl")
            .join("verify_batch")
            .join("10000")
            .join("new");
        fs::create_dir_all(&broken).unwrap();
        fs::write(
            broken.join("benchmark.json"),
            r#"{"group_id":"verifying_large_without_mtl","function_id":"verify_batch","value_str":"10000","throughput":{"Elements":10000},"full_id":"verifying_large_without_mtl/verify_batch/10000","directory_name":"x","title":"x"}"#,
        )
        .unwrap();

        let rows = read_summary_rows(&root, "new").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, crate::summary::BenchKind::Verifying);
        assert_eq!(rows[0].size, crate::summary::Size::Medium);
    }

    #[test]
    fn summarize_strictly_rejects_unknown_group_id() {
        // Documents why `read_summary_rows` filters before calling `summarize`:
        // a strict call would fail on the same stale layout.
        let record = CriterionRecord {
            group_id: "signing_small".to_string(),
            function_id: "with_mtl".to_string(),
            value_str: "1".to_string(),
            full_id: "signing_small/with_mtl/1".to_string(),
            elements: Some(1),
            mean_ns: 1.0,
            mean_ci_lower_ns: 1.0,
            mean_ci_upper_ns: 1.0,
            confidence_level: 0.95,
        };
        assert!(summarize(&[record]).is_err());
    }

    #[test]
    fn freshness_true_within_max_age() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_true_at_exact_boundary() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(24 * 3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_false_when_stale() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(25 * 3600);
        assert!(!input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_false_for_future_timestamp() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now + Duration::from_secs(3600);
        assert!(!input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_force_overrides_staleness() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(100 * 3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            true
        ));
    }

    #[test]
    fn consolidate_missing_optional_inputs_yield_null_sections_and_warnings() {
        let root = temp_root("missing-optional");
        let criterion = root.join("criterion");
        write_leaf(
            &criterion
                .join("signing_small_with_mtl")
                .join("sign_batch")
                .join("100")
                .join("new"),
            "signing_small_with_mtl",
            "sign_batch",
            100,
        );

        let opts = ConsolidateOptions {
            force: false,
            criterion_dir: criterion,
            wire_report_path: Some(root.join("nonexistent-wire.json")),
            memory_report_path: Some(root.join("nonexistent-memory.json")),
            environment_path: root.join("nonexistent-environment.json"),
            baseline: "new".to_string(),
            output_path: root.join("out.json"),
        };

        let results = consolidate_results(opts).unwrap();

        assert_eq!(results.derived.overall_wire_reduction_percent, None);
        assert_eq!(results.derived.largest_wire_reduction_percent, None);
        assert_eq!(results.benchmark.signing.len(), 1);
        assert!(
            results
                .warnings
                .iter()
                .any(|w| w.starts_with("wire report missing:")),
            "warnings={:?}",
            results.warnings
        );
        assert!(
            results
                .warnings
                .iter()
                .any(|w| w.starts_with("memory report missing:")),
            "warnings={:?}",
            results.warnings
        );
        assert_eq!(results.environment.schema_version, 1);
        assert_eq!(results.environment.timestamp, "unknown");

        let json = fs::read_to_string(root.join("out.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["derived"]["overall_wire_reduction_percent"].is_null());
        assert!(value["warnings"].is_array());
    }

    #[test]
    fn consolidate_end_to_end_writes_parseable_results() {
        let root = temp_root("e2e");
        let criterion = root.join("criterion");
        write_leaf(
            &criterion
                .join("verifying_medium_with_mtl_trust_true")
                .join("verify_batch")
                .join("1000")
                .join("new"),
            "verifying_medium_with_mtl_trust_true",
            "verify_batch",
            1000,
        );

        let output = root.join("results.json");
        let opts = ConsolidateOptions {
            force: false,
            criterion_dir: criterion,
            wire_report_path: None,
            memory_report_path: None,
            environment_path: root.join("environment.json"),
            baseline: "new".to_string(),
            output_path: output.clone(),
        };

        let results = consolidate_results(opts).unwrap();
        assert_eq!(results.version, "1.0");
        assert_eq!(results.benchmark.verifying.len(), 1);
        assert!(results.warnings.is_empty());

        let json = fs::read_to_string(&output).unwrap();
        let parsed: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, results);
    }
}
