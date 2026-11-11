//! The headline numbers of record, recomputed from raw rows.
//!
//! Every published headline figure has exactly one definition, here. The
//! definition starts from the raw measurement rows (`plain`, `benchmark`,
//! `wire_size`, `memory`, `ladder`, `classical`, `derived.udp_fit.rows`) and
//! recomputes the value; it never starts from another derived field. The
//! renderers in this module build the README tables from those values, and
//! `cargo xtask verify-numbers` checks every surface (artifact fields, quote
//! sheet, README, HTML report, TUI Overview) against the same recomputation.
//!
//! A surface that disagrees is a defect: the fix is to regenerate the
//! artifact and every surface from one run, never to edit a published value.

use std::path::PathBuf;

use crate::charts::fmt_count;
use crate::error::ReportError;
use crate::results::{size_display, ClimbResults, EvidenceClass};

/// Dataset size the headline comparison rows are quoted at.
pub const HEADLINE_SIZE: &str = "Medium";

/// Leaf count behind the published ladder-shape row.
pub const LADDER_LEAF_COUNT: usize = 100;

/// Stability ratio names the headline spread column requires.
pub const SPREAD_RATIOS: [&str; 2] = [
    "signing_speedup_c_vs_a_model_medium",
    "verifying_speedup_cached_c_vs_a_model_medium",
];

/// One published number, recomputed from raw rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    /// Stable identifier used by the verifier and the TUI surface.
    pub id: String,
    /// Label as it appears in the README table.
    pub label: String,
    /// Unit the quote sheet records; the display cell omits it for a
    /// `k of n` count.
    pub unit: &'static str,
    /// Evidence class label (`MEASURED`, `DERIVED`, `ANALYTICAL`).
    pub class: &'static str,
    /// Baseline the number is compared against (`A`, `B`, `C`, `n/a`).
    pub baseline: &'static str,
    /// Recomputed numeric value (the numerator for a `k of n` count).
    pub value: f64,
    /// Denominator when the value is published as `k of n`.
    pub denominator: Option<u64>,
    /// Canonical value cell, without the unit.
    pub display: String,
    /// JSON pointer to the matching artifact field.
    pub pointer: String,
    /// JSON pointer to the denominator when [`Metric::denominator`] is set.
    pub denominator_pointer: Option<String>,
    /// JSON pointer to the matching claim in the HTML report.
    pub report_pointer: String,
    /// Exact label in the generated quote sheet.
    pub sheet_label: String,
}

impl Metric {
    /// Value cell for the README table: no unit for a `k of n` count.
    pub fn value_cell(&self) -> String {
        if self.denominator.is_some() {
            self.display.clone()
        } else {
            format!("{} {}", self.display, self.unit)
        }
    }

    /// `true` when the value is a `k of n` count rather than a scalar.
    pub fn is_count_of(&self) -> bool {
        self.denominator.is_some()
    }
}

/// Format a reduction percentage the way the quote sheet records it.
fn fmt_percent(value: f64) -> String {
    format!("{value:.3}")
}

/// The size-and-baseline descriptor every surface uses beside a ratio, so a
/// reader never has to guess which dataset and comparison a card quotes.
///
/// The separator is a single space rather than a dash so the descriptor fits
/// the narrowest KPI card (14 inner columns at 80x24) without truncating the
/// baseline.
pub fn kpi_caption(size: &str, baseline: &str) -> String {
    format!("{} {}", size_display(size), baseline)
}

/// Format a ratio (a speed-up factor) for every surface: README, quote sheet,
/// HTML report and the TUI.
///
/// The rendering keeps four significant digits, groups the integer part with
/// thousands separators and trims trailing zeros. Four significant digits
/// means the value parses back to within 0.05% of the input, so a rendered
/// `0x` or `1x` only ever appears when the ratio really is within 5% of zero
/// or one — it is never a rounding artefact.
pub fn fmt_ratio(value: f64) -> String {
    if !value.is_finite() {
        return "\u{2014}".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs().log10().floor() as i32;
    let decimals = (3 - magnitude).clamp(0, 17) as usize;
    let mut text = format!("{:.*}", decimals, value);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "0" || text == "-0" {
        // The requested precision could not represent a tiny non-zero ratio;
        // fall back to scientific notation rather than printing a false zero.
        return format!("{value:.3e}");
    }
    group_integer_part(&text)
}

/// Insert thousands separators into the integer part of a decimal string.
fn group_integer_part(text: &str) -> String {
    if text.contains('e') || text.contains('E') {
        return text.to_string();
    }
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let (integer, fraction) = match rest.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (rest, None),
    };
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Format a per-message time in microseconds with two decimals.
fn fmt_micros(ns: f64) -> String {
    format!("{:.2}", ns / 1000.0)
}

fn wire_row<'a>(
    results: &'a ClimbResults,
    size: &str,
) -> Option<&'a crate::results::WireSizeResult> {
    results
        .wire_size
        .iter()
        .find(|w| w.size.eq_ignore_ascii_case(size))
}

