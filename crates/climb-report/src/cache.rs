//! Resolver cache usage: measured (Unbound's own accounting) and analytical.
//!
//! The measured side is read from Unbound's `stats_noreset` output
//! (`mem.cache.rrset`, `mem.cache.message`, `rrset.cache.count`,
//! `msg.cache.count`) plus the container's resident memory before and after
//! loading `R` distinct RRsets. Evidence class: `MEASURED`.
//!
//! The analytical side is `climb_analytical::resolver_cache_bytes` (wire sizes
//! plus the shared ladder once). It is never fitted to the measurement; the
//! measured / analytical ratio is reported on every surface, exactly as the
//! Memory tab does for benchmark-process memory.
//!
//! [`parse_unbound_stats`] is discriminating: a present-but-unparseable stats
//! blob is a hard error naming its source, never a silently empty cache
//! section.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Schema version written into the cache-use section.
pub const CACHE_USE_SCHEMA_VERSION: u32 = 1;

/// Fixed evidence class for the measured cache accounting.
pub const CACHE_USE_CLASS: &str = "MEASURED";

/// Fixed method label for the measured cache accounting.
pub const CACHE_USE_METHOD: &str = "unbound-control stats_noreset + container RSS";

/// One measurement run: querying `r` distinct names through one resolver
/// configuration and recording Unbound's cache accounting afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheRun {
    /// Resolver configuration: `plain` or `mtl-condensed`.
    pub config: String,
    /// Number of distinct RRset owner names loaded into the cache.
    pub r: usize,
    /// Container resident memory in bytes before loading.
    pub container_rss_before_bytes: u64,
    /// Container resident memory in bytes after loading.
    pub container_rss_after_bytes: u64,
    /// Unbound cache accounting observed after loading.
    pub stats: UnboundStats,
}

/// Unbound's own cache accounting from `unbound-control stats_noreset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnboundStats {
    /// `mem.cache.rrset` — bytes held by the rrset cache.
    pub rrset_cache_bytes: u64,
    /// `rrset.cache.count` — number of rrset cache entries.
    pub rrset_cache_entries: u64,
    /// `mem.cache.message` — bytes held by the message cache.
    pub message_cache_bytes: u64,
    /// `msg.cache.count` — number of message cache entries.
    pub message_cache_entries: u64,
    /// `mem.cache.ladder` — bytes held by the MTL ladder cache (present on
    /// the MTL-patched build; zero on stock Unbound).
    #[serde(default)]
    pub ladder_cache_bytes: u64,
}

/// One analytical row, pairing the measured cache bytes for a configuration
/// with the closed-form prediction and their ratio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheAnalyticalRow {
    /// Resolver configuration.
    pub config: String,
    /// Number of distinct RRsets the prediction covers.
    pub r: usize,
    /// Measured cache bytes (median of the runs for this configuration).
    pub measured_bytes: u64,
    /// Analytical predicted cache bytes from [`climb_analytical::CacheConfig`].
    pub analytical_bytes: u64,
    /// `measured_bytes / analytical_bytes`, or `0.0` when the analytical
    /// prediction is zero (ratio not computed).
    pub measured_over_analytical: f64,
}

/// The complete `cache_use` section of the consolidated results artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheResults {
    /// Artifact schema version.
    pub schema_version: u32,
    /// Evidence class label (`MEASURED`).
    pub class: String,
    /// Measurement method label.
    pub method: String,
    /// One row per measurement run.
    pub measured: Vec<CacheRun>,
    /// One analytical row per configuration.
    pub analytical: Vec<CacheAnalyticalRow>,
}

/// The raw cache-use measurement artifact written by `cargo xtask dns-cache`.
///
/// One file per measurement run; consolidation turns `runs` into the
/// `measured` half of [`CacheResults`] and computes `analytical` from the wire
/// report. The two types stay separate so the raw artifact can carry run
/// metadata (mode, commit, per-run probe output) that the consolidated
/// section does not need to repeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheArtifact {
    /// Artifact schema version.
    pub schema_version: u32,
    /// `quick` or `full`.
    pub mode: String,
    /// When the measurement was recorded.
    pub recorded_at: String,
    /// Git commit at recording time, if resolvable.
    pub git_commit: Option<String>,
    /// Host name the pipeline ran on.
    pub hostname: String,
    /// Kernel/userspace identification.
    pub uname: String,
    /// Fixed evidence label.
    pub label: String,
    /// Evidence class (`MEASURED`).
    pub class: String,
    /// Measurement method.
    pub method: String,
    /// Resolver measured.
    pub resolver: String,
    /// Distinct RRset owner names queried per cell.
    pub r_values: Vec<usize>,
    /// Configurations measured.
    pub configs: Vec<String>,
    /// Repetitions per cell.
    pub repeats: u64,
    /// One row per measurement run.
    pub runs: Vec<CacheRun>,
    /// Non-fatal measurement warnings.
    pub warnings: Vec<String>,
}

