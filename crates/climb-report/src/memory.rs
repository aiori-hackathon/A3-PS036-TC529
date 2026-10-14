//! The memory-report artifact: its on-disk shape and a discriminating parser.
//!
//! `cargo xtask memory-report` writes a single JSON document describing an
//! isolated-process RSS probe (one process per sample, so the delta window is
//! clean).  Two older shapes predate that method and may still be on disk, so
//! the artifact is recognized in three forms:
//!
//! * [`MemoryReportJson`] — the current isolated-process report, identified by
//!   its top-level `method` field;
//! * [`climb_bench::RssDeltaReport`] — the in-process delta report (top-level
//!   `seed` + `runs`, no `method` field);
//! * `Vec<climb_bench::RssComparison>` — the pre-consolidation comparison rows
//!   (a bare JSON array).
//!
//! [`parse_memory_artifact`] picks the shape by inspecting the JSON structure
//! (the top-level kind and, for objects, the `method` value) rather than
//! letting a lenient serde fall-through guess, so an artifact with an unknown
//! `method` is reported as unrecognized instead of being silently accepted.

use serde::{Deserialize, Serialize};

/// Value of the `method` field written by the isolated-process probe, and the
/// only value [`MemoryReportJson`] accepts there.
pub const ISOLATED_PROCESS_METHOD: &str = "isolated-process";

/// One RSS probe sample: the process-start baseline, the intermediate stage
/// points, the post-sign resident reading and the process high-water mark, all
/// in kilobytes.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProbeLine {
    /// Dataset size label this sample belongs to (`small`, `medium`, `large`).
    pub size: String,
    /// Process RSS in kilobytes at process start, before the dataset or keypair
    /// is generated. Every stage delta is measured from this baseline.
    pub baseline_kb: u64,
    /// RSS in kilobytes after dataset generation. `None` in artifacts written
    /// before stage attribution was recorded.
    #[serde(default)]
    pub after_dataset_kb: Option<u64>,
    /// RSS in kilobytes after keypair generation. `None` in artifacts written
    /// before stage attribution was recorded.
    #[serde(default)]
    pub after_keypair_kb: Option<u64>,
    /// RSS in kilobytes sampled after `sign_batch` returned.
    pub after_kb: u64,
    /// Peak RSS in kilobytes observed during the probe.
    pub hwm_kb: u64,
    /// Length in bytes of the key image the probe's keypair was built from;
    /// `None` in artifacts written before this field was recorded.
    #[serde(default)]
    pub key_image_bytes: Option<u64>,
}

/// Median and interquartile range of one measured stage's delta, in kilobytes.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct StageStats {
    /// Median delta in kilobytes.
    pub median_kb: f64,
    /// Interquartile range in kilobytes.
    pub iqr_kb: u64,
}

/// Per-stage delta breakdown for one size, all in kilobytes.
///
/// Each delta is measured from process start to that stage's sample point
/// (`dataset`, `keypair`, `sign`) or is a derived whole-run figure (`resident`
/// = after − baseline, `peak` = hwm − baseline).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct StageDeltas {
    /// Dataset generation delta.
    pub dataset: StageStats,
    /// Keypair generation delta.
    pub keypair: StageStats,
    /// `sign_batch` delta.
    pub sign: StageStats,
    /// Whole-run resident delta (`after − baseline`).
    pub resident: StageStats,
    /// Whole-run peak delta (`hwm − baseline`): the headline footprint metric.
    pub peak: StageStats,
}