fn plain_row<'a>(
    results: &'a ClimbResults,
    size: &str,
) -> Option<&'a crate::results::PlainTimingResult> {
    results
        .plain
        .iter()
        .find(|p| p.size.eq_ignore_ascii_case(size))
}

/// Per-message signing cost from a benchmark row.
fn signing_per_message(results: &ClimbResults, size: &str, condition: &str) -> Option<f64> {
    results
        .benchmark
        .signing
        .iter()
        .find(|r| r.size.eq_ignore_ascii_case(size) && r.condition == condition)
        .filter(|r| r.messages > 0)
        .map(|r| r.mean_ns / r.messages as f64)
}

/// Per-message verifying cost from a benchmark row.
fn verifying_per_message(results: &ClimbResults, size: &str, condition: &str) -> Option<f64> {
    results
        .benchmark
        .verifying
        .iter()
        .find(|r| r.size.eq_ignore_ascii_case(size) && r.condition == condition)
        .filter(|r| r.messages > 0)
        .map(|r| r.mean_ns / r.messages as f64)
}

fn memory_row<'a>(
    results: &'a ClimbResults,
    size: &str,
) -> Option<&'a crate::results::MemoryResult> {
    results
        .memory
        .measured
        .iter()
        .find(|m| m.size.eq_ignore_ascii_case(size))
}

fn class_label(class: EvidenceClass) -> &'static str {
    class.label()
}

/// The three aggregate UDP-fit rows, recomputed from the modelled rows when
/// the artifact predates the aggregate field.
fn udp_aggregate(results: &ClimbResults) -> Vec<(String, String, u64, u64, u64, u64)> {
    if let Some(section) = results.derived.udp_fit.as_ref() {
        if !section.aggregate.is_empty() {
            return section
                .aggregate
                .iter()
                .map(|row| {
                    (
                        row.config.clone(),
                        row.config_label.clone(),
                        row.fit_rows,
                        row.total_rows,
                        row.fit_messages,
                        row.total_messages,
                    )
                })
                .collect();
        }
        let mut out = Vec::new();
        for (config, label) in [
            ("plain", "A (plain SLH-DSA)"),
            ("mtl-full", "B (MTL non-amortised)"),
            ("mtl-amortised", "C (MTL amortised)"),
        ] {
            let matching: Vec<_> = section.rows.iter().filter(|r| r.config == config).collect();
            if matching.is_empty() {
                continue;
            }
            out.push((
                config.to_string(),
                label.to_string(),
                matching
                    .iter()
                    .filter(|r| r.fit_1232_percent == 100.0)
                    .count() as u64,
                matching.len() as u64,
                matching.iter().map(|r| r.fit_1232_count).sum(),
                matching.iter().map(|r| r.messages).sum(),
            ));
        }
        return out;
    }
    Vec::new()
}

/// Every headline metric, recomputed from raw rows.
/// Median `mem.cache.rrset` bytes over the raw measurement runs for one
/// configuration and RRset count.
///
/// The headline cache rows are recomputed from these per-run rows rather than
/// read from the consolidated median, so a tampered `measured_bytes` cell is
/// caught: the recomputed value and the stored pointer must agree.
fn median_cache_rrset_bytes(
    cache: &crate::cache::CacheResults,
    r: usize,
    config: &str,
) -> Option<u64> {
    let mut values: Vec<u64> = cache
        .measured
        .iter()
        .filter(|run| run.r == r && run.config == config)
        .map(|run| run.stats.rrset_cache_bytes)
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        let a = values.get(mid - 1).copied().unwrap_or(0);
        let b = values.get(mid).copied().unwrap_or(0);
        Some((a + b) / 2)
    } else {
        values.get(mid).copied()
    }
}

