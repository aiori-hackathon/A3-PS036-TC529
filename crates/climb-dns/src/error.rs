//! Typed errors for the `climb-dns` crate.
//!
//! [`DnsError`] covers the two things that can go wrong during synthetic DNS
//! message generation and encoding: hickory-proto's encoder rejecting input
//! (which for our fixed small shapes is impossible, but we handle it), and
//! our own invariants being violated by a code change (empty or oversized
//! messages).

use hickory_proto::ProtoError;

/// Errors from synthetic DNS generation / wire encoding and parsing.
#[derive(Debug, thiserror::Error)]
pub enum DnsError {
    /// hickory-proto failed to encode a constructed [`Message`].
    ///
    /// [`Message`]: hickory_proto::op::Message
    #[error("failed to encode DNS message: {0}")]
    Encode(#[from] ProtoError),

    /// An encoded message was empty — always indicates a construction bug,
    /// because every message we build has at least one query + one answer.
    #[error("generated DNS message is empty")]
    EmptyMessage,

    /// An encoded message exceeded libMTL's `uint16_t` length limit.
    /// For the small record shapes we generate this is also a construction bug.
    #[error("generated message length {len} exceeds MAX_MESSAGE_LEN ({max})")]
    MessageTooLong { len: usize, max: usize },

    /// Truncated DNS message at offset
    #[error("truncated DNS message at offset {offset} (need {need} bytes)")]
    Truncated { offset: usize, need: usize },

    /// A section count exceeds what the input could possibly contain.
    #[error(
        "DNS {section} section claims {count} entries, more than the input can hold (max {max})"
    )]
    SectionCount {
        section: &'static str,
        count: usize,
        max: usize,
    },

    /// Bytes remain after every declared section has been parsed.
    #[error("DNS message has {trailing} trailing byte(s) after offset {offset}")]
    TrailingBytes { offset: usize, trailing: usize },

    /// A dataset-digest document declared a schema this build does not know.
    #[error("dataset digest document uses unsupported schema {schema} (expected {expected})")]
    DigestSchema { schema: u32, expected: u32 },

    /// A dataset-digest document used an algorithm other than SHA-256.
    #[error("dataset digest document uses unsupported algorithm {algorithm:?}")]
    DigestAlgorithm { algorithm: String },

    /// A dataset-digest document's encoding description did not match the one
    /// implemented here, so its digests are not comparable.
    #[error("dataset digest document encoding does not match the implementation")]
    DigestEncoding,

    /// The document's seed differed from the seed the caller is checking.
    #[error("dataset digest document was generated with seed {found}, expected {expected}")]
    DigestSeed { found: u64, expected: u64 },

    /// A recomputed digest disagreed with the recorded one.
    #[error(
        "dataset digest mismatch for {size} ({count} messages, seed {seed}): recorded {recorded}, recomputed {recomputed}"
    )]
    DigestMismatch {
        size: String,
        count: usize,
        seed: u64,
        recorded: String,
        recomputed: String,
    },

    /// Two entries claimed the same dataset size.
    #[error("dataset digest document lists {size} more than once")]
    DuplicateDigestSize { size: String },
}
