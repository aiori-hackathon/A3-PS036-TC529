use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::ReportError;
use crate::ladder::{LadderSection, LadderSource, LadderVerification};
use crate::memory::{parse_memory_artifact, MemoryArtifact};
use crate::record::{scan_criterion, CriterionRecord};
use crate::summary::{summarize, SummaryRow};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SigningBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyingBenchmarkResult {
    pub size: String,
    pub condition: String,
    pub messages: u64,
    pub mean_ns: f64,
    pub ci_lower_ns: f64,
    pub ci_upper_ns: f64,
    pub speedup: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub signing: Vec<SigningBenchmarkResult>,
    pub verifying: Vec<VerifyingBenchmarkResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireSizeResult {
    pub size: String,
    pub message_count: usize,
    pub ladder_len: usize,
    pub condensed_min: usize,
    pub condensed_mean: f64,
    pub condensed_max: usize,
    pub condensed_uniform: bool,
    pub with_mtl_total_bytes: u64,
    pub without_mtl_total_bytes: u64,
    pub reduction_percent: f64,
    /// Configuration A wire total: `message_count × plain_signature_len_bytes`,
    /// one full SLH-DSA signature per message with no ladder. Zero for results
    /// written before configuration A was measured.
    #[serde(default)]
    pub plain_total_bytes: u64,
    /// Per-message plain SLH-DSA signature length in bytes, read from liboqs.
    /// Zero for results written before configuration A was measured.
    #[serde(default)]
    pub plain_signature_len_bytes: u64,
}

/// How a displayed number was obtained, so a reader can tell a measurement
/// apart from an identity-based extrapolation or a model estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    /// Read directly from a measurement artifact.
    Measured,
    /// Produced by the `N × per-message` identity from a measured per-message
    /// cost, rather than measured over the whole batch.
    Extrapolated,
    /// Produced by an analytical model rather than a measurement.
    Analytical,
    /// Computed as a ratio or summary of other values.
    Derived,
}

impl EvidenceClass {
    /// Artifact-stable lower-case name, matching the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceClass::Measured => "measured",
            EvidenceClass::Extrapolated => "extrapolated",
            EvidenceClass::Analytical => "analytical",
            EvidenceClass::Derived => "derived",
        }
    }

    /// Upper-case label for tables and chart legends.
    pub fn label(self) -> &'static str {
        match self {
            EvidenceClass::Measured => "MEASURED",
            EvidenceClass::Extrapolated => "EXTRAPOLATED",
            EvidenceClass::Analytical => "ANALYTICAL",
            EvidenceClass::Derived => "DERIVED",
        }
    }
}

/// Title-case a dataset size label for display (`"small"` → `"Small"`).
///
/// The wire and reuse sections of the results artifact carry the label
/// lowercase while the comparison section stores it title-cased; every
/// user-visible surface (report table, chart legend, TUI) spells it the same
/// way, so this normalises the label at display time without touching the
/// artifact value.
pub fn size_display(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Compare two size labels for equality, ignoring case.
///
/// Every per-size array in the results artifact is keyed by the label in its
/// `size` field, and the sections disagree about its spelling (lowercase in
/// `wire_size`/`reuse`, title-cased in `comparisons`). Consumers therefore must
/// never reach a row by position: an array reordered upstream (or a size added)
/// would silently relabel every number. [`find_by_size`] is the one lookup;
/// this is the comparison it uses.
pub fn size_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Find the entry of a per-size slice whose `size` field matches `size`.
///
/// The extractor is passed in so the same keyed lookup serves every section
/// (`comparisons`, `wire_size`, `reuse`, `memory`, `plain`, …) without knowing
/// each row's type. A label is matched case-insensitively via [`size_eq`], so
/// callers can pass either the artifact spelling (`"small"`) or the display
/// spelling (`"Small"`).
pub fn find_by_size<'a, T>(rows: &'a [T], size: &str, key: impl Fn(&T) -> &str) -> Option<&'a T> {
    rows.iter().find(|row| size_eq(key(row), size))
}

/// Configuration A (plain SLH-DSA) per-message timings for one dataset size.
///
/// A plain signature carries no ladder, so its cost is size-independent: a size
/// is measured by signing/verifying a single message of that size's seeded
/// dataset. Sizes without their own measurement carry the mean of the measured
/// samples, marked [`EvidenceClass::Extrapolated`] with the sample count `k`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlainTimingResult {
    pub size: String,
    pub messages: u64,
    pub signing_mean_ns: Option<f64>,
    pub signing_ci_lower_ns: Option<f64>,
    pub signing_ci_upper_ns: Option<f64>,
    pub verifying_mean_ns: Option<f64>,
    pub verifying_ci_lower_ns: Option<f64>,
    pub verifying_ci_upper_ns: Option<f64>,
    pub class: EvidenceClass,
    /// Number of measured per-message samples averaged into a row (`k`); `None`
    /// for a directly measured single-message row.
    #[serde(default)]
    pub samples: Option<u32>,
}

/// Classical signature baseline timing result (context only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassicalTimingResult {
    pub algorithm: String,
    pub size: String,
    pub messages: u64,
    pub signing_mean_ns: Option<f64>,
    pub signing_ci_lower_ns: Option<f64>,
    pub signing_ci_upper_ns: Option<f64>,
    pub verifying_mean_ns: Option<f64>,
    pub verifying_ci_lower_ns: Option<f64>,
    pub verifying_ci_upper_ns: Option<f64>,
    pub signature_len_bytes: usize,
    pub class: EvidenceClass,
}

/// Signature lengths in bytes for the three classical context baselines,
/// read from the document the classical benchmark writes after producing one
/// signature of each algorithm. Every field is required and must be
/// non-zero: a document that omits a field, carries a wrong shape, or
/// records a zero length fails to load with an error naming the path — a
/// present-but-invalid artifact is never silently ignored, and no length is
/// ever typed in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicalSignatureLengths {
    pub ed25519: usize,
    pub ecdsa_p256: usize,
    pub rsa2048: usize,
}

impl ClassicalSignatureLengths {
    /// The measured length for one algorithm, keyed by the parsed group-id
    /// algorithm.
    pub fn for_algo(self, algo: crate::summary::ClassicalAlgo) -> usize {
        match algo {
            crate::summary::ClassicalAlgo::Ed25519 => self.ed25519,
            crate::summary::ClassicalAlgo::EcdsaP256 => self.ecdsa_p256,
            crate::summary::ClassicalAlgo::Rsa2048 => self.rsa2048,
        }
    }
}

/// Three-way configuration comparison for one dataset size, derived from the
/// consolidated artifacts: A plain SLH-DSA, B MTL non-amortised, C MTL
/// amortised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigComparison {
    pub size: String,
    /// `(A − C) / A × 100`: amortised MTL's wire reduction against plain SLH-DSA.
    pub wire_reduction_c_vs_a_percent: Option<f64>,
    /// `(B − C) / B × 100`: amortised MTL's wire reduction against non-amortised MTL.
    pub wire_reduction_c_vs_b_percent: Option<f64>,
    /// Per-message signing speed-up `A / C`.
    pub signing_speedup_c_vs_a: Option<f64>,
    /// Per-message signing speed-up `B / C`.
    pub signing_speedup_c_vs_b: Option<f64>,
    /// Per-message verification speed-up `A / C` with a trust-cached ladder.
    pub verifying_speedup_cached_vs_a: Option<f64>,
    /// Per-message verification speed-up `A / C` with the ladder re-validated.
    pub verifying_speedup_cold_vs_a: Option<f64>,
    /// Weakest evidence class among the row's inputs.
    pub class: EvidenceClass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryResult {
    pub size: String,
    pub message_count: usize,
    pub measured_mean_bytes: u64,
    pub libmtl_analytical_bytes: usize,
    pub harness_buffers_bytes: u64,
    pub analytical_total_bytes: u64,
    pub residual_bytes: i64,
    pub residual_percent: f64,
    pub ratio: f64,
    pub measured_analytical_total_ratio: f64,
    pub measured_range_min_bytes: u64,
    pub measured_range_max_bytes: u64,
    pub measured_stddev_bytes: f64,
    /// Central measured value in bytes when the source reports one. For the
    /// isolated-process artifact this is its median; for the legacy
    /// comparison shape (which carries only a mean) it mirrors
    /// `measured_mean_bytes`. Zero for results written before this field
    /// existed.
    #[serde(default)]
    pub median_bytes: u64,
    /// Interquartile range in bytes, as reported by the isolated-process
    /// artifact. Zero for legacy shapes (which carry a range/stddev instead)
    /// and for results written before this field existed.
    #[serde(default)]
    pub iqr_bytes: u64,
    /// Number of measurement samples behind this row; zero when the source
    /// does not report a sample count.
    #[serde(default)]
    pub samples: usize,
    /// Measurement method that produced this row, e.g. `isolated-process` or
    /// `rss-delta`. Empty for results written before this field existed.
    #[serde(default)]
    pub method: String,
    /// Analytical dataset message-buffer bytes. Zero for rows written before
    /// this term was modelled.
    #[serde(default)]
    pub dataset_buffers_bytes: u64,
    /// Analytical keypair (serialized key image) bytes. Zero for rows written
    /// before this term was modelled.
    #[serde(default)]
    pub keypair_bytes: u64,
    /// Measured peak footprint in bytes (`hwm − baseline`, median over runs):
    /// the headline footprint metric. Zero when the source reports no
    /// high-water mark.
    #[serde(default)]
    pub measured_peak_bytes: u64,
    /// Per-stage measured deltas in bytes; `None` for rows written before stage
    /// attribution was recorded.
    #[serde(default)]
    pub stages: Option<MemoryStages>,
}

/// Per-stage measured deltas for one memory row, all in bytes.
///
/// `resident` (after − baseline) and `peak` (hwm − baseline) are whole-run
/// figures; `dataset`, `keypair` and `sign` are the stage widths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryStages {
    /// Dataset generation delta.
    pub dataset_delta_bytes: u64,
    /// Keypair generation delta.
    pub keypair_delta_bytes: u64,
    /// `sign_batch` delta.
    pub sign_delta_bytes: u64,
    /// Whole-run resident delta.
    pub resident_bytes: u64,
    /// Whole-run peak delta.
    pub peak_bytes: u64,
    /// Interquartile range of the dataset-generation delta.
    pub dataset_delta_iqr_bytes: u64,
    /// Interquartile range of the keypair-generation delta.
    pub keypair_delta_iqr_bytes: u64,
    /// Interquartile range of the `sign_batch` delta.
    pub sign_delta_iqr_bytes: u64,
    /// Interquartile range of the resident delta.
    pub resident_iqr_bytes: u64,
    /// Interquartile range of the peak delta.
    pub peak_iqr_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticalMemoryModel {
    pub tree_page_size_bytes: usize,
    pub tree_max_pages: usize,
    pub randomizer_pages: usize,
    pub hash_size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryResults {
    pub measured: Vec<MemoryResult>,
    pub analytical_model: AnalyticalMemoryModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentFingerprint {
    pub schema_version: u32,
    pub timestamp: String,
    pub system: serde_json::Value,
    pub toolchain: serde_json::Value,
    pub cpu: serde_json::Value,
    pub submodules: Vec<serde_json::Value>,
    pub docker: serde_json::Value,
    pub benchmark_runtime: serde_json::Value,
}

/// Where a [`DnsPipelineHeadline`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnsCaptureSource {
    /// A live `cargo xtask dns-demo --record` run.
    Live,
    /// The committed `data/dns-capture.recorded.json` fixture.
    RecordedFixture,
}

/// DNS header flags recorded for one captured query.
///
/// Only `rcode` is observed by the current capture path; `ad` and `tc` are
/// part of the shared schema but are left `None` rather than guessed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureFlags {
    pub ad: Option<bool>,
    pub tc: Option<bool>,
    pub rcode: Option<u8>,
}

/// One query captured during the live MTL DNS pipeline demo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsCaptureQuery {
    /// Capture pass: `full`, `condensed`, or `unbound`.
    pub label: String,
    pub qname: String,
    pub qtype: String,
    pub response_bytes: u64,
    /// Length of the first answer RRSIG signature field, when present.
    pub answer_rrsig_bytes: Option<u64>,
    pub flags: DnsCaptureFlags,
    pub transport: String,
    pub edns_udp_size: Option<u16>,
    pub elapsed_ms: Option<f64>,
    /// Raw response wire bytes as lowercase hex, when the recording captured
    /// them. This is what lets `climb-dns` parse a [`ResponseAnatomy`] offline.
    /// `#[serde(default)]` keeps older results files that predate this field
    /// parseable.
    ///
    /// [`ResponseAnatomy`]: climb_dns::ResponseAnatomy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_hex: Option<String>,
}

/// Host summary for a recorded capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureHost {
    pub hostname: String,
    pub uname: String,
}

/// Docker image reference (and digest, when resolvable) used for a capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCaptureImage {
    pub reference: String,
    pub digest: Option<String>,
}

/// A complete recorded run of the live MTL DNS pipeline demo.
///
/// Written by `cargo xtask dns-demo --record` and consumed by
/// `cargo xtask consolidate`. Fields the capture path does not observe are
/// optional and left `None`; nothing here is synthesized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsCapture {
    pub schema_version: u32,
    /// RFC 3339 timestamp of the recording.
    pub recorded_at: String,
    /// Git commit the recording was made at, when the recorder could resolve
    /// one. Captures written before this field existed carry `None` and stay
    /// parseable (`#[serde(default)]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    pub host: DnsCaptureHost,
    pub images: Vec<DnsCaptureImage>,
    pub algorithm: String,
    pub zone: String,
    pub queries: Vec<DnsCaptureQuery>,
}

/// Byte-accounting summary of one recorded DNS response, parsed from its raw
/// wire bytes by `climb-dns`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsAnatomyRecord {
    /// The `www.climb.example. A` capture pass this describes (`full` or
    /// `condensed`).
    pub label: String,
    /// Total captured response length in bytes.
    pub wire_length: usize,
    /// Fixed DNS header length (12 B).
    pub header_bytes: usize,
    /// Question section total bytes.
    pub question_bytes: usize,
    /// Answer-section RRSIG count.
    pub rrsig_count: usize,
    /// Per-RRSIG signature field length, in wire order.
    pub rrsig_signature_lengths: Vec<u32>,
    /// Count of full-signature (MTL-Type 0x01) RRSIGs.
    pub full_rrsig_count: usize,
    /// Count of condensed-proof (MTL-Type 0x02) RRSIGs.
    pub condensed_rrsig_count: usize,
    /// Sum of every part; equals `wire_length` for a well-formed response.
    pub parts_sum: usize,
}

/// The `dns.anatomy` section: byte decomposition of the headline
/// `www.climb.example. A` response pair (with and without MTL). `None` when no
/// capture with embedded wire bytes is available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsAnatomySection {
    /// Provenance of the parsed bytes.
    pub source: DnsCaptureSource,
    /// RFC 3339 timestamp of the recording.
    pub recorded_at: String,
    /// Full-signature (no MTL) response anatomy.
    pub full: Option<DnsAnatomyRecord>,
    /// Condensed-proof (with MTL) response anatomy.
    pub condensed: Option<DnsAnatomyRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsPipelineHeadline {
    pub full_signature_bytes_www_a: Option<u64>,
    pub condensed_proof_bytes_www_a: Option<u64>,
    pub reduction_percent_www_a: Option<f64>,
    pub notes: Option<String>,
    /// Provenance of these numbers.
    pub source: DnsCaptureSource,
    /// RFC 3339 timestamp of the recording, when known.
    pub recorded_at: Option<String>,
    /// Signature/hash algorithm these numbers were produced with, when known.
    /// Sourced from the DNS capture (never synthesized); `#[serde(default)]`
    /// keeps older results files that predate this field parseable.
    #[serde(default)]
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseCurvePoint {
    pub reuse: u64,
    pub effective_time_ns: f64,
    pub effective_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseResult {
    pub size: String,
    pub batch_size: u64,
    pub curve: Vec<ReuseCurvePoint>,
    pub breakeven_reuse_time: Option<u64>,
    pub breakeven_reuse_bytes: Option<u64>,
    pub r_star_time: Option<u64>,
    pub r_star_bytes: Option<u64>,
    pub baseline_time_ns: f64,
    pub baseline_bytes: f64,
    pub class: EvidenceClass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub overall_wire_reduction_percent: Option<f64>,
    pub largest_wire_reduction_percent: Option<f64>,
    pub overall_verifying_trust_cached_speedup_avg: Option<f64>,
    pub headline: Option<String>,
    #[serde(default)]
    pub reuse: Vec<ReuseResult>,
    /// Analytical DNS-over-UDP fit and TCP-fallback model, or `None` when no
    /// plain signature length was available to model with. `#[serde(default)]`
    /// keeps results written before this section existed parseable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp_fit: Option<UdpFitSection>,
    /// Compact signature-context table: classical baselines beside
    /// configurations A/B/C at the headline (Medium) dataset size. Empty
    /// when no classical rows were consolidated — the table exists to frame
    /// the post-quantum numbers with classical context, so it is emitted
    /// only when at least one classical context row is present.
    /// `#[serde(default)]` keeps results written before this section
    /// existed parseable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature_context: Vec<SignatureContextRow>,
}

/// One row of the compact signature-context table: a classical baseline or
/// one of the post-quantum configurations A/B/C, side by side at the
/// headline dataset size (Medium — the largest size whose wire report and
/// modelled udp-fit rows exist).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureContextRow {
    /// Machine configuration id (e.g. `classical-ed25519`, `plain`), matching
    /// the udp-fit section's ids.
    pub config: String,
    /// Display label (e.g. `Ed25519`, `B (MTL non-amortised)`).
    pub label: String,
    /// Family: `classical (context)` for the baselines, `post-quantum` for
    /// A/B/C.
    pub family: String,
    /// Dataset size label these numbers were taken at (title-cased).
    pub size: String,
    /// Signature bytes carried per message on the wire: the full signature
    /// for the classical baselines and A, the full signature plus ladder
    /// share for B, the ladder-plus-condensed share for C.
    pub signature_bytes: Option<f64>,
    /// Per-message signing mean in nanoseconds.
    pub signing_mean_ns: Option<f64>,
    /// Per-message verifying mean in nanoseconds.
    pub verifying_mean_ns: Option<f64>,
    /// Share of modelled responses that fit 1,232 bytes over UDP at this
    /// size (percent), read from the udp-fit section. Modelled
    /// (ANALYTICAL) regardless of the row's `class`, which covers the
    /// measured timing/length inputs only.
    pub fits_1232_percent: Option<f64>,
    /// Weakest evidence class among the row's measured timing/length inputs.
    pub class: EvidenceClass,
}

/// One modelled UDP-fit row: a (profile, config, dataset size) triple.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UdpFitRowArtifact {
    /// Response-profile name (stable id from the profile data).
    pub profile: String,
    /// Human label of the response profile.
    pub profile_label: String,
    /// Where the profile's overheads came from: `captured` or `parameter`.
    pub profile_source: String,
    /// Signature configuration id: `plain`, `mtl-full`, or `mtl-amortised`.
    pub config: String,
    /// Human label of the signature configuration.
    pub config_label: String,
    /// Zone leaf count (one batch signs one ladder).
    pub zone_leaves: u64,
    /// Messages the band counts are distributed over.
    pub messages: u64,
    /// Modelled largest response length in bytes.
    pub max_response_bytes: u64,
    /// Messages whose response fits 1,232 bytes.
    pub fit_1232_count: u64,
    /// Percentage of messages fitting 1,232 bytes.
    pub fit_1232_percent: f64,
    /// Messages in the 1,232 < x ≤ 4,096 TC band.
    pub tc_band_count: u64,
    /// Percentage of messages in the TC band.
    pub tc_band_percent: f64,
    /// Messages whose response exceeds 4,096 bytes.
    pub over_4096_count: u64,
    /// Percentage of messages whose response needs TCP.
    pub needs_tcp_percent: f64,
    /// TCP fallback round trips (UDP query + TC, handshake, TCP exchange).
    pub tcp_round_trips: usize,
    /// Bytes of the original UDP query.
    pub tcp_query_bytes: usize,
    /// TCP payload bytes (response + 4-byte length framing).
    pub tcp_payload_bytes: usize,
    /// Modelled fallback delay, only present when an RTT was supplied.
    pub tcp_modeled_ms: Option<f64>,
    /// Evidence class: always `analytical` for this section.
    pub class: EvidenceClass,
}

