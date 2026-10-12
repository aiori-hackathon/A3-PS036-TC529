//! The widget tree (phase 18.7): tab bar, placeholder body, status bar, help overlay, and
//! the too-small-screen guard. Every tab renders a placeholder until result data is wired.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::theme::{Glyphs, Theme};

/// The minimum terminal size the dashboard is designed for.
pub const REQUIRED_WIDTH: u16 = 80;
/// The minimum terminal size the dashboard is designed for.
pub const REQUIRED_HEIGHT: u16 = 24;

/// Render the whole UI for one frame.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = app.theme();
    let glyphs = theme.glyphs();

    if area.width < REQUIRED_WIDTH || area.height < REQUIRED_HEIGHT {
        render_too_small(frame, area, &theme);
        return;
    }

    let [tabs_area, body_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    render_tabs(frame, tabs_area, app, &theme, &glyphs);
    render_body(frame, body_area, app, &theme, &glyphs);
    render_status(frame, status_area, app, &theme, &glyphs);

    if app.show_help {
        render_help(frame, area, &theme, &glyphs);
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let titles: Vec<Line> = Tab::ALL.iter().map(|tab| Line::from(tab.title())).collect();
    let tabs = Tabs::new(titles)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    " CLIMB ",
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                )),
        )
        .select(app.tab.index())
        .divider(Span::styled(
            glyphs.rail.to_string(),
            Style::new().fg(theme.border),
        ))
        .padding("", "")
        .highlight_style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let key = |text: &'static str| Span::styled(text, Style::new().fg(theme.accent));
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "  {} — no result data yet (phase 18 skeleton)",
                app.tab.title()
            ),
            Style::new().fg(theme.muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  switch tabs: "),
            key("[Tab]/[Shift-Tab]"),
            Span::raw(" or "),
            key("[1-9]"),
        ]),
        Line::from(vec![
            Span::raw("  keys: "),
            key("[?]"),
            Span::raw(" help   "),
            key("[T]"),
            Span::raw(" theme   "),
            key("[a]"),
            Span::raw(" ascii   "),
            key("[q]"),
            Span::raw(" quit"),
        ]),
    ];
    let body = Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    format!(" {} ", app.tab.title()),
                    Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(body, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let spinner = glyphs.spinner[app.tick % glyphs.spinner.len()];
    let mode = if app.replay { "replay" } else { "live" };
    let tour = if app.tour { "on" } else { "off" };
    let ascii = if app.ascii { "on" } else { "off" };
    let line = Line::from(vec![
        Span::styled(format!(" {spinner} "), Style::new().fg(theme.accent)),
        Span::styled(
            format!("theme: {} ", app.theme.label()),
            Style::new().fg(theme.fg),
        ),
        Span::styled(
            format!("({}) ", app.color_depth.label()),
            Style::new().fg(theme.muted),
        ),
        Span::styled(format!("ascii: {ascii} "), Style::new().fg(theme.fg)),
        Span::styled(format!("mode: {mode} "), Style::new().fg(theme.fg)),
        Span::styled(format!("tour: {tour} "), Style::new().fg(theme.fg)),
        Span::styled(
            format!("source: {} ", app.results_path.display()),
            Style::new().fg(theme.muted),
        ),
        Span::styled("· ? help", Style::new().fg(theme.accent)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::new().fg(theme.fg).bg(theme.bg)),
        area,
    );
}

fn render_help(frame: &mut Frame, area: Rect, theme: &Theme, glyphs: &Glyphs) {
    let popup = centered_rect(64, 72, area);
    frame.render_widget(Clear, popup);

    let entry = |keys: &'static str, what: &'static str| {
        Line::from(vec![
            Span::styled(format!("  {keys:<18}"), Style::new().fg(theme.accent)),
            Span::styled(what, Style::new().fg(theme.fg)),
        ])
    };

    let lines = vec![
        Line::from(""),
        entry("Tab / Shift-Tab", "next / previous tab"),
        entry("1 … 9", "jump to a tab by number"),
        entry("?", "toggle this help overlay"),
        entry("T", "cycle theme: dark → light → contrast"),
        entry("a", "toggle ASCII glyphs"),
        entry("q / Esc / Ctrl-C", "quit"),
        Line::from(""),
        Line::from(vec![
            Span::raw("  legend: "),
            Span::styled("measured ", theme.measured),
            Span::styled("extrapolated ", theme.extrapolated),
            Span::styled("analytical", theme.analytical),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("  press ? to close   ·   requires {REQUIRED_WIDTH}x{REQUIRED_HEIGHT}",),
            Style::new().fg(theme.muted),
        )),
        Line::from(""),
    ];

    let help = Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.accent))
                .title(Span::styled(
                    " Help — key bindings ",
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(help, popup);
}

fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "terminal too small",
            Style::new().fg(theme.bad).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("required: {REQUIRED_WIDTH}x{REQUIRED_HEIGHT}")),
        Line::from(format!("current:  {}x{}", area.width, area.height)),
        Line::from(""),
        Line::from(Span::styled(
            "resize the window, or press q to quit",
            Style::new().fg(theme.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(middle);
    center
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::cli::Cli;
    use crate::theme::{ColorDepth, ThemeId};

    fn app_with(ascii: bool, theme: ThemeId, help: bool) -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.ascii = ascii;
        app.theme = theme;
        app.show_help = help;
        app
    }

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render(frame, app))
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
    fn renders_every_tab_title_at_80x24() {
        let app = app_with(false, ThemeId::Dark, false);
        let out = render_to_string(&app, 80, 24);
        for tab in Tab::ALL {
            assert!(
                out.contains(tab.title()),
                "tab title {:?} missing at 80x24:\n{out}",
                tab.title()
            );
        }
    }

    #[test]
    fn renders_status_bar_fields_at_120x40() {
        let app = app_with(false, ThemeId::Light, false);
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("theme: light"), "status bar:\n{out}");
        assert!(out.contains("ascii: off"));
        assert!(out.contains("source: target/climb-results.json"));
        assert!(out.contains("? help"));
    }

    #[test]
    fn renders_help_overlay_listing_every_key() {
        let app = app_with(false, ThemeId::Dark, true);
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("Help"), "help overlay:\n{out}");
        for needle in ["Shift-Tab", "cycle theme", "ASCII", "Ctrl-C", "quit"] {
            assert!(out.contains(needle), "help missing {needle:?}:\n{out}");
        }
    }

    #[test]
    fn renders_placeholder_body_naming_the_tab() {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.update(crate::app::Action::SelectTab(1));
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("no result data yet"));
        assert!(out.contains("Signing"));
    }

    #[test]
    fn renders_at_200x60() {
        let app = app_with(true, ThemeId::Contrast, false);
        let out = render_to_string(&app, 200, 60);
        assert!(out.contains("Overview"));
        assert!(out.contains("Method"));
    }

    #[test]
    fn too_small_screen_at_60x20_states_required_size() {
        let app = app_with(false, ThemeId::Dark, false);
        let out = render_to_string(&app, 60, 20);
        assert!(out.contains("80x24"), "too-small screen:\n{out}");
        assert!(out.contains("60x20"));
        assert!(
            !out.contains("Overview"),
            "tabs must not render when too small"
        );
    }

    #[test]
    fn ascii_glyphs_render_an_ascii_border() {
        let app = app_with(true, ThemeId::Dark, false);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains('+'), "ascii border expected:\n{out}");
        assert!(!out.contains('┌'));
    }

    #[test]
    fn resize_from_120x40_to_100x30_does_not_panic() {
        let app = app_with(false, ThemeId::Dark, false);
        let large = render_to_string(&app, 120, 40);
        let smaller = render_to_string(&app, 100, 30);
        assert!(large.contains("Overview"));
        assert!(smaller.contains("Overview"));
    }
}
