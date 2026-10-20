//! Deterministic synthetic DNS dataset generation.
//!
//! Provides [`DatasetSize`] (the predefined sizes CLIMB benchmarks)
//! and [`generate_dataset`], which produces a reproducible,
//! byte-identical dataset from a size and a seed.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::synthetic::SyntheticRecord;

/// Maximum message length in bytes.
///
/// Mirrors `climb_mtl::MAX_MESSAGE_LEN` (libMTL takes the message length as
/// a C `uint16_t`).  [`generate_dataset`] guarantees every produced message
/// respects this bound.
pub const MAX_MESSAGE_LEN: usize = u16::MAX as usize;

/// Predefined dataset sizes CLIMB benchmarks, plus the OneK
/// validation size.
///
/// | Variant | Messages | Represents |
/// |---|---|---|
/// | `Small` | 100 | Low-change-rate stub zone |
/// | `OneK` | 1,000 | Validation size: large enough to test linear extrapolation from Small, small enough to measure directly |
/// | `Medium` | 10,000 | Mid-size enterprise zone |
/// | `Large` | 1,000,000 | High-volume resolver cache / near-TLD scale |
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DatasetSize {
    /// 100 messages — a low-change-rate stub zone.
    Small = 100,
    /// 1,000 messages — validates linear extrapolation from Small without MTL.
    OneK = 1_000,
    /// 10,000 messages — a mid-size enterprise zone.
    Medium = 10_000,
    /// 1,000,000 messages — a high-volume resolver cache or near-TLD scale.
    Large = 1_000_000,
}

impl DatasetSize {
    /// The number of messages this size represents.
    #[must_use]
    pub const fn count(self) -> usize {
        self as usize
    }

    /// Alias for [`count`](Self::count).
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self as usize
    }
}

/// Generate a deterministic synthetic DNS dataset.
///
/// Each element is a serialized DNS message (response with one RRset of
/// 1..=4 records) produced by [`SyntheticRecord::generate`].  The RNG is a
/// [`ChaCha8Rng`] seeded **exclusively** from `seed` — no OS entropy is
/// consumed, so `generate_dataset(size, seed)` always returns the same byte
/// vector for the same `(size, seed)`.
///
/// # Panics
///
/// Panics only if hickory-proto fails to encode one of our fixed small shapes.
/// This is a genuine library regression and never a runtime error under
/// normal operation.
#[must_use]
pub fn generate_dataset(size: DatasetSize, seed: u64) -> Vec<Vec<u8>> {
    generate_dataset_count(size.count(), seed)
}

/// Generate a deterministic synthetic DNS dataset of exactly `count` messages.
///
/// Prefer this over [`generate_dataset`] for the ladder-shape investigation,
/// which needs leaf counts outside the predefined [`DatasetSize`] variants so
/// the Merkle tree can be exercised at awkward sizes (N = 5, 7, 13, 33, …).
///
/// The RNG is a [`ChaCha8Rng`] seeded **exclusively** from `seed`, and the
/// messages are the first `count` outputs of [`SyntheticRecord::generate`], so
/// `generate_dataset_count(n, seed)` is a prefix of
/// `generate_dataset_count(m, seed)` for `n <= m` and is byte-identical across
/// runs. `count == 0` yields an empty vector.
///
/// # Panics
///
/// Panics only if hickory-proto fails to encode one of our fixed small shapes.
/// This is a genuine library regression and never a runtime error under
/// normal operation.
#[must_use]
pub fn generate_dataset_count(count: usize, seed: u64) -> Vec<Vec<u8>> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut messages = Vec::with_capacity(count);
    for _ in 0..count {
        let record = SyntheticRecord::generate(&mut rng);
        // Infallible by construction: our fixed small record shapes always
        // encode correctly in hickory-proto.  A failure indicates a
        // hickory-proto regression, which we surface as a panic rather than
        // silently swallowing.
        let wire = record
            .to_wire()
            .expect("SyntheticRecord with fixed small shapes always encodes; failure indicates a hickory-proto regression");
        messages.push(wire);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_methods_match() {
        assert_eq!(DatasetSize::Small.count(), 100);
        assert_eq!(DatasetSize::Small.as_usize(), 100);
        assert_eq!(DatasetSize::OneK.count(), 1_000);
        assert_eq!(DatasetSize::OneK.as_usize(), 1_000);
        assert_eq!(DatasetSize::Medium.count(), 10_000);
        assert_eq!(DatasetSize::Large.count(), 1_000_000);
    }

    #[test]
    fn count_matches_generated_dataset_length() {
        for size in [DatasetSize::Small, DatasetSize::OneK, DatasetSize::Medium] {
            let dataset = generate_dataset(size, 0);
            assert_eq!(
                dataset.len(),
                size.count(),
                "generated dataset length must match DatasetSize::count()"
            );
        }
    }

    #[test]
    fn dataset_is_deterministic() {
        let d1 = generate_dataset(DatasetSize::Small, 42);
        let d2 = generate_dataset(DatasetSize::Small, 42);

        assert_eq!(d1.len(), d2.len());
        for (i, (a, b)) in d1.iter().zip(d2.iter()).enumerate() {
            assert_eq!(a, b, "messages at index {i} differ for same seed");
        }
    }

    #[test]
    fn different_seed_produces_different_dataset() {
        let d1 = generate_dataset(DatasetSize::Small, 1);
        let d2 = generate_dataset(DatasetSize::Small, 999);

        let any_different = d1.iter().zip(d2.iter()).any(|(a, b)| a != b);
        assert!(
            any_different,
            "different seeds must produce at least one differing message"
        );
    }

    #[test]
    fn generate_dataset_count_honours_exact_count() {
        for count in [0usize, 1, 2, 3, 5, 7, 13, 33, 256] {
            let dataset = generate_dataset_count(count, 42);
            assert_eq!(dataset.len(), count, "count={count}");
        }
    }

    #[test]
    fn generate_dataset_count_is_deterministic() {
        let a = generate_dataset_count(17, 7);
        let b = generate_dataset_count(17, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn generate_dataset_count_is_a_prefix_of_larger_samples() {
        let big = generate_dataset_count(64, 3);
        for n in [1usize, 2, 8, 16, 33, 64] {
            let small = generate_dataset_count(n, 3);
            assert_eq!(small, big[..n], "prefix mismatch at n={n}");
        }
    }

    #[test]
    fn generate_dataset_matches_generate_dataset_count() {
        for size in [DatasetSize::Small, DatasetSize::OneK] {
            assert_eq!(
                generate_dataset(size, 42),
                generate_dataset_count(size.count(), 42),
                "size={size:?}"
            );
        }
    }

    #[test]
    fn all_messages_within_bounds() {
        let dataset = generate_dataset(DatasetSize::Small, 7);

        for (i, msg) in dataset.iter().enumerate() {
            assert!(!msg.is_empty(), "message at index {i} is empty");
            assert!(
                msg.len() <= MAX_MESSAGE_LEN,
                "message at index {i} has len {} > MAX_MESSAGE_LEN={MAX_MESSAGE_LEN}",
                msg.len()
            );
        }
    }
}
