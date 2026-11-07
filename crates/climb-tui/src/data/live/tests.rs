use super::*;
use climb_report::{DnsCaptureFlags, DnsCaptureHost, DnsCaptureQuery};

/// Test-data builders: a minimal capture with the same shape the recorder
/// writes. The byte counts here are fixtures for the projection tests.
fn query(pass: &str, qname: &str, qtype: &str, bytes: u64) -> DnsCaptureQuery {
    DnsCaptureQuery {
        label: pass.to_string(),
        qname: qname.to_string(),
        qtype: qtype.to_string(),
        response_bytes: bytes,
        answer_rrsig_bytes: None,
        flags: DnsCaptureFlags {
            ad: None,
            tc: None,
            rcode: Some(0),
        },
        transport: "tcp".to_string(),
        edns_udp_size: None,
        elapsed_ms: None,
        wire_hex: None,
    }
}

fn capture(queries: Vec<DnsCaptureQuery>, git_commit: Option<&str>) -> DnsCapture {
    DnsCapture {
        schema_version: 1,
        recorded_at: "2026-10-12T14:24:26+05:30".to_string(),
        git_commit: git_commit.map(str::to_string),
        host: DnsCaptureHost {
            hostname: "test-host".to_string(),
            uname: "Linux test".to_string(),
        },
        images: Vec::new(),
        algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
        zone: "climb.example.".to_string(),
        queries,
    }
}

fn sample_capture() -> DnsCapture {
    capture(
        vec![
            query("full", "www.climb.example.", "A", 24_438),
            query("condensed", "www.climb.example.", "A", 658),
            query("full", "climb.example.", "SOA", 24_465),
        ],
        None,
    )
}

fn view() -> LiveView {
    LiveView::new(sample_capture(), LiveSource::RecordedFixture)
}

#[test]
fn step_list_has_a_query_and_an_answer_leg_per_recorded_packet() {
    let view = view();
    assert_eq!(
        view.steps().len(),
        6,
        "three recorded packets become six legs"
    );
    assert_eq!(view.steps()[0].direction, Direction::Query);
    assert_eq!(view.steps()[0].label, "query");
    assert_eq!(
        view.steps()[0].bytes,
        None,
        "the query size is not recorded"
    );
    assert_eq!(view.steps()[1].direction, Direction::Answer);
    assert_eq!(view.steps()[1].bytes, Some(24_438));
    assert_eq!(view.steps()[1].transport, "tcp");
}

#[test]
fn answer_leg_labels_name_the_capture_pass() {
    let capture = capture(
        vec![
            query("full", "climb.example.", "SOA", 100),
            query("condensed", "climb.example.", "SOA", 20),
            query("unbound", "climb.example.", "SOA", 60),
            query("weird", "climb.example.", "SOA", 1),
        ],
        None,
    );
    let view = LiveView::new(capture, LiveSource::LiveRun);
    let labels: Vec<&str> = view
        .steps()
        .iter()
        .filter(|step| step.direction == Direction::Answer)
        .map(|step| step.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec![
            "answer (full)",
            "answer (condensed)",
            "answer (resolver)",
            "answer (weird)"
        ]
    );
}

#[test]
fn steps_carry_a_running_byte_counter_across_the_capture() {
    let view = view();
    let running: Vec<u64> = view.steps().iter().map(|step| step.running_bytes).collect();
    // Each query leg shows the total before its packet, each answer leg
    // after it: 24,438 + 658 + 24,465 (the SOA record).
    assert_eq!(running, vec![0, 24_438, 24_438, 25_096, 25_096, 49_561]);
}

#[test]
fn steps_mark_only_the_headline_question() {
    let view = view();
    let headline: Vec<bool> = view.steps().iter().map(|step| step.headline).collect();
    assert_eq!(headline, vec![true, true, true, true, false, false]);
    assert_eq!(view.steps()[4].question, "climb.example. SOA");
    assert_eq!(view.steps()[0].question, "www.climb.example. A");
}

#[test]
fn headline_lanes_split_the_passes_with_lane_totals() {
    let lanes = view().lanes();
    assert_eq!(lanes.question, "www.climb.example. A");
    assert!(lanes.without_mtl.present);
    assert!(lanes.with_mtl.present);
    assert_eq!(lanes.without_mtl.total_bytes, 24_438);
    assert_eq!(lanes.with_mtl.total_bytes, 658);
    assert_eq!(lanes.max_bytes, 24_438);
    let reduction = lanes.reduction_percent.expect("both lanes present");
    assert!(
        (reduction - 97.307_37).abs() < 1e-3,
        "reduction={reduction}"
    );
}

