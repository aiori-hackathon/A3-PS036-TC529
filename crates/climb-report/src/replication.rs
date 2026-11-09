//! Cross-host replication: compare the headline ratios of two results
//! documents and summarise how closely they agree.
//!
//! The comparison is deliberately narrow. It reads only ratios that are
//! already derived in a consolidated artifact (wire reduction, signing and
//! verifying speed-ups, and the two aggregate wire reductions), computes the
//! relative difference of each pair, and reports the largest difference. No
//! value is re-measured here and no host is assumed to be the reference: the
//! two documents are named A and B and the difference is symmetric.
//!
//! The agreement tolerance is a declared threshold, not a computed value. The
//! computed value is `max_relative_difference_percent`; a section agrees only
//! when that value is at or below [`REPLICATION_TOLERANCE_PERCENT`]. A section
//! whose ratios disagree says so and still records the computed maximum, so a
//! reader can see how far apart the hosts were rather than only that they
//! missed a threshold.

use serde::{Deserialize, Serialize};

use crate::results::{ClimbResults, EvidenceClass};

/// Schema version written into the replication summary artifact.
pub const REPLICATION_SCHEMA_VERSION: u32 = 1;

/// Declared reproducibility tolerance for headline ratios, in percent.
///
/// This is a reporting threshold chosen before looking at any second-host
/// data; it is not derived from the measurements. The computed maximum
/// relative difference is always recorded separately.
pub const REPLICATION_TOLERANCE_PERCENT: f64 = 10.0;

/// Whether a replication summary compares two hosts or is waiting for the
/// second host's artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplicationStatus {
    /// Only one host is represented; no ratio comparison exists yet.
    Pending,
    /// Two hosts were compared.
    Compared,
}

impl ReplicationStatus {
    /// Stable artifact label.
    pub fn label(self) -> &'static str {
        match self {
            ReplicationStatus::Pending => "pending",
            ReplicationStatus::Compared => "compared",
        }
    }
}

/// One host's identity, as recorded in the environment fingerprint embedded
/// in its results document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationHost {
    /// First 12 hex characters of SHA-256 over the serialized environment
    /// fingerprint.
    pub host_hash: String,
    /// Results document the host facts were read from.
    pub results_path: String,
    /// CPU model string from the fingerprint.
    pub cpu_model: String,
    /// CPU governor recorded for the benchmark run.
    pub cpu_governor: String,
    /// Kernel release string.
    pub kernel_release: String,
    /// `rustc` version string.
    pub rustc_version: String,
    /// Logical CPU count, when recorded.
    pub logical_cpus: Option<u64>,
    /// The results document's own `generated_at` timestamp.
    pub generated_at: String,
    /// Dataset sizes the document carries, in artifact order.
    pub sizes: Vec<String>,
    /// Number of headline ratios this host contributed to the comparison.
    pub ratios: u64,
}

/// One headline ratio, measured independently on both hosts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationRatio {
    /// Stable identifier: `<ratio field>:<size>`.
    pub id: String,
    /// Human-readable row label.
    pub label: String,
    /// Dataset size label, or `all` for the aggregate rows.
    pub size: String,
    /// Host A's value.
    pub a: f64,
    /// Host B's value.
    pub b: f64,
    /// `|B − A| / max(|A|, |B|) × 100`.
    pub relative_difference_percent: f64,
    /// Always [`EvidenceClass::Derived`]: both inputs are derived ratios and
    /// the difference is computed from them.
    pub class: EvidenceClass,
}

/// Replication summary embedded in the consolidated results artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationSection {
    /// Artifact schema version.
    pub schema_version: u32,
    /// Pending or compared.
    pub status: ReplicationStatus,
    /// When the summary was produced.
    pub generated_at: String,
    /// One host when pending, two when compared.
    pub hosts: Vec<ReplicationHost>,
    /// One row per headline ratio present on both hosts.
    pub ratios: Vec<ReplicationRatio>,
    /// Largest computed relative difference, or `None` when pending.
    pub max_relative_difference_percent: Option<f64>,
    /// Declared tolerance for the agreement verdict.
    pub tolerance_percent: f64,
    /// Whether `max_relative_difference_percent <= tolerance_percent`.
    /// `None` while pending.
    pub agrees: Option<bool>,
    /// Ratios present on one host but not the other; they are not compared.
    pub missing_ratios: u64,
    /// Human-readable scope note, including the host count.
    pub note: String,
    /// Evidence class: always [`EvidenceClass::Derived`].
    pub class: EvidenceClass,
}

