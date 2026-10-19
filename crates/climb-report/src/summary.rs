//! Group-id parsing and summary row construction.
//!
//! Converts raw [`CriterionRecord`]s into categorized [`SummaryRow`]s by
//! parsing Criterion's `group_id` naming convention.

use std::fmt;

use crate::error::ReportError;
use crate::record::CriterionRecord;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Whether a benchmark measures signing or verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchKind {
    Signing,
    Verifying,
}

/// Dataset size dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Size {
    Small,
    OneK,
    Medium,
    Large,
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Size::Small => write!(f, "Small"),
            Size::OneK => write!(f, "OneK"),
            Size::Medium => write!(f, "Medium"),
            Size::Large => write!(f, "Large"),
        }
    }
}

/// MTL usage mode derived from the benchmark group id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MtlMode {
    /// Verifying with a previously validated ladder (no re-validation).
    WithMtlTrustCached,
    /// Signing with MTL, or verifying while re-validating the ladder.
    WithMtlFullVerify,
    /// One signature/verification per message (no amortization).
    WithoutMtl,
}

impl fmt::Display for MtlMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MtlMode::WithMtlTrustCached => write!(f, "MTL (trust cached)"),
            MtlMode::WithMtlFullVerify => write!(f, "With MTL"),
            MtlMode::WithoutMtl => write!(f, "Without MTL"),
        }
    }
}

impl MtlMode {
    /// Space-constrained descriptive label for plain-text tables.
    ///
    /// [`Self::Display`] keeps the canonical, artifact-stable names; this form
    /// names *what the condition signs* and is used where the full sentence
    /// would not fit.
    pub fn short_label(self) -> &'static str {
        match self {
            MtlMode::WithMtlTrustCached => "MTL (trust cached)",
            MtlMode::WithMtlFullVerify => SHORT_LABEL_WITH_MTL,
            MtlMode::WithoutMtl => SHORT_LABEL_WITHOUT_MTL,
        }
    }
}

/// Descriptive label for the amortised condition: one signed ladder is shared
/// across the batch and each message carries only a condensed proof.
pub const LABEL_WITH_MTL: &str = "MTL amortised (condensed signature + one signed ladder)";

/// Descriptive label for the non-amortised condition: every message carries a
/// complete MTL signature (a condensed proof concatenated with the ladder).
///
/// This is the MTL-aware full-signature mode, *not* a plain SLH-DSA RRSIG.
pub const LABEL_WITHOUT_MTL: &str = "No amortisation (full MTL signature on every message)";

/// Short form of [`LABEL_WITH_MTL`] for space-constrained surfaces.
pub const SHORT_LABEL_WITH_MTL: &str = "MTL amortised";

/// Short form of [`LABEL_WITHOUT_MTL`] for space-constrained surfaces.
pub const SHORT_LABEL_WITHOUT_MTL: &str = "No amortisation";

/// Map a canonical results condition name to its descriptive label.
///
/// The results artifact stores the canonical names (`"With MTL"`,
/// `"Without MTL"`); every user-facing surface maps them here at render time
/// so the artifact schema stays stable. Conditions with no descriptive form
/// (e.g. `"MTL (trust cached)"`) pass through unchanged.
pub fn condition_label(condition: &str) -> &str {
    match condition {
        "With MTL" => LABEL_WITH_MTL,
        "Without MTL" => LABEL_WITHOUT_MTL,
        other => other,
    }
}

/// Short form of [`condition_label`] for space-constrained surfaces.
pub fn condition_short_label(condition: &str) -> &str {
    match condition {
        "With MTL" => SHORT_LABEL_WITH_MTL,
        "Without MTL" => SHORT_LABEL_WITHOUT_MTL,
        other => other,
    }
}

// ---------------------------------------------------------------------------
// SummaryRow
// ---------------------------------------------------------------------------

/// One row in the rendered summary table.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryRow {
    /// Signing or verifying.
    pub kind: BenchKind,
    /// Dataset size.
    pub size: Size,
    /// MTL mode.
    pub mode: MtlMode,
    /// Mean iteration time in nanoseconds.
    pub mean_ns: f64,
    /// Lower 95% CI bound in nanoseconds.
    pub ci_lower_ns: f64,
    /// Upper 95% CI bound in nanoseconds.
    pub ci_upper_ns: f64,
    /// Number of messages (elements) if reported.
    pub elements: Option<u64>,
}

// ---------------------------------------------------------------------------
// Group-id parsing
// ---------------------------------------------------------------------------

