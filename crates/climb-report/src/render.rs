//! Plain-text table rendering and duration formatting.
//!
//! Builds the signed and verifying summary tables from [`SummaryRow`]s
//! and provides [`format_duration`] for converting nanosecond timings
//! to human-readable form.

use std::collections::BTreeMap;

use crate::summary::{BenchKind, MtlMode, Size, SummaryRow};

// ---------------------------------------------------------------------------
// Duration formatting
// ---------------------------------------------------------------------------

/// Format a nanosecond duration into a human-readable string.
///
/// Thresholds and precision:
///
/// | Range                | Unit | Decimals | Example          |
/// |----------------------|------|----------|------------------|
/// |    0 ..< 1_000       | ns   | 1        | `682.9 ns`       |
/// | 1_000 ..< 1_000_000  | us   | 1        | `599.0 us`       |
/// | 1_000_000 ..< 1e9    | ms   | 1        | `682.9 ms`       |
/// | 1e9 ..< 6e10         | s    | 2        | `13.50 s`        |
/// | 6e10 ..< 3.6e12      | min  | 2        | `1.81 min`       |
/// | >= 3.6e12            | h    | 2        | `7.55 h`         |
pub fn format_duration(ns: f64) -> String {
    if ns < 1_000.0 {
        format!("{ns:.1} ns")
    } else if ns < 1_000_000.0 {
        format!("{:.1} us", ns / 1_000.0)
    } else if ns < 1_000_000_000.0 {
        format!("{:.1} ms", ns / 1_000_000.0)
    } else if ns < 60_000_000_000.0 {
        format!("{:.2} s", ns / 1_000_000_000.0)
    } else if ns < 3_600_000_000_000.0 {
        format!("{:.2} min", ns / 60_000_000_000.0)
    } else {
        format!("{:.2} h", ns / 3_600_000_000_000.0)
    }
}

// ---------------------------------------------------------------------------
// Table rendering
// ---------------------------------------------------------------------------

/// Produce a plain-text summary table from categorized rows.
///
/// Returns a [`String`] containing a signing table and a verifying table.
/// The caller is responsible for printing it; this function performs no I/O.
pub fn render_text_summary(rows: &[SummaryRow]) -> String {
    let mut out = String::new();

    let signing: Vec<&SummaryRow> = rows
        .iter()
        .filter(|r| r.kind == BenchKind::Signing)
        .collect();
    let verifying: Vec<&SummaryRow> = rows
        .iter()
        .filter(|r| r.kind == BenchKind::Verifying)
        .collect();

    if !signing.is_empty() {
        out.push_str(&render_table(BenchKind::Signing, &signing));
    }
    if !verifying.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&render_table(BenchKind::Verifying, &verifying));
    }

    out
}

fn render_table(kind: BenchKind, rows: &[&SummaryRow]) -> String {
    let title = match kind {
        BenchKind::Signing => "Signing",
        BenchKind::Verifying => "Verifying",
    };

    let without_means: BTreeMap<Size, f64> = rows
        .iter()
        .filter(|r| r.mode == MtlMode::WithoutMtl)
        .map(|r| (r.size, r.mean_ns))
        .collect();

    let mut table: Vec<[String; 6]> = Vec::with_capacity(rows.len() + 1);
    table.push([
        "Size".to_string(),
        "Condition".to_string(),
        "Messages".to_string(),
        "Mean".to_string(),
        "95% CI".to_string(),
        "Speedup".to_string(),
    ]);

    for row in rows {
        let messages = row
            .elements
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        let mean = format_duration(row.mean_ns);
        let ci = format!(
            "[{}, {}]",
            format_duration(row.ci_lower_ns),
            format_duration(row.ci_upper_ns)
        );
        let speedup = if row.mode != MtlMode::WithoutMtl {
            without_means
                .get(&row.size)
                .map(|wmean| format_speedup(wmean / row.mean_ns))
                .unwrap_or_else(|| "-".to_string())
        } else {
            "-".to_string()
        };

        table.push([
            row.size.to_string(),
            row.mode.to_string(),
            messages,
            mean,
            ci,
            speedup,
        ]);
    }

    render_grid(&format!("=== {title} ==="), &table)
}

/// Column widths are computed from the widest cell so the grid always aligns.
/// `Messages`, `Mean`, and `Speedup` are right-aligned; the rest left-aligned.
fn render_grid(title: &str, table: &[[String; 6]]) -> String {
    const RIGHT_ALIGNED: [bool; 6] = [false, false, true, true, true, true];

    let widths: Vec<usize> = (0..6)
        .map(|col| {
            table
                .iter()
                .map(|row| row[col].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    out.push_str(title);
    out.push('\n');

    for (index, row) in table.iter().enumerate() {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(col, cell)| {
                let padding = " ".repeat(widths[col].saturating_sub(cell.chars().count()));
                if RIGHT_ALIGNED[col] {
                    format!("{padding}{cell}")
                } else {
                    format!("{cell}{padding}")
                }
            })
            .collect();
        out.push_str(&cells.join(" | "));
        out.push('\n');

        if index == 0 {
            let separator: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            out.push_str(&separator.join("-+-"));
            out.push('\n');
        }
    }

    out
}

/// Format a speedup ratio with a magnitude-appropriate precision.
fn format_speedup(ratio: f64) -> String {
    if ratio >= 1000.0 {
        format!("{ratio:.0}x")
    } else if ratio >= 100.0 {
        format!("{ratio:.1}x")
    } else {
        format!("{ratio:.2}x")
    }
}
