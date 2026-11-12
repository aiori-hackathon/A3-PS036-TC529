//! `cargo xtask verify-numbers`: recompute every published headline number
//! from raw rows and compare it with every surface that shows it.
//!
//! The surfaces are the consolidated artifact's derived fields, the generated
//! quote sheet, the README headline and classical tables, the HTML report's
//! embedded claims, and the TUI Overview's rendered KPI cards and context
//! table. A disagreement is a defect: regenerate the run and all surfaces;
//! never edit a published number.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::Value;

use climb_report::numbers::{classical_metrics, headline_metrics, spread, Metric, SPREAD_RATIOS};
use climb_report::ClimbResults;
use climb_tui::data::{HeadlineSurface, ReportHealth, ResultsView};

use crate::docs_check::{parse_quote_sheet, FactRow};

pub(crate) const HEADLINE_BEGIN: &str = "<!-- headline:begin -->";
pub(crate) const HEADLINE_END: &str = "<!-- headline:end -->";
pub(crate) const CLASSICAL_BEGIN: &str = "<!-- classical:begin -->";
pub(crate) const CLASSICAL_END: &str = "<!-- classical:end -->";
const CLAIMS_TAG: &str = "<script type=\"application/json\" id=\"climb-claims\">";

#[derive(Args)]
pub(crate) struct VerifyNumbersArgs {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Consolidated results artifact.
    #[arg(long, default_value = "docs/sample-output/climb-results.json")]
    results: PathBuf,
    /// Generated quote sheet.
    #[arg(long, default_value = "docs/deck-facts.md")]
    sheet: PathBuf,
    /// README carrying the generated headline and classical blocks.
    #[arg(long, default_value = "README.md")]
    readme: PathBuf,
    /// Rendered HTML report carrying the embedded claims registry.
    #[arg(long, default_value = "docs/sample-output/report.html")]
    report: PathBuf,
    /// Committed record of deterministic dataset digests.
    #[arg(long, default_value = "docs/sample-output/dataset-digests.json")]
    dataset_digests: PathBuf,
    /// Recompute the million-message digest too (slow; off by default).
    #[arg(long)]
    deep_digests: bool,
    /// Fail when a headline row that needs a spread only has `pending`.
    #[arg(long)]
    require_spread: bool,
}

impl VerifyNumbersArgs {
    /// The default paths under a workspace root, for callers that run the
    /// check as part of another gate.
    pub(crate) fn for_root(root: PathBuf, require_spread: bool) -> Self {
        Self {
            root,
            results: PathBuf::from("docs/sample-output/climb-results.json"),
            sheet: PathBuf::from("docs/deck-facts.md"),
            readme: PathBuf::from("README.md"),
            report: PathBuf::from("docs/sample-output/report.html"),
            dataset_digests: PathBuf::from("docs/sample-output/dataset-digests.json"),
            deep_digests: false,
            require_spread,
        }
    }
}

/// Recompute every recorded dataset digest.
///
/// A missing document is a warning, because a checkout that has not produced it
/// yet is not a corrupt artifact. A present-but-unparseable or stale document is
/// a hard error naming the path: silently ignoring it would turn the
/// determinism evidence into decoration.
fn check_dataset_digests(args: &VerifyNumbersArgs) -> Result<()> {
    let path = args.root.join(&args.dataset_digests);
    if !path.exists() {
        println!(
            "verify-numbers: no dataset digest document at {} \u{2014} skipping determinism check",
            path.display()
        );
        return Ok(());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let document: climb_dns::DatasetDigestDocument = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let limit = if args.deep_digests {
        usize::MAX
    } else {
        climb_dns::ROUTINE_DIGEST_MAX_COUNT
    };
    let outcome = document
        .check_limited(climb_bench::DEFAULT_WIRE_SEED, limit)
        .with_context(|| {
            format!(
                "dataset digests in {} disagree with a fresh generation",
                path.display()
            )
        })?;
    println!(
        "verify-numbers: {} dataset digest(s) recomputed from {}",
        outcome.checked,
        path.display()
    );
    if !outcome.skipped.is_empty() {
        println!(
            "verify-numbers: not recomputed (over {limit} messages); pass --deep-digests to include: {}",
            outcome.skipped.join(", ")
        );
    }
    Ok(())
}

/// One table row in the README's generated blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadmeRow {
    pub label: String,
    pub value: String,
    pub class: String,
    pub baseline: String,
}

