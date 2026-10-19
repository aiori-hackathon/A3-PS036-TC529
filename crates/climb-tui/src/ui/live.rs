//! The Live tab: the recorded DNS capture replayed as two lanes for the
//! headline question (the same query with and without MTL), with the full
//! packet list underneath. Keys are handled in [`crate::app`]; this module
//! only renders the model. Every number comes from the parsed capture through
//! [`crate::data`] — none is hardcoded.

use std::path::Path;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use climb_report::charts::fmt_count;

use crate::app::App;
use crate::data::{
    bar_cells, flag_text, Direction, HeadlineLanes, Lane, LaneStep, LiveLoadState, LiveStep,
    LiveView,
};
use crate::theme::{Glyphs, Theme};
use crate::widgets::{ellipsize, ellipsize_tokens};

/// Terminal width at or above which the two lanes share one row.
const WIDE_WIDTH: u16 = 100;
/// Lane block height: borders plus a column header, two step rows and a
/// footer line.
const LANE_HEIGHT: u16 = 6;
/// Minimum packet-log height worth drawing (column header plus five steps).
const MIN_LOG_HEIGHT: u16 = 7;
/// Widest a lane's label column gets; longer labels ellipsize.
const LABEL_MAX: usize = 18;
/// Smallest a lane's bar column gets (only reachable below the guard width).
const BAR_MIN: usize = 6;
/// Width of the direction column of a lane row (`"→ "`).
const ARROW_W: usize = 2;
const FLAGS_W: usize = 5;
const BYTES_W: usize = 9;
const SIGMA_W: usize = 10;
/// Inner width of a lane row outside its label and bar columns: direction,
/// flags, bytes, running total, and the four gaps between them.
const LANE_FIXED: usize = ARROW_W + FLAGS_W + BYTES_W + SIGMA_W + 4;
/// Inner width of a packet-log row outside its question column.
const LOG_FIXED: usize = 1 + 1 + 3 + 1 + 9 + 1 + 1 + 18 + 1 + 5 + 1 + 9 + 1 + 5 + 1 + 10;

/// Render the Live tab into `area`, including its recovery screens.
pub fn render_live(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    match app.live() {
        LiveLoadState::Loaded(view) => render_loaded(frame, area, app, view, theme, glyphs),
        LiveLoadState::Missing { path } => render_problem(
            frame,
            area,
            "recorded DNS capture not found",
            path,
            None,
            theme,
            glyphs,
        ),
        LiveLoadState::Invalid { path, error } => render_problem(
            frame,
            area,
            "recorded DNS capture could not be read",
            path,
            Some(error),
            theme,
            glyphs,
        ),
    }
}

/// Render a loaded capture: provenance header, headline summary, the
/// now-playing line, the two lanes and the run hint.
fn render_loaded(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &LiveView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let ascii = theme.ascii;
    let [header_area, summary_area, now_area, lanes_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(LANE_HEIGHT),
        Constraint::Length(1),
    ])
    .areas(area);

    let mut header = view.header_line(ascii);
    header.push_str(&format!(" [{}]", view.source_label()));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            ellipsize(&header, area.width as usize, ascii),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))),
        header_area,
    );

    let lanes = view.lanes_at(app.live_step());
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            ellipsize(&summary_line(&lanes, ascii), area.width as usize, ascii),
            Style::new().fg(theme.muted),
        ))),
        summary_area,
    );

    frame.render_widget(
        now_playing_line(view, app.live_step(), area.width, theme, glyphs),
        now_area,
    );
    render_lanes(frame, lanes_area, app, view, &lanes, theme, glyphs);
    frame.render_widget(hint_line(area.width as usize, theme), hint_area);
}

/// The one-line headline summary: both lane totals and the byte reduction.
fn summary_line(lanes: &HeadlineLanes, ascii: bool) -> String {
    let dash = if ascii { "-" } else { "\u{2014}" };
    let sum = |present: bool, total: u64| -> String {
        if present {
            format!("{} B", fmt_count(total as f64))
        } else {
            dash.to_string()
        }
    };
    let without = sum(lanes.without_mtl.present, lanes.without_mtl.total_bytes);
    let with_mtl = sum(lanes.with_mtl.present, lanes.with_mtl.total_bytes);
    let mut line = format!(
        "{} {sep} without MTL {without} {arrow} with MTL {with_mtl}",
        lanes.question,
        sep = sep(ascii),
        arrow = if ascii { ">" } else { "\u{2192}" },
    );
    if let Some(percent) = lanes.reduction_percent {
        line.push_str(&format!(" {sep} {percent:.1}% smaller", sep = sep(ascii)));
    }
    line
}

