//! `cargo xtask docs-check`: mechanically enforce the documentation's two
//! honesty invariants over `README.md` and the top-level `docs/*.md` files.
//!
//! 1. **Mermaid node names.** Every node identifier inside a fenced
//!    ` ```mermaid ` block that *looks like* a workspace crate or binary name
//!    (`climb_*` or `xtask`, compared case- and hyphen/underscore-insensitively)
//!    must be one of the real names reported by `cargo metadata`. Quoted label
//!    text is not scanned: infrastructure names such as the `climb-net` Docker
//!    network or `climb-builder` image tags legitimately share the prefix. The
//!    one infrastructure name that is a plausible node id is allowlisted
//!    explicitly (`NON_CRATE_IDS`), so the rule stays "every crate-like node id
//!    is a real crate or binary".
//! 2. **Numeric results.** Every number immediately followed by a result unit
//!    (`B`, `KiB`, `MiB`, `GiB`, `s`, `ms`, `µs`, `us`, `min`, `h`, `%`, `×`)
//!    must appear as a Value+Unit pair in some row of the committed quote sheet
//!    (`docs/deck-facts.md`), which is therefore excluded from its own scan.
//!    Versions (`3.4.1`), glued algorithm names (`SHA2-128s`) and hyphenated
//!    prose constants (`1232-byte`) fall outside the token grammar by
//!    construction; a line can opt out entirely with the marker
//!    `<!-- docs-check:allow-number -->`.
//!
//! The README's headline table (between the `<!-- headline:begin -->` and
//! `<!-- headline:end -->` markers) is checked one level further: each row's
//! value, unit, evidence class and baseline must match a quote-sheet row, so
//! the table cannot hand-type a number, a class or a baseline the sheet does
//! not carry.
//!
//! The README's badge block (between the `<!-- badges:begin -->` and
//! `<!-- badges:end -->` markers) is generated, so its numbers are verified by
//! regeneration instead of by the quote sheet: the block is rebuilt in memory
//! from the badge configuration and the claims registry, and a block that
//! differs from the fresh generation is a violation with a diff. The block's
//! own lines are exempt from the numeric-result rule — and only those lines;
//! the rule itself is unchanged for every other line of every document.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::readme_badges;
use climb_report::{BOUND_POINTER, CONSOLIDATE_COMMAND};

/// Marker line that exempts its own line from the numeric-result check.
const ALLOW_NUMBER_MARKER: &str = "<!-- docs-check:allow-number -->";

/// Markers bracketing the generated headline table in the README.
const HEADLINE_BEGIN: &str = "<!-- headline:begin -->";
const HEADLINE_END: &str = "<!-- headline:end -->";

/// Markers bracketing the numbers-of-record table in the progress log.
const NUMBERS_BEGIN: &str = "<!-- numbers-of-record:begin -->";
const NUMBERS_END: &str = "<!-- numbers-of-record:end -->";

/// The quote sheet, relative to the workspace root. It is the source of truth
/// for quotable numbers, so it is the one documentation file not scanned.
const QUOTE_SHEET: &str = "docs/deck-facts.md";

/// The committed consolidated results artifact that documentation is built on.
const SAMPLE_RESULTS: &str = "docs/sample-output/climb-results.json";

/// The progress-log filename, referenced where its numbers-of-record table is
/// read and labelled in tests.
const PROGRESS_FILE: &str = "the dev notes"; // source-hygiene:allow

/// Letter units that mark a number as a recorded result when they follow it
/// after whitespace. Percent and the multiplication sign count glued too.
/// Byte units beyond the sheet's own `B` are included so a result cannot hide
/// behind a unit spelling the sheet never uses.
const RESULT_UNITS: &[&str] = &[
    "B", "kB", "KB", "MB", "MiB", "GB", "GiB", "s", "ms", "µs", "us", "min", "h", "%", "\u{d7}",
];

/// Infrastructure names that legitimately share the crate-name prefix but are
/// not crates; allowed as Mermaid node ids alongside the real workspace names.
const NON_CRATE_IDS: &[&str] = &["climb_net"];

/// Marker on a numbers-of-record row whose artifact was superseded, so its
/// value need not appear in the quote sheet. Case-insensitive substring match.
const HISTORICAL_TAG: &str = "historical";

/// One parsed row of the quote sheet's fact tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactRow {
    pub(crate) label: String,
    /// The Value cell, normalised: whitespace, thousands separators and the
    /// extrapolation marker `≈` removed. Non-numeric values stay as written.
    pub(crate) value: String,
    pub(crate) unit: String,
    pub(crate) class: String,
    pub(crate) baseline: String,
}

/// One documentation rule violation, located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Violation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

#[derive(Args)]
pub(crate) struct DocsCheckArgs {
    /// Workspace root holding `README.md`, `docs/` and `Cargo.toml`.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Quote sheet to check numeric results against, relative to the root.
    #[arg(long, default_value = QUOTE_SHEET)]
    pub(crate) quote_sheet: PathBuf,

    /// Consolidated results artifact whose signing speed-up bound is enforced.
    #[arg(long, default_value = SAMPLE_RESULTS)]
    pub(crate) results: PathBuf,
}

/// Normalise a crate/binary name for comparison: lowercase, `_` for `-`.
pub(crate) fn normalize_id(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

/// Parse every fact table row of the quote sheet. Lines that are not pipe
/// tables (headings, blockquotes, the footer) are skipped.
pub(crate) fn parse_quote_sheet(md: &str) -> Vec<FactRow> {
    let mut rows = Vec::new();
    for line in md.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        // | Label | Value | Unit | Evidence class | n / k | Baseline | ... |
        if cells.len() < 9 {
            continue;
        }
        let label = cells[1];
        let value = cells[2];
        if label == "Label" || value.is_empty() || value.chars().all(|c| c == '-') {
            continue;
        }
        rows.push(FactRow {
            label: label.to_string(),
            value: normalize_value(value),
            unit: cells[3].to_string(),
            class: cells[4].to_string(),
            baseline: cells[6].to_string(),
        });
    }
    rows
}

/// Strip whitespace, thousands separators and the `≈` marker from a value.
/// Non-numeric values (timestamps, `yes`, `powersave`) come back as written.
fn normalize_value(raw: &str) -> String {
    let filtered: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '\u{2248}')
        .collect();
    if filtered.is_empty() {
        return raw.trim().to_string();
    }
    if filtered.chars().all(|c| c.is_ascii_digit() || c == '.') {
        filtered
    } else {
        raw.trim().to_string()
    }
}

