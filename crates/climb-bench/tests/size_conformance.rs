//! Conformance between three descriptions of the MTL wire format.
//!
//! Three independent sources are compared:
//!
//! 1. **Literal layout arithmetic** in this file, derived from the serialized
//!    field order in the vendored library (`mtl_buffer.c`) using only the two
//!    constants a reader can check against the source: a 16-byte hash and an
//!    8-byte `MTL_INDEX`.
//! 2. The closed-form model in `climb-analytical`, which exposes the same
//!    quantities as named functions.
//! 3. Real libMTL output, signed and parsed by
//!    [`climb_bench::measure_ladder_rows`] (the ignored test).
//!
//! The fast test needs no native library. The measured sweep is ignored because
//! it signs a million-message batch; run it with
//! `cargo test -p climb-bench --test size_conformance -- --ignored --nocapture`.

use climb_analytical::{
    auth_path_len, predicted_condensed_len, predicted_ladder_len, rungs, CONDENSED_FIXED_LEN,
};

/// Bytes in one `MTL_INDEX` on the wire (`typedef uint64_t MTL_INDEX`).
const INDEX: usize = 8;
/// Bytes in one hash (`OQS_SHA2_SHA256` output/security parameter, 128-bit).
const HASH: usize = 16;

/// Unsigned integer decomposition into descending powers of two.
///
/// Written as repeated subtraction of the largest power of two that still fits,
/// which is a different algorithm from the set-bit walk in `climb-analytical`.
fn independent_rungs(leaf_count: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut left = 0u64;
    let mut remaining = leaf_count;
    while remaining > 0 {
        let exponent = 63 - remaining.leading_zeros();
        let width = 1u64 << exponent;
        let right = left + width - 1;
        out.push((left, right));
        left = right + 1;
        remaining -= width;
    }
    out
}

/// Sibling count for one leaf, from the independent rung decomposition.
fn independent_auth_path_len(leaf_count: u64, leaf_index: u64) -> Option<usize> {
    if leaf_index >= leaf_count {
        return None;
    }
    independent_rungs(leaf_count)
        .into_iter()
        .find(|&(left, right)| left <= leaf_index && leaf_index <= right)
        .map(|(left, right)| {
            let width = right - left + 1;
            debug_assert!(width.is_power_of_two());
            width.trailing_zeros() as usize
        })
}

/// Fixed prefix of a condensed signature, written out field by field.
fn independent_condensed_prefix() -> usize {
    // SID (2*HASH) | Flags (2) | Randomizer (HASH) | LeafIndex (INDEX)
    // | RungLeft (INDEX) | RungRight (INDEX) | SiblingCount (2)
    2 * HASH + 2 + HASH + 3 * INDEX + 2
}

fn independent_condensed_len(leaf_count: u64, leaf_index: u64) -> Option<usize> {
    independent_auth_path_len(leaf_count, leaf_index)
        .map(|siblings| independent_condensed_prefix() + siblings * HASH)
}

fn independent_unsigned_ladder_len(leaf_count: u64) -> usize {
    // Flags (2) | SID (2*HASH) | RungCount (2) | one (left,right,hash) per rung
    let rung_count = independent_rungs(leaf_count).len();
    2 + 2 * HASH + 2 + rung_count * (2 * INDEX + HASH)
}

fn independent_signed_ladder_len(leaf_count: u64, signature_len: usize) -> usize {
    // Unsigned ladder | SignatureLength (4, big-endian) | Signature
    independent_unsigned_ladder_len(leaf_count) + 4 + signature_len
}

/// Leaves checked exhaustively for small trees, plus representative positions
/// for large ones.
fn sampled_leaves(leaf_count: u64) -> Vec<u64> {
    if leaf_count <= 256 {
        return (0..leaf_count).collect();
    }
    let mut leaves = vec![0, 1, leaf_count / 2, leaf_count - 2, leaf_count - 1];
    // Rung boundaries are where the auth-path length changes.
    for (left, right) in independent_rungs(leaf_count) {
        leaves.push(left);
        leaves.push(right);
        if right + 1 < leaf_count {
            leaves.push(right + 1);
        }
    }
    leaves.sort_unstable();
    leaves.dedup();
    leaves.retain(|&leaf| leaf < leaf_count);
    leaves
}