/// The cursor's current step, so a keypress always has a visible effect even
/// where the lane rows have not been revealed yet.
fn now_playing_line(
    view: &LiveView,
    cursor: usize,
    width: u16,
    theme: &Theme,
    glyphs: &Glyphs,
) -> Line<'static> {
    let ascii = theme.ascii;
    let Some(step) = view.steps().get(cursor) else {
        return Line::from(Span::styled(
            "capture contains no recorded packets".to_string(),
            Style::new().fg(theme.muted),
        ));
    };
    let sigma = if ascii { "total" } else { "\u{3a3}" };
    let total = format!("{} B", fmt_count(step.running_bytes as f64));
    let text = format!(
        "{}/{} {sep} {arrow} {label} {sep} {question} {sep} {transport} {sep} {bytes} {sep} {sigma} {total}",
        cursor + 1,
        view.steps().len(),
        sep = sep(ascii),
        arrow = direction_arrow(step.direction, ascii),
        label = step.label,
        question = step.question,
        transport = step.transport,
        bytes = bytes_cell(step.bytes, true, ascii),
        sigma = sigma,
    );
    let budget = (width as usize).saturating_sub(2);
    Line::from(vec![
        Span::styled(
            glyphs.pointer.to_string(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", ellipsize(&text, budget, ascii)),
            Style::new().fg(theme.fg),
        ),
    ])
}

/// Place the two lanes (and, when there is room, the packet log): side by
/// side when wide, stacked when narrow.
fn render_lanes(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &LiveView,
    lanes: &HeadlineLanes,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let ctx = LaneCtx {
        max_bytes: lanes.max_bytes,
        log_scale: app.live_log_scale,
        ascii: theme.ascii,
    };
    let log_area = if area.width >= WIDE_WIDTH {
        let [lane_row, log_area] =
            Layout::vertical([Constraint::Length(LANE_HEIGHT), Constraint::Min(0)]).areas(area);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(lane_row);
        render_lane(frame, left, &lanes.without_mtl, &ctx, theme, glyphs);
        render_lane(frame, right, &lanes.with_mtl, &ctx, theme, glyphs);
        log_area
    } else {
        let [first, rest] =
            Layout::vertical([Constraint::Length(LANE_HEIGHT), Constraint::Min(0)]).areas(area);
        let [second, log_area] =
            Layout::vertical([Constraint::Length(LANE_HEIGHT), Constraint::Min(0)]).areas(rest);
        render_lane(frame, first, &lanes.without_mtl, &ctx, theme, glyphs);
        render_lane(frame, second, &lanes.with_mtl, &ctx, theme, glyphs);
        log_area
    };
    if log_area.height >= MIN_LOG_HEIGHT {
        render_log(frame, log_area, view, app.live_step(), theme, glyphs);
    }
}

/// Values the lane rows need that the [`Lane`] itself does not carry.
struct LaneCtx {
    max_bytes: u64,
    log_scale: bool,
    ascii: bool,
}

