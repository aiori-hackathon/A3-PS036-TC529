//! The Method tab: the experimental method written to be read — scope,
//! baselines, evidence classes, the environment the run was taken in, the
//! sampling behind each number, and the limitations of what was measured.
//!
//! The tab is a document, so it scrolls rather than laying out panels: the view
//! model's sections (plus the baseline document, rendered verbatim) are
//! flattened into one list of lines that a single bordered panel windows. On
//! wide terminals a contents list sits beside the document with the current
//! section marked; below [`WIDE_WIDTH`] the contents list is dropped and the
//! document takes the full width, which is what keeps the whole tab readable at
//! the minimum supported terminal.
//!
//! Every string here is either a label or comes from the view model — no
//! result number is typed into this file.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::data::{BaselineSource, MethodLine, MethodView};
use crate::theme::{Glyphs, Theme};
use crate::widgets::ellipsize;

/// Terminal width at or above which the contents list sits beside the document.
const WIDE_WIDTH: u16 = 100;
/// Width of the contents column in the wide layout. The longest section title
/// ("Sampling and extrapolation", 26 columns) must fit inside the block's inner
/// width after the `▸ ` marker, so the column stays wider than it looks.
const TOC_WIDTH: u16 = 32;
/// Column the field labels are padded to before the value.
const LABEL_WIDTH: usize = 24;

/// Render the Method tab into `area`.
pub fn render_method(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, glyphs: &Glyphs) {
    let view = app.method();
    let [head, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    render_head(frame, head, view, theme);

    let doc = flatten(view, theme.ascii);
    if area.width >= WIDE_WIDTH {
        let [toc, content] =
            Layout::horizontal([Constraint::Length(TOC_WIDTH), Constraint::Min(0)]).areas(body);
        let start = scroll_start(doc.len(), visible_rows(content, theme), app.method_scroll);
        render_toc(frame, toc, view, doc_section(doc.get(start)), theme, glyphs);
        render_document(
            frame,
            content,
            &doc,
            start,
            app.method_scroll,
            theme,
            glyphs,
        );
    } else {
        let start = scroll_start(doc.len(), visible_rows(body, theme), app.method_scroll);
        render_document(frame, body, &doc, start, app.method_scroll, theme, glyphs);
    }
}

/// The one-line provenance strip above the document.
fn render_head(frame: &mut Frame, area: Rect, view: &MethodView, theme: &Theme) {
    let source = match view.baselines().source() {
        BaselineSource::File(path) => format!("baselines: {}", path.display()),
        BaselineSource::Embedded => "baselines: embedded copy".to_string(),
    };
    let hint = "(Up/Down scroll)";
    let text = ellipsize(
        &format!(" Method {} {source}  {hint}", sep(theme)),
        area.width as usize,
        theme.ascii,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text, Style::new().fg(theme.muted))))
            .style(Style::new().bg(theme.bg)),
        area,
    );
}

