//! Emulated-RTT DNS latency measurements from the local Docker bridge.
//!
//! This section is deliberately separate from the analytical UDP-fit model:
//! `derived.udp_fit` computes what a TCP fallback *would* cost under an
//! assumed RTT, while this section records what the local pipeline *did* cost
//! with a delay applied by `tc netem` on the bridge. The label is fixed to
//! [`DNS_RTT_LABEL`] so no surface can present these numbers as Internet
//! latency.

use serde::{Deserialize, Serialize};

use crate::results::EvidenceClass;

/// Schema version written into the RTT artifact.
///
/// Version 2 moves the per-query records to a companion samples document so
/// the consolidated results artifact stays small; a cell serializes without a
/// `queries` key when neither the measurement harness nor a legacy artifact
/// supplied any records.
pub const DNS_RTT_SCHEMA_VERSION: u32 = 2;

/// Fixed evidence label for every surface that quotes this section.
pub const DNS_RTT_LABEL: &str = "emulated RTT, local bridge";

/// Minimum samples per configuration cell for a full measurement.
pub const DNS_RTT_FULL_SAMPLES: u64 = 200;

/// The delay levels a full measurement covers, in milliseconds of
/// client-observed round-trip time.
pub const DNS_RTT_DELAYS_MS: [f64; 4] = [0.0, 20.0, 50.0, 100.0];

/// Reason recorded on an artifact whose delay mechanism could not delay every
/// packet. An artifact carrying this reason is displayed as pending, not as a
/// measurement, on every surface.
pub const DNS_RTT_SUPERSEDED_REASON: &str = "harness invalid: userspace-sleep delay was added once per client query, so TCP-fallback round trips were not delayed; superseded until the netem probe lands";

/// Whether the artifact is a placeholder or carries measured cells.
///
/// A measured section can still be quarantined by [`DnsRttSection::superseded`];
/// the status records what the harness wrote, the quarantine records whether
/// those cells may be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnsRttStatus {
    /// No measurement has been recorded yet.
    Pending,
    /// The artifact carries measured cells.
    Measured,
}

impl Default for DnsRttStatus {
    /// An artifact written by the measurement harness omits `status`; absence
    /// means it carries cells, so the default is `measured`. A placeholder
    /// always writes `pending` explicitly.
    fn default() -> Self {
        Self::Measured
    }
}

impl DnsRttStatus {
    /// Stable artifact label.
    pub fn label(self) -> &'static str {
        match self {
            DnsRttStatus::Pending => "pending",
            DnsRttStatus::Measured => "measured",
        }
    }
}

/// One per-query record: what the client actually sent and waited for.
///
/// Written additively after the first harness was invalidated. Older artifacts
/// omit it and deserialize to an empty vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DnsRttQuerySample {
    /// UDP messages sent for this query (normally 1).
    #[serde(default)]
    pub udp_attempts: u32,
    /// TCP messages sent for this query (0, or 1 after a TC=1 reply).
    #[serde(default)]
    pub tcp_attempts: u32,
    /// Bytes in the UDP response, or 0 when the query never used UDP.
    #[serde(default)]
    pub udp_response_bytes: u64,
    /// Bytes in the TCP response, or 0 when no TCP retry happened.
    #[serde(default)]
    pub tcp_response_bytes: u64,
    /// Wall time for the whole exchange in milliseconds.
    #[serde(default)]
    pub total_ms: f64,
    /// `total_ms / target RTT`, or `None` at a 0 ms target.
    #[serde(default)]
    pub rtt_multiple: Option<f64>,
}

/// One row of the `ping` validation of the kernel delay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DnsRttNetemCheck {
    /// Requested client-observed round-trip time in milliseconds.
    #[serde(default)]
    pub target_ms: f64,
    /// Mean `ping` round-trip time in milliseconds.
    #[serde(default)]
    pub ping_avg_ms: f64,
    /// `ping_avg_ms` minus the 0 ms baseline, in milliseconds.
    #[serde(default)]
    pub corrected_ms: f64,
    /// Whether the corrected value is within 10 percent of the target
    /// (or at most 1 ms at a 0 ms target).
    #[serde(default)]
    pub within_10_percent: bool,
    /// ICMP echo requests sent.
    #[serde(default)]
    pub samples: u32,
}

