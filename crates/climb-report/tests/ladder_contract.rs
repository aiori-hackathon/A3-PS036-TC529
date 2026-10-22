//! Contract test for the ladder section of the consolidated results artifact.
//!
//! `data/ladder-verification.recorded.json` is the committed copy of the real
//! ladder-verification artifact. This test pushes it through the consumers of
//! the artifact chain that live in this crate — consolidation, chart
//! generation and the HTML report (including `report --check`) — so a change to
//! either the artifact or the section's shape fails here rather than at run
//! time. The remaining consumer, the TUI, is covered in `climb-tui`'s own
//! `consolidated_ladder_section_parses_through_the_tui_loader`.

use std::path::{Path, PathBuf};

use climb_report::{
    check_report, consolidate_results, html::claims, ConsolidateOptions, LadderSource, ReportError,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ladder_fixture() -> PathBuf {
    repo_root().join("data/ladder-verification.recorded.json")
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "climb-ladder-contract-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn consolidate_from_committed_fixture(root: &Path) -> climb_report::ClimbResults {
    let criterion = root.join("criterion");
    std::fs::create_dir_all(&criterion).unwrap();
    consolidate_results(ConsolidateOptions {
        force: true,
        criterion_dir: criterion,
        wire_report_path: None,
        memory_report_path: None,
        dns_capture_path: root.join("no-capture.json"),
        dns_fixture_path: root.join("no-fixture.json"),
        ladder_verification_path: root.join("no-live.json"),
        ladder_fixture_path: ladder_fixture(),
        environment_path: root.join("no-env.json"),
        baseline: "new".to_string(),
        rtt_ms: None,
        classical_signatures_path: root.join("no-classical-signatures.json"),
        output_path: root.join("climb-results.json"),
    })
    .expect("consolidate from the committed ladder fixture")
}

fn consolidate(
    root: &Path,
    live: PathBuf,
    fixture: PathBuf,
) -> Result<climb_report::ClimbResults, ReportError> {
    let criterion = root.join("criterion");
    std::fs::create_dir_all(&criterion).unwrap();
    consolidate_results(ConsolidateOptions {
        force: true,
        criterion_dir: criterion,
        wire_report_path: None,
        memory_report_path: None,
        dns_capture_path: root.join("no-capture.json"),
        dns_fixture_path: root.join("no-fixture.json"),
        ladder_verification_path: live,
        ladder_fixture_path: fixture,
        environment_path: root.join("no-env.json"),
        rtt_ms: None,
        baseline: "new".to_string(),
        classical_signatures_path: root.join("no-classical-signatures.json"),
        output_path: root.join("climb-results.json"),
    })
}

#[test]
fn consolidated_ladder_section_reproduces_every_committed_fixture_row() {
    let root = temp_root("section");
    let results = consolidate_from_committed_fixture(&root);
    let section = results
        .ladder
        .as_ref()
        .expect("consolidated results carry a ladder section");
    assert_eq!(section.source, LadderSource::RecordedFixture);

    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ladder_fixture()).expect("read fixture"))
            .expect("the committed fixture is valid JSON");
    let rows = fixture["sizes"]
        .as_array()
        .expect("the committed fixture carries sizes");

    assert_eq!(
        section.sizes.len(),
        rows.len(),
        "the section covers every measured size"
    );
    for (consolidated, expected) in section.sizes.iter().zip(rows) {
        assert_eq!(
            consolidated.leaf_count as u64,
            expected["leaf_count"].as_u64().expect("leaf_count")
        );
        assert_eq!(
            consolidated.rung_count as u64,
            expected["rung_count"].as_u64().expect("rung_count")
        );
        assert_eq!(
            consolidated.ladder_buffer_len as u64,
            expected["ladder_buffer_len"]
                .as_u64()
                .expect("ladder_buffer_len")
        );
        assert_eq!(
            consolidated.signed_ladder_len as u64,
            expected["signed_ladder_len"]
                .as_u64()
                .expect("signed_ladder_len")
        );
        assert_eq!(
            consolidated.condensed_min as u64,
            expected["condensed"]["min"]
                .as_u64()
                .expect("condensed min")
        );
        assert_eq!(
            consolidated.condensed_max as u64,
            expected["condensed"]["max"]
                .as_u64()
                .expect("condensed max")
        );
        assert_eq!(
            consolidated.condensed_mean,
            expected["condensed"]["mean"]
                .as_f64()
                .expect("condensed mean")
        );
        assert!(
            consolidated.model_matches,
            "the model must reproduce the measured row for N={}",
            consolidated.leaf_count
        );
    }
    assert!(section.all_match, "all sizes must match the model");
    assert_eq!(
        section.recorded_at,
        fixture["recorded_at"].as_str().expect("recorded_at")
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn charts_and_report_render_the_ladder_section_and_check_stays_clean() {
    let root = temp_root("render");
    let mut results = consolidate_from_committed_fixture(&root);
    // This fixture carries no Criterion measurements, so the signing speed-up
    // bound was never checked. The report self-check rejects an artifact
    // without a bound check — that guard has its own tests — so the field is
    // filled in here to keep this test about the ladder section.
    results.derived.bound_ok = Some(true);

    let charts = root.join("charts");
    std::fs::create_dir_all(&charts).expect("charts dir");
    climb_report::charts::generate_charts(&results, &charts).expect("charts");
    let html = climb_report::html::render_html(&results, &charts, "../climb-results.json")
        .expect("render report");

    assert!(
        html.contains("Ladder: model vs libMTL"),
        "the report must render the ladder section"
    );
    assert!(
        html.contains("all sizes match") || html.contains("sizes match"),
        "the report must state the model-agreement claim"
    );

    // Every ladder claim's rendered string must appear verbatim in the body,
    // which is exactly what `report --check` re-validates.
    let claims = claims(&results).expect("claims");
    for claim in claims.iter().filter(|c| c.id.starts_with("ladder-")) {
        assert!(
            html.contains(&claim.formatted)
                || html.contains(&claim.formatted.replace('&', "&amp;")),
            "claim {} rendered string missing: {}",
            claim.id,
            claim.formatted
        );
    }
    assert_eq!(check_report(&html, &results), Vec::<String>::new());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_present_but_invalid_ladder_artifact_is_a_hard_error_naming_the_path() {
    let root = temp_root("invalid");
    let live = root.join("live-ladder.json");
    std::fs::write(&live, "{ not json").unwrap();

    let err = consolidate(&root, live.clone(), root.join("no-fixture.json")).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("live-ladder.json"),
        "the error must name the offending path: {message}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_live_ladder_artifact_overrides_the_committed_fixture() {
    let root = temp_root("live");
    let live = root.join("live-ladder.json");
    std::fs::copy(ladder_fixture(), &live).expect("copy the fixture as the live artifact");

    let results = consolidate(&root, live, root.join("no-fixture.json")).expect("consolidate");
    let section = results.ladder.expect("ladder section");
    assert_eq!(section.source, LadderSource::Live);
    assert!(section.all_match);

    let _ = std::fs::remove_dir_all(&root);
}
