//! The application model (phase 18.5), Elm style.
//!
//! [`App`] is the model, [`Action`] the messages, and [`App::update`] the pure transition:
//! it mutates the model and *returns* an optional [`Effect`] describing side work. It never
//! touches the terminal, files or the clock, so the whole model is unit-testable. All key
//! binding lives in one place, [`map_key`].

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::cli::Cli;
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
    /// Load consolidated results; wired in a later phase.
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
        }
    }

    /// Apply an action. Pure with respect to I/O: side work is returned as an [`Effect`].
    pub fn update(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                return Some(Effect::Quit);
            }
            Action::NextTab => self.tab = self.tab.next(),
            Action::PrevTab => self.tab = self.tab.prev(),
            Action::SelectTab(index) => {
                if let Some(tab) = Tab::from_index(index) {
                    self.tab = tab;
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::CycleTheme => self.theme = self.theme.next(),
            Action::ToggleAscii => self.ascii = !self.ascii,
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::Resize { width, height } => self.last_size = (width, height),
        }
        None
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn test_app() -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        App::from_cli(&cli, ColorDepth::TrueColor)
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
        let cases: [(KeyEvent, Option<Action>); 16] = [
            (KeyEvent::new(Tab, none), Some(Action::NextTab)),
            (KeyEvent::new(BackTab, none), Some(Action::PrevTab)),
            (KeyEvent::new(Char('1'), none), Some(Action::SelectTab(0))),
            (KeyEvent::new(Char('9'), none), Some(Action::SelectTab(8))),
            (KeyEvent::new(Char('5'), none), Some(Action::SelectTab(4))),
            (KeyEvent::new(Char('?'), none), Some(Action::ToggleHelp)),
            (KeyEvent::new(Char('T'), none), Some(Action::CycleTheme)),
            (KeyEvent::new(Char('a'), none), Some(Action::ToggleAscii)),
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
}
