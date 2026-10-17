//! Reuse-model view: the cold/warm break-even reuse count per dataset size.
//!
//! Mirrors the report's `derived.reuse` section. Every row is read from the
//! parsed artifact — no break-even is recomputed here — so the Verifying tab
//! can never disagree with the HTML report or the chart.

use super::{title_size, ResultsView};

/// One row of the Verifying tab's reuse panel.
#[derive(Debug, Clone, PartialEq)]
pub struct ReuseRow {
    /// Size label as displayed elsewhere in the tabs (title-cased).
    pub size: String,
    /// Smallest reuse count reaching time parity, or `None` when none did.
    pub time_r: Option<u64>,
    /// Smallest reuse count reaching bytes parity, or `None` when none did.
    pub bytes_r: Option<u64>,
    /// Evidence class label ("DERIVED" / "EXTRAPOLATED").
    pub class: String,
}

impl ResultsView {
    /// One row per size carrying a reuse curve, in artifact order.
    ///
    /// Entries whose curve is empty are skipped, matching the chart. The
    /// break-even fields fall back to the duplicate `r_star_*` fields so a
    /// model that carries only one of the pair still renders.
    pub fn reuse_rows(&self) -> Vec<ReuseRow> {
        self.results
            .derived
            .reuse
            .iter()
            .filter(|row| !row.curve.is_empty())
            .map(|row| ReuseRow {
                size: title_size(&row.size),
                time_r: row.breakeven_reuse_time.or(row.r_star_time),
                bytes_r: row.breakeven_reuse_bytes.or(row.r_star_bytes),
                class: row.class.label().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::test_support::{empty_derived, view_with};
    use climb_report::results::{EvidenceClass, ReuseCurvePoint, ReuseResult};

    fn reuse_row(size: &str, time_r: Option<u64>, bytes_r: Option<u64>) -> ReuseResult {
        ReuseResult {
            size: size.to_string(),
            batch_size: 100,
            curve: vec![
                ReuseCurvePoint {
                    reuse: 1,
                    effective_time_ns: 900.0,
                    effective_bytes: 8000.0,
                },
                ReuseCurvePoint {
                    reuse: 2,
                    effective_time_ns: 500.0,
                    effective_bytes: 4000.0,
                },
            ],
            breakeven_reuse_time: time_r,
            breakeven_reuse_bytes: bytes_r,
            r_star_time: time_r,
            r_star_bytes: bytes_r,
            baseline_time_ns: 1000.0,
            baseline_bytes: 7856.0,
            class: EvidenceClass::Derived,
        }
    }

    fn view_with_reuse(rows: Vec<ReuseResult>) -> ResultsView {
        let mut derived = empty_derived();
        derived.reuse = rows;
        view_with(Vec::new(), Vec::new(), Vec::new(), None, derived)
    }

    #[test]
    fn reuse_rows_title_case_the_wire_label_and_carry_break_evens() {
        let view = view_with_reuse(vec![reuse_row("small", Some(1), Some(2))]);
        let rows = view.reuse_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].size, "Small", "wire label is title-cased");
        assert_eq!(rows[0].time_r, Some(1));
        assert_eq!(rows[0].bytes_r, Some(2));
        assert_eq!(rows[0].class, "DERIVED");
    }

    #[test]
    fn reuse_rows_report_no_break_even_as_none() {
        let view = view_with_reuse(vec![reuse_row("medium", None, None)]);
        let rows = view.reuse_rows();
        assert_eq!(rows[0].time_r, None);
        assert_eq!(rows[0].bytes_r, None);
    }

    #[test]
    fn reuse_rows_skip_entries_without_a_curve() {
        let mut empty = reuse_row("medium", Some(5), Some(5));
        empty.curve.clear();
        let view = view_with_reuse(vec![reuse_row("small", Some(1), Some(1)), empty]);
        let rows = view.reuse_rows();
        assert_eq!(rows.len(), 1, "the curve-less entry is skipped");
        assert_eq!(rows[0].size, "Small");
    }

    #[test]
    fn reuse_rows_fall_back_to_r_star_when_breakeven_is_absent() {
        let mut row = reuse_row("small", None, None);
        row.r_star_time = Some(10);
        row.r_star_bytes = Some(100);
        let view = view_with_reuse(vec![row]);
        let rows = view.reuse_rows();
        assert_eq!(rows[0].time_r, Some(10));
        assert_eq!(rows[0].bytes_r, Some(100));
    }

    #[test]
    fn reuse_rows_are_empty_without_a_reuse_block() {
        let view = view_with_reuse(Vec::new());
        assert!(view.reuse_rows().is_empty());
    }
}
