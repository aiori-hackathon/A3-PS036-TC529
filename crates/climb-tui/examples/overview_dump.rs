//! Render the Overview onto an in-memory [`TestBackend`] and print the
//! character buffer to stdout (phase 19.7 evidence tool).
//!
//! Usage:
//! ```text
//! cargo run -p climb-tui --example overview_dump [RESULTS] [WIDTH] [HEIGHT]
//! ```
//! `RESULTS` defaults to `target/climb-results.json`, `WIDTH`/`HEIGHT` to
//! 120x40. The dump lets a reviewer verify the Overview layout on a
//! write-only terminal and becomes the dev notes evidence.

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use climb_tui::app::App;
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

    let cli = Cli::parse_from(["climb-tui", "--results", &results]);
    let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
    let state = data::load(Path::new(&results));
    app.set_data(state);

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::render(frame, &app))?;

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
    print!("{out}");
    Ok(())
}
