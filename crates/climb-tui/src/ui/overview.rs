//! The Overview tab: the headline story in ten seconds.
//!
//! Layout, top to bottom: the CLIMB word mark with the animated ladder on its
//! right, subtitle + tagline, four KPI cards and the extrapolation footnote,
//! then the environment and artifact-health panels. Every number on screen
//! comes from [`crate::data::ResultsView`]; nothing here is hardcoded result
//! data. The ladder animation is driven by [`crate::data::rung_state`] over
//! wall-clock time, so it only advances when this tab is actually rendered.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::data::{rung_state, ContextRow, Kpi, KpiKind, ReportHealth, ResultsView};
use crate::theme::{Glyphs, Theme};

/// Rungs on the Overview ladder (5 rows fit the word mark's height).
const RUNGS: usize = 5;

/// The CLIMB word mark: the letters C, L, I, M, B as 6-column block glyphs
/// joined by single spaces, so every row is exactly 34 columns by
/// construction (6+1+6+1+6+1+6+1+6).
const CLIMB: [&str; RUNGS] = [
    concat!("XXXXXX", " ", "X     ", " ", "XXXXXX", " ", "X    X", " ", "XXXXX "),
    concat!("X     ", " ", "X     ", " ", "  XX  ", " ", "XX  XX", " ", "X    X"),
    concat!("X     ", " ", "X     ", " ", "  XX  ", " ", "X XX X", " ", "XXXXX "),
    concat!("X     ", " ", "X     ", " ", "  XX  ", " ", "X    X", " ", "X    X"),
    concat!("XXXXXX", " ", "XXXXXX", " ", "XXXXXX", " ", "X    X", " ", "XXXXX "),
];

/// Wide enough below the KPIs to keep the environment and artifact panels
/// side by side; narrower terminals stack them into one block.
const TWO_COLUMN_PANELS_MIN_WIDTH: u16 = 100;

/// Render the Overview tab into `area`.
pub fn render_overview(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let context = view.context_table();
    let context_height = if context.is_empty() {
        Constraint::Length(0)
    } else {
        Constraint::Length(context.len() as u16 + 4)
    };
    let [hero, kpi_area, context_area, panels] = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(5),
        context_height,
        Constraint::Min(0),
    ])
    .areas(area);

    render_hero(frame, hero, app, theme, glyphs);
    render_kpis(frame, kpi_area, view, theme, glyphs);
    if !context.is_empty() {
        render_context(frame, context_area, theme, glyphs, &context);
    }
    render_panels(frame, panels, view, theme, glyphs);
}

/// The compact signature-context table: the classical baselines beside
/// configurations A/B/C at the headline dataset size, between the KPI cards
/// and the environment/artifact panels.
///
/// The caption must state plainly that the classical algorithms are not
/// quantum-resistant — the table is context, not a post-quantum alternative.
fn render_context(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    glyphs: &Glyphs,
    rows: &[ContextRow],
) {
    let dim = Style::new().fg(theme.muted);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{:<4}", "cfg"), dim),
        Span::styled(format!("{:<24}", "configuration"), dim),
        Span::styled(format!("{:>9}", "sig B"), dim),
        Span::styled(format!("{:>8}", "sign \u{b5}s"), dim),
        Span::styled(format!("{:>8}", "verify \u{b5}s"), dim),
        Span::styled(format!("{:>8}", "fits 1,232"), dim),
    ])];
    for row in rows {
        let style = if row.classical {
            dim
        } else {
            Style::new().fg(theme.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<4}", row.letter), style),
            Span::styled(format!("{:<24}", row.name), style),
            Span::styled(format!("{:>9}", row.bytes), style),
            Span::styled(format!("{:>8}", row.sign), style),
            Span::styled(format!("{:>8}", row.verify), style),
            Span::styled(format!("{:>8}", row.fits), style),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "classical rows are not quantum-resistant \u{b7} context only",
        dim,
    )));
    let body = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Signature context ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(body, area);
}

fn render_hero(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let [word_row, subtitle_area, tagline_area] = Layout::vertical([
        Constraint::Length(RUNGS as u16),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let [letters_col, ladder_col] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(5)]).areas(word_row);

    frame.render_widget(Paragraph::new(wordmark_lines(glyphs, theme)), letters_col);

    let ladder_lines = ladder_lines(app.elapsed(), glyphs, theme);
    frame.render_widget(
        Paragraph::new(ladder_lines).alignment(Alignment::Center),
        ladder_col,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Cryptographic Ladder Impact Measurement Benchmark",
            Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
        ))),
        subtitle_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "post-quantum DNSSEC signing with/without MTL amortization \u{00b7} AIORI-3 PS-036",
            Style::new().fg(theme.muted),
        ))),
        tagline_area,
    );
}

