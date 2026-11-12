use super::*;
use std::time::Duration;

/// Copy the committed sample results and report into a fresh temp
/// directory laid out like a real run (`<dir>/climb-report/report.html`)
/// and hand back the results path.
fn sample_dir(tag: &str) -> PathBuf {
    let root = crate::data::fixtures::temp_root("climb-tui-integrity", tag);
    std::fs::create_dir_all(root.join("climb-report")).expect("temp dir");
    crate::data::fixtures::copy_fixture("climb-results.json", &root.join("climb-results.json"));
    crate::data::fixtures::copy_fixture("report.html", &root.join("climb-report/report.html"));
    root
}

#[test]
fn run_checks_the_copied_sample_report_and_reports_agreement() {
    let root = sample_dir("agree");
    let state = super::super::load(&root.join("climb-results.json"));
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let integrity = view.integrity();
    let (tone, banner) = integrity.banner();
    assert_eq!(tone, BannerTone::Ok, "banner: {banner}");
    assert!(banner.starts_with("ALL CLAIMS AGREE"), "banner: {banner}");
    assert!(
        integrity
            .claims()
            .iter()
            .all(|row| row.status == CheckStatus::Agree),
        "every claim row must agree for the committed sample pair"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A violated signing speed-up bound is a failing claim: the banner reads
/// BOUND VIOLATION even when the report and the results agree that it is
/// violated (no drift).
#[test]
fn a_violated_bound_flips_the_banner_to_bound_violation() {
    let root = sample_dir("bound");
    let results_path = root.join("climb-results.json");
    // Flip the committed sample's bound flag to violated, keeping every
    // other field (and the report) untouched.
    let text = std::fs::read_to_string(&results_path).expect("read sample results");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    doc["derived"]["bound_ok"] = serde_json::json!(false);
    doc["derived"]["bound_offenders"] = serde_json::json!(["signing_speedup_c_vs_b Small (N=100)"]);
    std::fs::write(
        &results_path,
        serde_json::to_vec_pretty(&doc).expect("serialize"),
    )
    .expect("write flipped results");

    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the flipped sample results to load, got {state:?}");
    };
    let integrity = view.integrity();
    let (tone, banner) = integrity.banner();
    assert_eq!(tone, BannerTone::Bad, "banner: {banner}");
    assert_eq!(banner, "BOUND VIOLATION");
    let _ = std::fs::remove_dir_all(&root);
}

/// A results document that never checked the signing speed-up bound
/// cannot back any claim: the banner is Bad and names the command that
/// produces a document which does carry the check.
#[test]
fn an_unchecked_bound_flips_the_banner_to_bad() {
    let root = sample_dir("unchecked");
    let results_path = root.join("climb-results.json");
    let text = std::fs::read_to_string(&results_path).expect("read sample results");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    doc["derived"]
        .as_object_mut()
        .expect("derived object")
        .remove("bound_ok");
    std::fs::write(
        &results_path,
        serde_json::to_vec_pretty(&doc).expect("serialize"),
    )
    .expect("write stripped results");

    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the stripped sample results to load, got {state:?}");
    };
    let integrity = view.integrity();
    assert_eq!(integrity.bound_state(), BoundState::Unchecked);
    let (tone, banner) = integrity.banner();
    assert_eq!(tone, BannerTone::Bad, "banner: {banner}");
    assert!(banner.contains("BOUND NOT CHECKED"), "banner: {banner}");
    assert!(
        banner.contains(CONSOLIDATE_COMMAND),
        "the regenerating command must be named: {banner}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn run_inventories_the_reference_outputs_and_inputs() {
    let root = sample_dir("inventory");
    let results_path = root.join("climb-results.json");
    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let artifacts = view.integrity().artifacts();
    assert_eq!(artifacts.len(), ARTIFACTS.len() + 1);
    assert_eq!(artifacts[0].role, ArtifactRole::Reference);
    assert_eq!(artifacts[0].path, "climb-results.json");
    assert_eq!(artifacts[0].state, ArtifactState::Present);
    let report = artifacts
        .iter()
        .find(|row| row.path == "climb-report/report.html")
        .expect("report row");
    assert_eq!(report.role, ArtifactRole::Output);
    assert_eq!(
        report.state,
        ArtifactState::Present,
        "copied after the results"
    );
    let charts = artifacts
        .iter()
        .find(|row| row.path == "climb-report/charts")
        .expect("charts row");
    assert_eq!(
        charts.state,
        ArtifactState::Missing,
        "no charts dir was copied"
    );
    let wire = artifacts
        .iter()
        .find(|row| row.path == "wire-report.json")
        .expect("wire row");
    assert_eq!(wire.role, ArtifactRole::Input);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn outputs_older_than_the_results_file_are_marked_stale() {
    let root = sample_dir("stale");
    let results_path = root.join("climb-results.json");
    // Backdate the report so it predates the results file.
    let report = root.join("climb-report/report.html");
    let old = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    filetime_set(&report, old);
    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let report_row = view
        .integrity()
        .artifacts()
        .iter()
        .find(|row| row.path == "climb-report/report.html")
        .expect("report row");
    assert_eq!(report_row.state, ArtifactState::Stale);
    assert_eq!(report_row.modified, Some(old));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn environment_fingerprint_is_found_at_the_workspace_root() {
    let root = sample_dir("env-root");
    let results_path = root.join("climb-results.json");
    rewrite_workspace_root(&results_path, &root);
    std::fs::write(root.join("environment.json"), "{}").expect("write environment");
    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let row = view
        .integrity()
        .artifacts()
        .iter()
        .find(|row| row.path == "environment.json")
        .expect("environment row");
    assert_eq!(row.state, ArtifactState::Present);
    assert!(row.modified.is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn environment_fingerprint_is_found_under_target() {
    let root = sample_dir("env-target");
    let results_path = root.join("climb-results.json");
    rewrite_workspace_root(&results_path, &root);
    std::fs::create_dir_all(root.join("target")).expect("target dir");
    std::fs::write(root.join("target/environment.json"), "{}").expect("write environment");
    let state = super::super::load(&results_path);
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let row = view
        .integrity()
        .artifacts()
        .iter()
        .find(|row| row.path == "environment.json")
        .expect("environment row");
    assert_eq!(row.state, ArtifactState::Present);
    let _ = std::fs::remove_dir_all(&root);
}

/// Point a copied results document at a test workspace so its root-level
/// artifacts resolve inside the temp directory, not this checkout.
fn rewrite_workspace_root(results_path: &Path, workspace_root: &Path) {
    let text = std::fs::read_to_string(results_path).expect("read results");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    doc["workspace_root"] = serde_json::json!(workspace_root.display().to_string());
    std::fs::write(
        results_path,
        serde_json::to_vec_pretty(&doc).expect("serialize"),
    )
    .expect("write results");
}

/// Set a file's modification time by reading, rewriting and restoring
/// it — the only portable way to backdate a file from a unit test.
fn filetime_set(path: &Path, when: SystemTime) {
    let bytes = std::fs::read(path).expect("read");
    std::fs::write(path, &bytes).expect("rewrite");
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open");
    file.set_times(std::fs::FileTimes::new().set_modified(when))
        .expect("set mtime");
}

#[test]
fn absent_report_is_a_warning_state_that_still_lists_every_claim() {
    let root = crate::data::fixtures::temp_root("climb-tui-integrity", "absent");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp dir");
    crate::data::fixtures::copy_fixture("climb-results.json", &root.join("climb-results.json"));

    let state = super::super::load(&root.join("climb-results.json"));
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let integrity = view.integrity();
    let (tone, banner) = integrity.banner();
    assert_eq!(tone, BannerTone::Warn, "banner: {banner}");
    assert!(banner.starts_with("REPORT NOT FOUND"), "banner: {banner}");
    assert!(
        !integrity.claims().is_empty(),
        "an absent report still lists every claim as missing"
    );
    assert!(
        integrity
            .claims()
            .iter()
            .all(|row| row.status == CheckStatus::Missing),
        "every row is missing without a report"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn provenance_carries_commit_submodules_rustc_and_host_fingerprint() {
    let root = sample_dir("provenance");
    let state = super::super::load(&root.join("climb-results.json"));
    let super::super::LoadState::Loaded(view) = state else {
        panic!("expected the sample results to load, got {state:?}");
    };
    let provenance = view.integrity().provenance();
    // The committed sample records two submodules with full SHAs.
    assert_eq!(provenance.submodules.len(), 2);
    for (path, sha) in &provenance.submodules {
        assert!(path.starts_with("third_party/"), "submodule path: {path}");
        assert_eq!(sha.len(), 40, "full SHA for {path}");
    }
    assert!(
        provenance
            .rustc
            .as_deref()
            .is_some_and(|v| v.starts_with("rustc")),
        "rustc: {:?}",
        provenance.rustc
    );
    assert_eq!(provenance.host_fingerprint.len(), 16);
    assert!(
        provenance
            .host_fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit()),
        "fingerprint must be hex: {}",
        provenance.host_fingerprint
    );
    assert_eq!(provenance.generated_at, view.results().generated_at);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn host_fingerprint_differs_for_different_environments() {
    use climb_report::results::EnvironmentFingerprint;
    let mut first = test_support_environment();
    let second = EnvironmentFingerprint {
        cpu: serde_json::json!({ "model_name": "Another CPU" }),
        ..first.clone()
    };
    first.cpu = serde_json::json!({ "model_name": "Some CPU" });
    assert_ne!(
        environment_fingerprint_hash(&first),
        environment_fingerprint_hash(&second),
        "different environments must hash differently"
    );
    assert_eq!(
        environment_fingerprint_hash(&first),
        environment_fingerprint_hash(&first),
        "the hash is deterministic"
    );
}

fn test_support_environment() -> climb_report::results::EnvironmentFingerprint {
    super::super::test_support::environment()
}

#[test]
fn commit_sha_follows_a_symbolic_head_into_the_ref_file() {
    let root = std::env::temp_dir().join(format!("climb-tui-integrity-git-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".git/refs/heads")).expect("temp git dir");
    std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(
        root.join(".git/refs/heads/main"),
        "0123456789abcdef0123456789abcdef01234567\n",
    )
    .expect("ref");
    assert_eq!(
        git_commit_sha(&root.display().to_string()),
        Some("0123456789abcdef0123456789abcdef01234567".to_string())
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn commit_sha_is_none_outside_a_git_checkout() {
    assert_eq!(git_commit_sha("/nonexistent/workspace"), None);
}

#[test]
fn mtime_formatting_is_pinned_to_utc() {
    // 2026-01-01T00:00:00Z.
    let new_year = UNIX_EPOCH + Duration::from_secs(1_767_225_600);
    assert_eq!(
        format_utc(new_year).as_deref(),
        Some("2026-01-01 00:00 UTC")
    );
    // 2026-10-19T09:03:00Z.
    let october = UNIX_EPOCH + Duration::from_secs(1_792_400_580);
    assert_eq!(format_utc(october).as_deref(), Some("2026-10-19 09:03 UTC"));
}

#[test]
fn pre_epoch_times_format_as_unknown() {
    assert_eq!(format_utc(UNIX_EPOCH - Duration::from_secs(1)), None);
}

#[test]
fn artifact_lines_ellipsize_to_the_budget() {
    let row = ArtifactRow {
        path: "climb-classical-signatures.json".to_string(),
        role: ArtifactRole::Input,
        modified: None,
        state: ArtifactState::Missing,
    };
    let short = artifact_line(&row, 12, true);
    assert!(short.chars().count() <= 12, "short line: {short}");
    assert!(short.contains('~'), "truncation marker: {short}");
    let long = artifact_line(&row, 200, true);
    assert!(long.contains("climb-classical-signatures.json"));
    assert!(long.contains("missing"));
}

#[test]
fn not_run_view_renders_as_neutral_placeholders() {
    let view = IntegrityView::not_run();
    let (tone, banner) = view.banner();
    assert_eq!(tone, BannerTone::Warn);
    assert_eq!(banner, "CHECK NOT RUN");
    assert!(view.claims().is_empty());
    assert!(view.artifacts().is_empty());
    assert_eq!(view.report_path(), None);
    assert_eq!(view.drift_count(), None);
    assert_eq!(view.provenance().host_fingerprint, "");
}
