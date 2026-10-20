//! The guided tour's caption bar: one row above the status bar, naming the
//! step, showing its caption, its progress and the tour's keys.
//!
//! The bar is pure presentation over [`TourRunner`]: it receives the runner
//! and the injected clock's `now`, so its tests drive tour time by hand.

use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::theme::Theme;
use crate::tour::TourRunner;
use crate::widgets::ellipsize;

/// Draw the caption bar for the running tour across `area` (one row).
pub fn render_tour_bar(
    frame: &mut Frame,
    area: Rect,
    runner: &TourRunner,
    now: Duration,
    theme: &Theme,
) {
    let (head, tail) = labels(runner, now, theme.ascii);
    let budget = (area.width as usize).saturating_sub(head.chars().count() + tail.chars().count());
    let caption = ellipsize(runner.caption(), budget, theme.ascii);
    let line = Line::from(vec![
        Span::styled(
            head,
            Style::new()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(caption, Style::new().fg(theme.fg)),
        Span::styled(tail, Style::new().fg(theme.muted)),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .style(Style::new().fg(theme.fg).bg(theme.bg))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The fixed pieces of the bar: the head block (step position and tab) and
/// the tail (elapsed/dwell seconds and the tour's keys). The caption between
/// them is what gets ellipsized, so the controls never scroll off.
fn labels(runner: &TourRunner, now: Duration, ascii: bool) -> (String, String) {
    let sep = if ascii { " - " } else { " \u{b7} " };
    let tab = runner.tab().map_or("", |tab| tab.title());
    let (head, tail) = if runner.is_done() {
        (
            format!(" Tour complete{sep}{tab} "),
            " Esc returns to browsing ".to_string(),
        )
    } else {
        let (shown, total) = runner.position();
        let (pause, resume) = if runner.is_paused() {
            (" PAUSED", "p resume")
        } else {
            ("", "p pause")
        };
        let dwell = runner.step().map_or(0, |step| step.dwell.as_secs());
        (
            format!(" Tour {shown}/{total}{sep}{tab} "),
            format!(
                " {}/{}s{pause}{sep}{resume}{sep}n next{sep}Esc quit ",
                runner.step_elapsed(now).as_secs(),
                dwell,
            ),
        )
    };
    (head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};
    use crate::tour::TourRunner;

    fn theme(ascii: bool) -> Theme {
        Theme::new(ThemeId::Dark, ColorDepth::TrueColor, ascii)
    }

    /// Render just the bar, one row tall, into a string.
    fn bar(runner: &TourRunner, now: Duration, width: u16, ascii: bool) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                render_tour_bar(frame, frame.area(), runner, now, &theme(ascii));
            })
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, 0)) {
                out.push_str(cell.symbol());
            }
        }
        out
    }

    #[test]
    fn caption_bar_names_the_step_its_caption_and_its_keys() {
        let runner = TourRunner::new(1.0);
        let out = bar(&runner, Duration::ZERO, 120, false);
        for needle in [
            "Tour 1/9",
            "Overview",
            "The story in one screen",
            "0/20s",
            "p pause",
            "n next",
            "Esc quit",
        ] {
            assert!(out.contains(needle), "tour bar misses {needle:?}:\n{out}");
        }
    }

    #[test]
    fn caption_bar_ticks_the_elapsed_seconds_of_the_step() {
        let runner = TourRunner::new(1.0);
        let out = bar(&runner, Duration::from_secs(7), 120, false);
        assert!(out.contains("7/20s"), "elapsed not drawn:\n{out}");
        let over = bar(&runner, Duration::from_secs(999), 120, false);
        assert!(
            over.contains("20/20s"),
            "elapsed must clamp at the scripted dwell:\n{over}"
        );
    }

    #[test]
    fn paused_bar_marks_the_pause_and_the_resume_key() {
        let mut runner = TourRunner::new(1.0);
        runner.toggle_pause();
        let out = bar(&runner, Duration::ZERO, 120, false);
        assert!(out.contains("PAUSED"), "paused bar:\n{out}");
        assert!(out.contains("p resume"), "paused bar:\n{out}");
        assert!(!out.contains("p pause"), "paused bar:\n{out}");
    }

    #[test]
    fn completed_bar_announces_the_tour_is_over() {
        let mut runner = TourRunner::new(1.0);
        runner.on_tick(crate::tour::script_duration() + std::time::Duration::from_secs(60));
        assert!(runner.is_done());
        let out = bar(&runner, Duration::ZERO, 120, false);
        assert!(out.contains("Tour complete"), "done bar:\n{out}");
        assert!(out.contains("Method"), "the last tab stays named:\n{out}");
        assert!(out.contains("Esc returns to browsing"), "done bar:\n{out}");
    }

    #[test]
    fn caption_bar_falls_back_to_ascii_glyphs_when_asked() {
        let runner = TourRunner::new(1.0);
        let out = bar(&runner, Duration::ZERO, 120, true);
        assert!(
            out.is_ascii(),
            "ascii mode leaked a non-ascii glyph:\n{out}"
        );
        assert!(out.contains("Tour 1/9"), "ascii bar:\n{out}");
        assert!(out.contains("p pause"), "ascii bar:\n{out}");
    }

    #[test]
    fn a_narrow_bar_keeps_the_step_and_the_keys_and_ellipsizes_the_caption() {
        let runner = TourRunner::new(1.0);
        let out = bar(&runner, Duration::ZERO, 80, false);
        assert!(out.contains("Tour 1/9"), "narrow bar:\n{out}");
        assert!(out.contains("Esc quit"), "narrow bar:\n{out}");
        assert!(out.contains('\u{2026}'), "caption not shortened:\n{out}");
    }
}
