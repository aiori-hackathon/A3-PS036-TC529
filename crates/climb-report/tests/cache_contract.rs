//! Contract tests for the resolver-cache section.
//!
//! The committed raw measurement fixture must consolidate into a `cache_use`
//! section that every consumer accepts (charts, HTML report, quote sheet), and
//! the committed consolidated sample must keep the measured and analytical
//! rows separate on every surface.

use std::path::{Path, PathBuf};

use climb_report::deck_facts::{generate, DeckFactsInput, Section};
use climb_report::results::{size_display, EvidenceClass};
use climb_report::{
    check_report, consolidate_results, ClimbResults, ConsolidateOptions, ReportError,
};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn committed_sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "climb-cache-contract-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn consolidate_cache(root: &Path) -> Result<ClimbResults, ReportError> {
    let criterion = root.join("criterion");
    std::fs::create_dir_all(&criterion).unwrap();
    consolidate_results(ConsolidateOptions {
        force: true,
        criterion_dir: criterion,
        wire_report_path: Some(fixture_dir().join("wire-report.small.json")),
        memory_report_path: None,
        dns_capture_path: root.join("no-capture.json"),
        dns_fixture_path: root.join("no-fixture.json"),
        ladder_verification_path: root.join("no-ladder-live.json"),
        ladder_fixture_path: root.join("no-ladder-fixture.json"),
        replication_summary_path: root.join("no-replication-summary.json"),
        replication_fixture_path: root.join("no-replication-fixture.json"),
        dns_rtt_path: root.join("no-dns-rtt.json"),
        dns_rtt_fixture_path: root.join("no-dns-rtt-fixture.json"),
        cache_use_path: fixture_dir().join("cache-use.small.json"),
        cache_use_fixture_path: root.join("no-cache-use-fixture.json"),
        environment_path: root.join("no-env.json"),
        baseline: "new".to_string(),
        rtt_ms: None,
        classical_signatures_path: root.join("no-classical-signatures.json"),
        output_path: root.join("out.json"),
        reproduction_mode: climb_report::ReproductionMode::Unknown,
        generated_at: None,
        workspace_root: None,
    })
}

fn committed_results() -> ClimbResults {
    serde_json::from_str(&std::fs::read_to_string(committed_sample()).expect("read sample"))
        .expect("committed sample parses")
}

