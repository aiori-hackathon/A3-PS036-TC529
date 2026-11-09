//! `climb-tui` binary: logging, terminal lifecycle and the run loop.
//!
//! Logging goes to `target/climb-tui.log` via `tracing-appender`, never to stdout/stderr —
//! the TUI owns the screen while it runs.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing_appender::non_blocking::WorkerGuard;

use climb_tui::app::{map_key, map_key_tour, Action, App, Effect};
use climb_tui::cli::Cli;
use climb_tui::event::{Event, EventHandler};
use climb_tui::terminal::{install_panic_hook, TerminalGuard};
use climb_tui::{theme, ui};

fn main() -> Result<()> {
    let cli = Cli::parse_with_provenance();
    if cli.check {
        return check(&cli);
    }
    let _log_guard = init_logging()?;

    let color_depth = theme::depth_from_env();
    let mut app = App::from_cli(&cli, color_depth);
    tracing::info!(
        results = %climb_tui::paths::display(&app.results_path),
        theme = app.theme.label(),
        depth = color_depth.label(),
        "climb-tui starting"
    );
    let initial_path = app.results_path.clone();
    apply(&mut app, Effect::LoadResults(initial_path));

    run(&mut app)
}

/// Validate the dashboard's input artifacts without opening the terminal.
///
/// Each problem is reported as one line naming the file and the fix, and any
/// problem produces a non-zero exit. This is the pipeline's non-interactive
/// way to prove the TUI would not render stale or absent data.
fn check(cli: &Cli) -> Result<()> {
    use climb_tui::data::{
        load_ladder, load_ladder_auto, load_live, load_live_auto, load_results_auto,
        LadderLoadState, LiveLoadState, LoadState,
    };

    let mut problems: Vec<String> = Vec::new();

    let explicit_results = cli.results_explicit.then_some(cli.results.as_path());
    let (source, state) = load_results_auto(explicit_results);
    match state {
        LoadState::Loaded(_) => {
            let where_from = source
                .path()
                .map_or_else(|| "<embedded>".to_string(), |path| path.display().to_string());
            println!("results: ok ({where_from})");
        }
        LoadState::Missing { path } => problems.push(format!(
            "results: {} does not exist; run `cargo xtask reproduce --quick --skip-dns` or pass --results PATH",
            path.display()
        )),
        LoadState::Invalid { path, error } => problems.push(format!(
            "results: {} is unreadable or invalid ({error}); regenerate it with `cargo xtask reproduce --quick --skip-dns`",
            path.display()
        )),
        LoadState::NotLoaded => problems.push(
            "results: no load was attempted; pass --results PATH or run `cargo xtask reproduce --quick --skip-dns`"
                .to_string(),
        ),
    }

    let ladder = match cli.ladder.as_deref() {
        Some(path) => load_ladder(path),
        None => load_ladder_auto(),
    };
    match ladder {
        LadderLoadState::Loaded { .. } => println!("ladder: ok"),
        LadderLoadState::Absent => println!("ladder: absent (optional)"),
        LadderLoadState::Missing { path } => problems.push(format!(
            "ladder: {} does not exist; run `cargo xtask ladder-verify` or remove --ladder",
            path.display()
        )),
        LadderLoadState::Invalid { path, error } => problems.push(format!(
            "ladder: {} is unreadable or invalid ({error}); regenerate it with `cargo xtask ladder-verify`",
            path.display()
        )),
    }

    let live = match cli.live.as_deref() {
        Some(path) => load_live(path),
        None => load_live_auto(),
    };
    let live_steps = live.step_count();
    match live {
        LiveLoadState::Loaded(_) => println!("live: ok ({live_steps} steps)"),
        LiveLoadState::Missing { path } => problems.push(format!(
            "live: {} does not exist; run `cargo xtask dns-demo --record` or remove --live",
            path.display()
        )),
        LiveLoadState::Invalid { path, error } => problems.push(format!(
            "live: {} is unreadable or invalid ({error}); re-record it with `cargo xtask dns-demo --record`",
            path.display()
        )),
    }

    if problems.is_empty() {
        println!("climb-tui --check: ok");
        Ok(())
    } else {
        bail!("{}", problems.join("; "))
    }
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
            Ok(Event::Key(key)) => {
                if app.tour_active() {
                    map_key_tour(key)
                } else {
                    map_key(key)
                }
            }
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
        Effect::LoadResults(requested) => {
            // Resolve the artifact without requiring a checkout: an explicitly
            // named path, then target/, then the committed sample, then the
            // copy compiled into this binary. A present-but-unreadable file is
            // reported, never silently skipped.
            let explicit = app.results_explicit.then_some(requested.as_path());
            let (source, state) = climb_tui::data::load_results_auto(explicit);
            tracing::info!(
                source = %source.label(),
                loaded = %state.summary(),
                "results loaded"
            );
            app.set_data(state);
            app.set_source(source);

            let ladder = climb_tui::data::load_ladder_auto();
            tracing::info!(
                loaded = %ladder.summary(),
                "ladder verification loaded"
            );
            app.set_ladder(ladder);

            let live = climb_tui::data::load_live_auto();
            tracing::info!(
                loaded = %live.summary(),
                "recorded DNS capture loaded"
            );
            app.set_live(live);
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