pub fn headline_metrics(results: &ClimbResults) -> Vec<Metric> {
    let mut metrics = Vec::new();

    // --- DNS: full vs condensed response bytes from the parsed anatomy. ----
    if let Some(anatomy) = results.dns_anatomy.as_ref() {
        let full = anatomy.full.as_ref().map(|r| r.wire_length).unwrap_or(0);
        let condensed = anatomy
            .condensed
            .as_ref()
            .map(|r| r.wire_length)
            .unwrap_or(0);
        if full > 0 {
            let reduction = 100.0 * (full as f64 - condensed as f64) / full as f64;
            metrics.push(Metric {
                id: "dns_reduction".to_string(),
                label: "Live DNS response, `www.climb.example. A` — reduction, condensed vs full"
                    .to_string(),
                unit: "%",
                class: "DERIVED",
                baseline: "B",
                value: reduction,
                denominator: None,
                display: fmt_percent(reduction),
                pointer: "/dns_pipeline/reduction_percent_www_a".to_string(),
                denominator_pointer: None,
                report_pointer: "/dns_pipeline/reduction_percent_www_a".to_string(),
                sheet_label: "DNS, www.climb.example. A — reduction, condensed vs full".to_string(),
            });
        }
    }

    // --- Wire bytes at the headline size, from the wire report rows. -------
    if let Some(wire) = wire_row(results, HEADLINE_SIZE) {
        let wire_index = results
            .wire_size
            .iter()
            .position(|w| w.size.eq_ignore_ascii_case(HEADLINE_SIZE))
            .unwrap_or(0);
        // The wire reduction lives on the comparison row, not the wire-size
        // row: those arrays are ordered independently, so select by size.
        let comparison_index = results
            .comparisons
            .iter()
            .position(|c| c.size.eq_ignore_ascii_case(HEADLINE_SIZE))
            .unwrap_or(0);
        if wire.plain_total_bytes > 0 {
            let reduction = 100.0
                * (wire.plain_total_bytes as f64 - wire.with_mtl_total_bytes as f64)
                / wire.plain_total_bytes as f64;
            metrics.push(Metric {
                id: "wire_reduction_medium_c_vs_a".to_string(),
                label: "Wire reduction, Medium — C vs A".to_string(),
                unit: "%",
                class: "DERIVED",
                baseline: "A",
                value: reduction,
                denominator: None,
                display: fmt_percent(reduction),
                pointer: format!("/comparisons/{comparison_index}/wire_reduction_c_vs_a_percent"),
                denominator_pointer: None,
                report_pointer: format!(
                    "/comparisons/{comparison_index}/wire_reduction_c_vs_a_percent"
                ),
                sheet_label: "Medium — wire reduction, C vs A".to_string(),
            });
        }
        for (id, label, sheet, baseline, value) in [
            (
                "wire_a_medium",
                "Wire bytes, Medium — A (plain SLH-DSA)",
                "Medium — wire total, A (plain SLH-DSA)",
                "A",
                wire.plain_total_bytes,
            ),
            (
                "wire_b_medium",
                "Wire bytes, Medium — B (MTL, no amortisation)",
                "Medium — wire total, B (MTL, no amortisation)",
                "B",
                wire.without_mtl_total_bytes,
            ),
            (
                "wire_c_medium",
                "Wire bytes, Medium — C (MTL amortised)",
                "Medium — wire total, C (MTL amortised)",
                "C",
                wire.with_mtl_total_bytes,
            ),
        ] {
            let pointer = match id {
                "wire_a_medium" => format!("/wire_size/{wire_index}/plain_total_bytes"),
                "wire_b_medium" => format!("/wire_size/{wire_index}/without_mtl_total_bytes"),
                _ => format!("/wire_size/{wire_index}/with_mtl_total_bytes"),
            };
            let claim_suffix = match id {
                "wire_a_medium" => "plain_total_bytes",
                "wire_b_medium" => "without_mtl_total_bytes",
                _ => "with_mtl_total_bytes",
            };
            metrics.push(Metric {
                id: id.to_string(),
                label: label.to_string(),
                unit: "B",
                class: "MEASURED",
                baseline,
                value: value as f64,
                denominator: None,
                display: fmt_count(value as f64),
                pointer,
                denominator_pointer: None,
                report_pointer: format!("/wire_size/{wire_index}/{claim_suffix}"),
                sheet_label: sheet.to_string(),
            });
        }
    }

    // --- Signing and verifying speed-ups at the headline size. -------------
    if let Some(plain) = plain_row(results, HEADLINE_SIZE) {
        let comparison_index = results
            .comparisons
            .iter()
            .position(|c| c.size.eq_ignore_ascii_case(HEADLINE_SIZE));
        let c_sign = signing_per_message(results, HEADLINE_SIZE, "With MTL");
        if let (Some(a), Some(c)) = (plain.signing_mean_ns, c_sign) {
            if c > 0.0 {
                let speedup = a / c;
                let pointer = comparison_index
                    .map(|i| format!("/comparisons/{i}/signing_speedup_c_vs_a"))
                    .unwrap_or_default();
                metrics.push(Metric {
                    id: "signing_speedup_medium_c_vs_a".to_string(),
                    label: "Signing speed-up, Medium — C vs A".to_string(),
                    unit: "\u{d7}",
                    class: "DERIVED",
                    baseline: "A",
                    value: speedup,
                    denominator: None,
                    display: fmt_ratio(speedup),
                    pointer: pointer.clone(),
                    denominator_pointer: None,
                    report_pointer: pointer,
                    sheet_label: "Medium — signing speed-up, C vs A".to_string(),
                });
            }
        }
        let c_verify = verifying_per_message(results, HEADLINE_SIZE, "MTL (trust cached)");
        if let (Some(a), Some(c)) = (plain.verifying_mean_ns, c_verify) {
            if c > 0.0 {
                let speedup = a / c;
                let pointer = comparison_index
                    .map(|i| format!("/comparisons/{i}/verifying_speedup_cached_vs_a"))
                    .unwrap_or_default();
                metrics.push(Metric {
                    id: "verifying_speedup_medium_cached_c_vs_a".to_string(),
                    label: "Verifying speed-up, Medium, trust cached ladder — C vs A".to_string(),
                    unit: "\u{d7}",
                    class: "DERIVED",
                    baseline: "A",
                    value: speedup,
                    denominator: None,
                    display: fmt_ratio(speedup),
                    pointer: pointer.clone(),
                    denominator_pointer: None,
                    report_pointer: pointer,
                    sheet_label: "Medium — verifying speed-up (trust cached), C vs A".to_string(),
                });
            }
        }
    }

    // --- UDP fit: one honest `k of n` row per post-quantum configuration. --
    let aggregate = udp_aggregate(results);
    if !aggregate.is_empty() {
        for (config, _, fit, total, _, _) in &aggregate {
            let (baseline, suffix) = match config.as_str() {
                "plain" => ("A", "A (plain SLH-DSA)"),
                "mtl-full" => ("B", "B (MTL, no amortisation)"),
                _ => ("C", "C (MTL amortised)"),
            };
            let index = results
                .derived
                .udp_fit
                .as_ref()
                .and_then(|s| s.aggregate.iter().position(|r| &r.config == config));
            let pointer = index
                .map(|i| format!("/derived/udp_fit/aggregate/{i}/fit_rows"))
                .unwrap_or_default();
            let denominator_pointer =
                index.map(|i| format!("/derived/udp_fit/aggregate/{i}/total_rows"));
            let total_rows = results
                .derived
                .udp_fit
                .as_ref()
                .map(|s| s.aggregate.first().map(|r| r.total_rows).unwrap_or(*total))
                .unwrap_or(*total);
            metrics.push(Metric {
                id: match config.as_str() {
                    "plain" => "udp_fit_a",
                    "mtl-full" => "udp_fit_b",
                    _ => "udp_fit_c",
                }
                .to_string(),
                label: format!(
                    "UDP fit, {total_rows} analytical rows — {suffix} modelled responses fitting the 1,232-byte UDP payload"
                ),
                unit: "rows",
                class: "ANALYTICAL",
                baseline,
                value: *fit as f64,
                denominator: Some(*total),
                display: format!("{fit} of {total}"),
                pointer,
                denominator_pointer,
                report_pointer: match config.as_str() {
                    "plain" => "/derived/udp_fit/aggregate/0/fit_rows".to_string(),
                    "mtl-full" => "/derived/udp_fit/aggregate/1/fit_rows".to_string(),
                    _ => "/derived/udp_fit/aggregate/2/fit_rows".to_string(),
                },
                sheet_label: format!(
                    "UDP fit, aggregate — {suffix}, modelled responses fitting the 1,232-byte UDP payload"
                ),
            });
        }
    }

    // --- Memory at the largest measured size. -----------------------------
    if let Some(memory) = memory_row(results, "Large") {
        let memory_index = results
            .memory
            .measured
            .iter()
            .position(|m| m.size.eq_ignore_ascii_case("Large"))
            .unwrap_or(0);
        metrics.push(Metric {
            id: "large_memory_median".to_string(),
            label: "Large memory (measured resident median)".to_string(),
            unit: "B",
            class: "MEASURED",
            baseline: "n/a",
            value: memory.median_bytes as f64,
            denominator: None,
            display: fmt_count(memory.median_bytes as f64),
            pointer: format!("/memory/measured/{memory_index}/median_bytes"),
            denominator_pointer: None,
            report_pointer: format!("/memory/measured/{memory_index}/median_bytes"),
            sheet_label: "Large — resident median (measured)".to_string(),
        });
        if memory.analytical_total_bytes > 0 {
            let ratio = memory.median_bytes as f64 / memory.analytical_total_bytes as f64;
            metrics.push(Metric {
                id: "large_memory_ratio".to_string(),
                label: "Large memory (measured / analytical ratio)".to_string(),
                unit: "\u{d7}",
                class: "DERIVED",
                baseline: "n/a",
                value: ratio,
                denominator: None,
                display: fmt_ratio(ratio),
                pointer: format!("/memory/measured/{memory_index}/measured_analytical_total_ratio"),
                denominator_pointer: None,
                report_pointer: format!(
                    "/memory/measured/{memory_index}/measured_analytical_total_ratio"
                ),
                sheet_label: "Large — measured / analytical ratio".to_string(),
            });
        }
    }

    // --- Ladder shape at the published leaf count. -------------------------
    if let Some(ladder) = results.ladder.as_ref() {
        if let Some(row) = ladder
            .sizes
            .iter()
            .find(|row| row.leaf_count == LADDER_LEAF_COUNT)
        {
            let index = ladder
                .sizes
                .iter()
                .position(|row| row.leaf_count == LADDER_LEAF_COUNT)
                .unwrap_or(0);
            metrics.push(Metric {
                id: "ladder_buffer_n100".to_string(),
                label: "Ladder buffer at N=100 (Small dataset), 3 rungs".to_string(),
                unit: "B",
                class: class_label(EvidenceClass::Measured),
                baseline: "n/a",
                value: row.ladder_buffer_len as f64,
                denominator: None,
                display: fmt_count(row.ladder_buffer_len as f64),
                pointer: format!("/ladder/sizes/{index}/ladder_buffer_len"),
                denominator_pointer: None,
                report_pointer: format!("/ladder/sizes/{index}/ladder_buffer_len"),
                sheet_label: "Ladder, N=100 — ladder buffer bytes".to_string(),
            });
        }
    }

    // --- Resolver cache use at the headline size. ----------------------
    // The pointer indexes into the `cache_use.analytical` array, located by
    // (`message_count`, configuration) rather than by position.
    if let Some(cache) = results.cache_use.as_ref() {
        let medium_messages = results
            .wire_size
            .iter()
            .find(|w| w.size.eq_ignore_ascii_case(HEADLINE_SIZE))
            .map(|w| w.message_count)
            .unwrap_or(0);
        if medium_messages > 0 {
            for (config, _baseline, id, _label, config_label) in [
                (
                    "plain",
                    "A",
                    "cache_rrset_bytes_medium_plain",
                    "Resolver cache, Medium rrset bytes — plain",
                    "plain",
                ),
                (
                    "mtl-condensed",
                    "C",
                    "cache_rrset_bytes_medium_mtl",
                    "Resolver cache, Medium rrset bytes — MTL-condensed",
                    "MTL-condensed",
                ),
            ] {
                if let Some(index) = cache
                    .analytical
                    .iter()
                    .position(|r| r.r == medium_messages && r.config == config)
                {
                    if cache.analytical.get(index).is_some() {
                        if let Some(measured_bytes) =
                            median_cache_rrset_bytes(cache, medium_messages, config)
                        {
                            metrics.push(Metric {
                                id: id.to_string(),
                                label: format!(
                                    "Medium — resolver cache bytes, {config_label} (measured)"
                                ),
                                unit: "B",
                                class: "MEASURED",
                                baseline: "n/a",
                                value: measured_bytes as f64,
                                denominator: None,
                                display: fmt_count(measured_bytes as f64),
                                pointer: format!("/cache_use/analytical/{index}/measured_bytes"),
                                denominator_pointer: None,
                                report_pointer: format!(
                                    "/cache_use/analytical/{index}/measured_bytes"
                                ),
                                sheet_label: format!(
                                    "Medium — resolver cache bytes, {config_label} (measured)"
                                ),
                            });
                        }
                    }
                }
            }
            if let Some(index) = cache
                .analytical
                .iter()
                .position(|r| r.r == medium_messages && r.config == "mtl-condensed")
            {
                if let Some(row) = cache.analytical.get(index) {
                    if row.analytical_bytes > 0 {
                        if let Some(measured_bytes) =
                            median_cache_rrset_bytes(cache, medium_messages, "mtl-condensed")
                        {
                            let ratio = measured_bytes as f64 / row.analytical_bytes as f64;
                            metrics.push(Metric {
                                id: "cache_ratio_medium_mtl".to_string(),
                                label:
                                    "Medium — resolver cache measured / analytical, MTL-condensed"
                                        .to_string(),
                                unit: "\u{d7}",
                                class: "DERIVED",
                                value: ratio,
                                denominator: None,
                                display: fmt_ratio(ratio),
                                pointer: format!(
                                    "/cache_use/analytical/{index}/measured_over_analytical"
                                ),
                                denominator_pointer: None,
                                report_pointer: format!(
                                    "/cache_use/analytical/{index}/measured_over_analytical"
                                ),
                                sheet_label:
                                    "Medium — resolver cache measured / analytical, MTL-condensed"
                                        .to_string(),
                                baseline: "n/a",
                            });
                        }
                    }
                }
            }
        }
    }

    metrics
}

