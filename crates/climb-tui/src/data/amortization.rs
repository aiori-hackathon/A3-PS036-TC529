//! Per-message display mode and the signing amortization curve.

use climb_report::charts::{amortization_data, fmt_count, fmt_time_ns};

use super::ResultsView;

/// Which quantity the performance bars and tables show, toggled with `v`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueMode {
    /// Batch mean over the whole dataset (ns).
    #[default]
    Total,
    /// Batch mean divided by message count (ns/message).
    PerMessage,
}

impl ValueMode {
    /// Label used in block titles and the status bar.
    pub fn label(self) -> &'static str {
        match self {
            ValueMode::Total => "total time",
            ValueMode::PerMessage => "per message",
        }
    }

    /// The other mode.
    pub fn toggled(self) -> Self {
        match self {
            ValueMode::Total => ValueMode::PerMessage,
            ValueMode::PerMessage => ValueMode::Total,
        }
    }

    /// Map a batch mean (ns) over `messages` messages to the displayed value.
    pub fn apply(self, mean_ns: f64, messages: u64) -> f64 {
        match self {
            ValueMode::Total => mean_ns,
            ValueMode::PerMessage => per_message(mean_ns, messages),
        }
    }
}

/// Per-message cost (ns); a zero-message batch is treated as a single sample.
pub fn per_message(mean_ns: f64, messages: u64) -> f64 {
    if messages == 0 {
        mean_ns
    } else {
        mean_ns / messages as f64
    }
}

/// Render-ready amortization curve (log10-log10 coordinates for the chart).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmortizationView {
    pub has_data: bool,
    pub with_points: Vec<(f64, f64)>,
    pub without_points: Vec<(f64, f64)>,
    pub fit_points: Vec<(f64, f64)>,
    pub x_bounds: [f64; 2],
    pub y_bounds: [f64; 2],
    pub x_labels: Vec<String>,
    pub y_labels: Vec<String>,
}