/// Parse a Criterion group_id into its semantic components.
///
/// Recognized patterns:
/// ```text
/// signing_{size}_with_mtl
/// signing_{size}_without_mtl
/// verifying_{size}_with_mtl_trust_true
/// verifying_{size}_with_mtl_trust_false
/// verifying_{size}_without_mtl
/// ```
///
/// Exposed publicly so callers (e.g. the `xtask report` command) can skip
/// unrelated Criterion output left over from earlier benchmark layouts, while
/// [`summarize`] keeps its strict, all-or-nothing contract.
pub fn parse_group_id(id: &str) -> Result<(BenchKind, Size, MtlMode), ReportError> {
    if let Some(rest) = id.strip_prefix("signing_") {
        let (size, mode) = parse_signing_suffix(rest, id)?;
        Ok((BenchKind::Signing, size, mode))
    } else if let Some(rest) = id.strip_prefix("verifying_") {
        let (size, mode) = parse_verifying_suffix(rest, id)?;
        Ok((BenchKind::Verifying, size, mode))
    } else {
        Err(ReportError::UnknownGroupId { id: id.to_string() })
    }
}

fn parse_signing_suffix(s: &str, full_id: &str) -> Result<(Size, MtlMode), ReportError> {
    let (size_str, mode_str) =
        split_size_and_suffix(s).ok_or_else(|| ReportError::UnknownGroupId {
            id: full_id.to_string(),
        })?;
    let size = parse_size(size_str).ok_or_else(|| ReportError::UnknownGroupId {
        id: full_id.to_string(),
    })?;
    match mode_str {
        "with_mtl" => Ok((size, MtlMode::WithMtlFullVerify)),
        "without_mtl" => Ok((size, MtlMode::WithoutMtl)),
        _ => Err(ReportError::UnknownGroupId {
            id: full_id.to_string(),
        }),
    }
}

fn parse_verifying_suffix(s: &str, full_id: &str) -> Result<(Size, MtlMode), ReportError> {
    if s.contains("_trust_true") {
        let base = s.strip_suffix("_trust_true").unwrap();
        let (size_str, _) =
            split_size_and_suffix(base).ok_or_else(|| ReportError::UnknownGroupId {
                id: full_id.to_string(),
            })?;
        let size = parse_size(size_str).ok_or_else(|| ReportError::UnknownGroupId {
            id: full_id.to_string(),
        })?;
        Ok((size, MtlMode::WithMtlTrustCached))
    } else if s.contains("_trust_false") {
        let base = s.strip_suffix("_trust_false").unwrap();
        let (size_str, _) =
            split_size_and_suffix(base).ok_or_else(|| ReportError::UnknownGroupId {
                id: full_id.to_string(),
            })?;
        let size = parse_size(size_str).ok_or_else(|| ReportError::UnknownGroupId {
            id: full_id.to_string(),
        })?;
        Ok((size, MtlMode::WithMtlFullVerify))
    } else {
        let (size_str, mode_str) =
            split_size_and_suffix(s).ok_or_else(|| ReportError::UnknownGroupId {
                id: full_id.to_string(),
            })?;
        let size = parse_size(size_str).ok_or_else(|| ReportError::UnknownGroupId {
            id: full_id.to_string(),
        })?;
        match mode_str {
            "without_mtl" => Ok((size, MtlMode::WithoutMtl)),
            _ => Err(ReportError::UnknownGroupId {
                id: full_id.to_string(),
            }),
        }
    }
}

/// Suffix shared by every configuration-A (plain SLH-DSA) group id.
pub const PLAIN_SUFFIX: &str = "plain_slh_dsa";

/// Recognize a configuration-A (`plain_slh_dsa`) group id.
///
/// Configuration A signs or verifies a *single* message with a plain SLH-DSA
/// signature (no ladder), so its Criterion records carry a per-message cost
/// rather than a batch total. They are kept out of [`summarize`]'s
/// batch-oriented [`SummaryRow`]s and parsed separately:
///
/// ```text
/// signing_{size}_plain_slh_dsa
/// verifying_{size}_plain_slh_dsa
/// ```
///
/// Returns `None` for any other id, including the MTL groups handled by
/// [`parse_group_id`].
pub fn parse_plain_group_id(id: &str) -> Option<(BenchKind, Size)> {
    let (kind, rest) = if let Some(rest) = id.strip_prefix("signing_") {
        (BenchKind::Signing, rest)
    } else {
        (BenchKind::Verifying, id.strip_prefix("verifying_")?)
    };
    let (size_str, suffix) = split_size_and_suffix(rest)?;
    if suffix != PLAIN_SUFFIX {
        return None;
    }
    Some((kind, parse_size(size_str)?))
}

