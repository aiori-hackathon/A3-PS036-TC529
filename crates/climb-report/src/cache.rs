//! Resolver cache usage: measured (Unbound's own accounting) and analytical.
//!
//! The measured side is read from Unbound's `stats_noreset` output
//! (`mem.cache.rrset`, `mem.cache.message`, `mem.cache.ladder`,
//! `rrset.cache.count`, `msg.cache.count`) plus the container's resident memory
//! before and after loading `R` distinct RRsets. Evidence class: `MEASURED`.
//!
//! Two different caches are measured, and they are never merged:
//!
//! * the **answer cache** (`mem.cache.rrset`, `mem.cache.message`) after
//!   loading a zone signed by the MTL-capable signer, and
//! * the **ladder cache** (`mem.cache.ladder`), the MTL-specific structure the
//!   patched Unbound populates when its validator verifies a signed ladder.
//!
//! The answer cache is identical by construction: the reference Unbound build
//! does not serve condensed responses, so both measured configurations hold
//! the full responses the signed zone stores. Naming that cell
//! "MTL-condensed" would claim a measurement the build cannot produce, so
//! [`measured_config_label`] names it for what it is. The analytical side
//! still predicts condensed responses, and its ratio is reported, never used
//! to tune the model.
//!
//! [`parse_unbound_stats`] is discriminating: a present-but-unparseable stats
//! blob is a hard error naming its source, never a silently empty cache
//! section.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Schema version written into the cache-use section.
pub const CACHE_USE_SCHEMA_VERSION: u32 = 2;

/// Fixed evidence class for the measured cache accounting.
pub const CACHE_USE_CLASS: &str = "MEASURED";

/// Fixed method label for the measured cache accounting.
pub const CACHE_USE_METHOD: &str = "unbound-control stats_noreset + container RSS";

/// Display label for the measured MTL-signed-zone answer-cache cell.
///
/// This is the honest description of what the reference resolver served: a
/// zone signed by the MTL-capable signer, answered with full responses. It is
/// deliberately not "MTL-condensed", because this build ignores the SigTag and
/// returns the full response that the zone stores.
pub const MEASURED_MTL_ZONE_LABEL: &str =
    "MTL-signed zone, reference Unbound (serves full responses)";

/// Label for the analytical condensed-response prediction.
pub const ANALYTICAL_MTL_CONDENSED_LABEL: &str = "MTL-condensed";

/// One sentence every surface carries beside the measured answer-cache rows.
pub const ANSWER_CACHE_IDENTICAL_NOTE: &str =
    "The answer cache is identical by construction: the reference Unbound build \
     does not serve condensed responses, so both configurations cache the full \
     responses the signed zone stores.";

/// Human label for a configuration in the *measured* answer-cache table.
///
/// `mtl-condensed` becomes [`MEASURED_MTL_ZONE_LABEL`]; every other value is
/// passed through unchanged. No measured row may be labelled "condensed"
/// without the qualifier that says what was actually served.
#[must_use]
pub fn measured_config_label(config: &str) -> String {
    match config {
        "mtl-condensed" => MEASURED_MTL_ZONE_LABEL.to_string(),
        other => other.to_string(),
    }
}

/// Human label for a configuration in the *analytical* table.
#[must_use]
pub fn analytical_config_label(config: &str) -> String {
    match config {
        "mtl-condensed" => ANALYTICAL_MTL_CONDENSED_LABEL.to_string(),
        other => other.to_string(),
    }
}

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
    /// `mem.cache.ladder` — bytes held by the MTL ladder cache. This is the
    /// only ladder-cache statistic the patched build exposes; there is no
    /// ladder entry-count line beside it (see [`LadderRun`]).
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

/// One measured ladder-cache run for one dataset size and one repetition.
///
/// The pipeline flushes the resolver, loads `r` distinct names from the
/// MTL-signed zone (pass 1), then loads the same names again (pass 2). The
/// ladder cache is the MTL-specific structure: it grows during pass 1 by the
/// bytes of the ladder(s) the validator stored, and a second pass over the same
/// zone must not grow it again. The raw accounting is kept for every pass so a
/// later reader can check the arithmetic instead of trusting a delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderRun {
    /// Dataset size label (`small` / `medium`).
    pub size: String,
    /// Number of distinct owner names loaded per pass.
    pub r: usize,
    /// Repetition index within the cell.
    pub repeat: u64,
    /// Signed-ladder length read from the zone's own SOA RRSIG, in bytes.
    pub signed_ladder_bytes: u64,
    /// Number of distinct ladders the signed zone carries (one MTL batch).
    pub distinct_ladders: u64,
    /// `mem.cache.ladder` before pass 1.
    pub ladder_cache_bytes_before: u64,
    /// `mem.cache.ladder` after pass 1.
    pub ladder_cache_bytes_after_first_pass: u64,
    /// `mem.cache.ladder` after pass 2 (same names, now cache hits).
    pub ladder_cache_bytes_after_second_pass: u64,
    /// Full accounting before pass 1.
    pub before: UnboundStats,
    /// Full accounting after pass 1.
    pub after_first_pass: UnboundStats,
    /// Full accounting after pass 2.
    pub after_second_pass: UnboundStats,
    /// Whether this resolver build exposes a ladder entry-count statistic.
    ///
    /// The patched build exposes `mem.cache.ladder` (bytes) only; there is no
    /// `ladder.cache.count` line in `unbound-control stats_noreset`, so the
    /// entry count is not invented from a byte delta.
    pub ladder_entry_count_exposed: bool,
}

