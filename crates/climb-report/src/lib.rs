//! Criterion output parsing and plain-text summary rendering.
//!
//! Reads Criterion's on-disk `benchmark.json` + `estimates.json` files,
//! parses benchmark group identifiers into semantic categories, and
//! produces a human-readable two-table summary (signing + verifying).

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod cache;
pub mod charts;
pub mod deck_facts;
pub mod dns_rtt;
pub mod error;
pub mod html;
pub mod ladder;
pub mod memory;
pub mod numbers;
pub mod record;
pub mod render;
pub mod replication;
pub mod results;
pub mod summary;

pub use cache::{
    parse_unbound_stats, CacheAnalyticalRow, CacheArtifact, CacheParseError, CacheResults,
    CacheRun, UnboundStats, CACHE_USE_CLASS, CACHE_USE_METHOD, CACHE_USE_SCHEMA_VERSION,
};
pub use dns_rtt::{
    DnsRttCell, DnsRttSection, DnsRttStatus, DNS_RTT_DELAYS_MS, DNS_RTT_FULL_SAMPLES,
    DNS_RTT_LABEL, DNS_RTT_SCHEMA_VERSION,
};
pub use error::ReportError;
pub use html::{check, check_report, claims, CheckReport, CheckStatus, ClaimCheck};
pub use ladder::{
    model_matches, LadderProvenance, LadderSection, LadderSizeRow, LadderSource,
    LadderVerification, SubmoduleRef, LADDER_VERIFICATION_SCHEMA_VERSION,
};
pub use memory::{
    parse_memory_artifact, MemoryArtifact, MemoryParseError, MemoryReportJson, ProbeLine,
    SizeReport, ISOLATED_PROCESS_METHOD,
};
pub use record::{read_leaf, scan_criterion, CriterionRecord};
pub use render::{format_duration, render_text_summary};
pub use replication::{
    compare_results, compared_section, pending_section, relative_difference_percent,
    ReplicationHost, ReplicationRatio, ReplicationSection, ReplicationStatus,
    REPLICATION_SCHEMA_VERSION, REPLICATION_TOLERANCE_PERCENT,
};
pub use results::{
    consolidate_results, ClimbResults, ConsolidateOptions, DnsAnatomyRecord, DnsAnatomySection,
    DnsCapture, DnsCaptureFlags, DnsCaptureHost, DnsCaptureImage, DnsCaptureQuery,
    DnsCaptureSource, DnsPipelineHeadline, PrimitivesFreq, PrimitivesSection, ReproductionMode,
    StabilityRatioStats, StabilitySection, BASELINE_SPREAD_LIMIT, BOUND_POINTER,
    CONSOLIDATE_COMMAND, HEADLINE_QNAME, HEADLINE_QTYPE,
};
pub use summary::{
    condition_label, condition_short_label, parse_group_id, summarize, BenchKind, MtlMode, Size,
    SummaryRow, LABEL_WITHOUT_MTL, LABEL_WITH_MTL, SHORT_LABEL_WITHOUT_MTL, SHORT_LABEL_WITH_MTL,
};

pub use climb_bench::{LadderRow, LadderRung};
