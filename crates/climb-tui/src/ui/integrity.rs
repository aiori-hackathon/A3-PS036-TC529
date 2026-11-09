//! The Integrity tab: the claims drift banner and scrollable claim table,
//! the artifact inventory, the provenance block and the evidence-class
//! legend.
//!
//! The check itself runs once per results load in `climb_tui::data::
//! integrity`; this module only renders it. On wide terminals the claim
//! table sits beside the provenance and legend stack with the artifact
//! inventory full width below; below [`WIDE_WIDTH`] everything stacks and
//! the provenance block collapses to two compact lines so the whole tab
//! still fits the minimum supported terminal.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use climb_report::{CheckStatus, ClaimCheck};
use serde_json::Value;

use crate::app::App;
use crate::data::{
    artifact_line, ArtifactState, BannerTone, CheckState, IntegrityView, ResultsView,
    REFRESH_COMMAND,
};
use crate::theme::{Glyphs, Theme};
use crate::widgets::{ellipsize, ellipsize_tokens};

/// Width at or above which the claim table sits beside the provenance and
/// legend stack, with the artifact inventory on its own full-width row.
const WIDE_WIDTH: u16 = 100;
/// Rows the narrow layout gives the artifact/provenance/legend block
/// (border title plus thirteen content lines: nine artifacts, two provenance
/// lines, the legend and the refresh command).
const NARROW_LOWER_HEIGHT: u16 = 15;
/// Rows the wide layout gives the full-width artifact inventory (nine artifact
/// rows plus the refresh command inside the border).
const WIDE_ARTIFACT_HEIGHT: u16 = 12;

/// Render the Integrity tab into `area`.
pub fn render_integrity(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResultsView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let integrity = view.integrity();
    let [banner_area, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    render_banner(frame, banner_area, integrity, theme);
    if area.width >= WIDE_WIDTH {
        render_wide(frame, body, integrity, app.integrity_scroll, theme, glyphs);
    } else {
        render_narrow(frame, body, integrity, app.integrity_scroll, theme, glyphs);
    }
}

/// The one-line verdict banner.
fn render_banner(frame: &mut Frame, area: Rect, integrity: &IntegrityView, theme: &Theme) {
    let (tone, text) = integrity.banner();
    let color = match tone {
        BannerTone::Ok => theme.ok,
        BannerTone::Warn => theme.warn,
        BannerTone::Bad => theme.bad,
    };
    let text = if theme.ascii {
        text.replace(['\u{2014}', '\u{b7}'], "-")
    } else {
        text
    };
    let hint = if theme.ascii {
        "(Up/Down scroll, r re-check)"
    } else {
        "(Up/Down scroll \u{b7} r re-check)"
    };
    let text = if matches!(integrity.check_state(), CheckState::Ran { .. }) {
        format!("{text}  {hint}")
    } else {
        text
    };
    let line = Line::from(Span::styled(
        ellipsize(&format!(" {text}"), area.width as usize, theme.ascii),
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(line).style(Style::new().bg(theme.bg)), area);
}

/// Wide layout: claims table beside provenance and legend, artifacts below.
fn render_wide(
    frame: &mut Frame,
    area: Rect,
    integrity: &IntegrityView,
    scroll: usize,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let [top, artifacts] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(WIDE_ARTIFACT_HEIGHT)])
            .areas(area);
    let [claims, side] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(top);
    let [provenance, legend] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(6)]).areas(side);

    render_claims(frame, claims, integrity, scroll, theme, glyphs);
    render_provenance(frame, provenance, integrity, false, theme, glyphs);
    render_legend(frame, legend, false, theme, glyphs);
    render_artifacts(frame, artifacts, integrity, theme, glyphs);
}

/// Narrow layout: everything stacked, provenance collapsed to compact lines.
fn render_narrow(
    frame: &mut Frame,
    area: Rect,
    integrity: &IntegrityView,
    scroll: usize,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let [claims, lower] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(NARROW_LOWER_HEIGHT)]).areas(area);
    render_claims(frame, claims, integrity, scroll, theme, glyphs);

    let inner_height = lower.height.saturating_sub(2) as usize;
    let width = lower.width.saturating_sub(2) as usize;
    let mut lines = artifact_lines(integrity, width, theme);
    lines.extend(provenance_lines(integrity, width, true, theme));
    lines.extend(legend_lines(true, theme));
    lines.truncate(inner_height);
    let title = " Artifacts \u{b7} provenance \u{b7} evidence ";
    let title = if theme.ascii {
        title.replace('\u{b7}', "-")
    } else {
        title.to_string()
    };
    let block = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                title,
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(block, lower);
}

