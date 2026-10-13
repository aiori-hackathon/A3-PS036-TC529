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
    pub libmtl_analytical_bytes: usize,
    pub harness_buffers_bytes: u64,
    pub analytical_total_bytes: u64,
    pub residual_bytes: i64,
    pub residual_percent: f64,
    pub ratio: f64,
    pub measured_analytical_total_ratio: f64,
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

/// Where a [`DnsPipelineHeadline`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnsCaptureSource {
    /// A live `cargo xtask dns-demo --record` run.
    Live,
    /// The committed `data/dns-capture.recorded.json` fixture.
    RecordedFixture,
}

/// DNS header flags recorded for one captured query.
///
/// Only `rcode` is observed by the current capture path; `ad` and `tc` are
/// part of the shared schema but are left `None` rather than guessed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureFlags {
    pub ad: Option<bool>,
    pub tc: Option<bool>,
    pub rcode: Option<u8>,
}

/// One query captured during the live MTL DNS pipeline demo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsCaptureQuery {
    /// Capture pass: `full`, `condensed`, or `unbound`.
    pub label: String,
    pub qname: String,
    pub qtype: String,
    pub response_bytes: u64,
    /// Length of the first answer RRSIG signature field, when present.
    pub answer_rrsig_bytes: Option<u64>,
    pub flags: DnsCaptureFlags,
    pub transport: String,
    pub edns_udp_size: Option<u16>,
    pub elapsed_ms: Option<f64>,
}

/// Host summary for a recorded capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureHost {
    pub hostname: String,
    pub uname: String,
}

/// Docker image reference (and digest, when resolvable) used for a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureImage {
    pub reference: String,
    pub digest: Option<String>,
}

/// A complete recorded run of the live MTL DNS pipeline demo.
///
/// Written by `cargo xtask dns-demo --record` and consumed by
/// `cargo xtask consolidate`. Fields the capture path does not observe are
/// optional and left `None`; nothing here is synthesized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsCapture {
    pub schema_version: u32,
    /// RFC 3339 timestamp of the recording.
    pub recorded_at: String,
    pub host: DnsCaptureHost,
    pub images: Vec<DnsCaptureImage>,
    pub algorithm: String,
    pub zone: String,
    pub queries: Vec<DnsCaptureQuery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsPipelineHeadline {
    pub full_signature_bytes_www_a: Option<u64>,
    pub condensed_proof_bytes_www_a: Option<u64>,
    pub reduction_percent_www_a: Option<f64>,
    pub notes: Option<String>,
    /// Provenance of these numbers.
    pub source: DnsCaptureSource,
    /// RFC 3339 timestamp of the recording, when known.
    pub recorded_at: Option<String>,
    /// Signature/hash algorithm these numbers were produced with, when known.
    /// Sourced from the DNS capture (never synthesized); `#[serde(default)]`
    /// keeps older results files that predate this field parseable.
    #[serde(default)]
    pub algorithm: Option<String>,
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
    /// DNS pipeline headline, or `None` when no capture (live or recorded
    /// fixture) is available.
    pub dns_pipeline: Option<DnsPipelineHeadline>,
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
    /// Live DNS capture written by `cargo xtask dns-demo --record`
    /// (preferred when present).
    pub dns_capture_path: PathBuf,
    /// Committed recorded fixture used when no live capture exists.
    pub dns_fixture_path: PathBuf,
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
    dns_pipeline: Option<&DnsPipelineHeadline>,
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
        headline: dns_pipeline.and_then(headline_from_dns),
    }
}

/// Derive the one-line headline from captured DNS numbers; `None` when the
/// capture is absent or incomplete, so the headline is never a constant.
fn headline_from_dns(headline: &DnsPipelineHeadline) -> Option<String> {
    let full = headline.full_signature_bytes_www_a?;
    let cond = headline.condensed_proof_bytes_www_a?;
    let pct = headline.reduction_percent_www_a?;
    Some(format!(
        "MTL condensed responses are ~{pct:.1}% smaller than full signatures ({} B \u{2192} {} B)",
        crate::charts::fmt_count(full as f64),
        crate::charts::fmt_count(cond as f64),
    ))
}

