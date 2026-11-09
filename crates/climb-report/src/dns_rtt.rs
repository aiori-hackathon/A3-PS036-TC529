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
pub const DNS_RTT_SCHEMA_VERSION: u32 = 1;

/// Fixed evidence label for every surface that quotes this section.
pub const DNS_RTT_LABEL: &str = "emulated RTT, local bridge";

/// Minimum samples per configuration cell for a full measurement.
pub const DNS_RTT_FULL_SAMPLES: u64 = 200;

/// The delay levels a full measurement covers, in milliseconds of
/// client-observed round-trip time.
pub const DNS_RTT_DELAYS_MS: [f64; 4] = [0.0, 20.0, 50.0, 100.0];

/// Whether the artifact is a placeholder or carries measured cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnsRttStatus {
    /// No measurement has been recorded yet.
    Pending,
    /// The artifact carries measured cells.
    Measured,
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
}

/// Measured RTT section embedded in the consolidated results artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsRttSection {
    /// Artifact schema version.
    pub schema_version: u32,
    /// Pending or measured.
    pub status: DnsRttStatus,
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
}

impl DnsRttSection {
    /// A pending placeholder: no cells, no evidence claims.
    pub fn pending(recorded_at: String) -> Self {
        Self {
            schema_version: DNS_RTT_SCHEMA_VERSION,
            status: DnsRttStatus::Pending,
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
        }
    }

    /// True only when this section is a full measurement with at least the
    /// required samples in every cell. A quick smoke test never qualifies.
    pub fn is_full_evidence(&self) -> bool {
        self.status == DnsRttStatus::Measured
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
}
