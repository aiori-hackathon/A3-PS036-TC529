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
/// Attempts per tab. A shared runner can deschedule one frame past the
/// ceiling; a render regression fails every attempt, a scheduler outlier does
/// not.
const ATTEMPTS: usize = 3;
/// The shipped, optimised budget.
const RELEASE_BUDGET: Duration = Duration::from_millis(8);
/// A generous median ceiling for unoptimised test builds; still catches a view
/// that starts doing per-frame I/O or quadratic work.
const DEBUG_BUDGET: Duration = Duration::from_millis(40);
/// A single frame in a shared, unoptimised test build can be descheduled far
/// past the median. The debug run keeps a catastrophic-work ceiling on the
/// tail instead of treating one scheduler outlier as a render regression; the
/// release budget still checks the real p99.
const DEBUG_OUTLIER_BUDGET: Duration = Duration::from_millis(200);

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
    let budget = if cfg!(debug_assertions) {
        DEBUG_BUDGET
    } else {
        RELEASE_BUDGET
    };
    let outlier_budget = if cfg!(debug_assertions) {
        DEBUG_OUTLIER_BUDGET
    } else {
        RELEASE_BUDGET
    };

    for tab in Tab::ALL {
        app.tab = tab;
        let (passed, p99, median) = passes_within_attempts(budget, outlier_budget, || {
            let measurements = measure(&mut terminal, &app);
            eprintln!(
                "{}: p99 {:?}, median {:?}",
                tab.title(),
                measurements.0,
                measurements.1
            );
            measurements
        });
        assert!(
            passed,
            "{} p99 {p99:?} / median {median:?} exceeds the {budget:?} median budget or the \
             {outlier_budget:?} tail ceiling in all {ATTEMPTS} attempts",
            tab.title()
        );
    }
}

/// Run up to [`ATTEMPTS`] measurements and pass as soon as one is inside both
/// the median budget and the tail ceiling. Returns whether one passed and the
/// last `(p99, median)` measured.
fn passes_within_attempts<F>(
    budget: Duration,
    outlier_budget: Duration,
    mut sample: F,
) -> (bool, Duration, Duration)
where
    F: FnMut() -> (Duration, Duration),
{
    let mut last = (Duration::ZERO, Duration::ZERO);
    for _ in 0..ATTEMPTS {
        last = sample();
        if last.1 < budget && last.0 < outlier_budget {
            return (true, last.0, last.1);
        }
    }
    (false, last.0, last.1)
}

/// Measure one tab: warm-up frames, then the p99 and median wall time.
fn measure(terminal: &mut Terminal<TestBackend>, app: &App) -> (Duration, Duration) {
    for _ in 0..WARMUP {
        terminal
            .draw(|frame| ui::render(frame, app))
            .expect("a warm-up frame renders");
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        terminal
            .draw(|frame| ui::render(frame, app))
            .expect("a measured frame renders");
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let index = (SAMPLES * 99 / 100).min(SAMPLES - 1);
    (samples[index], samples[SAMPLES / 2])
}

/// A single scheduler outlier is retried; a tab whose median is over budget,
/// or whose tail shows catastrophic work, still fails on every attempt.
#[test]
fn a_scheduler_outlier_is_retried_and_a_slow_tab_is_not_excused() {
    let budget = Duration::from_millis(40);
    let outlier_budget = Duration::from_millis(200);

    let mut attempts = [(50_u64, 5_u64), (60, 6)].into_iter();
    let (passed, _, median) = passes_within_attempts(budget, outlier_budget, || {
        let (p99, median) = attempts.next().expect("two attempts");
        (Duration::from_millis(p99), Duration::from_millis(median))
    });
    assert!(passed, "a noisy tail with a healthy median must pass");
    assert_eq!(median, Duration::from_millis(5));

    let (passed, _, _) = passes_within_attempts(budget, outlier_budget, || {
        (Duration::from_millis(50), Duration::from_millis(50))
    });
    assert!(!passed, "a median over budget on every attempt must fail");

    let (passed, _, _) = passes_within_attempts(budget, outlier_budget, || {
        (Duration::from_millis(250), Duration::from_millis(5))
    });
    assert!(
        !passed,
        "a catastrophic tail is not excused by a healthy median"
    );
}
