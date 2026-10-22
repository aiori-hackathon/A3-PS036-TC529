//! Contract test for the non-amortised signing baseline in the consolidated
//! results artifact.
//!
//! `docs/sample-output/climb-results.json` is the committed copy of the real
//! consolidated artifact. The non-amortised baseline signs **one complete MTL
//! signature per message**, so its per-message cost is a property of the
//! algorithm and not of the dataset size: the sizes must agree. This test
//! pushes the committed artifact through every consumer that lives in this
//! crate — deserialization, chart generation and the HTML report (including
//! `report --check`) — and asserts the two invariants the artifact has to
//! carry:
//!
//! * the baseline row of every measured size reports exactly one message
//!   (the current benchmark layout, not a leftover batch-shaped row);
//! * the per-message costs of the baseline agree across the sizes within
//!   [`BASELINE_SPREAD_LIMIT`], and the artifact says so.
//!
//! It also pins the guard behaviour: an artifact whose bound was never
//! checked must fail the report check and name the command that regenerates
//! it.

use std::path::{Path, PathBuf};

use climb_report::charts::CHART_FILES;
use climb_report::html::render_html;
use climb_report::{
    check_report, ClimbResults, BASELINE_SPREAD_LIMIT, BOUND_POINTER, CONSOLIDATE_COMMAND,
};

fn sample_results_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
}

fn load_sample() -> ClimbResults {
    let path = sample_results_path();
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read the committed sample artifact {path:?}: {e}"));
    serde_json::from_str(&source).expect("the committed sample artifact parses")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "climb-baseline-contract-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The committed artifact measures the non-amortised baseline as one complete
/// MTL signature per iteration, so every baseline row reports exactly one
/// message.
#[test]
fn committed_baseline_rows_measure_one_signature_per_iteration() {
    let results = load_sample();
    let baselines: Vec<_> = results
        .benchmark
        .signing
        .iter()
        .filter(|row| row.condition == "Without MTL")
        .collect();
    assert!(
        baselines.len() >= 2,
        "the cross-size check needs at least two measured sizes, got {}",
        baselines.len()
    );
    for row in &baselines {
        assert_eq!(
            row.messages, 1,
            "the baseline at {} must report one message per iteration, got {}",
            row.size, row.messages
        );
    }
}

/// The per-message cost of the baseline must not depend on the dataset size:
/// the widest spread between two measured sizes stays inside the limit, and
/// the artifact records the check as passed.
#[test]
fn committed_baseline_agrees_across_sizes() {
    let results = load_sample();
    let per_message: Vec<(String, f64)> = results
        .benchmark
        .signing
        .iter()
        .filter(|row| row.condition == "Without MTL" && row.messages > 0 && row.mean_ns > 0.0)
        .map(|row| (row.size.clone(), row.mean_ns / row.messages as f64))
        .collect();
    assert!(per_message.len() >= 2, "measured sizes: {per_message:?}");

    let fastest = per_message
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::INFINITY, f64::min);
    let slowest = per_message
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let spread = slowest / fastest;
    assert!(
        spread <= BASELINE_SPREAD_LIMIT,
        "per-message baseline spread {spread:.2}x exceeds the {BASELINE_SPREAD_LIMIT:.0}x limit: {per_message:?}"
    );
    assert_eq!(
        results.derived.baseline_ok,
        Some(true),
        "the artifact must record the cross-size check as passed: {:?}",
        results.derived.baseline_offenders
    );
    assert!(results.derived.baseline_offenders.is_empty());
}

/// The bound was checked and it holds — the artifact says so, so the report
/// and the documentation check can rely on it.
#[test]
fn committed_artifact_records_a_checked_bound() {
    let results = load_sample();
    assert_eq!(
        results.derived.bound_ok,
        Some(true),
        "the committed artifact must record a checked, holding bound: {:?}",
        results.derived.bound_offenders
    );
    assert!(results.derived.bound_offenders.is_empty());
}

/// The committed artifact is accepted by every consumer in this crate:
/// chart generation, the HTML report and `report --check`.
#[test]
fn committed_artifact_renders_through_charts_and_report() {
    let results = load_sample();
    let dir = temp_dir("render");
    let charts = dir.join("charts");

    let paths = climb_report::charts::generate_charts(&results, &charts).expect("charts");
    assert_eq!(paths.len(), CHART_FILES.len());

    let html = render_html(&results, &charts, "../climb-results.json").expect("render report");
    assert_eq!(check_report(&html, &results), Vec::<String>::new());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The guard: a copy of the committed artifact whose bound check was removed
/// must fail the report check, and the failure must name the pointer and the
/// command that regenerates the artifact.
#[test]
fn an_artifact_without_the_bound_check_fails_the_report_check() {
    let results = load_sample();
    let dir = temp_dir("guard");
    let charts = dir.join("charts");
    climb_report::charts::generate_charts(&results, &charts).expect("charts");
    let html = render_html(&results, &charts, "../climb-results.json").expect("render report");

    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(sample_results_path()).expect("read"))
            .expect("valid JSON");
    doc["derived"]
        .as_object_mut()
        .expect("derived object")
        .remove("bound_ok");
    let stripped: ClimbResults =
        serde_json::from_value(doc).expect("an artifact without a bound check still parses");
    assert_eq!(stripped.derived.bound_ok, None);

    let failures = check_report(&html, &stripped);
    assert!(
        failures
            .iter()
            .any(|f| f.contains(BOUND_POINTER) && f.contains(CONSOLIDATE_COMMAND)),
        "the failure must name the pointer and the regenerating command: {failures:?}"
    );
}
