//! `climb-tui` binary: logging, terminal lifecycle and the run loop.
//!
//! Logging goes to `target/climb-tui.log` via `tracing-appender`, never to stdout/stderr —
//! the TUI owns the screen while it runs.

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::Terminal;
use tracing_appender::non_blocking::WorkerGuard;

use climb_tui::app::{map_key, map_key_tour, Action, App, Effect, Tab};
use climb_tui::cli::Cli;
use climb_tui::event::{Event, EventHandler};
use climb_tui::terminal::{install_panic_hook, install_signal_handlers, PanicExit, TerminalGuard};
use climb_tui::{runtime, theme, ui};

fn main() -> ExitCode {
    let cli = Cli::parse_with_provenance();
    if cli.check {
        return match check(&cli) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("climb-tui: {error:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(args) = cli.dump_frame.as_deref() {
        return match dump_frame(&cli, args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("climb-tui: {error:#}");
                ExitCode::FAILURE
            }
        };
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!(
            "climb-tui: this dashboard needs an interactive terminal (stdin and stdout must be a TTY).\n\
             Run it from a terminal, or use --dump-frame <tab> <width>x<height> for a text frame."
        );
        return ExitCode::from(2);
    }
    if !cfg!(debug_assertions) && (cli.debug_panic || cli.debug_hold_ms > 0) {
        eprintln!("climb-tui: --debug-panic and --debug-hold-ms are debug-build test hooks only");
        return ExitCode::FAILURE;
    }

    match run_main(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("climb-tui: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_main(cli: &Cli) -> Result<()> {
    let _log_guard = init_logging()?;

    let color_depth = theme::depth_from_env();
    let mut app = App::from_cli(cli, color_depth);
    tracing::info!(
        results = %climb_tui::paths::display(&app.results_path),
        theme = app.theme.label(),
        depth = color_depth.label(),
        "climb-tui starting"
    );
    let initial_path = app.results_path.clone();
    let explicit = app.results_explicit.then_some(initial_path);
    app.apply_reload(runtime::load_all(explicit.as_deref()));

    run(&mut app, cli.debug_panic, cli.debug_hold_ms)
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

/// Render one frame to plain text. Used by documentation and by tests that do
/// not have a terminal, so it deliberately ignores TTY state.
fn dump_frame(cli: &Cli, args: &[String]) -> Result<()> {
    let tab = parse_tab(args.first().context("--dump-frame needs a tab name")?)?;
    let (width, height) = parse_size(args.get(1).context("--dump-frame needs WIDTHxHEIGHT")?)?;

    let mut app = App::from_cli(cli, theme::depth_from_env());
    let explicit = app.results_explicit.then_some(app.results_path.clone());
    app.apply_reload(runtime::load_all(explicit.as_deref()));
    // A dump is a documentation frame, not a status report.
    app.set_status(None);
    app.update(Action::SelectTab(tab.index()));

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).context("creating the in-memory terminal")?;
    terminal
        .draw(|frame| ui::render(frame, &app))
        .context("rendering the frame")?;

    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    print!("{out}");
    Ok(())
}

fn parse_tab(raw: &str) -> Result<Tab> {
    Tab::ALL
        .iter()
        .copied()
        .find(|tab| {
            tab.title().eq_ignore_ascii_case(raw)
                || raw.parse::<usize>().ok() == Some(tab.index() + 1)
        })
        .with_context(|| format!("unknown tab {raw:?}; use a title such as `overview`"))
}

fn parse_size(raw: &str) -> Result<(u16, u16)> {
    let (width, height) = raw
        .split_once('x')
        .with_context(|| format!("{raw:?} is not WIDTHxHEIGHT"))?;
    let width: u16 = width.parse().context("width is not a number")?;
    let height: u16 = height.parse().context("height is not a number")?;
    if width == 0 || height == 0 {
        bail!("{raw:?} must have a non-zero width and height");
    }
    if width > 500 || height > 200 {
        bail!("{raw:?} is larger than the 500x200 dump limit");
    }
    Ok((width, height))
}

fn run(app: &mut App, debug_panic: bool, debug_hold_ms: u64) -> Result<()> {
    install_panic_hook(PanicExit::Exit);
    install_signal_handlers().context("installing signal handlers")?;
    let mut guard = TerminalGuard::enter().context("initialising terminal")?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).context("creating terminal")?;
    let events = EventHandler::new(Duration::from_millis(1000 / 30));

    if debug_panic {
        panic!("forced panic for terminal-restoration testing");
    }
    if debug_hold_ms > 0 {
        std::thread::sleep(Duration::from_millis(debug_hold_ms));
    }

    let result = event_loop(&mut terminal, &events, app);

    // Stop and join the input and tick threads before touching the terminal
    // mode. A live `poll` can block once raw mode is disabled, so restoring
    // first would risk hanging in the join below.
    drop(events);

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
        // Draw only when the model changed. An idle dashboard then costs one
        // wake-up per tick and no terminal writes at all.
        if app.needs_redraw() {
            terminal.draw(|frame| ui::render(frame, app))?;
            app.mark_clean();
        }

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
            Ok(Event::Reloaded(loaded)) => {
                app.apply_reload(*loaded);
                None
            }
            Err(_) => break,
        };

        if let Some(effect) = action.and_then(|action| app.update(action)) {
            apply(app, effect, events);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn apply(app: &mut App, effect: Effect, events: &EventHandler) {
    match effect {
        Effect::Quit => {}
        Effect::LoadResults(requested) => {
            // The reload runs on a worker thread; the dashboard keeps drawing
            // and shows the reloading state until Event::Reloaded arrives.
            let explicit = app.results_explicit.then_some(requested);
            runtime::spawn_reload(explicit, events.sender());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_names_parse_case_insensitively_and_by_number() {
        assert_eq!(parse_tab("overview").expect("title"), Tab::Overview);
        assert_eq!(parse_tab("METHOD").expect("title"), Tab::Method);
        assert_eq!(parse_tab("9").expect("number"), Tab::Method);
        assert!(parse_tab("nope").is_err());
        assert!(parse_tab("0").is_err());
    }

    #[test]
    fn sizes_parse_and_reject_bad_shapes() {
        assert_eq!(parse_size("80x24").expect("size"), (80, 24));
        assert_eq!(parse_size("200x60").expect("size"), (200, 60));
        for bad in ["80", "80X24", "0x24", "80x0", "501x24", "80x201", "axb"] {
            assert!(parse_size(bad).is_err(), "{bad:?} must be rejected");
        }
    }
}