/// The result of checking one surface for one metric.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// The surface agrees.
    Agree(String),
    /// The surface does not display this metric.
    NotShown,
    /// The surface is absent or lacks the row/claim.
    Missing(String),
    /// The surface disagrees or is malformed.
    Disagree(String),
}

impl Cell {
    fn failed(&self) -> bool {
        matches!(self, Cell::Missing(_) | Cell::Disagree(_))
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cell::Agree(value) => write!(f, "{value}"),
            Cell::NotShown => write!(f, "\u{2014}"),
            Cell::Missing(reason) => write!(f, "MISSING ({reason})"),
            Cell::Disagree(reason) => write!(f, "DISAGREE ({reason})"),
        }
    }
}

/// One metric's row across every surface.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckRow {
    pub metric: String,
    pub raw: String,
    pub artifact: Cell,
    pub sheet: Cell,
    pub readme: Cell,
    pub report: Cell,
    pub tui: Cell,
    pub spread: Cell,
}

impl CheckRow {
    fn failed(&self) -> bool {
        [
            &self.artifact,
            &self.sheet,
            &self.readme,
            &self.report,
            &self.tui,
            &self.spread,
        ]
        .iter()
        .any(|cell| cell.failed())
    }
}

/// Everything the checks need, in memory.
pub struct Inputs<'a> {
    pub results: &'a ClimbResults,
    pub results_json: &'a Value,
    pub sheet_facts: &'a [FactRow],
    pub headline_rows: &'a [ReadmeRow],
    pub classical_rows: &'a [ReadmeRow],
    pub claims: &'a HashMap<String, Value>,
    pub tui: &'a [HeadlineSurface],
    pub require_spread: bool,
}

fn approx(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

fn approx_display(expected: f64, shown: f64) -> bool {
    // Displayed values are rounded to one or more decimals; allow the
    // rounding, not a different number.
    (expected - shown).abs() <= 2e-3 * expected.abs().max(1.0)
}

/// Resolve an RFC 6901 JSON pointer against a document.
fn resolve<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for token in pointer.trim_start_matches('/').split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Object(map) => map.get(&token)?,
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// Normalise a published value cell for comparison: thousands separators,
/// the extrapolation marker and surrounding whitespace do not change it.
fn normalise_value(text: &str) -> String {
    text.replace([',', '\u{2248}'], "").trim().to_string()
}

fn first_number_and_unit(cell: &str) -> Option<(f64, String)> {
    let mut parts = cell.split_whitespace();
    let number = parts.next()?.replace(',', "");
    let value = number.parse::<f64>().ok()?;
    let unit = parts.next().unwrap_or("").to_string();
    Some((value, unit))
}

/// Parse a generated markdown table between two marker lines.
pub fn parse_marked_rows(text: &str, begin: &str, end: &str) -> Vec<ReadmeRow> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|line| line.trim() == begin) else {
        return Vec::new();
    };
    let Some(stop) = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim() == end)
        .map(|(idx, _)| idx)
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in &lines[start + 1..stop] {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 6 {
            continue;
        }
        let label = cells[1];
        let value = cells[2];
        if label == "Metric" || label == "Algorithm" || value == "Value" || value.starts_with("---")
        {
            continue;
        }
        rows.push(ReadmeRow {
            label: label.to_string(),
            value: value.to_string(),
            class: cells[3].to_string(),
            baseline: cells[4].to_string(),
        });
    }
    rows
}

/// Parse the claims registry embedded in the report HTML.
pub fn parse_claims(html: &str) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    let Some(start) = html.find(CLAIMS_TAG) else {
        return map;
    };
    let body_start = start + CLAIMS_TAG.len();
    let Some(end) = html[body_start..].find("</script>") else {
        return map;
    };
    let body = &html[body_start..body_start + end];
    let Ok(claims) = serde_json::from_str::<Vec<Value>>(body) else {
        return map;
    };
    for claim in claims {
        if let (Some(pointer), Some(value)) = (
            claim.get("pointer").and_then(|v| v.as_str()),
            claim.get("value"),
        ) {
            map.insert(pointer.to_string(), value.clone());
        }
    }
    map
}

