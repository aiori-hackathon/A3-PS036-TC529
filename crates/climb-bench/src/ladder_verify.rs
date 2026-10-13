//! Measured MTL ladder shape for arbitrary leaf counts.
//!
//! This module signs a deterministic dataset of exactly `N` messages, then
//! parses the *real* libMTL signed-ladder and condensed-signature bytes back
//! into their structural fields (rung ranges and the distribution of condensed
//! lengths). It is the measured counterpart to `climb_analytical::ladder`'s
//! closed-form model: the unit tests here feed the measured rows back through
//! the model and require exact agreement.
//!
//! # Why parse bytes instead of trusting an API
//!
//! libMTL's public API exposes the ladder and condensed signatures as opaque
//! byte buffers. The interesting structure — how many rungs, which leaf range
//! each covers, how many siblings a leaf's path has — is only visible by
//! decoding the serialized form. The layouts are pinned in the vendored source
//! (see `climb-analytical::ladder`'s table) and re-stated here so the parse can
//! be reviewed against them.
//!
//! # Errors are values
//!
//! Everything fallible returns [`LadderVerifyError`] rather than panicking:
//! signing can fail, and untrusted-but-parsed byte offsets are bounds-checked.

use climb_analytical::{HASH_SIZE, MTL_INDEX_LEN};
use climb_dns::generate_dataset_count;
use climb_mtl::{MtlError, MtlKeyPair};
use serde::{Deserialize, Serialize};

use crate::wire_size::CondensedStats;

/// Default dataset seed for ladder measurement.
///
/// Shares the signing-benchmark seed constant so the ladder shape is measured
/// over the same deterministic messages as every other wire-size number.
pub const DEFAULT_LADDER_SEED: u64 = crate::wire_size::DEFAULT_WIRE_SEED;

/// Leaf counts measured by default: the interesting tree shapes for small `N`
/// (odd sizes, exact powers of two, powers of two ± 1) plus a mid-size check.
pub const DEFAULT_LADDER_SIZES: &[usize] =
    &[1, 2, 3, 4, 5, 6, 7, 8, 9, 13, 16, 31, 32, 33, 64, 100, 1000];

/// One rung of the measured ladder: an inclusive range of leaf indices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderRung {
    /// First (inclusive) leaf index covered by the rung.
    pub left: u64,
    /// Last (inclusive) leaf index covered by the rung.
    pub right: u64,
}

/// Measured ladder shape and wire sizes for one leaf count.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LadderRow {
    /// Number of leaves (messages) in the batch.
    pub leaf_count: usize,
    /// Number of rungs libMTL emitted.
    pub rung_count: usize,
    /// Rung ranges, left to right.
    pub rungs: Vec<LadderRung>,
    /// Unsigned ladder-buffer length parsed from the signed ladder, in bytes.
    pub ladder_buffer_len: usize,
    /// Total signed-ladder length, in bytes.
    pub signed_ladder_len: usize,
    /// Length of the embedded underlying signature, in bytes.
    pub signature_len: u32,
    /// Statistics over every leaf's condensed-signature length.
    pub condensed: CondensedStats,
    /// Smallest full-signature length (`condensed_len + signed_ladder_len`).
    pub full_len_min: usize,
    /// Largest full-signature length (`condensed_len + signed_ladder_len`).
    pub full_len_max: usize,
    /// Underlying signatures paid with MTL amortisation: one, for the ladder.
    pub with_mtl_signatures: usize,
    /// Underlying signatures paid without MTL: one per message.
    pub without_mtl_signatures: usize,
}

/// Errors from [`measure_ladder_rows`].
#[derive(Debug, thiserror::Error)]
pub enum LadderVerifyError {
    /// Signing a batch failed inside libMTL.
    #[error("MTL signing failed: {0}")]
    Mtl(#[from] MtlError),
    /// A requested leaf count was zero; libMTL cannot sign an empty batch.
    #[error("leaf count must be at least 1 (got 0)")]
    ZeroLeafCount,
    /// The signed-ladder bytes did not match the pinned layout.
    #[error("malformed signed ladder for N={leaf_count}: {reason}")]
    MalformedLadder {
        /// The leaf count whose ladder failed to parse.
        leaf_count: usize,
        /// What was wrong.
        reason: String,
    },
    /// A condensed signature did not match the pinned layout.
    #[error("malformed condensed signature for N={leaf_count} leaf={leaf_index}: {reason}")]
    MalformedCondensed {
        /// The leaf count whose condensed signature failed to parse.
        leaf_count: usize,
        /// The input position of the offending message.
        leaf_index: usize,
        /// What was wrong.
        reason: String,
    },
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let slice: [u8; 2] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u16::from_be_bytes(slice))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice: [u8; 4] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(slice))
}

