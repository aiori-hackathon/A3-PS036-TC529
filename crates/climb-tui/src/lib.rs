//! `climb-tui` — the CLIMB live-run benchmark dashboard.
//!
//! This is the Batch 6 demo layer (the design notes §5 Layer 5, §10). Phase 18 establishes
//! the architecture every later TUI phase builds on: terminal safety, an Elm-style
//! model/update split, a threaded event pump, theming with colour-depth mapping, and a
//! widget-tree skeleton. No result data is rendered yet — every tab shows a placeholder
//! until the data-wiring phases land.
//!
//! Project rules carried in from the project conventions:
//! - No `println!`/`eprintln!` in the TUI path. Logging goes through `tracing` to
//!   `target/climb-tui.log` (see `src/main.rs`), never to the terminal while the TUI
//!   owns the screen.
//! - [`app::App::update`] is pure with respect to I/O: it mutates the model and returns an
//!   [`app::Effect`] for the runtime to execute. That is what keeps the model testable
//!   without a terminal.

pub mod app;
pub mod cli;
pub mod event;
pub mod terminal;
pub mod theme;
pub mod ui;

pub use app::{map_key, Action, App, Effect, Tab};
pub use cli::Cli;
pub use event::{Event, EventHandler, WorkerMsg};
pub use terminal::{install_panic_hook, TerminalGuard};
pub use theme::{ColorDepth, Glyphs, Theme, ThemeId};