/// The question whose byte counts the DNS headline is computed from.
const HEADLINE_QNAME: &str = "www.climb.example.";
const HEADLINE_QTYPE: &str = "A";

/// Derive the DNS headline from captured byte counts: the full/condensed
/// bytes come from the matching `www.climb.example. A` rows and the reduction
/// is computed from them, never hardcoded.
fn headline_from_capture(capture: &DnsCapture, source: DnsCaptureSource) -> DnsPipelineHeadline {
    let find = |label: &str| {
        capture
            .queries
            .iter()
            .find(|q| {
                q.label.eq_ignore_ascii_case(label)
                    && q.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
                    && q.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE)
            })
            .map(|q| q.response_bytes)
    };
    let full = find("full");
    let condensed = find("condensed");
    let reduction = match (full, condensed) {
        (Some(f), Some(c)) if f > 0 => Some(((f - c) as f64 / f as f64) * 100.0),
        _ => None,
    };
    DnsPipelineHeadline {
        full_signature_bytes_www_a: full,
        condensed_proof_bytes_www_a: condensed,
        reduction_percent_www_a: reduction,
        notes: Some(format!(
            "From {} DNS demo (www.climb.example. A)",
            match source {
                DnsCaptureSource::Live => "live",
                DnsCaptureSource::RecordedFixture => "recorded fixture",
            }
        )),
        source,
        recorded_at: Some(capture.recorded_at.clone()),
        algorithm: Some(capture.algorithm.clone()),
    }
}

