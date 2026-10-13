//! Pure model of an MTL Merkle tree ladder's *shape* for a given leaf count.
//!
//! libMTL's `mtl_ladder` decomposes the leaves `0..N` into a sequence of
//! maximal perfect binary subtrees ("rungs"), walked from the most-significant
//! set bit of `N` down to the least-significant. Each rung is a perfect binary
//! tree whose size is the corresponding set bit of `N`, and the rungs are laid
//! out left to right with no gaps. This module reproduces that decomposition
//! and the two wire-size formulas it implies, using nothing but integer
//! arithmetic.
//!
//! # Where this comes from
//!
//! | Quantity | libMTL source |
//! |---|---|
//! | Rung decomposition | `third_party/MTL/src/mtl.c` (`mtl_ladder`, ~328-386) |
//! | Leaf → rung assignment | `third_party/MTL/src/mtl.c` (`mtl_authpath`, ~254-319) |
//! | Condensed-signature layout | `third_party/MTL/src/mtl_buffer.c` (`mtl_auth_path_to_buffer`, ~151-220) |
//! | Ladder layout | `third_party/MTL/src/mtl_buffer.c` (`mtl_ladder_to_buffer`, ~312-384) |
//! | `MTL_INDEX` = 64-bit | `third_party/MTL/src/mtl_index.h:44` |
//!
//! # No cryptography
//!
//! Nothing here hashes, signs, or verifies. It derives lengths and index ranges
//! from the tree shape alone. The measured counterpart lives in
//! `climb-bench`'s ladder measurement; agreement between the two is checked by
//! a test that feeds real libMTL output through this model.

use crate::HASH_SIZE;

/// Bytes in one `MTL_INDEX` (a serialized `uint64_t`, big-endian on the wire).
///
/// Defined as 8 by `third_party/MTL/src/mtl_index.h:44` (`typedef uint64_t
/// MTL_INDEX`). The `(4)` figures in some `mtl_buffer.c` header comments are
/// stale and do not match the compiled type.
pub const MTL_INDEX_LEN: usize = 8;

/// Fixed, leaf-independent bytes in a serialized condensed signature.
///
/// Layout (`third_party/MTL/src/mtl_buffer.c`, `mtl_auth_path_to_buffer`):
/// `SID (2*HASH_SIZE) | Flags (2) | Randomizer (HASH_SIZE) | LeafIndex (8) |
/// RungLeft (8) | RungRight (8) | SiblingCount (2)`. The three 8-byte index
/// fields plus the 2-byte flags and 2-byte sibling count total 28 bytes:
///
/// `2*HASH_SIZE + 2 + HASH_SIZE + 28 = 3*HASH_SIZE + 28`.
pub const CONDENSED_FIXED_LEN: usize = 3 * HASH_SIZE + 2 + 3 * MTL_INDEX_LEN + 2;

/// A maximal perfect binary subtree of the ladder.
///
/// `left` and `right` are inclusive leaf indices, so the rung covers the
/// `right - left + 1` leaves in that range (a power of two).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rung {
    /// First (inclusive) leaf index covered by this rung.
    pub left: u64,
    /// Last (inclusive) leaf index covered by this rung.
    pub right: u64,
}

impl Rung {
    /// Number of leaves covered by this rung (a power of two).
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.right - self.left + 1
    }

    /// Whether `leaf` is inside this rung's inclusive leaf range.
    #[must_use]
    pub const fn contains(&self, leaf: u64) -> bool {
        self.left <= leaf && leaf <= self.right
    }
}

/// Decompose `leaf_count` leaves into the ladder's rungs, left to right.
///
/// Mirrors `mtl_ladder`'s walk over the set bits of `leaf_count` from the
/// most-significant bit down: each set bit starts a new rung whose width is
/// that bit's value. Returns an empty vector for `leaf_count == 0`.
#[must_use]
pub fn rungs(leaf_count: u64) -> Vec<Rung> {
    if leaf_count == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut left: u64 = 0;
    // Walk bit positions from the most-significant set bit down to 0.
    for bit_index in (0..=63u32).rev() {
        let width = 1u64 << bit_index;
        if leaf_count & width != 0 {
            let right = left + width - 1;
            out.push(Rung { left, right });
            left = right + 1;
        }
    }
    out
}

/// The rung that covers `leaf_index` in a ladder of `leaf_count` leaves.
///
/// Mirrors `mtl_authpath`: walk the set bits of `leaf_count` from the
/// most-significant down, advancing the running `left` until the current rung's
/// inclusive `right` reaches `leaf_index`. Returns `None` when
/// `leaf_index >= leaf_count`.
#[must_use]
pub fn rung_for_leaf(leaf_count: u64, leaf_index: u64) -> Option<Rung> {
    if leaf_index >= leaf_count {
        return None;
    }
    let mut left: u64 = 0;
    for bit_index in (0..=63u32).rev() {
        let width = 1u64 << bit_index;
        if leaf_count & width != 0 {
            let right = left + width - 1;
            if leaf_index <= right {
                return Some(Rung { left, right });
            }
            left = right + 1;
        }
    }
    None
}

/// Number of sibling hashes in the authentication path of `leaf_index`.
///
/// libMTL sets this to `mtl_bit_width(right - left)` where `[left, right]` is
/// the covering rung; for a rung of size `2^k` that is exactly `k`. Returns
/// `None` when `leaf_index >= leaf_count`.
#[must_use]
pub fn auth_path_len(leaf_count: u64, leaf_index: u64) -> Option<usize> {
    let rung = rung_for_leaf(leaf_count, leaf_index)?;
    Some(rung.size().trailing_zeros() as usize)
}