/// Find every result-shaped `(number, unit)` pair on a line: a number at a
/// word boundary followed (glued or spaced) by `%`/`×`, or followed after
/// whitespace by one of [`RESULT_UNITS`]. Versions (`3.4.1`), glued names
/// (`SHA2-128s`) and hyphen-suffixed constants (`1232-byte`) never qualify.
pub(crate) fn scan_result_numbers(line: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let boundary = i == 0 || {
            let prev = chars[i - 1];
            !(prev.is_alphanumeric() || matches!(prev, '_' | '-' | '.' | '/'))
        };
        if !boundary {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        while i + 1 < chars.len() && chars[i] == ',' && chars[i + 1].is_ascii_digit() {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
        let number: String = chars[start..i].iter().collect();
        let mut j = i;
        let mut spaced = false;
        while j < chars.len() && chars[j] == ' ' {
            spaced = true;
            j += 1;
        }
        let unit_start = j;
        while j < chars.len()
            && (chars[j].is_alphabetic() || chars[j] == '%' || chars[j] == '\u{d7}')
        {
            j += 1;
        }
        let unit: String = chars[unit_start..j].iter().collect();
        if unit.is_empty() {
            continue;
        }
        let flagged =
            unit == "%" || unit == "\u{d7}" || (spaced && RESULT_UNITS.contains(&unit.as_str()));
        if flagged {
            found.push((normalize_value(&number), unit));
        }
    }
    found
}

/// Blank out the contents of quoted spans so label text is never scanned as
/// if it were node ids.
fn strip_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_double = false;
    let mut in_single = false;
    for c in text.chars() {
        match c {
            '"' if !in_single => {
                in_double = !in_double;
                out.push(' ');
            }
            '\'' if !in_double => {
                in_single = !in_single;
                out.push(' ');
            }
            _ if in_double || in_single => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// The crate-like node ids of one Mermaid block: identifier tokens outside
/// quoted label text whose normalised form is `xtask` or starts `climb_`.
pub(crate) fn crate_like_node_ids(block: &str) -> Vec<String> {
    let stripped = strip_quoted(block);
    let mut ids = Vec::new();
    for token in stripped.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if token.is_empty() {
            continue;
        }
        let norm = token.to_ascii_lowercase();
        if norm == "xtask" || norm.starts_with("climb_") {
            ids.push(norm);
        }
    }
    ids
}

/// Check one documentation file's text against the quote sheet and the real
/// workspace names. Pure: no filesystem, no subprocesses.
pub(crate) fn check_file(
    file: &str,
    text: &str,
    facts: &[FactRow],
    known: &HashSet<String>,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    // (2) Numeric results: every number+unit pair must be in the quote sheet.
    // The generated badge block is exempt: its numbers are verified by
    // regenerating the block, not by the quote sheet, and only the block's
    // own lines are skipped.
    let badge_lines = badge_block_lines(text);
    for (idx, line) in text.lines().enumerate() {
        if line.contains(ALLOW_NUMBER_MARKER) {
            continue;
        }
        if let Some((begin, end)) = badge_lines {
            if idx >= begin && idx < end {
                continue;
            }
        }
        for (number, unit) in scan_result_numbers(line) {
            let known_value = facts.iter().any(|f| f.value == number && f.unit == unit);
            if !known_value {
                violations.push(Violation {
                    file: file.to_string(),
                    line: idx + 1,
                    message: format!(
                        "numeric result `{number} {unit}` is not in the quote sheet ({QUOTE_SHEET})"
                    ),
                });
            }
        }
    }

    // (1) Mermaid node names: crate-like ids must be real crates or binaries.
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].trim_start().starts_with("```mermaid") {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let mut block = String::new();
        while i < lines.len() && !lines[i].trim_start().starts_with("```") {
            block.push_str(lines[i]);
            block.push('\n');
            i += 1;
        }
        for id in crate_like_node_ids(&block) {
            if known.contains(&id) || NON_CRATE_IDS.contains(&id.as_str()) {
                continue;
            }
            violations.push(Violation {
                file: file.to_string(),
                line: start + 1,
                message: format!(
                    "Mermaid node `{id}` is not a real workspace crate or binary (see `cargo metadata`)"
                ),
            });
        }
        i += 1;
    }

    // Headline table, when present: value+unit+class+baseline must match a row.
    violations.extend(check_headline_block(file, text, facts));

    violations
}

/// Parse the README's marker-bracketed headline table. `None` when the
/// markers are absent; `Some(rows)` with `(line, value, class, baseline)`.
pub(crate) fn parse_headline_rows(text: &str) -> Option<Vec<(usize, String, String, String)>> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines.iter().position(|l| l.trim() == HEADLINE_BEGIN)?;
    let end = lines.iter().position(|l| l.trim() == HEADLINE_END)?;
    if end <= begin {
        return None;
    }
    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(begin + 1) {
        if idx >= end {
            break;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        // | Metric | Value | Evidence class | Baseline |
        if cells.len() < 5 {
            continue;
        }
        if cells[1] == "Metric" || cells[2] == "Value" || cells[2].starts_with("---") {
            continue;
        }
        rows.push((
            idx + 1,
            cells[2].to_string(),
            cells[3].to_string(),
            cells[4].to_string(),
        ));
    }
    Some(rows)
}

/// Parse the marker-bracketed numbers-of-record table of the progress log.
/// `None` when the markers are absent; `Some(rows)` with `(line, value_cell,
/// is_historical)`. The historical flag comes from a case-insensitive
/// `historical` tag in the row's label cell.
pub(crate) fn parse_numbers_of_record_rows(text: &str) -> Option<Vec<(usize, String, bool)>> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines.iter().position(|l| l.trim() == NUMBERS_BEGIN)?;
    let end = lines.iter().position(|l| l.trim() == NUMBERS_END)?;
    if end <= begin {
        return None;
    }
    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(begin + 1) {
        if idx >= end {
            break;
        }
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        // | Number | Value | Source artifact | Phase |
        if cells.len() < 5 {
            continue;
        }
        let label = cells[1];
        let value = cells[2];
        if label == "Number" || value.starts_with("---") || value.is_empty() {
            continue;
        }
        let historical = label.to_ascii_lowercase().contains(HISTORICAL_TAG);
        rows.push((idx + 1, value.to_string(), historical));
    }
    Some(rows)
}

/// Check the numbers-of-record table in the progress log: every value+unit
/// token in a row's value cell must appear in the quote sheet, unless the row
/// is tagged historical. Pure: no filesystem, no subprocesses.
pub(crate) fn check_numbers_of_record(file: &str, text: &str, facts: &[FactRow]) -> Vec<Violation> {
    let Some(rows) = parse_numbers_of_record_rows(text) else {
        return Vec::new();
    };
    let mut violations = Vec::new();
    for (line, value, historical) in rows {
        if historical {
            continue;
        }
        for (number, unit) in scan_result_numbers(&value) {
            let known_value = facts.iter().any(|f| f.value == number && f.unit == unit);
            if !known_value {
                violations.push(Violation {
                    file: file.to_string(),
                    line,
                    message: format!(
                        "numbers-of-record value `{number} {unit}` is not in the quote sheet \
                         ({QUOTE_SHEET}); correct the row, or tag it historical"
                    ),
                });
            }
        }
    }
    violations
}

fn check_headline_block(file: &str, text: &str, facts: &[FactRow]) -> Vec<Violation> {
    let Some(rows) = parse_headline_rows(text) else {
        return Vec::new();
    };
    let mut violations = Vec::new();
    for (line, value, class, baseline) in rows {
        let pairs = scan_result_numbers(&value);
        let Some((number, unit)) = pairs.first() else {
            violations.push(Violation {
                file: file.to_string(),
                line,
                message: format!("headline row value `{value}` carries no number+unit pair"),
            });
            continue;
        };
        let matched = facts.iter().any(|f| {
            f.value == *number && f.unit == *unit && f.class == class && f.baseline == baseline
        });
        if !matched {
            violations.push(Violation {
                file: file.to_string(),
                line,
                message: format!(
                    "headline row `{value} | {class} | {baseline}` does not match any quote-sheet row"
                ),
            });
        }
    }
    violations
}

/// Markers bracketing the README's logo header block.
const LOGO_BEGIN: &str = "<!-- logo:begin -->";
const LOGO_END: &str = "<!-- logo:end -->";

/// Markers bracketing the README's "Built in the open" commit-graph block.
const COMMIT_GRAPH_BEGIN: &str = "<!-- commit-graph:begin -->";
const COMMIT_GRAPH_END: &str = "<!-- commit-graph:end -->";

/// Markers bracketing the compact strip that sits directly under the badges.
const COMMIT_GRAPH_STRIP_BEGIN: &str = "<!-- commit-graph-strip:begin -->";
const COMMIT_GRAPH_STRIP_END: &str = "<!-- commit-graph-strip:end -->";

/// The fragments of the commit-graph caption that state the provenance a
/// reader is owed: the picture comes from the public history, and later
/// commits appear after the next refresh. Matched case-insensitively as
/// substrings so the surrounding wording can change without silently dropping
/// the provenance note.
const COMMIT_GRAPH_CAPTION: &str = "from the public history of";
const COMMIT_GRAPH_CAPTION_TAIL: &str = "later commits appear after the next refresh";

/// The two cards the README shows: one graphic per viewport, never both at
/// once. The narrow source is selected under the phone breakpoint and the wide
/// card is the fallback every renderer understands.
const COMMIT_GRAPH_WIDE: &str = "docs/media/commit-graph.svg";
const COMMIT_GRAPH_NARROW: &str = "docs/media/commit-graph-narrow.svg";
const COMMIT_GRAPH_MEDIA: &str = "(max-width: 600px)";

/// The two compact strips: one per viewport, like the cards.
const COMMIT_GRAPH_STRIP: &str = "docs/media/commit-graph-compact.svg";
const COMMIT_GRAPH_STRIP_NARROW: &str = "docs/media/commit-graph-compact-narrow.svg";

/// The line range of the managed badge block, exclusive of the markers.
/// `None` when the markers are absent or the end marker comes first.
pub(crate) fn badge_block_lines(text: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.trim() == readme_badges::BADGES_BEGIN)?;
    let end = lines
        .iter()
        .position(|line| line.trim() == readme_badges::BADGES_END)?;
    if end <= begin {
        return None;
    }
    Some((begin + 1, end))
}

/// Check the generated badge block of the README.
///
/// The block is rebuilt in memory from the badge configuration and the claims
/// registry of the results artifact; a committed block that differs from the
/// fresh generation is a violation carrying the diff. A missing configuration
/// or a missing results artifact is a violation too: the block cannot be
/// verified without them.
pub(crate) fn check_badge_block(file: &str, readme_text: &str, root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    let config_path = root.join(readme_badges::DEFAULT_CONFIG);
    let results_path = root.join(readme_badges::DEFAULT_RESULTS);
    let begin_line = readme_text
        .lines()
        .position(|line| line.trim() == readme_badges::BADGES_BEGIN)
        .map(|index| index + 1)
        .unwrap_or(1);

    if !config_path.exists() {
        violations.push(Violation {
            file: file.to_string(),
            line: begin_line,
            message: format!(
                "the badge configuration `{}` is missing, so the badge block cannot be verified",
                config_path.display()
            ),
        });
        return violations;
    }
    if !results_path.exists() {
        violations.push(Violation {
            file: file.to_string(),
            line: begin_line,
            message: format!(
                "the results artifact `{}` is missing, so the badge block cannot be verified",
                results_path.display()
            ),
        });
        return violations;
    }

    let (_, generated, _) = match readme_badges::generate(&config_path, &results_path) {
        Ok(generated) => generated,
        Err(error) => {
            violations.push(Violation {
                file: file.to_string(),
                line: begin_line,
                message: format!("the badge block could not be regenerated: {error:#}"),
            });
            return violations;
        }
    };

    let Some(committed) = readme_badges::extract_block(readme_text) else {
        violations.push(Violation {
            file: file.to_string(),
            line: begin_line,
            message: format!(
                "the README carries no managed badge block (`{}` / `{}`)",
                readme_badges::BADGES_BEGIN,
                readme_badges::BADGES_END
            ),
        });
        return violations;
    };

    if committed.trim_end() != generated.trim_end() {
        let diff = readme_badges::render_diff(&readme_badges::diff_lines(&generated, &committed));
        violations.push(Violation {
            file: file.to_string(),
            line: begin_line,
            message: format!("the managed badge block has drifted from the generated one:\n{diff}"),
        });
    }
    violations
}

