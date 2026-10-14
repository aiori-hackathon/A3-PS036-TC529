//! Criterion output parsing and plain-text summary rendering.
//!
//! Reads Criterion's on-disk `benchmark.json` + `estimates.json` files,
//! parses benchmark group identifiers into semantic categories, and
//! produces a human-readable two-table summary (signing + verifying).

pub mod charts;
pub mod error;
pub mod html;
pub mod ladder;
pub mod memory;
pub mod record;
pub mod render;
pub mod results;
pub mod summary;

pub use error::ReportError;
pub use html::check_report;
pub use ladder::{
    LadderProvenance, LadderVerification, SubmoduleRef, LADDER_VERIFICATION_SCHEMA_VERSION,
};
pub use memory::{
    parse_memory_artifact, MemoryArtifact, MemoryParseError, MemoryReportJson, ProbeLine,
    SizeReport, ISOLATED_PROCESS_METHOD,
};
pub use record::{read_leaf, scan_criterion, CriterionRecord};
pub use render::{format_duration, render_text_summary};
pub use results::{
    consolidate_results, ClimbResults, ConsolidateOptions, DnsCapture, DnsCaptureFlags,
    DnsCaptureHost, DnsCaptureImage, DnsCaptureQuery, DnsCaptureSource, DnsPipelineHeadline,
};
pub use summary::{parse_group_id, summarize, BenchKind, MtlMode, Size, SummaryRow};

pub use climb_bench::{LadderRow, LadderRung};
