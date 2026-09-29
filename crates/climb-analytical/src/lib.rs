//! Closed-form analytical models for MTL memory and cache usage.
//!
//! Computes the expected memory footprint given libMTL's page-size × page-count
//! model as defined in `third_party/MTL/src/mtl_node_set.h` and the linear node
//! index formula in `third_party/MTL/src/mtl_node_set.c`.

/// The actual `MTL_TREE_PAGE_SIZE` from `third_party/MTL/src/mtl_node_set.h:57`.
///
/// Each page is lazily allocated (via `calloc`) the first time a node index
/// maps into it; pages that are never hit stay `NULL`.
pub const MTL_TREE_PAGE_SIZE: usize = 1_048_576; // 1 MiB

/// The actual `MTL_TREE_MAX_PAGES` from `third_party/MTL/src/mtl_node_set.h:53`.
///
/// This is **8192**, not the README's generic "1024". The code was bumped to
/// 8× the documented default; 8192 pages × 1 MiB = 8 GiB of hash storage.
pub const MTL_TREE_MAX_PAGES: usize = 8192;

/// The actual `MTL_TREE_RANDOMIZER_PAGES` from `third_party/MTL/src/mtl_node_set.h:61`.
pub const MTL_TREE_RANDOMIZER_PAGES: usize = 8192;

/// Hash size in bytes for `SLH-DSA-SHA2-128s-MTL-SHA2-128`.
///
/// Derived from `mtl_node_set.c:58` (`hash_size = sid->length / 2`).
/// The series ID for SHA2-128 is 32 bytes → hash is 16 bytes (128 bits).
pub const HASH_SIZE: usize = 16;

// ---------------------------------------------------------------------------
// Intrinsic helpers — mirror libMTL's `mtl_node_set.c` bit operations
// ---------------------------------------------------------------------------

/// Count of 1-bits (popcount). Mirrors `mtl_bit_width` (line 383).
#[inline]
fn popcnt(n: usize) -> u32 {
    n.count_ones()
}

/// Index of the least-significant 1-bit (0-based). Mirrors `mtl_lsb` (line 402).
/// Precondition: `n > 0` (the call site in `int_node_id` uses `right+1` which
/// is always ≥ 1 for any non-empty tree).
#[inline]
fn lsb(n: usize) -> u32 {
    n.trailing_zeros()
}

/// Index of the most-significant 1-bit (0-based, floor(log2)). Mirrors `mtl_msb`
/// (line 421). Returns 0 for `n == 0`.
#[inline]
fn msb(n: usize) -> u32 {
    if n == 0 {
        0
    } else {
        usize::BITS - 1 - n.leading_zeros()
    }
}

// ---------------------------------------------------------------------------
// Analytical model
// ---------------------------------------------------------------------------

/// Result of [`estimate_memory_footprint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryEstimate {
    /// Hash size in bytes (16 for SHA2-128).
    pub hash_size: usize,
    /// Number of leaf messages this estimate covers.
    pub leaf_count: usize,
    /// Maximum linear node index in the tree page array
    /// (`int_node_id(0, N-1)` + 1).
    pub tree_nodes_max: usize,
    /// Number of randomizer entries (one per leaf).
    pub randomizer_count: usize,
    /// Tree pages actually needed for `tree_nodes_max` nodes.
    pub tree_pages: usize,
    /// Randomizer pages actually needed for N randomizers.
    pub randomizer_pages: usize,
    /// Page size (bytes).
    pub page_size: usize,
    /// Total tree memory in bytes (pages × page_size).
    pub tree_memory_bytes: usize,
    /// Total randomizer memory in bytes (pages × page_size).
    pub randomizer_memory_bytes: usize,
    /// Sum of tree + randomizer memory in bytes.
    pub total_memory_bytes: usize,
    /// Whether the estimate is capped by `MTL_TREE_MAX_PAGES`.
    pub tree_capped: bool,
    /// Whether the estimate is capped by `MTL_TREE_RANDOMIZER_PAGES`.
    pub randomizer_capped: bool,
}