/// Classical DNSSEC baselines measured by the benchmark, for the README's
/// context block. These are reference points, not the benchmark's subject.
pub fn classical_metrics(results: &ClimbResults) -> Vec<Metric> {
    let mut metrics = Vec::new();
    for (index, row) in results.classical.iter().enumerate() {
        if !row.size.eq_ignore_ascii_case(HEADLINE_SIZE) {
            continue;
        }
        let Some(verify) = row.verifying_mean_ns else {
            continue;
        };
        let bytes = row.signature_len_bytes as f64;
        let slug = row
            .algorithm
            .to_ascii_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let context_index = classical_context_index(results, &row.algorithm).unwrap_or(0);
        metrics.push(Metric {
            id: format!("classical_{slug}_bytes"),
            label: format!("Medium — {} signature bytes", row.algorithm),
            unit: "B",
            class: class_label(row.class),
            baseline: "n/a",
            value: bytes,
            denominator: None,
            display: fmt_count(bytes),
            pointer: format!("/classical/{index}/signature_len_bytes"),
            denominator_pointer: None,
            report_pointer: format!("/derived/signature_context/{context_index}/signature_bytes"),
            sheet_label: format!("Medium — {} signature bytes", row.algorithm),
        });
        metrics.push(Metric {
            id: format!("classical_{slug}_verify"),
            label: format!("Medium — {} verify per message", row.algorithm),
            unit: "\u{b5}s",
            class: class_label(row.class),
            baseline: "n/a",
            value: verify,
            denominator: None,
            display: fmt_micros(verify),
            pointer: format!("/classical/{index}/verifying_mean_ns"),
            denominator_pointer: None,
            report_pointer: format!("/derived/signature_context/{context_index}/verifying_mean_ns"),
            sheet_label: format!("Medium — {} verify per message", row.algorithm),
        });
    }
    metrics
}