#[test]
fn lane_running_counters_accumulate_within_the_lane() {
    let lanes = view().lanes();
    let without: Vec<u64> = lanes
        .without_mtl
        .steps
        .iter()
        .map(|step| step.running_bytes)
        .collect();
    assert_eq!(without, vec![0, 24_438]);
    let with_mtl: Vec<u64> = lanes
        .with_mtl
        .steps
        .iter()
        .map(|step| step.running_bytes)
        .collect();
    assert_eq!(with_mtl, vec![0, 658]);
}

#[test]
fn lanes_at_marks_rows_revealed_as_the_cursor_reaches_them() {
    let view = view();
    // The headline rows of the sample sit at indices 0/1 (full) and 2/3
    // (condensed).
    let early = view.lanes_at(0);
    assert!(early.without_mtl.steps[0].revealed, "cursor on the row");
    assert!(!early.without_mtl.steps[1].revealed, "row still pending");
    assert!(
        !early.with_mtl.steps[1].revealed,
        "the other lane is pending too"
    );
    let all = view.lanes();
    assert!(all.with_mtl.steps.iter().all(|step| step.revealed));
}

#[test]
fn lanes_are_absent_when_the_capture_has_no_headline_rows() {
    let capture = capture(vec![query("full", "climb.example.", "SOA", 100)], None);
    let lanes = LiveView::new(capture, LiveSource::LiveRun).lanes();
    assert!(!lanes.without_mtl.present);
    assert!(!lanes.with_mtl.present);
    assert_eq!(lanes.max_bytes, 0);
    assert_eq!(lanes.reduction_percent, None);
}

#[test]
fn bar_cells_is_linear_by_default() {
    // Test data: the sample's headline pair.
    assert_eq!(bar_cells(Some(24_438), 24_438, 40, false), 40);
    assert_eq!(bar_cells(Some(12_219), 24_438, 40, false), 20);
    assert_eq!(bar_cells(Some(658), 24_438, 40, false), 1, "rounds, min 1");
}

#[test]
fn bar_cells_in_log_mode_lifts_small_steps() {
    // Test data: the sample's headline pair. Log scale over a 1-byte floor
    // makes 658 B of 24,438 B more than half the column.
    let log = bar_cells(Some(658), 24_438, 40, true);
    let linear = bar_cells(Some(658), 24_438, 40, false);
    assert!(log > 20, "log={log}");
    assert!(log > linear, "log={log} linear={linear}");
    assert_eq!(bar_cells(Some(24_438), 24_438, 40, true), 40, "max fills");
}

#[test]
fn bar_cells_handles_missing_bytes_zero_width_and_never_overflows() {
    assert_eq!(bar_cells(None, 24_438, 40, false), 0, "no bar for no size");
    assert_eq!(
        bar_cells(Some(0), 24_438, 40, false),
        0,
        "zero draws nothing"
    );
    assert_eq!(bar_cells(Some(658), 24_438, 0, false), 0);
    assert_eq!(bar_cells(Some(658), 0, 40, false), 0, "no scale, no bar");
    assert_eq!(bar_cells(Some(99_999), 24_438, 40, false), 40, "clamps");
    assert_eq!(bar_cells(Some(1), 24_438, 40, true), 1, "log floor");
}

#[test]
fn header_line_names_the_date_and_commit() {
    let mut capture = sample_capture();
    capture.git_commit = Some("312c68268c96878b0ee01922d94c447cc9d0287e".to_string());
    let header = LiveView::new(capture, LiveSource::LiveRun).header_line(false);
    assert_eq!(
        header, "RECORDED CAPTURE 2026-10-12 \u{b7} commit 312c68268c96",
        "the date is the timestamp's day, the commit is shortened"
    );
}

#[test]
fn header_line_marks_an_absent_commit() {
    assert_eq!(
        view().header_line(false),
        "RECORDED CAPTURE 2026-10-12 \u{b7} commit \u{2014}"
    );
    assert_eq!(
        view().header_line(true),
        "RECORDED CAPTURE 2026-10-12 - commit -",
        "ascii mode stays ascii"
    );
}

#[test]
fn flag_text_reports_observed_and_set_flags() {
    assert_eq!(flag_text(None, None, false), "\u{2014}", "unobserved");
    assert_eq!(flag_text(Some(true), None, false), "TC");
    assert_eq!(flag_text(None, Some(true), false), "ad");
    assert_eq!(flag_text(Some(false), Some(true), false), "\u{b7} ad");
    assert_eq!(flag_text(None, None, true), "-", "ascii dash");
    assert_eq!(flag_text(Some(false), None, true), ".", "ascii dot");
}

// ── loading ─────────────────────────────────────────────────────

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "climb-tui-live-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn sample_json() -> String {
    serde_json::to_string(&sample_capture()).expect("serialize")
}

