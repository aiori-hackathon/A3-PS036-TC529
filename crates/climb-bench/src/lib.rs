//! CLIMB benchmark suite — MTL vs non-MTL signing and verification benchmarks.
//!
//! # Modules
//!
//! - [`wire_size`] — permanent, reproducible wire-size measurement (replaces
//!   the throwaway probe that previously existed only as a markdown table).
//! - [`memory`] — direct RSS delta measurement of libMTL's memory footprint
//!   via `sysinfo`, separated from dataset/harness overhead.

pub mod memory;
pub mod wire_size;
pub use memory::{
    compare_to_analytical, measure_all_sizes, measure_rss_delta, measure_rss_delta_repeated,
    RssComparison, RssDeltaReport, RssDeltaRun, RssSizeReport,
};
pub use wire_size::{measure_wire_sizes, CondensedStats, WireSizeReport, DEFAULT_WIRE_SEED};
