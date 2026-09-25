//! Typed errors for the safe MTL wrapper.

use thiserror::Error;

/// Errors returned by the safe MTL wrapper.
#[derive(Debug, Error)]
pub enum MtlError {
    /// The algorithm string is not one of the schemes compiled into libMTL.
    #[error("unknown or unsupported algorithm string: {0:?}")]
    UnknownAlgorithm(String),

    /// A native string argument contained a NUL byte and could not be passed to C.
    #[error("native string argument contained an interior NUL byte: {0:?}")]
    InteriorNul(String),

    /// libMTL reported an out-of-memory condition.
    #[error("libMTL could not allocate memory during {operation}")]
    OutOfMemory { operation: &'static str },

    /// libMTL reported that an output buffer was too small.
    #[error("libMTL reported an internal buffer shortage during {operation}")]
    BufferTooSmall { operation: &'static str },

    /// Signing failed inside libMTL.
    #[error("libMTL signing failed during {operation} (status {status})")]
    SignFailed {
        operation: &'static str,
        status: u32,
    },

    /// libMTL returned a status the wrapper does not classify.
    #[error("libMTL operation {operation} returned status {status}")]
    Native {
        operation: &'static str,
        status: u32,
    },

    /// A message was empty; libMTL rejects zero-length messages.
    #[error("message {index} is empty; libMTL rejects zero-length messages")]
    EmptyMessage { index: usize },

    /// A message exceeded the per-message length libMTL accepts.
    #[error("message {index} is {len} bytes, exceeding libMTL's {max}-byte per-message limit")]
    MessageTooLong {
        index: usize,
        len: usize,
        max: usize,
    },

    /// A batch contained no messages.
    #[error("a signing batch must contain at least one message")]
    EmptyBatch,

    /// A condensed signature was submitted for verification without its ladder.
    #[error("condensed signature verification requires the associated ladder")]
    LadderRequired,

    /// A signature index was requested that is not present in the batch output.
    #[error("signature index {index} is out of range for a batch of {count}")]
    IndexOutOfRange { index: usize, count: usize },

    /// A serialized key image could not be parsed.
    #[error("serialized key image is malformed: {reason}")]
    MalformedKeyImage { reason: &'static str },

    /// The series id length did not match `2 * sec_param` for the algorithm.
    #[error(
        "series id for {algorithm:?} is {actual} bytes, expected {expected} (2 x security parameter)"
    )]
    SeriesIdLength {
        algorithm: String,
        expected: usize,
        actual: usize,
    },
}
