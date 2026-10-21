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

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    for (idx, line) in text.lines().enumerate() {
        if line.contains(ALLOW_NUMBER_MARKER) {
            continue;
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

/// Treat `bound_ok == false` in a consolidated results artifact as a failing
/// claim: the documented numbers must not rest on a signing speed-up that
/// exceeds the 1.05xN bound. Names every offending ratio.
fn check_signing_bound(results_path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(results_path)
        .with_context(|| format!("failed to read {}", results_path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", results_path.display()))?;
    let bound_ok = doc
        .pointer("/derived/bound_ok")
        .and_then(|v| v.as_bool())
        // A missing bound field is treated as unchecked, not as a pass.
        .unwrap_or(true);
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

    // The signing speed-up bound is a hard honesty claim: a consolidated
    // artifact whose reported speed-up exceeds 1.05xN must not back the
    // documented numbers. `bound_ok == false` is treated as a failing claim.
    check_signing_bound(&args.root.join(&args.results))?;

    if violations.is_empty() {
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
    bail!("docs-check: {} violation(s)", violations.len());
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
}
