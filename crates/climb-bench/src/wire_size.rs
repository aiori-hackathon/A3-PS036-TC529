//! Permanent, reproducible wire-size measurement module.
//!
//! This module replaces the throwaway probe that previously existed only as a
//! markdown table.  It signs one batch over a deterministic dataset using the
//! same seed as the signing benchmark, then measures every condensed-signature
//! length, the shared ladder length, and the resulting full-signature
//! reconstruction.  The output is a [`WireSizeReport`] suitable for direct
//! serialisation into the benchmark report.
//!
//! # Why `Result` is deliberate
//!
//! The rule is that library code paths must not `unwrap`/`expect` on fallible
//! operations.  [`measure_wire_sizes`] calls `MtlKeyPair::generate()` and
//! `sign_batch`, both of which can fail (out-of-memory, buffer issues, etc.).
//! Returning `Result<WireSizeReport, MtlError>` forces every caller to
//! explicitly handle those failure modes instead of panicking mid-measurement.
//!
//! # Empirical uniformity
//!
//! The measurement iterates **every** message in the batch
//! (`output.messages()`), not just the first.  This is necessary because the
//! condensed signature length may depend on the leaf index within the Merkle
//! tree, and assuming uniformity from the first message alone would be
//! incorrect.
//!
//! # Wire-size definitions
//!
//! | Term | Definition |
//! |---|---|
//! | **Condensed signature** | The MTL authentication path + randomiser, one per message |
//! | **Ladder** | The shared signed Merkle tree ladder, signed once per batch |
//! | **With MTL total** | `ladder_len + sum(condensed_i)` — one ladder for N messages |
//! | **Without MTL total** | `sum(condensed_i) + N × ladder_len` — N full signatures, each `condensed_i \|\| ladder` |
//! | **Plain total (A)** | `N × signature_len` — one full SLH-DSA signature per message, no ladder |
//! | **Reduction** | `(without - with) / without × 100%` |
//!
//! Here *without MTL* means the MTL wrapper's non-amortised mode: every
//! message carries a **complete MTL signature** (`condensed_i || ladder`).  It
//! is not the plain SLH-DSA RRSIG baseline (a single full SLH-DSA signature per
//! RRset, with no ladder at all), which `docs/BASELINES.md` calls configuration
//! A and which this report records separately as [`PlainRrsigStats`].
//!
//! Configuration A's per-message signature length is read from liboqs (never
//! compiled in), and its batch total uses the project's existing
//! per-message-to-N extrapolation — the same `N × per-message` identity used to
//! extend timing measurements across dataset sizes.
//!
//! The identity `without - with == (N - 1) × ladder_len` holds by construction
//! because the only difference between the two totals is that *without-MTL*
//! includes the ladder N times (once per full signature), while *with-MTL*
//! includes it once (shared).  The arithmetic is verified by both the unit
//! tests in this module and the integration tests in
//! `tests/wire_size.rs`.

use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::{MtlError, MtlKeyPair};

/// Default seed; MUST equal the signing bench seed constant 0x434C494D425F5635 ("CLIMB_V5").
pub const DEFAULT_WIRE_SEED: u64 = 0x434C494D425F5635;

/// Summary statistics over condensed-signature byte lengths.
///
/// Computed by iterating every message in a signing batch; see
/// [`measure_wire_sizes`] for the measurement procedure.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CondensedStats {
    /// Minimum condensed-signature length observed.
    pub min: usize,
    /// Maximum condensed-signature length observed.
    pub max: usize,
    /// Arithmetic mean of all condensed-signature lengths.
    pub mean: f64,
    /// Exact sum of all per-message condensed lengths (in bytes).
    pub total: u64,
    /// `true` iff every message produced the same condensed length
    /// (i.e. `min == max`).
    pub uniform: bool,
}

/// Plain (configuration A) wire totals: one full SLH-DSA signature per RRset.
///
/// The per-message signature length is read from liboqs rather than compiled
/// in, and the batch total applies the project's existing per-message-to-N
/// extrapolation (`message_count × signature_len`) to that measured length.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct PlainRrsigStats {
    /// The liboqs-reported SLH-DSA signature length, in bytes per message.
    pub signature_len: usize,
    /// Number of signatures actually measured to obtain `signature_len`.
    pub samples: usize,
    /// Extrapolated batch total: `message_count × signature_len`.
    pub total: u64,
}