/// The body of the marker-bracketed logo block, and the line it starts on.
/// `None` when a marker is missing or the end marker comes first.
pub(crate) fn parse_logo_block(text: &str) -> Option<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines.iter().position(|line| line.trim() == LOGO_BEGIN)?;
    let end = lines.iter().position(|line| line.trim() == LOGO_END)?;
    if end <= begin {
        return None;
    }
    Some((begin + 1, lines[begin + 1..end].join("\n")))
}

/// Check a document's logo header block: both markers must be present, in
/// order, and the block must name at least one image. Pure: no filesystem, no
/// subprocesses.
pub(crate) fn check_logo_block(file: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    for marker in [LOGO_BEGIN, LOGO_END] {
        if !text.contains(marker) {
            violations.push(Violation {
                file: file.to_string(),
                line: 1,
                message: format!("missing the `{marker}` logo header block marker"),
            });
        }
    }
    if !violations.is_empty() {
        return violations;
    }
    let Some((line, body)) = parse_logo_block(text) else {
        violations.push(Violation {
            file: file.to_string(),
            line: 1,
            message: format!("the `{LOGO_BEGIN}` marker comes after the `{LOGO_END}` marker"),
        });
        return violations;
    };
    if extract_image_paths(&body).is_empty() {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the logo block names no image".to_string(),
        });
    }
    violations
}

/// The body of the marker-bracketed commit-graph block of the README, and the
/// line it starts on. `None` when a marker is missing or the end marker comes
/// first.
pub(crate) fn parse_commit_graph_block(text: &str) -> Option<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.trim() == COMMIT_GRAPH_BEGIN)?;
    let end = lines
        .iter()
        .position(|line| line.trim() == COMMIT_GRAPH_END)?;
    if end <= begin {
        return None;
    }
    Some((begin + 1, lines[begin + 1..end].join("\n")))
}

/// Check the README's commit-graph block: both markers must be present and in
/// order, the block must name the full card (an image), it must carry the
/// public-history provenance caption, and when `expected_commit_link` is known
/// the block must link to that public commit permalink. The image path itself
/// is checked by [`check_image_paths`]. Pure: no filesystem, no subprocesses.
pub(crate) fn check_commit_graph_block(
    file: &str,
    text: &str,
    expected_commit_link: Option<&str>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    for marker in [COMMIT_GRAPH_BEGIN, COMMIT_GRAPH_END] {
        if !text.contains(marker) {
            violations.push(Violation {
                file: file.to_string(),
                line: 1,
                message: format!("missing the `{marker}` commit-graph block marker"),
            });
        }
    }
    if !violations.is_empty() {
        return violations;
    }
    let Some((line, body)) = parse_commit_graph_block(text) else {
        violations.push(Violation {
            file: file.to_string(),
            line: 1,
            message: format!(
                "the `{COMMIT_GRAPH_BEGIN}` marker comes after the `{COMMIT_GRAPH_END}` marker"
            ),
        });
        return violations;
    };
    if extract_image_paths(&body).is_empty() {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the commit-graph block names no image".to_string(),
        });
    }
    // One card per viewport: the narrow card is selected by a media query and
    // the wide card is the fallback. Both must be named, and the block must
    // not reference the retired compact strip.
    if !body.contains("<picture") {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the commit-graph block is not a <picture> with one card per viewport"
                .to_string(),
        });
    }
    if !body.contains(COMMIT_GRAPH_NARROW) || !body.contains(COMMIT_GRAPH_MEDIA) {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: format!(
                "the commit-graph block does not select {COMMIT_GRAPH_NARROW} under \
                 `{COMMIT_GRAPH_MEDIA}`"
            ),
        });
    }
    if !body.contains(COMMIT_GRAPH_WIDE) {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: format!(
                "the commit-graph block does not name the wide card {COMMIT_GRAPH_WIDE}"
            ),
        });
    }
    if body.contains("commit-graph-compact") {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the full-card block names a compact strip; the strip belongs in its own \
                      `<!-- commit-graph-strip:begin -->` block"
                .to_string(),
        });
    }
    let lower = body.to_ascii_lowercase();
    let lower = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    if !lower.contains(COMMIT_GRAPH_CAPTION) || !lower.contains(COMMIT_GRAPH_CAPTION_TAIL) {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: format!(
                "the commit-graph block carries no public-history provenance caption \
                 (must state that the image comes {COMMIT_GRAPH_CAPTION} \
                 this repository and that {COMMIT_GRAPH_CAPTION_TAIL})"
            ),
        });
    }
    if let Some(expected) = expected_commit_link {
        if !href_targets(&body).iter().any(|href| href == expected) {
            violations.push(Violation {
                file: file.to_string(),
                line,
                message: format!(
                    "the commit-graph block does not link to the public as-of commit ({expected})"
                ),
            });
        }
    }
    violations
}

/// The body of the marker-bracketed compact-strip block of the README, and the
/// line it starts on. `None` when a marker is missing or the end marker comes
/// first.
pub(crate) fn parse_commit_graph_strip_block(text: &str) -> Option<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.trim() == COMMIT_GRAPH_STRIP_BEGIN)?;
    let end = lines
        .iter()
        .position(|line| line.trim() == COMMIT_GRAPH_STRIP_END)?;
    if end <= begin {
        return None;
    }
    Some((begin + 1, lines[begin + 1..end].join("\n")))
}

/// Check the compact strip that sits under the badges: both markers must be
/// present and in order, the block must select the narrow strip on phones and
/// fall back to the wide strip, link to the public as-of commit, and describe
/// the picture without quoting a number in its alt text. Pure: no filesystem,
/// no subprocesses.
pub(crate) fn check_commit_graph_strip_block(
    file: &str,
    text: &str,
    expected_commit_link: Option<&str>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    for marker in [COMMIT_GRAPH_STRIP_BEGIN, COMMIT_GRAPH_STRIP_END] {
        if !text.contains(marker) {
            violations.push(Violation {
                file: file.to_string(),
                line: 1,
                message: format!("missing the `{marker}` commit-graph strip marker"),
            });
        }
    }
    if !violations.is_empty() {
        return violations;
    }
    let Some((line, body)) = parse_commit_graph_strip_block(text) else {
        violations.push(Violation {
            file: file.to_string(),
            line: 1,
            message: format!(
                "the `{COMMIT_GRAPH_STRIP_BEGIN}` marker comes after the \
                 `{COMMIT_GRAPH_STRIP_END}` marker"
            ),
        });
        return violations;
    };
    if !body.contains("<picture") {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the commit-graph strip is not a <picture> with one strip per viewport"
                .to_string(),
        });
    }
    if !body.contains(COMMIT_GRAPH_STRIP_NARROW) || !body.contains(COMMIT_GRAPH_MEDIA) {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: format!(
                "the commit-graph strip does not select {COMMIT_GRAPH_STRIP_NARROW} under \
                 `{COMMIT_GRAPH_MEDIA}`"
            ),
        });
    }
    if !body.contains(COMMIT_GRAPH_STRIP) {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: format!(
                "the commit-graph strip does not name the wide strip {COMMIT_GRAPH_STRIP}"
            ),
        });
    }
    if let Some(expected) = expected_commit_link {
        if !href_targets(&body).iter().any(|href| href == expected) {
            violations.push(Violation {
                file: file.to_string(),
                line,
                message: format!(
                    "the commit-graph strip does not link to the public as-of commit ({expected})"
                ),
            });
        }
    }
    let mut alt_seen = false;
    for tag in body.split('<') {
        let tag = format!("<{tag}");
        for alt in html_attribute(&tag, "alt") {
            alt_seen = true;
            if alt.chars().any(|character| character.is_ascii_digit()) {
                violations.push(Violation {
                    file: file.to_string(),
                    line,
                    message: format!(
                        "the commit-graph strip alt text `{alt}` quotes a number; the picture \
                         may change before the prose does"
                    ),
                });
            }
        }
    }
    if !alt_seen {
        violations.push(Violation {
            file: file.to_string(),
            line,
            message: "the commit-graph strip carries no alt text".to_string(),
        });
    }
    violations
}