/// Cross-check of the model against the recorded capture, at zero tolerance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UdpFitValidation {
    /// Zone leaf count inferred from the capture's own signature fields.
    pub zone_leaves: u64,
    /// Plain SLH-DSA signature length the model was fed (from the artifact).
    pub plain_signature_len_bytes: u64,
    /// Full-response wire length recorded in the capture.
    pub recorded_full_response_bytes: u64,
    /// Full-response length produced by the model at the inferred zone size.
    pub modeled_full_response_bytes: u64,
    /// Condensed-response wire length recorded in the capture.
    pub recorded_condensed_response_bytes: u64,
    /// Condensed-response length produced by the model at the inferred zone
    /// size.
    pub modeled_condensed_response_bytes: u64,
    /// Allowed difference in bytes; always 0 — a mismatch is reported, never
    /// rounded away.
    pub tolerance_bytes: u64,
    /// Whether both modelled lengths equal the recorded ones exactly.
    pub matches: bool,
}

/// The analytical UDP-fit section: one row per (profile, config, size) plus
/// the capture cross-check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UdpFitSection {
    /// Modelled rows, in (profile, config, size) order.
    pub rows: Vec<UdpFitRowArtifact>,
    /// Model-vs-capture cross-check, when the capture carried signature
    /// fields to check against.
    pub validation: Option<UdpFitValidation>,
    /// RTT assumption (ms) the modelled TCP-fallback delay was computed
    /// with, or `None` when no RTT was supplied (and so no delay modelled).
    /// `#[serde(default)]` keeps results written before this field existed
    /// parseable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimbResults {
    pub version: String,
    pub generated_at: String,
    pub workspace_root: String,
    pub benchmark: BenchmarkResults,
    pub wire_size: Vec<WireSizeResult>,
    pub memory: MemoryResults,
    pub environment: EnvironmentFingerprint,
    /// DNS pipeline headline, or `None` when no capture (live or recorded
    /// fixture) is available.
    pub dns_pipeline: Option<DnsPipelineHeadline>,
    /// Per-byte anatomy of the headline `www.climb.example. A` response pair,
    /// parsed from the capture's embedded wire bytes. `None` when no capture
    /// carries wire bytes (older recordings only store counts). `#[serde(default)]`
    /// keeps results written before this section existed parseable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_anatomy: Option<DnsAnatomySection>,
    /// Recorded ladder-verification section: measured ladder shape per size
    /// plus the model-agreement verdict, or `None` when neither a live run nor
    /// the committed fixture is available. `#[serde(default)]` keeps results
    /// written before this section existed parseable.
    #[serde(default)]
    pub ladder: Option<LadderSection>,
    /// Configuration A (plain SLH-DSA) per-message timings, one row per dataset
    /// size. Empty for results written before configuration A was routed here.
    #[serde(default)]
    pub plain: Vec<PlainTimingResult>,
    /// Classical signature baselines (context only, not PQC). Empty for results
    /// written before the baselines were captured.
    #[serde(default)]
    pub classical: Vec<ClassicalTimingResult>,
    /// Three-way configuration comparisons (C vs A, C vs B). Empty for results
    /// written before the comparison was derived.
    #[serde(default)]
    pub comparisons: Vec<ConfigComparison>,
    pub derived: DerivedSummary,
    /// Non-fatal consolidation warnings (missing or stale optional inputs).
    ///
    /// These are surfaced in the consolidated output so a reader can tell an
    /// absent section apart from a genuinely empty one. An empty list means
    /// every requested optional input was present and fresh.
    pub warnings: Vec<String>,
}

impl ClimbResults {
    /// The three-way comparison row for `size`, matched by its size field.
    ///
    /// The comparison array is ordered by declaration, not by size; a consumer
    /// that indexes it by position can silently point a dataset's number at
    /// another dataset's row. Look the row up by its size label instead.
    pub fn comparison_for(&self, size: &str) -> Option<&ConfigComparison> {
        find_by_size(&self.comparisons, size, |row| &row.size)
    }

    /// The wire-size row for `size`, matched case-insensitively (the wire
    /// section stores its label lowercase, the comparison section title-cased).
    pub fn wire_for(&self, size: &str) -> Option<&WireSizeResult> {
        find_by_size(&self.wire_size, size, |row| &row.size)
    }

    /// The ladder-reuse row for `size`, matched case-insensitively.
    pub fn reuse_for(&self, size: &str) -> Option<&ReuseResult> {
        find_by_size(&self.derived.reuse, size, |row| &row.size)
    }

    /// The measured-memory row for `size`, matched case-insensitively.
    pub fn memory_for(&self, size: &str) -> Option<&MemoryResult> {
        find_by_size(&self.memory.measured, size, |row| &row.size)
    }
}

#[derive(Debug, Clone)]
pub struct ConsolidateOptions {
    pub force: bool,
    pub criterion_dir: PathBuf,
    pub wire_report_path: Option<PathBuf>,
    pub memory_report_path: Option<PathBuf>,
    /// Live DNS capture written by `cargo xtask dns-demo --record`
    /// (preferred when present).
    pub dns_capture_path: PathBuf,
    /// Committed recorded fixture used when no live capture exists.
    pub dns_fixture_path: PathBuf,
    /// Live ladder-verification artifact written by `cargo xtask ladder-verify`
    /// (preferred when present).
    pub ladder_verification_path: PathBuf,
    /// Committed recorded ladder-verification fixture used when no live run
    /// artifact exists.
    pub ladder_fixture_path: PathBuf,
    pub environment_path: PathBuf,
    pub baseline: String,
    /// Assumed round-trip time in milliseconds for the modelled TCP fallback
    /// cost; without it the fallback delay stays unmodelled.
    pub rtt_ms: Option<f64>,
    /// Classical signature lengths recorded by the classical benchmark run.
    /// When absent while classical Criterion records exist, consolidation
    /// warns and omits every classical context row rather than guessing
    /// lengths; present-but-invalid is a hard error naming the path.
    pub classical_signatures_path: PathBuf,
    pub output_path: PathBuf,
}

/// Pure freshness decision for an already-stat'd input file.
///
/// `force` short-circuits to `true` (the caller explicitly asked to accept the
/// input regardless of age). A `modified` timestamp in the future (i.e. `now`
/// is before `modified`, which is what `SystemTime::duration_since` reports as
/// `Err`) is treated as *not* fresh, matching the prior behavior.
fn input_is_fresh(modified: SystemTime, now: SystemTime, max_age: Duration, force: bool) -> bool {
    if force {
        return true;
    }
    match now.duration_since(modified) {
        Ok(age) => age <= max_age,
        Err(_) => false,
    }
}

/// Stat `path` and apply [`input_is_fresh`]. A missing file or unreadable
/// modification time is reported as not fresh.
fn path_is_fresh(path: &Path, max_age: Duration, force: bool) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(m) => m,
        Err(_) => return false,
    };
    input_is_fresh(modified, SystemTime::now(), max_age, force)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ReportError> {
    let s = fs::read_to_string(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    serde_json::from_str(&s).map_err(|e| ReportError::JsonDecode {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Batch summary rows plus the records deliberately kept out of them: the
/// plain configuration-A records and the classical context baselines, both
/// timed per operation rather than per batch.
type SummaryScan = (Vec<SummaryRow>, Vec<CriterionRecord>, Vec<CriterionRecord>);

/// Scans from the criterion root (same traversal as `cargo xtask report`);
/// unrecognized group ids from stale layouts are dropped instead of failing.
///
/// Returns the batch-oriented summary rows plus the configuration-A plain
/// records, which are parsed separately because they carry a per-message cost.
fn read_summary_rows(criterion_dir: &Path, baseline: &str) -> Result<SummaryScan, ReportError> {
    let records = scan_criterion(criterion_dir, baseline)?;
    let mut plain: Vec<CriterionRecord> = Vec::new();
    let mut classical: Vec<CriterionRecord> = Vec::new();
    let mut mtl: Vec<CriterionRecord> = Vec::new();
    for rec in records {
        if crate::summary::parse_plain_group_id(&rec.group_id).is_some() {
            plain.push(rec);
        } else if crate::summary::parse_classical_group_id(&rec.group_id).is_some() {
            classical.push(rec);
        } else {
            mtl.push(rec);
        }
    }
    let known: Vec<CriterionRecord> = mtl
        .into_iter()
        .filter(|rec| crate::summary::parse_group_id(&rec.group_id).is_ok())
        .collect();
    Ok((summarize(&known)?, plain, classical))
}

/// Pure derivation of the headline metrics from the collected sections; every
/// output is `None` when its inputs are absent.
fn derive_summary(
    wire_size: &[WireSizeResult],
    signing: &[SigningBenchmarkResult],
    verifying: &[VerifyingBenchmarkResult],
    plain: &[PlainTimingResult],
    dns_pipeline: Option<&DnsPipelineHeadline>,
) -> DerivedSummary {
    let mut overall_wire_reduction: Option<f64> = None;
    let mut largest_wire_reduction: Option<f64> = None;
    if !wire_size.is_empty() {
        let mut sum = 0.0f64;
        let mut max = f64::NEG_INFINITY;
        for w in wire_size {
            sum += w.reduction_percent;
            if w.reduction_percent > max {
                max = w.reduction_percent;
            }
        }
        overall_wire_reduction = Some(sum / wire_size.len() as f64);
        largest_wire_reduction = Some(if max.is_finite() { max } else { 0.0 });
    }

    let cached: Vec<f64> = verifying
        .iter()
        .filter(|v| v.condition.contains("trust cached"))
        .map(|v| {
            let mean_with = signing
                .iter()
                .find(|s| s.size == v.size && s.condition.contains("With MTL"))
                .map(|s| s.mean_ns)
                .unwrap_or(1.0);
            if mean_with > 0.0 {
                mean_with / v.mean_ns
            } else {
                0.0
            }
        })
        .collect();
    let trust_cached_avg = if cached.is_empty() {
        None
    } else {
        Some(cached.iter().sum::<f64>() / cached.len() as f64)
    };

    let reuse = build_reuse(wire_size, verifying, plain);

    DerivedSummary {
        overall_wire_reduction_percent: overall_wire_reduction,
        largest_wire_reduction_percent: largest_wire_reduction,
        overall_verifying_trust_cached_speedup_avg: trust_cached_avg,
        headline: dns_pipeline.and_then(headline_from_dns),
        reuse,
        // Attached by `consolidate_results` once the plain signature length is
        // known; the pure derivation has no input for it.
        udp_fit: None,
        // Likewise attached by `consolidate_results`, which has the classical
        // rows and the udp-fit section to build the context table from.
        signature_context: Vec::new(),
    }
}

/// Cold/warm ladder-reuse curve for every size that has a wire row, a pair of
/// verifying conditions, and a plain per-message baseline.
///
/// `t_cold` is the per-message verify cost with a fresh ladder and `t_warm` the
/// cost when the ladder is trusted from cache; reusing one ladder across `r`
/// messages amortizes the cold verify over the batch, and the shared ladder
/// replaces one ladder per message. Every input is measured (or explicitly
/// extrapolated), so a size whose inputs are not all measured carries the
/// `Extrapolated` class rather than a fabricated precision.
fn build_reuse(
    wire_size: &[WireSizeResult],
    verifying: &[VerifyingBenchmarkResult],
    plain: &[PlainTimingResult],
) -> Vec<ReuseResult> {
    wire_size
        .iter()
        .filter_map(|w| {
            let cold = verifying_per_message(verifying, &w.size, "With MTL");
            let warm = verifying_per_message(verifying, &w.size, "MTL (trust cached)");
            let (Some(t_cold), Some(t_warm)) = (cold.value, warm.value) else {
                return None;
            };
            let baseline_time_ns = plain
                .iter()
                .find(|p| p.size.eq_ignore_ascii_case(&w.size))
                .and_then(|p| p.verifying_mean_ns)?;
            let baseline_bytes = w.plain_signature_len_bytes as f64;
            if baseline_bytes <= 0.0 {
                return None;
            }
            let model = climb_analytical::reuse_model(climb_analytical::ReuseInput {
                t_cold,
                t_warm,
                s_cond: w.condensed_mean,
                s_ladder: w.ladder_len as f64,
                batch_size: w.message_count as f64,
                baseline_time_ns,
                baseline_bytes,
            });
            let extrapolated = cold.extrapolated
                || warm.extrapolated
                || plain
                    .iter()
                    .find(|p| p.size.eq_ignore_ascii_case(&w.size))
                    .is_some_and(|p| p.class == EvidenceClass::Extrapolated);
            Some(ReuseResult {
                size: w.size.clone(),
                batch_size: model.batch_size,
                curve: model
                    .points
                    .iter()
                    .map(|p| ReuseCurvePoint {
                        reuse: p.reuse,
                        effective_time_ns: p.effective_time_ns,
                        effective_bytes: p.effective_bytes,
                    })
                    .collect(),
                breakeven_reuse_time: model.breakeven_reuse_time,
                breakeven_reuse_bytes: model.breakeven_reuse_bytes,
                r_star_time: model.r_star_time,
                r_star_bytes: model.r_star_bytes,
                baseline_time_ns,
                baseline_bytes,
                class: if extrapolated {
                    EvidenceClass::Extrapolated
                } else {
                    EvidenceClass::Derived
                },
            })
        })
        .collect()
}

/// Artifact-stable configuration id for a signature config.
const fn config_id(config: climb_analytical::SignatureConfig) -> &'static str {
    match config {
        climb_analytical::SignatureConfig::Plain => "plain",
        climb_analytical::SignatureConfig::MtlFull => "mtl-full",
        climb_analytical::SignatureConfig::MtlAmortised => "mtl-amortised",
        climb_analytical::SignatureConfig::ClassicalEd25519 => "classical-ed25519",
        climb_analytical::SignatureConfig::ClassicalEcdsaP256 => "classical-ecdsa-p256",
        climb_analytical::SignatureConfig::ClassicalRsa2048 => "classical-rsa2048",
    }
}

/// Build the analytical UDP-fit section: one row per (profile, config,
/// dataset size), plus the cross-check against the recorded capture.
///
/// The plain SLH-DSA signature length is read from the first wire row that
/// carries one; rows that disagree on it produce a warning and the first
/// value is used. With no plain length at all there is nothing to model
/// configurations A and B with, so the whole section is `None` and a warning
/// says why — an absent section, never a fabricated one.
///
/// The capture cross-check feeds the model the capture's own inferred zone
/// size and profile and compares the modelled response lengths against the
/// recorded wire lengths at zero tolerance. A mismatch is a warning, not a
/// hard error: the plain length may legitimately have drifted from the
/// capture's algorithm parameter set since the fixture was recorded.
fn build_udp_fit(
    wire_size: &[WireSizeResult],
    classical_lengths: Option<&ClassicalSignatureLengths>,
    rtt_ms: Option<f64>,
    warnings: &mut Vec<String>,
) -> Option<UdpFitSection> {
    if wire_size.is_empty() {
        // No wire report at all (never supplied or its absence already
        // reported): nothing to model with, so the section is simply absent.
        return None;
    }
    let mut plain_len: Option<u64> = None;
    for w in wire_size {
        if w.plain_signature_len_bytes == 0 {
            continue;
        }
        match plain_len {
            None => plain_len = Some(w.plain_signature_len_bytes),
            Some(known) if known != w.plain_signature_len_bytes => {
                warnings.push(format!(
                    "udp fit: wire rows disagree on the plain signature length ({known} vs {}); using the first",
                    w.plain_signature_len_bytes
                ));
            }
            Some(_) => {}
        }
    }
    let Some(plain_len) = plain_len else {
        warnings.push(
            "udp fit: no wire row carries a plain signature length; section omitted".to_string(),
        );
        return None;
    };
    let Ok(plain_usize) = usize::try_from(plain_len) else {
        warnings.push(format!(
            "udp fit: plain signature length {plain_len} is out of range; section omitted"
        ));
        return None;
    };

    // A/B/C carry the plain SLH-DSA length; each classical context config
    // carries its own measured length. Classical configs appear only when
    // their lengths were recorded (a missing document was already warned
    // about by the caller).
    let mut configs: Vec<(climb_analytical::SignatureConfig, usize)> = vec![
        (climb_analytical::SignatureConfig::Plain, plain_usize),
        (climb_analytical::SignatureConfig::MtlFull, plain_usize),
        (climb_analytical::SignatureConfig::MtlAmortised, plain_usize),
    ];
    if let Some(lengths) = classical_lengths {
        for (config, len) in [
            (
                climb_analytical::SignatureConfig::ClassicalEd25519,
                lengths.ed25519,
            ),
            (
                climb_analytical::SignatureConfig::ClassicalEcdsaP256,
                lengths.ecdsa_p256,
            ),
            (
                climb_analytical::SignatureConfig::ClassicalRsa2048,
                lengths.rsa2048,
            ),
        ] {
            // A zero length cannot come from a produced signature; the
            // loader rejects such documents, so this is defence in depth.
            if len > 0 {
                configs.push((config, len));
            }
        }
    }
    let mut rows = Vec::new();
    for profile in climb_analytical::builtin_profiles() {
        for &(config, sig_len) in &configs {
            for size in climb_analytical::COMPARISON_SIZES {
                match climb_analytical::udp_fit_for_size(config, &profile, size, size, sig_len) {
                    Ok(row) => {
                        let tcp = climb_analytical::tcp_fallback_cost(
                            row.max_bytes,
                            profile.question_bytes,
                            rtt_ms,
                        );
                        rows.push(UdpFitRowArtifact {
                            profile: profile.name.clone(),
                            profile_label: profile.label.clone(),
                            profile_source: match profile.source {
                                climb_analytical::ProfileSource::Captured => "captured",
                                climb_analytical::ProfileSource::Parameter => "parameter",
                            }
                            .to_string(),
                            config: config_id(config).to_string(),
                            config_label: config.label().to_string(),
                            zone_leaves: row.size_n,
                            messages: row.total_messages,
                            max_response_bytes: row.max_bytes as u64,
                            fit_1232_count: row.fit_1232_count,
                            fit_1232_percent: row.fit_1232_percent,
                            tc_band_count: row.tc_band_count,
                            tc_band_percent: row.tc_band_percent,
                            over_4096_count: row.over_4096_count,
                            needs_tcp_percent: row.needs_tcp_percent,
                            tcp_round_trips: tcp.round_trips,
                            tcp_query_bytes: tcp.udp_query_bytes,
                            tcp_payload_bytes: tcp.tcp_payload_bytes,
                            tcp_modeled_ms: tcp.ms_modeled,
                            class: EvidenceClass::Analytical,
                        });
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "udp fit: profile {} / config {} / size {size} failed: {e}",
                            profile.name,
                            config_id(config)
                        ));
                    }
                }
            }
        }
    }

    let validation = validate_udp_fit_against_capture(plain_len, plain_usize, warnings);

    Some(UdpFitSection {
        rows,
        validation,
        rtt_ms,
    })
}

/// Cross-check the model against the recorded capture's headline responses.
///
/// The capture's full and condensed passes each carry `rrsig_count` matching
/// signature fields; their lengths plus the plain signature length pin the
/// zone size (a single-rung ladder). The model is then run at that zone size
/// with the captured with-NS profile and its response lengths compared
/// against the capture's recorded wire lengths, at zero tolerance.
fn validate_udp_fit_against_capture(
    plain_len: u64,
    plain_usize: usize,
    warnings: &mut Vec<String>,
) -> Option<UdpFitValidation> {
    let (full, condensed) = climb_dns::recorded_headline_anatomies();
    let (Some(full_field), Some(cond_field)) = (
        full.rrsig_entries.first().map(|e| e.signature_len),
        condensed.rrsig_entries.first().map(|e| e.signature_len),
    ) else {
        warnings.push(
            "udp fit: recorded capture carries no signature fields; validation omitted".to_string(),
        );
        return None;
    };
    let (Ok(full_usize), Ok(cond_usize)) =
        (usize::try_from(full_field), usize::try_from(cond_field))
    else {
        warnings.push(
            "udp fit: recorded signature fields are out of range; validation omitted".to_string(),
        );
        return None;
    };
    let Some(zone_leaves) =
        climb_analytical::infer_zone_leaf_count(full_usize, cond_usize, plain_usize)
    else {
        warnings.push(format!(
            "udp fit: cannot infer the capture's zone size from full={full_field} condensed={cond_field} plain={plain_len}; validation omitted"
        ));
        return None;
    };
    let Some(profile) = climb_analytical::builtin_profiles()
        .into_iter()
        .find(|p| p.name == "positive_a_with_ns")
    else {
        warnings.push(
            "udp fit: captured with-NS profile missing from the profile data; validation omitted"
                .to_string(),
        );
        return None;
    };
    let full_row = climb_analytical::udp_fit_for_size(
        climb_analytical::SignatureConfig::MtlFull,
        &profile,
        zone_leaves,
        zone_leaves,
        plain_usize,
    );
    let cond_row = climb_analytical::udp_fit_for_size(
        climb_analytical::SignatureConfig::MtlAmortised,
        &profile,
        zone_leaves,
        zone_leaves,
        plain_usize,
    );
    let (Ok(full_row), Ok(cond_row)) = (full_row, cond_row) else {
        warnings.push(
            "udp fit: model failed at the capture's inferred zone size; validation omitted"
                .to_string(),
        );
        return None;
    };
    let matches =
        full_row.max_bytes == full.wire_length && cond_row.max_bytes == condensed.wire_length;
    if !matches {
        warnings.push(format!(
            "udp fit: model does not reproduce the recorded wire lengths (full {} vs recorded {}, condensed {} vs recorded {}); the plain signature length may have drifted since the capture",
            full_row.max_bytes, full.wire_length, cond_row.max_bytes, condensed.wire_length
        ));
    }
    Some(UdpFitValidation {
        zone_leaves,
        plain_signature_len_bytes: plain_len,
        recorded_full_response_bytes: full.wire_length as u64,
        modeled_full_response_bytes: full_row.max_bytes as u64,
        recorded_condensed_response_bytes: condensed.wire_length as u64,
        modeled_condensed_response_bytes: cond_row.max_bytes as u64,
        tolerance_bytes: 0,
        matches,
    })
}

/// Dataset sizes and message counts used for configuration A and the
/// three-way comparison, in ascending order.
const COMPARISON_SIZES: [(crate::summary::Size, &str, u64); 4] = [
    (crate::summary::Size::Small, "Small", 100),
    (crate::summary::Size::OneK, "OneK", 1_000),
    (crate::summary::Size::Medium, "Medium", 10_000),
    (crate::summary::Size::Large, "Large", 1_000_000),
];

/// Per-message cost for one condition: measured for its own size when a row
/// exists, otherwise the mean of the condition's measured per-message costs.
struct PerMessage {
    value: Option<f64>,
    extrapolated: bool,
}

fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0_f64, 0u32), |(s, n), v| (s + v, n + 1));
    if count == 0 {
        None
    } else {
        Some(sum / f64::from(count))
    }
}

