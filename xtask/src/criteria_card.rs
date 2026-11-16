//! `cargo xtask criteria-card`: render the one-page PS-036 criteria card.
//!
//! The card is a projection of `docs/CRITERIA-AUDIT.md`: every status, evidence
//! path and proving command is read from the audit table. Nothing in this
//! module decides a status, and there is no default status for a malformed row.
use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

const BEGIN: &str = "<!-- criteria-audit:begin -->";
const END: &str = "<!-- criteria-audit:end -->";
const STATUSES: [&str; 4] = ["PASS", "PARTIAL", "FAIL", "N/A"];

#[derive(Args)]
pub(crate) struct CriteriaCardArgs {
    /// Criteria audit table to read.
    #[arg(long, default_value = "docs/CRITERIA-AUDIT.md")]
    pub(crate) audit: PathBuf,
    /// Printable HTML card to write.
    #[arg(long, default_value = "docs/CRITERIA-CARD.html")]
    pub(crate) out: PathBuf,
    /// Compare the committed card with a fresh render instead of writing.
    #[arg(long)]
    pub(crate) check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CriterionRow {
    pub(crate) id: String,
    pub(crate) requirement: String,
    pub(crate) status: String,
    pub(crate) evidence: String,
    pub(crate) command: String,
    pub(crate) caveat: String,
}

pub(crate) fn cmd_criteria_card(args: CriteriaCardArgs) -> Result<()> {
    let audit = fs::read_to_string(&args.audit)
        .with_context(|| format!("failed to read {}", args.audit.display()))?;
    let rows = parse_rows(&audit)?;
    let html = render_card(&rows);
    if args.check {
        let current = fs::read_to_string(&args.out).with_context(|| {
            format!(
                "failed to read {} (run `cargo xtask criteria-card`)",
                args.out.display()
            )
        })?;
        if current != html {
            bail!(
                "{} is stale; run `cargo xtask criteria-card` and commit the result",
                args.out.display()
            );
        }
        println!(
            "criteria-card: {} row(s) match {}",
            rows.len(),
            args.out.display()
        );
        return Ok(());
    }
    write_card(&args.out, &html)?;
    println!(
        "wrote {} ({} criteria, {} pass, {} partial, {} fail, {} n/a)",
        args.out.display(),
        rows.len(),
        count(&rows, "PASS"),
        count(&rows, "PARTIAL"),
        count(&rows, "FAIL"),
        count(&rows, "N/A")
    );
    Ok(())
}

fn write_card(path: &Path, html: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, html).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn parse_rows(text: &str) -> Result<Vec<CriterionRow>> {
    let mut inside = false;
    let mut rows = Vec::new();
    let mut table_rows = 0usize;
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed == BEGIN {
            if inside {
                bail!("nested {BEGIN} at line {line_number}");
            }
            inside = true;
            continue;
        }
        if trimmed == END {
            if !inside {
                bail!("{END} without {BEGIN} at line {line_number}");
            }
            if rows.is_empty() {
                bail!("the criteria markers carry no rows");
            }
            return Ok(rows);
        }
        if !inside || !line.starts_with('|') {
            continue;
        }
        table_rows += 1;
        if table_rows <= 2 {
            continue; // header and separator
        }
        let cells = split_cells(line);
        if cells.len() != 9 {
            bail!(
                "line {line_number}: a criteria row needs 7 cells, found {}",
                cells.len().saturating_sub(2)
            );
        }
        let cell = |index: usize| cells[index].trim().trim_matches('`').trim().to_string();
        let row = CriterionRow {
            id: cell(1),
            requirement: cell(2),
            status: cell(3),
            evidence: cell(4),
            command: cell(5),
            caveat: cell(7),
        };
        validate_row(line_number, &row)?;
        rows.push(row);
    }
    bail!("missing {BEGIN} / {END} markers");
}

/// Split a markdown table row on unescaped pipes, so a proving command may
/// contain a shell pipe written as `\|` without breaking the row.
fn split_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
                current.push(ch);
            }
            '|' => {
                cells.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    cells.push(current);
    cells
        .into_iter()
        .map(|cell| cell.replace("\\|", "|"))
        .collect()
}

fn validate_row(line: usize, row: &CriterionRow) -> Result<()> {
    if row.id.is_empty() {
        bail!("line {line}: a criteria row needs an id");
    }
    if !STATUSES.contains(&row.status.as_str()) {
        bail!(
            "line {line}: status `{}` is not one of {} — statuses are read, never inferred",
            row.status,
            STATUSES.join(", ")
        );
    }
    if row.requirement.is_empty() {
        bail!("line {line}: a criteria row needs a requirement");
    }
    if row.evidence.is_empty() || row.evidence == "—" {
        bail!("line {line}: a criteria row needs an evidence path");
    }
    Ok(())
}

