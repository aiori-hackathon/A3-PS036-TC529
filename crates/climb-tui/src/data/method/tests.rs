use std::path::{Path, PathBuf};

use super::artifacts::distinct_seed;
use super::*;
use crate::data::test_support::{empty_derived, memory_result, signing_row, view_with};
use climb_report::results::{ConfigComparison, PlainTimingResult};

/// A fresh temp directory laid out like a real run, so the baseline
/// candidate `<dir>/../docs/BASELINES.md` is under the test's control.
fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "climb-tui-method-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// Write `results` into `<root>/target/climb-results.json` and load it
/// through the TUI's own loader, exactly as the runtime does.
fn loaded(root: &Path, results: &climb_report::ClimbResults) -> LoadState {
    std::fs::create_dir_all(root.join("target")).expect("target dir");
    let path = root.join("target/climb-results.json");
    std::fs::write(&path, serde_json::to_vec(results).expect("serialize")).expect("write");
    super::super::load(&path)
}

fn sample_results() -> climb_report::ClimbResults {
    view_with(
        vec![
            signing_row("Small", "With MTL", 1_000.0),
            signing_row("Medium", "With MTL", 2_000.0),
            signing_row("Large", "With MTL", 3_000.0),
        ],
        Vec::new(),
        Vec::new(),
        None,
        empty_derived(),
    )
    .results()
    .clone()
}

