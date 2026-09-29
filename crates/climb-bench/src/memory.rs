//! Direct RSS measurement for libMTL's memory footprint.
//!
//! This module measures the resident-set-size (RSS) delta before-vs-after a
//! single [`climb_mtl::MtlSigner::sign_batch`] call, with the dataset and
//! keypair generated **before** the delta window so their memory is already
//! factored into the "before" baseline.  The delta therefore approximates
//! libMTL's own internal footprint (tree node pages, randomizer pages, and the
//! returned `SignOutput`) **separate** from the dataset and the Rust harness.
//!
//! # Isolation boundary
//!
//! | Component | Lives in which measurement | Typical size | Source |
//! |---|---|---|---|
//! | Synthetic dataset (`Vec<Vec<u8>>`) | **Before** baseline only | ~93 MiB (payload) / ~529 MiB (RSS overhead, Phase 4) | [`climb_dns::generate_dataset`] |
//! | `MtlKeyPair` (cached key-image + native context) | **Before** baseline only | ~186 B (key image) + native context | [`climb_mtl::MtlKeyPair::generate`] |
//! | Rust harness binary + stack + heap baseline | **Before** baseline only | OS-dependent | This binary |
//! | **libMTL tree node pages** | **Delta** | pages × 1 MiB (lazy `calloc`) | `sign_batch` → native context |
//! | **libMTL randomizer pages** | **Delta** | pages × 1 MiB | `sign_batch` → native context |
//! | **`SignOutput` (condensed sigs + ladder)** | **Delta** | `O(N × mean_condensed) + ladder` (tens of KB to ~370 MB for Large) | [`climb_mtl::SignOutput`] |
//! | **libMTL transient buffers during signing** | **Delta** | small, freed before return | Inside `sign_batch` |
//!
//! The `SignOutput` contribution is **not** separable at the RSS level (libMTL
//! does not expose internal page vs. output-buffer accounting).  At
//! Small/Medium the output is &lt; 3 MB, so the tree pages dominate.  At Large the
//! output is ~370 MB (1M × mean condensed ~367 B + 8 KB ladder), so the analytical
//! comparison in Phase 10.2 must account for this.
//!
//! # Measurement protocol
//!
//! 1. Generate the dataset and keypair — these are held across runs.
//! 2. (Optional) For repeat runs, drop the previous `SignOutput` first so
//!    libMTL can free its pages before the next "before" sample.
//! 3. Sample RSS (before).
//! 4. Call `sign_batch(&messages)`.
//! 5. Sample RSS (after) — the `SignOutput` is still alive.
//! 6. delta = after − before.
//!
//! # Why `sysinfo` rather than `/proc/self/statm`
//!
//! `sysinfo` provides a cross-platform abstraction and handles Linux `procfs`
//! details (page-size multiplication, PID filtering) correctly.  The RSS field
//! (`process.memory()`) returns the resident set size in bytes.

use climb_analytical::MemoryEstimate;
use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::{MtlError, MtlKeyPair};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sample the current process RSS in bytes.
///
/// Creates a fresh [`System`] and queries only the current process with
/// memory-refresh enabled.  Returns 0 if the process is not found (unreachable
/// in practice — `std::process::id()` always maps to a live PID).
fn sample_rss_bytes() -> u64 {
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_memory()),
    );
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of a single RSS delta measurement run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RssDeltaRun {
    /// RSS in bytes before calling `sign_batch`.
    pub rss_before_bytes: u64,
    /// RSS in bytes after `sign_batch` returned (output still alive).
    pub rss_after_bytes: u64,
    /// `after - before` in bytes.
    pub delta_bytes: u64,
    /// `delta_bytes` in mebibytes (MiB = 2²⁰ bytes).
    pub delta_mib: f64,
}

/// Result of a measurement across the three blueprint sizes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RssDeltaReport {
    /// The dataset seed used.
    pub seed: u64,
    /// Per-size measurement runs.
    pub runs: Vec<RssSizeReport>,
}

/// One dataset size's RSS delta measurements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RssSizeReport {
    /// Dataset size.
    pub size: DatasetSize,
    /// Individual measurement runs.
    pub runs: Vec<RssDeltaRun>,
    /// Number of repeats (configured by caller).
    pub repeat_count: usize,
}

// ---------------------------------------------------------------------------
// Core measurement
// ---------------------------------------------------------------------------

