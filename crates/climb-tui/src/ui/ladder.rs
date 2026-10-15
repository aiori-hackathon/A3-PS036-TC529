//! The Ladder tab (phases 24.3, 24.4): the MTL ladder drawn as a tree, beside a
//! facts panel that names the ladder's shape and shows the model-vs-library
//! badge. Keys are handled in [`crate::app`]; this module only renders the
//! model. Every number here is derived from `climb-analytical` or the recorded
//! artifact — none is hardcoded.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};
use ratatui::Frame;

use climb_analytical::{auth_path_len, predicted_condensed_len, predicted_ladder_len};
use climb_report::charts::fmt_bytes;

use crate::app::App;
use crate::data::{ladder_badge, BadgeKind};
use crate::theme::{Glyphs, Theme};
use crate::widgets::ladder_view::{self, LadderView, Layout as LadderLayout};

/// Terminal width at or above which the tree and facts panel share one row.
const WIDE_WIDTH: u16 = 100;
/// Height reserved for the facts panel when the regions stack vertically.
const FACTS_STACK_HEIGHT: u16 = 13;

/// Render the Ladder tab into `area`.
pub fn render_ladder(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let layout = ladder_view::build(app.ladder_n(), app.ladder_leaf());
    let (tree_area, facts_area) = regions(area);

    frame.render_widget(
        LadderView {
            layout: &layout,
            theme,
            glyphs,
        },
        tree_area,
    );
    render_facts(frame, facts_area, app, &layout, theme, glyphs);
}

/// Place the tree and the facts panel: side by side when wide, stacked when
/// narrow.
fn regions(area: Rect) -> (Rect, Rect) {
    if area.width >= WIDE_WIDTH {
        let [tree, facts] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
                .areas(area);
        (tree, facts)
    } else {
        let facts_height = FACTS_STACK_HEIGHT.min(area.height);
        let [tree, facts] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(facts_height)]).areas(area);
        (tree, facts)
    }
}

