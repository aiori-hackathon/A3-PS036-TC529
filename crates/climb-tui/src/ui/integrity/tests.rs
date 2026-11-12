use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::*;
use crate::app::Action;
use crate::cli::Cli;
use crate::data;
use crate::theme::{ColorDepth, ThemeId};
use clap::Parser;

/// Copy the committed sample results and report into a temp directory
/// laid out like a real run and load it through the TUI's own loader.
fn loaded_app(tag: &str, drift: bool) -> (crate::app::App, PathBuf) {
    let root = temp_root(tag);
    std::fs::create_dir_all(root.join("climb-report")).expect("temp dir");
    let results_source = crate::data::fixtures::fixture("climb-results.json");
    if drift {
        let mut results: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&results_source).expect("read sample results"),
        )
        .expect("parse sample results");
        results["derived"]["overall_wire_reduction_percent"] = serde_json::json!(90.0);
        std::fs::write(
            root.join("climb-results.json"),
            serde_json::to_vec(&results).expect("re-serialize"),
        )
        .expect("write drifted results");
    } else {
        crate::data::fixtures::copy_fixture("climb-results.json", &root.join("climb-results.json"));
    }
    crate::data::fixtures::copy_fixture("report.html", &root.join("climb-report/report.html"));

    let cli = Cli::parse_from(["climb-tui"]);
    let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
    app.results_path = root.join("climb-results.json");
    app.set_data(data::load(&app.results_path));
    app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
    (app, root)
}

fn temp_root(tag: &str) -> PathBuf {
    let root = crate::data::fixtures::temp_root("climb-tui-integrity-ui", tag);
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn render_to_string(app: &crate::app::App, width: u16, height: u16) -> String {
    let theme = app.theme();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("in-memory terminal");
    terminal
        .draw(|frame| {
            super::render_integrity(
                frame,
                frame.area(),
                app,
                match &app.data {
                    crate::data::LoadState::Loaded(view) => view,
                    _ => panic!("fixture must be loaded"),
                },
                &theme,
                &theme.glyphs(),
            )
        })
        .expect("rendering never fails on TestBackend");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
            }
        }
        out.push('\n');
    }
    out
}