/// One (server, response mode, delay) measurement cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsRttCell {
    /// Queried server: `nsd` or `unbound`.
    pub server: String,
    /// Response mode: `plain` (full SLH-DSA signature) or `mtl-condensed`.
    pub mode: String,
    /// Client-observed emulated round-trip time in milliseconds.
    pub delay_ms: f64,
    /// Successful samples in the cell.
    pub samples: u64,
    /// Samples whose first UDP response had the TC bit set.
    pub truncated: u64,
    /// Transport actually used for the measured exchange: `udp` or
    /// `udp-then-tcp`.
    pub transport: String,
    /// RRSIG kind observed in the final response: `full` or `condensed`.
    pub rrsig_kind: String,
    /// Median client latency in milliseconds.
    pub median_ms: f64,
    /// 95th-percentile client latency in milliseconds.
    pub p95_ms: f64,
    /// How the delay was applied: `tc-netem` or `userspace-sleep`.
    pub mechanism: String,
    /// Median UDP messages sent per query.
    #[serde(default)]
    pub udp_attempts: u32,
    /// Median TCP messages sent per query.
    #[serde(default)]
    pub tcp_attempts: u32,
    /// Median bytes in the UDP response.
    #[serde(default)]
    pub udp_response_bytes: u64,
    /// Median bytes in the TCP response.
    #[serde(default)]
    pub tcp_response_bytes: u64,
    /// Median wall time for the whole exchange in milliseconds.
    #[serde(default)]
    pub median_total_ms: f64,
    /// Median `total_ms / target RTT`; `None` at a 0 ms target.
    #[serde(default)]
    pub median_rtt_multiple: Option<f64>,
    /// Per-query records, empty on artifacts written before the probe
    /// instrumented packet counts and omitted from a consolidated artifact
    /// whose raw records live in [`DnsRttSamples`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queries: Vec<DnsRttQuerySample>,
}

/// Companion document holding the per-query records that consolidation moves
/// out of the results artifact.
///
/// The summaries stay on [`DnsRttCell`] in both documents: the capability to
/// recompute every published median from the raw rows is what makes the split
/// safe, and [`DnsRttSection::take_samples`] guarantees the two views describe
/// the same cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsRttSamples {
    /// Artifact schema version, the same as the section that was split.
    pub schema_version: u32,
    /// Fixed evidence label; always [`DNS_RTT_LABEL`].
    pub label: String,
    /// When the measurement was recorded.
    pub recorded_at: String,
    /// Commit SHA at recording time, when resolvable.
    #[serde(default)]
    pub git_commit: Option<String>,
    /// Human description of how the delay was applied.
    pub netem: String,
    /// Zone queried.
    pub zone: String,
    /// Query name and type.
    pub query: String,
    /// Cells with their per-query records retained.
    pub cells: Vec<DnsRttCell>,
}

impl DnsRttSamples {
    /// Total per-query records across every cell.
    pub fn total_queries(&self) -> usize {
        self.cells.iter().map(|cell| cell.queries.len()).sum()
    }

    /// The cell for one (server, mode, delay) combination.
    pub fn cell(&self, server: &str, mode: &str, delay_ms: f64) -> Option<&DnsRttCell> {
        self.cells.iter().find(|cell| {
            cell.server == server
                && cell.mode == mode
                && (cell.delay_ms - delay_ms).abs() < f64::EPSILON
        })
    }
}

