//! One key vocabulary for every front end.
//!
//! The dashboard's bindings live here, not in a backend crate: [`map_key`] takes
//! a neutral [`Key`] so the same table serves the terminal and the browser. The
//! native event pump converts a terminal key into a [`Key`] through
//! `From<KeyEvent>`; the browser converts the DOM's `KeyboardEvent.key` string
//! through [`key_from_web`], which is the one web adapter and is unit-tested
//! here rather than in the wasm crate.
//!
//! Mapping stays pure: no I/O, no clock, no terminal. A browser key that the
//! dashboard does not handle maps to `None`, which also tells the page not to
//! swallow the event.

use crate::app::Action;

/// A key code, reduced to the vocabulary the dashboard binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// A character key. The browser reports the shifted character itself.
    Char(char),
    /// A function key.
    F(u8),
    /// Tab.
    Tab,
    /// Shift-Tab, which terminals report directly and browsers imply.
    BackTab,
    /// Backspace.
    Backspace,
    /// Enter / return.
    Enter,
    /// Escape.
    Esc,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// The null key reported by some terminals.
    Null,
    /// A key the dashboard has no vocabulary for.
    Unknown,
}

/// Modifier keys, as a small set.
///
/// Only Control, Shift and Alt are meaningful here; every other terminal or
/// browser modifier is deliberately dropped at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    /// No modifier held.
    pub const NONE: Self = Self(0);
    /// The Shift key.
    pub const SHIFT: Self = Self(1);
    /// The Control key.
    pub const CONTROL: Self = Self(2);
    /// The Alt key.
    pub const ALT: Self = Self(4);

    /// Whether every modifier in `other` is held.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no modifier is held.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The union of two modifier sets.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// One key press, independent of the terminal or browser that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Which key was pressed.
    pub code: KeyCode,
    /// Which modifiers were held.
    pub modifiers: KeyModifiers,
}

impl Key {
    /// Build a key press.
    #[must_use]
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Build a key press with no modifier held.
    #[must_use]
    pub fn plain(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }
}

/// Translate a DOM `KeyboardEvent` into a [`Key`].
///
/// `key` is the browser's `KeyboardEvent.key` value, which is already
/// layout-resolved and shift-resolved: pressing shift and `t` reports `"T"`,
/// while shift and Tab reports `"Tab"` with `shift` set. Named keys are matched
/// exactly; any other single character becomes [`KeyCode::Char`]. Modifier
/// keys, composition keys and multi-character names this dashboard does not
/// bind return `None`.
#[must_use]
pub fn key_from_web(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Key> {
    let mut modifiers = KeyModifiers::NONE;
    if ctrl {
        modifiers = modifiers | KeyModifiers::CONTROL;
    }
    if alt {
        modifiers = modifiers | KeyModifiers::ALT;
    }
    if shift {
        modifiers = modifiers | KeyModifiers::SHIFT;
    }

    let code = match key {
        "Tab" if shift => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        "Enter" => KeyCode::Enter,
        "Escape" | "Esc" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "Insert" => KeyCode::Insert,
        "Delete" => KeyCode::Delete,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "F1" => KeyCode::F(1),
        "F2" => KeyCode::F(2),
        "F3" => KeyCode::F(3),
        "F4" => KeyCode::F(4),
        "F5" => KeyCode::F(5),
        "F6" => KeyCode::F(6),
        "F7" => KeyCode::F(7),
        "F8" => KeyCode::F(8),
        "F9" => KeyCode::F(9),
        "F10" => KeyCode::F(10),
        "F11" => KeyCode::F(11),
        "F12" => KeyCode::F(12),
        // Modifier keys arrive as their own keydown events. They are not a
        // dashboard input; treating them as unknown keeps Shift alone from
        // clearing a selected tab.
        "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" | "NumLock" | "ScrollLock" | "Dead"
        | "Unidentified" | "Process" | "Compose" => return None,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(character), None) => KeyCode::Char(character),
                _ => return None,
            }
        }
    };
    Some(Key::new(code, modifiers))
}