#[test]
fn the_committed_raw_fixture_consolidates_and_survives_every_consumer() {
    let root = temp_root("raw");
    let mut results = consolidate_cache(&root).expect("fixture consolidates");

    let cache = results.cache_use.as_ref().expect("cache section present");
    assert_eq!(cache.measured.len(), 4, "one row per raw measurement run");
    assert_eq!(cache.analytical.len(), 2, "one analytical row per config");
    for row in &cache.analytical {
        assert!(row.measured_bytes > 0, "measured median for {}", row.config);
        assert!(row.analytical_bytes > 0, "prediction for {}", row.config);
        assert!(row.measured_over_analytical > 0.0);
    }

    // The report self-check rejects an artifact without a bound check; this
    // fixture carries no Criterion measurements, so mark it as checked here.
    results.derived.bound_ok = Some(true);

    // The consolidated document round-trips through JSON before the consumers
    // read it, exactly as the on-disk artifact does.
    let json = serde_json::to_string(&results).expect("serialise");
    let reparsed: ClimbResults = serde_json::from_str(&json).expect("reparse");
    assert_eq!(
        reparsed
            .cache_use
            .as_ref()
            .map(|cache| cache.measured.len()),
        Some(4)
    );

    let charts = root.join("charts");
    climb_report::charts::generate_charts(&reparsed, &charts).expect("charts render");
    let html = climb_report::html::render_html(&reparsed, &charts, "../climb-results.json")
        .expect("html renders");
    let unbacked = check_report(&html, &reparsed);
    assert!(unbacked.is_empty(), "unbacked claims: {unbacked:?}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_committed_sample_parses_through_charts_and_the_html_report() {
    let results = committed_results();
    let cache = results.cache_use.as_ref().expect("cache section present");
    assert_eq!(
        cache.measured.len(),
        20,
        "five runs x two sizes x two configs"
    );
    assert_eq!(cache.analytical.len(), 4, "two sizes x two configs");

    let root = temp_root("sample");
    let charts = root.join("charts");
    climb_report::charts::generate_charts(&results, &charts).expect("charts render");
    let html = climb_report::html::render_html(&results, &charts, "../climb-results.json")
        .expect("html renders");
    assert!(
        html.contains("Resolver cache"),
        "the report carries a resolver-cache section"
    );
    let unbacked = check_report(&html, &results);
    assert!(unbacked.is_empty(), "unbacked claims: {unbacked:?}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn measured_and_analytical_rows_are_never_merged() {
    let results = committed_results();
    let input = DeckFactsInput {
        results_path: committed_sample(),
        wire_report_path: PathBuf::from("/nonexistent/wire-report.json"),
        memory_report_path: PathBuf::from("/nonexistent/memory-report.json"),
        ladder_verification_path: PathBuf::from("/nonexistent/ladder.json"),
        ladder_fixture_path: PathBuf::from("/nonexistent/ladder-fixture.json"),
        dns_capture_path: PathBuf::from("/nonexistent/dns-capture.json"),
        dns_fixture_path: PathBuf::from("/nonexistent/dns-fixture.json"),
        replication_summary_path: PathBuf::from("/nonexistent/replication.json"),
        replication_fixture_path: PathBuf::from("/nonexistent/replication-fixture.json"),
        dns_rtt_path: PathBuf::from("/nonexistent/dns-rtt.json"),
        dns_rtt_fixture_path: PathBuf::from("/nonexistent/dns-rtt-fixture.json"),
        environment_path: PathBuf::from("/nonexistent/environment.json"),
        commit_sha: None,
        test_summary: None,
    };
    let output = generate(&input).expect("quote sheet generates");
    let rows: Vec<_> = output
        .rows
        .iter()
        .filter(|fact| fact.section == Section::ResolverCache)
        .collect();
    assert!(!rows.is_empty(), "the sheet carries resolver-cache rows");

    for fact in &rows {
        let merged = fact.label.contains("(measured)") && fact.label.contains("(analytical)");
        assert!(
            !merged,
            "a single row must not mix the two evidence classes: {}",
            fact.label
        );
        match fact.class {
            Some(EvidenceClass::Measured) => assert!(
                fact.pointer.ends_with("/measured_bytes"),
                "measured row points at measured bytes: {}",
                fact.pointer
            ),
            Some(EvidenceClass::Analytical) => assert!(
                fact.pointer.ends_with("/analytical_bytes"),
                "analytical row points at predicted bytes: {}",
                fact.pointer
            ),
            Some(EvidenceClass::Derived) => assert!(
                fact.pointer.ends_with("/measured_over_analytical"),
                "ratio row points at the ratio: {}",
                fact.pointer
            ),
            other => panic!("cache row without a MEASURED/ANALYTICAL class: {other:?}"),
        }
    }

    // Each (size, configuration) cell carries exactly one row of each kind.
    let cache = results.cache_use.as_ref().expect("cache section present");
    for analytical in &cache.analytical {
        let size = results
            .wire_size
            .iter()
            .find(|wire| wire.message_count == analytical.r)
            .map(|wire| size_display(&wire.size))
            .unwrap_or_default();
        let config = if analytical.config == "mtl-condensed" {
            "MTL-condensed"
        } else {
            "plain"
        };
        let cell: Vec<_> = rows
            .iter()
            .filter(|fact| {
                fact.label.contains(size.as_str())
                    && fact.label.contains(config)
                    && fact.label.contains('—')
            })
            .collect();
        assert_eq!(
            cell.len(),
            3,
            "one measured, one analytical and one ratio row for {size}/{config}: {cell:?}"
        );
    }

    // The headline metrics keep the same split: the byte rows are MEASURED and
    // the ratio is DERIVED, never a single combined row.
    let metrics = climb_report::numbers::headline_metrics(&results);
    let plain = metrics
        .iter()
        .find(|metric| metric.id == "cache_rrset_bytes_medium_plain")
        .expect("measured plain row");
    assert_eq!(plain.class, "MEASURED");
    let mtl = metrics
        .iter()
        .find(|metric| metric.id == "cache_rrset_bytes_medium_mtl")
        .expect("measured MTL row");
    assert_eq!(mtl.class, "MEASURED");
    let ratio = metrics
        .iter()
        .find(|metric| metric.id == "cache_ratio_medium_mtl")
        .expect("derived ratio row");
    assert_eq!(ratio.class, "DERIVED");
}