/// `(from − to) / from × 100`, or `None` when `from` is zero.
fn percent_reduction(from: u64, to: u64) -> Option<f64> {
    if from == 0 {
        None
    } else {
        Some((from as f64 - to as f64) / from as f64 * 100.0)
    }
}

/// `a / c`, or `None` unless both are present and `c` is positive.
fn ratio(a: Option<f64>, c: Option<f64>) -> Option<f64> {
    match (a, c) {
        (Some(a), Some(c)) if c > 0.0 => Some(a / c),
        _ => None,
    }
}

/// Per-message cost for one condition of one dataset size, falling back to the
/// mean over the condition's measured sizes when the size itself was not run.
fn per_message<'a>(
    rows: impl Iterator<Item = (&'a str, &'a str, u64, f64)>,
    size: &str,
    condition: &str,
) -> PerMessage {
    let rows: Vec<(&str, &str, u64, f64)> = rows
        .filter(|(_, c, m, mean)| *c == condition && *m > 0 && *mean > 0.0)
        .collect();
    if let Some((_, _, messages, mean)) = rows
        .iter()
        .find(|(s, _, _, _)| s.eq_ignore_ascii_case(size))
    {
        return PerMessage {
            value: Some(mean / *messages as f64),
            extrapolated: false,
        };
    }
    let value = average(rows.iter().map(|(_, _, m, mean)| mean / *m as f64));
    PerMessage {
        extrapolated: value.is_some(),
        value,
    }
}

fn signing_per_message(rows: &[SigningBenchmarkResult], size: &str, condition: &str) -> PerMessage {
    per_message(
        rows.iter()
            .map(|r| (r.size.as_str(), r.condition.as_str(), r.messages, r.mean_ns)),
        size,
        condition,
    )
}

fn verifying_per_message(
    rows: &[VerifyingBenchmarkResult],
    size: &str,
    condition: &str,
) -> PerMessage {
    per_message(
        rows.iter()
            .map(|r| (r.size.as_str(), r.condition.as_str(), r.messages, r.mean_ns)),
        size,
        condition,
    )
}

/// Configuration A per-message timings, one row per dataset size. Sizes with a
/// measured plain group are reported as measured; the rest carry the mean of
/// the measured samples, marked extrapolated with the sample count `k`.
fn plain_timings(records: &[CriterionRecord]) -> Vec<PlainTimingResult> {
    use std::collections::BTreeMap;
    let mut signing: BTreeMap<crate::summary::Size, (f64, f64, f64)> = BTreeMap::new();
    let mut verifying: BTreeMap<crate::summary::Size, (f64, f64, f64)> = BTreeMap::new();
    for rec in records {
        let Some((kind, size)) = crate::summary::parse_plain_group_id(&rec.group_id) else {
            continue;
        };
        let entry = (rec.mean_ns, rec.mean_ci_lower_ns, rec.mean_ci_upper_ns);
        match kind {
            crate::summary::BenchKind::Signing => {
                let _ = signing.insert(size, entry);
            }
            crate::summary::BenchKind::Verifying => {
                let _ = verifying.insert(size, entry);
            }
        }
    }
    if signing.is_empty() && verifying.is_empty() {
        return Vec::new();
    }
    let sign_avg = average(signing.values().map(|e| e.0));
    let verif_avg = average(verifying.values().map(|e| e.0));
    let samples = signing.len().max(verifying.len()) as u32;

    COMPARISON_SIZES
        .iter()
        .filter_map(|(size, label, messages)| {
            let measured_sign = signing.get(size).copied();
            let measured_verif = verifying.get(size).copied();
            let measured = measured_sign.is_some() || measured_verif.is_some();
            let signing_mean_ns = measured_sign.map(|e| e.0).or(sign_avg);
            let verifying_mean_ns = measured_verif.map(|e| e.0).or(verif_avg);
            if signing_mean_ns.is_none() && verifying_mean_ns.is_none() {
                return None;
            }
            Some(PlainTimingResult {
                size: (*label).to_string(),
                messages: *messages,
                signing_mean_ns,
                signing_ci_lower_ns: measured_sign.map(|e| e.1),
                signing_ci_upper_ns: measured_sign.map(|e| e.2),
                verifying_mean_ns,
                verifying_ci_lower_ns: measured_verif.map(|e| e.1),
                verifying_ci_upper_ns: measured_verif.map(|e| e.2),
                class: if measured {
                    EvidenceClass::Measured
                } else {
                    EvidenceClass::Extrapolated
                },
                samples: if measured { None } else { Some(samples) },
            })
        })
        .collect()
}

/// Three-way comparisons for every size that has a configuration-A timing row.
/// A row is marked `extrapolated` when any input (A's per-message timing, or B's
/// per-message cost) was itself not measured for that size.
fn build_comparisons(
    wire_size: &[WireSizeResult],
    signing: &[SigningBenchmarkResult],
    verifying: &[VerifyingBenchmarkResult],
    plain: &[PlainTimingResult],
) -> Vec<ConfigComparison> {
    plain
        .iter()
        .map(|p| {
            let size = p.size.as_str();
            let wire = wire_size.iter().find(|w| w.size.eq_ignore_ascii_case(size));
            let wire_c_vs_b = wire.map(|w| w.reduction_percent);
            let wire_c_vs_a =
                wire.and_then(|w| percent_reduction(w.plain_total_bytes, w.with_mtl_total_bytes));

            let c_sign = signing_per_message(signing, size, "With MTL");
            let b_sign = signing_per_message(signing, size, "Without MTL");
            let c_cached = verifying_per_message(verifying, size, "MTL (trust cached)");
            let c_cold = verifying_per_message(verifying, size, "With MTL");

            let extrapolated = p.class == EvidenceClass::Extrapolated
                || b_sign.extrapolated
                || c_sign.extrapolated
                || c_cached.extrapolated
                || c_cold.extrapolated;
            ConfigComparison {
                size: p.size.clone(),
                wire_reduction_c_vs_a_percent: wire_c_vs_a,
                wire_reduction_c_vs_b_percent: wire_c_vs_b,
                signing_speedup_c_vs_a: ratio(p.signing_mean_ns, c_sign.value),
                signing_speedup_c_vs_b: ratio(b_sign.value, c_sign.value),
                verifying_speedup_cached_vs_a: ratio(p.verifying_mean_ns, c_cached.value),
                verifying_speedup_cold_vs_a: ratio(p.verifying_mean_ns, c_cold.value),
                class: if extrapolated {
                    EvidenceClass::Extrapolated
                } else {
                    EvidenceClass::Derived
                },
            }
        })
        .collect()
}

/// True when a Criterion record's benchmark is the verify function of its
/// group: the classical groups use `BenchmarkId::from_parameter`, so
/// Criterion writes the parameter string (`sign`/`verify`) as `value_str`
/// with a null `function_id`; a function-id layout is accepted too.
fn is_verify_record(rec: &crate::record::CriterionRecord) -> bool {
    rec.value_str == "verify" || rec.function_id.as_deref() == Some("verify")
}

/// Load the classical signature lengths, if the document was recorded.
///
/// * absent while classical Criterion records exist → `None` plus a
///   warning (consolidation then carries no classical rows anywhere — a
///   length is never typed in);
/// * absent with no classical records → `None`, silently (this run simply
///   never measured classical baselines);
/// * present but unparseable (missing field, wrong shape, bad number, zero
///   length) → hard error naming the path (a present-but-invalid artifact
///   is never silently ignored).
fn load_classical_signature_lengths(
    path: &Path,
    records: &[crate::record::CriterionRecord],
    warnings: &mut Vec<String>,
) -> Result<Option<ClassicalSignatureLengths>, ReportError> {
    if !path.exists() {
        if !records.is_empty() {
            warnings.push(format!(
                "classical signature lengths not recorded at {}; classical context rows omitted",
                path.display()
            ));
        }
        return Ok(None);
    }
    let lengths: ClassicalSignatureLengths = read_json(path)?;
    for (name, len) in [
        ("ed25519", lengths.ed25519),
        ("ecdsa_p256", lengths.ecdsa_p256),
        ("rsa2048", lengths.rsa2048),
    ] {
        if len == 0 {
            return Err(ReportError::MalformedData {
                path: path.to_path_buf(),
                reason: format!("classical {name} signature length is 0; expected bytes"),
            });
        }
    }
    Ok(Some(lengths))
}

fn classical_timings(
    records: &[crate::record::CriterionRecord],
    lengths: &ClassicalSignatureLengths,
) -> Vec<ClassicalTimingResult> {
    use crate::summary::{parse_classical_group_id, ClassicalAlgo};
    use std::collections::{BTreeMap, BTreeSet};
    let mut signing: BTreeMap<(ClassicalAlgo, crate::summary::Size), (f64, f64, f64)> =
        BTreeMap::new();
    let mut verifying: BTreeMap<(ClassicalAlgo, crate::summary::Size), (f64, f64, f64)> =
        BTreeMap::new();
    for rec in records {
        if let Some((algo, size)) = parse_classical_group_id(&rec.group_id) {
            let entry = (rec.mean_ns, rec.mean_ci_lower_ns, rec.mean_ci_upper_ns);
            if is_verify_record(rec) {
                let _ = verifying.insert((algo, size), entry);
            } else {
                let _ = signing.insert((algo, size), entry);
            }
        }
    }
    // Rows keyed by (algorithm, size) in artifact order: algorithm in
    // declaration order, size ascending — no name-based sort that could
    // drift from the presentation order.
    let keys: BTreeSet<(ClassicalAlgo, crate::summary::Size)> =
        signing.keys().chain(verifying.keys()).copied().collect();
    let mut out: Vec<ClassicalTimingResult> = Vec::new();
    for (algo, size) in keys {
        let Some((_, label, messages)) = COMPARISON_SIZES.iter().find(|(s, _, _)| *s == size)
        else {
            continue;
        };
        let skey = signing.get(&(algo, size));
        let vkey = verifying.get(&(algo, size));
        out.push(ClassicalTimingResult {
            algorithm: algo.to_string(),
            size: (*label).to_string(),
            messages: *messages,
            signing_mean_ns: skey.map(|e| e.0),
            signing_ci_lower_ns: skey.map(|e| e.1),
            signing_ci_upper_ns: skey.map(|e| e.2),
            verifying_mean_ns: vkey.map(|e| e.0),
            verifying_ci_lower_ns: vkey.map(|e| e.1),
            verifying_ci_upper_ns: vkey.map(|e| e.2),
            signature_len_bytes: lengths.for_algo(algo),
            class: EvidenceClass::Measured,
        });
    }
    out
}

/// Weaker (less direct) of two evidence classes.
fn weakest_class(a: EvidenceClass, b: EvidenceClass) -> EvidenceClass {
    fn rank(c: EvidenceClass) -> u8 {
        match c {
            EvidenceClass::Measured => 0,
            EvidenceClass::Extrapolated => 1,
            EvidenceClass::Analytical => 2,
            EvidenceClass::Derived => 3,
        }
    }
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

/// The evidence class of one measured input, or `None` when the input is
/// absent (an absent cell has no class of its own).
fn per_message_class(pm: &PerMessage) -> Option<EvidenceClass> {
    pm.value.map(|_| {
        if pm.extrapolated {
            EvidenceClass::Extrapolated
        } else {
            EvidenceClass::Measured
        }
    })
}

/// The signature configuration a classical timing row belongs to, keyed by
/// the row's algorithm display name (whose only producer is
/// [`crate::summary::ClassicalAlgo`]'s `Display`).
fn classical_signature_config(algorithm: &str) -> Option<climb_analytical::SignatureConfig> {
    match algorithm {
        "Ed25519" => Some(climb_analytical::SignatureConfig::ClassicalEd25519),
        "ECDSA P-256" => Some(climb_analytical::SignatureConfig::ClassicalEcdsaP256),
        "RSA-2048" => Some(climb_analytical::SignatureConfig::ClassicalRsa2048),
        _ => None,
    }
}

/// Response profile the context table's modelled fit cell is read from.
const CONTEXT_FIT_PROFILE: &str = "positive_a";

/// Build the compact signature-context table: classical baselines first
/// (artifact order), then A, B, C, all at the headline dataset size
/// (Medium — the largest size whose wire report and modelled udp-fit rows
/// exist, where every input is directly measured for the classical and
/// plain rows).
///
/// The section is emitted only when at least one classical row exists (the
/// table exists to frame the post-quantum numbers with classical context);
/// without classical timings the comparison table already covers A/B/C.
/// The `fits_1232_percent` cell is read from the modelled udp-fit section
/// and is ANALYTICAL regardless of the row's `class`, which covers the
/// measured timing/length inputs only (documented in the field note).
#[allow(clippy::too_many_arguments)]
fn build_signature_context(
    wire_size: &[WireSizeResult],
    signing: &[SigningBenchmarkResult],
    verifying: &[VerifyingBenchmarkResult],
    plain: &[PlainTimingResult],
    classical: &[ClassicalTimingResult],
    udp_fit: Option<&UdpFitSection>,
) -> Vec<SignatureContextRow> {
    if classical.is_empty() {
        return Vec::new();
    }
    let Some((_, label, messages)) = COMPARISON_SIZES
        .iter()
        .find(|(s, _, _)| *s == crate::summary::Size::Medium)
    else {
        return Vec::new();
    };
    let fit = |config: &str| -> Option<f64> {
        udp_fit.and_then(|s| {
            s.rows
                .iter()
                .find(|r| {
                    r.profile == CONTEXT_FIT_PROFILE
                        && r.messages == *messages
                        && r.config == config
                })
                .map(|r| r.fit_1232_percent)
        })
    };
    let wire = wire_size
        .iter()
        .find(|w| w.size.eq_ignore_ascii_case(label));

    let mut rows: Vec<SignatureContextRow> = Vec::new();

    for row in classical.iter().filter(|r| r.size == *label) {
        let Some(signature_config) = classical_signature_config(&row.algorithm) else {
            continue;
        };
        let config = config_id(signature_config);
        rows.push(SignatureContextRow {
            config: config.to_string(),
            label: signature_config.label().to_string(),
            family: "classical (context)".to_string(),
            size: size_display(label),
            signature_bytes: Some(row.signature_len_bytes as f64),
            signing_mean_ns: row.signing_mean_ns,
            verifying_mean_ns: row.verifying_mean_ns,
            fits_1232_percent: fit(config),
            class: row.class,
        });
    }

    // Configuration A: the plain timing row plus the wire row's measured
    // per-message signature length.
    if let Some(p) = plain.iter().find(|p| p.size == *label) {
        if p.signing_mean_ns.is_some() || p.verifying_mean_ns.is_some() {
            let signature_bytes = wire
                .filter(|w| w.plain_signature_len_bytes > 0)
                .map(|w| w.plain_signature_len_bytes as f64);
            let mut class = p.class;
            if signature_bytes.is_some() {
                class = weakest_class(class, EvidenceClass::Measured);
            }
            let config = config_id(climb_analytical::SignatureConfig::Plain);
            rows.push(SignatureContextRow {
                config: config.to_string(),
                label: climb_analytical::SignatureConfig::Plain.label().to_string(),
                family: "post-quantum".to_string(),
                size: size_display(label),
                signature_bytes,
                signing_mean_ns: p.signing_mean_ns,
                verifying_mean_ns: p.verifying_mean_ns,
                fits_1232_percent: fit(config),
                class,
            });
        }
    }

    // Configurations B and C: per-message cost from the signing/verifying
    // benchmarks, signature bytes from the wire totals.
    let wire_per_message = |total: u64| -> Option<f64> {
        wire.filter(|w| w.message_count > 0)
            .map(|w| total as f64 / w.message_count as f64)
    };
    for (config, condition) in [
        (climb_analytical::SignatureConfig::MtlFull, "Without MTL"),
        (climb_analytical::SignatureConfig::MtlAmortised, "With MTL"),
    ] {
        let sign = signing_per_message(signing, label, condition);
        let verify = verifying_per_message(verifying, label, condition);
        let (Some(sign_value), Some(verify_value)) = (sign.value, verify.value) else {
            continue;
        };
        let total = match config {
            climb_analytical::SignatureConfig::MtlFull => wire.map(|w| w.without_mtl_total_bytes),
            _ => wire.map(|w| w.with_mtl_total_bytes),
        }
        .and_then(wire_per_message);
        let mut class = EvidenceClass::Measured;
        class = weakest_class(
            class,
            per_message_class(&sign).unwrap_or(EvidenceClass::Measured),
        );
        class = weakest_class(
            class,
            per_message_class(&verify).unwrap_or(EvidenceClass::Measured),
        );
        let config_id = config_id(config);
        rows.push(SignatureContextRow {
            config: config_id.to_string(),
            label: config.label().to_string(),
            family: "post-quantum".to_string(),
            size: size_display(label),
            signature_bytes: total,
            signing_mean_ns: Some(sign_value),
            verifying_mean_ns: Some(verify_value),
            fits_1232_percent: fit(config_id),
            class,
        });
    }

    rows
}

/// Derive the one-line headline from captured DNS numbers; `None` when the
/// capture is absent or incomplete, so the headline is never a constant.
fn headline_from_dns(headline: &DnsPipelineHeadline) -> Option<String> {
    let full = headline.full_signature_bytes_www_a?;
    let cond = headline.condensed_proof_bytes_www_a?;
    let pct = headline.reduction_percent_www_a?;
    Some(format!(
        "MTL condensed responses are ~{pct:.1}% smaller than full signatures ({} B \u{2192} {} B)",
        crate::charts::fmt_count(full as f64),
        crate::charts::fmt_count(cond as f64),
    ))
}

/// The question whose byte counts the DNS headline is computed from.
///
/// Public so every consumer (consolidation, the report, the dashboard) picks
/// the same rows instead of restating the question.
pub const HEADLINE_QNAME: &str = "www.climb.example.";
pub const HEADLINE_QTYPE: &str = "A";

/// Derive the DNS headline from captured byte counts: the full/condensed
/// bytes come from the matching `www.climb.example. A` rows and the reduction
/// is computed from them, never hardcoded.
fn headline_from_capture(capture: &DnsCapture, source: DnsCaptureSource) -> DnsPipelineHeadline {
    let find = |label: &str| {
        capture
            .queries
            .iter()
            .find(|q| {
                q.label.eq_ignore_ascii_case(label)
                    && q.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
                    && q.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE)
            })
            .map(|q| q.response_bytes)
    };
    let full = find("full");
    let condensed = find("condensed");
    let reduction = match (full, condensed) {
        (Some(f), Some(c)) if f > 0 => Some(((f - c) as f64 / f as f64) * 100.0),
        _ => None,
    };
    DnsPipelineHeadline {
        full_signature_bytes_www_a: full,
        condensed_proof_bytes_www_a: condensed,
        reduction_percent_www_a: reduction,
        notes: Some(format!(
            "From {} DNS demo (www.climb.example. A)",
            match source {
                DnsCaptureSource::Live => "live",
                DnsCaptureSource::RecordedFixture => "recorded fixture",
            }
        )),
        source,
        recorded_at: Some(capture.recorded_at.clone()),
        algorithm: Some(capture.algorithm.clone()),
    }
}