fn check_artifact(metric: &Metric, root: &Value) -> Cell {
    let Some(value) = resolve(root, &metric.pointer) else {
        return Cell::Missing(format!("artifact pointer {}", metric.pointer));
    };
    let Some(found) = value.as_f64() else {
        return Cell::Disagree("artifact value is not numeric".to_string());
    };
    if !approx(metric.value, found) {
        return Cell::Disagree(format!("{} != {}", metric.value, found));
    }
    if let (Some(denominator), Some(pointer)) =
        (metric.denominator, metric.denominator_pointer.as_deref())
    {
        let Some(found_denominator) = resolve(root, pointer).and_then(Value::as_u64) else {
            return Cell::Missing(format!("artifact pointer {pointer}"));
        };
        if found_denominator != denominator {
            return Cell::Disagree(format!("denominator {found_denominator} != {denominator}"));
        }
        return Cell::Agree(format!("{} of {found_denominator}", found as u64));
    }
    Cell::Agree(format!("{found}"))
}

fn check_sheet(metric: &Metric, facts: &[FactRow]) -> Cell {
    let Some(fact) = facts.iter().find(|fact| fact.label == metric.sheet_label) else {
        return Cell::Missing(format!("sheet row `{}`", metric.sheet_label));
    };
    if normalise_value(&fact.value) != normalise_value(&metric.display) {
        return Cell::Disagree(format!("value {} != {}", fact.value, metric.display));
    }
    if fact.unit != metric.unit {
        return Cell::Disagree(format!("unit {} != {}", fact.unit, metric.unit));
    }
    if fact.class != metric.class {
        return Cell::Disagree(format!("class {} != {}", fact.class, metric.class));
    }
    if fact.baseline != metric.baseline {
        return Cell::Disagree(format!("baseline {} != {}", fact.baseline, metric.baseline));
    }
    Cell::Agree(fact.value.clone())
}

fn check_readme(metric: &Metric, rows: &[ReadmeRow]) -> Cell {
    let Some(row) = rows.iter().find(|row| row.label == metric.label) else {
        return Cell::Missing(format!("README row `{}`", metric.label));
    };
    if metric.is_count_of() {
        if normalise_value(&row.value) != normalise_value(&metric.display) {
            return Cell::Disagree(format!("value {} != {}", row.value, metric.display));
        }
    } else {
        let Some((shown, unit)) = first_number_and_unit(&row.value) else {
            return Cell::Disagree("value cell carries no number".to_string());
        };
        if !approx_display(metric.value, shown) {
            return Cell::Disagree(format!("value {shown} != {}", metric.value));
        }
        if unit != metric.unit {
            return Cell::Disagree(format!("unit `{unit}` != `{}`", metric.unit));
        }
    }
    if row.class != metric.class {
        return Cell::Disagree(format!("class {} != {}", row.class, metric.class));
    }
    if row.baseline != metric.baseline {
        return Cell::Disagree(format!("baseline {} != {}", row.baseline, metric.baseline));
    }
    Cell::Agree(row.value.clone())
}

fn check_report(metric: &Metric, claims: &HashMap<String, Value>) -> Cell {
    let Some(value) = claims.get(&metric.report_pointer) else {
        return Cell::Missing(format!("report claim {}", metric.report_pointer));
    };
    let Some(found) = value.as_f64() else {
        return Cell::Disagree("report claim is not numeric".to_string());
    };
    if !approx(metric.value, found) {
        return Cell::Disagree(format!("{} != {}", metric.value, found));
    }
    if let (Some(denominator), Some(pointer)) =
        (metric.denominator, metric.denominator_pointer.as_deref())
    {
        let Some(found_denominator) = claims.get(pointer).and_then(Value::as_u64) else {
            return Cell::Missing(format!("report claim {pointer}"));
        };
        if found_denominator != denominator {
            return Cell::Disagree(format!("denominator {found_denominator} != {denominator}"));
        }
    }
    Cell::Agree(value.to_string())
}

fn check_tui(metric: &Metric, tui: &[HeadlineSurface]) -> Cell {
    let Some(surface) = tui.iter().find(|surface| surface.id == metric.id) else {
        return Cell::NotShown;
    };
    if !approx_display(metric.value, surface.numeric) {
        return Cell::Disagree(format!(
            "shows {} but recomputed {}",
            surface.display, metric.value
        ));
    }
    Cell::Agree(surface.display.clone())
}