#[test]
fn agreeing_sample_pair_renders_the_full_tab_at_120x40() {
    let (app, root) = loaded_app("agree", false);
    let out = render_to_string(&app, 120, 40);
    for needle in [
        "ALL CLAIMS AGREE",
        "overall-wire-reduction",
        "Artifacts",
        "wire-report.json",
        "present",
        "Provenance",
        "commit",
        "rustc",
        "host hash",
        "MEASURED",
        "EXTRAPOLATED",
        "ANALYTICAL",
        "DERIVED",
        "refresh:",
        "cargo xtask reproduce --report-only --quick --skip-dns",
    ] {
        assert!(
            out.contains(needle),
            "integrity tab misses {needle:?} at 120x40:\n{out}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Phase evidence: one value changed in a temp copy of the artifact
/// pair must surface as exactly one drifted claim, named in the banner.
#[test]
fn drifted_results_banner_names_the_claim_id() {
    let (app, root) = loaded_app("drift", true);
    let out = render_to_string(&app, 120, 40);
    assert!(
        out.contains("1 DRIFT"),
        "banner must count the single drift:\n{out}"
    );
    assert!(
        out.contains("overall-wire-reduction"),
        "banner must name the drifted claim:\n{out}"
    );
    assert!(!out.contains("ALL CLAIMS AGREE"), "banner:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn narrow_layout_lists_every_artifact_and_the_legend_at_80x24() {
    let (app, root) = loaded_app("narrow", false);
    let out = render_to_string(&app, 80, 24);
    for needle in [
        "ALL CLAIMS AGREE",
        "climb-results.json",
        "report.html",
        "charts",
        "wire-report.json",
        "memory-report.json",
        "dns-capture.json",
        "ladder-verification.json",
        "climb-classical-signatures.json",
        "environment.json",
        "missing",
        "submodules",
        "host",
        "MEASURED",
        "DERIVED",
        "refresh:",
        "cargo xtask reproduce --report-only --quick --skip-dns",
    ] {
        assert!(
            out.contains(needle),
            "integrity tab misses {needle:?} at 80x24:\n{out}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn scrolling_windows_the_claim_table() {
    let (mut app, root) = loaded_app("scroll", false);
    let first = render_to_string(&app, 120, 40);
    for _ in 0..10 {
        app.update(Action::LadderDown);
    }
    let scrolled = render_to_string(&app, 120, 40);
    assert_ne!(first, scrolled, "scrolling must change the visible rows");
    assert!(
        scrolled.contains("rows 11-"),
        "scroll indicator:\n{scrolled}"
    );
    // Scrolling far past the end clamps to the last window instead of
    // blanking the table.
    for _ in 0..10_000 {
        app.update(Action::LadderDown);
    }
    let bottom = render_to_string(&app, 120, 40);
    assert!(
        bottom.contains("of"),
        "the table still shows a window at the bottom:\n{bottom}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ascii_mode_never_emits_non_ascii_glyphs() {
    let (mut app, root) = loaded_app("ascii", false);
    app.ascii = true;
    let out = render_to_string(&app, 120, 40);
    let offending: Vec<char> = out
        .chars()
        .filter(|c| !c.is_ascii() && !c.is_whitespace())
        .collect();
    assert!(
        offending.is_empty(),
        "ascii mode rendered non-ASCII glyphs: {offending:?}\n{out}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_results_file_without_a_report_says_so_and_lists_missing_claims() {
    let root = temp_root("absent");
    std::fs::create_dir_all(&root).expect("temp dir");
    crate::data::fixtures::copy_fixture("climb-results.json", &root.join("climb-results.json"));
    let cli = Cli::parse_from(["climb-tui"]);
    let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
    app.results_path = root.join("climb-results.json");
    app.set_data(data::load(&app.results_path));
    app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
    let out = render_to_string(&app, 120, 40);
    assert!(out.contains("REPORT NOT FOUND"), "banner:\n{out}");
    assert!(out.contains("missing"), "claim rows:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_view_built_without_a_check_renders_the_not_run_banner() {
    let view = crate::data::test_support::view_with(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        crate::data::test_support::empty_derived(),
    );
    let cli = Cli::parse_from(["climb-tui"]);
    let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
    app.set_data(crate::data::LoadState::Loaded(Box::new(view)));
    app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
    let theme = app.theme();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("in-memory terminal");
    terminal
        .draw(|frame| {
            super::render_integrity(
                frame,
                frame.area(),
                &app,
                match &app.data {
                    crate::data::LoadState::Loaded(view) => view,
                    _ => unreachable!(),
                },
                &theme,
                &theme.glyphs(),
            )
        })
        .expect("render");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
            }
        }
        out.push('\n');
    }
    assert!(out.contains("CHECK NOT RUN"), "banner:\n{out}");
}

#[test]
fn claim_values_render_verbatim_from_the_artifact_json() {
    use serde_json::json;
    assert_eq!(value_text(&json!(97.3), false), "97.3");
    assert_eq!(value_text(&json!(24438), false), "24438");
    assert_eq!(value_text(&json!(97.0), false), "97");
    assert_eq!(value_text(&json!(null), false), "\u{2014}");
    assert_eq!(value_text(&json!(null), true), "--");
    assert_eq!(value_text(&json!("Medium"), false), "Medium");
    assert_eq!(value_text(&json!(true), false), "true");
}

#[test]
fn every_theme_renders_the_tab() {
    // Smoke test: every theme renders the tab without panicking.
    let (mut app, root) = loaded_app("themes", false);
    for theme in [ThemeId::Dark, ThemeId::Light, ThemeId::Contrast] {
        app.theme = theme;
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("ALL CLAIMS AGREE"), "theme {theme:?}:\n{out}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
