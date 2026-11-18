//! `cargo xtask evidence-sheet`: render the one-page A4 evidence sheet.
//!
//! The five numbers are the headline metrics from `climb_report::numbers`,
//! recomputed from the committed artifact's raw rows. Their labels, evidence
//! classes, baselines, rendered values and JSON pointers all come from the
//! same definitions the verifier and the dashboard use, so the sheet cannot
//! publish a value that no other surface agrees with.
//!
//! The page carries no wall-clock timestamp and no remote asset: it prints the
//! artifact's own `generated_at` and `version`, so two renders of the same
//! artifact are byte-identical.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

use climb_report::numbers::{headline_metrics, Metric, SCOPE_NOTE};
use climb_report::results::ClimbResults;

/// The five headline rows, in reading order. The identifiers are the ones
/// `climb_report::numbers` defines; a missing identifier is an error, never a
/// silently omitted row.
const HEADLINE_IDS: [&str; 5] = [
    "wire_reduction_medium_c_vs_a",
    "signing_speedup_medium_c_vs_a",
    "verifying_speedup_medium_cached_c_vs_a",
    "udp_fit_c",
    "dns_reduction",
];

/// The three configurations every headline row is compared against.
const BASELINES: [(&str, &str); 3] = [
    (
        "A",
        "plain SLH-DSA: one full signature per message, no ladder",
    ),
    (
        "B",
        "MTL without amortisation: one complete MTL signature per message",
    ),
    (
        "C",
        "MTL amortised: one signed ladder per batch and a condensed path per message",
    ),
];

/// The committed results artifact the sheet is generated from.
const DEFAULT_RESULTS: &str = "docs/sample-output/climb-results.json";

#[derive(Args)]
pub(crate) struct EvidenceSheetArgs {
    /// Results artifact to read the five headline numbers from.
    #[arg(long, default_value = DEFAULT_RESULTS)]
    pub(crate) results: PathBuf,
    /// Printable HTML sheet to write.
    #[arg(long, default_value = "docs/EVIDENCE-SHEET.html")]
    pub(crate) out: PathBuf,
    /// Compare the committed sheet with a fresh render instead of writing.
    #[arg(long)]
    pub(crate) check: bool,
}

pub(crate) fn cmd_evidence_sheet(args: EvidenceSheetArgs) -> Result<()> {
    let results = load_results(&args.results)?;
    let html = render_sheet(&results, &args.results)?;
    if args.check {
        let current = fs::read_to_string(&args.out).with_context(|| {
            format!(
                "failed to read {} (run `cargo xtask evidence-sheet`)",
                args.out.display()
            )
        })?;
        if current != html {
            bail!(
                "{} is stale; run `cargo xtask evidence-sheet` and commit the result",
                args.out.display()
            );
        }
        println!(
            "evidence-sheet: {} number(s) match {}",
            HEADLINE_IDS.len(),
            args.out.display()
        );
        return Ok(());
    }
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&args.out, &html)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!(
        "wrote {} ({} numbers, source {})",
        args.out.display(),
        HEADLINE_IDS.len(),
        args.results.display()
    );
    Ok(())
}

fn load_results(path: &Path) -> Result<ClimbResults> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {} as a results artifact", path.display()))
}

/// Select the five headline metrics by identifier, preserving the documented
/// reading order. A missing row is a hard error naming the identifier and the
/// command that regenerates the artifact.
pub(crate) fn select_headline(metrics: &[Metric]) -> Result<Vec<&Metric>> {
    HEADLINE_IDS
        .iter()
        .map(|id| {
            metrics
                .iter()
                .find(|metric| metric.id == *id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "the artifact carries no `{id}` headline row; \
                     regenerate it with `cargo xtask consolidate`"
                    )
                })
        })
        .collect()
}

/// The value cell printed on the sheet. A `k of n` count keeps its unit so a
/// reader does not mistake it for a share.
fn value_cell(metric: &Metric) -> String {
    if metric.is_count_of() {
        format!("{} {}", metric.display, metric.unit)
    } else {
        metric.value_cell()
    }
}