fn check_spread(metric: &Metric, results: &ClimbResults, require_spread: bool) -> Cell {
    let ratio = match metric.id.as_str() {
        "signing_speedup_medium_c_vs_a" => SPREAD_RATIOS[0],
        "verifying_speedup_medium_cached_c_vs_a" => SPREAD_RATIOS[1],
        _ => return Cell::NotShown,
    };
    match spread(results, ratio) {
        Some(spread) => Cell::Agree(format!(
            "{:.1}\u{2013}{:.1} (n={})",
            spread.min, spread.max, spread.runs
        )),
        None if require_spread => Cell::Disagree(format!("spread pending for {ratio}")),
        None => Cell::Agree("pending".to_string()),
    }
}

/// The Overview's extra numbers (not headline rows, but still published TUI
/// values): wire reduction (C vs B) and signing B vs C. They are checked so no
/// Overview card is exempt.
fn tui_extra_checks(results: &ClimbResults, tui: &[HeadlineSurface]) -> Vec<CheckRow> {
    let mut rows = Vec::new();
    let mut push = |id: &str, expected: Option<f64>| {
        let Some(expected) = expected else {
            return;
        };
        let cell = match tui.iter().find(|surface| surface.id == id) {
            Some(surface) if approx_display(expected, surface.numeric) => {
                Cell::Agree(surface.display.clone())
            }
            Some(surface) => Cell::Disagree(format!(
                "shows {} but recomputed {expected}",
                surface.display
            )),
            None => Cell::NotShown,
        };
        rows.push(CheckRow {
            metric: format!("TUI {id}"),
            raw: format!("{expected}"),
            artifact: Cell::NotShown,
            sheet: Cell::NotShown,
            readme: Cell::NotShown,
            report: Cell::NotShown,
            tui: cell,
            spread: Cell::NotShown,
        });
    };
    let largest = results
        .wire_size
        .iter()
        .max_by_key(|wire| wire.message_count);
    push(
        "wire_reduction_large_c_vs_b",
        largest.map(|wire| wire.reduction_percent),
    );
    let with = results
        .benchmark
        .signing
        .iter()
        .find(|row| row.size.eq_ignore_ascii_case("Medium") && row.condition == "With MTL")
        .map(|row| (row.mean_ns, row.messages));
    let without = results
        .benchmark
        .signing
        .iter()
        .find(|row| row.size.eq_ignore_ascii_case("Medium") && row.condition == "Without MTL")
        .map(|row| (row.mean_ns, row.messages));
    push(
        "signing_speedup_medium_c_vs_b",
        with.zip(without)
            .and_then(|((with, with_n), (without, without_n))| {
                (with > 0.0 && with_n > 0 && without_n > 0)
                    .then(|| (without / without_n as f64) / (with / with_n as f64))
            }),
    );
    rows
}

/// Check every headline and classical metric against every surface.
pub fn check(inputs: &Inputs<'_>) -> Vec<CheckRow> {
    let mut rows = Vec::new();
    for metric in headline_metrics(inputs.results) {
        rows.push(CheckRow {
            metric: metric.id.clone(),
            raw: metric.value_cell(),
            artifact: check_artifact(&metric, inputs.results_json),
            sheet: check_sheet(&metric, inputs.sheet_facts),
            readme: check_readme(&metric, inputs.headline_rows),
            report: check_report(&metric, inputs.claims),
            tui: check_tui(&metric, inputs.tui),
            spread: check_spread(&metric, inputs.results, inputs.require_spread),
        });
    }
    for metric in classical_metrics(inputs.results) {
        rows.push(CheckRow {
            metric: metric.id.clone(),
            raw: metric.value_cell(),
            artifact: check_artifact(&metric, inputs.results_json),
            sheet: check_sheet(&metric, inputs.sheet_facts),
            readme: check_classical_readme(&metric, inputs.classical_rows),
            report: check_report(&metric, inputs.claims),
            tui: check_tui(&metric, inputs.tui),
            spread: Cell::NotShown,
        });
    }
    rows.extend(tui_extra_checks(inputs.results, inputs.tui));
    rows
}