/// One headline ratio read from a results document.
#[derive(Debug, Clone, PartialEq)]
struct HeadlineValue {
    id: String,
    label: String,
    size: String,
    value: f64,
}

/// Every headline ratio a results document carries, in a stable order.
fn headline_values(results: &ClimbResults) -> Vec<HeadlineValue> {
    let mut out = Vec::new();
    for comparison in &results.comparisons {
        for (field, label, value) in [
            (
                "wire_reduction_c_vs_b_percent",
                "Wire reduction, C vs B",
                comparison.wire_reduction_c_vs_b_percent,
            ),
            (
                "signing_speedup_c_vs_b",
                "Signing speed-up, C vs B",
                comparison.signing_speedup_c_vs_b,
            ),
            (
                "verifying_speedup_cached_vs_a",
                "Verifying speed-up, C vs A (cached)",
                comparison.verifying_speedup_cached_vs_a,
            ),
        ] {
            if let Some(value) = value {
                out.push(HeadlineValue {
                    id: format!("{field}:{}", comparison.size),
                    label: label.to_string(),
                    size: comparison.size.clone(),
                    value,
                });
            }
        }
    }
    for (field, label, value) in [
        (
            "overall_wire_reduction_percent",
            "Overall wire reduction",
            results.derived.overall_wire_reduction_percent,
        ),
        (
            "largest_wire_reduction_percent",
            "Largest wire reduction",
            results.derived.largest_wire_reduction_percent,
        ),
    ] {
        if let Some(value) = value {
            out.push(HeadlineValue {
                id: format!("{field}:all"),
                label: label.to_string(),
                size: "all".to_string(),
                value,
            });
        }
    }
    out
}

/// Symmetric relative difference between two values, in percent.
///
/// The denominator is the larger magnitude of the pair, so the result does not
/// depend on which host is called A. Two exact zeroes differ by zero; a zero
/// against a non-zero value differs by 100%.
pub fn relative_difference_percent(a: f64, b: f64) -> f64 {
    let scale = a.abs().max(b.abs());
    if scale == 0.0 {
        0.0
    } else {
        (b - a).abs() / scale * 100.0
    }
}

/// Host facts from the environment fingerprint embedded in a results
/// document, with the fingerprint's short hash.
pub fn host_from_results(results: &ClimbResults, results_path: &str) -> ReplicationHost {
    let environment = &results.environment;
    let string_at = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string()
    };
    let logical_cpus = environment
        .cpu
        .get("logical_cpus")
        .and_then(serde_json::Value::as_u64);
    let sizes = results
        .wire_size
        .iter()
        .map(|row| row.size.clone())
        .collect();
    let ratios = headline_values(results).len() as u64;
    ReplicationHost {
        host_hash: environment_hash(environment),
        results_path: results_path.to_string(),
        cpu_model: string_at(&environment.cpu, "model_name"),
        cpu_governor: string_at(&environment.benchmark_runtime, "cpu_governor"),
        kernel_release: string_at(&environment.system, "kernel_release"),
        rustc_version: string_at(&environment.toolchain, "rustc_version"),
        logical_cpus,
        generated_at: results.generated_at.clone(),
        sizes,
        ratios,
    }
}