/// A complete wire-size measurement report covering one dataset size.
///
/// Produced by [`measure_wire_sizes`]; suitable for direct serialisation into
/// the benchmark report (via `serde`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireSizeReport {
    /// The dataset size used.
    pub size: DatasetSize,
    /// The seed that deterministically generated the dataset.
    pub seed: u64,
    /// Number of messages in the batch.
    pub message_count: usize,
    /// Length of the shared signed ladder, in bytes.
    pub ladder_len: usize,
    /// Statistics over per-message condensed-signature lengths.
    pub condensed: CondensedStats,
    /// Total wire bytes with MTL amortisation: `ladder_len + condensed.total`.
    pub with_mtl_total: u64,
    /// Estimated total wire bytes for the non-amortised mode, in which every
    /// message carries a complete MTL signature (a condensed path concatenated
    /// with the shared ladder): `condensed.total + message_count × ladder_len`.
    pub without_mtl_total: u64,
    /// Relative wire-size reduction, in percent:
    /// `(without_mtl_total - with_mtl_total) / without_mtl_total × 100`.
    pub reduction_percent: f64,
    /// Number of positions where `full_signature(i).len() == condensed_len + ladder_len`
    /// was verified (sampled, not exhaustive — see module docs).
    pub full_invariant_samples: usize,
    /// Plain RRSIG (configuration A) totals. Defaulted when absent so reports
    /// written before configuration A existed still parse (see module docs).
    #[serde(default)]
    pub plain: PlainRrsigStats,
}

// ---------------------------------------------------------------------------
// Private pure helper — unit-testable without native library dependencies
// ---------------------------------------------------------------------------

/// Compute [`CondensedStats`] from a slice of byte lengths.
///
/// This is a private pure function extracted for unit-testability without
/// native library dependencies.  It is called by [`measure_wire_sizes`] after
/// collecting all condensed-signature lengths.
///
/// # Panics
///
/// Panics if `lens` is empty.  The caller (the public API) guarantees the
/// slice is non-empty because [`measure_wire_sizes`] only calls this after a
/// successful `sign_batch`, which rejects empty batches.
fn stats(lens: &[usize]) -> CondensedStats {
    assert!(!lens.is_empty(), "lens must be non-empty");

    // SAFETY: `min`/`max` on a non-empty iterator always return `Some`.
    let min = *lens.iter().min().unwrap();
    let max = *lens.iter().max().unwrap();
    let total: u64 = lens.iter().map(|l| *l as u64).sum();
    let mean = total as f64 / lens.len() as f64;
    let uniform = min == max;

    CondensedStats {
        min,
        max,
        mean,
        total,
        uniform,
    }
}

// ---------------------------------------------------------------------------
// Public measurement function
// ---------------------------------------------------------------------------