/// Convert per-message points to log10-log10 chart coordinates.
fn log_points(points: &[(u64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .filter(|(_, y)| y.is_finite() && *y > 0.0)
        .map(|(n, y)| ((*n as f64).log10(), y.log10()))
        .collect()
}

/// Sample the OLS fit of the with-MTL batch total as per-message time, over a
/// log-spaced sweep of `n` between the measured extremes.
fn log_fit(fit: Option<(f64, f64)>, n_min: f64, n_max: f64) -> Vec<(f64, f64)> {
    const STEPS: usize = 32;
    let Some((intercept, slope)) = fit else {
        return Vec::new();
    };
    let lo = n_min.log10();
    let hi = n_max.log10();
    if hi <= lo {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let t = i as f64 / STEPS as f64;
        let n = 10f64.powf(lo + t * (hi - lo));
        let per_msg = (intercept + slope * n) / n;
        if per_msg.is_finite() && per_msg > 0.0 {
            out.push((n.log10(), per_msg.log10()));
        }
    }
    out
}

/// Integer-decade axis bounds plus one label per decade in range.
fn decade_axis(min_log: f64, max_log: f64, fmt: fn(f64) -> String) -> ([f64; 2], Vec<String>) {
    let lo = if min_log.is_finite() {
        min_log.floor()
    } else {
        0.0
    };
    let mut hi = if max_log.is_finite() {
        max_log.ceil()
    } else {
        1.0
    };
    if hi <= lo {
        hi = lo + 1.0;
    }
    let mut labels = Vec::new();
    for k in lo as i64..=hi as i64 {
        labels.push(fmt(10f64.powi(k as i32)));
    }
    ([lo, hi], labels)
}

impl ResultsView {
    /// The signing amortization curve: measured per-message points, the
    /// without-MTL baseline, and the OLS fit of the with-MTL batch total.
    pub fn amortization_view(&self) -> AmortizationView {
        let rows = &self.results.benchmark.signing;
        let data = amortization_data(rows);
        let with_points = log_points(&data.with_points);
        let without_points = log_points(&data.without_points);
        let ns: Vec<u64> = data
            .with_points
            .iter()
            .chain(data.without_points.iter())
            .map(|(n, _)| *n)
            .collect();
        if ns.is_empty() {
            return AmortizationView::default();
        }
        let n_min = ns.iter().copied().min().unwrap_or(0) as f64;
        let n_max = ns.iter().copied().max().unwrap_or(0) as f64;
        let fit_points = log_fit(data.fit, n_min, n_max);

        let mut all_log_y: Vec<f64> = with_points
            .iter()
            .chain(without_points.iter())
            .chain(fit_points.iter())
            .map(|(_, y)| *y)
            .filter(|y| y.is_finite())
            .collect();
        if all_log_y.is_empty() {
            return AmortizationView::default();
        }
        all_log_y.sort_by(f64::total_cmp);
        let (x_bounds, x_labels) = decade_axis(n_min.log10(), n_max.log10(), fmt_count);
        let (y_bounds, y_labels) =
            decade_axis(all_log_y[0], all_log_y[all_log_y.len() - 1], fmt_time_ns);

        AmortizationView {
            has_data: true,
            with_points,
            without_points,
            fit_points,
            x_bounds,
            y_bounds,
            x_labels,
            y_labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::test_support::{approx, empty_derived, signing_row_n, view_with};

    #[test]
    fn value_mode_toggles_and_applies() {
        assert_eq!(ValueMode::default(), ValueMode::Total);
        assert_eq!(ValueMode::Total.toggled(), ValueMode::PerMessage);
        assert_eq!(ValueMode::PerMessage.toggled(), ValueMode::Total);
        assert_eq!(ValueMode::Total.label(), "total time");
        assert_eq!(ValueMode::PerMessage.label(), "per message");
        assert_eq!(ValueMode::Total.apply(1000.0, 100), 1000.0);
        assert_eq!(ValueMode::PerMessage.apply(1000.0, 100), 10.0);
        assert_eq!(per_message(50.0, 0), 50.0, "zero messages -> single sample");
    }

    #[test]
    fn amortization_view_builds_log10_points_and_decade_axes() {
        let view = view_with(
            vec![
                signing_row_n("Small", "With MTL", 1000.0, 100),
                signing_row_n("OneK", "With MTL", 10_000.0, 1000),
                signing_row_n("Medium", "With MTL", 100_000.0, 10_000),
                signing_row_n("Large", "With MTL", 10_000_000.0, 1_000_000),
                signing_row_n("Small", "Without MTL", 100_000.0, 100),
                signing_row_n("OneK", "Without MTL", 1_000_000.0, 1000),
            ],
            Vec::new(),
            Vec::new(),
            None,
            empty_derived(),
        );
        let amort = view.amortization_view();
        assert!(amort.has_data);
        assert_eq!(
            amort.with_points,
            vec![(2.0, 1.0), (3.0, 1.0), (4.0, 1.0), (6.0, 1.0)],
            "log10(n), log10(per-message ns); all with-MTL rows are 10 ns/msg"
        );
        assert_eq!(
            amort.without_points,
            vec![(2.0, 3.0), (3.0, 3.0)],
            "log10(1000 ns/msg) = 3"
        );
        assert_eq!(amort.fit_points.len(), 33, "32-step log-spaced sweep");
        assert!(
            amort.fit_points.iter().all(|(_, y)| approx(*y, 1.0)),
            "fit of the exact 10n line is per-message 10 ns everywhere"
        );
        assert_eq!(amort.x_bounds, [2.0, 6.0]);
        assert_eq!(
            amort.x_labels,
            vec!["100", "1,000", "10,000", "100,000", "1,000,000"]
        );
        assert_eq!(amort.y_bounds[1], 3.0, "y max decade is 1 ms");
        assert!(
            amort.y_bounds[0] == 1.0 || amort.y_bounds[0] == 0.0,
            "fp noise near the 10 ns boundary may floor to 0: {:?}",
            amort.y_bounds
        );
        assert_eq!(
            amort.y_labels.last().map(String::as_str),
            Some("1 \u{00b5}s")
        );
        assert!(amort.y_labels.contains(&"10 ns".to_string()));
        assert!(amort.y_labels.contains(&"100 ns".to_string()));
    }

    #[test]
    fn amortization_view_is_empty_without_signing_rows() {
        let view = view_with(Vec::new(), Vec::new(), Vec::new(), None, empty_derived());
        let amort = view.amortization_view();
        assert!(!amort.has_data);
        assert!(amort.with_points.is_empty());
        assert!(amort.without_points.is_empty());
        assert!(amort.fit_points.is_empty());
    }
}