/// The CLIMB word mark: every `X` in the mask becomes a filled bar glyph, and
/// each row carries the theme's vertical accent gradient.
fn wordmark_lines(glyphs: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
    CLIMB
        .iter()
        .zip(theme.wordmark_rows(RUNGS))
        .map(|(mask, style)| {
            let text: String = mask
                .chars()
                .map(|c| if c == 'X' { glyphs.bar_filled } else { c })
                .collect();
            Line::from(Span::styled(text, style))
        })
        .collect()
}

/// Five 3-character ladder rows, one rung lit, climbing bottom to top and
/// looping every [`RUNG_TICK`]. Rail everywhere, glowing rung in place.
fn ladder_lines(
    elapsed: std::time::Duration,
    glyphs: &Glyphs,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let lit = rung_state(elapsed, RUNGS);
    let rail = Span::styled(glyphs.rail.to_string(), Style::new().fg(theme.border));
    (0..RUNGS)
        .map(|from_top| {
            let rung = RUNGS - 1 - from_top;
            Line::from(vec![
                rail.clone(),
                if rung == lit {
                    Span::styled(glyphs.rung.to_string(), theme.measured)
                } else {
                    Span::raw(" ")
                },
                rail.clone(),
            ])
        })
        .collect()
}

fn render_kpis(frame: &mut Frame, area: Rect, view: &ResultsView, theme: &Theme, glyphs: &Glyphs) {
    let [cards_row, footnote_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(1)]).areas(area);

    let cards: [(&str, Option<Kpi>); 5] = [
        ("DNS", view.dns_reduction()),
        ("Wire", view.wire_reduction_largest()),
        ("Signing", view.signing_speedup_at("Medium")),
        ("Verify", view.cached_verify_speedup()),
        ("C vs A", view.c_vs_a_signing_speedup()),
    ];

    let card_areas: [Rect; 5] = Layout::horizontal([Constraint::Ratio(1, 5); 5]).areas(cards_row);
    let any_extrapolated = cards
        .iter()
        .any(|(_, kpi)| matches!(kpi, Some(k) if k.kind == KpiKind::Extrapolated));

    for (card_area, (title, kpi)) in card_areas.iter().zip(cards.iter()) {
        render_kpi_card(frame, *card_area, title, kpi.as_ref(), theme, glyphs);
    }

    if any_extrapolated {
        let prefix = if theme.ascii { "~" } else { "\u{2248}" };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{prefix} extrapolated, see Method tab"),
                Style::new().fg(theme.muted),
            ))),
            footnote_area,
        );
    }
}

fn render_kpi_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    kpi: Option<&Kpi>,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let value_style = match kpi {
        Some(kpi) if kpi.kind == KpiKind::Extrapolated => theme.extrapolated,
        Some(_) => theme.measured,
        None => Style::new().fg(theme.muted).add_modifier(Modifier::DIM),
    };
    let (value, caption) = match kpi {
        Some(kpi) => (kpi.value.clone(), kpi.caption.clone()),
        None => (dash(theme).to_string(), "not recorded".to_string()),
    };
    let card = Paragraph::new(vec![
        Line::from(Span::styled(value, value_style)),
        Line::from(Span::styled(caption, Style::new().fg(theme.muted))),
    ])
    .block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                format!(" {title} "),
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(card, area);
}

fn render_panels(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    if area.width >= TWO_COLUMN_PANELS_MIN_WIDTH {
        let [env_area, health_area] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(area);
        render_env_block(frame, env_area, view, theme, glyphs);
        render_health_block(frame, health_area, view, theme, glyphs);
    } else {
        render_combined_block(frame, area, view, theme, glyphs);
    }
}

fn render_env_block(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let body = Paragraph::new(env_lines(view, theme)).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Environment ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(body, area);
}

fn render_health_block(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let body = Paragraph::new(health_lines(view, theme)).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Artifacts ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(body, area);
}

/// Narrow terminals get one stacked block: environment first, then artifacts.
fn render_combined_block(
    frame: &mut Frame,
    area: Rect,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let mut lines = env_lines(view, theme);
    lines.push(Line::from(""));
    lines.extend(health_lines(view, theme));
    let body = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Environment \u{0026} artifacts ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(body, area);
}

fn env_lines(view: &ResultsView, theme: &Theme) -> Vec<Line<'static>> {
    let value = |field: Option<String>| field.unwrap_or_else(|| dash(theme).to_string());
    vec![
        label_value("cpu", value(view.cpu_model()), theme),
        label_value("governor", value(view.governor()), theme),
        label_value("pinned", value(view.pinned_core()), theme),
        label_value("rustc", value(view.rustc()), theme),
        label_value("mtl sha", value(view.mtl_sha()), theme),
        label_value("liboqs sha", value(view.liboqs_sha()), theme),
        label_value("algo", value(view.algorithm()), theme),
    ]
}

