//! Typed errors for the `climb-report` crate.
//!
//! [`ReportError`] covers filesystem I/O, JSON deserialization failures,
//! missing or malformed Criterion output files, and unrecognized
//! benchmark group identifiers.

use std::path::PathBuf;

/// Errors that can occur while parsing Criterion's on-disk output.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// An I/O error while reading a file or directory.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Filesystem path where the error occurred.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to decode JSON from a Criterion output file.
    #[error("failed to decode JSON in {path}: {source}")]
    JsonDecode {
        /// Path to the file that could not be parsed.
        path: PathBuf,
        /// serde_json deserialization error.
        #[source]
        source: serde_json::Error,
    },

    /// A required file is missing from a Criterion leaf directory.
    #[error("missing file {file} in directory {dir}")]
    MissingFile {
        /// Name of the file that was expected but absent (e.g. "benchmark.json").
        file: &'static str,
        /// Directory that should have contained it.
        dir: PathBuf,
    },

    /// A Criterion JSON file decoded but contains malformed or unexpected data.
    #[error("malformed data in {path}: {reason}")]
    MalformedData {
        /// Path to the file with bad data.
        path: PathBuf,
        /// Human-readable description of what was wrong.
        reason: String,
    },

    /// A benchmark group_id did not match any known pattern.
    #[error("unrecognized benchmark group id: {id}")]
    UnknownGroupId {
        /// The group_id string that could not be parsed.
        id: String,
    },

    /// The Criterion output root directory does not exist.
    #[error("benchmark output root directory not found: {path}")]
    MissingRoot {
        /// The path that was expected to exist.
        path: PathBuf,
    },
}