/// Decode a hex string into bytes; `None` on any malformed input.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    b.chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect()
}

/// Parse the byte anatomy of one capture query's embedded wire bytes. Returns
/// `None` when the query carries no wire bytes or the bytes fail to parse.
fn anatomy_record_from_query(q: &DnsCaptureQuery) -> Option<DnsAnatomyRecord> {
    let hex = q.wire_hex.as_ref()?;
    let bytes = decode_hex(hex)?;
    let anatomy = climb_dns::parse_response_anatomy(&bytes, Some(q.transport.as_str())).ok()?;
    Some(DnsAnatomyRecord {
        label: q.label.clone(),
        wire_length: anatomy.wire_length,
        header_bytes: anatomy.header_bytes,
        question_bytes: anatomy.question.total_bytes,
        rrsig_count: anatomy.rrsig_count,
        rrsig_signature_lengths: anatomy
            .rrsig_entries
            .iter()
            .map(|e| e.signature_len)
            .collect(),
        full_rrsig_count: anatomy.full_rrsig_count,
        condensed_rrsig_count: anatomy.condensed_rrsig_count,
        parts_sum: anatomy.parts_sum,
    })
}

/// Derive the headline anatomy section from the capture's embedded wire bytes
/// for the `www.climb.example. A` full and condensed passes. `None` when neither
/// pass carries wire bytes.
fn anatomy_from_capture(
    capture: &DnsCapture,
    source: DnsCaptureSource,
) -> Option<DnsAnatomySection> {
    let find = |label: &str| {
        capture
            .queries
            .iter()
            .find(|q| {
                q.label.eq_ignore_ascii_case(label)
                    && q.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
                    && q.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE)
            })
            .and_then(anatomy_record_from_query)
    };
    let full = find("full");
    let condensed = find("condensed");
    if full.is_none() && condensed.is_none() {
        return None;
    }
    Some(DnsAnatomySection {
        source,
        recorded_at: capture.recorded_at.clone(),
        full,
        condensed,
    })
}

/// Load the DNS capture, preferring a live recording over the committed
/// fixture; `None` when neither exists.
fn load_dns_capture(
    live: &Path,
    fixture: &Path,
) -> Result<Option<DnsPipelineHeadline>, ReportError> {
    let (path, source) = if live.exists() {
        (live, DnsCaptureSource::Live)
    } else if fixture.exists() {
        (fixture, DnsCaptureSource::RecordedFixture)
    } else {
        return Ok(None);
    };
    let capture: DnsCapture = read_json(path)?;
    Ok(Some(headline_from_capture(&capture, source)))
}

/// Load the DNS anatomy section from the same capture that feeds the headline,
/// preferring a live recording over the committed fixture. `None` when no
/// capture exists or neither headline pass carries embedded wire bytes. A
/// present-but-unparseable capture is a hard error naming the path.
fn load_dns_anatomy(live: &Path, fixture: &Path) -> Result<Option<DnsAnatomySection>, ReportError> {
    let (path, source) = if live.exists() {
        (live, DnsCaptureSource::Live)
    } else if fixture.exists() {
        (fixture, DnsCaptureSource::RecordedFixture)
    } else {
        return Ok(None);
    };
    let capture: DnsCapture = read_json(path)?;
    Ok(anatomy_from_capture(&capture, source))
}

/// Load the recorded ladder-verification artifact, preferring a live run over
/// the committed fixture; `None` when neither exists. A present-but-unparseable
/// artifact is a hard error naming the path, never a silently empty section.
fn load_ladder_section(live: &Path, fixture: &Path) -> Result<Option<LadderSection>, ReportError> {
    let (path, source) = if live.exists() {
        (live, LadderSource::Live)
    } else if fixture.exists() {
        (fixture, LadderSource::RecordedFixture)
    } else {
        return Ok(None);
    };
    let document: LadderVerification = read_json(path)?;
    Ok(Some(LadderSection::from_document(&document, source)))
}

pub fn consolidate_results(opts: ConsolidateOptions) -> Result<ClimbResults, ReportError> {
    let max_age = Duration::from_secs(24 * 3600);
    let mut warnings: Vec<String> = Vec::new();

    let workspace_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    let (summary_rows, plain_records, classical_records) =
        read_summary_rows(&opts.criterion_dir, &opts.baseline)?;

    let mut signing: Vec<SigningBenchmarkResult> = Vec::new();
    let mut verifying: Vec<VerifyingBenchmarkResult> = Vec::new();

    for row in &summary_rows {
        match row.kind {
            crate::summary::BenchKind::Signing => {
                signing.push(SigningBenchmarkResult {
                    size: row.size.to_string(),
                    condition: match row.mode {
                        crate::summary::MtlMode::WithMtlFullVerify => "With MTL".to_string(),
                        crate::summary::MtlMode::WithMtlTrustCached => {
                            "MTL (trust cached)".to_string()
                        }
                        crate::summary::MtlMode::WithoutMtl => "Without MTL".to_string(),
                    },
                    messages: row.elements.unwrap_or(0),
                    mean_ns: row.mean_ns,
                    ci_lower_ns: row.ci_lower_ns,
                    ci_upper_ns: row.ci_upper_ns,
                    speedup: None,
                });
            }
            crate::summary::BenchKind::Verifying => {
                verifying.push(VerifyingBenchmarkResult {
                    size: row.size.to_string(),
                    condition: match row.mode {
                        crate::summary::MtlMode::WithMtlFullVerify => "With MTL".to_string(),
                        crate::summary::MtlMode::WithMtlTrustCached => {
                            "MTL (trust cached)".to_string()
                        }
                        crate::summary::MtlMode::WithoutMtl => "Without MTL".to_string(),
                    },
                    messages: row.elements.unwrap_or(0),
                    mean_ns: row.mean_ns,
                    ci_lower_ns: row.ci_lower_ns,
                    ci_upper_ns: row.ci_upper_ns,
                    speedup: None,
                });
            }
        }
    }

    let mut wire_size: Vec<WireSizeResult> = Vec::new();
    if let Some(ref p) = opts.wire_report_path {
        if !p.exists() {
            warnings.push(format!("wire report missing: {}", p.display()));
        } else if !path_is_fresh(p, max_age, opts.force) {
            warnings.push(format!("wire report stale (>24h): {}", p.display()));
        }
        if p.exists() {
            let reports: Vec<climb_bench::WireSizeReport> = read_json(p)?;
            for r in reports {
                wire_size.push(WireSizeResult {
                    size: format!("{:?}", r.size).to_lowercase(),
                    message_count: r.message_count,
                    ladder_len: r.ladder_len,
                    condensed_min: r.condensed.min,
                    condensed_mean: r.condensed.mean,
                    condensed_max: r.condensed.max,
                    condensed_uniform: r.condensed.uniform,
                    with_mtl_total_bytes: r.with_mtl_total,
                    without_mtl_total_bytes: r.without_mtl_total,
                    reduction_percent: r.reduction_percent,
                    plain_total_bytes: r.plain.total,
                    plain_signature_len_bytes: r.plain.signature_len as u64,
                });
            }
        }
    }

    // The Rust harness holds one `SignOutput` per `sign_batch`: the shared
    // ladder plus one condensed signature per message. Those are exactly the
    // bytes the wire-size pass measured for the same dataset size, so the
    // harness term is derived from the wire row — `with_mtl_total` is the
    // ladder plus the condensed sum, and the condensed sum is the remainder.
    // A size with no wire row leaves the payload unmodelled rather than
    // inventing one.

    let mut memory_results = MemoryResults {
        measured: Vec::new(),
        analytical_model: AnalyticalMemoryModel {
            tree_page_size_bytes: climb_analytical::MTL_TREE_PAGE_SIZE,
            tree_max_pages: climb_analytical::MTL_TREE_MAX_PAGES,
            randomizer_pages: climb_analytical::MTL_TREE_RANDOMIZER_PAGES,
            hash_size_bytes: climb_analytical::HASH_SIZE,
        },
    };
    if let Some(ref p) = opts.memory_report_path {
        if !p.exists() {
            warnings.push(format!("memory report missing: {}", p.display()));
        } else if !path_is_fresh(p, max_age, opts.force) {
            warnings.push(format!("memory report stale (>24h): {}", p.display()));
        }
        if p.exists() {
            let content = fs::read_to_string(p).map_err(|e| ReportError::Io {
                path: p.clone(),
                source: e,
            })?;
            // A present-but-unparseable artifact is a hard error naming the
            // path, never a silently empty memory section.
            let artifact =
                parse_memory_artifact(&content).map_err(|e| ReportError::MalformedData {
                    path: p.clone(),
                    reason: e.to_string(),
                })?;
            memory_results.measured = memory_rows(&artifact, &wire_size, &mut warnings);
        }
    }

    let environment: EnvironmentFingerprint = if opts.environment_path.exists() {
        read_json(&opts.environment_path)?
    } else {
        EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "unknown".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({}),
            cpu: serde_json::json!({}),
            submodules: Vec::new(),
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({}),
        }
    };

    let dns_pipeline = load_dns_capture(&opts.dns_capture_path, &opts.dns_fixture_path)?;
    let dns_anatomy = load_dns_anatomy(&opts.dns_capture_path, &opts.dns_fixture_path)?;
    let ladder = load_ladder_section(&opts.ladder_verification_path, &opts.ladder_fixture_path)?;

    let plain = plain_timings(&plain_records);
    let classical_lengths = load_classical_signature_lengths(
        &opts.classical_signatures_path,
        &classical_records,
        &mut warnings,
    )?;
    let classical = match &classical_lengths {
        Some(lengths) => classical_timings(&classical_records, lengths),
        None => Vec::new(),
    };
    let comparisons = build_comparisons(&wire_size, &signing, &verifying, &plain);

    let mut derived = derive_summary(
        &wire_size,
        &signing,
        &verifying,
        &plain,
        dns_pipeline.as_ref(),
    );
    derived.udp_fit = build_udp_fit(
        &wire_size,
        classical_lengths.as_ref(),
        opts.rtt_ms,
        &mut warnings,
    );
    derived.signature_context = build_signature_context(
        &wire_size,
        &signing,
        &verifying,
        &plain,
        &classical,
        derived.udp_fit.as_ref(),
    );

    let results = ClimbResults {
        version: "1.0".to_string(),
        generated_at: chrono_like_timestamp(),
        workspace_root,
        benchmark: BenchmarkResults { signing, verifying },
        wire_size,
        memory: memory_results,
        environment,
        dns_pipeline,
        dns_anatomy,
        ladder,
        plain,
        classical,
        comparisons,
        derived,
        warnings,
    };

    let json = serde_json::to_string_pretty(&results).map_err(|e| ReportError::JsonDecode {
        path: opts.output_path.clone(),
        source: e,
    })?;
    fs::write(&opts.output_path, json).map_err(|e| ReportError::Io {
        path: opts.output_path.clone(),
        source: e,
    })?;

    Ok(results)
}

/// One measured central value plus its spread, in the units the consolidated
/// memory row records.
struct MeasuredRow {
    size: String,
    message_count: usize,
    mean: f64,
    median_bytes: u64,
    iqr_bytes: u64,
    samples: usize,
    method: String,
    min_bytes: u64,
    max_bytes: u64,
    stddev_bytes: f64,
    peak_bytes: u64,
    stages: Option<MemoryStages>,
}

/// The analytical decomposition for one size: the libMTL page model, the
/// harness-output buffers implied by the matching wire row, and the
/// size-proportional dataset-buffer and keypair terms.
struct Analytical {
    libmtl_bytes: usize,
    harness_bytes: u64,
    dataset_buffers_bytes: u64,
    keypair_bytes: u64,
    total_bytes: u64,
}

/// Population standard deviation of a sample, in the sample's own units.
fn population_stddev(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n == 0.0 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    variance.sqrt()
}

/// Decompose the analytical memory estimate for `size_label` into libMTL pages
/// and harness buffers, warning (and leaving the payload unmodelled) when no
/// wire row exists for that size.
fn analytical_for(
    message_count: usize,
    size_label: &str,
    wire_size: &[WireSizeResult],
    warnings: &mut Vec<String>,
) -> Analytical {
    let libmtl = climb_analytical::estimate_memory_footprint(message_count);
    let (harness_n, condensed_sum, ladder_len) = match wire_size
        .iter()
        .find(|w| w.size == size_label)
    {
        Some(w) => {
            let condensed = w.with_mtl_total_bytes.saturating_sub(w.ladder_len as u64);
            (
                w.message_count,
                usize::try_from(condensed).unwrap_or(usize::MAX),
                w.ladder_len,
            )
        }
        None => {
            warnings.push(format!(
                    "memory report size `{size_label}` has no matching wire-size row; harness payload not modelled"
                ));
            (message_count, 0, 0)
        }
    };
    let harness = climb_analytical::harness_output_estimate(harness_n, condensed_sum, ladder_len);
    let dataset_buffers_bytes = climb_analytical::dataset_buffers_estimate(message_count) as u64;
    let keypair_bytes = climb_analytical::KEY_IMAGE_BYTES as u64;
    let total_bytes = libmtl.total_memory_bytes as u64
        + harness.estimated_total_bytes as u64
        + dataset_buffers_bytes
        + keypair_bytes;
    Analytical {
        libmtl_bytes: libmtl.total_memory_bytes,
        harness_bytes: harness.estimated_total_bytes as u64,
        dataset_buffers_bytes,
        keypair_bytes,
        total_bytes,
    }
}

/// Assemble a consolidated memory row from a measured value and its analytical
/// decomposition.
fn memory_row(measured: MeasuredRow, analytical: Analytical) -> MemoryResult {
    let total = analytical.total_bytes;
    let measured_i64 = measured.mean.round() as i64;
    MemoryResult {
        size: measured.size,
        message_count: measured.message_count,
        measured_mean_bytes: measured.mean.round() as u64,
        libmtl_analytical_bytes: analytical.libmtl_bytes,
        harness_buffers_bytes: analytical.harness_bytes,
        analytical_total_bytes: total,
        residual_bytes: measured_i64 - total as i64,
        residual_percent: if total == 0 {
            0.0
        } else {
            (measured_i64 as f64 - total as f64) / (total as f64) * 100.0
        },
        ratio: if analytical.libmtl_bytes == 0 {
            0.0
        } else {
            measured.mean / (analytical.libmtl_bytes as f64)
        },
        measured_analytical_total_ratio: if total == 0 {
            0.0
        } else {
            measured.mean / (total as f64)
        },
        measured_range_min_bytes: measured.min_bytes,
        measured_range_max_bytes: measured.max_bytes,
        measured_stddev_bytes: measured.stddev_bytes,
        median_bytes: measured.median_bytes,
        iqr_bytes: measured.iqr_bytes,
        samples: measured.samples,
        method: measured.method,
        dataset_buffers_bytes: analytical.dataset_buffers_bytes,
        keypair_bytes: analytical.keypair_bytes,
        measured_peak_bytes: measured.peak_bytes,
        stages: measured.stages,
    }
}

/// The isolated-process report: the median is the measured central value, and
/// the spread is the artifact's own IQR (with a standard deviation derived from
/// the raw per-sample deltas for the pre-existing `measured_stddev_bytes`).
fn isolated_memory_row(
    run: &crate::memory::SizeReport,
    wire_size: &[WireSizeResult],
    warnings: &mut Vec<String>,
) -> MemoryResult {
    let stddev = 1024.0
        * population_stddev(
            &run.deltas_kb
                .iter()
                .map(|&kb| kb as f64)
                .collect::<Vec<_>>(),
        );
    // Peak delta is the headline; prefer the artifact's own stage summary and
    // fall back to the per-sample high-water marks when stages are absent.
    let peak_kb = run
        .stages
        .as_ref()
        .map(|s| s.peak.median_kb)
        .unwrap_or_else(|| {
            let peaks: Vec<u64> = run
                .samples
                .iter()
                .map(|s| s.hwm_kb.saturating_sub(s.baseline_kb))
                .collect();
            climb_bench::median_iqr(&peaks).0
        });
    let stages = run.stages.as_ref().map(memory_stages_from);
    let analytical = analytical_for(run.message_count, &run.size, wire_size, warnings);
    memory_row(
        MeasuredRow {
            size: run.size.clone(),
            message_count: run.message_count,
            mean: run.median_bytes as f64,
            median_bytes: run.median_bytes,
            iqr_bytes: run.iqr_bytes,
            samples: run.samples.len(),
            method: run.method.clone(),
            min_bytes: run.min_bytes,
            max_bytes: run.max_bytes,
            stddev_bytes: stddev,
            peak_bytes: (peak_kb * 1024.0).round() as u64,
            stages,
        },
        analytical,
    )
}