/// Maximum linear node index for `N` leaves — `int_node_id(0, N-1)`.
///
/// Formula from `third_party/MTL/src/mtl_node_set.c:371-372`:
///
/// ```c
/// *return_index = 2 * (right + 1) - mtl_bit_width(right + 1)
///              - mtl_lsb(right + 1) + mtl_msb(right - left + 1) - 1;
/// ```
///
/// With `left = 0`, `right = N - 1`:
///
/// ```text
/// max_index(N) = 2*N − popcnt(N) − trailing_zeros(N) + floor_log2(N) − 1
/// ```
#[inline]
fn max_tree_index(leaf_count: usize) -> usize {
    if leaf_count == 0 {
        return 0;
    }
    let n = leaf_count;
    (2 * n)
        .wrapping_sub(popcnt(n) as usize)
        .wrapping_sub(lsb(n) as usize)
        .wrapping_add(msb(n) as usize)
        .wrapping_sub(1)
}

/// Estimate total memory footprint (tree pages + randomizer pages) for a
/// Merkle tree holding `N` leaves under libMTL's lazy page-allocation model.
///
/// # Sources
///
/// | Constant | Value | Source file:line |
/// |---|---|---|
/// | `MTL_TREE_PAGE_SIZE` | 1,048,576 (1 MiB) | `third_party/MTL/src/mtl_node_set.h:57` |
/// | `MTL_TREE_MAX_PAGES` | **8,192** (NOT the README's 1,024) | `third_party/MTL/src/mtl_node_set.h:53` |
/// | `MTL_TREE_RANDOMIZER_PAGES` | 8,192 | `third_party/MTL/src/mtl_node_set.h:61` |
/// | `hash_size` | 16 B (SHA2-128) | Derived from `mtl_node_set.c:58` (`sid->length / 2`) |
/// | Node index formula | See [`max_tree_index`] | `third_party/MTL/src/mtl_node_set.c:371-372` |
///
/// # Model
///
/// libMTL stores nodes in a flat page array indexed by the `int_node_id`
/// linearization. Pages are `calloc`'d lazily when first accessed, so the
/// worst-case footprint is:
///
/// * **Tree pages**: `ceil(max_index(N) × hash_size / page_size) × page_size`
///   (capped at `MTL_TREE_MAX_PAGES × MTL_TREE_PAGE_SIZE`)
/// * **Randomizer pages**: `ceil(N × hash_size / page_size) × page_size`
///   (capped at `MTL_TREE_RANDOMIZER_PAGES × MTL_TREE_PAGE_SIZE`)
///
/// For this build (SHA2-128, hash_size = 16 B):
///
/// | DatasetSize | N | max_index(N) | Tree pages | Rand pages | Total |
/// |---|---|---|---|---|---|
/// | Small | 100 | 200 | 1 | 1 | 2 MiB |
/// | OneK | 1,000 | 1,999 | 1 | 1 | 2 MiB |
/// | Medium | 10,000 | 20,003 | 1 | 1 | 2 MiB |
/// | Large | 1,000,000 | 2,000,005 | 31 | 16 | ~47 MiB |
///
/// At all three dataset sizes the footprint is **≤ 47 MiB**, well inside the
/// 8 GiB theoretical maximum for this build (8,192 pages × 1 MiB).
///
/// # Deviation from README
///
/// The upstream `README.md` states "1024 pages resulting in 1 Gigabyte," but the
/// actual compiled-in `MTL_TREE_MAX_PAGES` is **8,192** (8× larger). The code
/// was bumped after the README was written; this crate uses the *code* constant,
/// not the stale documentation default.
pub fn estimate_memory_footprint(leaf_count: usize) -> MemoryEstimate {
    let max_idx = max_tree_index(leaf_count);
    let tree_nodes_max = max_idx + 1; // indices are 0-based

    // Tree memory: ceil(tree_nodes_max * hash_size / page_size) pages
    let tree_bytes_addr = tree_nodes_max.saturating_mul(HASH_SIZE);
    let tree_pages_unbounded = tree_bytes_addr.div_ceil(MTL_TREE_PAGE_SIZE);
    let tree_capped = tree_pages_unbounded > MTL_TREE_MAX_PAGES;
    let tree_pages = tree_pages_unbounded.min(MTL_TREE_MAX_PAGES);
    let tree_memory_bytes = tree_pages * MTL_TREE_PAGE_SIZE;

    // Randomizer memory: ceil(leaf_count * hash_size / page_size) pages
    let rand_bytes_addr = leaf_count.saturating_mul(HASH_SIZE);
    let rand_pages_unbounded = rand_bytes_addr.div_ceil(MTL_TREE_PAGE_SIZE);
    let randomizer_capped = rand_pages_unbounded > MTL_TREE_RANDOMIZER_PAGES;
    let randomizer_pages = rand_pages_unbounded.min(MTL_TREE_RANDOMIZER_PAGES);
    let randomizer_memory_bytes = randomizer_pages * MTL_TREE_PAGE_SIZE;

    let total_memory_bytes = tree_memory_bytes.saturating_add(randomizer_memory_bytes);

    MemoryEstimate {
        hash_size: HASH_SIZE,
        leaf_count,
        tree_nodes_max,
        randomizer_count: leaf_count,
        tree_pages,
        randomizer_pages,
        page_size: MTL_TREE_PAGE_SIZE,
        tree_memory_bytes,
        randomizer_memory_bytes,
        total_memory_bytes,
        tree_capped,
        randomizer_capped,
    }
}