/// First 12 hex characters of SHA-256 over the serialized environment
/// fingerprint. Deterministic for a given fingerprint document, so the same
/// machine always produces the same directory name.
pub fn environment_hash(environment: &crate::results::EnvironmentFingerprint) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(environment).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Compare the headline ratios of two results documents.
///
/// Returns one row per ratio present on both hosts. A ratio present on only
/// one host is counted in [`ReplicationSection::missing_ratios`] rather than
/// compared against a guessed value.
pub fn compare_results(a: &ClimbResults, b: &ClimbResults) -> (Vec<ReplicationRatio>, u64) {
    let host_a = headline_values(a);
    let host_b = headline_values(b);
    let mut rows = Vec::new();
    let mut matched = 0usize;
    for left in &host_a {
        if let Some(right) = host_b.iter().find(|row| row.id == left.id) {
            matched += 1;
            rows.push(ReplicationRatio {
                id: left.id.clone(),
                label: left.label.clone(),
                size: left.size.clone(),
                a: left.value,
                b: right.value,
                relative_difference_percent: relative_difference_percent(left.value, right.value),
                class: EvidenceClass::Derived,
            });
        }
    }
    let missing = (host_a.len() + host_b.len()).saturating_sub(matched * 2) as u64;
    (rows, missing)
}

/// Build a pending section for a host whose second artifact has not arrived.
pub fn pending_section(
    results: &ClimbResults,
    results_path: &str,
    generated_at: String,
) -> ReplicationSection {
    ReplicationSection {
        schema_version: REPLICATION_SCHEMA_VERSION,
        status: ReplicationStatus::Pending,
        generated_at,
        hosts: vec![host_from_results(results, results_path)],
        ratios: Vec::new(),
        max_relative_difference_percent: None,
        tolerance_percent: REPLICATION_TOLERANCE_PERCENT,
        agrees: None,
        missing_ratios: 0,
        note: "one host recorded; awaiting a second host's climb-results.json and \
               environment.json under data/replication/<host-hash>/"
            .to_string(),
        class: EvidenceClass::Derived,
    }
}

