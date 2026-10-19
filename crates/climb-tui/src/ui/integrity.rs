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
};
use crate::theme::{Glyphs, Theme};
use crate::widgets::{ellipsize, ellipsize_tokens};

/// Width at or above which the claim table sits beside the provenance and
/// legend stack, with the artifact inventory on its own full-width row.
const WIDE_WIDTH: u16 = 100;
/// Rows the narrow layout gives the artifact/provenance/legend block
/// (border title plus twelve content lines).
const NARROW_LOWER_HEIGHT: u16 = 14;
/// Rows the wide layout gives the full-width artifact inventory.
const WIDE_ARTIFACT_HEIGHT: u16 = 11;

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
    for row in &rows[start..end] {
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
    let title = " Artifacts \u{b7} outputs stale vs results \u{b7} reference stale over 24h ";
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
    integrity
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
        .collect()
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
mod tests {
    use std::path::{Path, PathBuf};

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::Action;
    use crate::cli::Cli;
    use crate::data;
    use crate::theme::{ColorDepth, ThemeId};
    use clap::Parser;

    /// Copy the committed sample results and report into a temp directory
    /// laid out like a real run and load it through the TUI's own loader.
    fn loaded_app(tag: &str, drift: bool) -> (crate::app::App, PathBuf) {
        let root = temp_root(tag);
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output");
        std::fs::create_dir_all(root.join("climb-report")).expect("temp dir");
        let results_source = sample.join("climb-results.json");
        if drift {
            let mut results: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&results_source).expect("read sample results"),
            )
            .expect("parse sample results");
            results["derived"]["overall_wire_reduction_percent"] = serde_json::json!(90.0);
            std::fs::write(
                root.join("climb-results.json"),
                serde_json::to_vec(&results).expect("re-serialize"),
            )
            .expect("write drifted results");
        } else {
            std::fs::copy(&results_source, root.join("climb-results.json"))
                .expect("copy sample results");
        }
        std::fs::copy(
            sample.join("report.html"),
            root.join("climb-report/report.html"),
        )
        .expect("copy sample report");

        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
        app.results_path = root.join("climb-results.json");
        app.set_data(data::load(&app.results_path));
        app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
        (app, root)
    }

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "climb-tui-integrity-ui-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn render_to_string(app: &crate::app::App, width: u16, height: u16) -> String {
        let theme = app.theme();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                super::render_integrity(
                    frame,
                    frame.area(),
                    app,
                    match &app.data {
                        crate::data::LoadState::Loaded(view) => view,
                        _ => panic!("fixture must be loaded"),
                    },
                    &theme,
                    &theme.glyphs(),
                )
            })
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
    fn agreeing_sample_pair_renders_the_full_tab_at_120x40() {
        let (app, root) = loaded_app("agree", false);
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "ALL CLAIMS AGREE",
            "overall-wire-reduction",
            "Artifacts",
            "wire-report.json",
            "present",
            "Provenance",
            "commit",
            "rustc",
            "host hash",
            "MEASURED",
            "EXTRAPOLATED",
            "ANALYTICAL",
            "DERIVED",
        ] {
            assert!(
                out.contains(needle),
                "integrity tab misses {needle:?} at 120x40:\n{out}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Phase evidence: one value changed in a temp copy of the artifact
    /// pair must surface as exactly one drifted claim, named in the banner.
    #[test]
    fn drifted_results_banner_names_the_claim_id() {
        let (app, root) = loaded_app("drift", true);
        let out = render_to_string(&app, 120, 40);
        assert!(
            out.contains("1 DRIFT"),
            "banner must count the single drift:\n{out}"
        );
        assert!(
            out.contains("overall-wire-reduction"),
            "banner must name the drifted claim:\n{out}"
        );
        assert!(!out.contains("ALL CLAIMS AGREE"), "banner:\n{out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn narrow_layout_lists_every_artifact_and_the_legend_at_80x24() {
        let (app, root) = loaded_app("narrow", false);
        let out = render_to_string(&app, 80, 24);
        for needle in [
            "ALL CLAIMS AGREE",
            "climb-results.json",
            "report.html",
            "charts",
            "wire-report.json",
            "memory-report.json",
            "dns-capture.json",
            "ladder-verification.json",
            "climb-classical-signatures.json",
            "environment.json",
            "missing",
            "submodules",
            "host",
            "MEASURED",
            "DERIVED",
        ] {
            assert!(
                out.contains(needle),
                "integrity tab misses {needle:?} at 80x24:\n{out}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scrolling_windows_the_claim_table() {
        let (mut app, root) = loaded_app("scroll", false);
        let first = render_to_string(&app, 120, 40);
        for _ in 0..10 {
            app.update(Action::LadderDown);
        }
        let scrolled = render_to_string(&app, 120, 40);
        assert_ne!(first, scrolled, "scrolling must change the visible rows");
        assert!(
            scrolled.contains("rows 11-"),
            "scroll indicator:\n{scrolled}"
        );
        // Scrolling far past the end clamps to the last window instead of
        // blanking the table.
        for _ in 0..10_000 {
            app.update(Action::LadderDown);
        }
        let bottom = render_to_string(&app, 120, 40);
        assert!(
            bottom.contains("of"),
            "the table still shows a window at the bottom:\n{bottom}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ascii_mode_never_emits_non_ascii_glyphs() {
        let (mut app, root) = loaded_app("ascii", false);
        app.ascii = true;
        let out = render_to_string(&app, 120, 40);
        let offending: Vec<char> = out
            .chars()
            .filter(|c| !c.is_ascii() && !c.is_whitespace())
            .collect();
        assert!(
            offending.is_empty(),
            "ascii mode rendered non-ASCII glyphs: {offending:?}\n{out}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_results_file_without_a_report_says_so_and_lists_missing_claims() {
        let root = temp_root("absent");
        std::fs::create_dir_all(&root).expect("temp dir");
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/climb-results.json");
        std::fs::copy(&sample, root.join("climb-results.json")).expect("copy sample results");
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
        app.results_path = root.join("climb-results.json");
        app.set_data(data::load(&app.results_path));
        app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
        let out = render_to_string(&app, 120, 40);
        assert!(out.contains("REPORT NOT FOUND"), "banner:\n{out}");
        assert!(out.contains("missing"), "claim rows:\n{out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_view_built_without_a_check_renders_the_not_run_banner() {
        let view = crate::data::test_support::view_with(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            crate::data::test_support::empty_derived(),
        );
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = crate::app::App::from_cli(&cli, ColorDepth::TrueColor);
        app.set_data(crate::data::LoadState::Loaded(Box::new(view)));
        app.update(Action::SelectTab(crate::app::Tab::Integrity.index()));
        let theme = app.theme();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                super::render_integrity(
                    frame,
                    frame.area(),
                    &app,
                    match &app.data {
                        crate::data::LoadState::Loaded(view) => view,
                        _ => unreachable!(),
                    },
                    &theme,
                    &theme.glyphs(),
                )
            })
            .expect("render");
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
        assert!(out.contains("CHECK NOT RUN"), "banner:\n{out}");
    }

    #[test]
    fn claim_values_render_verbatim_from_the_artifact_json() {
        use serde_json::json;
        assert_eq!(value_text(&json!(97.3), false), "97.3");
        assert_eq!(value_text(&json!(24438), false), "24438");
        assert_eq!(value_text(&json!(97.0), false), "97");
        assert_eq!(value_text(&json!(null), false), "\u{2014}");
        assert_eq!(value_text(&json!(null), true), "--");
        assert_eq!(value_text(&json!("Medium"), false), "Medium");
        assert_eq!(value_text(&json!(true), false), "true");
    }

    #[test]
    fn every_theme_renders_the_tab() {
        // Smoke test: every theme renders the tab without panicking.
        let (mut app, root) = loaded_app("themes", false);
        for theme in [ThemeId::Dark, ThemeId::Light, ThemeId::Contrast] {
            app.theme = theme;
            let out = render_to_string(&app, 120, 40);
            assert!(out.contains("ALL CLAIMS AGREE"), "theme {theme:?}:\n{out}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
