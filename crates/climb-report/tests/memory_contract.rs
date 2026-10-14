use std::path::{Path, PathBuf};

use climb_report::{
    check_report, consolidate_results, ClimbResults, ConsolidateOptions, ReportError,
    ISOLATED_PROCESS_METHOD,
};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "climb-memory-contract-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn consolidate_memory(
    root: &Path,
    memory_report_path: Option<PathBuf>,
) -> Result<ClimbResults, ReportError> {
    let criterion = root.join("criterion");
    std::fs::create_dir_all(&criterion).unwrap();
    consolidate_results(ConsolidateOptions {
        force: true,
        criterion_dir: criterion,
        wire_report_path: None,
        memory_report_path,
        dns_capture_path: root.join("no-capture.json"),
        dns_fixture_path: root.join("no-fixture.json"),
        environment_path: root.join("no-env.json"),
        baseline: "new".to_string(),
        output_path: root.join("out.json"),
    })
}

#[test]
fn committed_isolated_fixture_consolidates_and_survives_every_consumer() {
    let fixture = fixture_dir().join("memory-report.isolated.json");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
    let expected_runs = expected["runs"].as_array().unwrap();
    assert_eq!(expected_runs.len(), 3);

    let root = temp_root("isolated");
    let results = consolidate_memory(&root, Some(fixture)).unwrap();
    assert_eq!(results.memory.measured.len(), expected_runs.len());

    // Every consolidated field is cross-checked against the committed
    // artifact itself, never against a literal copied into the test.
    for expected_run in expected_runs {
        let size = expected_run["size"].as_str().unwrap();
        let row = results
            .memory
            .measured
            .iter()
            .find(|m| m.size == size)
            .unwrap_or_else(|| panic!("missing memory row for {size}"));
        assert_eq!(
            row.median_bytes,
            expected_run["median_bytes"].as_u64().unwrap(),
            "median_bytes for {size}"
        );
        assert_eq!(
            row.iqr_bytes,
            expected_run["iqr_bytes"].as_u64().unwrap(),
            "iqr_bytes for {size}"
        );
        assert_eq!(
            row.samples,
            expected_run["samples"].as_array().unwrap().len(),
            "samples for {size}"
        );
        assert_eq!(row.method, ISOLATED_PROCESS_METHOD, "method for {size}");
        assert_eq!(
            row.measured_mean_bytes, row.median_bytes,
            "isolated rows report the median as the measured central value"
        );
        assert_eq!(
            row.measured_range_min_bytes,
            expected_run["min_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            row.measured_range_max_bytes,
            expected_run["max_bytes"].as_u64().unwrap()
        );
    }

    // serde_json's default float parser is not bit-exact on round-trip, so the
    // structural and integer fields are compared rather than the whole struct
    // (derived f64 ratios may shift by one ULP).
    let json = serde_json::to_string(&results).unwrap();
    let reparsed: ClimbResults = serde_json::from_str(&json).unwrap();
    assert_eq!(
        reparsed.memory.measured.len(),
        results.memory.measured.len()
    );
    for (before, after) in results
        .memory
        .measured
        .iter()
        .zip(&reparsed.memory.measured)
    {
        assert_eq!(before.size, after.size);
        assert_eq!(before.message_count, after.message_count);
        assert_eq!(before.measured_mean_bytes, after.measured_mean_bytes);
        assert_eq!(before.median_bytes, after.median_bytes);
        assert_eq!(before.iqr_bytes, after.iqr_bytes);
        assert_eq!(before.samples, after.samples);
        assert_eq!(before.method, after.method);
        assert_eq!(
            before.measured_range_min_bytes,
            after.measured_range_min_bytes
        );
        assert_eq!(
            before.measured_range_max_bytes,
            after.measured_range_max_bytes
        );
    }

    let charts = root.join("charts");
    climb_report::charts::generate_charts(&reparsed, &charts).unwrap();
    let html =
        climb_report::html::render_html(&reparsed, &charts, "../climb-results.json").unwrap();
    let unbacked = check_report(&html, &reparsed);
    assert!(unbacked.is_empty(), "unbacked claims: {unbacked:?}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn committed_legacy_fixtures_still_consolidate() {
    let root = temp_root("legacy");

    let delta = fixture_dir().join("memory-report.legacy-rss-delta.json");
    let results = consolidate_memory(&root, Some(delta)).unwrap();
    assert_eq!(results.memory.measured.len(), 3);
    for row in &results.memory.measured {
        assert_eq!(row.method, "rss-delta");
        assert_eq!(row.samples, 2);
        assert!(row.median_bytes > 0);
    }

    let comparison = fixture_dir().join("memory-report.legacy-comparison.json");
    let results = consolidate_memory(&root, Some(comparison)).unwrap();
    assert_eq!(results.memory.measured.len(), 3);
    for row in &results.memory.measured {
        assert_eq!(row.method, "rss-comparison");
        assert_eq!(row.median_bytes, row.measured_mean_bytes);
        assert_eq!(row.iqr_bytes, 0);
        assert_eq!(row.samples, 0);
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unrecognized_artifact_names_every_shape() {
    for (label, content) in [
        ("garbage", "this is not json at all"),
        (
            "unknown-method",
            r#"{"method":"mystery","seed":1,"runs":[]}"#,
        ),
    ] {
        let message = climb_report::parse_memory_artifact(content)
            .unwrap_err()
            .to_string();
        for shape in [
            ISOLATED_PROCESS_METHOD,
            "legacy-rss-delta",
            "legacy-comparison",
        ] {
            assert!(
                message.contains(shape),
                "{label}: error must name `{shape}`: {message}"
            );
        }
    }
}

#[test]
fn present_but_invalid_artifact_is_a_hard_error_while_absent_only_warns() {
    let root = temp_root("invalid");
    let bad = root.join("memory.json");
    std::fs::write(&bad, r#"{"method":"mystery","seed":1,"runs":[]}"#).unwrap();

    match consolidate_memory(&root, Some(bad.clone())) {
        Err(ReportError::MalformedData { path, reason }) => {
            assert_eq!(path, bad, "error must name the offending path");
            assert!(
                reason.contains("legacy-rss-delta"),
                "reason must explain the shapes tried: {reason}"
            );
        }
        other => panic!("expected MalformedData for a present-but-invalid artifact, got {other:?}"),
    }

    let absent = root.join("does-not-exist.json");
    let results = consolidate_memory(&root, Some(absent.clone())).unwrap();
    assert!(results.memory.measured.is_empty());
    assert!(
        results
            .warnings
            .iter()
            .any(|w| w.contains("memory report missing")),
        "an absent artifact must warn, not error: {:?}",
        results.warnings
    );

    let _ = std::fs::remove_dir_all(&root);
}