/// Check where the compact strip sits: directly under the badges and before
/// the full-card block. Only blank lines may separate the badge block from the
/// strip. Pure: no filesystem, no subprocesses.
pub(crate) fn check_commit_graph_strip_placement(file: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let line_of = |needle: &str| lines.iter().position(|line| line.trim() == needle);
    let Some(badges_end) = line_of(readme_badges::BADGES_END) else {
        return violations;
    };
    let Some(strip_begin) = line_of(COMMIT_GRAPH_STRIP_BEGIN) else {
        return violations;
    };
    let Some(strip_end) = line_of(COMMIT_GRAPH_STRIP_END) else {
        return violations;
    };
    if strip_begin <= badges_end {
        violations.push(Violation {
            file: file.to_string(),
            line: strip_begin + 1,
            message: format!(
                "the `{COMMIT_GRAPH_STRIP_BEGIN}` block must come after `{}`",
                readme_badges::BADGES_END
            ),
        });
    } else if lines[badges_end + 1..strip_begin]
        .iter()
        .any(|line| !line.trim().is_empty())
    {
        violations.push(Violation {
            file: file.to_string(),
            line: strip_begin + 1,
            message: format!(
                "the `{COMMIT_GRAPH_STRIP_BEGIN}` block must sit directly under `{}`, \
                 separated only by blank lines",
                readme_badges::BADGES_END
            ),
        });
    }
    if let Some(graph_begin) = line_of(COMMIT_GRAPH_BEGIN) {
        if strip_end >= graph_begin {
            violations.push(Violation {
                file: file.to_string(),
                line: strip_end + 1,
                message: format!(
                    "the `{COMMIT_GRAPH_STRIP_END}` block must come before `{COMMIT_GRAPH_BEGIN}`"
                ),
            });
        }
    }
    violations
}

/// The public commit permalink the README block must carry, taken from the
/// committed document and the current configuration. `None` when either is
/// unreadable; the commit-graph check itself reports that failure.
fn expected_commit_link(root: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(crate::commit_graph::JSON_PATH)).ok()?;
    let graph: crate::commit_graph::CommitGraph = serde_json::from_str(&text).ok()?;
    let config =
        crate::commit_graph::load_config(&root.join(crate::commit_graph::CONFIG_PATH)).ok()?;
    Some(crate::commit_graph::commit_permalink(
        &config,
        &graph.as_of_sha,
    ))
}

/// The values of every `href="..."` attribute in a fragment.
fn href_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("href=\"") {
        let start = cursor + offset + "href=\"".len();
        let Some(end) = text[start..].find('"') else {
            break;
        };
        targets.push(text[start..start + end].to_string());
        cursor = start + end + 1;
    }
    targets
}

/// The image targets of a markdown line: `![alt](target)` images, with the
/// optional angle-bracket form and the optional title both handled.
fn markdown_image_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = line[cursor..].find("![") {
        let bang = cursor + offset;
        let after = bang + 2;
        let Some(alt_end) = line[after..].find(']') else {
            break;
        };
        let paren = after + alt_end + 1;
        let Some(rest) = line[paren..].strip_prefix('(') else {
            cursor = paren;
            continue;
        };
        let Some(close) = rest.find(')') else {
            break;
        };
        let raw = rest[..close].trim();
        let target = raw
            .strip_prefix('<')
            .and_then(|inner| inner.strip_suffix('>'))
            .unwrap_or(raw)
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if !target.is_empty() {
            targets.push(target.to_string());
        }
        cursor = paren + 1 + close + 1;
    }
    targets
}

/// The value of an HTML attribute inside a tag. The attribute name must be a
/// whole word, so `data-src=` is not read as `src=`.
fn html_attribute(tag: &str, name: &str) -> Vec<String> {
    let mut values = Vec::new();
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find(&needle) {
        let start = cursor + offset;
        if start > 0 && !lower.as_bytes()[start - 1].is_ascii_whitespace() {
            cursor = start + needle.len();
            continue;
        }
        let value_start = start + needle.len();
        let quote = tag[value_start..].chars().next();
        let (value, next) = match quote {
            Some('"') => match tag[value_start + 1..].find('"') {
                Some(end) => (
                    tag[value_start + 1..value_start + 1 + end].to_string(),
                    value_start + 1 + end + 1,
                ),
                None => break,
            },
            Some('\'') => match tag[value_start + 1..].find('\'') {
                Some(end) => (
                    tag[value_start + 1..value_start + 1 + end].to_string(),
                    value_start + 1 + end + 1,
                ),
                None => break,
            },
            _ => {
                let end = tag[value_start..]
                    .find(char::is_whitespace)
                    .unwrap_or(tag.len() - value_start);
                (
                    tag[value_start..value_start + end].to_string(),
                    value_start + end,
                )
            }
        };
        values.push(value);
        cursor = next;
    }
    values
}

/// The candidate files of a `srcset` attribute: the paths, without the density
/// descriptors that sit next to them.
fn srcset_candidates(value: &str) -> Vec<String> {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|candidate| !candidate.is_empty())
        .filter(|candidate| candidate.contains('.'))
        .filter(|candidate| match candidate.strip_suffix('x') {
            Some(number) => {
                !(number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.'))
            }
            None => true,
        })
        .map(str::to_string)
        .collect()
}

/// The image sources of an HTML line: the `src` and `srcset` attributes of
/// every `<img>` and `<source>` element.
fn html_image_sources(line: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let lower = line.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find('<') {
        let start = cursor + offset;
        let name_start = start + 1;
        let rest = &lower[name_start..];
        let name_len = if rest.starts_with("img") {
            3
        } else if rest.starts_with("source") {
            6
        } else {
            0
        };
        let boundary = rest
            .chars()
            .nth(name_len)
            .is_none_or(|c| c.is_ascii_whitespace() || c == '>' || c == '/');
        if name_len == 0 || !boundary {
            cursor = name_start;
            continue;
        }
        let Some(close) = line[start..].find('>') else {
            break;
        };
        let tag = &line[start..start + close];
        sources.extend(html_attribute(tag, "src"));
        for value in html_attribute(tag, "srcset") {
            sources.extend(srcset_candidates(&value));
        }
        cursor = start + close + 1;
    }
    sources
}

/// Every image path a document names, with the line it appears on. Markdown
/// images and the `<img>` / `<source>` elements of raw HTML are both read.
pub(crate) fn extract_image_paths(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for path in markdown_image_targets(line) {
            found.push((index + 1, path));
        }
        for path in html_image_sources(line) {
            found.push((index + 1, path));
        }
    }
    found
}