/// Measure the RSS delta for a single sign_batch call at one dataset size.
///
/// The dataset and keypair are generated *before* the delta window so their
/// memory is already accounted for in the "before" baseline.  See the module
/// documentation for the isolation rationale.
///
/// # Errors
///
/// Returns [`MtlError`] if key generation or signing fails.
pub fn measure_rss_delta(size: DatasetSize, seed: u64) -> Result<RssDeltaRun, MtlError> {
    // Generate dataset and keypair outside the delta window.
    let dataset = generate_dataset(size, seed);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let keypair = MtlKeyPair::generate()?;

    // Sample RSS before — dataset and keypair are already in memory.
    let rss_before = sample_rss_bytes();

    // Sign: this creates a fresh native context (tree pages, randomizers)
    // and returns a SignOutput.
    let signer = keypair.signer();
    let _output = signer.sign_batch(&messages)?;
    // Output is still alive — RSS includes tree pages + SignOutput.

    let rss_after = sample_rss_bytes();

    // Drop the output AFTER sampling to include libMTL pages in the delta.
    // (_output is dropped here)

    let delta_bytes = rss_after.saturating_sub(rss_before);
    let delta_mib = delta_bytes as f64 / (1024.0 * 1024.0);

    Ok(RssDeltaRun {
        rss_before_bytes: rss_before,
        rss_after_bytes: rss_after,
        delta_bytes,
        delta_mib,
    })
}

/// Measure RSS delta for one size, repeated `n` times, with a forced drop
/// of the previous `SignOutput` between runs so libMTL can free its pages.
///
/// Each run creates a fresh keypair and signer to ensure independent
/// measurements — a persistent context would accumulate leaves across calls,
/// making the delta grow with every repeat instead of resetting.
///
/// # Errors
///
/// Returns [`MtlError`] if any run's key generation or signing fails.
pub fn measure_rss_delta_repeated(
    size: DatasetSize,
    seed: u64,
    repeat_count: usize,
) -> Result<RssSizeReport, MtlError> {
    let mut runs = Vec::with_capacity(repeat_count);
    for _ in 0..repeat_count {
        let run = measure_rss_delta(size, seed)?;
        // Drop the SignOutput from this run before the next one, so the
        // next "before" baseline does not include stale libMTL pages.
        runs.push(run);
    }

    Ok(RssSizeReport {
        size,
        runs,
        repeat_count,
    })
}

/// Measure RSS delta across the three blueprint sizes (Small, Medium, Large),
/// each repeated `repeat_count` times.  Returns a complete [`RssDeltaReport`].
///
/// # Errors
///
/// Returns [`MtlError`] if any measurement fails.
pub fn measure_all_sizes(seed: u64, repeat_count: usize) -> Result<RssDeltaReport, MtlError> {
    let sizes = [DatasetSize::Small, DatasetSize::Medium, DatasetSize::Large];
    let mut runs = Vec::with_capacity(sizes.len());
    for &size in &sizes {
        let report = measure_rss_delta_repeated(size, seed, repeat_count)?;
        runs.push(report);
    }
    Ok(RssDeltaReport { seed, runs })
}

/// Cross-validate measured RSS delta against the Phase 9 analytical estimate.
///
/// Returns a comparison row suitable for table rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RssComparison {
    pub size: DatasetSize,
    pub message_count: usize,
    /// Mean of the measured repeat deltas, in bytes.
    pub measured_mean_bytes: u64,
    /// Phase 9 analytical total memory estimate, in bytes.
    pub analytical_bytes: usize,
    /// `measured_mean_bytes / analytical_bytes` as a ratio.  1.0 =
    /// perfect match.  &gt;1.0 = measured exceeds analytical.
    pub ratio: f64,
    /// The delta range across repeats (min–max).
    pub measured_range_bytes: (u64, u64),
    /// Standard deviation of the measured repeats, in bytes.
    pub measured_stddev_bytes: f64,
}

