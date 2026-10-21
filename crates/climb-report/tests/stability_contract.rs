//! Contract test for the repeated-run stability section of the consolidated
//! results artifact.
//!
//! `docs/sample-output/climb-results.json` is the committed copy of the real
//! consolidated artifact. This test pushes it through every consumer that
//! lives in this crate — deserialization, chart generation and the HTML
//! report (including `report --check`) — so a change to the artifact shape or
//! to the stability section fails here rather than at run time. The section is
//! purely additive (`#[serde(default)]`), so an artifact written before it
//! existed must still parse; the remaining consumer, the TUI, is covered by
//! `climb-tui`'s committed-sample loader test.

use std::path::{Path, PathBuf};

use climb_report::charts::CHART_FILES;
use climb_report::html::render_html;
use climb_report::{check_report, ClimbResults};

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
        "climb-stability-contract-{name}-{}-{}",
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

/// The committed artifact carries a stability section whose per-ratio rows are
/// internally consistent: the recorded spread brackets the median, the
/// coefficient of variation is non-negative and finite, and every row rests on
/// at least one run.
#[test]
fn committed_stability_section_is_self_consistent() {
    let results = load_sample();
    let section = results
        .derived
        .stability
        .as_ref()
        .expect("the committed artifact must exercise the stability section");
    assert!(!section.runs.is_empty());

    for row in &section.runs {
        assert!(
            row.ratio_name.starts_with("signing_speedup_model_"),
            "unexpected ratio name {}",
            row.ratio_name
        );
        assert!(row.n_runs >= 1, "a ratio needs at least one run");
        assert!(
            row.min <= row.median && row.median <= row.max,
            "spread must bracket the median for {}",
            row.ratio_name
        );
        assert!(
            row.cv.is_finite() && row.cv >= 0.0,
            "cv must be a non-negative finite number for {}",
            row.ratio_name
        );
        assert!(row.median > 0.0, "a speed-up ratio is positive");
    }

    // The three headline sizes are each represented once.
    let names: Vec<&str> = section.runs.iter().map(|r| r.ratio_name.as_str()).collect();
    for size in ["small", "onek", "medium"] {
        assert!(
            names.contains(&&*format!("signing_speedup_model_{size}")),
            "missing a stability ratio for {size}: {names:?}"
        );
    }
}

/// The stability section is consumed by every consumer of the artifact: chart
/// generation, the HTML report and `report --check` all accept the committed
/// artifact with the section present.
#[test]
fn committed_stability_section_renders_through_charts_and_report() {
    let results = load_sample();
    let dir = temp_dir("render");
    let charts = dir.join("charts");

    let paths = climb_report::charts::generate_charts(&results, &charts).expect("charts");
    assert_eq!(paths.len(), CHART_FILES.len());

    let html = render_html(&results, &charts, "../climb-results.json").expect("render report");
    assert_eq!(check_report(&html, &results), Vec::<String>::new());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The section is additive: an artifact written before it existed (no
/// `stability` key) must still deserialize, with the section defaulting to
/// `None`, so older results files remain readable.
#[test]
fn an_artifact_without_stability_still_parses() {
    let path = sample_results_path();
    let source = std::fs::read_to_string(&path).expect("read the committed sample artifact");
    let mut doc: serde_json::Value = serde_json::from_str(&source).expect("valid JSON");
    doc["derived"]
        .as_object_mut()
        .expect("derived object")
        .remove("stability");
    let stripped = serde_json::to_string(&doc).expect("re-serialize");
    let parsed: ClimbResults = serde_json::from_str(&stripped)
        .expect("an artifact without a stability section must still parse");
    assert!(parsed.derived.stability.is_none());
}
