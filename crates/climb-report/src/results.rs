use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::ReportError;
use crate::record::{scan_criterion, CriterionRecord};
use crate::summary::{summarize, SummaryRow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyingBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub signing: Vec<SigningBenchmarkResult>,
    pub verifying: Vec<VerifyingBenchmarkResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticalMemoryModel {
    pub tree_page_size_bytes: usize,
    pub tree_max_pages: usize,
    pub randomizer_pages: usize,
    pub hash_size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResults {
    pub measured: Vec<MemoryResult>,
    pub analytical_model: AnalyticalMemoryModel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsPipelineHeadline {
    pub full_signature_bytes_www_a: Option<u64>,
    pub condensed_proof_bytes_www_a: Option<u64>,
    pub reduction_percent_www_a: Option<f64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub overall_wire_reduction_percent: Option<f64>,
    pub largest_wire_reduction_percent: Option<f64>,
    pub overall_verifying_trust_cached_speedup_avg: Option<f64>,
    pub headline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn is_fresh(path: &Path, max_age: Duration) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(m) => m,
        Err(_) => return false,
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age <= max_age,
        Err(_) => false,
    }
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
        } else if !opts.force && !is_fresh(p, max_age) {
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
        if p.exists() && (!opts.force && !is_fresh(p, max_age)) {
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

    let mut overall_wire_reduction: Option<f64> = None;
    let mut largest_wire_reduction: Option<f64> = None;
    if !wire_size.is_empty() {
        let mut sum = 0.0f64;
        let mut max = f64::NEG_INFINITY;
        for w in &wire_size {
            sum += w.reduction_percent;
            if w.reduction_percent > max {
                max = w.reduction_percent;
            }
        }
        overall_wire_reduction = Some(sum / wire_size.len() as f64);
        largest_wire_reduction = Some(if max.is_finite() { max } else { 0.0 });
    }

    let mut trust_cached_avg: Option<f64> = None;
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
    if !cached.is_empty() {
        trust_cached_avg = Some(cached.iter().sum::<f64>() / cached.len() as f64);
    }

    let dns_pipeline = DnsPipelineHeadline {
        full_signature_bytes_www_a: Some(24438),
        condensed_proof_bytes_www_a: Some(658),
        reduction_percent_www_a: Some(97.3),
        notes: Some("From Phase 14 live DNS demo (www.climb.example. A)".to_string()),
    };

    let headline = Some(
        "MTL condensed responses are ~97.3% smaller than full signatures (24,438 B → 658 B)"
            .to_string(),
    );

    let derived = DerivedSummary {
        overall_wire_reduction_percent: overall_wire_reduction,
        largest_wire_reduction_percent: largest_wire_reduction,
        overall_verifying_trust_cached_speedup_avg: trust_cached_avg,
        headline,
    };

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
