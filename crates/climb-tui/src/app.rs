//! The application model (phase 18.5), Elm style.
//!
//! [`App`] is the model, [`Action`] the messages, and [`App::update`] the pure transition:
//! it mutates the model and *returns* an optional [`Effect`] describing side work. It never
//! touches the terminal, files or the clock, so the whole model is unit-testable. All key
//! binding lives in one place, [`map_key`].

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::cli::Cli;
use crate::data::{LoadState, ValueMode};
use crate::theme::{ColorDepth, Theme, ThemeId};

/// The dashboard's tabs, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Signing,
    Verifying,
    Wire,
    Memory,
    Ladder,
    Live,
    Integrity,
    Method,
}

impl Tab {
    /// Every tab, in display order. Key `1`..=`9` selects `ALL[0]`..=`ALL[8]`.
    pub const ALL: [Tab; 9] = [
        Tab::Overview,
        Tab::Signing,
        Tab::Verifying,
        Tab::Wire,
        Tab::Memory,
        Tab::Ladder,
        Tab::Live,
        Tab::Integrity,
        Tab::Method,
    ];

    /// The tab's display title.
    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Signing => "Signing",
            Tab::Verifying => "Verifying",
            Tab::Wire => "Wire",
            Tab::Memory => "Memory",
            Tab::Ladder => "Ladder",
            Tab::Live => "Live",
            Tab::Integrity => "Integrity",
            Tab::Method => "Method",
        }
    }

    /// The tab's index in [`Tab::ALL`].
    pub fn index(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Signing => 1,
            Tab::Verifying => 2,
            Tab::Wire => 3,
            Tab::Memory => 4,
            Tab::Ladder => 5,
            Tab::Live => 6,
            Tab::Integrity => 7,
            Tab::Method => 8,
        }
    }

    /// The tab at an index, or `None` if out of range.
    pub fn from_index(index: usize) -> Option<Tab> {
        Tab::ALL.get(index).copied()
    }

    /// The next tab, wrapping at the end.
    pub fn next(self) -> Tab {
        Tab::ALL[(self.index() + 1) % Tab::ALL.len()]
    }

    /// The previous tab, wrapping at the start.
    pub fn prev(self) -> Tab {
        Tab::ALL[(self.index() + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }
}

/// A message that can be applied to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextTab,
    PrevTab,
    /// Select a tab by zero-based index.
    SelectTab(usize),
    ToggleHelp,
    CycleTheme,
    ToggleAscii,
    Reload,
    PrevSize,
    NextSize,
    ToggleValueMode,
    Tick,
    Resize {
        width: u16,
        height: u16,
    },
}

/// Side work returned by [`App::update`] for the runtime to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Quit,
    /// (Re)load consolidated results and store the outcome on [`App::data`].
    LoadResults(PathBuf),
}

/// The dashboard model.
pub struct App {
    pub tab: Tab,
    pub theme: ThemeId,
    pub ascii: bool,
    pub show_help: bool,
    pub should_quit: bool,
    pub results_path: PathBuf,
    pub replay: bool,
    pub tour: bool,
    pub color_depth: ColorDepth,
    pub tick: usize,
    pub last_size: (u16, u16),
    /// Consolidated results state; updated by the runtime on load/reload.
    pub data: LoadState,
    /// Selected dataset size on the Signing/Verifying tabs (0-based).
    pub selected_size: usize,
    /// Whether Signing/Verifying show batch totals or per-message costs.
    pub value_mode: ValueMode,
    /// Wall clock at startup, driving the Overview ladder animation.
    started_at: Instant,
}

impl App {
    /// Build the model from parsed CLI options and the detected colour depth.
    pub fn from_cli(cli: &Cli, color_depth: ColorDepth) -> Self {
        Self {
            tab: Tab::Overview,
            theme: cli.theme,
            ascii: cli.ascii,
            show_help: false,
            should_quit: false,
            results_path: cli.results.clone(),
            replay: cli.replay,
            tour: cli.tour,
            color_depth,
            tick: 0,
            last_size: (80, 24),
            data: LoadState::NotLoaded,
            selected_size: 0,
            value_mode: ValueMode::Total,
            started_at: Instant::now(),
        }
    }

