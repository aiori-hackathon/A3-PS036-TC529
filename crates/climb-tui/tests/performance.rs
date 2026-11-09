//! Frame-time budget: every tab must render within 8 ms at 120x40.
//!
//! This measures the real `ui::render` path through a `TestBackend`, not a
//! synthetic widget. It is a regression guard, not a benchmark. The 8 ms
//! budget is a release-build property: an unoptimised test build is several
//! times slower, so it gets a scaled ceiling that still catches a view which
//! starts doing per-frame I/O or quadratic work. Run with `--release` to check
//! the real budget.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;
use climb_tui::app::{App, Tab};
use climb_tui::cli::Cli;
use climb_tui::theme::ColorDepth;
use climb_tui::{runtime, ui, Clock};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;
const WARMUP: usize = 8;
const SAMPLES: usize = 240;
/// The shipped, optimised budget.
const RELEASE_BUDGET: Duration = Duration::from_millis(8);
/// A generous ceiling for unoptimised test builds; still catches pathological
/// per-frame work.
const DEBUG_BUDGET: Duration = Duration::from_millis(40);

fn sample_results() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
}

fn sample_app() -> App {
    let path = sample_results();
    let path_text = path.to_str().expect("the sample path is UTF-8");
    let cli = Cli::parse_from(["climb-tui", "--results", path_text]);
    let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
    app.set_clock(Clock::fake());
    app.apply_reload(runtime::load_all(Some(Path::new(path_text))));
    app
}

#[test]
fn every_tab_renders_within_the_frame_budget_at_120x40() {
    let mut app = sample_app();
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("in-memory terminal");

    for tab in Tab::ALL {
        app.tab = tab;
        for _ in 0..WARMUP {
            terminal
                .draw(|frame| ui::render(frame, &app))
                .expect("a warm-up frame renders");
        }
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            terminal
                .draw(|frame| ui::render(frame, &app))
                .expect("a measured frame renders");
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        let index = (SAMPLES * 99 / 100).min(SAMPLES - 1);
        let p99 = samples[index];
        let median = samples[SAMPLES / 2];
        let budget = if cfg!(debug_assertions) {
            DEBUG_BUDGET
        } else {
            RELEASE_BUDGET
        };
        eprintln!("{}: p99 {p99:?}, median {median:?}", tab.title());
        assert!(
            p99 < budget,
            "{} p99 {p99:?} exceeds the {budget:?} budget (median {median:?})",
            tab.title()
        );
    }
}
