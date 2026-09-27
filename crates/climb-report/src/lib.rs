//! Criterion output parsing and plain-text summary rendering.
//!
//! Reads Criterion's on-disk `benchmark.json` + `estimates.json` files,
//! parses benchmark group identifiers into semantic categories, and
//! produces a human-readable two-table summary (signing + verifying).

pub mod error;
pub mod record;
pub mod render;
pub mod summary;

pub use error::ReportError;
pub use record::{read_leaf, scan_criterion, CriterionRecord};
pub use render::{format_duration, render_text_summary};
pub use summary::{parse_group_id, summarize, BenchKind, MtlMode, Size, SummaryRow};
