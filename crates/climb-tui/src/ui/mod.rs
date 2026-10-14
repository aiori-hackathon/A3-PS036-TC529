//! The widget tree (phase 18.7): tab bar, tab bodies, status bar, help overlay, and
//! the too-small-screen guard. Overview, Signing and Verifying render real data
//! views; the remaining tabs are placeholders until later phases wire theirs.

pub mod amortization;
pub mod ladder;
pub mod memory;
pub mod overview;
pub mod signing;
pub mod verifying;
pub mod wire;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::data::LoadState;
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
    if app.tab == Tab::Ladder {
        ladder::render_ladder(frame, area, app, theme, glyphs);
        return;
    }
    match &app.data {
        LoadState::Loaded(view) if app.tab == Tab::Overview => {
            overview::render_overview(frame, area, app, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Signing => {
            signing::render_signing(frame, area, app, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Verifying => {
            verifying::render_verifying(frame, area, app, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Wire => {
            wire::render_wire(frame, area, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Memory => {
            memory::render_memory(frame, area, view, theme, glyphs);
        }
        LoadState::Missing { path } => render_load_error(
            frame,
            area,
            theme,
            glyphs,
            "results file not found",
            path.display().to_string(),
            None,
        ),
        LoadState::Invalid { path, error } => render_load_error(
            frame,
            area,
            theme,
            glyphs,
            "results file unreadable",
            path.display().to_string(),
            Some(error),
        ),
        LoadState::NotLoaded => render_load_error(
            frame,
            area,
            theme,
            glyphs,
            "results not loaded",
            String::new(),
            None,
        ),
        _ => render_placeholder(frame, area, app, theme, glyphs),
    }
}

/// The recovery screen for a missing, invalid, or not-yet-loaded results file.
/// Loading never panics (phase 19.1); the problem is explained here instead.
fn render_load_error(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    glyphs: &Glyphs,
    title: &str,
    detail: String,
    error: Option<&String>,
) {
    let key = |text: &'static str| Span::styled(text, Style::new().fg(theme.accent));
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {title}"),
            Style::new().fg(theme.bad).add_modifier(Modifier::BOLD),
        )),
    ];
    if !detail.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  results: "),
            Span::styled(detail, Style::new().fg(theme.fg)),
        ]));
    }
    if let Some(error) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::new().fg(theme.muted),
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::raw("  regenerate with: "),
            key("cargo xtask consolidate"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  press "),
            key("r"),
            Span::raw(" to reload after fixing"),
        ]),
        Line::from(""),
    ]);
    let body = Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    " Problem ",
                    Style::new().fg(theme.bad).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(body, area);
}