/// Convert an artifact stage breakdown (kilobytes) into a memory row's stage
/// breakdown (bytes).
fn memory_stages_from(s: &crate::memory::StageDeltas) -> MemoryStages {
    fn kb_to_bytes(kb: f64) -> u64 {
        (kb * 1024.0).round() as u64
    }
    MemoryStages {
        dataset_delta_bytes: kb_to_bytes(s.dataset.median_kb),
        keypair_delta_bytes: kb_to_bytes(s.keypair.median_kb),
        sign_delta_bytes: kb_to_bytes(s.sign.median_kb),
        resident_bytes: kb_to_bytes(s.resident.median_kb),
        peak_bytes: kb_to_bytes(s.peak.median_kb),
        dataset_delta_iqr_bytes: s.dataset.iqr_kb * 1024,
        keypair_delta_iqr_bytes: s.keypair.iqr_kb * 1024,
        sign_delta_iqr_bytes: s.sign.iqr_kb * 1024,
        resident_iqr_bytes: s.resident.iqr_kb * 1024,
        peak_iqr_bytes: s.peak.iqr_kb * 1024,
    }
}

/// The legacy in-process delta report: mean, min, max and stddev are computed
/// from the per-run deltas; the median and IQR are computed from the same
/// sample so the newer columns are populated wherever they are derivable.
fn legacy_rss_delta_row(
    run: &climb_bench::RssSizeReport,
    wire_size: &[WireSizeResult],
    warnings: &mut Vec<String>,
) -> MemoryResult {
    let deltas: Vec<u64> = run.runs.iter().map(|r| r.delta_bytes).collect();
    let n = deltas.len() as f64;
    let mean = deltas.iter().map(|&d| d as f64).sum::<f64>() / n;
    let (median_f, _, _, iqr) = climb_bench::median_iqr(&deltas);
    let minv = deltas.iter().copied().min().unwrap_or(0);
    let maxv = deltas.iter().copied().max().unwrap_or(0);
    let stddev = population_stddev(&deltas.iter().map(|&d| d as f64).collect::<Vec<_>>());
    let size_label = format!("{:?}", run.size).to_lowercase();
    let analytical = analytical_for(run.size.count(), &size_label, wire_size, warnings);
    memory_row(
        MeasuredRow {
            size: size_label,
            message_count: run.size.count(),
            mean,
            median_bytes: median_f.round() as u64,
            iqr_bytes: iqr,
            samples: deltas.len(),
            method: "rss-delta".to_string(),
            min_bytes: minv,
            max_bytes: maxv,
            stddev_bytes: stddev,
            peak_bytes: 0,
            stages: None,
        },
        analytical,
    )
}

/// The legacy comparison shape carries a pre-computed analytical total and no
/// harness term, so it is mapped as-is; it reports a mean and a range but no
/// median, IQR or sample count.
fn comparison_memory_row(c: &climb_bench::RssComparison) -> MemoryResult {
    let measured = c.measured_mean_bytes as i64;
    let total = c.analytical_bytes as i64;
    MemoryResult {
        size: format!("{:?}", c.size).to_lowercase(),
        message_count: c.message_count,
        measured_mean_bytes: c.measured_mean_bytes,
        libmtl_analytical_bytes: c.analytical_bytes,
        harness_buffers_bytes: 0,
        analytical_total_bytes: c.analytical_bytes as u64,
        residual_bytes: measured - total,
        residual_percent: if total == 0 {
            0.0
        } else {
            (measured as f64 - total as f64) / (total as f64) * 100.0
        },
        ratio: c.ratio,
        measured_analytical_total_ratio: if c.analytical_bytes == 0 {
            0.0
        } else {
            c.measured_mean_bytes as f64 / (c.analytical_bytes as f64)
        },
        measured_range_min_bytes: c.measured_range_bytes.0,
        measured_range_max_bytes: c.measured_range_bytes.1,
        measured_stddev_bytes: c.measured_stddev_bytes,
        median_bytes: c.measured_mean_bytes,
        iqr_bytes: 0,
        samples: 0,
        method: "rss-comparison".to_string(),
        dataset_buffers_bytes: 0,
        keypair_bytes: 0,
        measured_peak_bytes: 0,
        stages: None,
    }
}

/// Turn one recognized memory artifact into the consolidated rows.
fn memory_rows(
    artifact: &MemoryArtifact,
    wire_size: &[WireSizeResult],
    warnings: &mut Vec<String>,
) -> Vec<MemoryResult> {
    match artifact {
        MemoryArtifact::Isolated(report) => report
            .runs
            .iter()
            .map(|run| isolated_memory_row(run, wire_size, warnings))
            .collect(),
        MemoryArtifact::LegacyRssDelta(report) => report
            .runs
            .iter()
            .filter(|run| !run.runs.is_empty())
            .map(|run| legacy_rss_delta_row(run, wire_size, warnings))
            .collect(),
        MemoryArtifact::LegacyComparison(rows) => rows.iter().map(comparison_memory_row).collect(),
    }
}