fn class_slug(class: &str) -> String {
    let slug: String = class
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if slug.is_empty() {
        "other".to_string()
    } else {
        slug
    }
}

pub(crate) fn render_sheet(results: &ClimbResults, source: &Path) -> Result<String> {
    let metrics = headline_metrics(results);
    let selected = select_headline(&metrics)?;
    let source = source.display().to_string();

    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<meta name=\"generator\" content=\"cargo xtask evidence-sheet\">\n");
    html.push_str("<title>CLIMB — evidence sheet</title>\n<style>\n");
    html.push_str(
        "@page { size: A4 portrait; margin: 11mm; }\n\
         * { box-sizing: border-box; }\n\
         body { margin: 0; color: #14161a; background: #fff; font: 9pt/1.35 \"DejaVu Sans\", Arial, sans-serif; }\n\
         header { display: flex; justify-content: space-between; gap: 8mm; border-bottom: 1.6pt solid #14161a; padding-bottom: 2.5mm; }\n\
         .kicker { margin: 0 0 1mm; color: #a33f00; font: 700 7pt/1.2 \"DejaVu Sans Mono\", monospace; letter-spacing: .14em; text-transform: uppercase; }\n\
         h1 { margin: 0; font-size: 19pt; line-height: 1.05; letter-spacing: .01em; }\n\
         .sub { margin: .8mm 0 0; color: #4a4f57; font-size: 8.5pt; }\n\
         .stamp { text-align: right; color: #4a4f57; font-size: 7.5pt; }\n\
         .stamp code { font-size: 7pt; }\n\
         table.numbers { width: 100%; border-collapse: collapse; margin-top: 4mm; }\n\
         table.numbers th { text-align: left; font-size: 7pt; text-transform: uppercase; letter-spacing: .08em; color: #4a4f57; border-bottom: .6pt solid #b9bec6; padding: 0 2mm 1.4mm 0; }\n\
         table.numbers td { vertical-align: top; padding: 2.7mm 2mm 2.7mm 0; border-bottom: .5pt solid #d9dce1; }\n\
         td.value { width: 45mm; font-size: 19pt; font-weight: 800; font-variant-numeric: tabular-nums; white-space: nowrap; }\n\
         .label { font-weight: 650; }\n\
         .meta { margin-top: .5mm; color: #4a4f57; font-size: 7pt; }\n\
         .meta code { font-size: 6.6pt; }\n\
         .pill { display: inline-block; border: .6pt solid #14161a; border-radius: 999px; padding: .2mm 1.6mm; font-size: 6.8pt; font-weight: 800; letter-spacing: .04em; }\n\
         .pill.derived { color: #0b4f8a; border-color: #0b4f8a; }\n\
         .pill.analytical { color: #6b3b00; border-color: #6b3b00; }\n\
         .pill.measured { color: #0b6b3a; border-color: #0b6b3a; }\n\
         .pill.extrapolated { color: #6b2a6b; border-color: #6b2a6b; }\n\
         .base { display: inline-block; min-width: 5mm; text-align: center; border: .8pt solid #14161a; font-weight: 800; padding: .2mm 1.4mm; }\n\
         .grid { display: grid; grid-template-columns: 1.35fr 1fr; gap: 6mm; margin-top: 4mm; }\n\
         section h2 { margin: 0 0 1.4mm; font-size: 7.6pt; text-transform: uppercase; letter-spacing: .08em; border-bottom: .5pt solid #b9bec6; padding-bottom: .8mm; }\n\
         .legend { margin: 0; padding: 0; list-style: none; }\n\
         .legend li { margin: 0 0 1.4mm; display: flex; gap: 2mm; break-inside: avoid; }\n\
         .legend .base { flex: 0 0 auto; align-self: flex-start; }\n\
         .scope { border: 1pt solid #14161a; padding: 2.5mm; background: #f5f3ee; font-weight: 650; }\n\
         footer { margin-top: 4mm; border-top: .8pt solid #14161a; padding-top: 1.8mm; color: #4a4f57; font-size: 7pt; }\n\
         footer p { margin: 0 0 1mm; }\n\
         footer code { font-size: 6.6pt; color: #14161a; }\n\
         @media print { body { -webkit-print-color-adjust: exact; print-color-adjust: exact; } }\n",
    );
    html.push_str("</style>\n</head>\n<body>\n<header>\n<div>\n");
    html.push_str("<p class=\"kicker\">PS-036 · post-quantum DNSSEC · MTL benchmark</p>\n");
    html.push_str("<h1>CLIMB — evidence sheet</h1>\n");
    html.push_str(
        "<p class=\"sub\">Five numbers of record, each with its evidence class, its baseline and the artifact field it comes from.</p>\n",
    );
    html.push_str("</div>\n<div class=\"stamp\">");
    html.push_str(&format!(
        "artifact <code>{}</code><br>generated <code>{}</code><br>schema <code>{}</code>",
        escape(&source),
        escape(&results.generated_at),
        escape(&results.version)
    ));
    html.push_str("</div>\n</header>\n");

    html.push_str("<table class=\"numbers\">\n<thead><tr><th>Value</th><th>What it is</th><th>Class</th><th>Baseline</th></tr></thead>\n<tbody>\n");
    for metric in selected {
        html.push_str("<tr>");
        html.push_str(&format!(
            "<td class=\"value\">{}</td>",
            escape(&value_cell(metric))
        ));
        html.push_str(&format!(
            "<td><span class=\"label\">{}</span><div class=\"meta\"><code>{}</code></div></td>",
            escape(&metric.label.replace('`', "")),
            escape(&metric.pointer)
        ));
        html.push_str(&format!(
            "<td><span class=\"pill {}\">{}</span></td>",
            class_slug(metric.class),
            escape(metric.class)
        ));
        html.push_str(&format!(
            "<td><span class=\"base\">{}</span></td>",
            escape(metric.baseline)
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody>\n</table>\n");

    html.push_str(
        "<div class=\"grid\">\n<section>\n<h2>Baselines compared</h2>\n<ul class=\"legend\">\n",
    );
    for (id, definition) in BASELINES {
        html.push_str(&format!(
            "<li><span class=\"base\">{}</span><span>{}</span></li>\n",
            escape(id),
            escape(definition)
        ));
    }
    html.push_str("</ul>\n</section>\n<section>\n<h2>Scope</h2>\n");
    html.push_str(&format!("<p class=\"scope\">{}</p>\n", escape(SCOPE_NOTE)));
    html.push_str("</section>\n</div>\n");

    html.push_str("<footer>\n");
    html.push_str(&format!(
        "<p><strong>Repository:</strong> <a href=\"{}\">{}</a></p>\n",
        escape(env!("CARGO_PKG_REPOSITORY")),
        escape(env!("CARGO_PKG_REPOSITORY"))
    ));
    html.push_str(&format!(
        "<p>Generated by <code>cargo xtask evidence-sheet</code> from <code>{}</code>. Every value, class and baseline is recomputed from the artifact's raw rows by <code>climb-report::numbers</code>; nothing on this page is typed by hand. <code>--check</code> fails when this page is stale.</p>\n",
        escape(&source)
    ));
    html.push_str("</footer>\n</body>\n</html>\n");
    Ok(html)
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

    fn sample_results() -> ClimbResults {
        serde_json::from_str(include_str!("../../docs/sample-output/climb-results.json"))
            .expect("the committed sample artifact parses")
    }

    #[test]
    fn the_five_headline_numbers_are_selected_by_id_in_order() {
        let results = sample_results();
        let metrics = headline_metrics(&results);
        let selected = select_headline(&metrics).expect("the sample carries all five rows");
        let ids: Vec<&str> = selected.iter().map(|metric| metric.id.as_str()).collect();
        assert_eq!(ids, HEADLINE_IDS);
    }

    #[test]
    fn every_number_carries_its_recorded_class_and_baseline() {
        let results = sample_results();
        let metrics = headline_metrics(&results);
        let selected = select_headline(&metrics).expect("all five rows");
        assert_eq!(selected[0].display, "96.508");
        assert_eq!(selected[0].class, "DERIVED");
        assert_eq!(selected[0].baseline, "A");
        assert_eq!(selected[1].display, "9,434");
        assert_eq!(selected[1].class, "DERIVED");
        assert_eq!(selected[1].baseline, "A");
        assert_eq!(selected[2].display, "77.34");
        assert_eq!(selected[2].class, "DERIVED");
        assert_eq!(selected[2].baseline, "A");
        assert_eq!(selected[3].display, "11 of 12");
        assert_eq!(selected[3].class, "ANALYTICAL");
        assert_eq!(selected[3].baseline, "C");
        assert_eq!(selected[4].display, "97.307");
        assert_eq!(selected[4].class, "DERIVED");
        assert_eq!(selected[4].baseline, "B");
        assert!(selected.iter().all(|metric| !metric.pointer.is_empty()));
    }

    #[test]
    fn a_tampered_artifact_moves_the_printed_value() {
        let mut results = sample_results();
        let medium = results
            .wire_size
            .iter_mut()
            .find(|row| row.size.eq_ignore_ascii_case("Medium"))
            .expect("the sample carries a Medium wire row");
        let plain = medium.plain_total_bytes;
        medium.with_mtl_total_bytes = plain / 2;

        let html = render_sheet(&results, Path::new(DEFAULT_RESULTS))
            .expect("the sheet renders from the tampered artifact");
        assert!(
            html.contains("50.000 %"),
            "the reduction is recomputed from the tampered totals"
        );
        assert!(
            !html.contains("96.508 %"),
            "the committed value must not survive a changed artifact"
        );
    }

    #[test]
    fn a_missing_headline_row_is_a_hard_error_naming_the_id() {
        let mut results = sample_results();
        results.dns_anatomy = None;
        let error = render_sheet(&results, Path::new(DEFAULT_RESULTS))
            .expect_err("a missing DNS row must fail");
        let message = error.to_string();
        assert!(message.contains("dns_reduction"), "{message}");
        assert!(message.contains("cargo xtask consolidate"), "{message}");
    }

    #[test]
    fn the_sheet_is_an_a4_page_without_scripts_or_remote_assets() {
        let html =
            render_sheet(&sample_results(), Path::new(DEFAULT_RESULTS)).expect("the sheet renders");
        assert!(html.contains("@page { size: A4 portrait;"), "{html}");
        for forbidden in [
            "<script",
            "<img",
            "<link",
            "src=\"http",
            "src='http",
            "url(http",
            "@import",
        ] {
            assert!(!html.contains(forbidden), "sheet carries {forbidden}");
        }
        assert!(
            !html.contains('`'),
            "markdown code ticks must not survive into the printable page"
        );
    }

    #[test]
    fn the_scope_sentence_and_repository_are_printed() {
        let html =
            render_sheet(&sample_results(), Path::new(DEFAULT_RESULTS)).expect("the sheet renders");
        assert!(html.contains(SCOPE_NOTE), "{html}");
        assert!(html.contains(env!("CARGO_PKG_REPOSITORY")), "{html}");
        assert!(html.contains("not a security proof"), "{html}");
    }

    #[test]
    fn the_baseline_legend_names_all_three_configurations() {
        let html =
            render_sheet(&sample_results(), Path::new(DEFAULT_RESULTS)).expect("the sheet renders");
        for (id, definition) in BASELINES {
            assert!(html.contains(&format!(">{id}<")), "baseline {id}");
            assert!(html.contains(definition), "definition {id}");
        }
    }

    #[test]
    fn the_committed_sheet_is_a_fresh_render_of_the_committed_artifact() {
        let html =
            render_sheet(&sample_results(), Path::new(DEFAULT_RESULTS)).expect("the sheet renders");
        let committed = include_str!("../../docs/EVIDENCE-SHEET.html");
        assert_eq!(
            committed, html,
            "the committed sheet is stale; run `cargo xtask evidence-sheet`"
        );
    }
}