/// Per-size summary of the isolated-process probe.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SizeReport {
    /// Dataset size label (`small`, `medium`, `large`).
    pub size: String,
    /// Number of messages in the dataset for this size.
    pub message_count: usize,
    /// Measurement method; equal to [`ISOLATED_PROCESS_METHOD`].
    pub method: String,
    /// Individual probe samples.
    pub samples: Vec<ProbeLine>,
    /// Per-sample RSS deltas in kilobytes (`after_kb - baseline_kb`).
    pub deltas_kb: Vec<u64>,
    /// Median delta in kilobytes.
    pub median_kb: f64,
    /// First-quartile delta in kilobytes.
    pub q1_kb: u64,
    /// Third-quartile delta in kilobytes.
    pub q3_kb: u64,
    /// Interquartile range in kilobytes (`q3_kb - q1_kb`).
    pub iqr_kb: u64,
    /// Minimum delta in kilobytes.
    pub min_kb: u64,
    /// Maximum delta in kilobytes.
    pub max_kb: u64,
    /// Median delta in bytes.
    pub median_bytes: u64,
    /// Minimum delta in bytes.
    pub min_bytes: u64,
    /// Maximum delta in bytes.
    pub max_bytes: u64,
    /// Interquartile range in bytes.
    pub iqr_bytes: u64,
    /// Per-stage delta breakdown; `None` in artifacts written before stage
    /// attribution was recorded.
    #[serde(default)]
    pub stages: Option<StageDeltas>,
    /// Why this size's sample count was chosen as it was (e.g. a `Large` run
    /// pinned to three samples because one sample exceeded the time budget);
    /// `None` when the default count was used.
    #[serde(default)]
    pub note: Option<String>,
}

/// The JSON document written by `cargo xtask memory-report`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MemoryReportJson {
    /// Measurement method; equal to [`ISOLATED_PROCESS_METHOD`].
    pub method: String,
    /// Dataset seed the probe used.
    pub seed: u64,
    /// One summary per dataset size.
    pub runs: Vec<SizeReport>,
}

/// A recognized memory-report document, in whichever shape it was written.
#[derive(Debug, Clone)]
pub enum MemoryArtifact {
    /// The current isolated-process probe report.
    Isolated(MemoryReportJson),
    /// The in-process RSS delta report (no top-level `method` field).
    LegacyRssDelta(climb_bench::RssDeltaReport),
    /// The pre-consolidation comparison rows (a bare JSON array).
    LegacyComparison(Vec<climb_bench::RssComparison>),
}

/// The memory-report document is present but matched none of the known shapes.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct MemoryParseError {
    message: String,
}

impl MemoryParseError {
    /// The input is not valid JSON, so no shape could be attempted. Names every
    /// shape the parser would have tried.
    fn invalid_json(source: &serde_json::Error) -> Self {
        Self {
            message: format!(
                "invalid JSON ({source}); tried shapes: {ISOLATED_PROCESS_METHOD}, \
                 legacy-rss-delta, legacy-comparison"
            ),
        }
    }

    /// A single shape was selected structurally but failed to deserialize.
    fn shape_failed(shape: &str, source: &serde_json::Error) -> Self {
        Self {
            message: format!("unrecognized memory artifact; tried {shape} ({source})"),
        }
    }

    /// No shape matched structurally. Names all three shapes and the first
    /// serde error each produced, so an operator can see why each was rejected.
    fn no_shape_matched(root: &serde_json::Value, method: Option<&str>) -> Self {
        let detail = attempt_shapes(root)
            .into_iter()
            .map(|(name, result)| match result {
                Ok(()) => format!("{name} (parsed)"),
                Err(e) => format!("{name} ({e})"),
            })
            .collect::<Vec<_>>()
            .join("; ");
        let lead = match method {
            Some(m) => format!("unrecognized memory artifact method '{m}'"),
            None => "unrecognized memory artifact shape".to_string(),
        };
        Self {
            message: format!("{lead}; tried shapes: {detail}"),
        }
    }
}