fn read_u64_be(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let slice: [u8; 8] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u64::from_be_bytes(slice))
}

/// Structural fields decoded from a signed ladder.
struct ParsedLadder {
    rungs: Vec<LadderRung>,
    ladder_buffer_len: usize,
    signature_len: u32,
}

/// Decode the signed ladder produced by libMTL for `leaf_count` leaves.
fn parse_signed_ladder(bytes: &[u8], leaf_count: usize) -> Result<ParsedLadder, LadderVerifyError> {
    let malformed = |reason: String| LadderVerifyError::MalformedLadder { leaf_count, reason };

    // Flags (2) | SID (2*HASH_SIZE) | RungCount (2) | rungs… | u32 sig len | sig
    let header_len = 2 + 2 * HASH_SIZE + 2;
    if bytes.len() < header_len {
        return Err(malformed(format!(
            "ladder is {} bytes, shorter than the {header_len}-byte header",
            bytes.len()
        )));
    }
    let rung_count = read_u16_be(bytes, 2 + 2 * HASH_SIZE)
        .ok_or_else(|| malformed("truncated rung-count field".to_string()))?
        as usize;

    let rung_len = 2 * MTL_INDEX_LEN + HASH_SIZE;
    let ladder_buffer_len = header_len + rung_count * rung_len;
    let min_total = ladder_buffer_len + 4;
    if bytes.len() < min_total {
        return Err(malformed(format!(
            "rung count {rung_count} implies a {ladder_buffer_len}-byte buffer but the ladder is {} bytes",
            bytes.len()
        )));
    }

    let mut rungs = Vec::with_capacity(rung_count);
    for i in 0..rung_count {
        let base = header_len + i * rung_len;
        let left = read_u64_be(bytes, base)
            .ok_or_else(|| malformed(format!("truncated rung {i} left index")))?;
        let right = read_u64_be(bytes, base + MTL_INDEX_LEN)
            .ok_or_else(|| malformed(format!("truncated rung {i} right index")))?;
        rungs.push(LadderRung { left, right });
    }

    let signature_len = read_u32_be(bytes, ladder_buffer_len)
        .ok_or_else(|| malformed("truncated signature-length field".to_string()))?;
    let expected_total = ladder_buffer_len + 4 + signature_len as usize;
    if bytes.len() != expected_total {
        return Err(malformed(format!(
            "signed ladder is {} bytes; buffer {ladder_buffer_len} + 4 + signature {signature_len} = {expected_total}",
            bytes.len()
        )));
    }

    Ok(ParsedLadder {
        rungs,
        ladder_buffer_len,
        signature_len,
    })
}