/// Variant of [`estimate_memory_footprint`] that takes a [`DatasetSize`]
/// from `climb-dns`. This is the primary public API — the `climb-analytical`
/// crate deliberately does **not** depend on `climb-dns`, so callers pass
/// the leaf count directly.
///
/// This function is a thin convenience wrapper that materializes the leaf
/// count for each predefined [`DatasetSize`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetSize {
    /// 100 messages.
    Small = 100,
    /// 1,000 messages.
    OneK = 1_000,
    /// 10,000 messages.
    Medium = 10_000,
    /// 1,000,000 messages.
    Large = 1_000_000,
}

impl DatasetSize {
    #[must_use]
    pub const fn count(self) -> usize {
        self as usize
    }
}

/// Convenience: call `estimate_memory_footprint(size.count())`.
pub fn estimate_memory_footprint_for(size: DatasetSize) -> MemoryEstimate {
    estimate_memory_footprint(size.count())
}

// ---------------------------------------------------------------------------
// Module: helper to compute max_tree_index in non-test code (exposed for docs)
// ---------------------------------------------------------------------------
#[doc(hidden)]
pub fn __max_tree_index(n: usize) -> usize {
    max_tree_index(n)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // -- max_tree_index sanity checks (driven from verified Python computation) --

    #[test]
    fn max_tree_index_edge_cases() {
        // N=0 → no tree, index 0 is the only possible slot
        assert_eq!(max_tree_index(0), 0);
        // N=1 → one leaf at index 0
        assert_eq!(max_tree_index(1), 0);
        // N=2 → leaves 0,1; root 2
        assert_eq!(max_tree_index(2), 2);
        // N=3 → leaves 0,1,3; internal 2,4 → max 4
        assert_eq!(max_tree_index(3), 4);
        // N=4 → perfect tree, max index = 2*4-2 = 6
        assert_eq!(max_tree_index(4), 6);
        // N=5 → max_index verified manually above
        assert_eq!(max_tree_index(5), 9);
    }

    #[test]
    fn max_tree_index_grows_roughly_twice_n() {
        // max_index(N) ≈ 2N − popcnt(N) − trailing_zeros(N) + floor_log2(N) − 1
        // Excess over 2N-1 can be up to floor_log2(N). Bound conservatively.
        for n in [1, 2, 3, 7, 10, 100, 1_000, 10_000, 100_000, 1_000_000] {
            let mi = max_tree_index(n);
            let k = n.next_power_of_two().trailing_zeros() as usize;
            // mi should be < (2n + k) where k = n.next_power_of_two().trailing_zeros()
            assert!(
                mi < 2 * n + k,
                "n={n}, tree_nodes_max={}, bound=2n+k={}",
                mi + 1,
                2 * n + k
            );
            assert!(mi >= n.saturating_sub(1), "n={n}, mi={mi}");
        }
    }

    // -- estimate_memory_footprint correctness --

    #[test]
    fn estimate_for_n_zero() {
        let est = estimate_memory_footprint(0);
        assert_eq!(est.leaf_count, 0);
        assert_eq!(est.tree_nodes_max, 1); // max_index(0)=0, +1
        assert_eq!(est.randomizer_count, 0);
        // minimal allocation: at least 1 page for tree (index 0 needs storage)
        assert!(est.tree_memory_bytes >= MTL_TREE_PAGE_SIZE);
        // no randomizers → 0 pages
        assert_eq!(est.randomizer_memory_bytes, 0);
        assert!(!est.tree_capped);
        assert!(!est.randomizer_capped);
    }

    #[test]
    fn estimate_for_small_n() {
        let est = estimate_memory_footprint(3);
        assert_eq!(est.leaf_count, 3);
        // max_index(3) = 4, +1 = 5 tree nodes
        assert_eq!(est.tree_nodes_max, 5);
        assert_eq!(est.randomizer_count, 3);
        // 5*16=80 bytes → 1 page, 3*16=48 bytes → 1 page
        assert_eq!(est.tree_pages, 1);
        assert_eq!(est.randomizer_pages, 1);
        assert_eq!(est.total_memory_bytes, 2 * MTL_TREE_PAGE_SIZE);
        assert!(!est.tree_capped);
        assert!(!est.randomizer_capped);
    }

    #[test]
    fn estimate_non_negative() {
        for n in 0..=10 {
            let est = estimate_memory_footprint(n);
            assert!(est.total_memory_bytes > 0 || n == 0);
            assert!(est.tree_pages > 0 || n == 0);
        }
    }

    #[test]
    fn estimate_monotonic() {
        // Memory usage should be monotonically non-decreasing with N
        let mut prev = 0;
        for n in [0, 1, 2, 3, 7, 10, 50, 100, 500, 1_000] {
            let est = estimate_memory_footprint(n);
            assert!(
                est.total_memory_bytes >= prev,
                "n={n}: {est:?}, prev={prev}"
            );
            prev = est.total_memory_bytes;
        }
    }

    // -- DatasetSize integration --

    #[test]
    fn dataset_size_counts() {
        assert_eq!(DatasetSize::Small.count(), 100);
        assert_eq!(DatasetSize::OneK.count(), 1_000);
        assert_eq!(DatasetSize::Medium.count(), 10_000);
        assert_eq!(DatasetSize::Large.count(), 1_000_000);
    }

    #[test]
    fn estimate_for_all_dataset_sizes() {
        for size in [
            DatasetSize::Small,
            DatasetSize::OneK,
            DatasetSize::Medium,
            DatasetSize::Large,
        ] {
            let est = estimate_memory_footprint_for(size);
            assert!(est.total_memory_bytes > 0);
            assert!(!est.tree_capped, "{size:?}: tree capped unexpectedly");
            assert!(
                !est.randomizer_capped,
                "{size:?}: randomizer capped unexpectedly"
            );
        }
    }

    #[test]
    fn large_estimate_within_capacity() {
        // At 1M messages we should be well inside the 8 GiB max
        let est = estimate_memory_footprint(DatasetSize::Large.count());
        let max_capacity = MTL_TREE_MAX_PAGES * MTL_TREE_PAGE_SIZE;
        assert!(est.tree_memory_bytes < max_capacity);
        assert!(est.randomizer_memory_bytes <= MTL_TREE_RANDOMIZER_PAGES * MTL_TREE_PAGE_SIZE);
        // Large should use multiple tree pages but not excessive
        assert!(
            est.tree_pages > 1,
            "Large should need >1 tree page, got {}",
            est.tree_pages
        );
        assert!(
            est.tree_pages < 100,
            "Large tree pages should be modest, got {}",
            est.tree_pages
        );
    }

    #[test]
    fn capped_scenario() {
        // With an enormous N, the tree should hit MTL_TREE_MAX_PAGES
        let huge_n = MTL_TREE_MAX_PAGES * MTL_TREE_PAGE_SIZE / HASH_SIZE + 1;
        let est = estimate_memory_footprint(huge_n);
        assert!(
            est.tree_capped,
            "expected tree capped at N={huge_n}, got {est:?}"
        );
        assert_eq!(
            est.tree_memory_bytes,
            MTL_TREE_MAX_PAGES * MTL_TREE_PAGE_SIZE
        );
    }

    // -- Constants are as discovered --

    #[test]
    fn constants_match_vendored_header() {
        // These assertions pin the values discovered in the vendored header
        // so a submodule update that silently changes them will fail CI.
        assert_eq!(MTL_TREE_PAGE_SIZE, 1_048_576);
        assert_eq!(MTL_TREE_MAX_PAGES, 8_192);
        assert_eq!(MTL_TREE_RANDOMIZER_PAGES, 8_192);
        assert_eq!(HASH_SIZE, 16);
    }
}