/// The scrollable claim table: status, claim id, the value in the results
/// and the value the report carries.
fn render_claims(
    frame: &mut Frame,
    area: Rect,
    integrity: &IntegrityView,
    scroll: usize,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let width = area.width.saturating_sub(2) as usize;
    let rows = integrity.claims();
    let visible = (area.height as usize).saturating_sub(3);
    let start = scroll.min(rows.len().saturating_sub(visible.max(1)));
    let end = (start + visible).min(rows.len());

    let counts = |status| rows.iter().filter(|row| row.status == status).count();
    let scrolled = if start > 0 || end < rows.len() {
        format!(" \u{b7} rows {}-{} of {}", start + 1, end, rows.len())
    } else {
        String::new()
    };
    let title = format!(
        " Claims \u{b7} {} agree \u{b7} {} drift \u{b7} {} missing{scrolled} ",
        counts(CheckStatus::Agree),
        counts(CheckStatus::Drift),
        counts(CheckStatus::Missing),
    );
    let title = if theme.ascii {
        title.replace('\u{b7}', "-")
    } else {
        title
    };

    let id_width = (width.saturating_sub(6) / 3).clamp(10, 34);
    let mut lines = vec![header_line(id_width, theme)];
    for row in rows.get(start..end).unwrap_or(&[]) {
        lines.push(claim_line(row, id_width, theme));
    }
    let table = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                title,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(table, area);
}

/// Column header for the claim table, padded to the same column widths the
/// claim rows use.
fn header_line(id_width: usize, theme: &Theme) -> Line<'static> {
    let muted = Style::new().fg(theme.muted).add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("st ", muted),
        Span::styled(format!("{:<id_width$} ", "claim"), muted),
        Span::styled(format!("{:<id_width$} ", "results"), muted),
        Span::styled("report", muted),
    ])
}

/// One claim row: status marker, claim id, value in results, value in
/// report.
fn claim_line(row: &ClaimCheck, id_width: usize, theme: &Theme) -> Line<'static> {
    let (marker, style) = match row.status {
        CheckStatus::Agree => (
            if theme.ascii { "ok" } else { "\u{2713}" },
            Style::new().fg(theme.ok),
        ),
        CheckStatus::Drift => (
            if theme.ascii { "!!" } else { "\u{2717}" },
            Style::new().fg(theme.bad).add_modifier(Modifier::BOLD),
        ),
        CheckStatus::Missing => (
            if theme.ascii { "--" } else { "\u{b7}" },
            Style::new().fg(theme.warn),
        ),
    };
    let id = ellipsize(&row.id, id_width, theme.ascii);
    let results = ellipsize(
        &value_text(&row.value_in_results, theme.ascii),
        id_width,
        theme.ascii,
    );
    let report = match &row.value_in_report {
        Some(value) => ellipsize(&value_text(value, theme.ascii), id_width, theme.ascii),
        None => ellipsize(
            if theme.ascii { "--" } else { "\u{2014}" },
            id_width,
            theme.ascii,
        ),
    };
    Line::from(vec![
        Span::styled(format!("{marker:<3}"), style),
        Span::styled(format!("{id:<id_width$} "), Style::new().fg(theme.fg)),
        Span::styled(format!("{results:<id_width$} "), Style::new().fg(theme.fg)),
        Span::styled(report, Style::new().fg(theme.fg)),
    ])
}