fn classical_context_index(results: &ClimbResults, algorithm: &str) -> Option<usize> {
    // Context labels carry the algorithm inside a decoration, e.g.
    // `E (Ed25519, context)`, so match on the algorithm substring rather than
    // requiring the whole label to equal it.
    let needle = algorithm.to_ascii_lowercase();
    results.derived.signature_context.iter().position(|row| {
        row.size.eq_ignore_ascii_case(HEADLINE_SIZE)
            && row.label.to_ascii_lowercase().contains(&needle)
    })
}

/// Median and range of one stability ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub runs: usize,
}

/// Look up a stability ratio by name.
pub fn spread(results: &ClimbResults, ratio_name: &str) -> Option<Spread> {
    results
        .derived
        .stability
        .as_ref()
        .and_then(|section| section.runs.iter().find(|run| run.ratio_name == ratio_name))
        .map(|run| Spread {
            median: run.median,
            min: run.min,
            max: run.max,
            runs: run.n_runs,
        })
}

/// Spread cell for the headline table: `—` when no stability data exists,
/// `pending` when this row expects one, and the model median plus range when
/// it does.
pub fn spread_cell(results: &ClimbResults, metric_id: &str) -> String {
    let ratio = match metric_id {
        "signing_speedup_medium_c_vs_a" => "signing_speedup_c_vs_a_model_medium",
        "verifying_speedup_medium_cached_c_vs_a" => "verifying_speedup_cached_c_vs_a_model_medium",
        _ => return "\u{2014}".to_string(),
    };
    match spread(results, ratio) {
        Some(spread) => format!(
            "model median {:.1} \u{d7} ({:.1}\u{2013}{:.1}, n={})",
            spread.median, spread.min, spread.max, spread.runs
        ),
        None => "pending".to_string(),
    }
}