/// Classical algorithm keys (context only, not PQC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClassicalAlgo {
    Ed25519,
    EcdsaP256,
    Rsa2048,
}

impl fmt::Display for ClassicalAlgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClassicalAlgo::Ed25519 => write!(f, "Ed25519"),
            ClassicalAlgo::EcdsaP256 => write!(f, "ECDSA P-256"),
            ClassicalAlgo::Rsa2048 => write!(f, "RSA-2048"),
        }
    }
}

/// Parse classical group id: `classical_{algo}_{size}` where algo is
/// `ed25519`, `ecdsa_p256`, `rsa2048`.
pub fn parse_classical_group_id(id: &str) -> Option<(ClassicalAlgo, Size)> {
    let rest = id.strip_prefix("classical_")?;
    // Longest algorithm fragments first: `ecdsa_p256` contains an underscore,
    // so splitting at the first underscore would misread its size.
    const ALGORITHMS: [(&str, ClassicalAlgo); 3] = [
        ("ecdsa_p256_", ClassicalAlgo::EcdsaP256),
        ("ed25519_", ClassicalAlgo::Ed25519),
        ("rsa2048_", ClassicalAlgo::Rsa2048),
    ];
    for (fragment, algo) in ALGORITHMS {
        if let Some(size_str) = rest.strip_prefix(fragment) {
            return Some((algo, parse_size(size_str)?));
        }
    }
    None
}

/// Given `"small_with_mtl"`, returns `Some(("small", "with_mtl"))`.
fn split_size_and_suffix(s: &str) -> Option<(&str, &str)> {
    let first_underscore = s.find('_')?;
    Some((&s[..first_underscore], &s[first_underscore + 1..]))
}

