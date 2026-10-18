//! Contract test for the cold/warm reuse section of the consolidated results
//! artifact.
//!
//! `docs/sample-output/climb-results.json` is the committed copy of the real
//! consolidated artifact. This test pushes it through every consumer that lives
//! in this crate — deserialization, chart generation and the HTML report
//! (including `report --check`) — so a change to the artifact shape or to the
//! reuse section fails here rather than at run time. The remaining consumer, the
//! TUI, is covered by `climb-tui`'s
//! `committed_sample_results_expose_reuse_rows_to_the_tui`.

use std::path::{Path, PathBuf};

use climb_report::{check_report, html::build_claims, ClimbResults};

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
        "climb-reuse-contract-{name}-{}-{}",
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

/// The committed artifact carries a reuse curve for every size that has both a
/// wire row and the two verifying conditions, with break-evens cross-checked
/// against the artifact's own numbers (never against literals in this test).
#[test]
fn committed_reuse_section_is_self_consistent() {
    let results = load_sample();
    assert!(
        !results.derived.reuse.is_empty(),
        "the committed artifact must exercise the reuse section"
    );

    for row in &results.derived.reuse {
        let wire = results
            .wire_size
            .iter()
            .find(|w| w.size.eq_ignore_ascii_case(&row.size))
            .unwrap_or_else(|| panic!("no wire row for reuse size {}", row.size));
        assert_eq!(row.batch_size, wire.message_count as u64);
        assert_eq!(row.baseline_bytes, wire.plain_signature_len_bytes as f64);
        assert!(!row.curve.is_empty());

        // r=1 must equal the cold verify once and the fully-unamortised bytes.
        let first = &row.curve[0];
        assert_eq!(first.reuse, 1);
        assert!(
            (first.effective_bytes - (wire.condensed_mean + wire.ladder_len as f64)).abs() < 1e-6,
            "r=1 bytes must be one condensed path plus one full ladder"
        );

        // Ratio is strictly decreasing once r grows beyond 1, and the
        // break-evens recorded on the row are the first candidate to reach
        // parity, exactly as a consumer would recompute them.
        let baseline_t = row.baseline_time_ns;
        let baseline_b = row.baseline_bytes;
        let expected_time = row
            .curve
            .iter()
            .find(|p| p.effective_time_ns <= baseline_t + 1e-12)
            .map(|p| p.reuse);
        let expected_bytes = row
            .curve
            .iter()
            .find(|p| p.effective_bytes <= baseline_b + 1e-12)
            .map(|p| p.reuse);
        assert_eq!(row.breakeven_reuse_time, expected_time);
        assert_eq!(row.breakeven_reuse_bytes, expected_bytes);
        assert_eq!(row.r_star_time, row.breakeven_reuse_time);
        assert_eq!(row.r_star_bytes, row.breakeven_reuse_bytes);
    }
}

/// Every consumer of the artifact — chart generation and the HTML report
/// including `--check` — accepts the committed artifact and renders the reuse
/// section.
#[test]
fn committed_reuse_section_renders_through_charts_and_report() {
    let results = load_sample();
    let dir = temp_dir("render");
    let charts = dir.join("charts");

    let paths = climb_report::charts::generate_charts(&results, &charts).expect("charts");
    assert_eq!(paths.len(), 7);
    let reuse_svg = charts.join("reuse_curve.svg");
    assert!(reuse_svg.exists(), "the reuse chart must be written");
    let svg = std::fs::read_to_string(&reuse_svg).expect("read reuse svg");
    assert!(svg.contains("<svg") && svg.contains("</svg>"));
    assert!(
        svg.contains("Ladder reuse vs reuse count"),
        "the reuse chart must carry its caption"
    );

    let html = climb_report::html::render_html(&results, &charts, "../climb-results.json")
        .expect("render report");
    assert!(
        html.contains("Ladder reuse: cold vs warm"),
        "the report must render the reuse section"
    );

    // Every reuse claim's rendered string must appear verbatim in the body,
    // which is exactly what `report --check` re-validates.
    let claims = build_claims(&results).expect("claims");
    assert!(
        claims.iter().any(|c| c.id.starts_with("reuse-")),
        "the registry must carry reuse claims"
    );
    for claim in claims.iter().filter(|c| c.id.starts_with("reuse-")) {
        assert!(
            html.contains(&claim.formatted)
                || html.contains(&claim.formatted.replace('&', "&amp;")),
            "claim {} rendered string missing: {}",
            claim.id,
            claim.formatted
        );
    }
    assert_eq!(check_report(&html, &results), Vec::<String>::new());

    let _ = std::fs::remove_dir_all(&dir);
}