/// Render one headline lane: column header, the pass's steps (revealed as the
/// replay cursor reaches them), and a footer with the lane total.
fn render_lane(
    frame: &mut Frame,
    area: Rect,
    lane: &Lane,
    ctx: &LaneCtx,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let title = lane
        .title
        .replace('\u{b7}', if ctx.ascii { "-" } else { "\u{b7}" });
    let block = Block::bordered()
        .border_set(glyphs.border)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
        ));
    let inner_w = area.width.saturating_sub(2) as usize;
    let muted = Style::new().fg(theme.muted);

    if !lane.present {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                ellipsize("headline query not in this capture", inner_w, ctx.ascii),
                muted,
            )))
            .block(block),
            area,
        );
        return;
    }

    let flex = inner_w.saturating_sub(LANE_FIXED);
    let label_w = flex.saturating_sub(BAR_MIN).clamp(1, LABEL_MAX);
    let bar_w = flex.saturating_sub(label_w);

    let mut lines = vec![Line::from(vec![
        Span::raw(" ".repeat(ARROW_W)),
        Span::styled(pad_right("step", label_w, ctx.ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("flags", FLAGS_W, ctx.ascii), muted),
        Span::raw(" "),
        Span::raw(" ".repeat(bar_w)),
        Span::raw(" "),
        Span::styled(pad_left("bytes", BYTES_W, ctx.ascii), muted),
        Span::raw(" "),
        Span::styled(pad_left(sigma_word(ctx.ascii), SIGMA_W, ctx.ascii), muted),
    ])];

    let step_rows = area.height.saturating_sub(4) as usize;
    for step in lane.steps.iter().take(step_rows) {
        lines.push(step_line(step, ctx, theme, glyphs, label_w, bar_w));
    }

    let hidden = lane.steps.len().saturating_sub(step_rows);
    let scale = if ctx.log_scale {
        "log10 scale (t toggles)"
    } else {
        "linear scale (t toggles)"
    };
    let mut footer = format!(
        "total {} B {} {}",
        fmt_count(lane.total_bytes as f64),
        sep(ctx.ascii),
        scale
    );
    if hidden > 0 {
        footer.push_str(&format!(" {} +{hidden} hidden", sep(ctx.ascii)));
    }
    lines.push(Line::from(Span::styled(
        ellipsize(&footer, inner_w, ctx.ascii),
        Style::new().fg(theme.fg),
    )));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One lane row: direction, label, flags, proportional byte bar, observed
/// size and the lane's running total. Pending rows stay readable but muted.
fn step_line(
    step: &LaneStep,
    ctx: &LaneCtx,
    theme: &Theme,
    glyphs: &Glyphs,
    label_w: usize,
    bar_w: usize,
) -> Line<'static> {
    let ascii = ctx.ascii;
    let row_style = if step.revealed {
        Style::new().fg(theme.fg)
    } else {
        Style::new().fg(theme.muted)
    };
    let (filled, empty) = if step.revealed && step.bytes.unwrap_or(0) > 0 {
        let n = bar_cells(step.bytes, ctx.max_bytes, bar_w as u16, ctx.log_scale) as usize;
        (
            glyphs.bar_filled.to_string().repeat(n),
            glyphs.bar_empty.to_string().repeat(bar_w.saturating_sub(n)),
        )
    } else {
        (" ".repeat(bar_w), String::new())
    };
    Line::from(vec![
        Span::styled(
            format!("{} ", direction_arrow(step.direction, ascii)),
            row_style,
        ),
        Span::styled(pad_right(&step.label, label_w, ascii), row_style),
        Span::raw(" "),
        Span::styled(
            pad_right(&flag_text(step.tc, step.ad, ascii), FLAGS_W, ascii),
            row_style,
        ),
        Span::raw(" "),
        Span::styled(filled, Style::new().fg(theme.accent2)),
        Span::styled(empty, Style::new().fg(theme.muted)),
        Span::raw(" "),
        Span::styled(
            pad_left(
                &bytes_cell(step.bytes, step.revealed, ascii),
                BYTES_W,
                ascii,
            ),
            row_style,
        ),
        Span::raw(" "),
        Span::styled(
            pad_left(
                &total_cell(step.running_bytes, step.revealed, ascii),
                SIGMA_W,
                ascii,
            ),
            row_style,
        ),
    ])
}