/// Join a document-relative path onto the document's own directory and
/// normalise the `.` and `..` segments.
fn resolve_doc_path(dir_prefix: &str, path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in dir_prefix.split('/').chain(path.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Whether a path names something outside the repository: an absolute path, a
/// protocol-relative URL or a URL with a scheme.
fn is_external_path(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    match path.find(':') {
        Some(index) => path[..index]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'),
        None => false,
    }
}

/// Check every relative image path a document names: it must resolve to a
/// file tracked in the repository. Paths outside the repository are skipped.
/// Pure: the tracked-file set is passed in.
pub(crate) fn check_image_paths(
    file: &str,
    dir_prefix: &str,
    text: &str,
    tracked: &HashSet<String>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (line, raw) in extract_image_paths(text) {
        let path = raw.split('#').next().unwrap_or_default().trim();
        if path.is_empty() || is_external_path(path) {
            continue;
        }
        let resolved = resolve_doc_path(dir_prefix, path);
        if !tracked.contains(&resolved) {
            violations.push(Violation {
                file: file.to_string(),
                line,
                message: format!(
                    "image path `{path}` resolves to `{resolved}`, which is not a tracked file"
                ),
            });
        }
    }
    violations
}

/// The repository paths tracked in git, relative to the workspace root.
pub(crate) fn tracked_files(root: &Path) -> Result<HashSet<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .context("failed to run `git ls-files`")?;
    if !output.status.success() {
        bail!(
            "`git ls-files` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

/// Every markdown file under a directory, recursively.
fn markdown_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current)
            .with_context(|| format!("failed to read {}", current.display()))?
        {
            let entry = entry
                .with_context(|| format!("failed to read an entry of {}", current.display()))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The real crate and binary names of the workspace, from `cargo metadata`.
pub(crate) fn workspace_names(root: &Path) -> Result<HashSet<String>> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .context("failed to run `cargo metadata`")?;
    if !output.status.success() {
        bail!(
            "`cargo metadata` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("failed to parse `cargo metadata` output")?;
    let mut names = HashSet::new();
    let packages = meta["packages"]
        .as_array()
        .context("`cargo metadata` carried no packages")?;
    for pkg in packages {
        if let Some(name) = pkg["name"].as_str() {
            names.insert(normalize_id(name));
        }
        for target in pkg["targets"].as_array().into_iter().flatten() {
            let is_bin = target["kind"]
                .as_array()
                .map(|kinds| kinds.iter().any(|k| k == "bin"))
                .unwrap_or(false);
            if is_bin {
                if let Some(name) = target["name"].as_str() {
                    names.insert(normalize_id(name));
                }
            }
        }
    }
    Ok(names)
}

/// Treat the signing speed-up bound in a consolidated results artifact as a
/// hard honesty claim.
///
/// Two failure modes, both fatal:
///
/// * the artifact carries no bound check at all — the bound was never
///   checked, so nothing in the documentation may rest on it. The message
///   names the command that regenerates the artifact.
/// * the bound was checked and violated — the documented numbers must not
///   rest on a signing speed-up that exceeds the 1.05xN bound. Every
///   offending ratio is named.
fn check_signing_bound(results_path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(results_path)
        .with_context(|| format!("failed to read {}", results_path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", results_path.display()))?;
    // An absent field — and a field that is present but not a boolean — means
    // the bound was never checked. It is not a pass.
    let Some(bound_ok) = doc.pointer(BOUND_POINTER).and_then(|value| value.as_bool()) else {
        bail!(
            "{}: the results artifact carries no `{BOUND_POINTER}` — the signing speed-up \
             bound was never checked; regenerate it with `{CONSOLIDATE_COMMAND}`",
            results_path.display()
        );
    };
    if bound_ok {
        return Ok(());
    }
    let offenders: Vec<String> = doc
        .pointer("/derived/bound_offenders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    bail!(
        "{}: signing speed-up bound violated (bound_ok=false){}",
        results_path.display(),
        if offenders.is_empty() {
            String::new()
        } else {
            format!(" — {}", offenders.join("; "))
        }
    );
}

pub(crate) fn cmd_docs_check(args: DocsCheckArgs) -> Result<()> {
    let sheet_path = args.root.join(&args.quote_sheet);
    let sheet_text = std::fs::read_to_string(&sheet_path)
        .with_context(|| format!("failed to read {}", sheet_path.display()))?;
    let facts = parse_quote_sheet(&sheet_text);
    if facts.is_empty() {
        bail!(
            "{} carries no fact rows — regenerate it with `cargo xtask deck-facts`",
            sheet_path.display()
        );
    }

    let known = workspace_names(&args.root)?;

    let readme_path = args.root.join("README.md");
    let readme_text = std::fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    if !readme_text.contains(HEADLINE_BEGIN) || !readme_text.contains(HEADLINE_END) {
        bail!(
            "{} is missing the `{HEADLINE_BEGIN}` / `{HEADLINE_END}` headline table markers",
            readme_path.display()
        );
    }

    let docs_dir = args.root.join("docs");
    let mut doc_files = vec![readme_path.clone()];
    let sheet_name = args.quote_sheet.file_name().unwrap_or_default();
    for entry in std::fs::read_dir(&docs_dir)
        .with_context(|| format!("failed to read {}", docs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read an entry of {}", docs_dir.display()))?;
        let path = entry.path();
        let is_md = path.extension().is_some_and(|ext| ext == "md");
        let is_sheet = path.file_name().is_some_and(|name| name == sheet_name);
        if is_md && !is_sheet {
            doc_files.push(path);
        }
    }
    doc_files.sort();

    let mut violations = Vec::new();
    for path in &doc_files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let label = path
            .strip_prefix(&args.root)
            .unwrap_or(path)
            .display()
            .to_string();
        violations.extend(check_file(&label, &text, &facts, &known));
    }

    // The README's logo header block is marker-bracketed and must name an
    // image; every relative image path in the documentation must resolve to a
    // file tracked in the repository.
    let tracked = tracked_files(&args.root)?;
    let readme_label = readme_path
        .strip_prefix(&args.root)
        .unwrap_or(&readme_path)
        .display()
        .to_string();
    violations.extend(check_logo_block(&readme_label, &readme_text));

    // The README's "Built in the open" block is marker-bracketed, must name the
    // full commit-graph card with its public-history provenance caption, and
    // must link to the public as-of commit; the image path itself is checked
    // with every other relative image path below.
    violations.extend(check_commit_graph_block(
        &readme_label,
        &readme_text,
        expected_commit_link(&args.root).as_deref(),
    ));

    // The compact strip is a separate managed block directly under the badges,
    // with its own image pair and no numbers in its alt text.
    violations.extend(check_commit_graph_strip_block(
        &readme_label,
        &readme_text,
        expected_commit_link(&args.root).as_deref(),
    ));
    violations.extend(check_commit_graph_strip_placement(
        &readme_label,
        &readme_text,
    ));

    // The badge block is generated: rebuild it in memory and fail when the
    // committed copy has drifted from the fresh generation.
    violations.extend(check_badge_block(&readme_label, &readme_text, &args.root));

    let mut image_files = vec![readme_path.clone()];
    image_files.extend(markdown_files(&args.root.join("docs"))?);
    for path in &image_files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let relative = path.strip_prefix(&args.root).unwrap_or(path);
        let label = relative.display().to_string();
        let dir_prefix = relative
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default();
        violations.extend(check_image_paths(&label, &dir_prefix, &text, &tracked));
    }

    // The progress log's numbers-of-record table is scanned too (its rows are
    // checked against the quote sheet), but the log is not a documentation
    // file for the numeric-result and Mermaid rules — only its tagged table is.
    let progress_path = args.root.join(PROGRESS_FILE);
    if progress_path.exists() {
        let text = std::fs::read_to_string(&progress_path)
            .with_context(|| format!("failed to read {}", progress_path.display()))?;
        let label = progress_path
            .strip_prefix(&args.root)
            .unwrap_or(&progress_path)
            .display()
            .to_string();
        violations.extend(check_numbers_of_record(&label, &text, &facts));
    }

    // The committed commit graph is regenerated from its own as-of commit, and
    // both cards are checked for shape, size, accessibility text and external
    // references. Staleness is a warning because a graph one commit behind is
    // the normal state right after a commit.
    let graph = crate::commit_graph::check_docs(&args.root)?;
    for warning in &graph.warnings {
        eprintln!("warning: {warning}");
    }
    if let Some(note) = &graph.note {
        eprintln!("note: {note}");
    }
    let graph_violations = graph.violations;

    // The signing speed-up bound is a hard honesty claim: a consolidated
    // artifact whose reported speed-up exceeds 1.05xN must not back the
    // documented numbers. `bound_ok == false` is treated as a failing claim.
    check_signing_bound(&args.root.join(&args.results))?;

    if violations.is_empty() && graph_violations.is_empty() {
        // A single command guards the whole documentation surface, so the
        // source-tree hygiene scan and the pinned-dependency agreement check
        // run alongside the documentation rules.
        crate::source_hygiene::run(&args.root).context("source-hygiene scan failed")?;
        crate::check_pins::run(&args.root).context("pin agreement check failed")?;
        println!(
            "docs-check: {} documentation file(s) clean against {} ({} facts)",
            doc_files.len(),
            sheet_path.display(),
            facts.len()
        );
        return Ok(());
    }
    for violation in &violations {
        eprintln!("{violation}");
    }
    for message in &graph_violations {
        eprintln!("{message}");
    }
    bail!(
        "docs-check: {} violation(s)",
        violations.len() + graph_violations.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a minimal results document with the given `bound_ok` flag and
    /// offenders, and return its path for the bound guard to read.
    fn write_results(dir: &Path, bound_ok: bool, offenders: &[&str]) -> PathBuf {
        let doc = serde_json::json!({
            "derived": {
                "bound_ok": bound_ok,
                "bound_offenders": offenders,
            }
        });
        let path = dir.join("climb-results.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json"))
            .expect("write temp results");
        path
    }

    #[test]
    fn the_bound_guard_passes_when_the_bound_holds() {
        let dir = std::env::temp_dir().join(format!("climb-docs-bound-ok-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = write_results(&dir, true, &[]);
        assert!(check_signing_bound(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bound_guard_fails_when_the_bound_is_violated() {
        let dir = std::env::temp_dir().join(format!("climb-docs-bound-bad-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = write_results(&dir, false, &["signing_speedup_c_vs_b Small (N=100)"]);
        let err = check_signing_bound(&path)
            .expect_err("bound_ok=false must be a failing claim")
            .to_string();
        assert!(err.contains("bound violated"), "unexpected message: {err}");
        assert!(err.contains("Small"), "the offender must be named: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bound_guard_fails_when_the_bound_was_never_checked() {
        let dir =
            std::env::temp_dir().join(format!("climb-docs-bound-absent-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // A results document that never carried the bound check: the field is
        // absent altogether, not `false`.
        let doc = serde_json::json!({ "derived": { "bound_offenders": [] } });
        let path = dir.join("climb-results.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json"))
            .expect("write temp results");
        let err = check_signing_bound(&path)
            .expect_err("an unchecked bound must be a failing claim")
            .to_string();
        assert!(
            err.contains(BOUND_POINTER),
            "the pointer must be named: {err}"
        );
        assert!(
            err.contains(CONSOLIDATE_COMMAND),
            "the regenerating command must be named: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A miniature quote sheet: two facts, the same shape as the real one.
    const SHEET: &str = "\
## Wire bytes

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Medium — wire total, C (MTL amortised) | 2,743,352 | B | MEASURED | n = 10000 | C | /wire_size/1 | target/climb-results.json |
| Medium — wire reduction, C vs B | 96.706 | % | DERIVED | n = 10000 | B | /comparisons/2 | target/climb-results.json |
| Medium — signing speed-up, C vs B | ≈9869 | × | EXTRAPOLATED | n = 10000 | B | /comparisons/2 | target/climb-results.json |
| Small — resident median | 4206592 | B | MEASURED | n = 7 | n/a | /memory/measured/0 | target/climb-results.json |

## Footer

- Rows rendered: 4.
";

    fn facts() -> Vec<FactRow> {
        parse_quote_sheet(SHEET)
    }

    fn known() -> HashSet<String> {
        ["climb_mtl", "climb_tui", "xtask"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn parses_fact_rows_with_value_unit_class_and_baseline() {
        let rows = facts();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].label, "Medium — wire total, C (MTL amortised)");
        assert_eq!(rows[0].value, "2743352");
        assert_eq!(rows[0].unit, "B");
        assert_eq!(rows[0].class, "MEASURED");
        assert_eq!(rows[0].baseline, "C");
    }

    #[test]
    fn skips_headers_separators_and_non_table_lines() {
        let rows = facts();
        assert!(rows.iter().all(|r| r.label != "Label"));
        assert!(rows.iter().all(|r| !r.label.starts_with("---")));
        assert!(rows.iter().all(|r| !r.label.starts_with('-')));
    }

    #[test]
    fn normalizes_commas_and_the_extrapolation_marker() {
        let rows = facts();
        assert_eq!(rows[2].value, "9869"); // "≈9869"
        let readme_value = normalize_value("≈9,869");
        assert_eq!(readme_value, "9869");
    }

    #[test]
    fn scanner_finds_plain_and_separated_values() {
        let pairs = scan_result_numbers("the wire total is 2,743,352 B for the batch");
        assert_eq!(pairs, vec![("2743352".to_string(), "B".to_string())]);
    }

    #[test]
    fn scanner_finds_glued_percent_and_multiplication_sign() {
        assert_eq!(
            scan_result_numbers("97.307% and \u{2248}9,869\u{d7} smaller"),
            vec![
                ("97.307".to_string(), "%".to_string()),
                ("9869".to_string(), "\u{d7}".to_string()),
            ]
        );
    }

    #[test]
    fn scanner_ignores_versions_glued_names_and_hyphenated_constants() {
        assert!(scan_result_numbers("liboqs 0.16.0 and OpenSSL 3.4.1").is_empty());
        assert!(scan_result_numbers("SLH-DSA-SHA2-128s-MTL-SHA2-128").is_empty());
        assert!(scan_result_numbers("the 1232-byte EDNS buffer").is_empty());
    }

    #[test]
    fn scanner_ignores_letter_runs_that_are_not_result_units() {
        assert!(scan_result_numbers("100 messages and 8,192 pages later").is_empty());
        assert!(scan_result_numbers("one document at 120 columns").is_empty());
    }

    #[test]
    fn scanner_respects_the_allow_marker_at_check_time() {
        let text = "constant 1234 B appears here <!-- docs-check:allow-number -->";
        let rows = facts();
        let without_marker = text.replace(" <!-- docs-check:allow-number -->", "");
        assert_eq!(
            check_file("doc.md", &without_marker, &rows, &known()).len(),
            1
        );
        assert!(check_file("doc.md", text, &rows, &known()).is_empty());
    }

    #[test]
    fn a_number_missing_from_the_quote_sheet_is_a_violation() {
        let text = "the response was 24,438 B on the wire";
        let violations = check_file("doc.md", text, &facts(), &known());
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("24438 B"), "{violations:?}");
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn a_number_present_in_the_quote_sheet_passes() {
        let text = "the amortised total is 2,743,352 B (or 96.706% less, \u{2248}9,869\u{d7})";
        assert!(check_file("doc.md", text, &facts(), &known()).is_empty());
    }

    #[test]
    fn crate_like_mermaid_ids_must_be_real_names() {
        let text = "```mermaid\nflowchart LR\n  climb_mtl --> climb_reprot\n```";
        let violations = check_file("doc.md", text, &facts(), &known());
        assert_eq!(violations.len(), 1);
        assert!(
            violations[0].message.contains("climb_reprot"),
            "{violations:?}"
        );
        assert!(violations[0].message.contains("crate or binary"));
    }

    #[test]
    fn real_crate_ids_and_infrastructure_ids_pass_the_mermaid_check() {
        let text = "```mermaid\nflowchart LR\n  climb_mtl --> xtask\n  nsd --> docker_net\n  docker_net --> climb_tui\n```";
        assert!(check_file("doc.md", text, &facts(), &known()).is_empty());
    }

    #[test]
    fn quoted_mermaid_labels_are_never_scanned_as_node_ids() {
        let text = "```mermaid\nflowchart LR\n  nsd[\"climb-builder image, climb-results.json output\"]\n```";
        assert!(check_file("doc.md", text, &facts(), &known()).is_empty());
    }

    #[test]
    fn non_mermaid_code_blocks_are_not_scanned_for_node_ids() {
        let text = "```text\nclimb_reprot is a typo but not mermaid\n```";
        assert!(check_file("doc.md", text, &facts(), &known()).is_empty());
    }

    #[test]
    fn headline_rows_are_parsed_between_the_markers() {
        let text = "before\n<!-- headline:begin -->\n| Metric | Value | Evidence class | Baseline |\n| --- | --- | --- | --- |\n| Wire total | 2,743,352 B | MEASURED | C |\n<!-- headline:end -->\nafter\n";
        let rows = parse_headline_rows(text).expect("markers present");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            (
                5,
                "2,743,352 B".to_string(),
                "MEASURED".to_string(),
                "C".to_string()
            )
        );
    }

    #[test]
    fn headline_rows_outside_the_markers_are_not_parsed() {
        let text = "| Wire total | 2,743,352 B | MEASURED | C |\n";
        assert!(parse_headline_rows(text).is_none());
    }

    #[test]
    fn a_headline_row_matching_the_sheet_passes() {
        let text = "<!-- headline:begin -->\n| Metric | Value | Evidence class | Baseline |\n| --- | --- | --- | --- |\n| Wire, Medium C | 2,743,352 B | MEASURED | C |\n| Speed-up | \u{2248}9,869 \u{d7} | EXTRAPOLATED | B |\n<!-- headline:end -->";
        assert!(check_file("README.md", text, &facts(), &known()).is_empty());
    }

    #[test]
    fn a_headline_row_with_the_wrong_class_or_baseline_is_a_violation() {
        let text = "<!-- headline:begin -->\n| Metric | Value | Evidence class | Baseline |\n| --- | --- | --- | --- |\n| Wire, Medium C | 2,743,352 B | MEASURED | B |\n<!-- headline:end -->";
        let violations = check_file("README.md", text, &facts(), &known());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0]
            .message
            .contains("does not match any quote-sheet row"));
    }

    #[test]
    fn a_headline_row_without_a_number_is_a_violation() {
        let text = "<!-- headline:begin -->\n| Metric | Value | Evidence class | Baseline |\n| --- | --- | --- | --- |\n| Wire, Medium C | smaller | MEASURED | C |\n<!-- headline:end -->";
        let violations = check_file("README.md", text, &facts(), &known());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("no number+unit pair"));
    }

    #[test]
    fn numbers_of_record_rows_are_parsed_between_the_markers() {
        let text = "before\n<!-- numbers-of-record:begin -->\n| Number | Value | Source artifact | Phase |\n|---|---|---|---|\n| Wire total | 2,743,352 B | target/x.json | 8 |\n| Old row `historical` | 999 B | target/y.json | 9 |\n<!-- numbers-of-record:end -->\nafter\n";
        let rows = parse_numbers_of_record_rows(text).expect("markers present");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (5, "2,743,352 B".to_string(), false));
        assert_eq!(rows[1], (6, "999 B".to_string(), true));
    }

    #[test]
    fn numbers_of_record_without_markers_is_not_parsed() {
        assert!(parse_numbers_of_record_rows("| Wire | 1 B | x | 8 |\n").is_none());
    }

    #[test]
    fn a_wrong_numbers_of_record_value_fails_with_a_located_line() {
        let text = "<!-- numbers-of-record:begin -->\n| Number | Value | Source artifact | Phase |\n|---|---|---|---|\n| Wire, Medium C vs A | 97.01% | target/climb-results.json | F18 |\n<!-- numbers-of-record:end -->";
        let violations = check_numbers_of_record(PROGRESS_FILE, text, &facts());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].file, PROGRESS_FILE);
        assert_eq!(violations[0].line, 4);
        assert!(violations[0].message.contains("97.01 %"), "{violations:?}");
        assert!(violations[0].message.contains("historical"));
    }

    #[test]
    fn a_historical_numbers_of_record_row_passes() {
        let text = "<!-- numbers-of-record:begin -->\n| Number | Value | Source artifact | Phase |\n|---|---|---|---|\n| Wire, Medium C vs A `historical` | 97.01% | target/climb-results.json | F18 |\n<!-- numbers-of-record:end -->";
        assert!(check_numbers_of_record(PROGRESS_FILE, text, &facts()).is_empty());
    }

    #[test]
    fn a_correct_numbers_of_record_row_passes() {
        let text = "<!-- numbers-of-record:begin -->\n| Number | Value | Source artifact | Phase |\n|---|---|---|---|\n| Medium wire reduction C vs B | 96.706% | target/climb-results.json | F18 |\n<!-- numbers-of-record:end -->";
        assert!(check_numbers_of_record(PROGRESS_FILE, text, &facts()).is_empty());
    }

    #[test]
    fn the_committed_numbers_of_record_table_passes_the_check() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let facts = parse_quote_sheet(
            &std::fs::read_to_string(root.join(QUOTE_SHEET)).expect("quote sheet is committed"),
        );
        let text = std::fs::read_to_string(root.join(PROGRESS_FILE)).expect("log is committed");
        let violations = check_numbers_of_record(PROGRESS_FILE, &text, &facts);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn normalize_id_folds_case_and_hyphens() {
        assert_eq!(normalize_id("climb-mtl-sys"), "climb_mtl_sys");
        assert_eq!(normalize_id("Climb_Tui"), "climb_tui");
    }

    #[test]
    fn crate_like_node_ids_skip_quoted_labels_and_punctuation() {
        let block = "climb_mtl[\"climb-mtl (safe wrapper)\"] --> xtask\nnsd[\"authoritative\"] --> docker_net";
        let ids = crate_like_node_ids(block);
        assert_eq!(ids, vec!["climb_mtl".to_string(), "xtask".to_string()]);
    }

    #[test]
    fn the_real_workspace_reports_its_crates_and_bins() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let names = workspace_names(root).expect("cargo metadata runs");
        for expected in [
            "climb_mtl",
            "climb_mtl_sys",
            "climb_report",
            "climb_tui",
            "xtask",
        ] {
            assert!(names.contains(expected), "missing {expected} in {names:?}");
        }
        // Test/example targets are not binaries and must not be reported.
        assert!(!names.contains("raw_ffi_smoke"));
    }

    #[test]
    fn the_committed_documentation_passes_the_check() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let facts = parse_quote_sheet(
            &std::fs::read_to_string(root.join(QUOTE_SHEET)).expect("quote sheet is committed"),
        );
        let known = workspace_names(root).expect("cargo metadata runs");
        let readme = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        assert!(
            readme.contains(HEADLINE_BEGIN) && readme.contains(HEADLINE_END),
            "README must carry the headline table markers"
        );
        for entry in std::fs::read_dir(root.join("docs")).expect("docs/ is committed") {
            let path = entry.expect("readable entry").path();
            let is_md = path.extension().is_some_and(|ext| ext == "md");
            let is_sheet = path.file_name().is_some_and(|n| n == "deck-facts.md");
            if !is_md || is_sheet {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("readable doc");
            let label = path.display().to_string();
            let violations = check_file(&label, &text, &facts, &known);
            assert!(violations.is_empty(), "{label}: {violations:?}");
        }
        let violations = check_file("README.md", &readme, &facts, &known);
        assert!(violations.is_empty(), "README.md: {violations:?}");
    }

    /// The tracked-file set of the workspace under test.
    fn tracked(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    /// The workspace root, one level above the `xtask` crate.
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace")
            .to_path_buf()
    }

    #[test]
    fn the_committed_readme_carries_the_logo_header_block() {
        let root = workspace_root();
        let text = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        let violations = check_logo_block("README.md", &text);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn a_document_without_the_logo_markers_is_a_violation() {
        let violations = check_logo_block("README.md", "# Title\n\nno logo header here\n");
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[0].message.contains(LOGO_BEGIN), "{violations:?}");
        assert!(violations[1].message.contains(LOGO_END), "{violations:?}");
    }

    #[test]
    fn a_logo_block_without_an_image_is_a_violation() {
        let text = format!("{LOGO_BEGIN}\n<p>no image in this block</p>\n{LOGO_END}\n");
        let violations = check_logo_block("README.md", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("names no image"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_logo_block_naming_an_image_passes() {
        let text = format!(
            "{LOGO_BEGIN}\n<picture><img src=\"docs/media/climb-mark-light.png\" alt=\"CLIMB\"></picture>\n{LOGO_END}\n"
        );
        assert!(check_logo_block("README.md", &text).is_empty());
    }

    #[test]
    fn the_committed_readme_carries_the_commit_graph_block() {
        let root = workspace_root();
        let text = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        let expected = expected_commit_link(&root).expect("the committed graph names its commit");
        let violations = check_commit_graph_block("README.md", &text, Some(&expected));
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn a_document_without_the_commit_graph_markers_is_a_violation() {
        let violations = check_commit_graph_block("README.md", "# Title\n\nno graph here\n", None);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(
            violations[0].message.contains(COMMIT_GRAPH_BEGIN),
            "{violations:?}"
        );
        assert!(
            violations[1].message.contains(COMMIT_GRAPH_END),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_without_an_image_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             from the public history of this repository; later commits appear after the next refresh\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, None);
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("names no image")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_without_the_phone_card_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <a href=\"https://example.invalid/commit/abc\"><img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\"></a>\n\
             Generated from the public history of this repository; later commits appear after the next refresh.\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, None);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("<picture>")),
            "{violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains(COMMIT_GRAPH_NARROW)),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_with_a_compact_strip_inside_it_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <picture><source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\"></picture>\n\
             <img src=\"docs/media/commit-graph-compact.svg\" alt=\"old strip\">\n\
             Generated from the public history of this repository; later commits appear after the next refresh.\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, None);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("strip belongs in its own"),
            "{violations:?}"
        );
    }

    #[test]
    fn the_committed_readme_carries_the_compact_strip() {
        let root = workspace_root();
        let text = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        let expected = expected_commit_link(&root).expect("the committed graph names its commit");
        let violations = check_commit_graph_strip_block("README.md", &text, Some(&expected));
        assert!(violations.is_empty(), "{violations:?}");
        let placement = check_commit_graph_strip_placement("README.md", &text);
        assert!(placement.is_empty(), "{placement:?}");
    }

    #[test]
    fn a_strip_with_a_number_in_its_alt_text_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_STRIP_BEGIN}\n\
             <picture><source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_STRIP_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_STRIP}\" alt=\"71 commits since September\"></picture>\n\
             {COMMIT_GRAPH_STRIP_END}\n"
        );
        let violations = check_commit_graph_strip_block("README.md", &text, None);
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("quotes a number")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_strip_without_the_phone_strip_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_STRIP_BEGIN}\n\
             <picture><img src=\"{COMMIT_GRAPH_STRIP}\" alt=\"the climb so far\"></picture>\n\
             {COMMIT_GRAPH_STRIP_END}\n"
        );
        let violations = check_commit_graph_strip_block("README.md", &text, None);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains(COMMIT_GRAPH_STRIP_NARROW),
            "{violations:?}"
        );
    }

    #[test]
    fn a_strip_linking_elsewhere_is_a_violation() {
        let expected =
            "https://example.invalid/public/commit/0123456789abcdef0123456789abcdef01234567";
        let text = format!(
            "{COMMIT_GRAPH_STRIP_BEGIN}\n\
             <a href=\"https://example.invalid/public\"><picture>\
             <source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_STRIP_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_STRIP}\" alt=\"the climb so far\"></picture></a>\n\
             {COMMIT_GRAPH_STRIP_END}\n"
        );
        let violations = check_commit_graph_strip_block("README.md", &text, Some(expected));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0]
                .message
                .contains("does not link to the public as-of commit"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_complete_compact_strip_passes() {
        let expected =
            "https://example.invalid/public/commit/0123456789abcdef0123456789abcdef01234567";
        let text = format!(
            "{COMMIT_GRAPH_STRIP_BEGIN}\n\
             <a href=\"{expected}\"><picture>\
             <source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_STRIP_NARROW}\" width=\"390\">\
             <img src=\"{COMMIT_GRAPH_STRIP}\" alt=\"the climb so far\" width=\"840\"></picture></a>\n\
             {COMMIT_GRAPH_STRIP_END}\n"
        );
        assert!(check_commit_graph_strip_block("README.md", &text, Some(expected)).is_empty());
    }

    #[test]
    fn a_strip_away_from_the_badges_is_a_violation() {
        let text = format!(
            "{}\n\
             # CLIMB\n\n\
             {COMMIT_GRAPH_STRIP_BEGIN}\n\
             <picture><source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_STRIP_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_STRIP}\" alt=\"the climb so far\"></picture>\n\
             {COMMIT_GRAPH_STRIP_END}\n\n\
             {COMMIT_GRAPH_BEGIN}\n\
             {COMMIT_GRAPH_END}\n",
            readme_badges::BADGES_END
        );
        let violations = check_commit_graph_strip_placement("README.md", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("directly under"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_without_the_public_caption_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <picture><source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\"></picture>\n\
             Generated from git history; the stamp shows the commit it was generated at.\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, None);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("provenance caption"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_missing_the_refresh_note_is_a_violation() {
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <picture><source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\"></picture>\n\
             Generated from the public history of this repository.\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, None);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("provenance caption"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_commit_graph_block_linking_elsewhere_is_a_violation() {
        let expected =
            "https://example.invalid/public/commit/0123456789abcdef0123456789abcdef01234567";
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <a href=\"https://example.invalid/public/commits/main\"><picture>\
             <source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_NARROW}\">\
             <img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\"></picture></a>\n\
             Generated from the public history of this repository; later commits appear after the next refresh.\n\
             {COMMIT_GRAPH_END}\n"
        );
        let violations = check_commit_graph_block("README.md", &text, Some(expected));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0]
                .message
                .contains("does not link to the public as-of commit"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_complete_commit_graph_block_passes() {
        let expected =
            "https://example.invalid/public/commit/0123456789abcdef0123456789abcdef01234567";
        let text = format!(
            "{COMMIT_GRAPH_BEGIN}\n\
             <a href=\"{expected}\"><picture>\
             <source media=\"{COMMIT_GRAPH_MEDIA}\" srcset=\"{COMMIT_GRAPH_NARROW}\" width=\"390\">\
             <img src=\"{COMMIT_GRAPH_WIDE}\" alt=\"a graph\" width=\"840\"></picture></a>\n\
             Generated by `cargo xtask commit-graph` from the public history of this repository,\n\
             as of the commit shown in the image; later commits appear after the next refresh.\n\
             {COMMIT_GRAPH_END}\n"
        );
        assert!(check_commit_graph_block("README.md", &text, Some(expected)).is_empty());
    }

    #[test]
    fn markdown_and_html_image_paths_are_extracted() {
        let text = concat!(
            "![the mark](docs/media/climb-mark-light.png)\n",
            "<picture>\n",
            "  <source media=\"(prefers-color-scheme: dark)\" srcset=\"docs/media/climb-mark-dark.png\">\n",
            "  <img src=\"docs/media/climb-mark-light.png\" alt=\"CLIMB\" width=\"380\">\n",
            "</picture>\n",
        );
        let found = extract_image_paths(text);
        let paths: Vec<&str> = found.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "docs/media/climb-mark-light.png",
                "docs/media/climb-mark-dark.png",
                "docs/media/climb-mark-light.png",
            ]
        );
        assert_eq!(found[0].0, 1);
        assert_eq!(found[1].0, 3);
    }

    #[test]
    fn a_srcset_density_descriptor_is_not_read_as_a_path() {
        let text = "<img srcset=\"docs/media/climb-mark-light.png 1x, docs/media/climb-mark-dark.png 2x\">\n";
        let found = extract_image_paths(text);
        let paths: Vec<&str> = found.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "docs/media/climb-mark-light.png",
                "docs/media/climb-mark-dark.png"
            ]
        );
    }

    #[test]
    fn a_relative_image_path_that_is_tracked_passes() {
        let text = "<img src=\"docs/media/climb-mark-light.png\" alt=\"CLIMB\">\n";
        let files = tracked(&["docs/media/climb-mark-light.png"]);
        assert!(check_image_paths("README.md", "", text, &files).is_empty());
    }

    #[test]
    fn a_relative_image_path_that_is_not_tracked_is_a_violation() {
        let text = "<img src=\"docs/media/climb-mark-missing.png\" alt=\"CLIMB\">\n";
        let files = tracked(&["docs/media/climb-mark-light.png"]);
        let violations = check_image_paths("README.md", "", text, &files);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].line, 1);
        assert!(
            violations[0]
                .message
                .contains("docs/media/climb-mark-missing.png"),
            "{violations:?}"
        );
        assert!(violations[0].message.contains("not a tracked file"));
    }

    #[test]
    fn paths_outside_the_repository_are_skipped() {
        let text = concat!(
            "<img src=\"/absolute/path.png\" alt=\"a\">\n",
            "<img src=\"https://example.invalid/x.png\" alt=\"b\">\n",
            "<img src=\"//cdn.example.invalid/x.png\" alt=\"c\">\n",
            "<img src=\"data:image/png;base64,AAAA\" alt=\"d\">\n",
        );
        assert!(check_image_paths("README.md", "", text, &tracked(&[])).is_empty());
    }

    #[test]
    fn image_paths_resolve_against_the_document_directory() {
        let text = "<img src=\"media/climb-icon.png\" alt=\"the icon\">\n";
        let files = tracked(&["docs/media/climb-icon.png"]);
        // A document in `docs/` resolves the path against its own directory.
        assert!(check_image_paths("docs/README.md", "docs", text, &files).is_empty());
        // The same document in `docs/media/` would look one level deeper.
        assert_eq!(
            check_image_paths("docs/media/README.md", "docs/media", text, &files).len(),
            1
        );
        // A sibling path in the document's own directory resolves the same way.
        let sibling = "<img src=\"climb-icon.png\" alt=\"the icon\">\n";
        assert!(
            check_image_paths("docs/media/README.md", "docs/media", sibling, &files).is_empty()
        );
    }

    #[test]
    fn the_committed_documentation_image_paths_are_tracked() {
        let root = workspace_root();
        let files = tracked_files(&root).expect("git ls-files runs in the workspace");
        let mut documents = vec![root.join("README.md")];
        documents.extend(markdown_files(&root.join("docs")).expect("docs/ is committed"));
        for path in &documents {
            let text = std::fs::read_to_string(path).expect("readable document");
            let relative = path.strip_prefix(&root).unwrap_or(path);
            let label = relative.display().to_string();
            let dir_prefix = relative
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default();
            let violations = check_image_paths(&label, &dir_prefix, &text, &files);
            assert!(violations.is_empty(), "{label}: {violations:?}");
        }
    }

    /// A miniature badge block: the shape the generator writes, with the
    /// percent-encoded query values that the numeric scanner would otherwise
    /// read as results.
    const BADGE_BLOCK: &str = "\
<!-- badges:begin -->
<div align=\"center\">
  <a href=\"https://portal.example.invalid/guideline/\"><img alt=\"AIORI 3 Hackathon\" src=\"https://img.shields.io/static/v1?label=AIORI%203&amp;message=Hackathon&amp;color=C55E33\"></a>
  <br>
  <a href=\"docs/deck-facts.md\"><img alt=\"Claims verified: 771/771\" src=\"https://img.shields.io/static/v1?label=claims%20verified&amp;message=771%2F771\"></a>
</div>
<!-- badges:end -->";

    #[test]
    fn the_badge_block_lines_are_exempt_from_the_numeric_rule() {
        let text = format!("# Title\n\n{BADGE_BLOCK}\n");
        let violations = check_file("README.md", &text, &facts(), &known());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn the_numeric_rule_still_applies_outside_the_badge_block() {
        // The same digits, one line above the block, are still a violation:
        // the exemption covers the managed block and nothing else.
        let text = format!("the block reads 771 % here\n\n{BADGE_BLOCK}\n");
        let violations = check_file("README.md", &text, &facts(), &known());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("771 %"), "{violations:?}");
    }

    #[test]
    fn the_badge_block_line_range_covers_only_the_body() {
        let text = format!("# Title\n\n{BADGE_BLOCK}\n");
        let (begin, end) = badge_block_lines(&text).expect("the markers are present");
        let lines: Vec<&str> = text.lines().collect();
        // The range starts after the opening marker and ends before the
        // closing one.
        assert_eq!(lines[begin - 1].trim(), "<!-- badges:begin -->");
        assert_eq!(lines[end].trim(), "<!-- badges:end -->");
        assert!(lines[begin].contains("<div align=\"center\">"), "{lines:?}");
        assert!(lines[end - 1].contains("</div>"), "{lines:?}");
    }

    #[test]
    fn a_document_without_the_badge_markers_has_no_exempt_range() {
        assert!(badge_block_lines("# Title\n\n771 % of nothing\n").is_none());
    }

    #[test]
    fn the_committed_readme_badge_block_matches_a_fresh_generation() {
        let root = workspace_root();
        let readme = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        let violations = check_badge_block("README.md", &readme, &root);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn an_injected_edit_in_the_badge_block_is_reported_as_drift() {
        let root = workspace_root();
        let readme = std::fs::read_to_string(root.join("README.md")).expect("README is committed");
        let edited = readme.replace("771%2F771", "772%2F771");
        assert_ne!(edited, readme, "the injection must change the block");
        let violations = check_badge_block("README.md", &edited, &root);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("has drifted"),
            "{violations:?}"
        );
        assert!(
            violations[0].message.contains("772%2F771"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_readme_without_the_badge_block_is_a_violation() {
        let root = workspace_root();
        let violations = check_badge_block("README.md", "# Title\n\nno block here\n", &root);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("no managed badge block"),
            "{violations:?}"
        );
    }
}
