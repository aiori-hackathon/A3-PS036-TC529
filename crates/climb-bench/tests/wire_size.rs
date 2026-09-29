//! Integration tests for wire-size measurement (Phase 8.4).
//!
//! Uses `DatasetSize::Small` (and `Medium` where cheap) to keep runtime
//! reasonable.  All tests verify behaviour through the public
//! [`climb_bench::measure_wire_sizes`] API.

use climb_bench::{measure_wire_sizes, DEFAULT_WIRE_SEED};
use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::MtlKeyPair;

// ---------------------------------------------------------------------------
// Test 1 — full_signature == condensed || ladder (independent verification)
// ---------------------------------------------------------------------------

/// Independently sign a Small batch via the public [`climb_mtl`] API and
/// verify, at several positions (incl. 0 and last), that
/// `full_signature(i).len() == condensed.len() + ladder.len()`.
#[test]
fn full_signature_equals_condensed_plus_ladder() {
    let dataset = generate_dataset(DatasetSize::Small, DEFAULT_WIRE_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();

    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let output = signer.sign_batch(&messages).expect("sign_batch");

    let ladder_len = output.ladder().len();
    let n = output.len();

    // Check several positions including 0 and last.
    let indices = [0usize, n.saturating_sub(1)];
    for &idx in &indices {
        if idx >= n {
            continue;
        }
        let condensed_len = output.messages()[idx].condensed().len();
        let full_len = output.full_signature(idx).expect("full_signature").len();
        assert_eq!(
            full_len,
            condensed_len + ladder_len,
            "full_signature({idx}).len() = {full_len}, \
             expected {condensed_len} + {ladder_len} = {}",
            condensed_len + ladder_len
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2 — condensed-length characterisation (empirical duty)
// ---------------------------------------------------------------------------

/// Call `measure_wire_sizes(Small, DEFAULT_WIRE_SEED)`, assert min ≤ mean ≤ max,
/// total consistency, uniform == (min == max), and **print** the observed
/// min / max / mean / uniform for human verification under `--nocapture`.
#[test]
fn condensed_length_is_characterized() {
    let report =
        measure_wire_sizes(DatasetSize::Small, DEFAULT_WIRE_SEED).expect("measure_wire_sizes");

    let c = &report.condensed;
    let n = report.message_count as f64;

    // Min ≤ mean ≤ max.
    assert!(
        (c.min as f64) <= c.mean + f64::EPSILON,
        "min={} > mean={}",
        c.min,
        c.mean
    );
    assert!(
        c.mean <= (c.max as f64) + f64::EPSILON,
        "mean={} > max={}",
        c.mean,
        c.max
    );

    // Total == message_count × mean (within f64 tolerance, or exact via
    // recomputation of the integer sum).
    let expected_total = (c.mean * n).round() as u64;
    let diff = c.total.abs_diff(expected_total);
    assert!(
        diff <= 1, // f64 rounding on 100 elements → at most 1 ulp off
        "total={} != message_count * mean ≈ {expected_total} (diff={diff})",
        c.total
    );

    // uniform must match min == max.
    assert_eq!(
        c.uniform,
        c.min == c.max,
        "uniform={} but min={} max={}",
        c.uniform,
        c.min,
        c.max
    );

    // Empirical duty: print the observed values.
    println!(
        "Small ({n:.0} messages, seed=0x{DEFAULT_WIRE_SEED:X}): \
         min={min}, max={max}, mean={mean:.6}, uniform={uniform}",
        n = report.message_count,
        min = c.min,
        max = c.max,
        mean = c.mean,
        uniform = c.uniform,
    );
}

// ---------------------------------------------------------------------------
// Test 3 — total-bytes arithmetic consistency
// ---------------------------------------------------------------------------

/// Verify the arithmetic identities defined in the wire_size module docs.
#[test]
fn total_bytes_arithmetic_consistent() {
    let report =
        measure_wire_sizes(DatasetSize::Small, DEFAULT_WIRE_SEED).expect("measure_wire_sizes");

    let n = report.message_count as u64;
    let l = report.ladder_len as u64;
    let c = report.condensed.total;

    // with_mtl_total == ladder_len + condensed.total
    assert_eq!(
        report.with_mtl_total,
        l + c,
        "with_mtl_total={} != ladder_len + condensed.total = {} + {c} = {}",
        report.with_mtl_total,
        l,
        l + c
    );

    // without_mtl_total == condensed.total + N * ladder_len
    assert_eq!(
        report.without_mtl_total,
        c + n * l,
        "without_mtl_total={} != condensed.total + N*ladder_len = {c} + {n}*{l} = {}",
        report.without_mtl_total,
        c + n * l
    );

    // without > with (amortisation must reduce wire size for N > 1)
    assert!(
        report.without_mtl_total > report.with_mtl_total,
        "without_mtl_total={} must be > with_mtl_total={}",
        report.without_mtl_total,
        report.with_mtl_total,
    );

    // reduction_percent matches the formula to 1e-9.
    let expected_reduction = (report.without_mtl_total - report.with_mtl_total) as f64
        / report.without_mtl_total as f64
        * 100.0;
    assert!(
        (report.reduction_percent - expected_reduction).abs() < 1e-9,
        "reduction_percent={} != expected={expected_reduction}",
        report.reduction_percent,
    );
}

// ---------------------------------------------------------------------------
// Test 4 — determinism for same seed
// ---------------------------------------------------------------------------

/// Two calls with the same seed must produce identical
/// [`WireSizeReport`] values (derived [`PartialEq`]).
///
/// Key generation uses fresh entropy each time, but the wire-size report
/// depends only on condensed-signature lengths and ladder length, which are
/// deterministic for a given dataset size and seed.
#[test]
fn deterministic_for_same_seed() {
    let r1 = measure_wire_sizes(DatasetSize::Small, DEFAULT_WIRE_SEED).expect("run 1");
    let r2 = measure_wire_sizes(DatasetSize::Small, DEFAULT_WIRE_SEED).expect("run 2");

    assert_eq!(r1, r2, "wire-size reports for same seed must be identical");
}

// ---------------------------------------------------------------------------
// Test 5 — larger batches have longer ladders
// ---------------------------------------------------------------------------

/// ladder_len(Small) < ladder_len(Medium) — empirical monotonicity
/// cross-check.  More messages → deeper tree → longer ladder.
#[test]
fn larger_batches_have_longer_ladders() {
    let small = measure_wire_sizes(DatasetSize::Small, DEFAULT_WIRE_SEED).expect("small");
    let medium = measure_wire_sizes(DatasetSize::Medium, DEFAULT_WIRE_SEED).expect("medium");

    assert!(
        small.ladder_len < medium.ladder_len,
        "ladder_len(Small)={} must be < ladder_len(Medium)={}",
        small.ladder_len,
        medium.ladder_len,
    );
}