fn health_lines(view: &ResultsView, theme: &Theme) -> Vec<Line<'static>> {
    let (fresh_mark, warn_mark) = if theme.ascii {
        ("ok", "STALE")
    } else {
        ("\u{2714}", "\u{26a0}")
    };
    let results_line = match view.age() {
        Some(age) if view.is_fresh() => {
            format!("{fresh_mark} fresh \u{00b7} {} old", fmt_age(age))
        }
        Some(age) => format!("{warn_mark} stale \u{00b7} {} old", fmt_age(age)),
        None => format!("{warn_mark} age unknown"),
    };
    let report_line = match view.report_health() {
        ReportHealth::Agree => format!("{fresh_mark} agree"),
        ReportHealth::ReportStale => {
            format!("{warn_mark} report older than results: run cargo xtask report")
        }
        ReportHealth::Drift(count) => {
            let mark = if theme.ascii { "MISMATCH" } else { "\u{2716}" };
            format!("{mark} {count} mismatches")
        }
        ReportHealth::NoReport => format!("{} no report yet", dash(theme)),
    };
    let dns_line = view
        .dns_provenance()
        .unwrap_or_else(|| format!("{} not recorded", dash(theme)));
    vec![
        label_value("results", results_line, theme),
        label_value("report", report_line, theme),
        label_value("dns", dns_line, theme),
    ]
}

fn label_value(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::new().fg(theme.accent)),
        Span::styled(value, Style::new().fg(theme.fg)),
    ])
}

/// "1 h 20 m" / "2 d 3 h" / "12 m".
fn fmt_age(age: std::time::Duration) -> String {
    let minutes = age.as_secs() / 60;
    let days = minutes / (24 * 60);
    let hours = (minutes % (24 * 60)) / 60;
    let minutes = minutes % 60;
    match (days, hours, minutes) {
        (0, 0, minutes) => format!("{minutes} m"),
        (0, hours, minutes) => format!("{hours} h {minutes} m"),
        (days, hours, _) => format!("{days} d {hours} h"),
    }
}

fn dash(theme: &Theme) -> &'static str {
    if theme.ascii {
        "-"
    } else {
        "\u{2014}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordmark_rows_are_all_34_columns() {
        let width = CLIMB[0].chars().count();
        assert_eq!(width, 34);
        for (i, row) in CLIMB.iter().enumerate() {
            assert_eq!(
                row.chars().count(),
                width,
                "row {i} must match the word mark width"
            );
        }
    }

    #[test]
    fn ladder_cycles_from_bottom_rung_to_top() {
        let glyphs = Glyphs::unicode();
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        // 450 ms / 150 ms tick = step 3, so rung 3 is lit: the second row
        // from the top (rungs render bottom = 0 at the last line).
        let lines = ladder_lines(std::time::Duration::from_millis(450), &glyphs, &theme);
        assert_eq!(lines.len(), RUNGS);
        assert_eq!(lines[0].spans[1].content, " ", "top rung idle at step 3");
        assert_eq!(
            lines[1].spans[1].content,
            glyphs.rung.to_string(),
            "step 3 lights rung 3"
        );
        // step 5 (750 ms) wraps around and lights rung 0 at the bottom.
        let wrapped = ladder_lines(std::time::Duration::from_millis(750), &glyphs, &theme);
        assert_eq!(
            wrapped[RUNGS - 1].spans[1].content,
            glyphs.rung.to_string(),
            "step 5 wraps to the bottom rung"
        );
    }

    #[test]
    fn age_formats_stay_compact() {
        assert_eq!(fmt_age(std::time::Duration::from_secs(90)), "1 m");
        assert_eq!(fmt_age(std::time::Duration::from_secs(75 * 60)), "1 h 15 m");
        assert_eq!(
            fmt_age(std::time::Duration::from_secs(51 * 3600)),
            "2 d 3 h"
        );
    }

    #[test]
    fn dash_is_ascii_safe_in_ascii_mode() {
        let unicode = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        let ascii = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            true,
        );
        assert_eq!(dash(&ascii), "-");
        assert_eq!(dash(&unicode), "\u{2014}");
    }

    #[test]
    fn wordmark_lines_fill_the_mask_with_the_active_glyph() {
        let theme = Theme::new(
            crate::theme::ThemeId::Dark,
            crate::theme::ColorDepth::TrueColor,
            false,
        );
        let unicode = wordmark_lines(&Glyphs::unicode(), &theme);
        assert_eq!(unicode.len(), RUNGS);
        assert!(
            unicode[0].spans[0].content.contains('\u{2588}'),
            "unicode word mark uses the filled block glyph"
        );
        assert!(
            !unicode[0].spans[0].content.contains('X'),
            "the X mask sentinel must be replaced"
        );
        let ascii = wordmark_lines(&Glyphs::ascii(), &theme);
        assert!(ascii[0].spans[0].content.contains('#'));
        assert!(!ascii[0].spans[0].content.contains('X'));
    }
}
