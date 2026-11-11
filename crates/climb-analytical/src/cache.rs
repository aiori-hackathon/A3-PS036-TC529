#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

//! Analytical model of resolver cache usage.
//!
//! `resolver_cache_bytes(R, config)` predicts the bytes a resolver cache holds
//! after loading `R` distinct signed RRsets, from response wire sizes plus the
//! ladder bytes stored once per batch when a shared ladder is used. No
//! constant is fitted to a measurement: every input (mean response size,
//! ladder size) comes from a measurement artifact (the wire report, the DNS
//! capture anatomy, or the ladder-verification artifact). The only structural
//! choice modelled is that a shared ladder is stored once rather than once per
//! response.

use serde::{Deserialize, Serialize};

/// One resolver configuration whose responses populate the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheConfig {
    /// Plain responses: every response carries a full signature, nothing is
    /// shared between responses.
    Plain,
    /// MTL-condensed responses: every response carries a condensed proof and
    /// the signed ladder is shared across the whole batch, so its bytes enter
    /// the cache once.
    MtlCondensed,
}

impl CacheConfig {
    /// Human label used on surfaces that quote this configuration.
    pub fn label(self) -> &'static str {
        match self {
            CacheConfig::Plain => "plain",
            CacheConfig::MtlCondensed => "mtl-condensed",
        }
    }
}

/// The result of the analytical cache model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolverCacheEstimate {
    /// Number of distinct RRsets loaded into the cache.
    pub r: usize,
    /// Mean wire size of one response in this configuration, in bytes.
    pub mean_response_bytes: usize,
    /// Ladder bytes stored once per batch (zero for [`CacheConfig::Plain`]).
    pub shared_ladder_bytes: usize,
    /// Whether the shared ladder is counted once (`MtlCondensed`) or not
    /// (`Plain`).
    pub use_shared_ladder: bool,
    /// Predicted cache bytes: `R * mean_response_bytes` plus the shared ladder
    /// once when applicable.
    pub predicted_cache_bytes: u64,
}

/// Predict the resolver cache bytes for `R` distinct RRsets.
///
/// The prediction is `R * mean_response_bytes` plus `shared_ladder_bytes`
/// once when `use_shared_ladder` is true. No term is fitted to a measurement:
/// `mean_response_bytes` and `shared_ladder_bytes` are inputs the caller
/// obtains from measurement artifacts.
#[must_use]
pub fn resolver_cache_bytes(
    r: usize,
    mean_response_bytes: usize,
    shared_ladder_bytes: usize,
    use_shared_ladder: bool,
) -> ResolverCacheEstimate {
    let per_response = r.saturating_mul(mean_response_bytes);
    let ladder = if use_shared_ladder {
        shared_ladder_bytes
    } else {
        0
    };
    let predicted_cache_bytes = per_response.saturating_add(ladder) as u64;
    ResolverCacheEstimate {
        r,
        mean_response_bytes,
        shared_ladder_bytes,
        use_shared_ladder,
        predicted_cache_bytes,
    }
}

/// Build an estimate from wire-size measurements for one dataset size.
///
/// * Plain: every response carries a full signature, so the mean response size
///   is `without_mtl_total_bytes / message_count` and no ladder is shared.
/// * MTL-condensed: the batch total carries the ladder once plus one condensed
///   path per response, so the mean response size is
///   `(with_mtl_total_bytes - ladder_len_bytes) / message_count` and the
///   ladder is stored once.
///
/// Returns `None` when `message_count` is zero (avoiding a division by zero
/// that no caller should have to guard against).
pub fn resolver_cache_bytes_from_wire(
    r: usize,
    message_count: usize,
    config: CacheConfig,
    without_mtl_total_bytes: u64,
    with_mtl_total_bytes: u64,
    ladder_len_bytes: usize,
) -> Option<ResolverCacheEstimate> {
    if message_count == 0 {
        return None;
    }
    let mean_response_bytes = match config {
        CacheConfig::Plain => without_mtl_total_bytes.saturating_div(message_count as u64) as usize,
        CacheConfig::MtlCondensed => {
            let condensed_total = with_mtl_total_bytes.saturating_sub(ladder_len_bytes as u64);
            condensed_total.saturating_div(message_count as u64) as usize
        }
    };
    let shared_ladder_bytes = if matches!(config, CacheConfig::MtlCondensed) {
        ladder_len_bytes
    } else {
        0
    };
    Some(resolver_cache_bytes(
        r,
        mean_response_bytes,
        shared_ladder_bytes,
        matches!(config, CacheConfig::MtlCondensed),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_never_counts_the_ladder() {
        let est = resolver_cache_bytes(100, 24_438, 8_056, false);
        assert_eq!(est.r, 100);
        assert_eq!(est.mean_response_bytes, 24_438);
        assert_eq!(est.shared_ladder_bytes, 8_056);
        assert!(!est.use_shared_ladder);
        assert_eq!(est.predicted_cache_bytes, 100 * 24_438);
    }

    #[test]
    fn mtl_condensed_counts_the_shared_ladder_once() {
        let est = resolver_cache_bytes(100, 658, 8_056, true);
        assert_eq!(est.predicted_cache_bytes, 100 * 658 + 8_056);
        assert!(est.use_shared_ladder);
    }

    #[test]
    fn zero_r_yields_zero_bytes() {
        let est = resolver_cache_bytes(0, 24_438, 8_056, true);
        assert_eq!(est.predicted_cache_bytes, 8_056);
    }

    #[test]
    fn saturates_rather_than_overflows() {
        let est = resolver_cache_bytes(usize::MAX, usize::MAX, 0, false);
        assert_ne!(
            est.predicted_cache_bytes,
            (usize::MAX as u128).saturating_mul(usize::MAX as u128) as u64
        );
    }

    #[test]
    fn wire_based_mean_for_plain_is_total_divided_by_count() {
        let est = resolver_cache_bytes_from_wire(
            10_000,
            10_000,
            CacheConfig::Plain,
            83_295_296,
            2_743_352,
            8_056,
        )
        .expect("plain parses");
        assert_eq!(est.mean_response_bytes, 8_329);
        assert_eq!(est.predicted_cache_bytes, 10_000 * 8_329);
    }

    #[test]
    fn wire_based_mean_for_condensed_subtracts_the_ladder_once() {
        let est = resolver_cache_bytes_from_wire(
            10_000,
            10_000,
            CacheConfig::MtlCondensed,
            83_295_296,
            2_743_352,
            8_056,
        )
        .expect("condensed parses");
        // (2_743_352 - 8_056) / 10_000 = 273
        assert_eq!(est.mean_response_bytes, 273);
        assert_eq!(est.shared_ladder_bytes, 8_056);
        assert!(est.use_shared_ladder);
        assert_eq!(est.predicted_cache_bytes, 10_000 * 273 + 8_056);
    }

    #[test]
    fn zero_message_count_returns_none() {
        assert!(
            resolver_cache_bytes_from_wire(100, 0, CacheConfig::Plain, 100, 100, 10,).is_none()
        );
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(CacheConfig::Plain.label(), "plain");
        assert_eq!(CacheConfig::MtlCondensed.label(), "mtl-condensed");
    }
}