/// Validate one condensed signature against the pinned layout.
///
/// Returns the serialized length in bytes. The per-leaf fields (leaf index,
/// rung range, sibling count) are read to confirm the layout is intact — the
/// index must match the input position and the rung range must contain it —
/// but they are not retained: a rung's leaf range and its sibling count are
/// both derivable from the ladder's rungs, so persisting them would duplicate
/// the rung array.
fn parse_condensed(
    bytes: &[u8],
    leaf_count: usize,
    leaf_index: usize,
) -> Result<usize, LadderVerifyError> {
    let malformed = |reason: String| LadderVerifyError::MalformedCondensed {
        leaf_count,
        leaf_index,
        reason,
    };

    // SID (2*HASH_SIZE) | Flags (2) | Randomizer (HASH_SIZE) | LeafIndex (8) |
    // RungLeft (8) | RungRight (8) | SiblingCount (2) | siblings…
    let prefix = 2 * HASH_SIZE + 2 + HASH_SIZE;
    let fixed_len = prefix + 3 * MTL_INDEX_LEN + 2;
    if bytes.len() < fixed_len {
        return Err(malformed(format!(
            "condensed signature is {} bytes, shorter than the {fixed_len}-byte fixed prefix",
            bytes.len()
        )));
    }

    let recorded_index =
        read_u64_be(bytes, prefix).ok_or_else(|| malformed("truncated leaf index".to_string()))?;
    let rung_left = read_u64_be(bytes, prefix + MTL_INDEX_LEN)
        .ok_or_else(|| malformed("truncated rung-left index".to_string()))?;
    let rung_right = read_u64_be(bytes, prefix + 2 * MTL_INDEX_LEN)
        .ok_or_else(|| malformed("truncated rung-right index".to_string()))?;
    let sibling_count = read_u16_be(bytes, prefix + 3 * MTL_INDEX_LEN)
        .ok_or_else(|| malformed("truncated sibling-count field".to_string()))?;

    if recorded_index != leaf_index as u64 {
        return Err(malformed(format!(
            "condensed signature at position {leaf_index} records leaf index {recorded_index}"
        )));
    }
    if !(rung_left..=rung_right).contains(&recorded_index) {
        return Err(malformed(format!(
            "leaf {recorded_index} is not inside its recorded rung [{rung_left}, {rung_right}]"
        )));
    }

    let expected_len = fixed_len + sibling_count as usize * HASH_SIZE;
    if bytes.len() != expected_len {
        return Err(malformed(format!(
            "condensed signature is {} bytes; {sibling_count} siblings imply {expected_len}",
            bytes.len()
        )));
    }

    Ok(bytes.len())
}

/// Sign `N` messages for each size and measure the resulting ladder shape.
///
/// One keypair is reused across every size; each [`sign_batch`] call builds a
/// fresh ladder from an empty node set, so sizes do not influence one another.
///
/// # Errors
///
/// * [`LadderVerifyError::ZeroLeafCount`] if any requested size is 0.
/// * [`LadderVerifyError::Mtl`] if libMTL key generation or signing fails.
/// * [`LadderVerifyError::MalformedLadder`] / [`LadderVerifyError::MalformedCondensed`]
///   if the returned bytes do not match the pinned layouts.
pub fn measure_ladder_rows(
    seed: u64,
    sizes: &[usize],
) -> Result<Vec<LadderRow>, LadderVerifyError> {
    if sizes.contains(&0) {
        return Err(LadderVerifyError::ZeroLeafCount);
    }
    if sizes.is_empty() {
        return Ok(Vec::new());
    }

    let keypair = MtlKeyPair::generate()?;
    let signer = keypair.signer();

    let mut rows = Vec::with_capacity(sizes.len());
    for &leaf_count in sizes {
        let dataset = generate_dataset_count(leaf_count, seed);
        let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
        let output = signer.sign_batch(&messages)?;

        let parsed_ladder = parse_signed_ladder(output.ladder().as_bytes(), leaf_count)?;
        let signed_ladder_len = output.ladder().as_bytes().len();

        let mut condensed_lens = Vec::with_capacity(leaf_count);
        for (index, message) in output.messages().iter().enumerate() {
            condensed_lens.push(parse_condensed(
                message.condensed().as_bytes(),
                leaf_count,
                index,
            )?);
        }

        let condensed = condensed_stats(&condensed_lens);
        let full_len_min = condensed.min + signed_ladder_len;
        let full_len_max = condensed.max + signed_ladder_len;

        rows.push(LadderRow {
            leaf_count,
            rung_count: parsed_ladder.rungs.len(),
            rungs: parsed_ladder.rungs,
            ladder_buffer_len: parsed_ladder.ladder_buffer_len,
            signed_ladder_len,
            signature_len: parsed_ladder.signature_len,
            condensed,
            full_len_min,
            full_len_max,
            with_mtl_signatures: 1,
            without_mtl_signatures: leaf_count,
        });
    }

    Ok(rows)
}

