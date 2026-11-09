//! The widget tree: tab bar, tab bodies, the guided-tour caption
//! bar, status bar, help overlay, and the too-small-screen guard. Every tab —
//! Overview, Signing, Verifying, Wire, Memory, Ladder, Live, Integrity and
//! Method — renders a real data view.

pub mod amortization;
pub mod integrity;
pub mod ladder;
pub mod live;
pub mod memory;
pub mod method;
pub mod overview;
pub mod signing;
pub mod tour;
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
        if app.ascii {
            sanitize_ascii(frame);
        }
        return;
    }

    // The tour caption bar takes one row between the body and the status bar
    // while the guided tour runs.
    let mut constraints = vec![Constraint::Length(3), Constraint::Min(0)];
    if app.tour_active() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let chunks = Layout::vertical(constraints).split(area);
    let tabs_area = chunks.first().copied().unwrap_or(area);
    let body_area = chunks.get(1).copied().unwrap_or(area);
    let status_area = chunks.last().copied().unwrap_or(area);

    render_tabs(frame, tabs_area, app, &theme, &glyphs);
    render_body(frame, body_area, app, &theme, &glyphs);
    if let (Some(runner), Some(&tour_area)) = (app.tour(), chunks.get(2)) {
        tour::render_tour_bar(frame, tour_area, runner, app.clock().now(), &theme);
    }
    render_status(frame, status_area, app, &theme, &glyphs);

    if app.show_help {
        render_help(frame, area, &theme, &glyphs);
    }

    // A single final pass guarantees the promise `--ascii` makes: no frame,
    // on any tab, contains a glyph outside ASCII. Doing it here means a view
    // model that introduces a new symbol cannot silently break ASCII mode.
    if app.ascii {
        sanitize_ascii(frame);
    }
}

/// Replace every non-ASCII cell symbol with a close ASCII equivalent.
fn sanitize_ascii(frame: &mut Frame) {
    let area = frame.area();
    let buffer = frame.buffer_mut();
    for y in 0..area.height {
        for x in 0..area.width {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            let replacement = {
                let symbol = cell.symbol();
                (!symbol.is_ascii()).then(|| ascii_symbol(symbol))
            };
            if let Some(replacement) = replacement {
                cell.set_symbol(replacement);
            }
        }
    }
}