/// Measured RTT section embedded in the consolidated results artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsRttSection {
    /// Artifact schema version.
    pub schema_version: u32,
    /// Pending or measured. Absent in artifacts written by the measurement
    /// harness (`xtask dns-rtt`), which only ever writes measured cells.
    #[serde(default)]
    pub status: DnsRttStatus,
    /// Set when the recorded cells may not be shown. The value states why.
    #[serde(default)]
    pub superseded: Option<String>,
    /// `quick` (smoke test) or `full` (the n-per-cell measurement).
    pub mode: String,
    /// When the measurement was recorded.
    pub recorded_at: String,
    /// Commit SHA at recording time, when resolvable.
    pub git_commit: Option<String>,
    /// Host name the pipeline ran on.
    pub hostname: String,
    /// Kernel and userspace identification.
    pub uname: String,
    /// Fixed evidence label; always [`DNS_RTT_LABEL`].
    pub label: String,
    /// Human description of how the delay was applied.
    pub netem: String,
    /// Zone queried.
    pub zone: String,
    /// Query name and type.
    pub query: String,
    /// One row per configuration cell.
    pub cells: Vec<DnsRttCell>,
    /// Minimum samples per cell for a full measurement.
    pub min_samples_per_cell: u64,
    /// Evidence class: always [`EvidenceClass::Measured`] for recorded cells.
    pub class: EvidenceClass,
    /// Non-fatal measurement warnings (timeouts, fallback mechanism).
    pub warnings: Vec<String>,
    /// `ping` validation of the kernel delay, empty on older artifacts.
    #[serde(default)]
    pub netem_validation: Vec<DnsRttNetemCheck>,
}

impl DnsRttSection {
    /// A pending placeholder: no cells, no evidence claims.
    pub fn pending(recorded_at: String) -> Self {
        Self {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            status: DnsRttStatus::Pending,
            superseded: None,
            mode: "full".to_string(),
            recorded_at,
            git_commit: None,
            hostname: "unknown".to_string(),
            uname: "unknown".to_string(),
            label: DNS_RTT_LABEL.to_string(),
            netem: "no measurement recorded".to_string(),
            zone: "climb.example.".to_string(),
            query: "www.climb.example. A".to_string(),
            cells: Vec::new(),
            min_samples_per_cell: DNS_RTT_FULL_SAMPLES,
            class: EvidenceClass::Measured,
            warnings: Vec::new(),
            netem_validation: Vec::new(),
        }
    }

    /// True when the artifact is marked as recorded by an invalid harness.
    /// A quarantined section is never evidence, whatever its cells say.
    pub fn is_quarantined(&self) -> bool {
        self.superseded.is_some()
    }