pub(crate) fn count(rows: &[CriterionRow], status: &str) -> usize {
    rows.iter().filter(|row| row.status == status).count()
}

pub(crate) fn render_card(rows: &[CriterionRow]) -> String {
    let groups = [
        ("R", "Requirements", "Problem-statement requirements, verbatim wording shortened only where the audit does so."),
        ("O", "Expected outcomes", "What the problem statement expects the benchmark to produce."),
        ("D", "Deliverables", "Submission artefacts, including the two that need a human or an external session."),
    ];
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>CLIMB — PS-036 criteria card</title>\n<style>\n");
    html.push_str(
        "@page { size: A4 portrait; margin: 9mm; }\n\
         * { box-sizing: border-box; }\n\
         body { margin: 0; color: #14161a; background: #fff; font: 7.6pt/1.25 \"DejaVu Sans\", Arial, sans-serif; }\n\
         header { display: flex; justify-content: space-between; gap: 8mm; border-bottom: 1.4pt solid #14161a; padding-bottom: 2mm; }\n\
         h1 { margin: 0; font-size: 13pt; letter-spacing: .02em; }\n\
         .sub { margin: .6mm 0 0; color: #4a4f57; font-size: 8pt; }\n\
         .summary { display: flex; gap: 2mm; flex-wrap: wrap; margin: 2.5mm 0 3mm; }\n\
         .pill { border: .6pt solid #14161a; border-radius: 999px; padding: .6mm 2mm; font-weight: 700; }\n\
         section { margin-top: 2.4mm; }\n\
         h2 { margin: 0 0 1mm; font-size: 8.4pt; text-transform: uppercase; letter-spacing: .08em; border-bottom: .5pt solid #b9bec6; padding-bottom: .6mm; }\n\
         .rows { column-count: 2; column-gap: 5mm; }\n\
         .row { padding: .75mm 0; border-bottom: .35pt solid #d9dce1; break-inside: avoid; }\n\
         .id { font-weight: 800; font-variant-numeric: tabular-nums; margin-right: 1.2mm; }\n\
         .status { font-weight: 800; letter-spacing: .03em; margin-right: 1.2mm; }\n\
         .req { display: inline; }\n\
         .status.pass { color: #0b6b3a; }\n\
         .status.partial { color: #8a5200; }\n\
         .status.fail { color: #a01818; }\n\
         .status.na { color: #4a4f57; }\n\
         .req { font-weight: 650; }\n\
         .meta { color: #4a4f57; font-size: 6.8pt; margin-top: .25mm; }\n\
         code { font-family: \"DejaVu Sans Mono\", monospace; font-size: 6.6pt; color: #14161a; }\n\
         footer { margin-top: 3mm; border-top: .8pt solid #14161a; padding-top: 1.2mm; color: #4a4f57; font-size: 6.8pt; }\n\
         @media print { body { -webkit-print-color-adjust: exact; print-color-adjust: exact; } }\n",
    );
    html.push_str("</style>\n</head>\n<body>\n<header>\n<div>\n");
    html.push_str("<h1>CLIMB — PS-036 criteria audit</h1>\n");
    html.push_str("<p class=\"sub\">Cryptographic Ladder Impact Measurement Benchmark · AIORI-3 PS-036 · one-page judge card</p>\n");
    html.push_str("</div>\n<div class=\"sub\">Generated from <code>docs/CRITERIA-AUDIT.md</code><br>statuses are read, never typed</div>\n</header>\n");
    html.push_str("<div class=\"summary\">\n");
    for (label, status) in [
        ("pass", "PASS"),
        ("partial", "PARTIAL"),
        ("fail", "FAIL"),
        ("n/a", "N/A"),
    ] {
        html.push_str(&format!(
            "<span class=\"pill\"><span class=\"status {}\">{}</span> {}</span>\n",
            label,
            status,
            count(rows, status)
        ));
    }
    html.push_str(&format!(
        "<span class=\"pill\">total {}</span>\n</div>\n",
        rows.len()
    ));

    for (prefix, title, note) in groups {
        let group: Vec<&CriterionRow> = rows
            .iter()
            .filter(|row| row.id.starts_with(prefix))
            .collect();
        if group.is_empty() {
            continue;
        }
        html.push_str(&format!(
            "<section>\n<h2>{title} <span class=\"sub\" style=\"text-transform:none;letter-spacing:0\">{note}</span></h2>\n<div class=\"rows\">\n"
        ));
        for row in group {
            let class = status_class(&row.status);
            let title = if row.caveat.is_empty() || row.caveat == "—" {
                String::new()
            } else {
                format!(" title=\"caveat: {}\"", escape(&row.caveat))
            };
            html.push_str(&format!("<div class=\"row\"{title}>\n"));
            html.push_str(&format!(
                "<span class=\"id\">{}</span><span class=\"status {class}\">{}</span>",
                escape(&row.id),
                escape(&row.status)
            ));
            html.push_str(&format!(
                "<span class=\"req\">{}</span>\n",
                escape(&row.requirement)
            ));
            html.push_str(&format!(
                "<div class=\"meta\">evidence <code>{}</code> · prove <code>{}</code></div>\n",
                escape(&row.evidence),
                escape(&row.command)
            ));
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n</section>\n");
    }
    html.push_str(
        "<footer>Empirical benchmark evidence only: no formal security proof, guarantee or certification is claimed. \
         Edit the audit table, then run <code>cargo xtask criteria-card</code>; <code>--check</code> fails when this card is stale.</footer>\n",
    );
    html.push_str("</body>\n</html>\n");
    html
}

fn status_class(status: &str) -> &'static str {
    match status {
        "PASS" => "pass",
        "PARTIAL" => "partial",
        "FAIL" => "fail",
        _ => "na",
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(rows: &str) -> String {
        format!(
            "{BEGIN}\n| ID | Requirement | Status | Evidence | Command | Excerpt | Caveat |\n| --- | --- | --- | --- | --- | --- | --- |\n{rows}{END}\n"
        )
    }

    fn row(id: &str, status: &str) -> String {
        format!("| {id} | requirement {id} | {status} | `docs/X.md` | `cargo xtask docs-check` | ok | — |\n")
    }

    #[test]
    fn statuses_are_read_from_the_table_not_defaulted() {
        let doc = audit(&format!(
            "{}{}{}",
            row("R1", "PASS"),
            row("O1", "PARTIAL"),
            row("D1", "N/A")
        ));
        let rows = parse_rows(&doc).expect("the table parses");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, "PASS");
        assert_eq!(rows[1].status, "PARTIAL");
        assert_eq!(rows[2].status, "N/A");
    }

    #[test]
    fn an_unknown_status_is_rejected_rather_than_guessed() {
        let doc = audit(&row("R1", "MAYBE"));
        let error = parse_rows(&doc).expect_err("an unknown status fails");
        assert!(error
            .to_string()
            .contains("statuses are read, never inferred"));
    }

    #[test]
    fn an_escaped_pipe_in_a_command_stays_inside_its_cell() {
        let doc = audit("| R1 | requirement | PASS | `docs/X.md` | `jq -c '.a[] \\| length' f.json` | ok | — |\n");
        let rows = parse_rows(&doc).expect("the escaped pipe parses");
        assert_eq!(rows[0].command, "jq -c '.a[] | length' f.json");
        assert_eq!(rows[0].caveat, "—");
    }

    #[test]
    fn a_row_without_evidence_is_rejected() {
        let doc = audit("| R1 | requirement | PASS | — | — | — | — |\n");
        assert!(parse_rows(&doc).is_err());
    }

    #[test]
    fn the_card_counts_each_status_from_the_rows() {
        let doc = audit(&format!(
            "{}{}{}{}",
            row("R1", "PASS"),
            row("R2", "PASS"),
            row("O1", "PARTIAL"),
            row("D1", "FAIL")
        ));
        let rows = parse_rows(&doc).expect("the table parses");
        assert_eq!(count(&rows, "PASS"), 2);
        assert_eq!(count(&rows, "PARTIAL"), 1);
        assert_eq!(count(&rows, "FAIL"), 1);
        assert_eq!(count(&rows, "N/A"), 0);
    }

    #[test]
    fn the_card_renders_every_row_and_escapes_text() {
        let doc = audit(&row("R1", "PASS"));
        let rows = parse_rows(&doc).expect("the table parses");
        let html = render_card(&rows);
        assert!(html.contains("Generated from"));
        assert!(html.contains("R1"));
        assert!(html.contains("statuses are read, never typed"));
        assert!(html.contains("@page"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn the_card_escapes_angle_brackets_in_evidence() {
        let doc =
            audit("| R1 | requirement | PASS | `a<b.md` | `cargo xtask docs-check` | ok | — |\n");
        let rows = parse_rows(&doc).expect("the table parses");
        let html = render_card(&rows);
        assert!(html.contains("a&lt;b.md"));
        assert!(!html.contains("a<b.md"));
    }
}