    /// Apply an action. Pure with respect to I/O: side work is returned as an [`Effect`].
    pub fn update(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                return Some(Effect::Quit);
            }
            Action::NextTab => {
                self.tab = self.tab.next();
                self.clamp_size();
            }
            Action::PrevTab => {
                self.tab = self.tab.prev();
                self.clamp_size();
            }
            Action::SelectTab(index) => {
                if let Some(tab) = Tab::from_index(index) {
                    self.tab = tab;
                    self.clamp_size();
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::CycleTheme => self.theme = self.theme.next(),
            Action::ToggleAscii => self.ascii = !self.ascii,
            Action::Reload => return Some(Effect::LoadResults(self.results_path.clone())),
            Action::PrevSize => self.selected_size = self.step_size(-1),
            Action::NextSize => self.selected_size = self.step_size(1),
            Action::ToggleValueMode => self.value_mode = self.value_mode.toggled(),
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::Resize { width, height } => self.last_size = (width, height),
        }
        None
    }

    /// The number of selectable dataset sizes on the current tab, or 0 when
    /// the data is not loaded or the tab shows no sizes.
    fn size_count(&self) -> usize {
        let LoadState::Loaded(view) = &self.data else {
            return 0;
        };
        match self.tab {
            Tab::Signing => view.signing_sizes().len(),
            Tab::Verifying => view.verifying_sizes().len(),
            _ => 0,
        }
    }

    /// Step the selected size by `delta` (`-1` or `+1`), wrapping around.
    fn step_size(&self, delta: isize) -> usize {
        let count = self.size_count();
        if count <= 1 {
            return 0;
        }
        (self.selected_size as isize + delta).rem_euclid(count as isize) as usize
    }

    /// Keep the selected size in range after the tab or data changes.
    fn clamp_size(&mut self) {
        let count = self.size_count();
        self.selected_size = self.selected_size.min(count.saturating_sub(1));
    }

    /// The currently selected dataset size index (Signing/Verifying tabs).
    pub fn selected_size(&self) -> usize {
        self.selected_size
    }

    /// The currently selected value mode (totals vs per-message).
    pub fn value_mode(&self) -> ValueMode {
        self.value_mode
    }

    /// Replace the loaded results state (called by the runtime after a load).
    pub fn set_data(&mut self, state: LoadState) {
        self.data = state;
    }

    /// Wall-clock time since startup (drives the Overview ladder animation).
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Resolve the current theme for rendering.
    pub fn theme(&self) -> Theme {
        Theme::new(self.theme, self.color_depth, self.ascii)
    }

    /// The most recently observed terminal size.
    pub fn last_size(&self) -> (u16, u16) {
        self.last_size
    }
}