/// Render the packet log: every step with its pass, question, leg, transport,
/// size and flags, with a pointer on the replay cursor.
fn render_log(
    frame: &mut Frame,
    area: Rect,
    view: &LiveView,
    cursor: usize,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let ascii = theme.ascii;
    let count = view.steps().len();
    let block = Block::bordered()
        .border_set(glyphs.border)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            format!(" Recorded packets {} {count} steps ", sep(ascii)),
            Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
        ));
    let inner_w = area.width.saturating_sub(2) as usize;
    let q_w = inner_w.saturating_sub(LOG_FIXED).max(10);
    let rows = area.height.saturating_sub(3) as usize;
    let (start, end) = log_window(count, cursor, rows);

    let muted = Style::new().fg(theme.muted);
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(pad_left("#", 3, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("pass", 9, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("question", q_w, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("leg", 18, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("proto", 5, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_left("bytes", BYTES_W, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_right("flags", FLAGS_W, ascii), muted),
        Span::raw(" "),
        Span::styled(pad_left(sigma_word(ascii), SIGMA_W, ascii), muted),
    ])];
    if let Some(slice) = view.steps().get(start..end) {
        for (offset, step) in slice.iter().enumerate() {
            lines.push(log_row_line(
                start + offset,
                step,
                cursor,
                q_w,
                theme,
                glyphs,
                ascii,
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One packet-log row.
fn log_row_line(
    index: usize,
    step: &LiveStep,
    cursor: usize,
    q_w: usize,
    theme: &Theme,
    glyphs: &Glyphs,
    ascii: bool,
) -> Line<'static> {
    let current = index == cursor;
    let row_style = if current {
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
    } else if index <= cursor {
        Style::new().fg(theme.fg)
    } else {
        Style::new().fg(theme.muted)
    };
    let pointer = if current {
        glyphs.pointer.to_string()
    } else {
        " ".to_string()
    };
    Line::from(vec![
        Span::styled(pointer, row_style),
        Span::raw(" "),
        Span::styled(pad_left(&(index + 1).to_string(), 3, ascii), row_style),
        Span::raw(" "),
        Span::styled(pad_right(&step.pass, 9, ascii), row_style),
        Span::raw(" "),
        Span::styled(pad_right(&step.question, q_w, ascii), row_style),
        Span::raw(" "),
        Span::styled(pad_right(&step.label, 18, ascii), row_style),
        Span::raw(" "),
        Span::styled(pad_right(&step.transport, 5, ascii), row_style),
        Span::raw(" "),
        Span::styled(
            pad_left(&bytes_cell(step.bytes, true, ascii), BYTES_W, ascii),
            row_style,
        ),
        Span::raw(" "),
        Span::styled(
            pad_right(&flag_text(step.tc, step.ad, ascii), FLAGS_W, ascii),
            row_style,
        ),
        Span::raw(" "),
        Span::styled(
            pad_left(&total_cell(step.running_bytes, true, ascii), SIGMA_W, ascii),
            row_style,
        ),
    ])
}

/// The window of step rows that fits `rows` log lines around `cursor`: the
/// whole list when it fits, otherwise a slice that always contains the cursor
/// (centred when it can be, pinned to an end when it cannot).
fn log_window(count: usize, cursor: usize, rows: usize) -> (usize, usize) {
    if count == 0 || rows == 0 {
        return (0, 0);
    }
    if count <= rows {
        return (0, count);
    }
    let cursor = cursor.min(count - 1);
    let start = cursor.saturating_sub(rows / 2).min(count - rows);
    (start, (start + rows).min(count))
}

/// The footer hint: how to produce a capture, and the replay keys.
fn hint_line(width: usize, theme: &Theme) -> Line<'static> {
    let ascii = theme.ascii;
    let command = "run live: cargo xtask dns-demo";
    let keys = format!(
        "space/backspace step {} enter autoplay {} t scale",
        sep(ascii),
        sep(ascii)
    );
    let mut spans = vec![Span::styled(
        ellipsize(command, width, ascii),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )];
    let used = width.min(command.chars().count());
    if width >= used + 4 {
        spans.push(Span::styled(
            ellipsize(&format!("  {keys}"), width - used, ascii),
            Style::new().fg(theme.muted),
        ));
    }
    Line::from(spans)
}

/// The recovery screen for a missing or unparseable capture: name the path,
/// show the error when there is one, and point at the recording command.
fn render_problem(
    frame: &mut Frame,
    area: Rect,
    headline: &str,
    path: &Path,
    error: Option<&String>,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let ascii = theme.ascii;
    let inner = area.width.saturating_sub(3) as usize;
    let fit = |text: String| ellipsize_tokens(&text, inner, ascii);
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            fit(headline.to_string()),
            Style::new().fg(theme.bad).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            fit(format!("capture: {}", path.display())),
            Style::new().fg(theme.fg),
        )),
    ];
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            fit(format!("error: {error}")),
            Style::new().fg(theme.muted),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        fit("run live: cargo xtask dns-demo".to_string()),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        fit("press r to reload after fixing".to_string()),
        Style::new().fg(theme.muted),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .border_set(glyphs.border)
                    .border_style(Style::new().fg(theme.border))
                    .title(Span::styled(
                        " Live ",
                        Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                    )),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// `text` left-aligned in `width` columns, ellipsized when too long.
fn pad_right(text: &str, width: usize, ascii: bool) -> String {
    let text = ellipsize(text, width, ascii);
    let pad = width.saturating_sub(text.chars().count());
    format!("{text}{}", " ".repeat(pad))
}

/// `text` right-aligned in `width` columns, ellipsized when too long.
fn pad_left(text: &str, width: usize, ascii: bool) -> String {
    let text = ellipsize(text, width, ascii);
    let pad = width.saturating_sub(text.chars().count());
    format!("{}{text}", " ".repeat(pad))
}

/// Observed size cell: an em dash without a recorded size, the size otherwise,
/// and an ellipsis while the replay has not reached the row.
fn bytes_cell(bytes: Option<u64>, revealed: bool, ascii: bool) -> String {
    if !revealed {
        return pending(ascii).to_string();
    }
    match bytes {
        Some(value) => format!("{} B", fmt_count(value as f64)),
        None => dash(ascii).to_string(),
    }
}

/// Running-total cell of one row (same reveal rules as [`bytes_cell`]).
fn total_cell(value: u64, revealed: bool, ascii: bool) -> String {
    if !revealed {
        pending(ascii).to_string()
    } else {
        format!("{} B", fmt_count(value as f64))
    }
}

fn dash(ascii: bool) -> &'static str {
    if ascii {
        "-"
    } else {
        "\u{2014}"
    }
}

fn pending(ascii: bool) -> &'static str {
    if ascii {
        "~"
    } else {
        "\u{2026}"
    }
}

fn sep(ascii: bool) -> &'static str {
    if ascii {
        "-"
    } else {
        "\u{b7}"
    }
}