#[test]
fn independent_formulas_agree_with_the_analytical_model() {
    let mut checked = 0usize;
    for leaf_count in 1u64..=4096 {
        let expected_rungs = independent_rungs(leaf_count);
        let model_rungs = rungs(leaf_count);
        assert_eq!(
            expected_rungs.len(),
            model_rungs.len(),
            "rung count for N={leaf_count}"
        );
        for ((left, right), model) in expected_rungs.iter().zip(model_rungs.iter()) {
            assert_eq!(
                (*left, *right),
                (model.left, model.right),
                "rung range for N={leaf_count}"
            );
        }

        assert_eq!(
            independent_unsigned_ladder_len(leaf_count),
            predicted_ladder_len(leaf_count),
            "unsigned ladder length for N={leaf_count}"
        );

        for leaf in sampled_leaves(leaf_count) {
            checked += 1;
            assert_eq!(
                independent_auth_path_len(leaf_count, leaf),
                auth_path_len(leaf_count, leaf),
                "auth-path length for N={leaf_count} leaf={leaf}"
            );
            assert_eq!(
                independent_condensed_len(leaf_count, leaf),
                predicted_condensed_len(leaf_count, leaf),
                "condensed length for N={leaf_count} leaf={leaf}"
            );
        }
    }
    assert!(checked > 80_000, "sampled only {checked} leaf positions");
}

#[test]
fn the_analytical_prefix_constant_matches_the_field_layout() {
    assert_eq!(
        CONDENSED_FIXED_LEN,
        independent_condensed_prefix(),
        "the model's fixed prefix must equal the sum of the serialized fields"
    );
    assert_eq!(independent_condensed_prefix(), 3 * HASH + 28);
}

#[test]
fn signed_ladder_adds_exactly_a_length_field_and_the_signature() {
    for leaf_count in [0u64, 1, 2, 3, 5, 13, 64, 1000, 1_000_000] {
        let signature_len = 7856;
        assert_eq!(
            independent_signed_ladder_len(leaf_count, signature_len)
                - independent_unsigned_ladder_len(leaf_count),
            4 + signature_len,
            "signed-ladder overhead for N={leaf_count}"
        );
    }
}

// ---------------------------------------------------------------------------
// Measured sweep: real libMTL bytes vs both formulas
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
#[test]
#[ignore = "signs a million-message batch through libMTL; run explicitly with --ignored --nocapture"]
fn measured_ladder_bytes_agree_with_both_formulas() {
    use climb_bench::{measure_ladder_rows, DEFAULT_LADDER_SEED};

    let mut sizes: Vec<usize> = (1..=64).collect();
    sizes.extend([100, 1000, 10_000, 1_000_000]);

    let rows = measure_ladder_rows(DEFAULT_LADDER_SEED, &sizes).expect("native measurement");
    assert_eq!(rows.len(), sizes.len());

    for row in &rows {
        let n = row.leaf_count as u64;
        let independent = independent_rungs(n);
        let model = rungs(n);

        assert_eq!(
            row.rung_count,
            independent.len(),
            "measured rung count N={n}"
        );
        assert_eq!(row.rung_count, model.len(), "model rung count N={n}");
        assert_eq!(
            row.rungs
                .iter()
                .map(|r| (r.left, r.right))
                .collect::<Vec<_>>(),
            independent,
            "measured rung ranges N={n}"
        );
        assert_eq!(
            row.ladder_buffer_len,
            independent_unsigned_ladder_len(n),
            "measured unsigned ladder N={n}"
        );
        assert_eq!(
            row.signed_ladder_len,
            independent_signed_ladder_len(n, row.signature_len as usize),
            "measured signed ladder N={n}"
        );

        let predicted: Vec<usize> = (0..n)
            .map(|leaf| independent_condensed_len(n, leaf).expect("measured leaf is in range"))
            .collect();
        let min = *predicted.iter().min().expect("non-empty batch");
        let max = *predicted.iter().max().expect("non-empty batch");
        let total: u64 = predicted.iter().map(|&len| len as u64).sum();
        let mean = total as f64 / predicted.len() as f64;

        assert_eq!(row.condensed.min, min, "measured condensed min N={n}");
        assert_eq!(row.condensed.max, max, "measured condensed max N={n}");
        assert_eq!(row.condensed.total, total, "measured condensed total N={n}");
        assert!(
            (row.condensed.mean - mean).abs() < 1e-9,
            "measured condensed mean N={n}: {} vs {mean}",
            row.condensed.mean
        );
        assert_eq!(
            row.condensed.uniform,
            min == max,
            "measured condensed uniformity N={n}"
        );

        println!(
            "N={n:<7} rungs={:<10} unsigned={:<7} signed={:<7} condensed={}..{:<4} total={:<9} mean={:.2}",
            row.rung_count,
            row.ladder_buffer_len,
            row.signed_ladder_len,
            row.condensed.min,
            row.condensed.max,
            row.condensed.total,
            row.condensed.mean,
        );
    }

    println!(
        "agreement: {} measured sizes matched the independent formulas and the analytical model",
        rows.len()
    );
}