/// Compact one-line rendering of a claim value straight from the artifact
/// JSON: numbers verbatim, strings unquoted, null as an em dash.
fn value_text(value: &Value, ascii: bool) -> String {
    match value {
        Value::Null => (if ascii { "--" } else { "\u{2014}" }).to_string(),
        Value::Number(number) => match number.as_f64() {
            Some(float) if float.fract() == 0.0 && float.abs() < 1e15 => (float as i64).to_string(),
            Some(float) => format!("{float}"),
            None => number.to_string(),
        },
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The artifact inventory block (wide layout).
fn render_artifacts(
    frame: &mut Frame,
    area: Rect,
    integrity: &IntegrityView,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let width = area.width.saturating_sub(2) as usize;
    let lines = artifact_lines(integrity, width, theme);
    let title = " Artifacts \u{b7} outputs stale vs results \u{b7} reference older than 24 hours ";
    let title = if theme.ascii {
        title.replace('\u{b7}', "-")
    } else {
        title.to_string()
    };
    let block = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                title,
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(block, area);
}

fn artifact_lines(integrity: &IntegrityView, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = integrity
        .artifacts()
        .iter()
        .map(|row| {
            let style = match row.state {
                ArtifactState::Present => Style::new().fg(theme.fg),
                ArtifactState::Stale => Style::new().fg(theme.warn),
                ArtifactState::Missing => Style::new().fg(theme.warn).add_modifier(Modifier::BOLD),
            };
            Line::from(Span::styled(artifact_line(row, width, theme.ascii), style))
        })
        .collect();
    lines.push(Line::from(Span::styled(
        ellipsize(&format!("  refresh: {REFRESH_COMMAND}"), width, theme.ascii),
        Style::new().fg(theme.muted),
    )));
    lines
}

/// The provenance block: commit, rustc, submodule SHAs, host fingerprint
/// and the results timestamp.
fn render_provenance(
    frame: &mut Frame,
    area: Rect,
    integrity: &IntegrityView,
    compact: bool,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let width = area.width.saturating_sub(2) as usize;
    let lines = provenance_lines(integrity, width, compact, theme);
    let block = Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_set(glyphs.border)
                .border_style(Style::new().fg(theme.border))
                .title(Span::styled(
                    " Provenance ",
                    Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(block, area);
}

/// The first 12 characters of a commit or submodule SHA, enough to identify
/// it in a compact block. Uses `get` so an unexpected non-ASCII or short
/// value can never panic the renderer.
fn short_sha(sha: &str) -> &str {
    sha.get(..sha.len().min(12)).unwrap_or(sha)
}

fn provenance_lines(
    integrity: &IntegrityView,
    width: usize,
    compact: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let provenance = integrity.provenance();
    let dash = if theme.ascii { "--" } else { "\u{2014}" };
    let joiner = if theme.ascii { " - " } else { " \u{b7} " };
    let commit = provenance.commit.as_deref().map(short_sha).unwrap_or(dash);
    let rustc = provenance.rustc.as_deref().unwrap_or(dash);
    let fingerprint = if provenance.host_fingerprint.is_empty() {
        dash
    } else {
        &provenance.host_fingerprint
    };

    if compact {
        let submodules: Vec<String> = provenance
            .submodules
            .iter()
            .map(|(path, sha)| {
                let name = path.rsplit('/').next().unwrap_or(path);
                format!("{name} {}", short_sha(sha))
            })
            .collect();
        let mut second = format!(
            "submodules {}{joiner}host {fingerprint}",
            submodules.join(joiner)
        );
        if submodules.is_empty() {
            second = format!("host {fingerprint}");
        }
        return vec![
            Line::from(Span::styled(
                ellipsize_tokens(
                    &format!("commit {commit}{joiner}rustc {rustc}"),
                    width,
                    theme.ascii,
                ),
                Style::new().fg(theme.fg),
            )),
            Line::from(Span::styled(
                ellipsize_tokens(&second, width, theme.ascii),
                Style::new().fg(theme.fg),
            )),
        ];
    }

    let label = |text: &str| Span::styled(format!("  {text:<11}"), Style::new().fg(theme.muted));
    let value = |text: &str| Span::styled(text.to_string(), Style::new().fg(theme.fg));
    let mut lines = vec![
        Line::from(vec![label("commit"), value(commit)]),
        Line::from(vec![
            label("rustc"),
            value(&ellipsize(rustc, width.saturating_sub(13), theme.ascii)),
        ]),
    ];
    let joined = provenance
        .submodules
        .iter()
        .map(|(path, sha)| format!("{path} {}", short_sha(sha)))
        .collect::<Vec<_>>()
        .join(joiner);
    if !joined.is_empty() && joined.chars().count() + 13 <= width {
        lines.push(Line::from(vec![
            label("submodules"),
            value(&ellipsize(&joined, width.saturating_sub(13), theme.ascii)),
        ]));
    } else {
        for (path, sha) in &provenance.submodules {
            lines.push(Line::from(vec![
                label(""),
                value(&ellipsize(
                    &format!("{path} {}", short_sha(sha)),
                    width.saturating_sub(13),
                    theme.ascii,
                )),
            ]));
        }
    }
    lines.push(Line::from(vec![label("host hash"), value(fingerprint)]));
    if !provenance.generated_at.is_empty() {
        lines.push(Line::from(vec![
            label("generated"),
            value(&ellipsize(
                &provenance.generated_at,
                width.saturating_sub(13),
                theme.ascii,
            )),
        ]));
    }
    lines
}

/// The evidence-class legend block.
fn render_legend(frame: &mut Frame, area: Rect, compact: bool, theme: &Theme, glyphs: &Glyphs) {
    let width = area.width.saturating_sub(2) as usize;
    let lines: Vec<Line> = legend_lines(compact, theme)
        .into_iter()
        .map(|line| {
            let text = line.to_string();
            Line::from(Span::styled(
                ellipsize(&text, width, theme.ascii),
                line.style,
            ))
        })
        .collect();
    let block = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                " Evidence classes ",
                Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(block, area);
}

fn legend_lines(compact: bool, theme: &Theme) -> Vec<Line<'static>> {
    let plain = Style::new().fg(theme.muted);
    if compact {
        let separator = if theme.ascii { " " } else { " \u{b7} " };
        return vec![Line::from(vec![
            Span::styled("  ", plain),
            Span::styled("MEASURED", theme.measured),
            Span::styled(separator, plain),
            Span::styled("EXTRAPOLATED", theme.extrapolated),
            Span::styled(separator, plain),
            Span::styled("ANALYTICAL", theme.analytical),
            Span::styled(separator, plain),
            Span::styled("DERIVED", Style::new().fg(theme.accent2)),
        ])];
    }
    vec![
        Line::from(vec![
            Span::styled("  MEASURED ", theme.measured),
            Span::styled("read from measurement", plain),
        ]),
        Line::from(vec![
            Span::styled("  EXTRAPOLATED ", theme.extrapolated),
            Span::styled("N x per-message identity", plain),
        ]),
        Line::from(vec![
            Span::styled("  ANALYTICAL ", theme.analytical),
            Span::styled("closed-form model", plain),
        ]),
        Line::from(vec![
            Span::styled("  DERIVED ", Style::new().fg(theme.accent2)),
            Span::styled("ratio or summary", plain),
        ]),
    ]
}

#[cfg(test)]
mod tests;