fn direction_arrow(direction: Direction, ascii: bool) -> &'static str {
    match (direction, ascii) {
        (Direction::Query, false) => "\u{2192}",
        (Direction::Query, true) => ">",
        (Direction::Answer, false) => "\u{2190}",
        (Direction::Answer, true) => "<",
    }
}

fn sigma_word(ascii: bool) -> &'static str {
    if ascii {
        "total"
    } else {
        "\u{3a3}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::Tab;
    use crate::cli::Cli;
    use crate::data::LiveSource;
    use crate::theme::ColorDepth;

    /// The committed sample capture, read at compile time so the tests need no
    /// local run artifacts.
    fn loaded_live() -> LiveLoadState {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/sample-output/dns-capture.recorded.json"
        ));
        let capture: climb_report::DnsCapture =
            serde_json::from_str(json).expect("the recorded capture parses");
        LiveLoadState::Loaded(Box::new(LiveView::new(
            capture,
            LiveSource::RecordedFixture,
        )))
    }

    fn live_app() -> App {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.tab = Tab::Live;
        app.set_live(loaded_live());
        app
    }

    /// The whole dashboard, as the user sees it (tab bar, body, status bar).
    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| crate::ui::render(frame, app))
            .expect("rendering never fails on TestBackend");
        buffer_to_string(&terminal)
    }

    /// Only the Live widget, without the status bar's fixed decorations —
    /// what the ASCII-mode test needs to judge whether the tab stayed ASCII.
    fn render_widget_to_string(app: &App, width: u16, height: u16) -> String {
        let theme = app.theme();
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_live(frame, area, app, &theme, &glyphs);
            })
            .expect("rendering never fails on TestBackend");
        buffer_to_string(&terminal)
    }

    fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
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
    fn renders_header_lanes_summary_log_and_hint_at_120x40() {
        let app = live_app();
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "RECORDED CAPTURE 2026-10-12",
            "commit \u{2014}",
            "[recorded fixture]",
            "www.climb.example. A",
            "without MTL \u{b7} full pass",
            "with MTL \u{b7} condensed pass",
            "answer (full)",
            "answer (condensed)",
            "without MTL 24,438 B",
            "with MTL 658 B",
            "% smaller",
            "linear scale",
            "Recorded packets",
            "1/18",
            "run live: cargo xtask dns-demo",
        ] {
            assert!(
                out.contains(needle),
                "Live tab misses {needle:?} at 120x40:\n{out}"
            );
        }
    }

    #[test]
    fn renders_both_lanes_stacked_and_drops_the_log_at_80x24() {
        let app = live_app();
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "without MTL",
            "with MTL",
            "answer (condensed)",
            "run live: cargo xtask dns-demo",
        ] {
            assert!(
                out.contains(needle),
                "Live tab misses {needle:?} at 80x24:\n{out}"
            );
        }
        assert!(
            !out.contains("Recorded packets"),
            "no room for the packet log at 80x24:\n{out}"
        );
    }

    #[test]
    fn mono_depth_renders_the_same_text() {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::Mono);
        app.tab = Tab::Live;
        app.set_live(loaded_live());
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "RECORDED CAPTURE 2026-10-12",
            "without MTL",
            "with MTL",
            "run live: cargo xtask dns-demo",
        ] {
            assert!(
                out.contains(needle),
                "Live tab misses {needle:?} in mono depth:\n{out}"
            );
        }
    }

    #[test]
    fn ascii_mode_stays_ascii_on_the_live_tab() {
        let mut app = live_app();
        app.ascii = true;
        let out = render_widget_to_string(&app, 120, 40);
        assert!(out.contains('+'), "ascii borders:\n{out}");
        assert!(
            out.contains("run live: cargo xtask dns-demo"),
            "hint:\n{out}"
        );
        for unicode_only in [
            '\u{2014}', '\u{b7}', '\u{2192}', '\u{2190}', '\u{2026}', '\u{3a3}', '\u{2588}',
            '\u{25b6}',
        ] {
            assert!(
                !out.contains(unicode_only),
                "ascii mode leaked {unicode_only:?}:\n{out}"
            );
        }
    }

    #[test]
    fn missing_capture_renders_the_recovery_screen() {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.tab = Tab::Live;
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "recorded DNS capture not found",
            "capture: target/dns-capture.json",
            "run live: cargo xtask dns-demo",
            "press r to reload after fixing",
        ] {
            assert!(
                out.contains(needle),
                "missing screen misses {needle:?}:\n{out}"
            );
        }
    }

    #[test]
    fn invalid_capture_names_the_error_and_the_path() {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.tab = Tab::Live;
        app.set_live(LiveLoadState::Invalid {
            path: std::path::PathBuf::from("target/dns-capture.json"),
            error: "expected value at line 1 column 2".to_string(),
        });
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "recorded DNS capture could not be read",
            "capture: target/dns-capture.json",
            "error: expected value at line 1 column 2",
            "run live: cargo xtask dns-demo",
        ] {
            assert!(
                out.contains(needle),
                "invalid screen misses {needle:?}:\n{out}"
            );
        }
    }

    #[test]
    fn stepping_the_replay_reveals_more_lane_bytes_than_the_cursor_started_with() {
        let mut app = live_app();
        let count = app.live().step_count();
        let before = render_to_string(&app, 120, 40);
        for _ in 0..count - 1 {
            app.update(crate::app::Action::ToggleBanner);
        }
        assert_eq!(app.live_step(), count - 1, "cursor at the last step");
        let after = render_to_string(&app, 120, 40);
        assert_ne!(before, after, "stepping must change what is drawn");
        assert!(
            after.matches("24,438 B").count() > before.matches("24,438 B").count(),
            "reaching the headline step must reveal its lane bytes:\n{after}"
        );
    }

    #[test]
    fn t_switches_the_lane_scale_note_between_linear_and_log10() {
        let mut app = live_app();
        let linear = render_to_string(&app, 120, 40);
        assert!(linear.contains("linear scale"), "default:\n{linear}");
        app.update(crate::app::Action::ToggleScale);
        let log = render_to_string(&app, 120, 40);
        assert!(log.contains("log10 scale"), "after t:\n{log}");
        assert!(!log.contains("linear scale"), "after t:\n{log}");
    }

    #[test]
    fn log_window_fits_the_whole_list_when_it_is_short() {
        assert_eq!(log_window(18, 5, 20), (0, 18), "everything fits");
        assert_eq!(log_window(0, 0, 20), (0, 0), "empty capture");
        assert_eq!(log_window(18, 5, 0), (0, 0), "no rows to fill");
    }

    #[test]
    fn log_window_centres_the_cursor_and_pins_to_the_ends() {
        assert_eq!(log_window(18, 0, 10), (0, 10), "start-pinned");
        assert_eq!(log_window(18, 9, 10), (4, 14), "centred");
        assert_eq!(log_window(18, 17, 10), (8, 18), "end-pinned");
        for (count, cursor, rows) in [(18, 0, 10), (18, 9, 10), (18, 17, 10), (30, 29, 7)] {
            let (start, end) = log_window(count, cursor, rows);
            assert!(
                start <= cursor && cursor < end,
                "cursor {cursor} outside [{start}, {end})"
            );
            assert_eq!(end - start, rows.min(count), "window height");
        }
    }

    #[test]
    fn pad_helpers_align_and_ellipsize() {
        assert_eq!(pad_right("abc", 5, false), "abc  ");
        assert_eq!(pad_left("abc", 5, false), "  abc");
        assert_eq!(pad_right("abcdef", 4, false), "abc\u{2026}");
        assert_eq!(pad_right("abcdef", 4, true), "abc~");
        assert_eq!(pad_right("abc", 0, false), "");
        assert_eq!(pad_left("abcdef", 3, false), "ab\u{2026}", "marked cut");
    }
}