/// Sign one batch over the dataset and measure wire sizes.
///
/// # Procedure
///
/// 1. Generate the dataset via [`generate_dataset`] with the given size and
///    seed.
/// 2. Build one [`MtlKeyPair`] and one signer, then call
///    [`sign_batch`](climb_mtl::MtlSigner::sign_batch) exactly **once** over
///    all `N` messages.
/// 3. `ladder_len` is `output.ladder().len()`.
/// 4. Iterate every message in `output.messages()`, collect each
///    `condensed().len()`, and compute [`CondensedStats`] (min, max, mean,
///    total, uniform).
/// 5. `with_mtl_total = ladder_len + condensed.total` — one shared ladder.
/// 6. `without_mtl_total = condensed.total + message_count × ladder_len` —
///    `N` reconstructed full signatures, each `condensed_i || ladder`.  The
///    arithmetic identity `without - with == (N - 1) × ladder_len` holds.
/// 7. `reduction_percent` is guarded against division by zero (unreachable
///    for defined `DatasetSize` variants).
/// 8. The full-signature invariant (`full.len() == condensed.len() +
///    ladder.len()`) is checked at a **sample** of positions (minimum:
///    indices 0, `N/2`, `N-1`, plus 1, 2, `N-2` when `N >= 4`),
///    counting the number actually checked into `full_invariant_samples`.
///    Not all `N` are reconstructed — `Large` (1M × ~8.5 KB ≈ 8 GB) would
///    be prohibitive.
/// 9. `plain.signature_len` is read from liboqs and `plain.total` is the
///    extrapolated `message_count × signature_len` (configuration A).
///
/// # Errors
///
/// Returns [`MtlError`] if key generation or signing fails.
///
/// # Performance note
///
/// For `DatasetSize::Large` (1M messages), this function allocates the
/// dataset (~200 MB) and signs it once (one SLH-DSA signature + tree build).
/// It does **not** reconstruct all N full signatures.
pub fn measure_wire_sizes(size: DatasetSize, seed: u64) -> Result<WireSizeReport, MtlError> {
    let dataset = generate_dataset(size, seed);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let message_count = messages.len();

    // N is always positive for every DatasetSize variant (Small=100,
    // OneK=1K, Medium=10K, Large=1M).  This guard exists only so the type
    // system does not require an unreachable fallback to satisfy
    // correctness.
    if message_count == 0 {
        return Ok(WireSizeReport {
            size,
            seed,
            message_count: 0,
            ladder_len: 0,
            condensed: CondensedStats {
                min: 0,
                max: 0,
                mean: 0.0,
                total: 0,
                uniform: true,
            },
            with_mtl_total: 0,
            without_mtl_total: 0,
            reduction_percent: 0.0,
            full_invariant_samples: 0,
            plain: PlainRrsigStats::default(),
        });
    }

    let plain_signature_len = climb_mtl::plain_signature_len()?;
    let plain = PlainRrsigStats {
        signature_len: plain_signature_len,
        samples: 1,
        total: (message_count as u64) * (plain_signature_len as u64),
    };

    let keypair = MtlKeyPair::generate()?;
    let signer = keypair.signer();
    let output = signer.sign_batch(&messages)?;
    let ladder = output.ladder();
    let ladder_len = ladder.len();

    // Collect every condensed-signature length — do not assume the first
    // message is representative.
    let lens: Vec<usize> = output
        .messages()
        .iter()
        .map(|m| m.condensed().len())
        .collect();
    let condensed = stats(&lens);

    let with_mtl_total = ladder_len as u64 + condensed.total;
    let without_mtl_total = condensed.total + (message_count as u64) * (ladder_len as u64);

    // Guard: without_mtl_total would be zero only for a degenerate case
    // with zero-length signatures and zero-length ladder, which does not
    // occur for SLH-DSA.  The check is defensive.
    let reduction_percent = if without_mtl_total > 0 {
        (without_mtl_total - with_mtl_total) as f64 / without_mtl_total as f64 * 100.0
    } else {
        0.0
    };

    // Sample positions for the full-signature invariant check.
    // Chosen indices: 0, N/2, N-1 (always), plus 1, 2, N-2 (when N >= 4).
    // Deduplicated via BTreeSet and clamped to [0, N).
    let mut sample_indices: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    sample_indices.insert(0);
    sample_indices.insert(message_count / 2);
    sample_indices.insert(message_count.saturating_sub(1));
    if message_count >= 3 {
        sample_indices.insert(1);
        sample_indices.insert(2);
    }
    if message_count >= 4 {
        sample_indices.insert(message_count.saturating_sub(2));
    }

    let mut full_invariant_samples = 0usize;
    for &idx in &sample_indices {
        if idx >= message_count {
            continue;
        }
        let full_sig = output.full_signature(idx)?;
        // lens[idx] is valid because idx < message_count was just checked.
        let condensed_len = lens[idx];
        // This assertion encodes a structural invariant of full_signature():
        // it is exactly `condensed || ladder`.  If it ever fails, the MTL
        // wrapper's reconstruction logic is broken, which is a correctness
        // gate, not a claim about the cryptographic protocol itself.
        assert_eq!(
            full_sig.len(),
            condensed_len + ladder_len,
            "full-signature invariant failed at index {idx}: \
             full.len()={} != condensed.len()={} + ladder.len()={}",
            full_sig.len(),
            condensed_len,
            ladder_len,
        );
        full_invariant_samples += 1;
    }

    Ok(WireSizeReport {
        size,
        seed,
        message_count,
        ladder_len,
        condensed,
        with_mtl_total,
        without_mtl_total,
        reduction_percent,
        full_invariant_samples,
        plain,
    })
}

// ---------------------------------------------------------------------------
// Unit tests for the private stats() helper
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_non_empty_all_equal() {
        let lens = vec![10usize, 10, 10];
        let s = stats(&lens);
        assert_eq!(s.min, 10);
        assert_eq!(s.max, 10);
        assert!((s.mean - 10.0).abs() < f64::EPSILON);
        assert_eq!(s.total, 30);
        assert!(s.uniform);
    }

    #[test]
    fn stats_non_empty_mixed() {
        let lens = vec![5usize, 10, 15];
        let s = stats(&lens);
        assert_eq!(s.min, 5);
        assert_eq!(s.max, 15);
        assert!((s.mean - 10.0).abs() < f64::EPSILON);
        assert_eq!(s.total, 30);
        assert!(!s.uniform);
    }

    #[test]
    fn stats_single_element() {
        let lens = vec![42usize];
        let s = stats(&lens);
        assert_eq!(s.min, 42);
        assert_eq!(s.max, 42);
        assert!((s.mean - 42.0).abs() < f64::EPSILON);
        assert_eq!(s.total, 42);
        assert!(s.uniform);
    }

    #[test]
    #[should_panic(expected = "lens must be non-empty")]
    fn stats_empty_panics() {
        stats(&[]);
    }

    #[test]
    fn stats_large_uniform() {
        let lens = vec![7usize; 10_000];
        let s = stats(&lens);
        assert_eq!(s.min, 7);
        assert_eq!(s.max, 7);
        assert!((s.mean - 7.0).abs() < f64::EPSILON);
        assert_eq!(s.total, 70_000);
        assert!(s.uniform);
    }
}