#[test]
fn load_of_missing_capture_is_missing_state() {
    let path = std::env::temp_dir().join("climb-tui-definitely-missing-capture.json");
    let _ = std::fs::remove_file(&path);
    let state = load_live(&path);
    assert!(matches!(state, LiveLoadState::Missing { .. }));
    assert_eq!(state.step_count(), 0);
    assert!(state.view().is_none());
}

#[test]
fn load_of_garbage_capture_is_invalid_and_never_panics() {
    let root = temp_root("invalid");
    let path = root.join("dns-capture.json");
    std::fs::write(&path, "{\"queries\": not json").expect("write garbage");
    let state = load_live(&path);
    match &state {
        LiveLoadState::Invalid { error, .. } => assert!(!error.is_empty()),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(state.step_count(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_live_auto_prefers_target_over_the_sample_fixture() {
    let root = temp_root("prefer-live");
    let live = root.join("target/dns-capture.json");
    let recorded = root.join("sample/dns-capture.recorded.json");
    std::fs::create_dir_all(live.parent().unwrap()).expect("live parent");
    std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
    std::fs::write(&live, sample_json()).expect("write live");
    std::fs::write(&recorded, sample_json()).expect("write recorded");

    let state = load_live_auto_from(&live, &recorded);
    assert_eq!(
        state.view().map(LiveView::source),
        Some(&LiveSource::LiveRun)
    );
    assert_eq!(state.step_count(), 6);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_live_auto_falls_back_to_the_sample_fixture() {
    let root = temp_root("fallback");
    let live = root.join("target/dns-capture.json");
    let recorded = root.join("sample/dns-capture.recorded.json");
    std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
    std::fs::write(&recorded, sample_json()).expect("write recorded");

    let state = load_live_auto_from(&live, &recorded);
    assert_eq!(
        state.view().map(LiveView::source),
        Some(&LiveSource::RecordedFixture)
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_live_auto_names_the_live_path_when_nothing_exists_and_no_embedded_copy_is_offered() {
    let root = temp_root("nowhere");
    let live = root.join("target/dns-capture.json");
    let state = load_live_auto_with(&live, &root.join("sample/dns-capture.recorded.json"), None);
    match state {
        LiveLoadState::Missing { path } => {
            assert_eq!(path, live, "the remedy is a new recording at the live path")
        }
        other => panic!("expected Missing naming the live path, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_live_auto_falls_back_to_the_embedded_capture() {
    let root = temp_root("embedded");
    let state = load_live_auto_from(
        &root.join("target/dns-capture.json"),
        &root.join("sample/dns-capture.recorded.json"),
    );
    assert_eq!(
        state.view().map(LiveView::source),
        Some(&LiveSource::Embedded)
    );
    assert!(state.step_count() > 0);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_live_auto_treats_a_present_but_invalid_live_capture_as_an_error() {
    let root = temp_root("invalid-live");
    let live = root.join("target/dns-capture.json");
    let recorded = root.join("sample/dns-capture.recorded.json");
    std::fs::create_dir_all(live.parent().unwrap()).expect("live parent");
    std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
    // The live capture is corrupt while the fixture is valid: the loader
    // must surface the live error, not silently fall back.
    std::fs::write(&live, "{ not json").expect("write garbage");
    std::fs::write(&recorded, sample_json()).expect("write recorded");

    let state = load_live_auto_from(&live, &recorded);
    match state {
        LiveLoadState::Invalid { path, .. } => assert_eq!(path, live),
        other => panic!("expected Invalid naming the live path, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// The committed sample capture must load through this loader and expose
/// the same step and lane numbers the artifact itself records (§7c: the
/// dashboard is one of the artifact's consumers).
#[test]
fn committed_sample_capture_exposes_steps_and_lanes_to_the_tui() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/sample-output/dns-capture.recorded.json");
    // No live capture at the first candidate, so the loader falls through
    // to the committed fixture and labels it as such.
    let absent =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-directory/dns-capture.json");
    let state = load_live_auto_from(&absent, &path);
    let LiveLoadState::Loaded(view) = state else {
        panic!("expected the committed sample capture to load, got {state:?}");
    };
    assert_eq!(view.source(), &LiveSource::RecordedFixture);

    // Read the expected headline pair straight from the parsed artifact.
    let find = |pass: &str| {
        view.capture()
            .queries
            .iter()
            .find(|q| {
                q.label == pass
                    && q.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
                    && q.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE)
            })
            .map(|q| q.response_bytes)
    };
    let full = find("full").expect("the sample carries a full pass");
    let condensed = find("condensed").expect("the sample carries a condensed pass");

    assert_eq!(view.steps().len(), view.capture().queries.len() * 2);
    let lanes = view.lanes();
    assert_eq!(lanes.without_mtl.total_bytes, full);
    assert_eq!(lanes.with_mtl.total_bytes, condensed);
    assert_eq!(view.recorded_date(), &view.capture().recorded_at[..10]);
}
