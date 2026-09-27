//! Integration test: parse the committed fixture tree and produce a text summary.
//!
//! Fixtures mirror the real Criterion 3-level directory layout. This test
//! exercises `scan_criterion`, `summarize`, and `render_text_summary` end to end.

use climb_report::{render_text_summary, scan_criterion, summarize, BenchKind, MtlMode, Size};

#[test]
fn scan_fixture_tree_returns_expected_count() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion");

    let records = scan_criterion(&root, "new").unwrap();

    // Fixture tree: 3 leaf dirs.
    assert_eq!(records.len(), 3);
}

#[test]
fn scan_fixture_tree_ordering_is_deterministic() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion");

    let records_a = scan_criterion(&root, "new").unwrap();
    let records_b = scan_criterion(&root, "new").unwrap();

    assert_eq!(records_a, records_b);
}

#[test]
fn summarize_fixture_records() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion");

    let records = scan_criterion(&root, "new").unwrap();
    let rows = summarize(&records).unwrap();

    assert_eq!(rows.len(), 3);

    // signing_small_with_mtl → (Signing, Small, WithMtlFullVerify)
    let signing_with = &rows[0];
    assert_eq!(signing_with.kind, BenchKind::Signing);
    assert_eq!(signing_with.size, Size::Small);
    assert_eq!(signing_with.mode, MtlMode::WithMtlFullVerify);
    assert_eq!(signing_with.elements, Some(100));
    // mean is ~682.9 ms → 682_919_952 ns.
    assert!((signing_with.mean_ns - 682_919_952.51).abs() < 10.0);

    // signing_small_without_mtl → (Signing, Small, WithoutMtl)
    let signing_wo = &rows[1];
    assert_eq!(signing_wo.kind, BenchKind::Signing);
    assert_eq!(signing_wo.size, Size::Small);
    assert_eq!(signing_wo.mode, MtlMode::WithoutMtl);
    assert_eq!(signing_wo.elements, Some(100));

    // verifying_medium_with_mtl_trust_true → (Verifying, Medium, WithMtlTrustCached)
    let verifying = &rows[2];
    assert_eq!(verifying.kind, BenchKind::Verifying);
    assert_eq!(verifying.size, Size::Medium);
    assert_eq!(verifying.mode, MtlMode::WithMtlTrustCached);
    assert_eq!(verifying.elements, Some(10_000));
}

#[test]
fn render_text_summary_contains_expected_substrings() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion");

    let records = scan_criterion(&root, "new").unwrap();
    let rows = summarize(&records).unwrap();
    let text = render_text_summary(&rows);

    // Signing table
    assert!(text.contains("=== Signing ==="));
    assert!(text.contains("With MTL"));
    assert!(text.contains("Without MTL"));
    assert!(text.contains("682.9 ms"));
    assert!(text.contains("1.09 min"));
    // Speedup: 65238500000 / 682919952.51 ≈ 95.5
    assert!(text.contains("95.53x"));

    // Verifying table
    assert!(text.contains("=== Verifying ==="));
    assert!(text.contains("MTL (trust cached)"));
    assert!(text.contains("115.0 ms"));
}

#[test]
fn render_text_summary_speedup_formatting() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion");

    let records = scan_criterion(&root, "new").unwrap();
    let rows = summarize(&records).unwrap();
    let text = render_text_summary(&rows);

    // signing_with_mtl (682.9 ms) row should have a speedup number
    // signing_without_mtl (65238.5 ms) row should have "–" for speedup
    let lines: Vec<&str> = text.lines().collect();

    let signing_with_line = lines
        .iter()
        .find(|l| l.contains("With MTL") && l.contains("682.9"))
        .unwrap();
    let signing_wo_line = lines
        .iter()
        .find(|l| l.contains("Without MTL") && l.contains("1.09"))
        .unwrap();

    // With MTL line has speedup (95.5x)
    assert!(
        signing_with_line.contains("95.53x"),
        "expected speedup in: {signing_with_line}"
    );

    // Without MTL line should have dash for speedup
    assert!(
        signing_wo_line.contains('–') || signing_wo_line.contains('-'),
        "expected no speedup in: {signing_wo_line}"
    );
}