/// Render the README headline table (without the surrounding markers).
pub fn render_headline_block(
    results: &ClimbResults,
    metrics: &[Metric],
) -> Result<String, ReportError> {
    if metrics.is_empty() {
        return Err(ReportError::MalformedData {
            path: PathBuf::from("<results>"),
            reason: "no headline metrics could be recomputed from the raw rows".to_string(),
        });
    }
    let mut out = String::new();
    out.push_str("| Metric | Value | Evidence class | Baseline | Spread |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for metric in metrics {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            metric.label,
            metric.value_cell(),
            metric.class,
            metric.baseline,
            spread_cell(results, &metric.id)
        ));
    }
    out.push('\n');
    out.push_str(SCOPE_SENTENCE);
    out.push('\n');
    Ok(out)
}

/// Scope note under the headline table. No unapproved digits: `1,232` and
/// `n=5` have no result unit and are not scanned as results.
pub const SCOPE_SENTENCE: &str = "**Scope.** Live DNS bytes were measured over TCP in a local capture; the UDP-fit rows are an analytical payload model, not a measurement; signing and verifying speed-ups are derived from per-message measurements. No row in this table is extrapolated by the `N × per-message` identity; the OneK and Large comparisons in the artifact are. The spread column is the median (min–max) of the per-run primitive model ratio over n=5 runs, not a re-run of the headline benchmark.";

