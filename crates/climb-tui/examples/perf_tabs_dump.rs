//! Render the Signing, Verifying, Wire, Memory and Ladder tabs onto an
//! in-memory [`TestBackend`] and print the character buffers to stdout (the
//! evidence tool for the data-view tabs).
//!
//! Usage:
//! ```text
//! cargo run -p climb-tui --example perf_tabs_dump [RESULTS] [WIDTH] [HEIGHT] [N]
//! ```
//! `RESULTS` defaults to `target/climb-results.json`, `WIDTH`/`HEIGHT` to
//! 120x40, and `N` (the Ladder tab's leaf count) to 8. The dump lets a reviewer
//! verify the tab layouts on a write-only terminal and becomes the dev notes
//! evidence.

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use climb_tui::app::{Action, App, Tab};
use climb_tui::cli::Cli;
use climb_tui::data;
use climb_tui::theme::ColorDepth;
use climb_tui::ui;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let results = args
        .next()
        .unwrap_or_else(|| "target/climb-results.json".to_string());
    let width: u16 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(120);
    let height: u16 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(40);
    let ladder_n: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(8);

    let cli = Cli::parse_from(["climb-tui", "--results", &results]);
    let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
    app.set_data(data::load(Path::new(&results)));
    let ladder = data::load_ladder(&app.ladder_path);
    app.set_ladder(ladder);
    app.ladder_n = ladder_n;

    for tab in [
        Tab::Signing,
        Tab::Verifying,
        Tab::Wire,
        Tab::Memory,
        Tab::Ladder,
    ] {
        app.update(Action::SelectTab(tab.index()));
        println!("=== {} ({width}x{height}) ===", tab.title());
        print!("{}", dump(&mut app, width, height)?);
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
