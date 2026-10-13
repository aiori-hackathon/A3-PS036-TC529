//! CLIMB benchmark suite — MTL vs non-MTL signing and verification benchmarks.
//!
//! # Modules
//!
//! - [`wire_size`] — permanent, reproducible wire-size measurement (replaces
//!   the throwaway probe that previously existed only as a markdown table).
//! - [`memory`] — direct RSS delta measurement of libMTL's memory footprint
//!   via `sysinfo`, separated from dataset/harness overhead.
//! - [`ladder_verify`] — measured MTL ladder shape (rungs, per-leaf auth paths,
//!   wire lengths) for arbitrary leaf counts, parsed from real libMTL bytes.

pub mod ladder_verify;
pub mod memory;
pub mod wire_size;
pub use ladder_verify::{
    measure_ladder_rows, LadderRow, LadderRung, LadderVerifyError, DEFAULT_LADDER_SEED,
    DEFAULT_LADDER_SIZES,
};
pub use memory::{
    compare_to_analytical, measure_all_sizes, measure_rss_delta, measure_rss_delta_repeated,
    RssComparison, RssDeltaReport, RssDeltaRun, RssSizeReport,
};
pub use wire_size::{measure_wire_sizes, CondensedStats, WireSizeReport, DEFAULT_WIRE_SEED};

// F11.2: parse /proc/self/status VmRSS and VmHWM (kB)
pub fn parse_status_kb(s: &str) -> Option<(u64, u64)> {
    let mut rss: Option<u64> = None;
    let mut hwm: Option<u64> = None;
    for line in s.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            if let Some(kb) = extract_status_kb(rest) {
                rss = Some(kb);
            }
        } else if let Some(rest) = line.strip_prefix("VmHWM:") {
            if let Some(kb) = extract_status_kb(rest) {
                hwm = Some(kb);
            }
        }
    }
    match (rss, hwm) {
        (Some(r), Some(h)) => Some((r, h)),
        _ => None,
    }
}

fn extract_status_kb(s: &str) -> Option<u64> {
    let s = s.trim();
    let mut parts = s.split_whitespace();
    let num_str = parts.next()?;
    let unit = parts.next().unwrap_or("");
    let num: u64 = num_str.parse().ok()?;
    match unit.to_ascii_lowercase().as_str() {
        "kb" | "kib" => Some(num),
        "mb" | "mib" => Some(num * 1024),
        "gb" | "gib" => Some(num * 1024 * 1024),
        _ => Some(num),
    }
}

#[cfg(test)]
mod tests_parse {
    use super::*;

    #[test]
    fn parse_status_kb_parses_valid_sample() {
        let s = "VmRSS:    12345 kB\nVmHWM:    23456 kB\n";
        assert_eq!(parse_status_kb(s), Some((12345, 23456)));
    }

    #[test]
    fn parse_status_kb_handles_mib() {
        let s = "VmRSS:    2 MB\nVmHWM:    4 MiB\n";
        assert_eq!(parse_status_kb(s), Some((2048, 4096)));
    }

    #[test]
    fn parse_status_kb_returns_none_for_malformed() {
        let s = "VmRSS: notanumber\nVmHWM: 100 kB\n";
        assert!(parse_status_kb(s).is_none());
    }
}

/// Compute median and interquartile range (IQR) for a slice of numbers.
/// Returns (median, q1, q3, iqr). The input is not sorted in-place.
pub fn median_iqr(values: &[u64]) -> (f64, u64, u64, u64) {
    let mut v = values.to_vec();
    if v.is_empty() {
        return (0.0, 0, 0, 0);
    }
    v.sort_unstable();
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2] as f64
    } else {
        (v[n / 2 - 1] as f64 + v[n / 2] as f64) / 2.0
    };
    let q1_idx = (n as f64 / 4.0).floor() as usize;
    let q3_idx = ((3.0 * n as f64) / 4.0).floor() as usize;
    let q1 = v[q1_idx];
    let q3 = v[q3_idx];
    let iqr = q3.saturating_sub(q1);
    (median, q1, q3, iqr)
}

#[cfg(test)]
mod tests_stats_median {
    use super::*;

    #[test]
    fn median_iqr_odd() {
        let v = [1, 2, 3, 4, 5];
        let (med, q1, q3, iqr) = median_iqr(&v);
        assert!((med - 3.0).abs() < 1e-9);
        assert_eq!(q1, 2);
        assert_eq!(q3, 4);
        assert_eq!(iqr, 2);
    }

    #[test]
    fn median_iqr_even() {
        let v = [1, 2, 3, 4, 5, 6];
        let (med, q1, q3, iqr) = median_iqr(&v);
        assert!((med - 3.5).abs() < 1e-9);
        assert_eq!(q1, 2);
        assert_eq!(q3, 5);
        assert_eq!(iqr, 3);
    }
}