fn check_classical_readme(metric: &Metric, rows: &[ReadmeRow]) -> Cell {
    let algorithm = metric
        .sheet_label
        .strip_prefix("Medium — ")
        .and_then(|rest| rest.strip_suffix(" signature bytes"))
        .or_else(|| {
            metric
                .sheet_label
                .strip_prefix("Medium — ")
                .and_then(|rest| rest.strip_suffix(" verify per message"))
        });
    let Some(algorithm) = algorithm else {
        return Cell::Missing("classical label shape".to_string());
    };
    let Some(row) = rows.iter().find(|row| row.label.starts_with(algorithm)) else {
        return Cell::Missing(format!("README classical row for {algorithm}"));
    };
    // The parser keeps only four cells, so the signature and verify cells are
    // packed into `value` as `bytes | verify`.
    let Some((bytes_cell, verify_cell)) = row.value.split_once('|') else {
        return Cell::Missing("classical cells".to_string());
    };
    let cell = if metric.id.ends_with("_bytes") {
        bytes_cell.trim()
    } else {
        verify_cell.trim()
    };
    let Some((shown, unit)) = first_number_and_unit(cell) else {
        return Cell::Disagree("classical cell carries no number".to_string());
    };
    // The README publishes the display unit (µs for verify), while
    // `metric.value` is stored in nanoseconds; compare the rendered value.
    let Some(expected) = metric.display.replace(',', "").parse::<f64>().ok() else {
        return Cell::Disagree(format!(
            "classical display `{}` carries no number",
            metric.display
        ));
    };
    if !approx_display(expected, shown) {
        return Cell::Disagree(format!("value {shown} != {expected}"));
    }
    if unit != metric.unit {
        return Cell::Disagree(format!("unit `{unit}` != `{}`", metric.unit));
    }
    if row.class != metric.class {
        return Cell::Disagree(format!("class {} != {}", row.class, metric.class));
    }
    if row.baseline != metric.baseline {
        return Cell::Disagree(format!("baseline {} != {}", row.baseline, metric.baseline));
    }
    Cell::Agree(cell.to_string())
}

/// Parse the classical README table into rows whose `value` cell packs the
/// signature and verify cells as `bytes | verify`.
pub(crate) fn parse_classical_rows(text: &str) -> Vec<ReadmeRow> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|line| line.trim() == CLASSICAL_BEGIN) else {
        return Vec::new();
    };
    let Some(stop) = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim() == CLASSICAL_END)
        .map(|(idx, _)| idx)
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in &lines[start + 1..stop] {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 7 {
            continue;
        }
        let label = cells[1];
        if label == "Algorithm" || label.starts_with("---") {
            continue;
        }
        rows.push(ReadmeRow {
            label: label.to_string(),
            value: format!("{} | {}", cells[2], cells[3]),
            class: cells[4].to_string(),
            baseline: cells[5].to_string(),
        });
    }
    rows
}

/// What every surface must say about the emulated-RTT section.
///
/// Returns an empty string when the artifact carries a usable measurement.
/// Otherwise the line names the pending state: an absent section, a
/// placeholder, or a quarantined artifact (invalid harness) are all pending.
fn rtt_pending_status(results_json: &Value) -> String {
    let Some(section) = results_json.pointer("/dns_rtt") else {
        return "RTT: pending (no emulated-RTT measurement recorded)".to_string();
    };
    if section.is_null() {
        return "RTT: pending (no emulated-RTT measurement recorded)".to_string();
    }
    if let Some(reason) = section.get("superseded").and_then(Value::as_str) {
        return format!("RTT: pending (artifact superseded - {reason})");
    }
    let measured = section.get("status").and_then(Value::as_str) == Some("measured");
    let has_cells = section
        .get("cells")
        .and_then(Value::as_array)
        .is_some_and(|cells| !cells.is_empty());
    if !measured || !has_cells {
        return "RTT: pending (no measured cells)".to_string();
    }
    String::new()
}

/// Fail when the artifact is pending but the quote sheet still publishes RTT
/// numbers, and print the pending line so the gate itself shows the state.
fn check_rtt_pending(results_json: &Value, sheet: &str) -> Result<()> {
    let pending = rtt_pending_status(results_json);
    if pending.is_empty() {
        let cells = results_json
            .pointer("/dns_rtt/cells")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        println!("verify-numbers: RTT: measured ({cells} cells)");
        return Ok(());
    }
    if !sheet.contains("RTT: pending") {
        bail!(
            "the results document reports {pending}, but the quote sheet carries no `RTT: pending` \
             marker — regenerate it with `cargo xtask deck-facts`"
        );
    }
    println!("verify-numbers: {pending}");
    Ok(())
}

