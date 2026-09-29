//! CLIMB benchmark suite — MTL vs non-MTL signing and verification benchmarks.
//!
//! # Modules
//!
//! - [`wire_size`] — permanent, reproducible wire-size measurement (replaces
//!   the throwaway probe that previously existed only as a markdown table).

pub mod wire_size;
pub use wire_size::{measure_wire_sizes, CondensedStats, WireSizeReport, DEFAULT_WIRE_SEED};