/// The web adapter used by the browser runtime: one DOM key, one action.
///
/// This is deliberately a thin wrapper over [`map_key`] so the browser cannot
/// drift from the terminal's binding table.
#[must_use]
pub fn map_web_key(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Action> {
    key_from_web(key, ctrl, alt, shift).and_then(map_key)
}

/// Map a key press to an [`Action`]. This is the single source of key bindings.
pub fn map_key(key: Key) -> Option<Action> {
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

/// Map a key press while the guided tour is running.
///
/// Only the tour's own controls pass: `p` pause/resume, `n` next step, `Esc`
/// leave the tour, plus `q`/Ctrl-C to quit the dashboard and the theme/ascii
/// and help toggles. Every other binding is swallowed so a recording cannot
/// be knocked off the script mid-step, and the tour re-pins its tab on every
/// tick regardless.
pub fn map_key_tour(key: Key) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(Action::Quit),
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Esc => Some(Action::QuitTour),
        KeyCode::Char('p') => Some(Action::TourPause),
        KeyCode::Char('n') => Some(Action::TourNext),
        KeyCode::Char('T') => Some(Action::CycleTheme),
        KeyCode::Char('a') => Some(Action::ToggleAscii),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}

/// Convert the native terminal's key event into the shared [`Key`].
///
/// Only the terminal build needs crossterm, and only this conversion mentions
/// it; the binding table above never does.
#[cfg(feature = "terminal")]
impl From<crossterm::event::KeyEvent> for Key {
    fn from(event: crossterm::event::KeyEvent) -> Self {
        use crossterm::event::{KeyCode as Terminal, KeyModifiers as TerminalModifiers};

        let mut modifiers = KeyModifiers::NONE;
        if event.modifiers.contains(TerminalModifiers::CONTROL) {
            modifiers = modifiers | KeyModifiers::CONTROL;
        }
        if event.modifiers.contains(TerminalModifiers::ALT) {
            modifiers = modifiers | KeyModifiers::ALT;
        }
        if event.modifiers.contains(TerminalModifiers::SHIFT) {
            modifiers = modifiers | KeyModifiers::SHIFT;
        }
        let code = match event.code {
            Terminal::Char(c) => KeyCode::Char(c),
            Terminal::F(n) => KeyCode::F(n),
            Terminal::Tab => KeyCode::Tab,
            Terminal::BackTab => KeyCode::BackTab,
            Terminal::Backspace => KeyCode::Backspace,
            Terminal::Enter => KeyCode::Enter,
            Terminal::Esc => KeyCode::Esc,
            Terminal::Left => KeyCode::Left,
            Terminal::Right => KeyCode::Right,
            Terminal::Up => KeyCode::Up,
            Terminal::Down => KeyCode::Down,
            Terminal::Insert => KeyCode::Insert,
            Terminal::Delete => KeyCode::Delete,
            Terminal::Home => KeyCode::Home,
            Terminal::End => KeyCode::End,
            Terminal::PageUp => KeyCode::PageUp,
            Terminal::PageDown => KeyCode::PageDown,
            Terminal::Null => KeyCode::Null,
            _ => KeyCode::Unknown,
        };
        Self::new(code, modifiers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_key_table() {
        use KeyCode::*;
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let cases: [(Key, Option<Action>); 27] = [
            (Key::new(Tab, none), Some(Action::NextTab)),
            (Key::new(BackTab, none), Some(Action::PrevTab)),
            (Key::new(Char('1'), none), Some(Action::SelectTab(0))),
            (Key::new(Char('9'), none), Some(Action::SelectTab(8))),
            (Key::new(Char('5'), none), Some(Action::SelectTab(4))),
            (Key::new(Char('?'), none), Some(Action::ToggleHelp)),
            (Key::new(Char('T'), none), Some(Action::CycleTheme)),
            (Key::new(Char('a'), none), Some(Action::ToggleAscii)),
            (Key::new(Char('r'), none), Some(Action::Reload)),
            (Key::new(Left, none), Some(Action::PrevSize)),
            (Key::new(Right, none), Some(Action::NextSize)),
            (Key::new(Up, none), Some(Action::LadderUp)),
            (Key::new(Down, none), Some(Action::LadderDown)),
            (Key::new(Char(' '), none), Some(Action::ToggleBanner)),
            (Key::new(Enter, none), Some(Action::ToggleAutoplay)),
            (Key::new(Backspace, none), Some(Action::LivePrevStep)),
            (Key::new(Char('t'), none), Some(Action::ToggleScale)),
            (Key::new(Char('v'), none), Some(Action::ToggleValueMode)),
            (Key::new(Char('q'), none), Some(Action::Quit)),
            (Key::new(Esc, none), Some(Action::Quit)),
            (Key::new(Char('c'), ctrl), Some(Action::Quit)),
            (Key::new(Char('0'), none), None),
            (Key::new(Char('c'), none), None),
            (Key::new(Char('x'), none), None),
            (Key::new(Char('z'), none), None),
            // `p` and `n` are the guided tour's keys; outside the tour they
            // are unbound (`map_key_tour` binds them).
            (Key::new(Char('p'), none), None),
            (Key::new(Char('n'), none), None),
        ];
        for (key, expected) in cases {
            assert_eq!(map_key(key), expected, "wrong mapping for {key:?}");
        }
    }

    #[test]
    fn map_key_tour_scopes_the_keys_while_the_tour_runs() {
        use KeyCode::*;
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let cases: [(Key, Option<Action>); 14] = [
            (Key::new(Char('p'), none), Some(Action::TourPause)),
            (Key::new(Char('n'), none), Some(Action::TourNext)),
            (Key::new(Esc, none), Some(Action::QuitTour)),
            (Key::new(Char('q'), none), Some(Action::Quit)),
            (Key::new(Char('c'), ctrl), Some(Action::Quit)),
            (Key::new(Char('T'), none), Some(Action::CycleTheme)),
            (Key::new(Char('a'), none), Some(Action::ToggleAscii)),
            (Key::new(Char('?'), none), Some(Action::ToggleHelp)),
            // Everything else is swallowed mid-tour: a recording must not be
            // knocked off the script.
            (Key::new(Tab, none), None),
            (Key::new(Char('1'), none), None),
            (Key::new(Char(' '), none), None),
            (Key::new(Char('r'), none), None),
            (Key::new(Up, none), None),
            (Key::new(Char('t'), none), None),
        ];
        for (key, expected) in cases {
            assert_eq!(
                map_key_tour(key),
                expected,
                "wrong tour mapping for {key:?}"
            );
        }
    }

    #[test]
    fn unknown_and_exotic_keys_are_ignored() {
        use KeyCode::*;
        let none = KeyModifiers::NONE;
        for key in [
            Key::new(Char('z'), none),
            Key::new(Char('0'), none),
            Key::new(F(1), none),
            Key::new(F(12), none),
            Key::new(Insert, none),
            Key::new(Delete, none),
            Key::new(Home, none),
            Key::new(End, none),
            Key::new(PageUp, none),
            Key::new(PageDown, none),
            Key::new(Null, none),
            Key::new(Unknown, none),
        ] {
            assert_eq!(map_key(key), None, "{key:?} must be ignored");
        }
    }

    #[test]
    fn named_dom_keys_map_to_the_shared_vocabulary() {
        let cases = [
            ("Tab", false, Key::plain(KeyCode::Tab)),
            ("Enter", false, Key::plain(KeyCode::Enter)),
            ("Escape", false, Key::plain(KeyCode::Esc)),
            ("Backspace", false, Key::plain(KeyCode::Backspace)),
            ("ArrowLeft", false, Key::plain(KeyCode::Left)),
            ("ArrowRight", false, Key::plain(KeyCode::Right)),
            ("ArrowUp", false, Key::plain(KeyCode::Up)),
            ("ArrowDown", false, Key::plain(KeyCode::Down)),
            ("Delete", false, Key::plain(KeyCode::Delete)),
            ("Home", false, Key::plain(KeyCode::Home)),
            ("End", false, Key::plain(KeyCode::End)),
            ("PageUp", false, Key::plain(KeyCode::PageUp)),
            ("PageDown", false, Key::plain(KeyCode::PageDown)),
            ("F5", false, Key::plain(KeyCode::F(5))),
            ("q", false, Key::plain(KeyCode::Char('q'))),
            ("Q", true, Key::new(KeyCode::Char('Q'), KeyModifiers::SHIFT)),
            (" ", false, Key::plain(KeyCode::Char(' '))),
            ("?", true, Key::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
        ];
        for (name, shift, expected) in cases {
            assert_eq!(
                key_from_web(name, false, false, shift),
                Some(expected),
                "DOM key {name:?} with shift={shift}"
            );
        }
    }

    #[test]
    fn shift_tab_is_backtab_and_modifiers_are_kept() {
        assert_eq!(
            key_from_web("Tab", false, false, true),
            Some(Key::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        );
        assert_eq!(
            key_from_web("c", true, false, false),
            Some(Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            key_from_web("r", false, true, false),
            Some(Key::new(KeyCode::Char('r'), KeyModifiers::ALT))
        );
    }

    #[test]
    fn the_web_adapter_agrees_with_the_native_binding_table() {
        // Every binding the terminal offers is reachable from the DOM. If a
        // binding is added to `map_key` without a DOM spelling here, this test
        // fails rather than the browser silently losing the key.
        let bindings = [
            ("Tab", false, false, false),
            ("Tab", false, false, true),
            ("1", false, false, false),
            ("9", false, false, false),
            ("?", false, false, true),
            ("T", true, false, true),
            ("a", false, false, false),
            ("r", false, false, false),
            ("ArrowLeft", false, false, false),
            ("ArrowRight", false, false, false),
            ("ArrowUp", false, false, false),
            ("ArrowDown", false, false, false),
            (" ", false, false, false),
            ("Enter", false, false, false),
            ("Backspace", false, false, false),
            ("t", false, false, false),
            ("v", false, false, false),
            ("b", false, false, false),
            ("q", false, false, false),
            ("Escape", false, false, false),
            ("c", true, false, false),
        ];
        for (name, ctrl, alt, shift) in bindings {
            let native = key_from_web(name, ctrl, alt, shift).expect("the DOM key is known");
            let action = map_web_key(name, ctrl, alt, shift);
            assert_eq!(
                action,
                map_key(native),
                "the web path and the native table disagree for {name:?}"
            );
            assert!(
                action.is_some(),
                "{name:?} is a native binding and must stay reachable from the web"
            );
        }
    }

    #[test]
    fn modifier_and_unbound_dom_keys_are_ignored() {
        for name in [
            "Shift",
            "Control",
            "Alt",
            "Meta",
            "CapsLock",
            "Dead",
            "Unidentified",
            "Process",
            "Compose",
            "F1",
            "z",
            "0",
        ] {
            assert_eq!(
                map_web_key(name, false, false, false),
                None,
                "{name:?} must not act"
            );
        }
    }
}