/// Build a compared section from two results documents.
pub fn compared_section(
    a: &ClimbResults,
    b: &ClimbResults,
    path_a: &str,
    path_b: &str,
    generated_at: String,
) -> ReplicationSection {
    let (ratios, missing_ratios) = compare_results(a, b);
    let max_relative_difference_percent = ratios
        .iter()
        .map(|row| row.relative_difference_percent)
        .fold(None, |acc: Option<f64>, value| {
            Some(acc.map_or(value, |current| current.max(value)))
        });
    let agrees = max_relative_difference_percent
        .map(|difference| difference <= REPLICATION_TOLERANCE_PERCENT);
    let note = match (max_relative_difference_percent, agrees) {
        (Some(difference), Some(true)) => format!(
            "two hosts; headline ratios agree within {difference:.2}% (tolerance {REPLICATION_TOLERANCE_PERCENT:.0}%)"
        ),
        (Some(difference), Some(false)) => format!(
            "two hosts; headline ratios disagree: largest relative difference {difference:.2}% exceeds the {REPLICATION_TOLERANCE_PERCENT:.0}% tolerance"
        ),
        _ => "two hosts; no headline ratio is present on both".to_string(),
    };
    ReplicationSection {
        schema_version: REPLICATION_SCHEMA_VERSION,
        status: ReplicationStatus::Compared,
        generated_at,
        hosts: vec![host_from_results(a, path_a), host_from_results(b, path_b)],
        ratios,
        max_relative_difference_percent,
        tolerance_percent: REPLICATION_TOLERANCE_PERCENT,
        agrees,
        missing_ratios,
        note,
        class: EvidenceClass::Derived,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample() -> ClimbResults {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/climb-results.json");
        let source = std::fs::read_to_string(&path).expect("read committed sample");
        serde_json::from_str(&source).expect("parse committed sample")
    }

    #[test]
    fn relative_difference_is_symmetric_and_zero_for_equal_values() {
        assert_eq!(relative_difference_percent(10.0, 10.0), 0.0);
        assert_eq!(relative_difference_percent(0.0, 0.0), 0.0);
        let forward = relative_difference_percent(100.0, 90.0);
        let backward = relative_difference_percent(90.0, 100.0);
        assert!((forward - 10.0).abs() < 1e-12);
        assert!((forward - backward).abs() < 1e-12);
        assert!((relative_difference_percent(0.0, 5.0) - 100.0).abs() < 1e-12);
    }

    #[test]
    fn the_committed_sample_compared_with_itself_agrees_at_zero() {
        let results = sample();
        let section = compared_section(
            &results,
            &results,
            "docs/sample-output/climb-results.json",
            "docs/sample-output/climb-results.json",
            "2026-11-09T00:00:00+05:30".to_string(),
        );
        assert_eq!(section.status, ReplicationStatus::Compared);
        assert_eq!(section.hosts.len(), 2);
        assert!(!section.ratios.is_empty(), "the sample carries ratios");
        assert!(section
            .ratios
            .iter()
            .all(|row| row.relative_difference_percent == 0.0));
        assert_eq!(section.max_relative_difference_percent, Some(0.0));
        assert_eq!(section.agrees, Some(true));
        assert_eq!(section.missing_ratios, 0);
        assert!(section.note.contains("agree"), "{}", section.note);
    }

    #[test]
    fn the_committed_sample_covers_every_headline_ratio_kind() {
        let results = sample();
        let ids: Vec<String> = headline_values(&results)
            .into_iter()
            .map(|row| row.id)
            .collect();
        for expected in [
            "overall_wire_reduction_percent:all",
            "largest_wire_reduction_percent:all",
        ] {
            assert!(ids.iter().any(|id| id == expected), "missing {expected}");
        }
        assert!(ids
            .iter()
            .any(|id| id.starts_with("wire_reduction_c_vs_b_percent:")));
        assert!(ids
            .iter()
            .any(|id| id.starts_with("signing_speedup_c_vs_b:")));
        assert!(ids
            .iter()
            .any(|id| id.starts_with("verifying_speedup_cached_vs_a:")));
    }

    #[test]
    fn a_perturbed_host_is_reported_as_disagreeing_with_the_computed_maximum() {
        let a = sample();
        let mut b = sample();
        let first = b
            .comparisons
            .first_mut()
            .expect("the sample has comparison rows");
        let baseline = first
            .wire_reduction_c_vs_b_percent
            .expect("the first row carries a wire reduction");
        first.wire_reduction_c_vs_b_percent = Some(baseline * 0.5);
        let section = compared_section(&a, &b, "a.json", "b.json", "fixed".to_string());
        let expected = relative_difference_percent(baseline, baseline * 0.5);
        assert!((section.max_relative_difference_percent.unwrap() - expected).abs() < 1e-9);
        assert_eq!(section.agrees, Some(false));
        assert!(section.note.contains("disagree"), "{}", section.note);
        let row = section
            .ratios
            .iter()
            .find(|row| row.relative_difference_percent > 0.0)
            .expect("the perturbed row is compared");
        assert_eq!(row.class, EvidenceClass::Derived);
    }

    #[test]
    fn a_missing_ratio_is_counted_not_compared() {
        let a = sample();
        let mut b = sample();
        b.comparisons
            .first_mut()
            .expect("row")
            .signing_speedup_c_vs_b = None;
        let section = compared_section(&a, &b, "a.json", "b.json", "fixed".to_string());
        assert_eq!(section.missing_ratios, 1);
        // When one ratio is missing from one host, the comparison still
        // produces ratios for the sizes that exist on both hosts.
        assert!(
            !section.ratios.is_empty(),
            "at least one ratio should compare"
        );
    }

    #[test]
    fn a_pending_section_has_one_host_and_no_verdict() {
        let results = sample();
        let section = pending_section(
            &results,
            "docs/sample-output/climb-results.json",
            "fixed".to_string(),
        );
        assert_eq!(section.status, ReplicationStatus::Pending);
        assert_eq!(section.hosts.len(), 1);
        assert!(section.ratios.is_empty());
        assert_eq!(section.max_relative_difference_percent, None);
        assert_eq!(section.agrees, None);
        assert_eq!(section.class, EvidenceClass::Derived);
        assert!(section.note.contains("awaiting"), "{}", section.note);
    }

    #[test]
    fn host_facts_read_the_embedded_environment_fingerprint() {
        let results = sample();
        let host = host_from_results(&results, "sample.json");
        assert_eq!(host.host_hash.len(), 12);
        assert!(!host.cpu_model.is_empty());
        assert!(!host.kernel_release.is_empty());
        assert!(host.ratios > 0, "the sample carries headline ratios");
        assert!(!host.sizes.is_empty());
    }
}