/// One analytical row for the ladder cache: the measured growth beside the
/// prediction `distinct_ladders × recorded signed-ladder bytes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderAnalyticalRow {
    /// Dataset size label (`small` / `medium`).
    pub size: String,
    /// Number of distinct owner names loaded per pass.
    pub r: usize,
    /// Number of distinct ladders in the signed zone.
    pub distinct_ladders: u64,
    /// Signed-ladder length read from the zone's SOA RRSIG, in bytes.
    pub signed_ladder_bytes: u64,
    /// Median first-pass growth across the repetitions.
    pub measured_bytes: u64,
    /// Median second-pass growth across the repetitions (must be zero).
    pub second_pass_growth_bytes: u64,
    /// `distinct_ladders × signed_ladder_bytes`.
    pub analytical_bytes: u64,
    /// `measured_bytes / analytical_bytes`, or `0.0` when the prediction is
    /// zero (ratio not computed).
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
    /// One raw row per ladder-cache run (additive; absent in older artifacts).
    #[serde(default)]
    pub ladder: Vec<LadderRun>,
    /// One analytical row per dataset size for the ladder cache.
    #[serde(default)]
    pub ladder_analytical: Vec<LadderAnalyticalRow>,
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
    /// One row per ladder-cache run (additive; absent in older artifacts).
    #[serde(default)]
    pub ladder_runs: Vec<LadderRun>,
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

    /// The stats lines exactly as the reference build prints them: the ladder
    /// cache carries a byte count only, with no `ladder.cache.count` beside it.
    const LADDER_STATS_BLOB: &str = "mem.cache.rrset=921471\n\
                                     mem.cache.message=97594\n\
                                     mem.cache.ladder=82952\n\
                                     msg.cache.count=100\n\
                                     rrset.cache.count=102\n\
                                     infra.cache.count=1\n\
                                     key.cache.count=0";

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
    fn parses_the_ladder_cache_line_and_its_neighbours() {
        let stats = parse_unbound_stats(LADDER_STATS_BLOB).expect("ladder stats parse");
        assert_eq!(stats.ladder_cache_bytes, 82_952);
        assert_eq!(stats.rrset_cache_bytes, 921_471);
        assert_eq!(stats.message_cache_bytes, 97_594);
        assert_eq!(stats.rrset_cache_entries, 102);
        assert_eq!(stats.message_cache_entries, 100);
    }

    #[test]
    fn the_ladder_cache_has_no_entry_count_stat_to_read() {
        // The reference build prints `mem.cache.ladder` (bytes) and cache
        // counts for the message, rrset, infra and key caches only. There is
        // no ladder entry count in the blob, so a parser that invented one
        // from the byte count would be fabricating evidence.
        assert!(LADDER_STATS_BLOB.contains("mem.cache.ladder="));
        assert!(!LADDER_STATS_BLOB.contains("ladder.cache.count"));
        let stats = parse_unbound_stats(LADDER_STATS_BLOB).expect("ladder stats parse");
        assert_eq!(stats.ladder_cache_bytes, 82_952);
    }

    #[test]
    fn measured_labels_never_say_condensed_alone() {
        assert_eq!(measured_config_label("plain"), "plain");
        assert_eq!(
            measured_config_label("mtl-condensed"),
            MEASURED_MTL_ZONE_LABEL
        );
        assert_eq!(
            measured_config_label("mtl-condensed"),
            "MTL-signed zone, reference Unbound (serves full responses)"
        );
        assert!(!measured_config_label("mtl-condensed").contains("MTL-condensed"));
        assert!(ANSWER_CACHE_IDENTICAL_NOTE.contains("identical by construction"));
    }

    #[test]
    fn analytical_labels_still_name_the_condensed_prediction() {
        assert_eq!(analytical_config_label("plain"), "plain");
        assert_eq!(analytical_config_label("mtl-condensed"), "MTL-condensed");
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