fn render_facts(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    layout: &LadderLayout,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let n = layout.leaf_count;
    let leaf = layout.selected;
    let path = auth_path_len(n, leaf).unwrap_or(0);
    let condensed = predicted_condensed_len(n, leaf).unwrap_or(0);
    let ladder = predicted_ladder_len(n);

    let rungs: String = layout
        .rung_spans
        .iter()
        .map(|(left, right)| format!("[{left}..{right}]"))
        .collect::<Vec<_>>()
        .join(" ");
    let sep = if theme.ascii { "-" } else { "·" };

    let label = |text: String| -> Line<'static> {
        Line::from(Span::styled(text, Style::new().fg(theme.fg)))
    };
    let mut lines = vec![
        label(format!("N = {n} leaves (messages)")),
        label(format!("rungs: {rungs}")),
        label(format!(
            "selected leaf {leaf} {sep} auth path {path} siblings"
        )),
        label(format!(
            "condensed signature: {}",
            fmt_bytes(condensed as f64)
        )),
        label(format!("ladder buffer: {}", fmt_bytes(ladder as f64))),
    ];

    let badge = ladder_badge(app.ladder(), n, leaf, theme.ascii);
    let badge_style = match badge.kind {
        BadgeKind::Match => Style::new().fg(theme.ok),
        BadgeKind::Mismatch => Style::new().fg(theme.bad),
        BadgeKind::ModelOnly | BadgeKind::NoArtifact => Style::new().fg(theme.muted),
    };
    lines.push(Line::from(Span::styled(
        badge.text.clone(),
        badge_style.add_modifier(Modifier::BOLD),
    )));

    if app.ladder_banner() {
        let measured = app
            .ladder()
            .verification()
            .and_then(|document| document.row(n as usize))
            .map_or(ladder, |row| row.ladder_buffer_len);
        lines.push(Line::from(Span::styled(
            format!(
                "\u{2605} one big post-quantum signature covers the whole ladder ({})",
                fmt_bytes(measured as f64)
            ),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "The big signature is paid once; each message carries only its own short path.",
        Style::new().fg(theme.muted),
    )));

    if theme.ascii {
        lines.push(Line::from(Span::styled(
            "keys: up/down size  left/right leaf  space banner  enter autoplay",
            Style::new().fg(theme.accent2),
        )));
    }

    let title = if app.ladder_autoplay() {
        " Ladder facts \u{00b7} autoplay "
    } else {
        " Ladder facts "
    };
    // The left padding is what indents wrapped continuation lines: ratatui's
    // word wrapper keeps the block's inner column, whereas a literal leading
    // space on each line would be lost the moment that line wraps.
    let facts = Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .padding(Padding::left(1))
                .title(Span::styled(
                    title,
                    Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(facts, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use climb_report::LadderVerification;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::cli::Cli;
    use crate::data::{LadderLoadState, LadderSource};
    use crate::theme::ColorDepth;

    fn app() -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        App::from_cli(&cli, ColorDepth::TrueColor)
    }

    fn loaded_document() -> LadderLoadState {
        // Read the committed fixture, not `target/`: the badge fallback must be
        // exercisable on a fresh clone with no local run artifacts.
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/ladder-verification.recorded.json"
        ));
        let document: LadderVerification =
            serde_json::from_str(json).expect("the recorded artifact parses");
        LadderLoadState::Loaded {
            document: Box::new(document),
            source: LadderSource::RecordedFixture,
        }
    }

    /// The facts-panel interior rows, left/right borders removed, so indent is
    /// observable independently of the surrounding tree panel.
    fn facts_lines(app: &App, width: u16, height: u16) -> Vec<String> {
        let theme = app.theme();
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render_ladder(frame, frame.area(), app, &theme, &glyphs))
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
        let (_, facts) = regions(Rect::new(0, 0, width, height));
        let mut lines = Vec::new();
        for y in (facts.y + 1)..facts.bottom().saturating_sub(1) {
            let mut line = String::new();
            for x in (facts.x + 1)..facts.right().saturating_sub(1) {
                if let Some(cell) = buffer.cell((x, y)) {
                    line.push_str(cell.symbol());
                }
            }
            lines.push(line);
        }
        lines
    }

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let theme = app.theme();
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render_ladder(frame, frame.area(), app, &theme, &glyphs))
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if let Some(cell) = buffer.cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn regions_split_side_by_side_when_wide() {
        let (tree, facts) = regions(Rect::new(0, 0, 120, 40));
        assert_eq!(tree.y, facts.y, "wide layout shares a row");
        assert!(facts.x > tree.x);
        assert_eq!(tree.height, 40);
    }

    #[test]
    fn regions_stack_vertically_when_narrow() {
        let (tree, facts) = regions(Rect::new(0, 0, 80, 20));
        assert_eq!(tree.x, facts.x, "narrow layout shares a column");
        assert!(facts.y > tree.y);
        assert!(tree.height > 0 && facts.height > 0);
    }

    #[test]
    fn renders_the_n_thirteen_shape_at_120x40() {
        let mut app = app();
        app.ladder_n = 13;
        app.ladder_leaf = 5;
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Ladder geometry",
            "Ladder facts",
            "N = 13 leaves",
            "[0..7]",
            "selected leaf 5",
            "auth path 3 siblings",
            "The big signature is paid once",
        ] {
            assert!(
                out.contains(needle),
                "ladder tab misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn renders_without_an_artifact_as_model_only() {
        let mut app = app();
        app.ladder_n = 8;
        app.ladder_leaf = 0;
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("no verification artifact"), "badge:\n{out}");
        assert!(out.contains("N = 8 leaves"), "facts:\n{out}");
    }

    #[test]
    fn renders_the_match_badge_from_the_recorded_artifact() {
        let mut app = app();
        app.set_ladder(loaded_document());
        app.ladder_n = 13;
        app.ladder_leaf = 5;
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("matches libMTL"), "match badge:\n{out}");
        assert!(
            out.contains("(recorded fixture 2026-10-14)"),
            "recorded date:\n{out}"
        );
    }

    #[test]
    fn banner_appears_only_when_enabled() {
        let mut app = app();
        app.ladder_n = 8;
        let off = render_to_string(&app, 80, 24);
        assert!(
            !off.contains("covers the whole ladder"),
            "banner off:\n{off}"
        );
        app.ladder_banner = true;
        let on = render_to_string(&app, 80, 24);
        assert!(on.contains("covers the whole ladder"), "banner on:\n{on}");
    }

    #[test]
    fn ascii_mode_renders_the_text_tree_and_key_hint() {
        let mut app = app();
        app.ascii = true;
        app.ladder_n = 13;
        app.ladder_leaf = 5;
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("space banner"), "key hint:\n{out}");
        assert!(!out.contains('●'), "ascii must stay ascii:\n{out}");
    }

    #[test]
    fn wrapped_facts_lines_keep_the_left_indent_at_100x30() {
        let mut app = app();
        app.set_ladder(loaded_document());
        app.ladder_n = 13;
        app.ladder_leaf = 5;
        let lines = facts_lines(&app, 100, 30);

        // The selected-leaf and signature-note lines both wrap at this width.
        assert!(
            lines
                .iter()
                .any(|l| l.trim_start().starts_with("selected leaf 5")),
            "first wrapped segment missing:\n{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l.trim() == "siblings"),
            "continuation segment missing:\n{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.trim_start().starts_with("each message carries")),
            "note continuation missing:\n{lines:#?}"
        );

        // Every continuation line must keep the one-space left indent.
        for line in lines.iter().filter(|l| !l.trim().is_empty()) {
            assert!(
                line.starts_with(' '),
                "facts line lost its left indent: {line:?}"
            );
        }
    }

    #[test]
    fn facts_lines_keep_the_left_indent_at_80x24() {
        let mut app = app();
        app.set_ladder(loaded_document());
        app.ladder_n = 13;
        app.ladder_leaf = 5;
        let lines = facts_lines(&app, 80, 24);

        assert!(
            lines
                .iter()
                .any(|l| l.trim_start().starts_with("N = 13 leaves")),
            "facts panel missing:\n{lines:#?}"
        );
        for line in lines.iter().filter(|l| !l.trim().is_empty()) {
            assert!(
                line.starts_with(' '),
                "facts line lost its left indent at 80x24: {line:?}"
            );
        }
    }
}