/// The ASCII stand-in for one rendered symbol. Unknown symbols become `?`
/// rather than disappearing, so a missing mapping is visible in a test frame.
fn ascii_symbol(symbol: &str) -> &'static str {
    match symbol.chars().next() {
        Some('─' | '━') => "-",
        Some('│' | '┃') => "|",
        Some('┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╯' | '╰') => {
            "+"
        }
        Some('█' | '▓' | '▒' | '░') => "#",
        Some('╫') => "=",
        Some('▶' | '▸' | '►' | '→') => ">",
        Some('◀' | '◂' | '←') => "<",
        Some('▲' | '↑') => "^",
        Some('▼' | '↓') => "v",
        Some('…') => ".",
        Some('·' | '•' | '∙') => ".",
        Some('≈') => "~",
        Some('—' | '–') => "-",
        Some('✔') => "+",
        Some('✖' | '×' | '✗') => "x",
        Some('µ') => "u",
        Some('°') => "o",
        Some('±') => "+",
        Some('≥') => ">",
        Some('≤') => "<",
        Some('●' | '◆' | '◇' | '○') => "*",
        Some('⇄' | '⇆') => "<",
        Some('’' | '‘') => "'",
        Some('“' | '”') => "\"",
        Some('§') => "S",
        Some('⏎') => "\\",
        Some(_) => "?",
        None => "?",
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
    if app.tab == Tab::Live {
        live::render_live(frame, area, app, theme, glyphs);
        return;
    }
    if app.tab == Tab::Method {
        method::render_method(frame, area, app, theme, glyphs);
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
            wire::render_wire(frame, area, app, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Memory => {
            memory::render_memory(frame, area, view, theme, glyphs);
        }
        LoadState::Loaded(view) if app.tab == Tab::Integrity => {
            integrity::render_integrity(frame, area, app, view, theme, glyphs);
        }
        LoadState::Missing { path } => render_load_error(
            frame,
            area,
            theme,
            glyphs,
            "results file not found",
            crate::paths::display(path),
            None,
        ),
        LoadState::Invalid { path, error } => render_load_error(
            frame,
            area,
            theme,
            glyphs,
            "results file unreadable",
            crate::paths::display(path),
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
        // Every tab is dispatched above; this arm only exists because the match
        // must be exhaustive.
        _ => {}
    }
}

/// The recovery screen for a missing, invalid, or not-yet-loaded results file.
/// Loading never panics; the problem is explained here instead.
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

fn render_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let spinner = glyphs
        .spinner
        .get(app.tick % glyphs.spinner.len())
        .copied()
        .unwrap_or(' ');
    let mode = if app.replay { "replay" } else { "live" };
    let tour = if app.tour_active() { "on" } else { "off" };
    let ascii = if app.ascii { "on" } else { "off" };
    // Everything except the tail is fixed width, so the tail (the source path,
    // or a status message when one exists) is shortened with a marker until
    // the trailing help key hint stays visible even at the minimum width.
    let fixed: usize = format!(" {spinner} ").chars().count()
        + format!("theme: {} ", app.theme.label()).chars().count()
        + format!("({}) ", app.color_depth.label()).chars().count()
        + format!("ascii: {ascii} ").chars().count()
        + format!("mode: {mode} ").chars().count()
        + format!("tour: {tour} ").chars().count()
        + " ".len()
        + "· ? help".len();
    let tail_budget = (area.width as usize).saturating_sub(fixed);
    let (tail, tail_style) = match &app.status {
        Some(status) => {
            let (marker, colour) = match status.tone {
                crate::app::StatusTone::Info => (glyphs.info_marker, theme.accent),
                crate::app::StatusTone::Warn => (glyphs.warn_marker, theme.warn),
                crate::app::StatusTone::Bad => (glyphs.bad_marker, theme.bad),
            };
            let text = crate::widgets::ellipsize(
                &format!("{marker} {}", status.text),
                tail_budget,
                theme.ascii,
            );
            (text, colour)
        }
        None => {
            let text = crate::widgets::ellipsize(
                &format!("source: {}", app.source.label()),
                tail_budget,
                theme.ascii,
            );
            (text, theme.muted)
        }
    };
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
        Span::styled(format!("{tail} "), Style::new().fg(tail_style)),
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
        entry(
            "↑ / ↓",
            "ladder: leaves · integrity: scroll · method: scroll",
        ),
        entry("Space", "ladder: toggle the signature banner"),
        entry("Enter", "ladder: auto-play the leaves"),
        entry("Space / Backspace", "live: next / previous step"),
        entry("Enter", "live: auto-play the replay"),
        entry("t", "live: log / linear byte scale"),
        entry("p / n / Esc", "tour: pause · next step · quit tour"),
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
        AnalyticalMemoryModel, BenchmarkResults, ConfigComparison, DerivedSummary,
        EnvironmentFingerprint, EvidenceClass, MemoryResults, ReuseCurvePoint, ReuseResult,
        SigningBenchmarkResult, WireSizeResult,
    };
    use climb_report::{ClimbResults, DnsCaptureSource, DnsPipelineHeadline};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::cli::Cli;
    use crate::data::test_support::{empty_derived, view_with};
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
            plain_total_bytes: 0,
            plain_signature_len_bytes: 0,
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
                ladder: None,
                plain: Vec::new(),
                classical: Vec::new(),
                comparisons: vec![ConfigComparison {
                    size: "Medium".to_string(),
                    wire_reduction_c_vs_a_percent: Some(96.5),
                    wire_reduction_c_vs_b_percent: Some(96.7),
                    signing_speedup_c_vs_a: Some(9_405.206),
                    signing_speedup_c_vs_b: Some(9_433.089),
                    verifying_speedup_cached_vs_a: Some(215.311),
                    verifying_speedup_cold_vs_a: Some(0.954),
                    class: EvidenceClass::Derived,
                }],
                derived: DerivedSummary {
                    overall_wire_reduction_percent: None,
                    largest_wire_reduction_percent: None,
                    overall_verifying_trust_cached_speedup_avg: Some(388.162),
                    headline: None,
                    reuse: Vec::new(),
                    udp_fit: None,
                    signature_context: Vec::new(),
                    stability: None,
                    primitives: None,
                    bound_ok: Some(true),
                    baseline_ok: Some(true),
                    bound_offenders: Vec::new(),
                    baseline_offenders: Vec::new(),
                },
                reproduction_mode: climb_report::ReproductionMode::Unknown,
                warnings: Vec::new(),
            },
            Some(Duration::from_secs(60)),
            ReportHealth::Agree,
        )
    }

    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
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

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render(frame, app))
            .expect("rendering never fails on TestBackend");
        buffer_to_string(terminal.backend().buffer())
    }

    /// A stable digest of every cell's symbol and style. Text snapshots alone
    /// cannot see a theme change; this catches a palette or modifier drift.
    fn style_digest(buffer: &ratatui::buffer::Buffer) -> u64 {
        // FNV-1a over the symbol bytes and the resolved style fields.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let Some(cell) = buffer.cell((x, y)) else {
                    continue;
                };
                mix(cell.symbol().as_bytes());
                mix(format!(
                    "{:?}|{:?}|{:?}|{:?}",
                    cell.fg,
                    cell.bg,
                    cell.underline_color,
                    cell.modifier.bits()
                )
                .as_bytes());
            }
        }
        hash
    }

    fn sample_output_path(name: &str) -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output")
            .join(name)
    }

    /// The committed sample artifacts, loaded through the TUI's own loaders,
    /// so a snapshot exercises the same path a user does. The results-file age
    /// is pinned and the clock is fake, so the golden frame does not drift as
    /// the checkout gets older or the test run gets slower.
    fn sample_app_with(theme: ThemeId) -> App {
        let results_path = sample_output_path("climb-results.json");
        let mut app = app_with(false, theme, false);
        app.set_clock(crate::tour::Clock::fake());
        let state = match crate::data::load(&results_path) {
            LoadState::Loaded(view) => LoadState::Loaded(Box::new(ResultsView::new(
                view.results().clone(),
                Some(std::time::Duration::from_secs(60)),
                view.report_health(),
            ))),
            other => other,
        };
        app.set_data(state);
        app.set_live(crate::data::load_live(&sample_output_path(
            "dns-capture.recorded.json",
        )));
        app.set_ladder(crate::data::load_ladder(&sample_output_path(
            "ladder-verification.recorded.json",
        )));
        app
    }

    fn sample_app() -> App {
        sample_app_with(ThemeId::Dark)
    }

    /// The four sizes the dashboard is checked at, from the minimum to a large
    /// terminal.
    const SNAPSHOT_SIZES: [(u16, u16); 4] = [(80, 24), (100, 30), (120, 40), (200, 60)];

    fn snapshot_path(width: u16, height: u16) -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/ui/snapshots")
            .join(format!("frames-{width}x{height}.txt"))
    }

    /// Render one tab and return its text frame and style digest.
    fn tab_frame(theme: ThemeId, tab: Tab, width: u16, height: u16) -> (String, u64) {
        let mut app = sample_app_with(theme);
        app.tab = tab;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
        (buffer_to_string(buffer), style_digest(buffer))
    }

    /// The status bar names the active theme, so that one label is the only
    /// text that legitimately differs between themes. Normalising it lets the
    /// committed text frame be shared while a separate test still checks that
    /// the label tracks the theme.
    fn normalize_theme_label(text: &str) -> String {
        let mut out = text.to_string();
        for theme in ThemeId::ALL {
            out = out.replace(&format!("theme: {} ", theme.label()), "theme: <theme> ");
        }
        out
    }

    /// Build one size's snapshot: the text frame once per tab (it is the same
    /// for every theme, which the test also asserts) plus a style digest per
    /// theme. Set `CLIMB_UPDATE_SNAPSHOTS=1` to rewrite the committed copies.
    fn frame_snapshot(width: u16, height: u16) -> String {
        let mut out = format!("size: {width}x{height}\n");
        for tab in Tab::ALL {
            let (text, _) = tab_frame(ThemeId::Dark, tab, width, height);
            out.push_str(&format!("\n### {}\n", tab.title()));
            // Trailing spaces carry no information in a text snapshot; the
            // style digest still covers the full cell grid.
            for line in normalize_theme_label(&text).lines() {
                out.push_str(line.trim_end());
                out.push('\n');
            }
            for theme in ThemeId::ALL {
                let (_, digest) = tab_frame(theme, tab, width, height);
                out.push_str(&format!("style-digest {}: {digest:016x}\n", theme.label()));
            }
        }
        out
    }

    /// Every tab, every theme, at every supported size. The text frame catches
    /// layout drift and the style digest catches a theme or colour-depth drift.
    #[test]
    fn every_tab_and_theme_matches_committed_snapshots() {
        let update = std::env::var_os("CLIMB_UPDATE_SNAPSHOTS").is_some();
        for (width, height) in SNAPSHOT_SIZES {
            // The text is a property of the layout, not the palette: every
            // theme must produce exactly the same frame. A theme that changed
            // the text (for example by choosing different glyphs) would make
            // the single committed text copy a lie.
            for tab in Tab::ALL {
                let (baseline, _) = tab_frame(ThemeId::Dark, tab, width, height);
                // The status bar is the last row and legitimately differs:
                // the theme label changes width, which changes how much of the
                // source path fits. Compare the body only.
                let body = |text: &str| {
                    let lines: Vec<&str> = text.lines().collect();
                    lines[..lines.len().saturating_sub(1)].join("\n")
                };
                let baseline_body = body(&baseline);
                for theme in ThemeId::ALL {
                    let (text, _) = tab_frame(theme, tab, width, height);
                    assert_eq!(
                        body(&text),
                        baseline_body,
                        "{} at {width}x{height} changes its text under {theme:?}",
                        tab.title()
                    );
                }
            }
            let out = frame_snapshot(width, height);
            let path = snapshot_path(width, height);
            if update {
                std::fs::create_dir_all(path.parent().expect("snapshot dir"))
                    .expect("create snapshot dir");
                std::fs::write(&path, &out).expect("write snapshot");
                continue;
            }
            let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "missing snapshot {} ({error}); run with CLIMB_UPDATE_SNAPSHOTS=1",
                    path.display()
                )
            });
            assert_eq!(
                out,
                expected,
                "rendered frames drifted from {}",
                path.display()
            );
        }
    }

    #[test]
    fn no_tab_renders_a_non_ascii_glyph_in_ascii_mode() {
        let mut app = sample_app();
        app.ascii = true;
        for tab in Tab::ALL {
            app.tab = tab;
            for (width, height) in SNAPSHOT_SIZES {
                let out = render_to_string(&app, width, height);
                assert!(
                    out.is_ascii(),
                    "{} at {width}x{height} contains a non-ASCII glyph in ASCII mode:\n{out}",
                    tab.title()
                );
            }
        }
    }

    #[test]
    fn a_resize_storm_ends_on_the_final_size_frame() {
        let mut app = sample_app();
        app.tab = Tab::Overview;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("in-memory terminal");
        let storm = [
            (100u16, 30u16),
            (200, 60),
            (60, 20),
            (80, 24),
            (120, 40),
            (90, 25),
            (100, 30),
        ];
        for (width, height) in storm {
            terminal.backend_mut().resize(width, height);
            terminal
                .draw(|frame| render(frame, &app))
                .expect("a resize frame renders");
        }
        let (width, height) = *storm.last().expect("the storm has a final size");
        assert_eq!(
            buffer_to_string(terminal.backend().buffer()),
            render_to_string(&app, width, height),
            "the last frame must match a fresh {width}x{height} frame"
        );
    }

    #[test]
    fn a_status_message_always_carries_a_non_colour_marker() {
        let mut app = sample_app();
        for (tone, marker) in [
            (crate::app::StatusTone::Info, "reloading"),
            (crate::app::StatusTone::Warn, "careful"),
            (crate::app::StatusTone::Bad, "failed"),
        ] {
            let status = match tone {
                crate::app::StatusTone::Info => crate::app::StatusLine::info(marker),
                crate::app::StatusTone::Warn => crate::app::StatusLine::warn(marker),
                crate::app::StatusTone::Bad => crate::app::StatusLine::bad(marker),
            };
            app.set_status(Some(status));
            let out = render_to_string(&app, 120, 40);
            let line = out.lines().last().expect("the status bar is the last row");
            assert!(line.contains(marker), "status text missing:\n{line}");
            assert!(
                line.contains('\u{2022}')
                    || line.contains('\u{2716}')
                    || line.contains('!')
                    || line.contains("[i]")
                    || line.contains("[x]")
                    || line.contains("[!]"),
                "status line has no non-colour marker:\n{line}"
            );
        }
    }

    /// No frame, at either supported terminal size, may print an absolute path
    /// or the developer's username.
    #[test]
    fn no_rendered_frame_leaks_an_absolute_path_or_username() {
        let mut app = sample_app();
        for tab in Tab::ALL {
            app.tab = tab;
            for (width, height) in [(80u16, 24u16), (120, 40)] {
                let out = render_to_string(&app, width, height);
                for needle in ["/home/", "/Users/", "pratyay"] {
                    assert!(
                        !out.contains(needle),
                        "{} at {width}x{height} leaks {needle:?}:\n{out}",
                        tab.title()
                    );
                }
            }
        }
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
    fn the_status_bar_names_the_active_theme() {
        for theme in ThemeId::ALL {
            let app = app_with(false, theme, false);
            let out = render_to_string(&app, 120, 40);
            assert!(
                out.contains(&format!("theme: {} ", theme.label())),
                "the status bar must name {theme:?}:\n{out}"
            );
        }
    }

    #[test]
    fn renders_status_bar_fields_at_120x40() {
        let app = app_with(false, ThemeId::Light, false);
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("theme: light"), "status bar:\n{out}");
        assert!(out.contains("ascii: off"));
        assert!(out.contains("source: [explicit] target/climb-results.json"));
        assert!(out.contains("? help"));
    }

    #[test]
    fn status_bar_keeps_help_and_ellipsizes_the_path_at_80x24() {
        let app = app_with(false, ThemeId::Dark, false);
        let out = render_to_string(&app, 80, 24);
        let status = out.lines().last().expect("the status bar is the last row");
        assert!(status.contains("source: "), "status bar:\n{status}");
        assert!(status.contains("\u{2026}"), "path not shortened:\n{status}");
        assert!(
            !status.contains("target/climb-results.json"),
            "full path does not fit at 80 columns:\n{status}"
        );
        assert!(status.contains("? help"), "help clipped:\n{status}");
    }

    #[test]
    fn renders_help_overlay_listing_every_key() {
        let app = app_with(false, ThemeId::Dark, true);
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("Help"), "help overlay:\n{out}");
        for needle in [
            "Shift-Tab",
            "cycle theme",
            "ASCII",
            "Ctrl-C",
            "quit",
            "live: next / previous step",
            "live: auto-play the replay",
            "live: log / linear byte scale",
            "ladder: leaves",
            "integrity: scroll",
            "method: scroll",
            "tour: pause",
        ] {
            assert!(out.contains(needle), "help missing {needle:?}:\n{out}");
        }
    }

    #[test]
    fn tour_caption_bar_sits_between_the_body_and_the_status_bar() {
        let mut app = app_with(false, ThemeId::Dark, false);
        app.set_tour(Some(crate::tour::TourRunner::new(1.0)));
        let out = render_to_string(&app, 120, 40);
        let lines: Vec<&str> = out.lines().collect();
        let bar = lines
            .iter()
            .position(|line| line.contains("Tour 1/9"))
            .unwrap_or_else(|| panic!("the tour caption bar is missing:\n{out}"));
        assert_eq!(
            bar,
            lines.len() - 2,
            "the caption bar sits directly above the status bar:\n{out}"
        );
        assert!(
            lines[lines.len() - 1].contains("tour: on"),
            "the status bar names the running tour:\n{out}"
        );
        assert!(
            out.contains("The story in one screen"),
            "the step caption is missing:\n{out}"
        );
    }

    #[test]
    fn live_tab_renders_the_capture_replay_not_the_placeholder() {
        let mut app = app_loaded_with(story_view(None));
        app.set_live(crate::data::load_live(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/sample-output/dns-capture.recorded.json"),
        ));
        app.update(crate::app::Action::SelectTab(Tab::Live.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Live",
            "RECORDED CAPTURE",
            "without MTL",
            "with MTL",
            "run live: cargo xtask dns-demo",
        ] {
            assert!(out.contains(needle), "Live tab misses {needle:?}:\n{out}");
        }
        assert!(
            !out.contains("data view lands in a later phase"),
            "Live must render its view, not the placeholder:\n{out}"
        );
    }

    #[test]
    fn method_tab_renders_its_sections_instead_of_a_placeholder() {
        let mut app = app_loaded_with(story_view(None));
        app.update(crate::app::Action::SelectTab(Tab::Method.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Method",
            "Contents",
            "Scope",
            "Sampling and extrapolation",
            "cargo xtask reproduce --report-only",
            "cargo xtask reproduce --remeasure",
        ] {
            assert!(out.contains(needle), "method tab misses {needle:?}:\n{out}");
        }
        assert!(
            !out.contains("data view lands in a later phase"),
            "Method must render its view, not a placeholder:\n{out}"
        );
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
    fn verifying_tab_renders_the_reuse_panel_at_120x40() {
        let mut derived = empty_derived();
        derived.reuse = vec![ReuseResult {
            size: "small".to_string(),
            batch_size: 100,
            curve: vec![ReuseCurvePoint {
                reuse: 1,
                effective_time_ns: 900.0,
                effective_bytes: 8000.0,
            }],
            breakeven_reuse_time: Some(1),
            breakeven_reuse_bytes: Some(2),
            r_star_time: Some(1),
            r_star_bytes: Some(2),
            baseline_time_ns: 1000.0,
            baseline_bytes: 7856.0,
            class: EvidenceClass::Derived,
        }];
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, derived);
        let mut app = app_loaded_with(view);
        app.update(crate::app::Action::SelectTab(Tab::Verifying.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Reuse", "Time r*", "Bytes r*", "Small", "r=1", "r=2", "DERIVED",
        ] {
            assert!(
                out.contains(needle),
                "verifying tab misses reuse {needle:?} at 120x40:\n{out}"
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
            "Live C vs A",
            "40x",
            "Medium C vs B",
            "215.3x",
            "9,405x",
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
                dns_anatomy: None,
                ladder: None,
                plain: Vec::new(),
                classical: Vec::new(),
                comparisons: Vec::new(),
                derived: DerivedSummary {
                    overall_wire_reduction_percent: None,
                    largest_wire_reduction_percent: None,
                    overall_verifying_trust_cached_speedup_avg: None,
                    headline: None,
                    udp_fit: None,
                    reuse: Vec::new(),
                    signature_context: Vec::new(),
                    stability: None,
                    primitives: None,
                    bound_ok: Some(true),
                    baseline_ok: Some(true),
                    bound_offenders: Vec::new(),
                    baseline_offenders: Vec::new(),
                },
                reproduction_mode: climb_report::ReproductionMode::Unknown,
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
