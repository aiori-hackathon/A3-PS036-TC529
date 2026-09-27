//! Public-API tests for [`climb_report::format_duration`] and
//! [`climb_report::render_text_summary`].

use climb_report::{format_duration, render_text_summary, BenchKind, MtlMode, Size, SummaryRow};

fn make_row(
    kind: BenchKind,
    size: Size,
    mode: MtlMode,
    mean_ms: f64,
    ci_low_ms: f64,
    ci_high_ms: f64,
    elements: Option<u64>,
) -> SummaryRow {
    SummaryRow {
        kind,
        size,
        mode,
        mean_ns: mean_ms * 1_000_000.0,
        ci_lower_ns: ci_low_ms * 1_000_000.0,
        ci_upper_ns: ci_high_ms * 1_000_000.0,
        elements,
    }
}

#[test]
fn format_duration_nanoseconds() {
    assert_eq!(format_duration(0.0), "0.0 ns");
    assert_eq!(format_duration(500.3), "500.3 ns");
    assert_eq!(format_duration(999.9), "999.9 ns");
}

#[test]
fn format_duration_microseconds() {
    assert_eq!(format_duration(1_000.0), "1.0 us");
    assert_eq!(format_duration(599_000.0), "599.0 us");
    assert_eq!(format_duration(999_000.0), "999.0 us");
}

#[test]
fn format_duration_milliseconds() {
    assert_eq!(format_duration(1_000_000.0), "1.0 ms");
    assert_eq!(format_duration(682_919_952.51), "682.9 ms");
    assert_eq!(format_duration(500_000_000.0), "500.0 ms");
}

#[test]
fn format_duration_seconds() {
    assert_eq!(format_duration(1_000_000_000.0), "1.00 s");
    assert_eq!(format_duration(13_500_000_000.0), "13.50 s");
    assert_eq!(format_duration(59_999_999_999.0), "60.00 s");
}

#[test]
fn format_duration_minutes() {
    assert_eq!(format_duration(60_000_000_000.0), "1.00 min");
    assert_eq!(format_duration(65_238_500_000.0), "1.09 min");
    assert_eq!(format_duration(1_800_000_000_000.0), "30.00 min");
}

#[test]
fn format_duration_hours() {
    assert_eq!(format_duration(3_600_000_000_000.0), "1.00 h");
    assert_eq!(format_duration(65_238_000_000_000.0), "18.12 h");
    assert_eq!(format_duration(652_385_000_000_000.0), "181.22 h");
}

#[test]
fn signing_table_contains_speedup() {
    let rows = vec![
        make_row(
            BenchKind::Signing,
            Size::Small,
            MtlMode::WithMtlFullVerify,
            682.9,
            667.2,
            701.6,
            Some(100),
        ),
        make_row(
            BenchKind::Signing,
            Size::Small,
            MtlMode::WithoutMtl,
            65238.5,
            62888.1,
            67516.3,
            Some(100),
        ),
    ];

    let text = render_text_summary(&rows);

    assert!(text.contains("=== Signing ==="));
    assert!(text.contains("Small"));
    assert!(text.contains("With MTL"));
    assert!(text.contains("Without MTL"));
    assert!(text.contains("682.9 ms"));
    assert!(text.contains("1.09 min"));
    assert!(text.contains("95.53x"));
}

#[test]
fn verifying_table_all_three_modes() {
    let rows = vec![
        make_row(
            BenchKind::Verifying,
            Size::Small,
            MtlMode::WithMtlTrustCached,
            0.599,
            0.559,
            0.653,
            Some(100),
        ),
        make_row(
            BenchKind::Verifying,
            Size::Small,
            MtlMode::WithMtlFullVerify,
            70.4,
            67.3,
            73.7,
            Some(100),
        ),
        make_row(
            BenchKind::Verifying,
            Size::Small,
            MtlMode::WithoutMtl,
            62.6,
            61.5,
            63.7,
            Some(100),
        ),
    ];

    let text = render_text_summary(&rows);

    assert!(text.contains("=== Verifying ==="));
    assert!(text.contains("MTL (trust cached)"));
    assert!(text.contains("With MTL"));
    assert!(text.contains("Without MTL"));
    assert!(text.contains("599.0 us"));
    assert!(text.contains("70.4 ms"));
    assert!(text.contains("62.6 ms"));
}

#[test]
fn no_without_mtl_means_no_speedup() {
    let rows = vec![make_row(
        BenchKind::Signing,
        Size::Small,
        MtlMode::WithMtlFullVerify,
        682.9,
        667.2,
        701.6,
        Some(100),
    )];

    let text = render_text_summary(&rows);
    let data_row = text
        .lines()
        .find(|l| l.starts_with("Small"))
        .expect("data row");
    let last_cell = data_row.rsplit('|').next().expect("speedup cell");
    assert_eq!(
        last_cell.trim(),
        "-",
        "expected a placeholder speedup cell, got: {data_row}"
    );
}

#[test]
fn columns_aligned_across_header_and_rows() {
    let rows = vec![
        make_row(
            BenchKind::Signing,
            Size::Small,
            MtlMode::WithMtlFullVerify,
            682.9,
            667.2,
            701.6,
            Some(100),
        ),
        make_row(
            BenchKind::Signing,
            Size::Medium,
            MtlMode::WithoutMtl,
            65238.5,
            62888.1,
            67516.3,
            Some(10_000),
        ),
    ];

    let text = render_text_summary(&rows);
    let pipe_positions: Vec<Vec<usize>> = text
        .lines()
        .filter(|l| !l.starts_with("===") && l.contains('|'))
        .map(|line| {
            line.char_indices()
                .filter(|(_, c)| *c == '|')
                .map(|(i, _)| i)
                .collect()
        })
        .collect();

    let expected = pipe_positions[0].clone();
    for (line_no, positions) in pipe_positions.iter().enumerate() {
        assert_eq!(
            positions, &expected,
            "pipe columns misaligned on line {line_no}:\n{text}"
        );
    }
}