/// Min/max/mean/total/uniform over a non-empty slice of condensed lengths.
fn condensed_stats(lens: &[usize]) -> CondensedStats {
    debug_assert!(!lens.is_empty(), "a signed batch is never empty");
    let min = lens.iter().copied().min().unwrap_or(0);
    let max = lens.iter().copied().max().unwrap_or(0);
    let total: u64 = lens.iter().map(|&l| l as u64).sum();
    let mean = if lens.is_empty() {
        0.0
    } else {
        total as f64 / lens.len() as f64
    };
    CondensedStats {
        min,
        max,
        mean,
        total,
        uniform: min == max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use climb_analytical::{predicted_condensed_len, predicted_ladder_len, rungs};

    #[test]
    fn readers_are_bounds_checked() {
        let bytes = [0x01u8, 0x02, 0x03];
        assert_eq!(read_u16_be(&bytes, 0), Some(0x0102));
        assert_eq!(read_u16_be(&bytes, 2), None);
        assert_eq!(read_u32_be(&bytes, 0), None);
        assert_eq!(read_u64_be(&bytes, 0), None);
    }

    #[test]
    fn zero_leaf_count_is_rejected() {
        assert!(matches!(
            measure_ladder_rows(0x434C_494D_425F_5635, &[1, 0]),
            Err(LadderVerifyError::ZeroLeafCount)
        ));
    }

    #[test]
    fn empty_sizes_yields_no_rows() {
        assert_eq!(measure_ladder_rows(1, &[]).expect("no signing"), Vec::new());
    }

    #[test]
    #[ignore = "requires the native libMTL build; run via `cargo xtask ladder-verify`"]
    fn measured_rows_agree_with_model() {
        let rows = measure_ladder_rows(DEFAULT_LADDER_SEED, &[1, 2, 3, 5, 8, 13])
            .expect("native measurement");
        assert_eq!(rows.len(), 6);
        for row in &rows {
            assert_row_agrees_with_model(row);
        }
    }

    #[test]
    fn default_seed_matches_wire_seed() {
        assert_eq!(DEFAULT_LADDER_SEED, crate::wire_size::DEFAULT_WIRE_SEED);
    }

    /// Assert a measured row equals the closed-form model.
    ///
    /// The model predicts a condensed length for *every* leaf, so comparing the
    /// exact distribution (min, max, total, mean) against the measured one
    /// checks every leaf's path without persisting per-leaf records.
    fn assert_row_agrees_with_model(row: &LadderRow) {
        let n = row.leaf_count as u64;
        assert!(n >= 1, "a measured batch is never empty");

        let model_rungs = rungs(n);
        assert_eq!(row.rung_count, model_rungs.len(), "rung count for N={n}");
        assert_eq!(
            row.rungs,
            model_rungs
                .iter()
                .map(|r| LadderRung {
                    left: r.left,
                    right: r.right,
                })
                .collect::<Vec<_>>(),
            "rung ranges for N={n}"
        );

        assert_eq!(
            row.ladder_buffer_len,
            predicted_ladder_len(n),
            "ladder buffer length for N={n}"
        );
        assert_eq!(
            row.signed_ladder_len,
            predicted_ladder_len(n) + 4 + row.signature_len as usize,
            "signed ladder length for N={n}"
        );

        let predicted: Vec<usize> = (0..n)
            .map(|li| predicted_condensed_len(n, li).expect("leaf in range"))
            .collect();
        let expected_min = *predicted.iter().min().expect("a batch has leaves");
        let expected_max = *predicted.iter().max().expect("a batch has leaves");
        let expected_total: u64 = predicted.iter().map(|&l| l as u64).sum();
        let expected_mean = expected_total as f64 / predicted.len() as f64;

        assert_eq!(row.condensed.min, expected_min, "condensed min for N={n}");
        assert_eq!(row.condensed.max, expected_max, "condensed max for N={n}");
        assert_eq!(
            row.condensed.total, expected_total,
            "condensed total for N={n}"
        );
        assert!(
            (row.condensed.mean - expected_mean).abs() < 1e-9,
            "condensed mean for N={n}: measured {} vs model {expected_mean}",
            row.condensed.mean
        );
        assert_eq!(
            row.condensed.uniform,
            expected_min == expected_max,
            "condensed uniformity for N={n}"
        );

        assert_eq!(
            row.full_len_min,
            expected_min + row.signed_ladder_len,
            "full-signature min for N={n}"
        );
        assert_eq!(
            row.full_len_max,
            expected_max + row.signed_ladder_len,
            "full-signature max for N={n}"
        );
        assert_eq!(row.with_mtl_signatures, 1, "MTL signatures for N={n}");
        assert_eq!(
            row.without_mtl_signatures, row.leaf_count,
            "non-MTL signatures for N={n}"
        );
    }

    // Measured rows captured from a real run; every field is parsed out of
    // real libMTL bytes, not copied from a report.
    include!("ladder_verify_inline.rs");
}