/// Error parsing an `unbound-control stats_noreset` blob.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct CacheParseError {
    message: String,
}

impl CacheParseError {
    fn invalid_blob(source: &str) -> Self {
        Self {
            message: format!(
                "unbound-control stats_noreset blob did not carry the expected \
                 cache accounting lines; tried mem.cache.rrset, \
                 rrset.cache.count, mem.cache.message, msg.cache.count: {source}"
            ),
        }
    }
}

/// Parse one `unbound-control stats_noreset` blob into cache accounting.
///
/// Lines are of the form `key=value`. A blob that carries none of the expected
/// cache keys is an error, so a corrupt stats capture is never mistaken for an
/// empty cache.
pub fn parse_unbound_stats(blob: &str) -> Result<UnboundStats, CacheParseError> {
    let mut rrset_bytes: Option<u64> = None;
    let mut rrset_entries: Option<u64> = None;
    let mut message_bytes: Option<u64> = None;
    let mut message_entries: Option<u64> = None;
    let mut ladder_bytes: Option<u64> = None;

    for line in blob.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            "mem.cache.rrset" => rrset_bytes = value.parse::<u64>().ok(),
            "rrset.cache.count" => rrset_entries = value.parse::<u64>().ok(),
            "mem.cache.message" => message_bytes = value.parse::<u64>().ok(),
            "msg.cache.count" => message_entries = value.parse::<u64>().ok(),
            "mem.cache.ladder" => ladder_bytes = value.parse::<u64>().ok(),
            _ => {}
        }
    }

    let stats = UnboundStats {
        rrset_cache_bytes: rrset_bytes.ok_or_else(|| CacheParseError::invalid_blob(blob))?,
        rrset_cache_entries: rrset_entries.ok_or_else(|| CacheParseError::invalid_blob(blob))?,
        message_cache_bytes: message_bytes.ok_or_else(|| CacheParseError::invalid_blob(blob))?,
        message_cache_entries: message_entries
            .ok_or_else(|| CacheParseError::invalid_blob(blob))?,
        ladder_cache_bytes: ladder_bytes.unwrap_or(0),
    };
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATS_BLOB: &str = "thread0.num.cachehits=0\n\
                              total.num.cachehits=0\n\
                              mem.cache.rrset=1048576\n\
                              rrset.cache.count=400\n\
                              mem.cache.message=2097152\n\
                              msg.cache.count=200\n\
                              mem.cache.ladder=8192\n\
                              num.query.total=600";

    #[test]
    fn parses_the_cache_accounting_lines() {
        let stats = parse_unbound_stats(STATS_BLOB).expect("stats parse");
        assert_eq!(stats.rrset_cache_bytes, 1_048_576);
        assert_eq!(stats.rrset_cache_entries, 400);
        assert_eq!(stats.message_cache_bytes, 2_097_152);
        assert_eq!(stats.message_cache_entries, 200);
        assert_eq!(stats.ladder_cache_bytes, 8_192);
    }

    #[test]
    fn ladder_absent_defaults_to_zero() {
        let blob =
            "mem.cache.rrset=100\nrrset.cache.count=1\nmem.cache.message=200\nmsg.cache.count=1";
        let stats = parse_unbound_stats(blob).expect("stats parse");
        assert_eq!(stats.ladder_cache_bytes, 0);
    }

    #[test]
    fn missing_required_key_is_a_hard_error_naming_the_shapes_tried() {
        for blob in [
            "not stats at all",
            "mem.cache.rrset=100\nrrset.cache.count=1",
        ] {
            let err = parse_unbound_stats(blob).unwrap_err().to_string();
            for shape in [
                "mem.cache.rrset",
                "rrset.cache.count",
                "mem.cache.message",
                "msg.cache.count",
            ] {
                assert!(err.contains(shape), "error must name {shape}: {err}");
            }
        }
    }

    #[test]
    fn empty_blob_is_a_hard_error() {
        assert!(parse_unbound_stats("").is_err());
        assert!(parse_unbound_stats("\n\n").is_err());
    }
}