/// Produce a [`RssComparison`] table from an [`RssSizeReport`] and the
/// corresponding [`MemoryEstimate`].
pub fn compare_to_analytical(report: &RssSizeReport, analytical: MemoryEstimate) -> RssComparison {
    let n = report.runs.len() as f64;
    let mean_delta: f64 = report
        .runs
        .iter()
        .map(|r| r.delta_bytes as f64)
        .sum::<f64>()
        / n;
    let mean_delta_u64 = mean_delta.round() as u64;

    let min_delta = report.runs.iter().map(|r| r.delta_bytes).min().unwrap_or(0);
    let max_delta = report.runs.iter().map(|r| r.delta_bytes).max().unwrap_or(0);

    let variance: f64 = report
        .runs
        .iter()
        .map(|r| {
            let d = r.delta_bytes as f64 - mean_delta;
            d * d
        })
        .sum::<f64>()
        / n;
    let stddev = variance.sqrt();

    let analytical_bytes = analytical.total_memory_bytes;
    let ratio = if analytical_bytes > 0 {
        mean_delta_u64 as f64 / analytical_bytes as f64
    } else {
        f64::NAN
    };

    RssComparison {
        size: report.size,
        message_count: report.size.count(),
        measured_mean_bytes: mean_delta_u64,
        analytical_bytes,
        ratio,
        measured_range_bytes: (min_delta, max_delta),
        measured_stddev_bytes: stddev,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use climb_analytical::estimate_memory_footprint;

    /// Smoke test: a single RSS delta measurement must return a
    /// non-negative, reasonable delta for Small.  We do NOT assert a
    /// specific value — RSS is OS-dependent and varies across machines.
    /// We only assert the delta is positive (libMTL DOES allocate memory
    /// during signing) and within plausible bounds (&lt;100 MiB for Small).
    #[test]
    fn rss_delta_small_is_positive_and_plausible() {
        let run = measure_rss_delta(
            DatasetSize::Small,
            0x434C494D425F5635, // CLIMB_V5
        )
        .expect("measure_rss_delta(Small)");

        // Before/after are OS-dependent but must be positive.
        assert!(run.rss_before_bytes > 0, "rss_before must be > 0");
        assert!(run.rss_after_bytes > 0, "rss_after must be > 0");

        // Delta must be non-negative and within reasonable bounds.
        // Small's analytical estimate is 2 MiB (tree + randomizer).
        // Allow generous headroom for sign output + OS noise.
        assert!(
            run.delta_bytes > 0,
            "delta must be positive — libMTL allocates during sign_batch"
        );
        assert!(
            run.delta_bytes < 100 * 1024 * 1024,
            "delta {} > 100 MiB for Small — implausible",
            run.delta_mib
        );

        // The delta_mib helper should match.
        let expected_mib = run.delta_bytes as f64 / (1024.0 * 1024.0);
        assert!(
            (run.delta_mib - expected_mib).abs() < 1e-6,
            "delta_mib helper mismatch"
        );
    }

    /// Two repeat runs of Small should produce non-negative, plausible deltas.
    /// After the first run, Linux may retain freed libMTL pages in RSS, so
    /// subsequent runs may show a smaller delta (pages are reused, not freshly
    /// allocated). This test only checks that both deltas are non-negative and
    /// within plausible bounds — NOT that they match.
    #[test]
    fn rss_delta_repeated_is_plausible() {
        let report = measure_rss_delta_repeated(DatasetSize::Small, 0x434C494D425F5635, 2)
            .expect("measure_rss_delta_repeated(Small, 2)");

        assert_eq!(report.runs.len(), 2);
        assert_eq!(report.repeat_count, 2);

        for (i, run) in report.runs.iter().enumerate() {
            assert!(
                run.delta_bytes < 100 * 1024 * 1024,
                "run {i}: delta {} > 100 MiB — implausible",
                run.delta_mib
            );
            assert!(
                run.rss_before_bytes > 0 && run.rss_after_bytes > 0,
                "run {i}: before={} after={}",
                run.rss_before_bytes,
                run.rss_after_bytes,
            );
        }
    }

    #[test]
    fn compare_to_analytical_produces_reasonable_ratio() {
        // Use a known analytical estimate.
        let analytical = estimate_memory_footprint(climb_analytical::DatasetSize::Small.count());

        let report = RssSizeReport {
            size: DatasetSize::Small,
            runs: vec![
                RssDeltaRun {
                    rss_before_bytes: 100_000_000,
                    rss_after_bytes: 102_097_152, // ~2 MiB delta
                    delta_bytes: 2_097_152,
                    delta_mib: 2.0,
                },
                RssDeltaRun {
                    rss_before_bytes: 101_000_000,
                    rss_after_bytes: 103_200_000,
                    delta_bytes: 2_200_000,
                    delta_mib: 2.098,
                },
            ],
            repeat_count: 2,
        };

        let cmp = compare_to_analytical(&report, analytical);

        assert_eq!(cmp.size, DatasetSize::Small);
        assert_eq!(cmp.message_count, 100);
        // Mean of 2_097_152 + 2_200_000 = 2_148_576
        assert_eq!(cmp.measured_mean_bytes, 2_148_576);

        // Analytical: Small = 2 MiB = 2,097,152 bytes
        // Ratio: 2,148,576 / 2,097,152 ≈ 1.0245
        assert!(
            cmp.ratio > 0.9 && cmp.ratio < 1.5,
            "ratio {:.4} outside expected range",
            cmp.ratio
        );

        assert_eq!(cmp.measured_range_bytes, (2_097_152, 2_200_000));
        assert!(cmp.measured_stddev_bytes > 0.0, "stddev should be positive");
    }
}