/// Render the README classical-baseline table (without markers).
pub fn render_classical_block(metrics: &[Metric]) -> String {
    let mut out = String::new();
    out.push_str(
        "| Algorithm | Signature bytes | Verify per message | Evidence class | Baseline |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for metric in metrics {
        // Two metrics per algorithm: bytes then verify.
        if !metric.id.ends_with("_bytes") {
            continue;
        }
        let algorithm = metric
            .sheet_label
            .strip_prefix("Medium — ")
            .and_then(|rest| rest.strip_suffix(" signature bytes"))
            .unwrap_or(&metric.sheet_label);
        let verify = metrics
            .iter()
            .find(|other| {
                other.id.ends_with("_verify")
                    && other
                        .sheet_label
                        .starts_with(&format!("Medium — {algorithm} verify"))
            })
            .map(|other| format!("{} {}", other.display, other.unit))
            .unwrap_or_else(|| "\u{2014}".to_string());
        out.push_str(&format!(
            "| {} (context) | {} {} | {} | {} | {} |\n",
            algorithm, metric.display, metric.unit, verify, metric.class, metric.baseline
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed sample artifact is the real fixture: every surface is
    /// built from this exact document, so the recomputation is tested against
    /// recorded rows rather than a hand-written subset.
    fn sample_results() -> ClimbResults {
        serde_json::from_str(include_str!(
            "../../../docs/sample-output/climb-results.json"
        ))
        .expect("committed sample artifact parses")
    }

    fn metric<'a>(metrics: &'a [Metric], id: &str) -> &'a Metric {
        metrics
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("metric {id}"))
    }

    #[test]
    fn headline_metrics_recompute_from_the_real_fixture() {
        let results = sample_results();
        let metrics = headline_metrics(&results);
        let dns = metric(&metrics, "dns_reduction");
        assert!((dns.value - 97.30747197).abs() < 1e-6, "{}", dns.value);
        let wire = metric(&metrics, "wire_reduction_medium_c_vs_a");
        assert!((wire.value - 96.5079531568).abs() < 1e-6, "{}", wire.value);
        assert_eq!(metric(&metrics, "wire_a_medium").display, "78,560,000");
        let signing = metric(&metrics, "signing_speedup_medium_c_vs_a");
        assert!((signing.value - 9405.20631576).abs() < 1e-5);
        let verifying = metric(&metrics, "verifying_speedup_medium_cached_c_vs_a");
        assert!((verifying.value - 215.311121087).abs() < 1e-5);
        assert_eq!(
            metric(&metrics, "large_memory_median").display,
            "1,087,139,840"
        );
        assert!((metric(&metrics, "large_memory_ratio").value - 1.1044228).abs() < 1e-5);
        assert_eq!(metric(&metrics, "ladder_buffer_n100").value, 132.0);
    }

    #[test]
    fn cache_metrics_recompute_the_median_from_raw_runs() {
        let mut results = sample_results();
        let medium = results
            .wire_size
            .iter()
            .find(|w| w.size.eq_ignore_ascii_case("Medium"))
            .expect("committed sample carries a Medium wire row")
            .message_count;
        let cache = results
            .cache_use
            .as_mut()
            .expect("committed sample carries cache use");
        let index = cache
            .analytical
            .iter()
            .position(|r| r.r == medium && r.config == "mtl-condensed")
            .expect("committed sample carries a Medium MTL-condensed row");
        // Tamper with the stored median and ratio: the metric must be
        // recomputed from the raw per-run rows, not read from these cells.
        if let Some(row) = cache.analytical.get_mut(index) {
            row.measured_bytes = 1;
            row.measured_over_analytical = 1.0;
        }
        let analytical_bytes = cache
            .analytical
            .get(index)
            .map(|row| row.analytical_bytes)
            .unwrap_or(0);
        assert!(analytical_bytes > 0);

        let metrics = headline_metrics(&results);
        let bytes = metric(&metrics, "cache_rrset_bytes_medium_mtl");
        assert_eq!(
            bytes.value, 8_429_932.0,
            "the median of the raw runs wins over the tampered cell"
        );
        let ratio = metric(&metrics, "cache_ratio_medium_mtl");
        assert!((ratio.value - 8_429_932.0 / analytical_bytes as f64).abs() < 1e-9);
        assert_ne!(ratio.value, 1.0);
    }

    #[test]
    fn cache_metrics_skip_a_configuration_without_raw_runs() {
        let mut results = sample_results();
        let medium = results
            .wire_size
            .iter()
            .find(|w| w.size.eq_ignore_ascii_case("Medium"))
            .expect("committed sample carries a Medium wire row")
            .message_count;
        let cache = results
            .cache_use
            .as_mut()
            .expect("committed sample carries cache use");
        cache
            .measured
            .retain(|run| !(run.r == medium && run.config == "plain"));

        let metrics = headline_metrics(&results);
        assert!(
            !metrics
                .iter()
                .any(|m| m.id == "cache_rrset_bytes_medium_plain"),
            "a configuration with no raw runs must not publish a median"
        );
    }

    #[test]
    fn udp_aggregate_reports_k_of_n_from_the_real_fixture() {
        let results = sample_results();
        let metrics = headline_metrics(&results);
        let a = metric(&metrics, "udp_fit_a");
        assert_eq!(a.display, "0 of 12");
        assert_eq!(a.denominator, Some(12));
        let b = metric(&metrics, "udp_fit_b");
        assert_eq!(b.display, "0 of 12");
        let c = metric(&metrics, "udp_fit_c");
        assert_eq!(c.display, "11 of 12");
        assert!(a.value_cell().contains(" of "));
    }

    #[test]
    fn spread_cell_names_the_model_and_range_or_pending() {
        let results = sample_results();
        let metric_id = "verifying_speedup_medium_cached_c_vs_a";
        let cell = spread_cell(&results, metric_id);
        assert!(
            cell == "pending" || cell.starts_with("model median"),
            "{cell}"
        );
        assert_eq!(spread_cell(&results, "dns_reduction"), "\u{2014}");
    }

    #[test]
    fn headline_block_carries_a_scope_sentence_and_three_udp_rows() {
        let results = sample_results();
        let metrics = headline_metrics(&results);
        let block = render_headline_block(&results, &metrics).unwrap();
        assert!(block.contains("| 0 of 12 | ANALYTICAL | A |"), "{block}");
        assert!(block.contains("| 11 of 12 | ANALYTICAL | C |"), "{block}");
        assert!(block.contains("analytical payload model"), "{block}");
        assert_eq!(metrics.iter().filter(|m| m.is_count_of()).count(), 3);
    }

    #[test]
    fn classical_block_lists_every_measured_algorithm() {
        let results = sample_results();
        let metrics = classical_metrics(&results);
        let block = render_classical_block(&metrics);
        assert!(block.contains("Ed25519 (context)"), "{block}");
        assert!(block.contains("ECDSA P-256 (context)"), "{block}");
        assert!(block.contains("RSA-2048 (context)"), "{block}");
        assert!(block.contains("MEASURED"), "{block}");
    }

    #[test]
    fn ratio_format_groups_thousands_and_keeps_honest_precision() {
        assert_eq!(fmt_ratio(9_433.089_280_263_535), "9,433");
        assert_eq!(fmt_ratio(119_656.685_718_215_54), "119,657");
        assert_eq!(fmt_ratio(215.311_121_086_966_15), "215.3");
        assert_eq!(fmt_ratio(83.282_744_164_802_05), "83.28");
        assert_eq!(fmt_ratio(0.925_218_329_794_511), "0.9252");
        assert_eq!(fmt_ratio(1.0), "1");
        assert_eq!(fmt_ratio(0.0), "0");
    }

    #[test]
    fn kpi_caption_states_the_canonical_size_and_baseline() {
        assert_eq!(kpi_caption("medium", "C vs A"), "Medium C vs A");
        assert_eq!(kpi_caption("1000", "C vs B"), "OneK C vs B");
        assert_eq!(kpi_caption("Large", "C vs B"), "Large C vs B");
    }

    #[test]
    fn ratio_format_parses_back_within_half_a_percent_for_a_thousand_ratios() {
        // Deterministic LCG so the property test is reproducible. The range
        // spans every ratio the recorded artifacts publish, plus the boundary
        // values that used to render as a bare `0x` or `1x`.
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut ratios = vec![
            0.0,
            0.01,
            0.05,
            0.95,
            1.0,
            1.05,
            9.999,
            99.99,
            999.9,
            1_000.0,
            119_656.685_718_215_54,
        ];
        for _ in 0..1000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
            let exponent = -3.0 + unit * 9.0;
            ratios.push(10f64.powf(exponent));
        }
        for ratio in ratios {
            let rendered = fmt_ratio(ratio);
            let parsed: f64 = rendered
                .replace(',', "")
                .parse()
                .unwrap_or_else(|error| panic!("{ratio} rendered {rendered}: {error}"));
            let error = if ratio == 0.0 {
                parsed.abs()
            } else {
                (parsed - ratio).abs() / ratio.abs()
            };
            assert!(
                error <= 0.005,
                "{ratio} rendered {rendered}, parsed {parsed} (relative error {error})"
            );
            if rendered == "0" {
                assert!(
                    ratio.abs() <= 0.05,
                    "{ratio} rendered a false zero as {rendered}"
                );
            }
            if rendered == "1" {
                assert!(
                    (ratio - 1.0).abs() <= 0.05,
                    "{ratio} rendered a false one as {rendered}"
                );
            }
        }
    }
}