/// Try each known shape against `root`, returning the first serde error for
/// each (or `Ok(())` when a shape parses). Used only to build error messages.
fn attempt_shapes(root: &serde_json::Value) -> [(&'static str, Result<(), String>); 3] {
    fn attempt<T: serde::de::DeserializeOwned>(root: &serde_json::Value) -> Result<(), String> {
        serde_json::from_value::<T>(root.clone())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    [
        (ISOLATED_PROCESS_METHOD, attempt::<MemoryReportJson>(root)),
        (
            "legacy-rss-delta",
            attempt::<climb_bench::RssDeltaReport>(root),
        ),
        (
            "legacy-comparison",
            attempt::<Vec<climb_bench::RssComparison>>(root),
        ),
    ]
}

/// Parse a memory-report document, selecting the shape by structure.
///
/// # Errors
///
/// Returns [`MemoryParseError`] when the document is not valid JSON, or when it
/// matches none of the known shapes.  The message names every shape tried and
/// the first serde error for each.
pub fn parse_memory_artifact(content: &str) -> Result<MemoryArtifact, MemoryParseError> {
    let root: serde_json::Value =
        serde_json::from_str(content).map_err(|e| MemoryParseError::invalid_json(&e))?;

    match &root {
        // Only the legacy comparison shape is a bare array.
        serde_json::Value::Array(_) => {
            match serde_json::from_value::<Vec<climb_bench::RssComparison>>(root) {
                Ok(rows) => Ok(MemoryArtifact::LegacyComparison(rows)),
                Err(e) => Err(MemoryParseError::shape_failed("legacy-comparison", &e)),
            }
        }
        serde_json::Value::Object(map) => {
            match map.get("method").and_then(serde_json::Value::as_str) {
                Some(ISOLATED_PROCESS_METHOD) => {
                    match serde_json::from_value::<MemoryReportJson>(root) {
                        Ok(doc) => Ok(MemoryArtifact::Isolated(doc)),
                        Err(e) => Err(MemoryParseError::shape_failed(ISOLATED_PROCESS_METHOD, &e)),
                    }
                }
                // A `method` value we do not know is a hard "unrecognized" rather
                // than a lenient guess at one of the older shapes.
                Some(other) => Err(MemoryParseError::no_shape_matched(&root, Some(other))),
                // No `method` key: the only object shape without one is the
                // legacy in-process delta report.
                None => match serde_json::from_value::<climb_bench::RssDeltaReport>(root) {
                    Ok(report) => Ok(MemoryArtifact::LegacyRssDelta(report)),
                    Err(e) => Err(MemoryParseError::shape_failed("legacy-rss-delta", &e)),
                },
            }
        }
        _ => Err(MemoryParseError::no_shape_matched(&root, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISOLATED: &str = r#"{"method":"isolated-process","seed":7,"runs":[
      {"size":"small","message_count":100,"method":"isolated-process","samples":[
         {"size":"small","baseline_kb":100,"after_kb":204,"hwm_kb":204}],
       "deltas_kb":[104],"median_kb":104.0,"q1_kb":104,"q3_kb":104,"iqr_kb":0,
       "min_kb":104,"max_kb":104,"median_bytes":106496,"min_bytes":106496,
       "max_bytes":106496,"iqr_bytes":0}]}"#;

    const LEGACY_DELTA: &str = r#"{"seed":1,"runs":[
      {"size":"Small","runs":[
        {"rss_before_bytes":1,"rss_after_bytes":3,"delta_bytes":2,"delta_mib":0.0}],
       "repeat_count":1}]}"#;

    const LEGACY_COMPARISON: &str = r#"[
      {"size":"Small","message_count":100,"measured_mean_bytes":851968,
       "analytical_bytes":2097152,"ratio":0.406,"measured_range_bytes":[73728,1630208],
       "measured_stddev_bytes":778240.0}]"#;

    /// The current shape, with stage attribution and the key-image provenance.
    const STAGED: &str = r#"{"method":"isolated-process","seed":1,"runs":[
      {"size":"large","message_count":1000000,"method":"isolated-process",
       "samples":[{"size":"large","baseline_kb":10,"after_dataset_kb":100,
         "after_keypair_kb":110,"after_kb":300,"hwm_kb":320,
         "key_image_bytes":186}],
       "deltas_kb":[290],"median_kb":290.0,"q1_kb":290,"q3_kb":290,"iqr_kb":0,
       "min_kb":290,"max_kb":290,"median_bytes":296960,"min_bytes":296960,
       "max_bytes":296960,"iqr_bytes":0,
       "stages":{
         "dataset":{"median_kb":90.0,"iqr_kb":0},
         "keypair":{"median_kb":10.0,"iqr_kb":0},
         "sign":{"median_kb":190.0,"iqr_kb":0},
         "resident":{"median_kb":290.0,"iqr_kb":0},
         "peak":{"median_kb":310.0,"iqr_kb":0}},
       "note":"large measured with n=3 (one sample exceeded 90 s)"}]}"#;

    #[test]
    fn parses_the_isolated_process_shape_by_method() {
        match parse_memory_artifact(ISOLATED).unwrap() {
            MemoryArtifact::Isolated(doc) => {
                assert_eq!(doc.method, ISOLATED_PROCESS_METHOD);
                assert_eq!(doc.runs.len(), 1);
                assert_eq!(doc.runs[0].median_bytes, 106_496);
                assert_eq!(doc.runs[0].samples.len(), 1);
            }
            other => panic!("expected isolated shape, got {other:?}"),
        }
    }

    #[test]
    fn pre_stage_isolated_samples_parse_with_none_stage_fields() {
        match parse_memory_artifact(ISOLATED).unwrap() {
            MemoryArtifact::Isolated(doc) => {
                let sample = &doc.runs[0].samples[0];
                assert_eq!(sample.after_dataset_kb, None);
                assert_eq!(sample.after_keypair_kb, None);
                assert_eq!(sample.key_image_bytes, None);
                assert_eq!(doc.runs[0].stages, None);
                assert_eq!(doc.runs[0].note, None);
            }
            other => panic!("expected isolated shape, got {other:?}"),
        }
    }

    #[test]
    fn parses_stage_attribution_and_key_image_provenance() {
        match parse_memory_artifact(STAGED).unwrap() {
            MemoryArtifact::Isolated(doc) => {
                let run = &doc.runs[0];
                let sample = &run.samples[0];
                assert_eq!(sample.after_dataset_kb, Some(100));
                assert_eq!(sample.after_keypair_kb, Some(110));
                assert_eq!(sample.key_image_bytes, Some(186));
                let stages = run.stages.as_ref().expect("stages present");
                assert_eq!(stages.dataset.median_kb, 90.0);
                assert_eq!(stages.keypair.median_kb, 10.0);
                assert_eq!(stages.sign.median_kb, 190.0);
                assert_eq!(stages.resident.median_kb, 290.0);
                assert_eq!(stages.peak.median_kb, 310.0);
                assert_eq!(
                    run.note.as_deref(),
                    Some("large measured with n=3 (one sample exceeded 90 s)")
                );
            }
            other => panic!("expected isolated shape, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_legacy_delta_shape_without_a_method_field() {
        match parse_memory_artifact(LEGACY_DELTA).unwrap() {
            MemoryArtifact::LegacyRssDelta(report) => assert_eq!(report.runs.len(), 1),
            other => panic!("expected legacy delta shape, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_legacy_comparison_array() {
        match parse_memory_artifact(LEGACY_COMPARISON).unwrap() {
            MemoryArtifact::LegacyComparison(rows) => assert_eq!(rows.len(), 1),
            other => panic!("expected legacy comparison shape, got {other:?}"),
        }
    }

    #[test]
    fn unknown_method_and_invalid_json_name_every_shape() {
        for input in [
            "not json at all",
            r#"{"method":"mystery","seed":1,"runs":[]}"#,
        ] {
            let message = parse_memory_artifact(input).unwrap_err().to_string();
            for shape in [
                ISOLATED_PROCESS_METHOD,
                "legacy-rss-delta",
                "legacy-comparison",
            ] {
                assert!(
                    message.contains(shape),
                    "error must name {shape}: {message}"
                );
            }
        }
    }
}