fn read_inputs(args: &VerifyNumbersArgs) -> Result<(ClimbResults, Value, String, String)> {
    let results_path = args.root.join(&args.results);
    let results_text = std::fs::read_to_string(&results_path)
        .with_context(|| format!("failed to read {}", results_path.display()))?;
    let results: ClimbResults = serde_json::from_str(&results_text)
        .with_context(|| format!("failed to parse {}", results_path.display()))?;
    let results_json: Value = serde_json::from_str(&results_text)
        .with_context(|| format!("failed to parse {}", results_path.display()))?;
    let sheet_path = args.root.join(&args.sheet);
    let sheet = std::fs::read_to_string(&sheet_path)
        .with_context(|| format!("failed to read {}", sheet_path.display()))?;
    let readme_path = args.root.join(&args.readme);
    let readme = std::fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    Ok((results, results_json, sheet, readme))
}

fn read_report(args: &VerifyNumbersArgs) -> Result<String> {
    let path = args.root.join(&args.report);
    std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
}

/// Recompute, compare and report. Non-zero exit when any surface disagrees.
pub(crate) fn cmd_verify_numbers(args: VerifyNumbersArgs) -> Result<()> {
    let (results, results_json, sheet, readme) = read_inputs(&args)?;
    check_rtt_pending(&results_json, &sheet)?;
    let report = read_report(&args)?;
    let facts = parse_quote_sheet(&sheet);
    if facts.is_empty() {
        bail!(
            "{} carries no fact rows — regenerate it with `cargo xtask deck-facts`",
            args.root.join(&args.sheet).display()
        );
    }
    let headline_rows = parse_marked_rows(&readme, HEADLINE_BEGIN, HEADLINE_END);
    if headline_rows.is_empty() {
        bail!(
            "{} has no rows between {HEADLINE_BEGIN} and {HEADLINE_END}",
            args.root.join(&args.readme).display()
        );
    }
    let classical_rows = parse_classical_rows(&readme);
    let claims = parse_claims(&report);
    if claims.is_empty() {
        bail!(
            "{} carries no embedded claims registry",
            args.root.join(&args.report).display()
        );
    }
    let view = ResultsView::new(results.clone(), None, ReportHealth::Agree);
    let tui = view.headline_surface();

    let inputs = Inputs {
        results: &results,
        results_json: &results_json,
        sheet_facts: &facts,
        headline_rows: &headline_rows,
        classical_rows: &classical_rows,
        claims: &claims,
        tui: &tui,
        require_spread: args.require_spread,
    };
    let rows = check(&inputs);

    println!(
        "verify-numbers: {} metrics across artifact / sheet / README / report / TUI",
        rows.len()
    );
    println!(
        "{:<46} {:<16} {:<16} {:<14} {:<14} {:<14} {:<10}",
        "metric", "recomputed", "artifact", "sheet", "README", "report", "TUI"
    );
    for row in &rows {
        println!(
            "{:<46} {:<16} {:<16} {:<14} {:<14} {:<14} {:<10}",
            row.metric,
            truncate(&row.raw, 16),
            truncate(&row.artifact.to_string(), 16),
            truncate(&row.sheet.to_string(), 14),
            truncate(&row.readme.to_string(), 14),
            truncate(&row.report.to_string(), 14),
            truncate(&row.tui.to_string(), 10),
        );
        if !matches!(row.spread, Cell::NotShown) {
            println!("  spread: {}", row.spread);
        }
    }

    let failures: Vec<&CheckRow> = rows.iter().filter(|row| row.failed()).collect();
    if failures.is_empty() {
        check_dataset_digests(&args)?;
        println!("verify-numbers: every published value agrees with its raw rows");
        return Ok(());
    }

    println!("\ndisagreements:");
    for row in &failures {
        for (surface, cell) in [
            ("artifact", &row.artifact),
            ("sheet", &row.sheet),
            ("README", &row.readme),
            ("report", &row.report),
            ("TUI", &row.tui),
            ("spread", &row.spread),
        ] {
            if cell.failed() {
                println!("  {} | {} | {}", row.metric, surface, cell);
            }
        }
    }
    bail!(
        "{} published value(s) disagree with the raw rows; regenerate the run and every surface",
        failures.len()
    )
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

/// Build the in-memory inputs from the committed fixture files. Kept for the
/// tests, which exercise one surface at a time.
#[cfg(test)]
type FixtureInputs = (
    Vec<FactRow>,
    Vec<ReadmeRow>,
    Vec<ReadmeRow>,
    HashMap<String, Value>,
    Vec<HeadlineSurface>,
);

/// Build the in-memory inputs from the committed fixture files. Kept for the
/// tests, which exercise one surface at a time.
#[cfg(test)]
fn fixture_inputs<'a>(
    results: &'a ClimbResults,
    _results_json: &'a Value,
    sheet: &'a str,
    readme: &'a str,
    report: &'a str,
) -> FixtureInputs {
    let facts = parse_quote_sheet(sheet);
    let headline_rows = parse_marked_rows(readme, HEADLINE_BEGIN, HEADLINE_END);
    let classical_rows = parse_classical_rows(readme);
    let claims = parse_claims(report);
    let view = ResultsView::new(results.clone(), None, ReportHealth::Agree);
    (
        facts,
        headline_rows,
        classical_rows,
        claims,
        view.headline_surface(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_results() -> ClimbResults {
        serde_json::from_str(include_str!("../../docs/sample-output/climb-results.json"))
            .expect("committed sample parses")
    }

    fn sample_json() -> Value {
        serde_json::from_str(include_str!("../../docs/sample-output/climb-results.json"))
            .expect("committed sample parses")
    }

    const SHEET: &str = include_str!("../../docs/deck-facts.md");
    const README: &str = include_str!("../../README.md");
    const REPORT: &str = include_str!("../../docs/sample-output/report.html");

    fn run(
        results: &ClimbResults,
        results_json: &Value,
        sheet: &str,
        readme: &str,
        report: &str,
        require_spread: bool,
    ) -> Vec<CheckRow> {
        let (facts, headline_rows, classical_rows, claims, tui) =
            fixture_inputs(results, results_json, sheet, readme, report);
        let inputs = Inputs {
            results,
            results_json,
            sheet_facts: &facts,
            headline_rows: &headline_rows,
            classical_rows: &classical_rows,
            claims: &claims,
            tui: &tui,
            require_spread,
        };
        check(&inputs)
    }

    #[test]
    fn every_surface_agrees_on_the_committed_fixture() {
        let results = sample_results();
        let json = sample_json();
        let rows = run(&results, &json, SHEET, README, REPORT, false);
        let failures: Vec<String> = rows
            .iter()
            .filter(|row| row.failed())
            .map(|row| format!("{:?}", row))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn an_injected_artifact_disagreement_is_caught() {
        let results = sample_results();
        let mut json = sample_json();
        let idx = results
            .comparisons
            .iter()
            .position(|c| c.size == "Medium")
            .unwrap();
        json["comparisons"][idx]["signing_speedup_c_vs_a"] = Value::from(1.0);
        let rows = run(&results, &json, SHEET, README, REPORT, false);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.artifact.failed(), "{:?}", row.artifact);
    }

    #[test]
    fn an_injected_sheet_disagreement_is_caught() {
        let results = sample_results();
        let json = sample_json();
        let sheet = SHEET.replacen("| 9,434 | × | DERIVED |", "| 1 | × | DERIVED |", 1);
        assert_ne!(sheet, SHEET, "fixture row shape changed");
        let rows = run(&results, &json, &sheet, README, REPORT, false);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.sheet.failed(), "{:?}", row.sheet);
    }

    #[test]
    fn an_injected_readme_disagreement_is_caught() {
        let results = sample_results();
        let json = sample_json();
        let readme = README.replacen("| 9,434 × | DERIVED |", "| 1 × | DERIVED |", 1);
        assert_ne!(readme, README, "fixture row shape changed");
        let rows = run(&results, &json, SHEET, &readme, REPORT, false);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.readme.failed(), "{:?}", row.readme);
    }

    #[test]
    fn an_injected_report_disagreement_is_caught() {
        let results = sample_results();
        let json = sample_json();
        let mut claims = parse_claims(REPORT);
        let metric = headline_metrics(&results)
            .into_iter()
            .find(|metric| metric.id == "signing_speedup_medium_c_vs_a")
            .unwrap();
        claims.insert(metric.report_pointer.clone(), Value::from(1.0));
        let (facts, headline_rows, classical_rows, _, tui) =
            fixture_inputs(&results, &json, SHEET, README, REPORT);
        let inputs = Inputs {
            results: &results,
            results_json: &json,
            sheet_facts: &facts,
            headline_rows: &headline_rows,
            classical_rows: &classical_rows,
            claims: &claims,
            tui: &tui,
            require_spread: false,
        };
        let rows = check(&inputs);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.report.failed(), "{:?}", row.report);
    }

    #[test]
    fn an_injected_tui_disagreement_is_caught() {
        let mut results = sample_results();
        results
            .comparisons
            .iter_mut()
            .find(|c| c.size == "Medium")
            .unwrap()
            .signing_speedup_c_vs_a = Some(1.0);
        let json = sample_json();
        let rows = run(&results, &json, SHEET, README, REPORT, false);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.tui.failed(), "{:?}", row.tui);
    }

    #[test]
    fn require_spread_fails_when_a_headline_spread_is_pending() {
        let mut results = sample_results();
        results.derived.stability = None;
        let json = sample_json();
        let rows = run(&results, &json, SHEET, README, REPORT, true);
        let row = rows
            .iter()
            .find(|row| row.metric == "signing_speedup_medium_c_vs_a")
            .unwrap();
        assert!(row.spread.failed(), "{:?}", row.spread);
    }

    #[test]
    fn the_committed_dataset_digests_match_a_fresh_generation() {
        let text = include_str!("../../docs/sample-output/dataset-digests.json");
        let document: climb_dns::DatasetDigestDocument =
            serde_json::from_str(text).expect("committed digest document parses");
        let outcome = document
            .check_limited(
                climb_bench::DEFAULT_WIRE_SEED,
                climb_dns::ROUTINE_DIGEST_MAX_COUNT,
            )
            .expect("committed digests match a fresh generation");
        assert!(
            outcome.checked >= 3,
            "the routine gate must cover Small, OneK and Medium"
        );
        assert_eq!(
            outcome.skipped,
            vec!["Large".to_string()],
            "only the million-message entry is deferred"
        );
    }

    #[test]
    fn a_malformed_dataset_digest_document_names_the_path() {
        let dir = std::env::temp_dir().join("climb-verify-numbers-digests");
        let path = dir.join("bad.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        let args = VerifyNumbersArgs {
            root: PathBuf::from("."),
            results: PathBuf::new(),
            sheet: PathBuf::new(),
            readme: PathBuf::new(),
            report: PathBuf::new(),
            dataset_digests: path.clone(),
            deep_digests: false,
            require_spread: false,
        };
        let error = check_dataset_digests(&args).expect_err("malformed document must fail");
        assert!(
            format!("{error:#}").contains(&path.display().to_string()),
            "error must name the path: {error:#}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pointer_resolution_handles_arrays_and_escapes() {
        let value: Value = serde_json::from_str(r#"{"a":[{"b":1}],"x/y":2}"#).unwrap();
        assert_eq!(resolve(&value, "/a/0/b").unwrap().as_u64(), Some(1));
        assert_eq!(resolve(&value, "/x~1y").unwrap().as_u64(), Some(2));
        assert!(resolve(&value, "/missing").is_none());
    }

    #[test]
    fn an_absent_or_superseded_rtt_section_is_reported_as_pending() {
        let absent = serde_json::json!({});
        assert!(rtt_pending_status(&absent).starts_with("RTT: pending"));

        let superseded = serde_json::json!({
            "dns_rtt": {
                "status": "measured",
                "superseded": "harness invalid",
                "cells": [{"median_ms": 200.99}]
            }
        });
        let line = rtt_pending_status(&superseded);
        assert_eq!(line, "RTT: pending (artifact superseded - harness invalid)");
        assert!(!line.contains("200.99"));
    }

    #[test]
    fn a_measured_rtt_section_is_not_pending() {
        let measured = serde_json::json!({
            "dns_rtt": { "status": "measured", "cells": [{"median_ms": 20.1}] }
        });
        assert!(rtt_pending_status(&measured).is_empty());
    }

    #[test]
    fn a_pending_artifact_without_the_sheet_marker_fails_the_gate() {
        let absent = serde_json::json!({});
        let error = check_rtt_pending(&absent, "no marker here").expect_err("must fail");
        assert!(format!("{error:#}").contains("RTT: pending"), "{error:#}");
        check_rtt_pending(&absent, "> RTT: pending (no measurement)").expect("marker present");
    }
}