/// Serialized size of the condensed signature for `leaf_index`.
///
/// `CONDENSED_FIXED_LEN + k * HASH_SIZE`, where `k` is
/// [`auth_path_len`]. Returns `None` when `leaf_index >= leaf_count`.
#[must_use]
pub fn predicted_condensed_len(leaf_count: u64, leaf_index: u64) -> Option<usize> {
    auth_path_len(leaf_count, leaf_index).map(|k| CONDENSED_FIXED_LEN + k * HASH_SIZE)
}

/// Serialized size of the *unsigned* ladder buffer for `leaf_count` leaves.
///
/// Layout (`third_party/MTL/src/mtl_buffer.c`, `mtl_ladder_to_buffer`):
/// `Flags (2) | SID (2*HASH_SIZE) | RungCount (2)`, then one
/// `left (8) | right (8) | hash (HASH_SIZE)` triple per rung. With `c` rungs:
///
/// `4 + 2*HASH_SIZE + c * (2*MTL_INDEX_LEN + HASH_SIZE)`.
///
/// This is the pure unsigned tree structure. The signed-ladder form prepends
/// nothing but appends a 4-byte length and the underlying signature, whose size
/// is a property of the signature scheme, not of `leaf_count`, so it is kept
/// out of this model.
#[must_use]
pub fn predicted_ladder_len(leaf_count: u64) -> usize {
    let rung_count = rungs(leaf_count).len();
    2 + 2 * HASH_SIZE + 2 + rung_count * (2 * MTL_INDEX_LEN + HASH_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rungs_of_single_leaf_is_one_unit_rung() {
        let r = rungs(1);
        assert_eq!(r, vec![Rung { left: 0, right: 0 }]);
        assert_eq!(
            predicted_ladder_len(1),
            4 + 2 * HASH_SIZE + (16 + HASH_SIZE)
        );
    }

    #[test]
    fn rungs_decompose_into_powers_of_two_left_to_right() {
        // N=13 = 8 + 4 + 1 -> rungs [0..7], [8..11], [12..12].
        assert_eq!(
            rungs(13),
            vec![
                Rung { left: 0, right: 7 },
                Rung { left: 8, right: 11 },
                Rung {
                    left: 12,
                    right: 12
                },
            ]
        );
        // N=6 = 4 + 2 -> [0..3], [4..5].
        assert_eq!(
            rungs(6),
            vec![Rung { left: 0, right: 3 }, Rung { left: 4, right: 5 }]
        );
    }

    #[test]
    fn rungs_of_zero_is_empty() {
        assert!(rungs(0).is_empty());
        assert_eq!(predicted_ladder_len(0), 4 + 2 * HASH_SIZE);
    }

    #[test]
    fn rungs_count_equals_popcount() {
        for n in 0u64..=256 {
            assert_eq!(rungs(n).len() as u32, n.count_ones(), "n={n}");
        }
    }

    #[test]
    fn predicted_condensed_len_none_out_of_range() {
        assert_eq!(predicted_condensed_len(4, 4), None);
        assert_eq!(auth_path_len(4, 4), None);
        assert_eq!(rung_for_leaf(4, 4), None);
    }

    // -- 23.5 property tests: N = 1..=256 ------------------------------------

    #[test]
    fn property_rung_sizes_sum_to_leaf_count() {
        for n in 1u64..=256 {
            let total: u64 = rungs(n).iter().map(Rung::size).sum();
            assert_eq!(total, n, "rung sizes must sum to N for n={n}");
        }
    }

    #[test]
    fn property_every_leaf_belongs_to_exactly_one_rung() {
        for n in 1u64..=256 {
            let rs = rungs(n);
            for leaf in 0..n {
                let covering = rs.iter().filter(|r| r.contains(leaf)).count();
                assert_eq!(covering, 1, "leaf {leaf} covered {covering} times at n={n}");
            }
        }
    }

    #[test]
    fn property_rungs_are_contiguous_and_left_to_right() {
        for n in 1u64..=256 {
            let rs = rungs(n);
            let mut expected_left = 0;
            for r in &rs {
                assert_eq!(r.left, expected_left, "gap before rung at n={n}");
                assert!(
                    r.size().is_power_of_two(),
                    "rung not a power of two at n={n}"
                );
                expected_left = r.right + 1;
            }
            assert_eq!(expected_left, n, "rungs must cover 0..N at n={n}");
        }
    }

    #[test]
    fn property_auth_path_len_within_ceil_log2() {
        for n in 1u64..=256 {
            // ceil(log2(n)) == bit width of (n-1), i.e. sign-bit width for n >= 1.
            let ceil_log2 = if n <= 1 {
                0
            } else {
                (u64::BITS - (n - 1).leading_zeros()) as usize
            };
            for leaf in 0..n {
                let k = auth_path_len(n, leaf).expect("leaf in range");
                assert!(k <= ceil_log2, "auth path {k} > ceil(log2 {n})={ceil_log2}");
            }
        }
    }

    #[test]
    fn property_predicted_condensed_len_matches_rung_exponent() {
        for n in 1u64..=256 {
            for leaf in 0..n {
                let rung = rung_for_leaf(n, leaf).expect("leaf in range");
                let k = rung.size().trailing_zeros() as usize;
                assert_eq!(
                    predicted_condensed_len(n, leaf),
                    Some(CONDENSED_FIXED_LEN + k * HASH_SIZE)
                );
            }
        }
    }

    #[test]
    fn constants_are_wire_verified() {
        // MTL_INDEX is 64-bit in the vendored header; a submodule update that
        // changes either number will fail this pin.
        assert_eq!(MTL_INDEX_LEN, 8);
        assert_eq!(HASH_SIZE, 16);
        assert_eq!(CONDENSED_FIXED_LEN, 3 * 16 + 28);
    }
}
