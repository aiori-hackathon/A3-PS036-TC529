//! Typed errors for the `climb-dns` crate.
//!
//! [`DnsError`] covers the two things that can go wrong during synthetic DNS
//! message generation and encoding: hickory-proto's encoder rejecting input
//! (which for our fixed small shapes is impossible, but we handle it), and
//! our own invariants being violated by a code change (empty or oversized
//! messages).

use hickory_proto::ProtoError;

/// Errors from synthetic DNS generation / wire encoding.
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
}