/// The contents column: every section title, the visible one marked.
fn render_toc(
    frame: &mut Frame,
    area: Rect,
    view: &MethodView,
    current: Option<usize>,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let width = area.width.saturating_sub(2) as usize;
    let lines: Vec<Line<'static>> = view
        .sections()
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let marker = if Some(index) == current { 1 } else { 0 };
            let glyph = if Some(index) == current {
                if theme.ascii {
                    ">"
                } else {
                    "\u{25b8}"
                }
            } else {
                " "
            };
            let style = if Some(index) == current {
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.muted)
            };
            let _ = marker;
            Line::from(Span::styled(
                ellipsize(&format!("{glyph} {}", section.title), width, theme.ascii),
                style,
            ))
        })
        .collect();
    let block = Block::bordered()
        .border_set(glyphs.border)
        .border_style(Style::new().fg(theme.border))
        .title(Span::styled(
            " Contents ",
            Style::new().fg(theme.accent2).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The document panel: the windowed slice of flattened lines.
fn render_document(
    frame: &mut Frame,
    area: Rect,
    doc: &[DocLine],
    start: usize,
    scroll: usize,
    theme: &Theme,
    glyphs: &Glyphs,
) {
    let visible = visible_rows(area, theme);
    let end = (start + visible).min(doc.len());

    let window = doc.get(start..end).unwrap_or(&[]);
    let lines: Vec<Line<'static>> = window.iter().map(|line| line.to_line(theme)).collect();

    let title = document_title(doc, start, end, scroll, theme);
    let block = Paragraph::new(lines).block(
        Block::bordered()
            .border_set(glyphs.border)
            .border_style(Style::new().fg(theme.border))
            .title(Span::styled(
                title,
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(block, area);
}

/// The document block title: the current section, the row window and the
/// scroll hint, ellipsized to the block's width.
fn document_title(
    doc: &[DocLine],
    start: usize,
    end: usize,
    scroll: usize,
    theme: &Theme,
) -> String {
    let section = doc_section(doc.get(start)).unwrap_or(0);
    let current = MethodView::SECTION_TITLES
        .get(section)
        .copied()
        .unwrap_or("Method");
    let total = doc.len();
    let window = if start > 0 || end < total {
        format!(" \u{b7} rows {}-{} of {total}", start + 1, end)
    } else {
        String::new()
    };
    let at_top = scroll == 0 && start == 0;
    let hint = if at_top {
        format!(" \u{b7} {total} rows \u{b7} Up/Down scroll ")
    } else {
        format!(" \u{b7} {total} rows \u{b7} Up/Down scroll{window} ")
    };
    let title = format!(" {current}{hint}");
    if theme.ascii {
        title.replace(['\u{b7}', '\u{25b8}', '\u{2014}'], "-")
    } else {
        title
    }
}

/// How many body rows fit inside one bordered panel of `area`.
fn visible_rows(area: Rect, _theme: &Theme) -> usize {
    (area.height as usize).saturating_sub(2)
}

/// The scroll offset clamped so the window never runs past the end of the
/// document: scrolling further than the last full window pins to the bottom.
fn scroll_start(total: usize, visible: usize, scroll: usize) -> usize {
    scroll.min(total.saturating_sub(visible.max(1)))
}

/// The section a flattened line belongs to, for the contents marker.
fn doc_section(line: Option<&DocLine>) -> Option<usize> {
    line.map(|line| line.section)
}

/// One flattened document line: the section it belongs to plus its body role.
#[derive(Debug, Clone, PartialEq)]
struct DocLine {
    /// Index into [`MethodView::sections`].
    section: usize,
    /// What kind of line this is; drives wording and style.
    body: DocBody,
}

/// The body of a flattened line, before it is wrapped and styled.
#[derive(Debug, Clone, PartialEq)]
enum DocBody {
    /// A section heading.
    Heading(String),
    /// A plain paragraph line.
    Text(String),
    /// A command or identity fragment.
    Code(String),
    /// A muted note.
    Note(String),
    /// A labelled value.
    Field { label: String, value: String },
}

impl DocLine {
    fn heading(section: usize, title: &str) -> Self {
        Self {
            section,
            body: DocBody::Heading(title.to_string()),
        }
    }

    /// Render to a styled terminal line.
    fn to_line(&self, theme: &Theme) -> Line<'static> {
        let (style, indent, text) = match &self.body {
            DocBody::Heading(text) => (
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                0usize,
                text.clone(),
            ),
            DocBody::Text(text) => (Style::new().fg(theme.fg), 2usize, text.clone()),
            DocBody::Code(text) => (Style::new().fg(theme.accent2), 4usize, text.clone()),
            DocBody::Note(text) => (Style::new().fg(theme.muted), 2usize, text.clone()),
            DocBody::Field { label, value } => {
                let pad = LABEL_WIDTH.saturating_sub(label.chars().count());
                return Line::from(vec![
                    Span::styled(
                        format!("  {label}{}  ", " ".repeat(pad.max(1))),
                        Style::new().fg(theme.muted),
                    ),
                    Span::styled(value.clone(), Style::new().fg(theme.fg)),
                ]);
            }
        };
        Line::from(Span::styled(
            format!("{}{}", " ".repeat(indent), text),
            style,
        ))
    }
}

/// Flatten the sections into one list of document lines, wrapping plain text to
/// a target width so the renderer only has to window it. Wrapping is done here
/// rather than by the paragraph so the section marker and the row count both
/// refer to the same list.
fn flatten(view: &MethodView, ascii: bool) -> Vec<DocLine> {
    let mut out = Vec::new();
    for (index, section) in view.sections().iter().enumerate() {
        out.push(DocLine::heading(index, &sanitize(&section.title, ascii)));
        for line in &section.lines {
            out.push(DocLine {
                section: index,
                body: match line {
                    MethodLine::Text(text) => DocBody::Text(sanitize(text, ascii)),
                    MethodLine::Code(text) => DocBody::Code(sanitize(text, ascii)),
                    MethodLine::Note(text) => DocBody::Note(sanitize(text, ascii)),
                    MethodLine::Field { label, value } => DocBody::Field {
                        label: sanitize(label, ascii),
                        value: sanitize(value, ascii),
                    },
                },
            });
        }
        // The two sections whose bodies are produced by the renderer (the
        // baseline document and the environment table) are appended here so the
        // document panel shows them like every other section.
        match section.title.as_str() {
            "Baselines A/B/C" => out.extend(baseline_lines(view, index, ascii)),
            "Environment and pinning" => out.extend(environment_lines(view, index)),
            _ => {}
        }
    }
    out
}

/// The baseline document as document lines: markdown headings become headings,
/// tables and code fences become code lines, the rest plain text.
fn baseline_lines(view: &MethodView, section: usize, ascii: bool) -> Vec<DocLine> {
    view.baselines()
        .text()
        .lines()
        .map(|raw| {
            let body = if raw.starts_with('#') {
                let title = raw.trim_start_matches('#').trim();
                DocBody::Heading(sanitize(title, ascii))
            } else if raw.trim().is_empty() {
                DocBody::Text(String::new())
            } else if raw.starts_with('|') || raw.starts_with("```") {
                DocBody::Code(sanitize(raw, ascii))
            } else {
                DocBody::Text(sanitize(raw, ascii))
            };
            DocLine { section, body }
        })
        .collect()
}

/// The environment rows as document lines.
fn environment_lines(view: &MethodView, section: usize) -> Vec<DocLine> {
    if view.environment().is_empty() {
        return vec![DocLine {
            section,
            body: DocBody::Note(
                "no environment fingerprint recorded in this results file".to_string(),
            ),
        }];
    }
    view.environment()
        .iter()
        .map(|row| DocLine {
            section,
            body: DocBody::Field {
                label: sanitize(row.label, false),
                value: sanitize(
                    &row.value
                        .clone()
                        .unwrap_or_else(|| "not recorded".to_string()),
                    false,
                ),
            },
        })
        .collect()
}

/// The separator between fields in a title or strip: the middle dot, or an
/// ASCII hyphen in ASCII mode.
fn sep(theme: &Theme) -> &'static str {
    if theme.ascii {
        "-"
    } else {
        "\u{b7}"
    }
}

/// Replace the punctuation the terminal's plain glyph set cannot be trusted
/// for with ASCII equivalents, so mono and ASCII modes stay readable.
fn sanitize(text: &str, ascii: bool) -> String {
    if !ascii {
        return text.to_string();
    }
    text.chars()
        .map(|c| match c {
            '\u{2014}' | '\u{2013}' | '\u{2212}' | '\u{b7}' | '\u{2022}' => '-',
            '\u{2192}' => '>',
            '\u{d7}' => 'x',
            '\u{2248}' => '~',
            '\u{3a3}' => 'S',
            '\u{2026}' => '.',
            '\u{25b8}' => '>',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use clap::Parser;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::{App, Tab};
    use crate::cli::Cli;
    use crate::data;
    use crate::theme::{ColorDepth, ThemeId};

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "climb-tui-method-ui-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// An app on the Method tab with the committed sample results loaded
    /// through the TUI's own loader, so the tab is exercised end to end.
    fn sample_app(tag: &str) -> (App, PathBuf) {
        let root = temp_root(tag);
        std::fs::create_dir_all(root.join("target")).expect("target dir");
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output");
        std::fs::copy(
            sample.join("climb-results.json"),
            root.join("target/climb-results.json"),
        )
        .expect("copy sample results");
        // The baseline document sits one level above the results directory,
        // under the test's control (`<root>/docs/BASELINES.md`).
        std::fs::create_dir_all(root.join("docs")).expect("docs dir");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/BASELINES.md"),
            root.join("docs/BASELINES.md"),
        )
        .expect("copy baselines");

        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.results_path = root.join("target/climb-results.json");
        app.set_data(data::load(&app.results_path));
        app.update(crate::app::Action::SelectTab(Tab::Method.index()));
        (app, root)
    }

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let theme = app.theme();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| render_method(frame, frame.area(), app, &theme, &theme.glyphs()))
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
    fn renders_the_sections_contents_and_baselines_at_120x40() {
        let (app, root) = sample_app("wide");
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Contents",
            "Scope",
            "Evidence classes",
            "Environment and pinning",
            "Sampling and extrapolation",
            "Known limits",
            "cargo xtask reproduce",
            "Signing baselines",
        ] {
            assert!(
                out.contains(needle),
                "method tab misses {needle:?} at 120x40:\n{out}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn narrow_terminal_drops_the_contents_column_and_keeps_the_document() {
        let (app, root) = sample_app("narrow");
        let out = render_to_string(&app, 80, 24);
        assert!(
            !out.contains("Contents"),
            "the contents column is dropped below {WIDE_WIDTH} columns:\n{out}"
        );
        for needle in ["Method", "Scope", "cargo xtask reproduce"] {
            assert!(out.contains(needle), "80x24 loses {needle:?}:\n{out}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scrolling_windows_the_document_and_clamps_at_the_bottom() {
        let (mut app, root) = sample_app("scroll");
        let first = render_to_string(&app, 120, 40);
        for _ in 0..40 {
            app.update(crate::app::Action::LadderDown);
        }
        let scrolled = render_to_string(&app, 120, 40);
        assert_ne!(first, scrolled, "scrolling must change the visible rows");
        assert!(
            scrolled.contains("Known limits"),
            "scrolling reaches the last section:\n{scrolled}"
        );
        for _ in 0..10_000 {
            app.update(crate::app::Action::LadderDown);
        }
        let bottom = render_to_string(&app, 120, 40);
        assert!(
            bottom.contains("Known limits"),
            "clamped at the bottom the last section stays visible:\n{bottom}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ascii_mode_never_emits_non_ascii_glyphs() {
        let (mut app, root) = sample_app("ascii");
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
    fn missing_results_still_render_the_method_sections() {
        let cli = Cli::parse_from(["climb-tui"]);
        let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
        app.results_path = PathBuf::from("target/definitely-missing.json");
        app.set_data(data::load(&app.results_path));
        app.update(crate::app::Action::SelectTab(Tab::Method.index()));
        let out = render_to_string(&app, 120, 40);
        for needle in [
            "Scope",
            "Known limits",
            "no results loaded",
            "embedded copy",
        ] {
            assert!(
                out.contains(needle),
                "an unloaded Method tab still explains itself: {needle:?}\n{out}"
            );
        }
    }

    #[test]
    fn a_present_but_invalid_sibling_is_scrolled_into_view() {
        let (mut app, root) = sample_app("invalid-sibling");
        std::fs::write(root.join("target/wire-report.json"), "{ not json").expect("write garbage");
        app.set_data(data::load(&app.results_path));
        // Scroll until the sampling section's wire-report row is on screen.
        let mut out = render_to_string(&app, 120, 40);
        for _ in 0..600 {
            if out.contains("wire report") {
                break;
            }
            app.update(crate::app::Action::LadderDown);
            out = render_to_string(&app, 120, 40);
        }
        assert!(
            out.contains("wire report") && out.contains("unreadable"),
            "an unreadable sibling names itself in the document:\n{out}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_theme_renders_the_tab() {
        let (mut app, root) = sample_app("themes");
        for theme in [ThemeId::Dark, ThemeId::Light, ThemeId::Contrast] {
            app.theme = theme;
            let out = render_to_string(&app, 120, 40);
            assert!(out.contains("Scope"), "theme {theme:?}:\n{out}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scroll_start_clamps_and_never_runs_past_the_end() {
        assert_eq!(scroll_start(10, 4, 0), 0);
        assert_eq!(scroll_start(10, 4, 3), 3);
        assert_eq!(
            scroll_start(10, 4, 6),
            6,
            "the last full window is rows 7-10"
        );
        assert_eq!(scroll_start(10, 4, 100), 6, "clamped to the bottom");
        assert_eq!(scroll_start(2, 4, 5), 0, "a short document never scrolls");
    }

    #[test]
    fn sanitize_maps_the_plain_glyphs_only_in_ascii_mode() {
        assert_eq!(sanitize("a\u{2014}b\u{b7}c", false), "a\u{2014}b\u{b7}c");
        assert_eq!(sanitize("a\u{2014}b\u{b7}c", true), "a-b-c");
        assert_eq!(sanitize("x\u{d7}y\u{2248}z", true), "xxy~z");
    }

    #[test]
    fn every_section_title_fits_the_contents_column() {
        let inner = (TOC_WIDTH as usize).saturating_sub(2);
        for title in MethodView::SECTION_TITLES {
            // The marker plus a space precedes every title.
            assert!(
                title.chars().count() + 2 <= inner,
                "title {title:?} needs {} columns but only {inner} are inside the block",
                title.chars().count() + 2
            );
        }
    }
}
