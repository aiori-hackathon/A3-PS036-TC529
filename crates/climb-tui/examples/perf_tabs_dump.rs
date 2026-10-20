//! Render the Signing, Verifying, Wire, Memory, Ladder, Live, Integrity and
//! Method tabs onto an in-memory [`TestBackend`] and print the character buffers
//! to stdout (the evidence tool for the data-view tabs).
//!
//! Usage:
//! ```text
//! cargo run -p climb-tui --example perf_tabs_dump [RESULTS] [WIDTH] [HEIGHT] [N] [DEPTH] [STEP]
//! ```
//! `RESULTS` defaults to `target/climb-results.json`, `WIDTH`/`HEIGHT` to
//! 120x40, `N` (the Ladder tab's leaf count) to 8, `DEPTH` to `truecolor`
//! (also `mono`, `ansi256`, `ansi16`), and `STEP` (the Live tab's initial
//! replay cursor) to 0. The dump lets a reviewer verify the tab layouts on a
//! write-only terminal and becomes the evidence pasted into the run report.
//!
//! Tour frames — the same frame a screen recording shows — come from `--tour`:
//!
//! ```text
//! cargo run -p climb-tui --example perf_tabs_dump -- --tour [RESULTS] [WIDTH] [HEIGHT] [DEPTH]
//! ```
//! `--tour` prints the first, middle and last step of the default script, each
//! at the top of its step, from a fake clock. `--tour-frame N` prints one frame
//! (0-based) instead:
//!
//! ```text
//! cargo run -p climb-tui --example perf_tabs_dump -- --tour-frame 2
//! cargo run -p climb-tui --example perf_tabs_dump -- --tour-frame 4 160 50
//! ```

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use climb_tui::app::{Action, App, Tab};
use climb_tui::cli::Cli;
use climb_tui::data;
use climb_tui::theme::ColorDepth;
use climb_tui::tour::{Clock, TourRunner, SCRIPT};
use climb_tui::ui;

#[derive(Debug, Parser)]
#[command(
    name = "perf_tabs_dump",
    about = "Dump the dashboard's tabs, or guided-tour frames, to stdout"
)]
struct Args {
    /// Print the first, middle and last frame of the guided tour.
    #[arg(long)]
    tour: bool,

    /// Print one guided-tour frame by index (0-based).
    #[arg(long, value_name = "N", conflicts_with = "tour")]
    tour_frame: Option<usize>,

    /// Path to the consolidated results JSON.
    results: Option<String>,

    /// Terminal width in columns.
    #[arg(default_value_t = 120)]
    width: u16,

    /// Terminal height in rows.
    #[arg(default_value_t = 40)]
    height: u16,

    /// Colour depth: `truecolor`, `ansi256`, `ansi16` or `mono`.
    #[arg(default_value = "truecolor")]
    depth: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let results = args
        .results
        .clone()
        .unwrap_or_else(|| "target/climb-results.json".to_string());
    let depth = match args.depth.to_ascii_lowercase().as_str() {
        "mono" => ColorDepth::Mono,
        "ansi16" => ColorDepth::Ansi16,
        "ansi256" => ColorDepth::Ansi256,
        _ => ColorDepth::TrueColor,
    };

    let cli = Cli::parse_from(["climb-tui", "--results", &results]);
    let mut app = App::from_cli(&cli, depth);
    app.set_data(data::load(Path::new(&results)));
    app.set_ladder(data::load_ladder_auto());
    app.set_live(data::load_live_auto());

    if args.tour || args.tour_frame.is_some() {
        return tour_dump(&app, &args);
    }

    // Tab dump: `RESULTS WIDTH HEIGHT N DEPTH STEP` positionally.
    let positional: Vec<String> = std::env::args().skip(1).collect();
    let ladder_n: u64 = positional
        .get(3)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(8);
    let live_step: usize = positional
        .get(5)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(0);
    app.ladder_n = ladder_n;
    app.live_step = live_step.min(app.live().step_count().saturating_sub(1));

    for tab in [
        Tab::Signing,
        Tab::Verifying,
        Tab::Wire,
        Tab::Memory,
        Tab::Ladder,
        Tab::Live,
        Tab::Integrity,
        Tab::Method,
    ] {
        app.update(Action::SelectTab(tab.index()));
        println!(
            "=== {} ({w}x{h}) ===",
            tab.title(),
            w = args.width,
            h = args.height
        );
        print!("{}", dump(&mut app, args.width, args.height)?);
    }
    Ok(())
}

/// The frame indices `--tour` prints when `--tour-frame` is not given.
fn default_frames(total: usize) -> Vec<usize> {
    if total == 0 {
        Vec::new()
    } else {
        vec![0, total / 2, total - 1]
    }
}

/// Print guided-tour frames, each at the top of its step (elapsed zero), by
/// driving a runner from a fake clock: the clock advances by each scripted
/// dwell, and only the selected frames are rendered.
fn tour_dump(app: &App, args: &Args) -> Result<()> {
    let total = TourRunner::new(1.0).total();
    let frames: Vec<usize> = match args.tour_frame {
        Some(index) => vec![index.min(total.saturating_sub(1))],
        None => default_frames(total),
    };

    let cli = Cli::parse_from(["climb-tui", "--tour"]);

    for &index in &frames {
        // Each frame is dumped from its own runner so the requested step is
        // reached one dwell at a time — exactly the clock sequence a live run
        // produces — rather than by one jump that would carry the runner past
        // the end of an earlier step.
        let mut tour_app = App::from_cli(&cli, app.color_depth);
        tour_app.set_data(app.data.clone());
        tour_app.set_ladder(app.ladder().clone());
        tour_app.set_live(data::load_live_auto());
        let mut clock = Clock::fake();
        tour_app.set_clock(clock);
        tour_app.set_tour(Some(TourRunner::new(1.0)));
        tour_app.update(Action::Tick); // the tour opens on step 0

        // Walk the fake clock to the start of the requested step, one ticked
        // dwell per step, then tick once more so the step shows a little of
        // its own elapsed time instead of a first-frame stub.
        for _ in 0..index {
            let dwell = tour_app
                .tour()
                .map_or(Duration::ZERO, TourRunner::step_dwell);
            clock.advance(dwell);
            tour_app.set_clock(clock);
            tour_app.update(Action::Tick);
        }
        clock.advance(Duration::from_secs(1));
        tour_app.set_clock(clock);
        tour_app.update(Action::Tick);

        let step = SCRIPT[index];
        println!(
            "=== tour frame {index}/{total} · {} · {}s · script clock {}s @ {}x{} ===",
            step.tab.title(),
            step.dwell.as_secs(),
            clock.now().as_secs(),
            args.width,
            args.height,
        );
        println!("caption: {}", step.caption);
        print!("{}", dump(&mut tour_app, args.width, args.height)?);
    }
    Ok(())
}

fn dump(app: &mut App, width: u16, height: u16) -> Result<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::render(frame, app))?;

    let buffer = terminal.backend().buffer();
    let mut out = String::with_capacity(buffer.area.width as usize * buffer.area.height as usize);
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
            }
        }
        out.push('\n');
    }
    Ok(out)
}