fn parse_size(s: &str) -> Option<Size> {
    match s {
        "small" => Some(Size::Small),
        "onek" => Some(Size::OneK),
        "medium" => Some(Size::Medium),
        "large" => Some(Size::Large),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Summarize
// ---------------------------------------------------------------------------

/// Convert raw Criterion records into categorized summary rows.
///
/// Every record's group_id must parse via [`parse_group_id`]; unrecognized
/// group ids produce an error rather than being silently dropped.
pub fn summarize(records: &[CriterionRecord]) -> Result<Vec<SummaryRow>, ReportError> {
    let mut rows: Vec<SummaryRow> = Vec::with_capacity(records.len());

    for rec in records {
        let (kind, size, mode) = parse_group_id(&rec.group_id)?;
        rows.push(SummaryRow {
            kind,
            size,
            mode,
            mean_ns: rec.mean_ns,
            ci_lower_ns: rec.mean_ci_lower_ns,
            ci_upper_ns: rec.mean_ci_upper_ns,
            elements: rec.elements,
        });
    }

    rows.sort_by_key(|r| (r.kind, r.size, r.mode));

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(group_id: &str, mean_ns: f64) -> CriterionRecord {
        CriterionRecord {
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

    // -- parse_group_id ----------------------------------------------------

    #[test]
    fn parse_signing_small_with_mtl() {
        let (kind, size, mode) = parse_group_id("signing_small_with_mtl").unwrap();
        assert_eq!(kind, BenchKind::Signing);
        assert_eq!(size, Size::Small);
        assert_eq!(mode, MtlMode::WithMtlFullVerify);
    }

    #[test]
    fn parse_signing_medium_without_mtl() {
        let (kind, size, mode) = parse_group_id("signing_medium_without_mtl").unwrap();
        assert_eq!(kind, BenchKind::Signing);
        assert_eq!(size, Size::Medium);
        assert_eq!(mode, MtlMode::WithoutMtl);
    }

    #[test]
    fn parse_signing_onek_with_mtl() {
        let (kind, size, mode) = parse_group_id("signing_onek_with_mtl").unwrap();
        assert_eq!(kind, BenchKind::Signing);
        assert_eq!(size, Size::OneK);
        assert_eq!(mode, MtlMode::WithMtlFullVerify);
    }

    #[test]
    fn parse_verifying_small_trust_true() {
        let (kind, size, mode) = parse_group_id("verifying_small_with_mtl_trust_true").unwrap();
        assert_eq!(kind, BenchKind::Verifying);
        assert_eq!(size, Size::Small);
        assert_eq!(mode, MtlMode::WithMtlTrustCached);
    }

    #[test]
    fn parse_verifying_large_trust_false() {
        let (kind, size, mode) = parse_group_id("verifying_large_with_mtl_trust_false").unwrap();
        assert_eq!(kind, BenchKind::Verifying);
        assert_eq!(size, Size::Large);
        assert_eq!(mode, MtlMode::WithMtlFullVerify);
    }

    #[test]
    fn parse_verifying_medium_without_mtl() {
        let (kind, size, mode) = parse_group_id("verifying_medium_without_mtl").unwrap();
        assert_eq!(kind, BenchKind::Verifying);
        assert_eq!(size, Size::Medium);
        assert_eq!(mode, MtlMode::WithoutMtl);
    }

    #[test]
    fn parse_group_id_garbage_returns_error() {
        assert!(parse_group_id("garbage").is_err());
        assert!(parse_group_id("signing_giga_with_mtl").is_err());
        assert!(parse_group_id("verifying_small_trust_unknown").is_err());
    }

    // -- summarize ---------------------------------------------------------

    #[test]
    fn summarize_ordering_is_deterministic() {
        let records = vec![
            make_record("signing_large_with_mtl", 1000.0),
            make_record("signing_small_with_mtl", 100.0),
            make_record("signing_medium_with_mtl", 500.0),
        ];

        let sorted = summarize(&records).unwrap();
        assert_eq!(sorted[0].size, Size::Small);
        assert_eq!(sorted[1].size, Size::Medium);
        assert_eq!(sorted[2].size, Size::Large);
    }

    // -- parse_plain_group_id ---------------------------------------------

    #[test]
    fn parse_plain_group_id_accepts_measured_sizes() {
        assert_eq!(
            parse_plain_group_id("signing_small_plain_slh_dsa"),
            Some((BenchKind::Signing, Size::Small))
        );
        assert_eq!(
            parse_plain_group_id("signing_medium_plain_slh_dsa"),
            Some((BenchKind::Signing, Size::Medium))
        );
        assert_eq!(
            parse_plain_group_id("verifying_small_plain_slh_dsa"),
            Some((BenchKind::Verifying, Size::Small))
        );
        assert_eq!(
            parse_plain_group_id("verifying_medium_plain_slh_dsa"),
            Some((BenchKind::Verifying, Size::Medium))
        );
    }

    #[test]
    fn parse_plain_group_id_rejects_mtl_and_malformed_ids() {
        assert_eq!(parse_plain_group_id("signing_small_with_mtl"), None);
        assert_eq!(
            parse_plain_group_id("verifying_small_with_mtl_trust_true"),
            None
        );
        assert_eq!(parse_plain_group_id("signing_small_plain"), None);
        assert_eq!(parse_plain_group_id("signing_giga_plain_slh_dsa"), None);
        assert_eq!(parse_plain_group_id("garbage"), None);
    }

    // -- parse_classical_group_id -----------------------------------------

    #[test]
    fn parse_classical_group_id_accepts_every_algorithm_and_size() {
        for (fragment, algo) in [
            ("ed25519", ClassicalAlgo::Ed25519),
            ("ecdsa_p256", ClassicalAlgo::EcdsaP256),
            ("rsa2048", ClassicalAlgo::Rsa2048),
        ] {
            for (size_str, size) in [
                ("small", Size::Small),
                ("onek", Size::OneK),
                ("medium", Size::Medium),
                ("large", Size::Large),
            ] {
                let id = format!("classical_{fragment}_{size_str}");
                assert_eq!(
                    parse_classical_group_id(&id),
                    Some((algo, size)),
                    "{id} must parse"
                );
            }
        }
    }

    #[test]
    fn parse_classical_group_id_splits_ecdsa_p256_at_the_full_algorithm_fragment() {
        // Regression: splitting at the first underscore misread
        // `ecdsa_p256_medium` as algorithm `ecdsa` with size `p256_medium`,
        // dropping every ECDSA row.
        assert_eq!(
            parse_classical_group_id("classical_ecdsa_p256_medium"),
            Some((ClassicalAlgo::EcdsaP256, Size::Medium))
        );
        assert_eq!(
            parse_classical_group_id("classical_ecdsa_p256_small"),
            Some((ClassicalAlgo::EcdsaP256, Size::Small))
        );
    }

    #[test]
    fn parse_classical_group_id_rejects_unknown_algorithms_and_sizes() {
        assert_eq!(parse_classical_group_id("classical_dilithium_small"), None);
        assert_eq!(parse_classical_group_id("classical_ed25519_giga"), None);
        assert_eq!(parse_classical_group_id("classical_ed25519"), None);
        assert_eq!(parse_classical_group_id("classical__small"), None);
        assert_eq!(parse_classical_group_id("garbage"), None);
        assert_eq!(
            parse_classical_group_id("signing_small_plain_slh_dsa"),
            None
        );
    }
}