/// Map a key press to an [`Action`]. This is the single source of key bindings.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(Action::Quit),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Tab => Some(Action::NextTab),
        KeyCode::BackTab => Some(Action::PrevTab),
        KeyCode::Char(c @ '1'..='9') => Some(Action::SelectTab(c as usize - '1' as usize)),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('T') => Some(Action::CycleTheme),
        KeyCode::Char('a') => Some(Action::ToggleAscii),
        KeyCode::Char('r') => Some(Action::Reload),
        KeyCode::Left => Some(Action::PrevSize),
        KeyCode::Right => Some(Action::NextSize),
        KeyCode::Char('v') => Some(Action::ToggleValueMode),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use climb_report::results::{
        AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, EnvironmentFingerprint,
        MemoryResults, SigningBenchmarkResult,
    };
    use climb_report::ClimbResults;

    use crate::data::{ReportHealth, ResultsView};

    fn test_app() -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        App::from_cli(&cli, ColorDepth::TrueColor)
    }

    fn signing_row(size: &str, mean_ns: f64) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: "With MTL".to_string(),
            messages: 100,
            mean_ns,
            ci_lower_ns: mean_ns * 0.9,
            ci_upper_ns: mean_ns * 1.1,
            speedup: None,
        }
    }

    fn loaded_app() -> App {
        let signing = vec![
            signing_row("Small", 1000.0),
            signing_row("OneK", 10_000.0),
            signing_row("Medium", 50_000.0),
        ];
        let view = ResultsView::new(
            ClimbResults {
                version: "1.0".to_string(),
                generated_at: "2026-10-12T00:00:00Z".to_string(),
                workspace_root: "/tmp/climb".to_string(),
                benchmark: BenchmarkResults {
                    signing,
                    verifying: Vec::new(),
                },
                wire_size: Vec::new(),
                memory: MemoryResults {
                    measured: Vec::new(),
                    analytical_model: AnalyticalMemoryModel {
                        tree_page_size_bytes: 4096,
                        tree_max_pages: 1,
                        randomizer_pages: 1,
                        hash_size_bytes: 32,
                    },
                },
                environment: EnvironmentFingerprint {
                    schema_version: 1,
                    timestamp: "2026-10-12T00:00:00Z".to_string(),
                    system: serde_json::json!({}),
                    toolchain: serde_json::json!({}),
                    cpu: serde_json::json!({}),
                    submodules: Vec::new(),
                    docker: serde_json::json!({}),
                    benchmark_runtime: serde_json::json!({}),
                },
                dns_pipeline: None,
                derived: DerivedSummary {
                    overall_wire_reduction_percent: None,
                    largest_wire_reduction_percent: None,
                    overall_verifying_trust_cached_speedup_avg: None,
                    headline: None,
                },
                warnings: Vec::new(),
            },
            Some(std::time::Duration::from_secs(60)),
            ReportHealth::Agree,
        );
        let mut app = test_app();
        app.data = LoadState::Loaded(Box::new(view));
        app.tab = Tab::Signing;
        app
    }

    #[test]
    fn next_wraps_forward_through_every_tab() {
        let mut app = test_app();
        for expected in Tab::ALL.iter().skip(1) {
            app.update(Action::NextTab);
            assert_eq!(app.tab, *expected);
        }
        app.update(Action::NextTab);
        assert_eq!(app.tab, Tab::Overview, "next must wrap past Method");
    }

    #[test]
    fn prev_wraps_backward_through_every_tab() {
        let mut app = test_app();
        app.update(Action::PrevTab);
        assert_eq!(
            app.tab,
            Tab::Method,
            "prev from Overview must wrap to Method"
        );
        for expected in Tab::ALL.iter().rev().skip(1) {
            app.update(Action::PrevTab);
            assert_eq!(app.tab, *expected);
        }
    }

    #[test]
    fn select_tab_uses_zero_based_index_and_ignores_out_of_range() {
        let mut app = test_app();
        app.update(Action::SelectTab(2));
        assert_eq!(app.tab, Tab::Verifying);
        app.update(Action::SelectTab(8));
        assert_eq!(app.tab, Tab::Method);
        app.update(Action::SelectTab(99));
        assert_eq!(app.tab, Tab::Method, "out-of-range select must be ignored");
    }

    #[test]
    fn every_tab_index_round_trips() {
        for (index, tab) in Tab::ALL.iter().enumerate() {
            assert_eq!(tab.index(), index);
            assert_eq!(Tab::from_index(index), Some(*tab));
            assert!(!tab.title().is_empty());
        }
        assert_eq!(Tab::from_index(Tab::ALL.len()), None);
    }

    #[test]
    fn help_toggles_both_ways() {
        let mut app = test_app();
        assert!(!app.show_help);
        app.update(Action::ToggleHelp);
        assert!(app.show_help);
        app.update(Action::ToggleHelp);
        assert!(!app.show_help);
    }

    #[test]
    fn theme_cycles_through_all_and_wraps() {
        let mut app = test_app();
        app.update(Action::CycleTheme);
        assert_eq!(app.theme, ThemeId::Light);
        app.update(Action::CycleTheme);
        assert_eq!(app.theme, ThemeId::Contrast);
        app.update(Action::CycleTheme);
        assert_eq!(app.theme, ThemeId::Dark);
    }

    #[test]
    fn ascii_toggles_both_ways() {
        let mut app = test_app();
        assert!(!app.ascii);
        app.update(Action::ToggleAscii);
        assert!(app.ascii);
        app.update(Action::ToggleAscii);
        assert!(!app.ascii);
    }

    #[test]
    fn tick_increments_and_wraps_on_overflow() {
        let mut app = test_app();
        app.update(Action::Tick);
        assert_eq!(app.tick, 1);
        app.tick = usize::MAX;
        app.update(Action::Tick);
        assert_eq!(app.tick, 0);
    }

    #[test]
    fn resize_records_the_new_size() {
        let mut app = test_app();
        app.update(Action::Resize {
            width: 100,
            height: 30,
        });
        assert_eq!(app.last_size(), (100, 30));
    }

    #[test]
    fn reload_returns_load_effect_for_the_configured_path() {
        let cli = Cli::parse_from(["climb-tui", "--results", "/tmp/fake-results.json"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        assert_eq!(app.data, LoadState::NotLoaded);
        let effect = app.update(Action::Reload);
        assert_eq!(
            effect,
            Some(Effect::LoadResults(PathBuf::from("/tmp/fake-results.json")))
        );
    }

    #[test]
    fn quit_sets_flag_and_returns_effect() {
        let mut app = test_app();
        let effect = app.update(Action::Quit);
        assert_eq!(effect, Some(Effect::Quit));
        assert!(app.should_quit);
    }

    #[test]
    fn non_quit_actions_return_no_effect() {
        let mut app = test_app();
        for action in [
            Action::NextTab,
            Action::PrevTab,
            Action::SelectTab(0),
            Action::ToggleHelp,
            Action::CycleTheme,
            Action::ToggleAscii,
            Action::PrevSize,
            Action::NextSize,
            Action::ToggleValueMode,
            Action::Tick,
            Action::Resize {
                width: 1,
                height: 1,
            },
        ] {
            assert_eq!(app.update(action), None);
        }
    }

    #[test]
    fn map_key_table() {
        use KeyCode::*;
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let cases: [(KeyEvent, Option<Action>); 20] = [
            (KeyEvent::new(Tab, none), Some(Action::NextTab)),
            (KeyEvent::new(BackTab, none), Some(Action::PrevTab)),
            (KeyEvent::new(Char('1'), none), Some(Action::SelectTab(0))),
            (KeyEvent::new(Char('9'), none), Some(Action::SelectTab(8))),
            (KeyEvent::new(Char('5'), none), Some(Action::SelectTab(4))),
            (KeyEvent::new(Char('?'), none), Some(Action::ToggleHelp)),
            (KeyEvent::new(Char('T'), none), Some(Action::CycleTheme)),
            (KeyEvent::new(Char('a'), none), Some(Action::ToggleAscii)),
            (KeyEvent::new(Char('r'), none), Some(Action::Reload)),
            (KeyEvent::new(Left, none), Some(Action::PrevSize)),
            (KeyEvent::new(Right, none), Some(Action::NextSize)),
            (
                KeyEvent::new(Char('v'), none),
                Some(Action::ToggleValueMode),
            ),
            (KeyEvent::new(Char('q'), none), Some(Action::Quit)),
            (KeyEvent::new(Esc, none), Some(Action::Quit)),
            (KeyEvent::new(Char('c'), ctrl), Some(Action::Quit)),
            (KeyEvent::new(Char('0'), none), None),
            (KeyEvent::new(Char('t'), none), None),
            (KeyEvent::new(Char('c'), none), None),
            (KeyEvent::new(Char('x'), none), None),
            (KeyEvent::new(Enter, none), None),
        ];
        for (key, expected) in cases {
            assert_eq!(map_key(key), expected, "wrong mapping for {key:?}");
        }
    }

    #[test]
    fn size_selection_steps_and_wraps_within_loaded_sizes() {
        let mut app = loaded_app();
        let count = app.size_count();
        assert!(count >= 2, "fixture must expose selectable sizes");
        assert_eq!(app.selected_size(), 0);
        app.update(Action::NextSize);
        assert_eq!(app.selected_size(), 1);
        for _ in 0..count - 1 {
            app.update(Action::NextSize);
        }
        assert_eq!(app.selected_size(), 0, "next past the last size wraps");
        app.update(Action::PrevSize);
        assert_eq!(
            app.selected_size(),
            count - 1,
            "prev from the first size wraps"
        );
    }

    #[test]
    fn size_selection_stays_zero_without_loaded_data() {
        let mut app = test_app();
        app.update(Action::NextSize);
        app.update(Action::PrevSize);
        assert_eq!(app.selected_size(), 0);
    }

    #[test]
    fn size_selection_is_clamped_on_tab_change() {
        let mut app = loaded_app();
        let signing_count = app.size_count();
        for _ in 0..signing_count - 1 {
            app.update(Action::NextSize);
        }
        assert_eq!(app.selected_size(), signing_count - 1);
        app.update(Action::SelectTab(2)); // Verifying — no rows in the fixture
        assert_eq!(
            app.selected_size(),
            0,
            "a tab with no selectable sizes clamps to 0"
        );
    }

    #[test]
    fn value_mode_toggles_both_ways() {
        let mut app = loaded_app();
        assert_eq!(app.value_mode(), ValueMode::Total);
        app.update(Action::ToggleValueMode);
        assert_eq!(app.value_mode(), ValueMode::PerMessage);
        app.update(Action::ToggleValueMode);
        assert_eq!(app.value_mode(), ValueMode::Total);
    }
}