/// Load the DNS capture, preferring a live recording over the committed
/// fixture; `None` when neither exists.
fn load_dns_capture(
    live: &Path,
    fixture: &Path,
) -> Result<Option<DnsPipelineHeadline>, ReportError> {
    let (path, source) = if live.exists() {
        (live, DnsCaptureSource::Live)
    } else if fixture.exists() {
        (fixture, DnsCaptureSource::RecordedFixture)
    } else {
        return Ok(None);
    };
    let capture: DnsCapture = read_json(path)?;
    Ok(Some(headline_from_capture(&capture, source)))
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

    // The Rust harness holds one `SignOutput` per `sign_batch`: the shared
    // ladder plus one condensed signature per message. Those are exactly the
    // bytes the wire-size pass measured for the same dataset size, so the
    // harness term is derived from the wire row — `with_mtl_total` is the
    // ladder plus the condensed sum, and the condensed sum is the remainder.
    // A size with no wire row leaves the payload unmodelled rather than
    // inventing one.
    let wire_for = |size_label: &str| wire_size.iter().find(|w| w.size == size_label);

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
                    let measured = c.measured_mean_bytes as i64;
                    let total = c.analytical_bytes as i64;
                    memory_results.measured.push(MemoryResult {
                        size: format!("{:?}", c.size).to_lowercase(),
                        message_count: c.message_count,
                        measured_mean_bytes: c.measured_mean_bytes,
                        libmtl_analytical_bytes: c.analytical_bytes,
                        harness_buffers_bytes: 0,
                        analytical_total_bytes: c.analytical_bytes as u64,
                        residual_bytes: measured - total,
                        residual_percent: if total == 0 {
                            0.0
                        } else {
                            (measured as f64 - total as f64) / (total as f64) * 100.0
                        },
                        ratio: c.ratio,
                        measured_analytical_total_ratio: if c.analytical_bytes == 0 {
                            0.0
                        } else {
                            c.measured_mean_bytes as f64 / (c.analytical_bytes as f64)
                        },
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
                        let size_label = format!("{:?}", run.size).to_lowercase();
                        let (harness_n, condensed_sum, ladder_len) = match wire_for(&size_label) {
                            Some(w) => {
                                let condensed =
                                    w.with_mtl_total_bytes.saturating_sub(w.ladder_len as u64);
                                (
                                    w.message_count,
                                    usize::try_from(condensed).unwrap_or(usize::MAX),
                                    w.ladder_len,
                                )
                            }
                            None => {
                                warnings.push(format!(
                                    "memory report size `{size_label}` has no matching wire-size row; harness payload not modelled"
                                ));
                                (run.size.count(), 0, 0)
                            }
                        };
                        let harness = climb_analytical::harness_output_estimate(
                            harness_n,
                            condensed_sum,
                            ladder_len,
                        );
                        let total = (analytical.total_memory_bytes as u64)
                            + (harness.estimated_total_bytes as u64);
                        let measured_i64 = mean.round() as i64;
                        memory_results.measured.push(MemoryResult {
                            size: format!("{:?}", run.size).to_lowercase(),
                            message_count: run.size.count(),
                            measured_mean_bytes: mean.round() as u64,
                            libmtl_analytical_bytes: analytical.total_memory_bytes,
                            harness_buffers_bytes: harness.estimated_total_bytes as u64,
                            analytical_total_bytes: total,
                            residual_bytes: measured_i64 - (total as i64),
                            residual_percent: if total == 0 {
                                0.0
                            } else {
                                ((measured_i64 as f64) - (total as f64)) / (total as f64) * 100.0
                            },
                            ratio: if analytical.total_memory_bytes == 0 {
                                0.0
                            } else {
                                mean / (analytical.total_memory_bytes as f64)
                            },
                            measured_analytical_total_ratio: if total == 0 {
                                0.0
                            } else {
                                mean / (total as f64)
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

    let dns_pipeline = load_dns_capture(&opts.dns_capture_path, &opts.dns_fixture_path)?;

    let derived = derive_summary(&wire_size, &signing, &verifying, dns_pipeline.as_ref());

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
            libmtl_analytical_bytes: 4096,
            harness_buffers_bytes: 0,
            analytical_total_bytes: 4096,
            residual_bytes: 0,
            residual_percent: 0.0,
            measured_analytical_total_ratio: 1.0,
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
        let dns = DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(24_438),
            condensed_proof_bytes_www_a: Some(658),
            reduction_percent_www_a: Some(97.3),
            notes: Some("note".to_string()),
            source: DnsCaptureSource::Live,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        };
        let derived = derive_summary(&wire, &signing, &verifying, Some(&dns));
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
            dns_pipeline: Some(dns),
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
            dns_pipeline: None,
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
        let derived = derive_summary(&wire, &[], &[], None);
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
        let derived = derive_summary(&[], &[], &[], None);
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
        let derived = derive_summary(&[], &signing, &verifying, None);
        let avg = derived
            .overall_verifying_trust_cached_speedup_avg
            .expect("some");
        assert!((avg - 7.5).abs() < 1e-9, "avg={avg}");
    }

    #[test]
    fn derive_trust_cached_speedup_is_none_without_cached_rows() {
        let signing = vec![signing_row("Small", "With MTL", 1000.0)];
        let verifying = vec![verifying_row("Small", "With MTL", 100.0)];
        let derived = derive_summary(&[], &signing, &verifying, None);
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
            dns_capture_path: root.join("nonexistent-dns-capture.json"),
            dns_fixture_path: root.join("nonexistent-dns-fixture.json"),
            environment_path: root.join("nonexistent-environment.json"),
            baseline: "new".to_string(),
            output_path: root.join("out.json"),
        };

        let results = consolidate_results(opts).unwrap();

        assert_eq!(results.derived.overall_wire_reduction_percent, None);
        assert_eq!(results.derived.largest_wire_reduction_percent, None);
        assert_eq!(results.dns_pipeline, None);
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
            dns_capture_path: root.join("dns-capture.json"),
            dns_fixture_path: root.join("dns-fixture.json"),
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

    const WIRE_REPORT_JSON: &str = r#"[
      {"size":"Small","seed":1,"message_count":100,"ladder_len":7992,
       "condensed":{"min":108,"max":172,"mean":164.32,"total":16432,"uniform":false},
       "with_mtl_total":24424,"without_mtl_total":815632,"reduction_percent":97.00551228985621,"full_invariant_samples":6},
      {"size":"Medium","seed":1,"message_count":10000,"ladder_len":8056,
       "condensed":{"min":140,"max":284,"mean":273.5296,"total":2735296,"uniform":false},
       "with_mtl_total":2743352,"without_mtl_total":83295296,"reduction_percent":96.70647427677068,"full_invariant_samples":6},
      {"size":"Large","seed":1,"message_count":1000000,"ladder_len":8120,
       "condensed":{"min":172,"max":380,"mean":367.059712,"total":367059712,"uniform":false},
       "with_mtl_total":367067832,"without_mtl_total":8487059712,"reduction_percent":95.67497054980069,"full_invariant_samples":6}
    ]"#;

    // Two repeats per size, chosen so the arithmetic mean equals the mean of
    // record: small (1_630_208 + 73_728)/2 = 851_968,
    // medium (3_575_808 + 942_080)/2 = 2_258_944,
    // large (561_266_688 + 80_093_184)/2 = 320_679_936.
    const MEMORY_REPORT_JSON: &str = r#"{"seed":1,"runs":[
      {"size":"Small","runs":[
        {"rss_before_bytes":14675968,"rss_after_bytes":16306176,"delta_bytes":1630208,"delta_mib":1.5546875},
        {"rss_before_bytes":16273408,"rss_after_bytes":16347136,"delta_bytes":73728,"delta_mib":0.0703125}],"repeat_count":2},
      {"size":"Medium","runs":[
        {"rss_before_bytes":22171648,"rss_after_bytes":25747456,"delta_bytes":3575808,"delta_mib":3.41015625},
        {"rss_before_bytes":25903104,"rss_after_bytes":26845184,"delta_bytes":942080,"delta_mib":0.8984375}],"repeat_count":2},
      {"size":"Large","runs":[
        {"rss_before_bytes":584851456,"rss_after_bytes":1146118144,"delta_bytes":561266688,"delta_mib":535.265625},
        {"rss_before_bytes":1075388416,"rss_after_bytes":1155481600,"delta_bytes":80093184,"delta_mib":76.3828125}],"repeat_count":2}
    ]}"#;

    fn write_report_inputs(root: &Path) -> (PathBuf, PathBuf) {
        let wire = root.join("wire.json");
        let memory = root.join("memory.json");
        fs::write(&wire, WIRE_REPORT_JSON).unwrap();
        fs::write(&memory, MEMORY_REPORT_JSON).unwrap();
        (wire, memory)
    }

    fn consolidate_with(
        root: &Path,
        wire: Option<PathBuf>,
        memory: Option<PathBuf>,
    ) -> ClimbResults {
        let criterion = root.join("criterion");
        fs::create_dir_all(&criterion).unwrap();
        let opts = ConsolidateOptions {
            force: true,
            criterion_dir: criterion,
            wire_report_path: wire,
            memory_report_path: memory,
            dns_capture_path: root.join("no-capture.json"),
            dns_fixture_path: root.join("no-fixture.json"),
            environment_path: root.join("no-env.json"),
            baseline: "new".to_string(),
            output_path: root.join("out.json"),
        };
        consolidate_results(opts).unwrap()
    }

    fn memory_of<'a>(results: &'a ClimbResults, size: &str) -> &'a MemoryResult {
        results
            .memory
            .measured
            .iter()
            .find(|m| m.size == size)
            .unwrap_or_else(|| panic!("missing memory row for {size}"))
    }

    #[test]
    fn consolidate_derives_harness_from_wire_condensed_payload() {
        let root = temp_root("memory-wire");
        let (wire, memory) = write_report_inputs(&root);
        let results = consolidate_with(&root, Some(wire), Some(memory));

        assert_eq!(results.memory.measured.len(), 3);
        assert!(
            results.warnings.is_empty(),
            "warnings={:?}",
            results.warnings
        );

        // The harness payload term is the wire row's condensed sum
        // (with_mtl_total - ladder_len) plus the ladder, plus (n+1) x 32.
        // Small:  24_424 + 101 x 32         = 27_656;   total = 2_097_152 + 27_656
        // Medium: 2_743_352 + 10_001 x 32   = 3_063_384; total = 2_097_152 + 3_063_384
        // Large:  367_067_832 + 1_000_001 x 32 = 399_067_864; total = 49_283_072 + that
        let small = memory_of(&results, "small");
        assert_eq!(small.harness_buffers_bytes, 27_656);
        assert_eq!(small.analytical_total_bytes, 2_124_808);
        assert_eq!(small.residual_bytes, 851_968 - 2_124_808);
        assert_eq!(
            small.measured_analytical_total_ratio,
            851_968.0 / 2_124_808.0
        );

        let medium = memory_of(&results, "medium");
        assert_eq!(medium.harness_buffers_bytes, 3_063_384);
        assert_eq!(medium.analytical_total_bytes, 5_160_536);
        assert_eq!(medium.residual_bytes, 2_258_944 - 5_160_536);

        let large = memory_of(&results, "large");
        assert_eq!(large.harness_buffers_bytes, 399_067_864);
        assert_eq!(large.analytical_total_bytes, 448_350_936);
        assert_eq!(large.residual_bytes, 320_679_936 - 448_350_936);
        assert!(large.residual_bytes < 0);
        assert!(large.measured_analytical_total_ratio < 1.0);

        // libMTL pages + harness buffers reconstructs the analytical total.
        for m in &results.memory.measured {
            assert_eq!(
                m.libmtl_analytical_bytes as u64 + m.harness_buffers_bytes,
                m.analytical_total_bytes,
                "size {}",
                m.size
            );
        }
    }

    #[test]
    fn consolidate_memory_without_wire_row_leaves_payload_unmodelled() {
        let root = temp_root("memory-no-wire");
        let (_, memory) = write_report_inputs(&root);
        let results = consolidate_with(&root, None, Some(memory));

        assert_eq!(results.memory.measured.len(), 3);
        for m in &results.memory.measured {
            // Only the (n+1)-Vec overhead is modelled when no wire row exists.
            assert_eq!(
                m.harness_buffers_bytes,
                (m.message_count + 1) as u64 * 32,
                "size {}",
                m.size
            );
            assert_eq!(
                m.analytical_total_bytes,
                m.libmtl_analytical_bytes as u64 + m.harness_buffers_bytes
            );
        }
        let fallbacks = results
            .warnings
            .iter()
            .filter(|w| w.contains("no matching wire-size row"))
            .count();
        assert_eq!(fallbacks, 3, "warnings={:?}", results.warnings);
    }

    #[test]
    fn harness_estimate_payload_term_equals_wire_condensed_sum() {
        // Independent of consolidate: the payload handed to the estimate is the
        // wire condensed sum (367_059_712 for Large) and the total adds the
        // ladder plus (n+1) x 32 overhead.
        let est = climb_analytical::harness_output_estimate(1_000_000, 367_059_712, 8_120);
        assert_eq!(est.condensed_len_sum, 367_059_712);
        assert_eq!(est.payload_bytes, 367_067_832);
        assert_eq!(est.vec_overhead_bytes, 1_000_001 * 32);
        assert_eq!(est.estimated_total_bytes, 367_067_832 + 1_000_001 * 32);
    }

    fn dns_query(label: &str, bytes: u64) -> DnsCaptureQuery {
        DnsCaptureQuery {
            label: label.to_string(),
            qname: "www.climb.example.".to_string(),
            qtype: "A".to_string(),
            response_bytes: bytes,
            answer_rrsig_bytes: Some(69),
            flags: DnsCaptureFlags {
                ad: None,
                tc: None,
                rcode: Some(0),
            },
            transport: "tcp".to_string(),
            edns_udp_size: None,
            elapsed_ms: None,
        }
    }

    fn dns_capture_fixture(full: u64, condensed: u64) -> DnsCapture {
        DnsCapture {
            schema_version: 1,
            recorded_at: "2026-10-12T00:00:00Z".to_string(),
            host: DnsCaptureHost {
                hostname: "test-host".to_string(),
                uname: "Linux test".to_string(),
            },
            images: Vec::new(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            zone: "climb.example.".to_string(),
            queries: vec![dns_query("full", full), dns_query("condensed", condensed)],
        }
    }

    #[test]
    fn dns_capture_roundtrip_equals_original() {
        let original = dns_capture_fixture(24_438, 658);
        let json = serde_json::to_string(&original).unwrap();
        let restored: DnsCapture = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn headline_from_capture_computes_reduction_from_www_a_rows() {
        let capture = dns_capture_fixture(24_438, 658);
        let headline = headline_from_capture(&capture, DnsCaptureSource::Live);
        assert_eq!(headline.full_signature_bytes_www_a, Some(24_438));
        assert_eq!(headline.condensed_proof_bytes_www_a, Some(658));
        let pct = headline.reduction_percent_www_a.expect("some");
        assert!((pct - 97.307_37).abs() < 1e-3, "pct={pct}");
        assert_eq!(headline.source, DnsCaptureSource::Live);
        assert_eq!(
            headline.recorded_at.as_deref(),
            Some("2026-10-12T00:00:00Z")
        );
    }

    #[test]
    fn headline_from_capture_is_null_when_rows_absent() {
        let mut capture = dns_capture_fixture(24_438, 658);
        capture.queries.clear();
        let headline = headline_from_capture(&capture, DnsCaptureSource::RecordedFixture);
        assert_eq!(headline.full_signature_bytes_www_a, None);
        assert_eq!(headline.condensed_proof_bytes_www_a, None);
        assert_eq!(headline.reduction_percent_www_a, None);
        assert_eq!(headline.source, DnsCaptureSource::RecordedFixture);
    }

    #[test]
    fn headline_from_dns_renders_expected_sentence() {
        let dns = DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(24_438),
            condensed_proof_bytes_www_a: Some(658),
            reduction_percent_www_a: Some(97.307_37),
            notes: None,
            source: DnsCaptureSource::RecordedFixture,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        };
        let headline = headline_from_dns(&dns).expect("some");
        assert!(headline.contains("97.3%"), "headline={headline}");
        assert!(headline.contains("24,438 B"), "headline={headline}");
        assert!(headline.contains("658 B"), "headline={headline}");
    }

    #[test]
    fn derive_summary_headline_is_none_without_dns_capture() {
        let derived = derive_summary(&[], &[], &[], None);
        assert_eq!(derived.headline, None);
    }

    #[test]
    fn load_dns_capture_prefers_live_over_fixture() {
        let root = temp_root("dns-live-preference");
        let live = root.join("live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &live,
            serde_json::to_string(&dns_capture_fixture(100, 10)).unwrap(),
        )
        .unwrap();
        fs::write(
            &fixture,
            serde_json::to_string(&dns_capture_fixture(999, 999)).unwrap(),
        )
        .unwrap();
        let headline = load_dns_capture(&live, &fixture).unwrap().expect("some");
        assert_eq!(headline.source, DnsCaptureSource::Live);
        assert_eq!(headline.full_signature_bytes_www_a, Some(100));
    }

    #[test]
    fn load_dns_capture_falls_back_to_recorded_fixture() {
        let root = temp_root("dns-fixture-fallback");
        let live = root.join("missing-live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &fixture,
            serde_json::to_string(&dns_capture_fixture(24_438, 658)).unwrap(),
        )
        .unwrap();
        let headline = load_dns_capture(&live, &fixture).unwrap().expect("some");
        assert_eq!(headline.source, DnsCaptureSource::RecordedFixture);
        assert_eq!(headline.full_signature_bytes_www_a, Some(24_438));
    }

    #[test]
    fn load_dns_capture_returns_none_when_neither_exists() {
        let root = temp_root("dns-none");
        let headline =
            load_dns_capture(&root.join("no-live.json"), &root.join("no-fixture.json")).unwrap();
        assert_eq!(headline, None);
    }
}
