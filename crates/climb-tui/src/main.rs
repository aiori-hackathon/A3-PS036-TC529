//! `climb-tui` binary: logging, terminal lifecycle and the run loop.
//!
//! Logging goes to `target/climb-tui.log` via `tracing-appender`, never to stdout/stderr —
//! the TUI owns the screen while it runs.

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing_appender::non_blocking::WorkerGuard;

use climb_tui::app::{map_key, Action, App, Effect};
use climb_tui::cli::Cli;
use climb_tui::event::{Event, EventHandler};
use climb_tui::terminal::{install_panic_hook, TerminalGuard};
use climb_tui::{theme, ui};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let _log_guard = init_logging()?;

    let color_depth = theme::depth_from_env();
    let mut app = App::from_cli(&cli, color_depth);
    tracing::info!(
        results = %app.results_path.display(),
        theme = app.theme.label(),
        depth = color_depth.label(),
        "climb-tui starting"
    );
    let initial_path = app.results_path.clone();
    apply(&mut app, Effect::LoadResults(initial_path));

    run(&mut app)
}

fn run(app: &mut App) -> Result<()> {
    install_panic_hook();
    let mut guard = TerminalGuard::enter().context("initialising terminal")?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).context("creating terminal")?;
    let events = EventHandler::new(Duration::from_millis(1000 / 30));

    let result = event_loop(&mut terminal, &events, app);

    let _ = terminal.clear();
    drop(terminal);
    guard.restore();
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    events: &EventHandler,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        let action = match events.next() {
            Ok(Event::Key(key)) => map_key(key),
            Ok(Event::Tick) => Some(Action::Tick),
            Ok(Event::Resize(width, height)) => Some(Action::Resize { width, height }),
            Ok(Event::Worker(_)) => None,
            Err(_) => break,
        };

        if let Some(effect) = action.and_then(|action| app.update(action)) {
            apply(app, effect);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn apply(app: &mut App, effect: Effect) {
    match effect {
        Effect::Quit => {}
        Effect::LoadResults(path) => {
            let state = climb_tui::data::load(&path);
            tracing::info!(path = %path.display(), loaded = %state.summary(), "results loaded");
            app.set_data(state);

            let ladder = climb_tui::data::load_ladder(&app.ladder_path);
            tracing::info!(
                path = %app.ladder_path.display(),
                loaded = %ladder.summary(),
                "ladder verification loaded"
            );
            app.set_ladder(ladder);
        }
    }
}

fn init_logging() -> Result<WorkerGuard> {
    std::fs::create_dir_all("target").context("creating target directory for logs")?;
    let file = tracing_appender::rolling::never("target", "climb-tui.log");
    let (writer, guard) = tracing_appender::non_blocking(file);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("climb_tui=info,warn"));

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .ok();

    Ok(guard)
}