#[test]
fn sections_are_always_the_same_titles_in_order() {
    let root = temp_root("order");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let titles: Vec<&str> = view
        .sections()
        .iter()
        .map(|section| section.title.as_str())
        .collect();
    assert_eq!(titles, MethodView::SECTION_TITLES);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unloaded_view_still_lists_every_section_with_an_explanation() {
    let view = MethodView::embedded();
    assert_eq!(view.sections().len(), MethodView::SECTION_TITLES.len());
    for section in view.sections() {
        assert!(
            matches!(section.lines.as_slice(), [MethodLine::Note(_)]),
            "section {:?} must explain the empty state",
            section.title
        );
    }
    assert!(
        view.seeds()
            .iter()
            .all(|row| row.read == ArtifactRead::Unread),
        "no sibling artifact is read without a load"
    );
    assert!(view.environment().is_empty());
}

#[test]
fn scope_carries_the_algorithm_sizes_and_reproduce_commands() {
    let root = temp_root("scope");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let scope = &view.sections()[0];
    assert_eq!(scope.title, "Scope");
    assert!(
        scope
            .lines
            .contains(&MethodLine::code("cargo xtask reproduce")),
        "scope must carry the reproduce command: {:?}",
        scope.lines
    );
    assert!(
        scope
            .lines
            .contains(&MethodLine::code("cargo xtask report --check")),
        "scope must carry the report check command"
    );
    assert!(
        scope.lines.contains(&MethodLine::Field {
            label: "dataset sizes".to_string(),
            value: "Small, Medium, Large".to_string(),
        }),
        "scope must list the sizes the benchmark covered: {:?}",
        scope.lines
    );
    assert!(
        scope.lines.iter().any(|line| matches!(
            line,
            MethodLine::Note(text) if text.contains("no formal security guarantee")
        )),
        "scope must carry the no-security-claim note"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn evidence_class_lines_are_the_four_classes_explained() {
    let root = temp_root("classes");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let classes = &view.sections()[2];
    assert_eq!(classes.title, "Evidence classes");
    for label in ["MEASURED", "EXTRAPOLATED", "ANALYTICAL", "DERIVED"] {
        assert!(
            classes.lines.iter().any(|line| matches!(
                line,
                MethodLine::Field { label: l, .. } if l == label
            )),
            "evidence classes must explain {label}: {:?}",
            classes.lines
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn environment_rows_read_every_field_from_the_artifact() {
    let root = temp_root("env");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let value = |label: &str| {
        view.environment()
            .iter()
            .find(|row| row.label == label)
            .and_then(|row| row.value.clone())
    };
    assert_eq!(value("cpu").as_deref(), Some("Test CPU Model"));
    assert_eq!(value("governor").as_deref(), Some("powersave"));
    assert_eq!(value("pinning").as_deref(), Some("cpu 2"));
    assert_eq!(
        value("rustc").as_deref(),
        Some("rustc 1.98.1 (x 2026-01-01)")
    );
    assert_eq!(value("libMTL pin").as_deref(), Some("abcdef0"));
    assert_eq!(value("liboqs pin").as_deref(), Some("0123456"));
    assert_eq!(value("recorded").as_deref(), Some("2026-10-12T00:00:00Z"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn pinning_prefers_the_recorded_taskset_expression() {
    let mut results = sample_results();
    results.environment.benchmark_runtime = serde_json::json!({
        "cpu_governor": "powersave",
        "pin_cpu": 2,
        "pin_expression": "taskset -c 2",
    });
    let root = temp_root("pinning-expr");
    let state = loaded(&root, &results);
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let pinning = view
        .environment()
        .iter()
        .find(|row| row.label == "pinning")
        .and_then(|row| row.value.clone());
    assert_eq!(pinning.as_deref(), Some("taskset -c 2"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn absent_environment_fields_read_as_none_rather_than_fabricated() {
    let mut results = sample_results();
    results.environment.cpu = serde_json::json!({});
    results.environment.toolchain = serde_json::json!({});
    results.environment.submodules = Vec::new();
    results.environment.timestamp = String::new();
    let root = temp_root("env-absent");
    let state = loaded(&root, &results);
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let cpu = view
        .environment()
        .iter()
        .find(|r| r.label == "cpu")
        .unwrap();
    assert_eq!(cpu.value, None);
    let recorded = view
        .environment()
        .iter()
        .find(|r| r.label == "recorded")
        .unwrap();
    assert_eq!(recorded.value, None, "an empty timestamp is not recorded");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sampling_rows_carry_the_measured_sample_counts_and_classes() {
    let mut results = sample_results();
    results.memory.measured = vec![memory_result("small", 100, 851_968, 2_097_152, 3_232)];
    results.plain = vec![PlainTimingResult {
        size: "Medium".to_string(),
        messages: 10_000,
        signing_mean_ns: Some(689_910_000.0),
        signing_ci_lower_ns: None,
        signing_ci_upper_ns: None,
        verifying_mean_ns: None,
        verifying_ci_lower_ns: None,
        verifying_ci_upper_ns: None,
        class: EvidenceClass::Extrapolated,
        samples: Some(2),
    }];
    results.comparisons = vec![ConfigComparison {
        size: "Small".to_string(),
        wire_reduction_c_vs_a_percent: Some(97.0),
        wire_reduction_c_vs_b_percent: Some(96.7),
        signing_speedup_c_vs_a: Some(100.0),
        signing_speedup_c_vs_b: Some(9_800.0),
        verifying_speedup_cached_vs_a: Some(50.0),
        verifying_speedup_cold_vs_a: Some(49.0),
        class: EvidenceClass::Derived,
    }];
    let root = temp_root("sampling");
    let state = loaded(&root, &results);
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let sampling = &view.sections()[4];
    assert!(
        sampling.lines.contains(&MethodLine::Field {
            label: "memory Small".to_string(),
            value: "3 samples · isolated-process".to_string(),
        }),
        "memory row: {:?}",
        sampling.lines
    );
    assert!(
        sampling.lines.contains(&MethodLine::Field {
            label: "plain Medium".to_string(),
            value: "extrapolated (k = 2)".to_string(),
        }),
        "plain row: {:?}",
        sampling.lines
    );
    assert!(
        sampling.lines.contains(&MethodLine::Field {
            label: "comparison Small".to_string(),
            value: "derived".to_string(),
        }),
        "comparison row: {:?}",
        sampling.lines
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sampling_states_the_extrapolation_identity() {
    let root = temp_root("identity");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let sampling = &view.sections()[4];
    assert!(
        sampling.lines.contains(&MethodLine::code(
            "3 × per-message identity over the k measured sizes".to_string()
        )),
        "the identity line names k: {:?}",
        sampling.lines
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn seed_rows_name_an_absent_artifact_instead_of_dropping_it() {
    let root = temp_root("seeds-absent");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let names: Vec<&str> = view.seeds().iter().map(|row| row.artifact).collect();
    assert_eq!(
        names,
        vec!["wire report", "memory report", "ladder verification"]
    );
    assert!(
        view.seeds()
            .iter()
            .all(|row| row.read == ArtifactRead::Missing),
        "every sibling is absent in a bare temp dir: {:?}",
        view.seeds()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn seed_rows_read_the_committed_artifacts() {
    let root = temp_root("seeds-present");
    let state = loaded(&root, &sample_results());
    // The committed ladder fixture next to the results exercises the real
    // artifact path (§7c: the TUI consumes the artifact, not a fake).
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/sample-output/ladder-verification.recorded.json");
    std::fs::copy(&fixture, root.join("target/ladder-verification.json"))
        .expect("copy ladder fixture");
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let ladder = view
        .seeds()
        .iter()
        .find(|row| row.artifact == "ladder verification")
        .expect("ladder seed row");
    assert_eq!(
        ladder.read,
        ArtifactRead::Read {
            seed: Some(4_849_331_494_946_035_253),
            detail: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        },
        "the seed and algorithm come from the artifact, not a literal"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_present_but_unparseable_sibling_is_a_hard_error_naming_the_path() {
    let root = temp_root("seeds-invalid");
    let state = loaded(&root, &sample_results());
    let path = root.join("target/wire-report.json");
    std::fs::write(&path, "{ not json at all").expect("write garbage");
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let wire = view
        .seeds()
        .iter()
        .find(|row| row.artifact == "wire report")
        .expect("wire seed row");
    match &wire.read {
        ArtifactRead::Invalid { path: p, error } => {
            assert_eq!(p, &path, "the invalid state names the exact path");
            assert!(!error.is_empty());
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn memory_seed_uses_the_shared_artifact_parser() {
    let root = temp_root("seeds-memory-shape");
    let state = loaded(&root, &sample_results());
    // A legacy in-process delta report: the shared parser recognizes it,
    // so the Method tab reads its seed rather than declaring it unreadable.
    std::fs::write(
        root.join("target/memory-report.json"),
        r#"{"seed":1,"runs":[{"size":"Small","runs":[
            {"rss_before_bytes":1,"rss_after_bytes":3,"delta_bytes":2,"delta_mib":0.0}],
           "repeat_count":1}]}"#,
    )
    .expect("write legacy memory report");
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    let memory = view
        .seeds()
        .iter()
        .find(|row| row.artifact == "memory report")
        .expect("memory seed row");
    assert_eq!(
        memory.read,
        ArtifactRead::Read {
            seed: Some(1),
            detail: Some("legacy rss-delta".to_string()),
        }
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_readable_baseline_copy_next_to_the_results_is_preferred_over_embedded() {
    let root = temp_root("baselines-file");
    let state = loaded(&root, &sample_results());
    // The candidate is `<results dir>/../docs/BASELINES.md`.
    std::fs::create_dir_all(root.join("docs")).expect("docs dir");
    std::fs::write(
        root.join("docs/BASELINES.md"),
        "# Test baselines\nMTL amortised\n",
    )
    .expect("write baselines");
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    assert!(
        matches!(view.baselines().source, BaselineSource::File(_)),
        "a readable copy wins: {:?}",
        view.baselines().source
    );
    assert!(view.baselines().text.contains("MTL amortised"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_a_readable_copy_the_embedded_baseline_document_is_used() {
    let root = temp_root("baselines-embedded");
    let state = loaded(&root, &sample_results());
    let view = MethodView::build(&root.join("target/climb-results.json"), &state);
    assert_eq!(view.baselines().source, BaselineSource::Embedded);
    assert!(
        view.baselines().text.contains("Signing baselines"),
        "the embedded document is the repository one"
    );
}

#[test]
fn seed_text_formats_every_read_outcome() {
    assert_eq!(
        seed_text(&ArtifactRead::Read {
            seed: Some(7),
            detail: Some("2 size rows".to_string())
        }),
        "seed 7 · 2 size rows"
    );
    assert_eq!(
        seed_text(&ArtifactRead::Read {
            seed: Some(7),
            detail: None
        }),
        "seed 7"
    );
    assert_eq!(
        seed_text(&ArtifactRead::Read {
            seed: None,
            detail: Some("legacy comparison rows (3)".to_string())
        }),
        "legacy comparison rows (3)"
    );
    assert_eq!(
        seed_text(&ArtifactRead::Missing),
        "not on disk — this run did not produce it"
    );
    assert!(
        seed_text(&ArtifactRead::Invalid {
            path: PathBuf::from("x.json"),
            error: "bad".to_string()
        })
        .contains("x.json"),
        "an invalid read names the path"
    );
}

#[test]
fn distinct_seed_rejects_disagreeing_rows() {
    assert_eq!(distinct_seed(&[5, 5, 5]), Some(5));
    assert_eq!(
        distinct_seed(&[5, 6]),
        None,
        "disagreement is never resolved by picking one"
    );
    assert_eq!(distinct_seed(&[]), None);
}
