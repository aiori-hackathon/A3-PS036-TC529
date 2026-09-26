//! Deterministic synthetic DNS dataset generation for CLIMB benchmarks.
//!
//! This crate produces reproducible, byte-identical DNS messages (response
//! wire format, via `hickory-proto`) from a seeded PRNG.  Each message is a
//! single DNS RRset — one owner name, one record type, 1..=4 resource records
//! — with the record type varying across messages (A / AAAA / MX / TXT / NS).
//!
//! ## Example
//!
//! ```
//! use climb_dns::{DatasetSize, generate_dataset};
//!
//! let dataset = generate_dataset(DatasetSize::Small, 42);
//! assert_eq!(dataset.len(), 100);
//! ```

pub mod dataset;
pub mod error;
mod synthetic;

pub use dataset::{generate_dataset, DatasetSize, MAX_MESSAGE_LEN};
pub use error::DnsError;
pub use synthetic::SyntheticRecord;