fn chrono_like_timestamp() -> String {
    // Use local time similar to ISO 8601; we don't have chrono as dep here - simple format
    use std::process::Command;
    if let Ok(out) = Command::new("date").arg("--iso-8601=seconds").output() {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn size_display_title_cases_only_the_first_letter() {
        assert_eq!(size_display("small"), "Small");
        assert_eq!(size_display("medium"), "Medium");
        assert_eq!(size_display("Small"), "Small", "already title-cased");
        assert_eq!(size_display("oneK"), "OneK");
        assert_eq!(size_display(""), "");
    }

    const EST_JSON: &str = r#"{"mean":{"confidence_interval":{"confidence_level":0.95,"lower_bound":90.0,"upper_bound":110.0},"point_estimate":100.0,"standard_error":5.0}}"#;

    fn temp_root(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "climb-report-results-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_leaf(leaf: &Path, group_id: &str, function_id: &str, elements: u64) {
        fs::create_dir_all(leaf).unwrap();
        let bench = format!(
            r#"{{"group_id":"{group_id}","function_id":"{function_id}","value_str":"{elements}","throughput":{{"Elements":{elements}}},"full_id":"{group_id}/{function_id}/{elements}","directory_name":"{group_id}/{function_id}/{elements}","title":"{group_id}/{function_id}/{elements}"}}"#
        );
        fs::write(leaf.join("benchmark.json"), bench).unwrap();
        fs::write(leaf.join("estimates.json"), EST_JSON).unwrap();
    }

    fn signing_row(size: &str, condition: &str, mean_ns: f64) -> SigningBenchmarkResult {
        SigningBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 100,
            mean_ns,
            ci_lower_ns: mean_ns - 10.0,
            ci_upper_ns: mean_ns + 10.0,
            speedup: None,
        }
    }

    fn verifying_row(size: &str, condition: &str, mean_ns: f64) -> VerifyingBenchmarkResult {
        VerifyingBenchmarkResult {
            size: size.to_string(),
            condition: condition.to_string(),
            messages: 100,
            mean_ns,
            ci_lower_ns: mean_ns - 10.0,
            ci_upper_ns: mean_ns + 10.0,
            speedup: None,
        }
    }

    fn wire_row(size: &str, reduction_percent: f64) -> WireSizeResult {
        WireSizeResult {
            size: size.to_string(),
            message_count: 100,
            ladder_len: 8,
            condensed_min: 650,
            condensed_mean: 658.0,
            condensed_max: 660,
            condensed_uniform: true,
            with_mtl_total_bytes: 658,
            without_mtl_total_bytes: 24438,
            reduction_percent,
            plain_total_bytes: 0,
            plain_signature_len_bytes: 0,
        }
    }

    fn plain_row(size: &str) -> PlainTimingResult {
        PlainTimingResult {
            size: size.to_string(),
            messages: 100,
            signing_mean_ns: Some(737_922_991.4),
            signing_ci_lower_ns: Some(698_016_512.78),
            signing_ci_upper_ns: Some(771_913_495.5),
            verifying_mean_ns: Some(736_473.46),
            verifying_ci_lower_ns: Some(715_353.3),
            verifying_ci_upper_ns: Some(756_882.23),
            class: EvidenceClass::Measured,
            samples: None,
        }
    }

    fn make_plain_record(group_id: &str, mean_ns: f64) -> crate::record::CriterionRecord {
        crate::record::CriterionRecord {
            group_id: group_id.to_string(),
            function_id: Some("f".to_string()),
            value_str: "1".to_string(),
            full_id: format!("{}/f/1", group_id),
            elements: None,
            mean_ns,
            mean_ci_lower_ns: mean_ns * 0.9,
            mean_ci_upper_ns: mean_ns * 1.1,
            confidence_level: 0.95,
        }
    }

    fn memory_row(size: &str) -> MemoryResult {
        MemoryResult {
            size: size.to_string(),
            message_count: 100,
            measured_mean_bytes: 4096,
            libmtl_analytical_bytes: 4096,
            harness_buffers_bytes: 0,
            analytical_total_bytes: 4096,
            residual_bytes: 0,
            residual_percent: 0.0,
            measured_analytical_total_ratio: 1.0,
            ratio: 1.0,
            measured_range_min_bytes: 4000,
            measured_range_max_bytes: 4200,
            measured_stddev_bytes: 50.0,
            median_bytes: 4096,
            iqr_bytes: 0,
            samples: 1,
            method: "isolated-process".to_string(),
            dataset_buffers_bytes: 0,
            keypair_bytes: 0,
            measured_peak_bytes: 4096,
            stages: None,
        }
    }

    fn analytical_model() -> AnalyticalMemoryModel {
        AnalyticalMemoryModel {
            tree_page_size_bytes: 4096,
            tree_max_pages: 1,
            randomizer_pages: 1,
            hash_size_bytes: 32,
        }
    }

    fn environment_fingerprint() -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            schema_version: 1,
            timestamp: "unknown".to_string(),
            system: serde_json::json!({}),
            toolchain: serde_json::json!({}),
            cpu: serde_json::json!({}),
            submodules: Vec::new(),
            docker: serde_json::json!({}),
            benchmark_runtime: serde_json::json!({}),
        }
    }

    /// A [`LadderVerification`] whose single row is reproduced by the model, so
    /// the loader tests exercise real `model_matches` logic, not a stub.
    fn ladder_document_fixture(recorded_at: &str) -> LadderVerification {
        let n = 8usize;
        let lengths: Vec<usize> = (0..n)
            .map(|index| {
                climb_analytical::predicted_condensed_len(n as u64, index as u64).unwrap_or(0)
            })
            .collect();
        let min = lengths.iter().copied().min().unwrap_or(0);
        let max = lengths.iter().copied().max().unwrap_or(0);
        let total: u64 = lengths.iter().map(|&length| length as u64).sum();
        LadderVerification {
            schema_version: crate::ladder::LADDER_VERIFICATION_SCHEMA_VERSION,
            recorded_at: recorded_at.to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            hash_size: 16,
            seed: 1,
            provenance: crate::ladder::LadderProvenance {
                submodules: Vec::new(),
            },
            sizes: vec![climb_bench::LadderRow {
                leaf_count: n,
                rung_count: climb_analytical::rungs(n as u64).len(),
                rungs: Vec::new(),
                ladder_buffer_len: climb_analytical::predicted_ladder_len(n as u64),
                signed_ladder_len: climb_analytical::predicted_ladder_len(n as u64) + 4 + 7856,
                signature_len: 7856,
                condensed: climb_bench::CondensedStats {
                    min,
                    max,
                    mean: total as f64 / n as f64,
                    total,
                    uniform: min == max,
                },
                full_len_min: 0,
                full_len_max: 0,
                with_mtl_signatures: 1,
                without_mtl_signatures: n,
            }],
        }
    }

    fn sample_ladder_section() -> LadderSection {
        LadderSection {
            source: LadderSource::RecordedFixture,
            recorded_at: "2026-10-14T00:16:11+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            all_match: true,
            sizes: vec![crate::ladder::LadderSizeRow {
                leaf_count: 8,
                rung_count: 2,
                ladder_buffer_len: 104,
                signed_ladder_len: 7964,
                condensed_min: 108,
                condensed_max: 108,
                condensed_mean: 108.0,
                model_matches: true,
            }],
        }
    }

    fn full_fixture() -> ClimbResults {
        let signing = vec![signing_row("Small", "With MTL", 1000.0)];
        let verifying = vec![verifying_row("Small", "MTL (trust cached)", 100.0)];
        let wire = vec![wire_row("small", 97.3)];
        let dns = DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(24_438),
            condensed_proof_bytes_www_a: Some(658),
            reduction_percent_www_a: Some(97.3),
            notes: Some("note".to_string()),
            source: DnsCaptureSource::Live,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        };
        let plain = vec![plain_row("Small")];
        let comparisons = build_comparisons(&wire, &signing, &verifying, &plain);
        let derived = derive_summary(&wire, &signing, &verifying, &plain, Some(&dns));
        ClimbResults {
            version: "1.0".to_string(),
            generated_at: "2026-10-12T00:00:00Z".to_string(),
            workspace_root: "/tmp/climb".to_string(),
            benchmark: BenchmarkResults { signing, verifying },
            wire_size: wire,
            memory: MemoryResults {
                measured: vec![memory_row("small")],
                analytical_model: analytical_model(),
            },
            environment: EnvironmentFingerprint {
                system: serde_json::json!({"os": "linux"}),
                submodules: vec![serde_json::json!({"name": "climb-mtl"})],
                ..environment_fingerprint()
            },
            dns_pipeline: Some(dns),
            dns_anatomy: None,
            ladder: Some(sample_ladder_section()),
            plain,
            classical: Vec::new(),
            comparisons,
            derived,
            warnings: vec!["wire report missing: /x".to_string()],
        }
    }

    #[test]
    fn climb_results_full_roundtrip_equals_original() {
        let original = full_fixture();
        let json = serde_json::to_string(&original).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn climb_results_empty_roundtrip_equals_original() {
        let original = ClimbResults {
            version: "1.0".to_string(),
            generated_at: "unknown".to_string(),
            workspace_root: ".".to_string(),
            benchmark: BenchmarkResults {
                signing: Vec::new(),
                verifying: Vec::new(),
            },
            wire_size: Vec::new(),
            memory: MemoryResults {
                measured: Vec::new(),
                analytical_model: analytical_model(),
            },
            environment: environment_fingerprint(),
            dns_pipeline: None,
            dns_anatomy: None,
            ladder: None,
            plain: Vec::new(),
            classical: Vec::new(),
            comparisons: Vec::new(),
            derived: DerivedSummary {
                overall_wire_reduction_percent: None,
                largest_wire_reduction_percent: None,
                overall_verifying_trust_cached_speedup_avg: None,
                headline: None,
                reuse: Vec::new(),
                udp_fit: None,
                signature_context: Vec::new(),
            },
            warnings: Vec::new(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn derive_wire_reduction_matches_hand_computed_three_size_table() {
        // Hand-computed: (97.0 + 96.7 + 95.7) / 3 = 289.4 / 3 = 96.466_666_...67
        let wire = vec![
            wire_row("small", 97.0),
            wire_row("medium", 96.7),
            wire_row("large", 95.7),
        ];
        let derived = derive_summary(&wire, &[], &[], &[], None);
        let overall = derived.overall_wire_reduction_percent.expect("some");
        assert!(
            (overall - 96.466_666_666_666_67).abs() < 1e-9,
            "overall={overall}"
        );
        let largest = derived.largest_wire_reduction_percent.expect("some");
        assert!((largest - 97.0).abs() < 1e-9, "largest={largest}");
    }

    #[test]
    fn derive_wire_reduction_is_none_without_wire_data() {
        let derived = derive_summary(&[], &[], &[], &[], None);
        assert_eq!(derived.overall_wire_reduction_percent, None);
        assert_eq!(derived.largest_wire_reduction_percent, None);
        assert_eq!(derived.overall_verifying_trust_cached_speedup_avg, None);
    }

    #[test]
    fn derive_trust_cached_speedup_average_from_two_rows() {
        // Signing "With MTL" means: Small 1000, Medium 2000 ns.
        // Trust-cached verifying means: Small 100 ns -> 10.0, Medium 400 ns -> 5.0.
        // Average = (10.0 + 5.0) / 2 = 7.5.
        let signing = vec![
            signing_row("Small", "With MTL", 1000.0),
            signing_row("Medium", "With MTL", 2000.0),
        ];
        let verifying = vec![
            verifying_row("Small", "MTL (trust cached)", 100.0),
            verifying_row("Medium", "MTL (trust cached)", 400.0),
        ];
        let derived = derive_summary(&[], &signing, &verifying, &[], None);
        let avg = derived
            .overall_verifying_trust_cached_speedup_avg
            .expect("some");
        assert!((avg - 7.5).abs() < 1e-9, "avg={avg}");
    }

    #[test]
    fn derive_trust_cached_speedup_is_none_without_cached_rows() {
        let signing = vec![signing_row("Small", "With MTL", 1000.0)];
        let verifying = vec![verifying_row("Small", "With MTL", 100.0)];
        let derived = derive_summary(&[], &signing, &verifying, &[], None);
        assert_eq!(derived.overall_verifying_trust_cached_speedup_avg, None);
    }

    #[test]
    fn read_summary_rows_returns_valid_rows_and_drops_unknown_ids() {
        let root = temp_root("rows-valid");
        write_leaf(
            &root
                .join("signing_small_with_mtl")
                .join("sign_batch")
                .join("100")
                .join("new"),
            "signing_small_with_mtl",
            "sign_batch",
            100,
        );
        // Stale layout: the group id is the first path component, so
        // "signing_small" does not parse as a known group.
        write_leaf(
            &root
                .join("signing_small")
                .join("with_mtl")
                .join("batch")
                .join("new"),
            "signing_small",
            "with_mtl",
            1,
        );

        let (rows, _plain, _classical) = read_summary_rows(&root, "new").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, crate::summary::BenchKind::Signing);
        assert_eq!(rows[0].size, crate::summary::Size::Small);
        assert_eq!(rows[0].mode, crate::summary::MtlMode::WithMtlFullVerify);
        assert_eq!(rows[0].elements, Some(100));
    }

    #[test]
    fn read_summary_rows_skips_leaf_missing_estimates_json() {
        let root = temp_root("rows-malformed");
        write_leaf(
            &root
                .join("verifying_medium_with_mtl_trust_true")
                .join("verify_batch")
                .join("1000")
                .join("new"),
            "verifying_medium_with_mtl_trust_true",
            "verify_batch",
            1000,
        );
        let broken = root
            .join("verifying_large_without_mtl")
            .join("verify_batch")
            .join("10000")
            .join("new");
        fs::create_dir_all(&broken).unwrap();
        fs::write(
            broken.join("benchmark.json"),
            r#"{"group_id":"verifying_large_without_mtl","function_id":"verify_batch","value_str":"10000","throughput":{"Elements":10000},"full_id":"verifying_large_without_mtl/verify_batch/10000","directory_name":"x","title":"x"}"#,
        )
        .unwrap();

        let (rows, _plain, _classical) = read_summary_rows(&root, "new").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, crate::summary::BenchKind::Verifying);
        assert_eq!(rows[0].size, crate::summary::Size::Medium);
    }

    #[test]
    fn summarize_strictly_rejects_unknown_group_id() {
        // Documents why `read_summary_rows` filters before calling `summarize`:
        // a strict call would fail on the same stale layout.
        let record = CriterionRecord {
            group_id: "signing_small".to_string(),
            function_id: Some("with_mtl".to_string()),
            value_str: "1".to_string(),
            full_id: "signing_small/with_mtl/1".to_string(),
            elements: Some(1),
            mean_ns: 1.0,
            mean_ci_lower_ns: 1.0,
            mean_ci_upper_ns: 1.0,
            confidence_level: 0.95,
        };
        assert!(summarize(&[record]).is_err());
    }

    #[test]
    fn freshness_true_within_max_age() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_true_at_exact_boundary() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(24 * 3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_false_when_stale() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(25 * 3600);
        assert!(!input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_false_for_future_timestamp() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now + Duration::from_secs(3600);
        assert!(!input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            false
        ));
    }

    #[test]
    fn freshness_force_overrides_staleness() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let modified = now - Duration::from_secs(100 * 3600);
        assert!(input_is_fresh(
            modified,
            now,
            Duration::from_secs(24 * 3600),
            true
        ));
    }

    #[test]
    fn consolidate_missing_optional_inputs_yield_null_sections_and_warnings() {
        let root = temp_root("missing-optional");
        let criterion = root.join("criterion");
        write_leaf(
            &criterion
                .join("signing_small_with_mtl")
                .join("sign_batch")
                .join("100")
                .join("new"),
            "signing_small_with_mtl",
            "sign_batch",
            100,
        );

        let opts = ConsolidateOptions {
            force: false,
            criterion_dir: criterion,
            wire_report_path: Some(root.join("nonexistent-wire.json")),
            memory_report_path: Some(root.join("nonexistent-memory.json")),
            dns_capture_path: root.join("nonexistent-dns-capture.json"),
            dns_fixture_path: root.join("nonexistent-dns-fixture.json"),
            ladder_verification_path: root.join("nonexistent-ladder-live.json"),
            ladder_fixture_path: root.join("nonexistent-ladder-fixture.json"),
            environment_path: root.join("nonexistent-environment.json"),
            baseline: "new".to_string(),
            rtt_ms: None,
            classical_signatures_path: root.join("no-classical-signatures.json"),
            output_path: root.join("out.json"),
        };

        let results = consolidate_results(opts).unwrap();

        assert_eq!(results.derived.overall_wire_reduction_percent, None);
        assert_eq!(results.derived.largest_wire_reduction_percent, None);
        assert_eq!(results.dns_pipeline, None);
        assert_eq!(results.ladder, None);
        assert_eq!(results.benchmark.signing.len(), 1);
        assert!(
            results
                .warnings
                .iter()
                .any(|w| w.starts_with("wire report missing:")),
            "warnings={:?}",
            results.warnings
        );
        assert!(
            results
                .warnings
                .iter()
                .any(|w| w.starts_with("memory report missing:")),
            "warnings={:?}",
            results.warnings
        );
        assert_eq!(results.environment.schema_version, 1);
        assert_eq!(results.environment.timestamp, "unknown");

        let json = fs::read_to_string(root.join("out.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["derived"]["overall_wire_reduction_percent"].is_null());
        assert!(value["warnings"].is_array());
    }

    #[test]
    fn consolidate_end_to_end_writes_parseable_results() {
        let root = temp_root("e2e");
        let criterion = root.join("criterion");
        write_leaf(
            &criterion
                .join("verifying_medium_with_mtl_trust_true")
                .join("verify_batch")
                .join("1000")
                .join("new"),
            "verifying_medium_with_mtl_trust_true",
            "verify_batch",
            1000,
        );

        let output = root.join("results.json");
        let opts = ConsolidateOptions {
            force: false,
            criterion_dir: criterion,
            wire_report_path: None,
            memory_report_path: None,
            dns_capture_path: root.join("dns-capture.json"),
            dns_fixture_path: root.join("dns-fixture.json"),
            ladder_verification_path: root.join("ladder-live.json"),
            ladder_fixture_path: root.join("ladder-fixture.json"),
            environment_path: root.join("environment.json"),
            rtt_ms: None,
            baseline: "new".to_string(),
            classical_signatures_path: root.join("no-classical-signatures.json"),
            output_path: output.clone(),
        };

        let results = consolidate_results(opts).unwrap();
        assert_eq!(results.version, "1.0");
        assert_eq!(results.benchmark.verifying.len(), 1);
        assert!(results.warnings.is_empty());
        assert_eq!(results.ladder, None, "no ladder artifact was supplied");

        let json = fs::read_to_string(&output).unwrap();
        let parsed: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, results);
    }

    const WIRE_REPORT_JSON: &str = r#"[
      {"size":"Small","seed":1,"message_count":100,"ladder_len":7992,
       "condensed":{"min":108,"max":172,"mean":164.32,"total":16432,"uniform":false},
       "with_mtl_total":24424,"without_mtl_total":815632,"reduction_percent":97.00551228985621,"full_invariant_samples":6},
      {"size":"Medium","seed":1,"message_count":10000,"ladder_len":8056,
       "condensed":{"min":140,"max":284,"mean":273.5296,"total":2735296,"uniform":false},
       "with_mtl_total":2743352,"without_mtl_total":83295296,"reduction_percent":96.70647427677068,"full_invariant_samples":6},
      {"size":"Large","seed":1,"message_count":1000000,"ladder_len":8120,
       "condensed":{"min":172,"max":380,"mean":367.059712,"total":367059712,"uniform":false},
       "with_mtl_total":367067832,"without_mtl_total":8487059712,"reduction_percent":95.67497054980069,"full_invariant_samples":6}
    ]"#;

    // The same three rows as they are written today, with the additive plain
    // (configuration A) block present. A committed copy of the current
    // wire-report artifact, used to prove the consumer parses the new shape.
    const WIRE_REPORT_JSON_WITH_PLAIN: &str = r#"[
      {"size":"Small","seed":4849331494946035253,"message_count":100,"ladder_len":7992,
       "condensed":{"min":108,"max":172,"mean":164.32,"total":16432,"uniform":false},
       "with_mtl_total":24424,"without_mtl_total":815632,"reduction_percent":97.00551228985621,"full_invariant_samples":6,
       "plain":{"signature_len":7856,"samples":1,"total":785600}},
      {"size":"Medium","seed":4849331494946035253,"message_count":10000,"ladder_len":8056,
       "condensed":{"min":140,"max":284,"mean":273.5296,"total":2735296,"uniform":false},
       "with_mtl_total":2743352,"without_mtl_total":83295296,"reduction_percent":96.70647427677068,"full_invariant_samples":6,
       "plain":{"signature_len":7856,"samples":1,"total":78560000}},
      {"size":"Large","seed":4849331494946035253,"message_count":1000000,"ladder_len":8120,
       "condensed":{"min":172,"max":380,"mean":367.059712,"total":367059712,"uniform":false},
       "with_mtl_total":367067832,"without_mtl_total":8487059712,"reduction_percent":95.67497054980069,"full_invariant_samples":6,
       "plain":{"signature_len":7856,"samples":1,"total":7856000000}}
    ]"#;

    // Two repeats per size, chosen so the arithmetic mean equals the mean of
    // record: small (1_630_208 + 73_728)/2 = 851_968,
    // medium (3_575_808 + 942_080)/2 = 2_258_944,
    // large (561_266_688 + 80_093_184)/2 = 320_679_936.
    const MEMORY_REPORT_JSON: &str = r#"{"seed":1,"runs":[
      {"size":"Small","runs":[
        {"rss_before_bytes":14675968,"rss_after_bytes":16306176,"delta_bytes":1630208,"delta_mib":1.5546875},
        {"rss_before_bytes":16273408,"rss_after_bytes":16347136,"delta_bytes":73728,"delta_mib":0.0703125}],"repeat_count":2},
      {"size":"Medium","runs":[
        {"rss_before_bytes":22171648,"rss_after_bytes":25747456,"delta_bytes":3575808,"delta_mib":3.41015625},
        {"rss_before_bytes":25903104,"rss_after_bytes":26845184,"delta_bytes":942080,"delta_mib":0.8984375}],"repeat_count":2},
      {"size":"Large","runs":[
        {"rss_before_bytes":584851456,"rss_after_bytes":1146118144,"delta_bytes":561266688,"delta_mib":535.265625},
        {"rss_before_bytes":1075388416,"rss_after_bytes":1155481600,"delta_bytes":80093184,"delta_mib":76.3828125}],"repeat_count":2}
    ]}"#;

    fn write_report_inputs(root: &Path) -> (PathBuf, PathBuf) {
        let wire = root.join("wire.json");
        let memory = root.join("memory.json");
        fs::write(&wire, WIRE_REPORT_JSON).unwrap();
        fs::write(&memory, MEMORY_REPORT_JSON).unwrap();
        (wire, memory)
    }

    fn consolidate_with(
        root: &Path,
        wire: Option<PathBuf>,
        memory: Option<PathBuf>,
    ) -> ClimbResults {
        let criterion = root.join("criterion");
        fs::create_dir_all(&criterion).unwrap();
        let opts = ConsolidateOptions {
            force: true,
            criterion_dir: criterion,
            wire_report_path: wire,
            memory_report_path: memory,
            dns_capture_path: root.join("no-capture.json"),
            dns_fixture_path: root.join("no-fixture.json"),
            ladder_verification_path: root.join("no-ladder-live.json"),
            ladder_fixture_path: root.join("no-ladder-fixture.json"),
            rtt_ms: None,
            environment_path: root.join("no-env.json"),
            baseline: "new".to_string(),
            classical_signatures_path: root.join("no-classical-signatures.json"),
            output_path: root.join("out.json"),
        };
        consolidate_results(opts).unwrap()
    }

    fn memory_of<'a>(results: &'a ClimbResults, size: &str) -> &'a MemoryResult {
        results
            .memory
            .measured
            .iter()
            .find(|m| m.size == size)
            .unwrap_or_else(|| panic!("missing memory row for {size}"))
    }

    #[test]
    fn consolidate_derives_harness_from_wire_condensed_payload() {
        let root = temp_root("memory-wire");
        let (wire, memory) = write_report_inputs(&root);
        let results = consolidate_with(&root, Some(wire), Some(memory));

        assert_eq!(results.memory.measured.len(), 3);
        // The legacy wire fixture carries no plain signature length, so the
        // UDP-fit section is omitted and the omission is reported.
        assert_eq!(
            results.warnings,
            ["udp fit: no wire row carries a plain signature length; section omitted"],
            "warnings={:?}",
            results.warnings
        );
        assert!(results.derived.udp_fit.is_none());

        // The harness payload term is the wire row's condensed sum
        // (with_mtl_total - ladder_len) plus the ladder, plus (n+1) x 32.
        // Small:  24_424 + 101 x 32         = 27_656;   dataset 100 x 536
        // Medium: 2_743_352 + 10_001 x 32   = 3_063_384; dataset 10_000 x 536
        // Large:  367_067_832 + 1_000_001 x 32 = 399_067_864; dataset 1e6 x 536
        // Each total also adds the fixed key image (186 B).
        let small = memory_of(&results, "small");
        assert_eq!(small.harness_buffers_bytes, 27_656);
        assert_eq!(small.dataset_buffers_bytes, 53_600);
        assert_eq!(small.keypair_bytes, 186);
        assert_eq!(small.analytical_total_bytes, 2_178_594);
        assert_eq!(small.residual_bytes, 851_968 - 2_178_594);
        assert_eq!(
            small.measured_analytical_total_ratio,
            851_968.0 / 2_178_594.0
        );

        let medium = memory_of(&results, "medium");
        assert_eq!(medium.harness_buffers_bytes, 3_063_384);
        assert_eq!(medium.dataset_buffers_bytes, 5_360_000);
        assert_eq!(medium.analytical_total_bytes, 10_520_722);
        assert_eq!(medium.residual_bytes, 2_258_944 - 10_520_722);

        let large = memory_of(&results, "large");
        assert_eq!(large.harness_buffers_bytes, 399_067_864);
        assert_eq!(large.dataset_buffers_bytes, 536_000_000);
        assert_eq!(large.analytical_total_bytes, 984_351_122);
        assert_eq!(large.residual_bytes, 320_679_936 - 984_351_122);
        assert!(large.residual_bytes < 0);
        assert!(large.measured_analytical_total_ratio < 1.0);

        // The four modelled terms reconstruct the analytical total.
        for m in &results.memory.measured {
            assert_eq!(
                m.libmtl_analytical_bytes as u64
                    + m.harness_buffers_bytes
                    + m.dataset_buffers_bytes
                    + m.keypair_bytes,
                m.analytical_total_bytes,
                "size {}",
                m.size
            );
        }
    }

    #[test]
    fn consolidate_parses_wire_report_with_plain_config_a() {
        let root = temp_root("wire-plain");
        let wire = root.join("wire.json");
        let memory = root.join("memory.json");
        fs::write(&wire, WIRE_REPORT_JSON_WITH_PLAIN).unwrap();
        fs::write(&memory, MEMORY_REPORT_JSON).unwrap();

        let results = consolidate_with(&root, Some(wire), Some(memory));
        assert!(
            results.warnings.is_empty(),
            "warnings={:?}",
            results.warnings
        );
        assert_eq!(results.wire_size.len(), 3);

        let medium = results
            .wire_size
            .iter()
            .find(|r| r.size == "medium")
            .expect("medium wire row");
        assert_eq!(medium.with_mtl_total_bytes, 2_743_352);
        assert_eq!(medium.without_mtl_total_bytes, 83_295_296);
        assert!((medium.reduction_percent - 96.70647427677068).abs() < 1e-9);
    }

    #[test]
    fn consolidate_accepts_legacy_wire_report_without_plain() {
        let root = temp_root("wire-legacy");
        let wire = root.join("wire.json");
        let memory = root.join("memory.json");
        fs::write(&wire, WIRE_REPORT_JSON).unwrap();
        fs::write(&memory, MEMORY_REPORT_JSON).unwrap();

        let results = consolidate_with(&root, Some(wire), Some(memory));
        // The legacy rows still parse; only the UDP-fit section is missing,
        // and that gap is reported rather than silently ignored.
        assert_eq!(
            results.warnings,
            ["udp fit: no wire row carries a plain signature length; section omitted"],
            "warnings={:?}",
            results.warnings
        );
        assert_eq!(results.wire_size.len(), 3);
        assert!(results.derived.udp_fit.is_none());
    }

    #[test]
    fn consolidate_builds_udp_fit_rows_from_plain_wire_rows() {
        let root = temp_root("udp-fit");
        let wire = root.join("wire.json");
        let memory = root.join("memory.json");
        fs::write(&wire, WIRE_REPORT_JSON_WITH_PLAIN).unwrap();
        fs::write(&memory, MEMORY_REPORT_JSON).unwrap();

        let results = consolidate_with(&root, Some(wire), Some(memory));
        assert!(
            results.warnings.is_empty(),
            "warnings={:?}",
            results.warnings
        );
        let section = results
            .derived
            .udp_fit
            .as_ref()
            .expect("udp-fit section from plain wire rows");
        // Three response profiles x three signature configurations x four
        // comparison sizes.
        assert_eq!(section.rows.len(), 36);
        assert!(section.rows.iter().all(|r| r.messages > 0));
        assert_eq!(section.rtt_ms, None, "no RTT was supplied");
        let validation = section.validation.as_ref().expect("capture cross-check");
        assert_eq!(validation.tolerance_bytes, 0);
        assert!(
            validation.matches,
            "modelled {} vs recorded {} (full), {} vs {} (condensed)",
            validation.modeled_full_response_bytes,
            validation.recorded_full_response_bytes,
            validation.modeled_condensed_response_bytes,
            validation.recorded_condensed_response_bytes
        );
    }

    #[test]
    fn consolidate_without_wire_report_omits_udp_fit_silently() {
        let root = temp_root("udp-fit-no-wire");
        let criterion = root.join("criterion");
        fs::create_dir_all(&criterion).unwrap();
        let opts = ConsolidateOptions {
            force: true,
            criterion_dir: criterion,
            wire_report_path: None,
            memory_report_path: None,
            dns_capture_path: root.join("no-capture.json"),
            dns_fixture_path: root.join("no-fixture.json"),
            ladder_verification_path: root.join("no-ladder-live.json"),
            ladder_fixture_path: root.join("no-ladder-fixture.json"),
            rtt_ms: None,
            environment_path: root.join("no-env.json"),
            baseline: "new".to_string(),
            classical_signatures_path: root.join("no-classical-signatures.json"),
            output_path: root.join("out.json"),
        };
        let results = consolidate_results(opts).unwrap();
        assert!(
            results.warnings.is_empty(),
            "warnings={:?}",
            results.warnings
        );
        assert!(results.derived.udp_fit.is_none());
    }

    #[test]
    fn consolidate_memory_without_wire_row_leaves_payload_unmodelled() {
        let root = temp_root("memory-no-wire");
        let (_, memory) = write_report_inputs(&root);
        let results = consolidate_with(&root, None, Some(memory));

        assert_eq!(results.memory.measured.len(), 3);
        for m in &results.memory.measured {
            // Only the (n+1)-Vec overhead is modelled when no wire row exists.
            assert_eq!(
                m.harness_buffers_bytes,
                (m.message_count + 1) as u64 * 32,
                "size {}",
                m.size
            );
            assert_eq!(
                m.analytical_total_bytes,
                m.libmtl_analytical_bytes as u64
                    + m.harness_buffers_bytes
                    + m.dataset_buffers_bytes
                    + m.keypair_bytes
            );
        }
        let fallbacks = results
            .warnings
            .iter()
            .filter(|w| w.contains("no matching wire-size row"))
            .count();
        assert_eq!(fallbacks, 3, "warnings={:?}", results.warnings);
    }

    #[test]
    fn harness_estimate_payload_term_equals_wire_condensed_sum() {
        // Independent of consolidate: the payload handed to the estimate is the
        // wire condensed sum (367_059_712 for Large) and the total adds the
        // ladder plus (n+1) x 32 overhead.
        let est = climb_analytical::harness_output_estimate(1_000_000, 367_059_712, 8_120);
        assert_eq!(est.condensed_len_sum, 367_059_712);
        assert_eq!(est.payload_bytes, 367_067_832);
        assert_eq!(est.vec_overhead_bytes, 1_000_001 * 32);
        assert_eq!(est.estimated_total_bytes, 367_067_832 + 1_000_001 * 32);
    }

    fn dns_query(label: &str, bytes: u64) -> DnsCaptureQuery {
        DnsCaptureQuery {
            label: label.to_string(),
            qname: "www.climb.example.".to_string(),
            qtype: "A".to_string(),
            response_bytes: bytes,
            answer_rrsig_bytes: Some(69),
            flags: DnsCaptureFlags {
                ad: None,
                tc: None,
                rcode: Some(0),
            },
            transport: "tcp".to_string(),
            edns_udp_size: None,
            elapsed_ms: None,
            wire_hex: None,
        }
    }

    fn dns_capture_fixture(full: u64, condensed: u64) -> DnsCapture {
        DnsCapture {
            schema_version: 1,
            recorded_at: "2026-10-12T00:00:00Z".to_string(),
            git_commit: None,
            host: DnsCaptureHost {
                hostname: "test-host".to_string(),
                uname: "Linux test".to_string(),
            },
            images: Vec::new(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            zone: "climb.example.".to_string(),
            queries: vec![dns_query("full", full), dns_query("condensed", condensed)],
        }
    }

    #[test]
    fn dns_capture_roundtrip_equals_original() {
        let original = dns_capture_fixture(24_438, 658);
        let json = serde_json::to_string(&original).unwrap();
        let restored: DnsCapture = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    /// A capture written before the git-commit field existed must still parse
    /// (§7e backward compatibility): the field defaults to `None`, and the
    /// serializer omits it entirely rather than writing a fake value.
    #[test]
    fn dns_capture_without_git_commit_still_parses() {
        let legacy = serde_json::json!({
            "schema_version": 1,
            "recorded_at": "2026-10-12T00:00:00Z",
            "host": { "hostname": "test-host", "uname": "Linux test" },
            "images": [],
            "algorithm": "SLH-DSA-SHA2-128s-MTL-SHA2-128",
            "zone": "climb.example.",
            "queries": [],
        });
        let parsed: DnsCapture = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.git_commit, None);

        // The serializer never invents a commit either.
        let without = dns_capture_fixture(24_438, 658);
        let json = serde_json::to_string(&without).unwrap();
        assert!(!json.contains("git_commit"), "absent commit leaked: {json}");
    }

    /// A recording that resolved its commit carries it through serialize and
    /// parse unchanged.
    #[test]
    fn dns_capture_roundtrip_preserves_the_git_commit() {
        let mut original = dns_capture_fixture(24_438, 658);
        original.git_commit = Some("312c68268c96878b0ee01922d94c447cc9d0287e".to_string());
        let json = serde_json::to_string(&original).unwrap();
        let restored: DnsCapture = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert_eq!(
            restored.git_commit.as_deref(),
            Some("312c68268c96878b0ee01922d94c447cc9d0287e")
        );
    }

    #[test]
    fn headline_from_capture_computes_reduction_from_www_a_rows() {
        let capture = dns_capture_fixture(24_438, 658);
        let headline = headline_from_capture(&capture, DnsCaptureSource::Live);
        assert_eq!(headline.full_signature_bytes_www_a, Some(24_438));
        assert_eq!(headline.condensed_proof_bytes_www_a, Some(658));
        let pct = headline.reduction_percent_www_a.expect("some");
        assert!((pct - 97.307_37).abs() < 1e-3, "pct={pct}");
        assert_eq!(headline.source, DnsCaptureSource::Live);
        assert_eq!(
            headline.recorded_at.as_deref(),
            Some("2026-10-12T00:00:00Z")
        );
    }

    #[test]
    fn headline_from_capture_is_null_when_rows_absent() {
        let mut capture = dns_capture_fixture(24_438, 658);
        capture.queries.clear();
        let headline = headline_from_capture(&capture, DnsCaptureSource::RecordedFixture);
        assert_eq!(headline.full_signature_bytes_www_a, None);
        assert_eq!(headline.condensed_proof_bytes_www_a, None);
        assert_eq!(headline.reduction_percent_www_a, None);
        assert_eq!(headline.source, DnsCaptureSource::RecordedFixture);
    }

    #[test]
    fn headline_from_dns_renders_expected_sentence() {
        let dns = DnsPipelineHeadline {
            full_signature_bytes_www_a: Some(24_438),
            condensed_proof_bytes_www_a: Some(658),
            reduction_percent_www_a: Some(97.307_37),
            notes: None,
            source: DnsCaptureSource::RecordedFixture,
            recorded_at: Some("2026-10-12T00:00:00Z".to_string()),
            algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
        };
        let headline = headline_from_dns(&dns).expect("some");
        assert!(headline.contains("97.3%"), "headline={headline}");
        assert!(headline.contains("24,438 B"), "headline={headline}");
        assert!(headline.contains("658 B"), "headline={headline}");
    }

    #[test]
    fn derive_summary_headline_is_none_without_dns_capture() {
        let derived = derive_summary(&[], &[], &[], &[], None);
        assert_eq!(derived.headline, None);
    }

    #[test]
    fn load_dns_capture_prefers_live_over_fixture() {
        let root = temp_root("dns-live-preference");
        let live = root.join("live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &live,
            serde_json::to_string(&dns_capture_fixture(100, 10)).unwrap(),
        )
        .unwrap();
        fs::write(
            &fixture,
            serde_json::to_string(&dns_capture_fixture(999, 999)).unwrap(),
        )
        .unwrap();
        let headline = load_dns_capture(&live, &fixture).unwrap().expect("some");
        assert_eq!(headline.source, DnsCaptureSource::Live);
        assert_eq!(headline.full_signature_bytes_www_a, Some(100));
    }

    #[test]
    fn load_dns_capture_falls_back_to_recorded_fixture() {
        let root = temp_root("dns-fixture-fallback");
        let live = root.join("missing-live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &fixture,
            serde_json::to_string(&dns_capture_fixture(24_438, 658)).unwrap(),
        )
        .unwrap();
        let headline = load_dns_capture(&live, &fixture).unwrap().expect("some");
        assert_eq!(headline.source, DnsCaptureSource::RecordedFixture);
        assert_eq!(headline.full_signature_bytes_www_a, Some(24_438));
    }

    #[test]
    fn load_dns_capture_returns_none_when_neither_exists() {
        let root = temp_root("dns-none");
        let headline =
            load_dns_capture(&root.join("no-live.json"), &root.join("no-fixture.json")).unwrap();
        assert_eq!(headline, None);
    }

    #[test]
    fn load_ladder_section_prefers_live_run_over_fixture() {
        let root = temp_root("ladder-live-preference");
        let live = root.join("live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &live,
            serde_json::to_string(&ladder_document_fixture("2026-10-15T00:00:00+05:30")).unwrap(),
        )
        .unwrap();
        fs::write(
            &fixture,
            serde_json::to_string(&ladder_document_fixture("2026-10-14T00:16:11+05:30")).unwrap(),
        )
        .unwrap();
        let section = load_ladder_section(&live, &fixture).unwrap().expect("some");
        assert_eq!(section.source, LadderSource::Live);
        assert_eq!(section.recorded_at, "2026-10-15T00:00:00+05:30");
        assert_eq!(section.sizes.len(), 1);
        assert!(section.all_match);
    }

    #[test]
    fn load_ladder_section_falls_back_to_recorded_fixture() {
        let root = temp_root("ladder-fixture-fallback");
        let live = root.join("missing-live.json");
        let fixture = root.join("fixture.json");
        fs::write(
            &fixture,
            serde_json::to_string(&ladder_document_fixture("2026-10-14T00:16:11+05:30")).unwrap(),
        )
        .unwrap();
        let section = load_ladder_section(&live, &fixture).unwrap().expect("some");
        assert_eq!(section.source, LadderSource::RecordedFixture);
        assert_eq!(section.recorded_at, "2026-10-14T00:16:11+05:30");
    }

    #[test]
    fn load_ladder_section_returns_none_when_neither_exists() {
        let root = temp_root("ladder-none");
        let section =
            load_ladder_section(&root.join("no-live.json"), &root.join("no-fixture.json")).unwrap();
        assert_eq!(section, None);
    }

    #[test]
    fn load_ladder_section_is_a_hard_error_for_a_present_invalid_artifact() {
        let root = temp_root("ladder-invalid");
        let live = root.join("live.json");
        fs::write(&live, "{ not json").unwrap();
        let err = load_ladder_section(&live, &root.join("no-fixture.json")).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("live.json"),
            "error must name the path: {message}"
        );
    }

    #[test]
    fn consolidate_reports_ladder_source_and_model_verdict() {
        let root = temp_root("ladder-consolidate");
        std::fs::create_dir_all(root.join("criterion")).unwrap();
        let fixture = root.join("ladder-fixture.json");
        fs::write(
            &fixture,
            serde_json::to_string(&ladder_document_fixture("2026-10-14T00:16:11+05:30")).unwrap(),
        )
        .unwrap();
        let results = consolidate_results(ConsolidateOptions {
            force: true,
            criterion_dir: root.join("criterion"),
            wire_report_path: None,
            memory_report_path: None,
            dns_capture_path: root.join("no-capture.json"),
            dns_fixture_path: root.join("no-fixture.json"),
            ladder_verification_path: root.join("no-live.json"),
            rtt_ms: None,
            ladder_fixture_path: fixture,
            environment_path: root.join("no-env.json"),
            baseline: "new".to_string(),
            classical_signatures_path: root.join("no-classical-signatures.json"),
            output_path: root.join("out.json"),
        })
        .unwrap();

        let section = results.ladder.as_ref().expect("ladder section");
        assert_eq!(section.source, LadderSource::RecordedFixture);
        assert_eq!(section.sizes.len(), 1);
        let row = &section.sizes[0];
        assert_eq!(row.leaf_count, 8);
        assert!(row.model_matches);
        assert!(section.all_match);
    }

    #[test]
    fn results_without_a_ladder_section_still_deserialize() {
        // Backward compatibility: climb-results.json written before the ladder
        // section existed must still parse (§7e).
        let results = full_fixture();
        let mut without = results.clone();
        without.ladder = None;
        let json = serde_json::to_string(&without).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ladder, None);

        let mut with_section = results;
        let json = serde_json::to_string(&with_section).unwrap();
        let restored: ClimbResults = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ladder, with_section.ladder.take());
    }

    // -- configuration A timings and three-way comparison -------------------

    #[test]
    fn plain_timings_mark_measured_and_extrapolated_sizes() {
        let records = vec![
            make_plain_record("signing_small_plain_slh_dsa", 738_000_000.0),
            make_plain_record("signing_medium_plain_slh_dsa", 736_000_000.0),
            make_plain_record("verifying_small_plain_slh_dsa", 736_473.46),
        ];

        let rows = plain_timings(&records);
        assert_eq!(rows.len(), 4);

        let small = &rows[0];
        assert_eq!(small.size, "Small");
        assert_eq!(small.class, EvidenceClass::Measured);
        assert_eq!(small.samples, None);
        assert_eq!(small.signing_mean_ns, Some(738_000_000.0));
        assert_eq!(small.verifying_mean_ns, Some(736_473.46));

        let onek = &rows[1];
        assert_eq!(onek.size, "OneK");
        assert_eq!(onek.class, EvidenceClass::Extrapolated);
        assert_eq!(onek.samples, Some(2));
        assert_eq!(onek.signing_mean_ns, Some(737_000_000.0));
        assert_eq!(onek.signing_ci_lower_ns, None);
        assert_eq!(onek.verifying_mean_ns, Some(736_473.46));

        let medium = &rows[2];
        assert_eq!(medium.size, "Medium");
        assert_eq!(medium.class, EvidenceClass::Measured);
        assert_eq!(medium.signing_mean_ns, Some(736_000_000.0));

        let large = &rows[3];
        assert_eq!(large.size, "Large");
        assert_eq!(large.class, EvidenceClass::Extrapolated);
    }

    #[test]
    fn plain_timings_is_empty_without_plain_records() {
        assert!(plain_timings(&[]).is_empty());
    }

    #[test]
    fn build_comparisons_matches_wire_size_case_insensitively() {
        let wire = vec![WireSizeResult {
            size: "small".to_string(),
            message_count: 100,
            ladder_len: 8,
            condensed_min: 650,
            condensed_mean: 658.0,
            condensed_max: 660,
            condensed_uniform: true,
            with_mtl_total_bytes: 10_000,
            without_mtl_total_bytes: 815_632,
            reduction_percent: 99.0,
            plain_total_bytes: 1_000_000,
            plain_signature_len_bytes: 10_000,
        }];
        let signing = vec![
            signing_row("Small", "With MTL", 1_000_000.0),
            signing_row("Small", "Without MTL", 100_000_000.0),
        ];
        let verifying = vec![
            verifying_row("Small", "MTL (trust cached)", 100_000.0),
            verifying_row("Small", "With MTL", 500_000.0),
        ];
        let plain = vec![PlainTimingResult {
            size: "Small".to_string(),
            messages: 100,
            signing_mean_ns: Some(20_000.0),
            signing_ci_lower_ns: Some(19_000.0),
            signing_ci_upper_ns: Some(21_000.0),
            verifying_mean_ns: Some(50_000.0),
            verifying_ci_lower_ns: Some(49_000.0),
            verifying_ci_upper_ns: Some(51_000.0),
            class: EvidenceClass::Measured,
            samples: None,
        }];

        let comparisons = build_comparisons(&wire, &signing, &verifying, &plain);
        assert_eq!(comparisons.len(), 1);
        let c = &comparisons[0];
        assert_eq!(c.class, EvidenceClass::Derived);
        assert_eq!(c.wire_reduction_c_vs_a_percent, Some(99.0));
        assert_eq!(c.wire_reduction_c_vs_b_percent, Some(99.0));
        assert_eq!(c.signing_speedup_c_vs_a, Some(2.0));
        assert_eq!(c.signing_speedup_c_vs_b, Some(100.0));
        assert_eq!(c.verifying_speedup_cached_vs_a, Some(50.0));
        assert_eq!(c.verifying_speedup_cold_vs_a, Some(10.0));
    }

    #[test]
    fn build_comparisons_marks_extrapolated_when_mtl_cost_is_missing() {
        let plain = vec![PlainTimingResult {
            size: "Large".to_string(),
            messages: 1_000_000,
            signing_mean_ns: Some(737_000_000.0),
            signing_ci_lower_ns: None,
            signing_ci_upper_ns: None,
            verifying_mean_ns: Some(736_473.46),
            verifying_ci_lower_ns: None,
            verifying_ci_upper_ns: None,
            class: EvidenceClass::Extrapolated,
            samples: Some(2),
        }];

        let comparisons = build_comparisons(&[], &[], &[], &plain);
        assert_eq!(comparisons.len(), 1);
        let c = &comparisons[0];
        assert_eq!(c.class, EvidenceClass::Extrapolated);
        assert_eq!(c.signing_speedup_c_vs_a, None);
        assert_eq!(c.wire_reduction_c_vs_a_percent, None);
    }

    #[test]
    fn percent_and_ratio_helpers_guard_zero_denominators() {
        assert_eq!(percent_reduction(100, 25), Some(75.0));
        assert_eq!(percent_reduction(0, 25), None);
        assert_eq!(ratio(Some(10.0), Some(2.0)), Some(5.0));
        assert_eq!(ratio(Some(10.0), Some(0.0)), None);
        assert_eq!(ratio(None, Some(2.0)), None);
    }

    /// Per-size lookups must be keyed by the size label, never by position: a
    /// consumer that indexes `comparisons`/`wire_size`/`reuse`/`memory` by
    /// array position silently relabels every number when the artifact order
    /// changes. This shuffles each per-size array and asserts the keyed
    /// accessors still resolve the same row for every size.
    #[test]
    fn size_keyed_accessors_are_independent_of_array_order() {
        let sizes = ["Small", "Medium", "Large"];
        let build = |order: &[usize]| -> ClimbResults {
            let wire: Vec<WireSizeResult> = order
                .iter()
                .map(|&i| wire_row(&sizes[i].to_lowercase(), 90.0 + i as f64))
                .collect();
            let memory: Vec<MemoryResult> = order
                .iter()
                .map(|&i| {
                    let mut row = memory_row(&sizes[i].to_lowercase());
                    row.median_bytes = 1_000 * (i as u64 + 1);
                    row
                })
                .collect();
            let comparisons: Vec<ConfigComparison> = order
                .iter()
                .map(|&i| ConfigComparison {
                    size: sizes[i].to_string(),
                    wire_reduction_c_vs_a_percent: Some(90.0 + i as f64),
                    wire_reduction_c_vs_b_percent: Some(91.0 + i as f64),
                    signing_speedup_c_vs_a: Some(100.0 * (i as f64 + 1.0)),
                    signing_speedup_c_vs_b: Some(200.0 * (i as f64 + 1.0)),
                    verifying_speedup_cached_vs_a: Some(10.0 * (i as f64 + 1.0)),
                    verifying_speedup_cold_vs_a: Some(1.0 * (i as f64 + 1.0)),
                    class: EvidenceClass::Derived,
                })
                .collect();
            let reuse: Vec<ReuseResult> = order
                .iter()
                .map(|&i| ReuseResult {
                    size: sizes[i].to_lowercase(),
                    batch_size: 100,
                    curve: vec![ReuseCurvePoint {
                        reuse: 1,
                        effective_time_ns: 1.0,
                        effective_bytes: 1.0,
                    }],
                    breakeven_reuse_time: Some(i as u64 + 1),
                    breakeven_reuse_bytes: Some(i as u64 + 2),
                    r_star_time: None,
                    r_star_bytes: None,
                    baseline_time_ns: 1.0,
                    baseline_bytes: 1.0,
                    class: EvidenceClass::Derived,
                })
                .collect();
            let mut derived = derive_summary(&wire, &[], &[], &[], None);
            derived.reuse = reuse;
            ClimbResults {
                version: "1.0".to_string(),
                generated_at: "2026-10-12T00:00:00Z".to_string(),
                workspace_root: "/tmp/climb".to_string(),
                benchmark: BenchmarkResults {
                    signing: Vec::new(),
                    verifying: Vec::new(),
                },
                wire_size: wire,
                memory: MemoryResults {
                    measured: memory,
                    analytical_model: analytical_model(),
                },
                environment: environment_fingerprint(),
                dns_pipeline: None,
                dns_anatomy: None,
                ladder: None,
                plain: Vec::new(),
                classical: Vec::new(),
                comparisons,
                derived,
                warnings: Vec::new(),
            }
        };

        let canonical = build(&[0, 1, 2]);
        let shuffled = build(&[2, 0, 1]);

        for (i, size) in sizes.iter().enumerate() {
            let expected = 90.0 + i as f64;
            assert_eq!(
                canonical.wire_for(size).map(|r| r.reduction_percent),
                Some(expected),
                "canonical wire {size}"
            );
            assert_eq!(
                shuffled.wire_for(size).map(|r| r.reduction_percent),
                Some(expected),
                "shuffled wire {size} must key by size"
            );
            assert_eq!(
                shuffled
                    .comparison_for(size)
                    .and_then(|r| r.wire_reduction_c_vs_a_percent),
                Some(expected),
                "shuffled comparison {size}"
            );
            assert_eq!(
                canonical
                    .comparison_for(size)
                    .and_then(|r| r.signing_speedup_c_vs_a),
                Some(100.0 * (i as f64 + 1.0)),
                "signing speedup {size}"
            );
            assert_eq!(
                shuffled
                    .reuse_for(size)
                    .and_then(|r| r.breakeven_reuse_time),
                Some(i as u64 + 1),
                "shuffled reuse {size}"
            );
            assert_eq!(
                shuffled.memory_for(size).map(|r| r.median_bytes),
                Some(1_000 * (i as u64 + 1)),
                "shuffled memory {size}"
            );
        }
        // A size absent from the artifact resolves to no row, never to a
        // neighbouring one.
        assert!(shuffled.wire_for("OneK").is_none());
        assert!(shuffled.comparison_for("OneK").is_none());
    }

    #[test]
    fn evidence_class_serde_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&EvidenceClass::Extrapolated).unwrap(),
            "\"extrapolated\""
        );
        assert_eq!(EvidenceClass::Measured.as_str(), "measured");
        assert_eq!(EvidenceClass::Analytical.label(), "ANALYTICAL");
    }

    // -- cold/warm ladder-reuse model ----------------------------------------

    /// A wire row carrying the configuration-A plain signature length, which is
    /// what the byte axis of the reuse model is normalised against.
    fn reuse_wire_row(
        size: &str,
        message_count: usize,
        ladder_len: usize,
        sig_len: u64,
    ) -> WireSizeResult {
        WireSizeResult {
            size: size.to_string(),
            message_count,
            ladder_len,
            condensed_min: 100,
            condensed_mean: 164.32,
            condensed_max: 172,
            condensed_uniform: false,
            with_mtl_total_bytes: 24_424,
            without_mtl_total_bytes: 815_632,
            reduction_percent: 97.0,
            plain_total_bytes: sig_len * message_count as u64,
            plain_signature_len_bytes: sig_len,
        }
    }

    fn reuse_verifying_rows(size: &str, messages: u64) -> Vec<VerifyingBenchmarkResult> {
        // Cold verify is far more expensive than warm verify, so the time axis
        // breaks even at r = 1; the ladder alone is larger than one plain
        // signature, so the byte axis needs at least r = 2.
        vec![
            VerifyingBenchmarkResult {
                size: size.to_string(),
                condition: "With MTL".to_string(),
                messages,
                mean_ns: 6_000_000.0,
                ci_lower_ns: 5_900_000.0,
                ci_upper_ns: 6_100_000.0,
                speedup: None,
            },
            VerifyingBenchmarkResult {
                size: size.to_string(),
                condition: "MTL (trust cached)".to_string(),
                messages,
                mean_ns: 500_000.0,
                ci_lower_ns: 490_000.0,
                ci_upper_ns: 510_000.0,
                speedup: None,
            },
        ]
    }

    #[test]
    fn build_reuse_emits_curve_and_breakevens_for_a_measured_size() {
        let wire = vec![reuse_wire_row("small", 100, 7_992, 7_856)];
        let verifying = reuse_verifying_rows("Small", 100);
        let plain = vec![plain_row("Small")];

        let reuse = build_reuse(&wire, &verifying, &plain);
        assert_eq!(reuse.len(), 1);
        let r = &reuse[0];
        assert_eq!(r.size, "small");
        assert_eq!(r.batch_size, 100);
        assert_eq!(r.class, EvidenceClass::Derived);
        assert_eq!(r.baseline_bytes, 7_856.0);
        assert_eq!(r.baseline_time_ns, 736_473.46);

        // r = 1 is the cold verify (60_000 ns per message) which already beats
        // the 736_473.46 ns plain baseline, so the time axis breaks even first.
        assert_eq!(r.r_star_time, Some(1));
        assert_eq!(r.breakeven_reuse_time, Some(1));
        // r = 1 bytes = condensed (164.32) + ladder (7_992) > 7_856, so the byte
        // axis only breaks even at r = 2: 164.32 + 3_996 = 4_160.32.
        assert_eq!(r.r_star_bytes, Some(2));
        assert_eq!(r.breakeven_reuse_bytes, Some(2));

        // The candidates are the clipped, deduplicated ascending set including
        // the batch size itself.
        assert_eq!(
            r.curve.iter().map(|p| p.reuse).collect::<Vec<_>>(),
            vec![1, 2, 5, 10, 100]
        );
        let first = &r.curve[0];
        assert!((first.effective_time_ns - 60_000.0).abs() < 1e-9);
        assert!((first.effective_bytes - (164.32 + 7_992.0)).abs() < 1e-9);
    }

    #[test]
    fn build_reuse_marks_extrapolated_when_any_input_is_extrapolated() {
        let wire = vec![reuse_wire_row("small", 100, 7_992, 7_856)];
        let verifying = reuse_verifying_rows("Small", 100);
        let mut plain = vec![plain_row("Small")];
        plain[0].class = EvidenceClass::Extrapolated;

        let reuse = build_reuse(&wire, &verifying, &plain);
        assert_eq!(reuse.len(), 1);
        assert_eq!(reuse[0].class, EvidenceClass::Extrapolated);
    }

    #[test]
    fn build_reuse_skips_sizes_without_wire_or_verifying_inputs() {
        // No wire row -> no model.
        let verifying = reuse_verifying_rows("Small", 100);
        let plain = vec![plain_row("Small")];
        assert!(build_reuse(&[], &verifying, &plain).is_empty());

        // Wire row but no verifying conditions -> no cold/warm pair.
        let wire = vec![reuse_wire_row("small", 100, 7_992, 7_856)];
        assert!(build_reuse(&wire, &[], &plain).is_empty());

        // Wire row but no plain baseline -> no reference to break even against.
        assert!(build_reuse(&wire, &verifying, &[]).is_empty());
    }

    #[test]
    fn build_reuse_handles_a_reuse_count_of_one_short_circuiting_the_ratio() {
        let wire = vec![reuse_wire_row("tiny", 1, 100, 7_856)];
        let verifying = reuse_verifying_rows("Tiny", 1);
        let plain = vec![plain_row("Tiny")];
        let reuse = build_reuse(&wire, &verifying, &plain);
        assert_eq!(reuse.len(), 1);
        // With a batch of one the only candidate is r = 1.
        assert_eq!(reuse[0].batch_size, 1);
        assert_eq!(reuse[0].curve.len(), 1);
        assert_eq!(reuse[0].curve[0].reuse, 1);
    }

    #[test]
    fn results_without_a_reuse_block_still_deserialize() {
        // Backward compatibility: a derived summary written before the reuse
        // block existed must still parse (§7e).
        let results = full_fixture();
        let value = serde_json::to_value(&results).unwrap();
        let mut object = value.as_object().unwrap().clone();
        let derived = object
            .get_mut("derived")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .clone();
        let mut derived_without = derived;
        derived_without.remove("reuse");
        object.insert(
            "derived".to_string(),
            serde_json::Value::Object(derived_without),
        );
        let restored: ClimbResults =
            serde_json::from_value(serde_json::Value::Object(object)).unwrap();
        assert!(restored.derived.reuse.is_empty());
    }

    // -- classical context baselines -------------------------------------

    /// A Criterion record for a classical group: those benches use
    /// `BenchmarkId::from_parameter`, so the parameter string (`sign` or
    /// `verify`) lands in `value_str` and `function_id` stays null.
    fn make_classical_record(
        group_id: &str,
        parameter: &str,
        mean_ns: f64,
    ) -> crate::record::CriterionRecord {
        crate::record::CriterionRecord {
            group_id: group_id.to_string(),
            function_id: None,
            value_str: parameter.to_string(),
            full_id: format!("{group_id}/{parameter}"),
            elements: None,
            mean_ns,
            mean_ci_lower_ns: mean_ns * 0.9,
            mean_ci_upper_ns: mean_ns * 1.1,
            confidence_level: 0.95,
        }
    }

    /// A wire row carrying a plain SLH-DSA length (and a plain total), for
    /// sections that need configuration A's inputs.
    fn wire_row_with_plain(size: &str, plain_len: u64) -> WireSizeResult {
        WireSizeResult {
            plain_total_bytes: plain_len * 100,
            plain_signature_len_bytes: plain_len,
            ..wire_row(size, 97.0)
        }
    }

    /// Sign and verify records for all three classical algorithms at Medium.
    fn classical_medium_records() -> Vec<crate::record::CriterionRecord> {
        [
            ("classical_ed25519_medium", 1_100.0, 220.0),
            ("classical_ecdsa_p256_medium", 4_400.0, 880.0),
            ("classical_rsa2048_medium", 9_900.0, 330.0),
        ]
        .into_iter()
        .flat_map(|(group, sign, verify)| {
            [
                make_classical_record(group, "sign", sign),
                make_classical_record(group, "verify", verify),
            ]
        })
        .collect()
    }

    const TEST_LENGTHS: ClassicalSignatureLengths = ClassicalSignatureLengths {
        ed25519: 64,
        ecdsa_p256: 64,
        rsa2048: 256,
    };

    #[test]
    fn classical_timings_splits_sign_and_verify_by_parameter() {
        let records = vec![
            make_classical_record("classical_ed25519_small", "sign", 1_000.0),
            make_classical_record("classical_ed25519_small", "verify", 200.0),
            make_classical_record("classical_ecdsa_p256_medium", "sign", 4_000.0),
            make_classical_record("classical_ecdsa_p256_medium", "verify", 800.0),
            make_classical_record("classical_rsa2048_small", "sign", 9_000.0),
            make_classical_record("classical_rsa2048_small", "verify", 300.0),
        ];
        let rows = classical_timings(&records, &TEST_LENGTHS);
        assert_eq!(rows.len(), 3);
        // Row order is algorithm declaration order, size ascending within an
        // algorithm; the ECDSA group id parses despite its own underscore.
        assert_eq!(rows[0].algorithm, "Ed25519");
        assert_eq!(rows[0].size, "Small");
        assert_eq!(rows[0].signing_mean_ns, Some(1_000.0));
        assert_eq!(rows[0].verifying_mean_ns, Some(200.0));
        assert_eq!(rows[0].signature_len_bytes, 64);
        assert_eq!(rows[0].class, EvidenceClass::Measured);
        assert_eq!(rows[1].algorithm, "ECDSA P-256");
        assert_eq!(rows[1].size, "Medium");
        assert_eq!(rows[1].signing_mean_ns, Some(4_000.0));
        assert_eq!(rows[1].verifying_mean_ns, Some(800.0));
        assert_eq!(rows[2].algorithm, "RSA-2048");
        assert_eq!(rows[2].size, "Small");
        assert_eq!(rows[2].signing_mean_ns, Some(9_000.0));
        assert_eq!(rows[2].verifying_mean_ns, Some(300.0));
        assert_eq!(rows[2].signature_len_bytes, 256);
    }

    #[test]
    fn classical_timings_carries_ci_bounds_from_the_records() {
        let records = vec![
            make_classical_record("classical_ed25519_small", "sign", 1_000.0),
            make_classical_record("classical_ed25519_small", "verify", 200.0),
        ];
        let rows = classical_timings(&records, &TEST_LENGTHS);
        assert_eq!(rows.len(), 1);
        // The CI bounds flow from the record's own mean (computed as the
        // helper does, to stay exact in floating point).
        assert_eq!(rows[0].signing_ci_lower_ns, Some(1_000.0 * 0.9));
        assert_eq!(rows[0].signing_ci_upper_ns, Some(1_000.0 * 1.1));
        assert_eq!(rows[0].verifying_ci_lower_ns, Some(200.0 * 0.9));
        assert_eq!(rows[0].verifying_ci_upper_ns, Some(200.0 * 1.1));
    }

    #[test]
    fn classical_lengths_absent_warns_with_records_and_is_silent_without() {
        let root = temp_root("classical-missing");
        let path = root.join("climb-classical-signatures.json");
        let records = vec![make_classical_record(
            "classical_ed25519_small",
            "sign",
            1.0,
        )];

        let mut warnings = Vec::new();
        assert_eq!(
            load_classical_signature_lengths(&path, &records, &mut warnings).unwrap(),
            None
        );
        assert_eq!(warnings.len(), 1, "missing lengths must warn exactly once");
        assert!(
            warnings[0].contains(&path.display().to_string()),
            "the warning must name the path: {}",
            warnings[0]
        );

        let mut warnings = Vec::new();
        assert_eq!(
            load_classical_signature_lengths(&path, &[], &mut warnings).unwrap(),
            None
        );
        assert!(
            warnings.is_empty(),
            "no classical records means no expectation: {warnings:?}"
        );
    }

    #[test]
    fn classical_lengths_present_but_invalid_are_a_hard_error_naming_the_path() {
        let root = temp_root("classical-invalid");
        let unparseable = root.join("unparseable.json");
        fs::write(&unparseable, "not json at all").unwrap();
        let missing_field = root.join("missing-field.json");
        fs::write(&missing_field, r#"{"ed25519":64,"ecdsa_p256":64}"#).unwrap();
        let wrong_type = root.join("wrong-type.json");
        fs::write(
            &wrong_type,
            r#"{"ed25519":64,"ecdsa_p256":64,"rsa2048":"nope"}"#,
        )
        .unwrap();
        let zero = root.join("zero-length.json");
        fs::write(&zero, r#"{"ed25519":0,"ecdsa_p256":64,"rsa2048":256}"#).unwrap();

        for path in [&unparseable, &missing_field, &wrong_type, &zero] {
            let mut warnings = Vec::new();
            let err = load_classical_signature_lengths(path, &[], &mut warnings)
                .expect_err("a present-but-invalid document must be a hard error");
            match err {
                ReportError::JsonDecode { path: found, .. }
                | ReportError::MalformedData { path: found, .. } => {
                    assert_eq!(&found, path, "the error must name the path");
                }
                other => panic!("expected a path-naming error, got {other:?}"),
            }
        }
    }

    #[test]
    fn classical_lengths_flow_from_the_document_into_rows_and_context() {
        // Deliberately non-standard byte counts: consumers must read them
        // from the document, never reproduce the spec sizes on their own
        // (test data, deliberately unlike any real signature length).
        let root = temp_root("classical-read");
        let path = root.join("climb-classical-signatures.json");
        fs::write(&path, r#"{"ed25519":77,"ecdsa_p256":88,"rsa2048":99}"#).unwrap();

        let records = classical_medium_records();
        let mut warnings = Vec::new();
        let lengths = load_classical_signature_lengths(&path, &records, &mut warnings)
            .unwrap()
            .expect("the recorded document loads");
        assert!(warnings.is_empty());
        assert_eq!(
            lengths,
            ClassicalSignatureLengths {
                ed25519: 77,
                ecdsa_p256: 88,
                rsa2048: 99
            }
        );

        let rows = classical_timings(&records, &lengths);
        let from_rows: Vec<usize> = rows.iter().map(|r| r.signature_len_bytes).collect();
        assert_eq!(from_rows, [77, 88, 99]);

        let context = build_signature_context(&[], &[], &[], &[], &rows, None);
        let from_context: Vec<Option<f64>> = context.iter().map(|r| r.signature_bytes).collect();
        assert_eq!(from_context, [Some(77.0), Some(88.0), Some(99.0)]);
    }

    #[test]
    fn build_signature_context_orders_classical_then_configs_at_medium_only() {
        let wire = vec![
            wire_row_with_plain("Small", 1070),
            wire_row_with_plain("Medium", 1070),
        ];
        let signing = vec![
            signing_row("Medium", "Without MTL", 40_000_000.0),
            signing_row("Medium", "With MTL", 1_000_000.0),
        ];
        let verifying = vec![
            verifying_row("Medium", "Without MTL", 8_000_000.0),
            verifying_row("Medium", "With MTL", 200_000.0),
        ];
        let plain = vec![plain_row("Medium")];
        let classical = classical_timings(&classical_medium_records(), &TEST_LENGTHS);
        let mut warnings = Vec::new();
        let udp_fit = build_udp_fit(&wire, Some(&TEST_LENGTHS), None, &mut warnings);

        let rows = build_signature_context(
            &wire,
            &signing,
            &verifying,
            &plain,
            &classical,
            udp_fit.as_ref(),
        );
        let configs: Vec<&str> = rows.iter().map(|r| r.config.as_str()).collect();
        assert_eq!(
            configs,
            [
                "classical-ed25519",
                "classical-ecdsa-p256",
                "classical-rsa2048",
                "plain",
                "mtl-full",
                "mtl-amortised",
            ],
            "classical baselines first, then A, B, C"
        );
        assert!(rows.iter().all(|r| r.size == "Medium"), "Medium size only");
        assert_eq!(rows[0].family, "classical (context)");
        assert_eq!(rows[1].family, "classical (context)");
        assert_eq!(rows[2].family, "classical (context)");
        assert!(
            rows[3..].iter().all(|r| r.family == "post-quantum"),
            "A/B/C are post-quantum"
        );

        // Signature bytes per message: classical from the document, A from
        // the wire row's plain length, B/C from the wire totals per message.
        assert_eq!(rows[0].signature_bytes, Some(64.0));
        assert_eq!(rows[1].signature_bytes, Some(64.0));
        assert_eq!(rows[2].signature_bytes, Some(256.0));
        assert_eq!(rows[3].signature_bytes, Some(1070.0));
        assert_eq!(rows[4].signature_bytes, Some(24_438.0 / 100.0));
        assert_eq!(rows[5].signature_bytes, Some(658.0 / 100.0));

        // Per-message costs: A from the plain row, B/C from the condition
        // rows, classical from the classical rows.
        assert_eq!(rows[0].signing_mean_ns, Some(1_100.0));
        assert_eq!(rows[0].verifying_mean_ns, Some(220.0));
        assert_eq!(rows[3].signing_mean_ns, plain[0].signing_mean_ns);
        assert_eq!(rows[3].verifying_mean_ns, plain[0].verifying_mean_ns);
        assert_eq!(rows[4].signing_mean_ns, Some(400_000.0));
        assert_eq!(rows[4].verifying_mean_ns, Some(80_000.0));
        assert_eq!(rows[5].signing_mean_ns, Some(10_000.0));
        assert_eq!(rows[5].verifying_mean_ns, Some(2_000.0));

        // The fits cell is the modelled udp-fit row for the same config at
        // the Medium message count, under the headline profile.
        let udp = udp_fit.as_ref().expect("the section must model");
        for row in &rows {
            let expected = udp
                .rows
                .iter()
                .find(|r| {
                    r.profile == CONTEXT_FIT_PROFILE
                        && r.messages == 10_000
                        && r.config == row.config
                })
                .map(|r| r.fit_1232_percent);
            assert_eq!(
                row.fits_1232_percent, expected,
                "fits cell for {} must come from the modelled row",
                row.config
            );
            assert!(row.fits_1232_percent.is_some());
            assert_eq!(row.class, EvidenceClass::Measured);
        }

        // Without a udp-fit section the fits cell stays absent, never
        // fabricated.
        let without =
            build_signature_context(&wire, &signing, &verifying, &plain, &classical, None);
        assert!(without.iter().all(|r| r.fits_1232_percent.is_none()));
    }

    #[test]
    fn build_signature_context_is_empty_without_classical_rows() {
        // The context table exists to frame the post-quantum numbers with
        // classical context; without classical rows it is not built at all.
        let wire = vec![wire_row_with_plain("Medium", 1070)];
        let plain = vec![plain_row("Medium")];
        let rows = build_signature_context(&wire, &[], &[], &plain, &[], None);
        assert!(rows.is_empty());
    }

    #[test]
    fn build_udp_fit_models_each_classical_config_with_its_own_length() {
        let wire = vec![wire_row_with_plain("Medium", 1070)];
        let mut warnings = Vec::new();
        let without = build_udp_fit(&wire, None, None, &mut warnings).expect("section");
        let configs: std::collections::BTreeSet<&str> =
            without.rows.iter().map(|r| r.config.as_str()).collect();
        assert_eq!(
            configs.into_iter().collect::<Vec<_>>(),
            ["mtl-amortised", "mtl-full", "plain"]
        );

        let mut warnings = Vec::new();
        let with = build_udp_fit(&wire, Some(&TEST_LENGTHS), None, &mut warnings).expect("section");
        let configs: std::collections::BTreeSet<&str> =
            with.rows.iter().map(|r| r.config.as_str()).collect();
        assert_eq!(
            configs.into_iter().collect::<Vec<_>>(),
            [
                "classical-ecdsa-p256",
                "classical-ed25519",
                "classical-rsa2048",
                "mtl-amortised",
                "mtl-full",
                "plain",
            ]
        );
        assert!(
            warnings
                .iter()
                .all(|w| !w.contains("signature length is missing")),
            "no classical-length warning: {warnings:?}"
        );

        // Each classical config is modelled with its own fed length: RSA's
        // 256-byte signature must exceed the 64-byte ones at every size.
        let rsa = with
            .rows
            .iter()
            .find(|r| r.config == "classical-rsa2048" && r.profile == CONTEXT_FIT_PROFILE)
            .expect("rsa row");
        let ed = with
            .rows
            .iter()
            .find(|r| r.config == "classical-ed25519" && r.profile == CONTEXT_FIT_PROFILE)
            .expect("ed25519 row");
        assert!(rsa.max_response_bytes > ed.max_response_bytes);
    }
}
