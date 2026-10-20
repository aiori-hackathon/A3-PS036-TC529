//! Terminal lifecycle safety.
//!
//! [`TerminalGuard`] owns raw mode and the alternate screen and restores both in `Drop`,
//! including on early return from `?`. [`install_panic_hook`] installs a process-wide hook
//! that restores the terminal *before* the panic message is printed, so a panic never
//! leaves the user in a broken alternate screen full of raw-mode output.

use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

static PANIC_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Whether [`install_panic_hook`] has run in this process.
pub fn panic_hook_installed() -> bool {
    PANIC_HOOK_INSTALLED.load(Ordering::SeqCst)
}

/// Install a panic hook that restores the terminal before delegating to the previous hook.
///
/// Idempotent: repeated calls are no-ops, so it is safe to call from any entry point.
pub fn install_panic_hook() {
    if PANIC_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

fn restore_terminal() {
    let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    let _ = disable_raw_mode();
}

/// Owns the terminal's raw mode and alternate screen for the lifetime of the TUI.
pub struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    /// Enter raw mode and the alternate screen.
    ///
    /// Mouse capture is deliberately left disabled: nothing in the dashboard needs it and
    /// enabling it would swallow the terminal's own text selection.
    pub fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling raw mode")?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(anyhow::Error::from(error).context("entering alternate screen"));
        }
        Ok(Self { active: true })
    }

    /// Restore the terminal. Idempotent; also called from `Drop`.
    pub fn restore(&mut self) {
        if self.active {
            let _ = execute!(stdout(), LeaveAlternateScreen, Show);
            let _ = disable_raw_mode();
            self.active = false;
        }
    }

    /// Whether the terminal is currently owned by the guard.
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[cfg(test)]
    pub(crate) fn inactive() -> Self {
        Self { active: false }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_hook_install_is_idempotent() {
        install_panic_hook();
        assert!(panic_hook_installed());
        install_panic_hook();
        assert!(panic_hook_installed());
    }

    #[test]
    fn inactive_guard_restore_is_a_noop_and_repeatable() {
        let mut guard = TerminalGuard::inactive();
        assert!(!guard.is_active());
        guard.restore();
        guard.restore();
        assert!(!guard.is_active());
    }
}