fn render_placeholder(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let key = |text: &'static str| Span::styled(text, Style::new().fg(theme.accent));
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {} — data view lands in a later phase", app.tab.title()),
            Style::new().fg(theme.muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  the headline numbers are on the Overview tab",
            Style::new().fg(theme.fg),
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
            key("[r]"),
            Span::raw(" reload   "),
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
        entry("r", "reload results from disk"),
        entry("← / →", "select dataset size"),
        entry("v", "toggle total / per message"),
        entry("↑ / ↓", "ladder: fewer / more leaves"),
        entry("Space", "ladder: toggle the signature banner"),
        entry("Enter", "ladder: auto-play the leaves"),
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
    use std::path::PathBuf;
    use std::time::Duration;

    use clap::Parser;
    use climb_report::results::{
        AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, EnvironmentFingerprint,
        MemoryResults, SigningBenchmarkResult, WireSizeResult,
    };
    use climb_report::{ClimbResults, DnsCaptureSource, DnsPipelineHeadline};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::cli::Cli;
    use crate::data::{LoadState, ReportHealth, ResultsView};
    use crate::theme::{ColorDepth, ThemeId};

    fn app_with(ascii: bool, theme: ThemeId, help: bool) -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.ascii = ascii;
        app.theme = theme;
        app.show_help = help;
        app
    }

    fn app_loaded_with(view: ResultsView) -> App {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.set_data(LoadState::Loaded(Box::new(view)));
        app
    }

    fn signing_row(size: &str, condition: &str, mean_ns: f64) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 1000,
            mean_ns,
            ci_lower_ns: mean_ns * 0.9,
            ci_upper_ns: mean_ns * 1.1,
            speedup: None,
        }
    }

    fn wire_row(reduction: f64) -> WireSizeResult {
        WireSizeResult {
            size: "large".to_string(),
            message_count: 1_000_000,
            ladder_len: 120,
            condensed_min: 100,
            condensed_mean: 100.0,
            condensed_max: 100,
            condensed_uniform: true,
            with_mtl_total_bytes: 1000,
            without_mtl_total_bytes: 100_000,
            reduction_percent: reduction,
        }
    }

    fn dns_headline(full: u64, condensed: u64) -> DnsPipelineHeadline {
        DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(full),
            condensed_proof_bytes_www_a: Some(condensed),
            reduction_percent_www_a: Some((full - condensed) as f64 / full as f64 * 100.0),
            notes: None,
            source: DnsCaptureSource::RecordedFixture,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        }
    }

    fn story_view(dns: Option<DnsPipelineHeadline>) -> ResultsView {
        ResultsView::new(
            ClimbResults {
                version: "1.0".to_string(),
                generated_at: "2026-10-12T00:00:00Z".to_string(),
                workspace_root: "/tmp/climb".to_string(),
                benchmark: BenchmarkResults {
                    signing: vec![
                        signing_row("Small", "With MTL", 100.0),
                        signing_row("Medium", "With MTL", 1000.0),
                        signing_row("Medium", "Without MTL", 40_000.0),
                    ],
                    verifying: Vec::new(),
                },
                wire_size: vec![wire_row(97.0)],
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
                    toolchain: serde_json::json!({ "rustc_version": "rustc 1.98.1 (x 2026-01-01)" }),
                    cpu: serde_json::json!({ "model_name": "Test CPU Model" }),
                    submodules: vec![
                        serde_json::json!({ "path": "third_party/liboqs", "sha": "0123456789abcdef" }),
                        serde_json::json!({ "path": "third_party/MTL", "sha": "abcdef0123456789" }),
                    ],
                    docker: serde_json::json!({}),
                    benchmark_runtime: serde_json::json!({ "cpu_governor": "powersave", "pin_cpu": 2 }),
                },
                dns_pipeline: dns,
                derived: DerivedSummary {
                    overall_wire_reduction_percent: None,
                    largest_wire_reduction_percent: None,
                    overall_verifying_trust_cached_speedup_avg: Some(388.162),
                    headline: None,
                },
                warnings: Vec::new(),
            },
            Some(Duration::from_secs(60)),
            ReportHealth::Agree,
        )
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
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Method.index()));
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("data view lands in a later phase"));
        assert!(out.contains("Method"));
    }

    #[test]
    fn wire_tab_renders_bars_and_gauge_at_120x40() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Wire.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Wire",
            "MTL amortised",
            "No amortisation",
            "Single-answer UDP fit",
            "normalised per size",
        ] {
            assert!(
                out.contains(needle),
                "wire tab misses {needle:?} at 120x40:\n{out}"
            );
        }
        assert!(
            !out.contains("data view lands in a later phase"),
            "Wire must render its view, not the placeholder:\n{out}"
        );
    }

    #[test]
    fn memory_tab_renders_at_80x24_and_120x40() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Memory.index()));
        let narrow = render_to_string(&app, 80, 24);
        assert!(narrow.contains("Memory"), "memory tab at 80x24:\n{narrow}");
        assert!(
            narrow.contains("Reading these numbers"),
            "explainer panel at 80x24:\n{narrow}"
        );
        let wide = render_to_string(&app, 120, 40);
        assert!(
            wide.contains("no rows recorded"),
            "empty memory rows show the placeholder at 120x40:\n{wide}"
        );
    }

    #[test]
    fn signing_tab_renders_bars_table_and_amortization_at_120x40() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Signing.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Signing",
            "total time",
            "MTL amortised",
            "No amortisation",
            "Speed-up",
            "40x",
            "Amortization",
            "messages",
        ] {
            assert!(
                out.contains(needle),
                "signing tab misses {needle:?} at 120x40:\n{out}"
            );
        }
        assert!(
            !out.contains("no signing rows recorded"),
            "amortization panel must show the chart, not the placeholder:\n{out}"
        );
    }

    #[test]
    fn verifying_tab_renders_bars_and_table_at_120x40() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Verifying.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Verifying",
            "total time",
            "Trust-cached",
            "Full ladder",
            "Without MTL",
            "no rows recorded",
        ] {
            assert!(
                out.contains(needle),
                "verifying tab misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn value_mode_v_toggles_the_signing_titles() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Signing.index()));
        let total = render_to_string(&app, 120, 40);
        assert!(total.contains("total time"), "total mode:\n{total}");
        app.update(crate::app::Action::ToggleValueMode);
        let per = render_to_string(&app, 120, 40);
        assert!(per.contains("per message"), "per-message mode:\n{per}");
        assert!(
            !per.contains("Tables \u{00b7} total time"),
            "table title must switch with v:\n{per}"
        );
    }

    #[test]
    fn signing_tab_renders_at_80x24_without_panic() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Signing.index()));
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Signing"), "signing tab at 80x24:\n{out}");
        assert!(
            out.contains("Amortization"),
            "amortization panel at 80x24:\n{out}"
        );
    }

    #[test]
    fn overview_renders_the_ten_second_story_at_80x24() {
        let app = app_loaded_with(story_view(Some(dns_headline(24_438, 658))));
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "CLIMB",
            "97.3%",
            "24,438 B",
            "40x",
            "Medium",
            "388x",
            "Test CPU Model",
        ] {
            assert!(
                out.contains(needle),
                "overview misses {needle:?} at 80x24:\n{out}"
            );
        }
    }

    #[test]
    fn overview_marks_none_sources_as_not_recorded() {
        let app = app_loaded_with(story_view(None));
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("not recorded"), "overview:\n{out}");
        assert!(
            out.contains('—'),
            "none value must show the em dash:\n{out}"
        );
    }

    #[test]
    fn empty_results_view_renders_without_panic() {
        let view = ResultsView::new(
            ClimbResults {
                version: "1.0".to_string(),
                generated_at: "2026-10-12T00:00:00Z".to_string(),
                workspace_root: "/tmp/climb".to_string(),
                benchmark: BenchmarkResults {
                    signing: Vec::new(),
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
            Some(Duration::from_secs(60)),
            ReportHealth::NoReport,
        );
        let app = app_loaded_with(view);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("not recorded"), "empty view:\n{out}");
    }

    #[test]
    fn overview_shows_environment_and_artifacts_at_120x40() {
        let app = app_loaded_with(story_view(Some(dns_headline(24_438, 658))));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Environment",
            "Artifacts",
            "Test CPU Model",
            "powersave",
            "cpu 2",
            "rustc 1.98.1",
            "abcdef0",
            "0123456",
            "SLH-DSA-SHA2-128s-MTL-SHA2-128",
            "fresh",
            "agree",
            "recorded 2026-10-12",
        ] {
            assert!(
                out.contains(needle),
                "panels miss {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn overview_artifact_panel_reports_a_stale_report() {
        let view = ResultsView::new(
            story_view(None).results().clone(),
            Some(Duration::from_secs(60)),
            ReportHealth::ReportStale,
        );
        let app = app_loaded_with(view);
        let out = render_to_string(&app, 120, 40);
        assert!(
            out.contains("report older than results"),
            "stale-report copy missing:\n{out}"
        );
    }

    #[test]
    fn missing_results_show_the_recovery_screen() {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.set_data(LoadState::Missing {
            path: PathBuf::from("target/climb-results.json"),
        });
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "results file not found",
            "cargo xtask consolidate",
            "reload after fixing",
        ] {
            assert!(
                out.contains(needle),
                "missing screen loses {needle:?}:\n{out}"
            );
        }
    }

    #[test]
    fn invalid_results_show_the_recovery_screen() {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.set_data(LoadState::Invalid {
            path: PathBuf::from("target/climb-results.json"),
            error: "expected value at line 1 column 2".to_string(),
        });
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "results file unreadable",
            "expected value at line 1 column 2",
            "cargo xtask consolidate",
        ] {
            assert!(
                out.contains(needle),
                "invalid screen loses {needle:?}:\n{out}"
            );
        }
    }

    #[test]
    fn reloading_after_a_missing_file_restores_the_overview() {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.set_data(LoadState::Missing {
            path: PathBuf::from("nope.json"),
        });
        let missing = render_to_string(&app, 80, 24);
        assert!(missing.contains("results file not found"));
        app.set_data(LoadState::Loaded(Box::new(story_view(Some(dns_headline(
            24_438, 658,
        ))))));
        let loaded = render_to_string(&app, 80, 24);
        assert!(loaded.contains("97.3%"));
        assert!(!loaded.contains("results file not found"));
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