    /// The label every surface shows: an invalid artifact is `pending`, never
    /// `measured`, and a placeholder is pending too.
    pub fn display_state(&self) -> &'static str {
        if self.is_quarantined() || self.status != DnsRttStatus::Measured || self.cells.is_empty() {
            "pending"
        } else {
            "measured"
        }
    }

    /// True only when this section is a full measurement with at least the
    /// required samples in every cell. A quick smoke test or a quarantined
    /// artifact never qualifies.
    pub fn is_full_evidence(&self) -> bool {
        !self.is_quarantined()
            && self.status == DnsRttStatus::Measured
            && self.mode == "full"
            && !self.cells.is_empty()
            && self
                .cells
                .iter()
                .all(|cell| cell.samples >= self.min_samples_per_cell)
    }

    /// The cell for one (server, mode, delay) combination.
    pub fn cell(&self, server: &str, mode: &str, delay_ms: f64) -> Option<&DnsRttCell> {
        self.cells.iter().find(|cell| {
            cell.server == server
                && cell.mode == mode
                && (cell.delay_ms - delay_ms).abs() < f64::EPSILON
        })
    }

    /// Move the per-query records out of this section and into a companion
    /// document, leaving the section with summaries only.
    ///
    /// Returns `None` when no cell carries a record: a legacy or superseded
    /// artifact has nothing to split, and writing an empty companion would
    /// pretend otherwise. The section's schema version is updated either way.
    pub fn take_samples(&mut self) -> Option<DnsRttSamples> {
        self.schema_version = DNS_RTT_SCHEMA_VERSION;
        if !self.cells.iter().any(|cell| !cell.queries.is_empty()) {
            return None;
        }
        let samples = DnsRttSamples {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            label: self.label.clone(),
            recorded_at: self.recorded_at.clone(),
            git_commit: self.git_commit.clone(),
            netem: self.netem.clone(),
            zone: self.zone.clone(),
            query: self.query.clone(),
            cells: self.cells.clone(),
        };
        for cell in &mut self.cells {
            cell.queries.clear();
        }
        Some(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(samples: u64) -> DnsRttCell {
        DnsRttCell {
            server: "nsd".to_string(),
            mode: "plain".to_string(),
            delay_ms: 20.0,
            samples,
            truncated: samples,
            transport: "udp-then-tcp".to_string(),
            rrsig_kind: "full".to_string(),
            median_ms: 40.0,
            p95_ms: 55.0,
            mechanism: "tc-netem".to_string(),
            udp_attempts: 1,
            tcp_attempts: 1,
            udp_response_bytes: 512,
            tcp_response_bytes: 1400,
            median_total_ms: 60.0,
            median_rtt_multiple: Some(3.0),
            queries: Vec::new(),
        }
    }

    #[test]
    fn a_pending_section_is_not_evidence_and_carries_the_fixed_label() {
        let section = DnsRttSection::pending("2026-11-09T00:00:00+05:30".to_string());
        assert_eq!(section.status, DnsRttStatus::Pending);
        assert!(!section.is_full_evidence());
        assert_eq!(section.label, DNS_RTT_LABEL);
        assert!(section.cells.is_empty());
    }

    #[test]
    fn a_quick_section_never_qualifies_as_full_evidence() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.status = DnsRttStatus::Measured;
        section.mode = "quick".to_string();
        section.cells = vec![cell(5)];
        assert!(!section.is_full_evidence());
    }

    #[test]
    fn a_full_section_with_short_cells_never_qualifies_as_full_evidence() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.status = DnsRttStatus::Measured;
        section.cells = vec![cell(199), cell(200)];
        assert!(!section.is_full_evidence());
        section.cells = vec![cell(200), cell(200)];
        assert!(section.is_full_evidence());
    }

    #[test]
    fn cells_are_looked_up_by_server_mode_and_delay() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.cells = vec![cell(200)];
        assert_eq!(section.cell("nsd", "plain", 20.0), Some(&cell(200)));
        assert!(section.cell("unbound", "plain", 20.0).is_none());
        assert!(section.cell("nsd", "mtl-condensed", 20.0).is_none());
        assert!(section.cell("nsd", "plain", 50.0).is_none());
    }

    #[test]
    fn a_superseded_section_is_pending_and_never_evidence() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.status = DnsRttStatus::Measured;
        section.mode = "full".to_string();
        section.cells = vec![cell(200), cell(200)];
        assert!(section.is_full_evidence());
        assert_eq!(section.display_state(), "measured");

        section.superseded = Some(DNS_RTT_SUPERSEDED_REASON.to_string());
        assert!(section.is_quarantined());
        assert!(!section.is_full_evidence());
        assert_eq!(section.display_state(), "pending");
    }

    #[test]
    fn the_committed_rtt_artifact_is_quarantined_and_parses() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/dns-rtt.recorded.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let section: DnsRttSection =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        assert!(
            section.is_quarantined(),
            "the committed artifact must carry the superseded marker"
        );
        assert_eq!(section.display_state(), "pending");
        assert!(!section.is_full_evidence());
        assert_eq!(
            section.superseded.as_deref(),
            Some(DNS_RTT_SUPERSEDED_REASON)
        );
        assert!(
            !section.cells.is_empty(),
            "the invalid cells stay in the record as evidence of what was run"
        );
    }

    #[test]
    fn an_artifact_without_the_additive_fields_parses_with_defaults() {
        let json = r#"{
            "schema_version": 1,
            "status": "measured",
            "mode": "quick",
            "recorded_at": "fixed",
            "git_commit": null,
            "hostname": "host",
            "uname": "linux",
            "label": "emulated RTT, local bridge",
            "netem": "old harness",
            "zone": "climb.example.",
            "query": "www.climb.example. A",
            "cells": [{
                "server": "nsd",
                "mode": "plain",
                "delay_ms": 100.0,
                "samples": 20,
                "truncated": 20,
                "transport": "udp-then-tcp",
                "rrsig_kind": "full",
                "median_ms": 200.99,
                "p95_ms": 203.0,
                "mechanism": "userspace-sleep"
            }],
            "min_samples_per_cell": 200,
            "class": "measured",
            "warnings": []
        }"#;
        let section: DnsRttSection = serde_json::from_str(json).expect("old artifact parses");
        assert!(section.superseded.is_none());
        assert!(section.netem_validation.is_empty());
        let cell = &section.cells[0];
        assert_eq!(cell.udp_attempts, 0);
        assert_eq!(cell.tcp_attempts, 0);
        assert_eq!(cell.median_total_ms, 0.0);
        assert!(cell.median_rtt_multiple.is_none());
        assert!(cell.queries.is_empty());
    }

    #[test]
    fn splitting_a_section_moves_every_query_to_the_samples_document() {
        let mut section = DnsRttSection::pending("2026-11-12T00:00:00+05:30".to_string());
        section.status = DnsRttStatus::Measured;
        section.git_commit = Some("abc123".to_string());
        let mut measured = cell(200);
        measured.queries = vec![
            DnsRttQuerySample {
                udp_attempts: 1,
                tcp_attempts: 1,
                udp_response_bytes: 46,
                tcp_response_bytes: 24_438,
                total_ms: 1.5,
                rtt_multiple: Some(0.075),
            };
            200
        ];
        section.cells = vec![measured];

        let samples = section.take_samples().expect("records moved");
        assert_eq!(samples.schema_version, DNS_RTT_SCHEMA_VERSION);
        assert_eq!(samples.total_queries(), 200);
        assert_eq!(samples.git_commit.as_deref(), Some("abc123"));
        assert_eq!(samples.cells[0].median_ms, section.cells[0].median_ms);
        assert_eq!(samples.cells[0].queries.len(), 200);
        assert!(
            section.cells[0].queries.is_empty(),
            "the results section keeps summaries only"
        );
        assert_eq!(section.schema_version, DNS_RTT_SCHEMA_VERSION);
    }

    #[test]
    fn a_slimmed_section_serializes_without_a_queries_key() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.status = DnsRttStatus::Measured;
        section.cells = vec![cell(200)];
        let value = serde_json::to_value(&section).expect("serializes");
        let cell = value
            .pointer("/cells/0")
            .and_then(serde_json::Value::as_object)
            .expect("cell object");
        assert!(
            !cell.contains_key("queries"),
            "an empty query vector must not appear in the consolidated artifact"
        );
    }

    #[test]
    fn a_section_without_queries_has_nothing_to_split() {
        let mut section = DnsRttSection::pending("fixed".to_string());
        section.cells = vec![cell(200)];
        assert!(section.take_samples().is_none());
        assert_eq!(section.schema_version, DNS_RTT_SCHEMA_VERSION);
    }

    #[test]
    fn a_superseded_artifact_with_queries_still_parses_and_keeps_them() {
        let json = r#"{
            "schema_version": 1,
            "status": "measured",
            "superseded": "harness invalid",
            "mode": "full",
            "recorded_at": "fixed",
            "git_commit": null,
            "hostname": "host",
            "uname": "linux",
            "label": "emulated RTT, local bridge",
            "netem": "old harness",
            "zone": "climb.example.",
            "query": "www.climb.example. A",
            "cells": [{
                "server": "nsd",
                "mode": "plain",
                "delay_ms": 100.0,
                "samples": 1,
                "truncated": 1,
                "transport": "udp-then-tcp",
                "rrsig_kind": "full",
                "median_ms": 200.99,
                "p95_ms": 203.0,
                "mechanism": "userspace-sleep",
                "queries": [{
                    "udp_attempts": 1,
                    "tcp_attempts": 1,
                    "udp_response_bytes": 46,
                    "tcp_response_bytes": 24438,
                    "total_ms": 200.99,
                    "rtt_multiple": 2.0099
                }]
            }],
            "min_samples_per_cell": 200,
            "class": "measured",
            "warnings": []
        }"#;
        let mut section: DnsRttSection =
            serde_json::from_str(json).expect("legacy artifact parses");
        assert_eq!(section.cells[0].queries.len(), 1);
        let samples = section.take_samples().expect("the legacy record is split");
        assert_eq!(samples.total_queries(), 1);
        assert!(section.is_quarantined());
    }
}
