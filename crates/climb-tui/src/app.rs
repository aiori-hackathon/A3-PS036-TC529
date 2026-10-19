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
use crate::data::{LadderLoadState, LiveLoadState, LoadState, ValueMode};
use crate::theme::{ColorDepth, Theme, ThemeId};
use crate::widgets::ladder_view::{
    MAX_LEAVES as LADDER_MAX_LEAVES, MIN_LEAVES as LADDER_MIN_LEAVES,
};

/// Replay beat on the Live tab: one step per second at the runtime's 30 fps
/// tick.
pub const LIVE_AUTOPLAY_TICKS: usize = 30;

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
    /// Grow the modelled ladder by one leaf (Ladder tab).
    LadderUp,
    /// Shrink the modelled ladder by one leaf (Ladder tab).
    LadderDown,
    /// Toggle the "one big post-quantum signature" banner (Ladder tab); step
    /// the replay forward (Live tab).
    ToggleBanner,
    /// Toggle automatic leaf advance (Ladder tab) or replay advance (Live tab).
    ToggleAutoplay,
    /// Step the Live replay back one packet leg (Live tab).
    LivePrevStep,
    /// Toggle the Live bar scale between linear and log10 (Live tab).
    ToggleScale,
    /// Cycle the baseline shown on Wire/Signing tabs (A=plain, B=MTL non-amortised, C=MTL amortised).
    CycleBaseline,
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
    /// Active baseline on Wire/Signing tabs: 0=A (plain), 1=B (MTL non-amortised), 2=C (MTL amortised).
    pub baseline: usize,
    /// Leaves (messages) modelled on the Ladder tab.
    pub ladder_n: u64,
    /// Selected leaf (message) on the Ladder tab.
    pub ladder_leaf: u64,
    /// Whether the "one big signature" banner is shown.
    pub ladder_banner: bool,
    /// Whether the Ladder tab advances the selected leaf each tick.
    pub ladder_autoplay: bool,
    /// Recorded DNS capture state; updated by the runtime on load/reload.
    live: LiveLoadState,
    /// Replay cursor into the recorded capture's step list (Live tab).
    pub live_step: usize,
    /// Whether the Live tab advances the replay cursor each beat.
    pub live_autoplay: bool,
    /// Live bar scale: `true` = log10, `false` = linear.
    pub live_log_scale: bool,
    /// Scroll offset into the Integrity tab's claim list.
    pub integrity_scroll: usize,
    /// Recorded ladder verification; updated by the runtime on load/reload.
    ladder: LadderLoadState,
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
            baseline: 0,
            ladder_n: 8,
            ladder_leaf: 0,
            ladder_banner: false,
            ladder_autoplay: false,
            // Pre-load placeholder: the runtime replaces it with the real load
            // outcome before the first frame is drawn.
            live: LiveLoadState::Missing {
                path: PathBuf::from(crate::data::LIVE_CAPTURE_PATH),
            },
            live_step: 0,
            live_autoplay: false,
            live_log_scale: false,
            integrity_scroll: 0,
            ladder: LadderLoadState::Absent,
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
            Action::PrevSize => {
                if self.tab == Tab::Ladder {
                    self.ladder_leaf = self.step_leaf(-1);
                } else {
                    self.selected_size = self.step_size(-1);
                }
            }
            Action::NextSize => {
                if self.tab == Tab::Ladder {
                    self.ladder_leaf = self.step_leaf(1);
                } else {
                    self.selected_size = self.step_size(1);
                }
            }
            Action::ToggleValueMode => self.value_mode = self.value_mode.toggled(),
            Action::CycleBaseline => self.baseline = (self.baseline + 1) % 3,
            Action::Tick => {
                self.tick = self.tick.wrapping_add(1);
                if self.tab == Tab::Ladder && self.ladder_autoplay {
                    self.ladder_leaf = self.step_leaf(1);
                }
                if self.tab == Tab::Live
                    && self.live_autoplay
                    && self.tick.is_multiple_of(LIVE_AUTOPLAY_TICKS)
                {
                    self.step_live(1);
                }
            }
            Action::LadderUp => {
                if self.tab == Tab::Integrity {
                    self.integrity_scroll = self.integrity_scroll.saturating_sub(1);
                } else {
                    self.ladder_n = (self.ladder_n + 1).min(LADDER_MAX_LEAVES);
                    self.clamp_leaf();
                }
            }
            Action::LadderDown => {
                if self.tab == Tab::Integrity {
                    self.integrity_scroll = self.integrity_scroll.saturating_add(1);
                } else {
                    self.ladder_n = self.ladder_n.saturating_sub(1).max(LADDER_MIN_LEAVES);
                    self.clamp_leaf();
                }
            }
            Action::ToggleBanner => {
                if self.tab == Tab::Live {
                    self.step_live(1);
                } else {
                    self.ladder_banner = !self.ladder_banner;
                }
            }
            Action::ToggleAutoplay => {
                if self.tab == Tab::Live {
                    self.live_autoplay = !self.live_autoplay;
                } else {
                    self.ladder_autoplay = !self.ladder_autoplay;
                }
            }
            Action::LivePrevStep => self.step_live(-1),
            Action::ToggleScale => self.live_log_scale = !self.live_log_scale,
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

    /// Step the selected leaf by `delta` (`-1` or `+1`), wrapping around.
    fn step_leaf(&self, delta: isize) -> u64 {
        let n = self.ladder_n.max(1);
        (self.ladder_leaf as isize + delta).rem_euclid(n as isize) as u64
    }

    /// Step the Live replay cursor by `delta`, wrapping within the step list.
    /// An empty or unloaded capture keeps the cursor at 0 and never reaches
    /// the wrapping arithmetic.
    fn step_live(&mut self, delta: isize) {
        let count = self.live.step_count();
        if count == 0 {
            self.live_step = 0;
            return;
        }
        self.live_step = (self.live_step as isize + delta).rem_euclid(count as isize) as usize;
    }

    /// Keep the selected leaf in range after the ladder size changes.
    fn clamp_leaf(&mut self) {
        self.ladder_leaf = self.ladder_leaf.min(self.ladder_n.saturating_sub(1));
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

    /// The active baseline on Wire/Signing tabs (0=A, 1=B, 2=C).
    pub fn baseline(&self) -> usize {
        self.baseline
    }

    /// Leaves (messages) modelled on the Ladder tab.
    pub fn ladder_n(&self) -> u64 {
        self.ladder_n
    }

    /// Selected leaf (message) on the Ladder tab.
    pub fn ladder_leaf(&self) -> u64 {
        self.ladder_leaf
    }

    /// Whether the "one big signature" banner is shown.
    pub fn ladder_banner(&self) -> bool {
        self.ladder_banner
    }

    /// Whether the Ladder tab auto-advances the selected leaf.
    pub fn ladder_autoplay(&self) -> bool {
        self.ladder_autoplay
    }

    /// The recorded ladder verification, when one loaded.
    pub fn ladder(&self) -> &LadderLoadState {
        &self.ladder
    }

    /// Replace the loaded ladder-verification state (called by the runtime).
    pub fn set_ladder(&mut self, state: LadderLoadState) {
        self.ladder = state;
    }

    /// The recorded DNS capture state.
    pub fn live(&self) -> &LiveLoadState {
        &self.live
    }

    /// Replace the loaded capture state (called by the runtime), keeping the
    /// replay cursor inside the new step list.
    pub fn set_live(&mut self, state: LiveLoadState) {
        self.live = state;
        self.live_step = self.live_step.min(self.live.step_count().saturating_sub(1));
    }

    /// Replay cursor into the recorded capture's step list (Live tab).
    pub fn live_step(&self) -> usize {
        self.live_step
    }

    /// Whether the Live tab auto-advances the replay.
    pub fn live_autoplay(&self) -> bool {
        self.live_autoplay
    }

    /// Whether the Live bars are drawn on a log10 scale.
    pub fn live_log_scale(&self) -> bool {
        self.live_log_scale
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
        KeyCode::Up => Some(Action::LadderUp),
        KeyCode::Down => Some(Action::LadderDown),
        KeyCode::Char(' ') => Some(Action::ToggleBanner),
        KeyCode::Enter => Some(Action::ToggleAutoplay),
        KeyCode::Backspace => Some(Action::LivePrevStep),
        KeyCode::Char('t') => Some(Action::ToggleScale),
        KeyCode::Char('v') => Some(Action::ToggleValueMode),
        KeyCode::Char('b') => Some(Action::CycleBaseline),
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
                dns_anatomy: None,
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
                ladder: None,
                plain: Vec::new(),
                classical: Vec::new(),
                comparisons: Vec::new(),
                derived: DerivedSummary {
                    overall_wire_reduction_percent: None,
                    largest_wire_reduction_percent: None,
                    overall_verifying_trust_cached_speedup_avg: None,
                    headline: None,
                    reuse: Vec::new(),
                    udp_fit: None,
                    signature_context: Vec::new(),
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
            Action::LadderUp,
            Action::LadderDown,
            Action::ToggleBanner,
            Action::ToggleAutoplay,
            Action::LivePrevStep,
            Action::ToggleScale,
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
        let cases: [(KeyEvent, Option<Action>); 25] = [
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
            (KeyEvent::new(Up, none), Some(Action::LadderUp)),
            (KeyEvent::new(Down, none), Some(Action::LadderDown)),
            (KeyEvent::new(Char(' '), none), Some(Action::ToggleBanner)),
            (KeyEvent::new(Enter, none), Some(Action::ToggleAutoplay)),
            (KeyEvent::new(Backspace, none), Some(Action::LivePrevStep)),
            (KeyEvent::new(Char('t'), none), Some(Action::ToggleScale)),
            (
                KeyEvent::new(Char('v'), none),
                Some(Action::ToggleValueMode),
            ),
            (KeyEvent::new(Char('q'), none), Some(Action::Quit)),
            (KeyEvent::new(Esc, none), Some(Action::Quit)),
            (KeyEvent::new(Char('c'), ctrl), Some(Action::Quit)),
            (KeyEvent::new(Char('0'), none), None),
            (KeyEvent::new(Char('c'), none), None),
            (KeyEvent::new(Char('x'), none), None),
            (KeyEvent::new(Char('z'), none), None),
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

    #[test]
    fn ladder_size_clamps_between_one_and_thirty_two() {
        let mut app = test_app();
        app.ladder_n = LADDER_MAX_LEAVES;
        app.update(Action::LadderUp);
        assert_eq!(app.ladder_n(), LADDER_MAX_LEAVES, "up past 32 clamps");
        for _ in 0..(LADDER_MAX_LEAVES + 5) {
            app.update(Action::LadderDown);
        }
        assert_eq!(app.ladder_n(), LADDER_MIN_LEAVES, "down below 1 clamps");
        assert_eq!(app.ladder_leaf(), 0);
    }

    #[test]
    fn up_and_down_scroll_the_integrity_claim_list_instead_of_the_ladder() {
        let mut app = test_app();
        app.tab = Tab::Integrity;
        app.update(Action::LadderDown);
        assert_eq!(app.integrity_scroll, 1);
        app.update(Action::LadderDown);
        assert_eq!(app.integrity_scroll, 2);
        app.update(Action::LadderUp);
        assert_eq!(app.integrity_scroll, 1);
        app.update(Action::LadderUp);
        app.update(Action::LadderUp);
        assert_eq!(app.integrity_scroll, 0, "scrolling up at the top stays put");
        assert_eq!(
            app.ladder_n(),
            8,
            "the ladder size is untouched on Integrity"
        );

        // Back to a non-zero offset so the tab switch below proves the
        // offset survives rather than being coincidentally zero.
        app.update(Action::LadderDown);
        assert_eq!(app.integrity_scroll, 1);
        app.update(Action::SelectTab(Tab::Ladder.index()));
        app.update(Action::LadderDown);
        assert_eq!(
            app.ladder_n(),
            7,
            "down on the Ladder tab still shrinks the ladder"
        );
        assert_eq!(
            app.integrity_scroll, 1,
            "the scroll offset is kept for the tab"
        );
    }

    #[test]
    fn shrinking_the_ladder_clamps_the_selected_leaf() {
        let mut app = test_app();
        app.ladder_n = 16;
        app.ladder_leaf = 15;
        for _ in 0..8 {
            app.update(Action::LadderDown);
        }
        assert_eq!(app.ladder_n(), 8);
        assert_eq!(app.ladder_leaf(), 7, "leaf clamps into the smaller ladder");
    }

    #[test]
    fn left_and_right_steer_the_leaf_on_the_ladder_tab() {
        let mut app = test_app();
        app.tab = Tab::Ladder;
        app.ladder_n = 8;
        app.ladder_leaf = 0;
        app.update(Action::NextSize);
        assert_eq!(app.ladder_leaf(), 1);
        app.update(Action::PrevSize);
        app.update(Action::PrevSize);
        assert_eq!(app.ladder_leaf(), 7, "prev from the first leaf wraps");
        assert_eq!(app.selected_size(), 0, "the size index is untouched");
    }

    #[test]
    fn banner_and_autoplay_toggle_independently() {
        let mut app = test_app();
        assert!(!app.ladder_banner());
        assert!(!app.ladder_autoplay());
        app.update(Action::ToggleBanner);
        assert!(app.ladder_banner());
        assert!(!app.ladder_autoplay());
        app.update(Action::ToggleAutoplay);
        app.update(Action::ToggleBanner);
        assert!(!app.ladder_banner());
        assert!(app.ladder_autoplay());
    }

    #[test]
    fn tick_advances_the_leaf_only_when_the_ladder_tab_autoplays() {
        let mut app = test_app();
        app.tab = Tab::Ladder;
        app.ladder_n = 4;
        app.ladder_leaf = 0;
        app.update(Action::Tick);
        assert_eq!(app.ladder_leaf(), 0, "no autoplay, no advance");

        app.update(Action::ToggleAutoplay);
        app.update(Action::Tick);
        assert_eq!(app.ladder_leaf(), 1);
        app.update(Action::Tick);
        app.update(Action::Tick);
        app.update(Action::Tick);
        assert_eq!(app.ladder_leaf(), 0, "autoplay wraps past the last leaf");

        app.tab = Tab::Overview;
        app.update(Action::Tick);
        assert_eq!(
            app.ladder_leaf(),
            0,
            "off the Ladder tab the leaf is frozen"
        );
    }

    /// An app on the Live tab with the committed sample capture loaded (§7c:
    /// the dashboard consumes the real artifact, not a hand-built fake).
    fn live_app() -> App {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/dns-capture.recorded.json");
        let mut app = test_app();
        app.tab = Tab::Live;
        app.set_live(crate::data::load_live(&path));
        assert!(
            app.live().view().is_some(),
            "the committed sample capture must load"
        );
        app
    }

    #[test]
    fn space_and_backspace_step_the_live_replay_wrapping_at_both_ends() {
        let mut app = live_app();
        let count = app.live().step_count();
        assert!(count >= 3, "fixture must expose several steps");
        assert_eq!(app.live_step(), 0);

        app.update(Action::ToggleBanner); // Space on the Live tab steps forward
        assert_eq!(app.live_step(), 1);
        assert!(
            !app.ladder_banner,
            "stepping the replay must not touch the ladder banner"
        );

        app.update(Action::LivePrevStep);
        assert_eq!(app.live_step(), 0, "back on the first step stays put");
        app.update(Action::LivePrevStep);
        assert_eq!(app.live_step(), count - 1, "back past the first wraps");
        app.update(Action::ToggleBanner);
        assert_eq!(app.live_step(), 0, "forward past the last wraps");
    }

    #[test]
    fn enter_autoplays_the_live_replay_once_per_beat_and_only_on_the_live_tab() {
        let mut app = live_app();
        app.update(Action::ToggleAutoplay);
        assert!(app.live_autoplay());
        assert!(!app.ladder_autoplay, "autoplay is live-scoped here");

        for _ in 0..LIVE_AUTOPLAY_TICKS - 1 {
            app.update(Action::Tick);
        }
        assert_eq!(app.live_step(), 0, "no beat has elapsed yet");
        app.update(Action::Tick);
        assert_eq!(app.live_step(), 1, "one beat, one step");

        app.tab = Tab::Overview;
        for _ in 0..LIVE_AUTOPLAY_TICKS {
            app.update(Action::Tick);
        }
        assert_eq!(app.live_step(), 1, "off the Live tab the replay is frozen");
    }

    #[test]
    fn t_toggles_the_live_bar_scale() {
        let mut app = test_app();
        assert!(!app.live_log_scale());
        app.update(Action::ToggleScale);
        assert!(app.live_log_scale(), "t turns the log scale on");
        app.update(Action::ToggleScale);
        assert!(!app.live_log_scale(), "t turns it back off");
    }

    #[test]
    fn stepping_an_unloaded_capture_stays_at_zero_instead_of_panicking() {
        let mut app = test_app();
        app.tab = Tab::Live;
        assert_eq!(app.live_step(), 0);
        app.update(Action::ToggleBanner);
        app.update(Action::LivePrevStep);
        app.update(Action::Tick);
        assert_eq!(app.live_step(), 0, "no steps to move through");
    }

    #[test]
    fn set_live_clamps_the_replay_cursor_into_the_new_state() {
        let mut app = live_app();
        app.live_step = 4;
        app.set_live(crate::data::LiveLoadState::Missing {
            path: PathBuf::from("missing.json"),
        });
        assert_eq!(
            app.live_step(),
            0,
            "an unloaded capture has no cursor to keep"
        );
    }
}
