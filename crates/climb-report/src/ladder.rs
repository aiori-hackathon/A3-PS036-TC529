//! The recorded MTL ladder-verification artifact.
//!
//! `cargo xtask ladder-verify` signs a deterministic dataset at a range of leaf
//! counts, measures the real ladder shape (see `climb-bench`'s ladder module),
//! and records the result in this document. The per-size measurement rows come
//! from [`climb_bench::LadderRow`]; this module adds the recording metadata
//! (schema version, timestamp, algorithm, submodule provenance) that has
//! nothing to do with the measurement itself.
//!
//! The document deliberately contains no measured numbers of its own: every
//! byte count in it is decoded from real libMTL output, and the closed-form
//! model in `climb-analytical` is checked against those rows by a test.

use serde::{Deserialize, Serialize};

/// Current `LadderVerification` schema version. Bump on breaking changes.
pub const LADDER_VERIFICATION_SCHEMA_VERSION: u32 = 1;

/// A recorded ladder-shape measurement across one or more leaf counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderVerification {
    /// Schema version for this document.
    pub schema_version: u32,
    /// RFC 3339 timestamp of the recording.
    pub recorded_at: String,
    /// libMTL algorithm used for the measurement.
    pub algorithm: String,
    /// Hash size in bytes implied by the algorithm.
    pub hash_size: usize,
    /// Seed used for deterministic dataset generation.
    pub seed: u64,
    /// Resolved submodule pins at measurement time.
    pub provenance: LadderProvenance,
    /// One measurement row per leaf count, in requested order.
    pub sizes: Vec<climb_bench::LadderRow>,
}

impl LadderVerification {
    /// The measured row for `leaf_count`, if it was measured.
    #[must_use]
    pub fn row(&self, leaf_count: usize) -> Option<&climb_bench::LadderRow> {
        self.sizes.iter().find(|r| r.leaf_count == leaf_count)
    }
}

/// Where a recorded ladder-verification artifact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LadderSource {
    /// A live `cargo xtask ladder-verify` run.
    Live,
    /// The committed `data/ladder-verification.recorded.json` fixture.
    RecordedFixture,
}

/// One size's measured ladder shape plus the model-agreement verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderSizeRow {
    /// Leaf count (messages) the row was measured at.
    pub leaf_count: usize,
    /// Number of ladder rungs covering the leaves.
    pub rung_count: usize,
    /// Serialized ladder-buffer length in bytes.
    pub ladder_buffer_len: usize,
    /// Ladder plus one full SLH-DSA signature, in bytes.
    pub signed_ladder_len: usize,
    /// Shortest condensed-signature length, in bytes.
    pub condensed_min: usize,
    /// Longest condensed-signature length, in bytes.
    pub condensed_max: usize,
    /// Mean condensed-signature length, in bytes.
    pub condensed_mean: f64,
    /// Whether the closed-form model reproduces this row's shape.
    pub model_matches: bool,
}

/// The consolidated ladder section: the measured ladder shape per size and
/// whether the closed-form model reproduces every row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderSection {
    /// Whether a live run or the committed fixture supplied the numbers.
    pub source: LadderSource,
    /// RFC 3339 timestamp of the recording.
    pub recorded_at: String,
    /// libMTL algorithm the recording used.
    pub algorithm: String,
    /// Whether the closed-form model reproduces every measured row.
    pub all_match: bool,
    /// One row per measured leaf count, in the artifact's order.
    pub sizes: Vec<LadderSizeRow>,
}

impl LadderSection {
    /// Build the consolidated section from a recorded document and its source.
    #[must_use]
    pub fn from_document(document: &LadderVerification, source: LadderSource) -> Self {
        let sizes: Vec<LadderSizeRow> = document
            .sizes
            .iter()
            .map(|row| LadderSizeRow {
                leaf_count: row.leaf_count,
                rung_count: row.rung_count,
                ladder_buffer_len: row.ladder_buffer_len,
                signed_ladder_len: row.signed_ladder_len,
                condensed_min: row.condensed.min,
                condensed_max: row.condensed.max,
                condensed_mean: row.condensed.mean,
                model_matches: model_matches(row),
            })
            .collect();
        let all_match = !sizes.is_empty() && sizes.iter().all(|row| row.model_matches);
        Self {
            source,
            recorded_at: document.recorded_at.clone(),
            algorithm: document.algorithm.clone(),
            all_match,
            sizes,
        }
    }
}

/// Whether the closed-form model reproduces a measured ladder row.
///
/// The model is the single source of truth for the ladder shape: this checks
/// the ladder-buffer length and the min/max/total/uniform summary of the
/// predicted condensed-signature lengths over every leaf. It is the one place
/// the comparison is written, shared by the TUI badge and consolidation.
#[must_use]
pub fn model_matches(row: &climb_bench::LadderRow) -> bool {
    let n = row.leaf_count;
    if n == 0 {
        return false;
    }
    if climb_analytical::predicted_ladder_len(n as u64) != row.ladder_buffer_len {
        return false;
    }
    let mut total: u64 = 0;
    let mut min = usize::MAX;
    let mut max = 0usize;
    for index in 0..n {
        let Some(length) = climb_analytical::predicted_condensed_len(n as u64, index as u64) else {
            return false;
        };
        total += length as u64;
        min = min.min(length);
        max = max.max(length);
    }
    min == row.condensed.min
        && max == row.condensed.max
        && total == row.condensed.total
        && (min == max) == row.condensed.uniform
}

/// Provenance of a ladder-verification recording: the source pins the measured
/// bytes were produced from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderProvenance {
    /// Resolved git submodule pins, sorted by path.
    pub submodules: Vec<SubmoduleRef>,
}

/// One resolved git submodule pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmoduleRef {
    /// Submodule path relative to the workspace root.
    pub path: String,
    /// Resolved commit SHA.
    pub sha: String,
    /// `git describe`-style label, when available.
    pub describe: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use climb_bench::{LadderRow, LadderRung};

    fn sample_row(leaf_count: usize) -> LadderRow {
        LadderRow {
            leaf_count,
            rung_count: 1,
            rungs: vec![LadderRung {
                left: 0,
                right: (leaf_count - 1) as u64,
            }],
            ladder_buffer_len: 4 + 2 * 16 + (16 + 16),
            signed_ladder_len: 4 + 2 * 16 + 32 + 4 + 7856,
            signature_len: 7856,
            condensed: climb_bench::CondensedStats {
                min: 76,
                max: 76,
                mean: 76.0,
                total: 76 * leaf_count as u64,
                uniform: true,
            },
            full_len_min: 76 + 4 + 2 * 16 + 32 + 4 + 7856,
            full_len_max: 76 + 4 + 2 * 16 + 32 + 4 + 7856,
            with_mtl_signatures: 1,
            without_mtl_signatures: leaf_count,
        }
    }

    fn sample() -> LadderVerification {
        LadderVerification {
            schema_version: LADDER_VERIFICATION_SCHEMA_VERSION,
            recorded_at: "2026-10-13T00:00:00+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            hash_size: 16,
            seed: 0x434C_494D_425F_5635,
            provenance: LadderProvenance {
                submodules: vec![
                    SubmoduleRef {
                        path: "third_party/MTL".to_string(),
                        sha: "53ce25dcc35c15e051c5263e41aaeaede18a8aa7".to_string(),
                        describe: Some("MTLLIB_1_2_1-3-g53ce25d".to_string()),
                    },
                    SubmoduleRef {
                        path: "third_party/liboqs".to_string(),
                        sha: "5a1a854b0dc9f2141bdc771c555ee60c37950183".to_string(),
                        describe: Some("0.16.0".to_string()),
                    },
                ],
            },
            sizes: vec![sample_row(1), sample_row(4)],
        }
    }

    #[test]
    fn roundtrips_through_json() {
        let original = sample();
        let json = serde_json::to_string(&original).unwrap();
        let restored: LadderVerification = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn row_lookup_finds_measured_size_only() {
        let doc = sample();
        assert!(doc.row(4).is_some());
        assert!(doc.row(3).is_none());
    }

    /// A [`LadderRow`] whose shape the closed-form model reproduces exactly,
    /// built from the model itself so the test data is never a hardcoded copy.
    fn modeled_row(leaf_count: usize) -> LadderRow {
        let n = leaf_count as u64;
        let model: Vec<usize> = (0..n)
            .map(|index| climb_analytical::predicted_condensed_len(n, index).unwrap_or(0))
            .collect();
        let min = model.iter().copied().min().unwrap_or(0);
        let max = model.iter().copied().max().unwrap_or(0);
        let total: u64 = model.iter().map(|&length| length as u64).sum();
        LadderRow {
            leaf_count,
            rung_count: climb_analytical::rungs(n).len(),
            rungs: Vec::new(),
            ladder_buffer_len: climb_analytical::predicted_ladder_len(n),
            signed_ladder_len: climb_analytical::predicted_ladder_len(n) + 4 + 7856,
            signature_len: 7856,
            condensed: climb_bench::CondensedStats {
                min,
                max,
                mean: total as f64 / leaf_count as f64,
                total,
                uniform: min == max,
            },
            full_len_min: 0,
            full_len_max: 0,
            with_mtl_signatures: 1,
            without_mtl_signatures: leaf_count,
        }
    }

    #[test]
    fn model_matches_accepts_every_modelled_size() {
        for n in [1, 2, 3, 4, 5, 8, 13, 16, 31] {
            assert!(model_matches(&modeled_row(n)), "N={n} must match");
        }
    }

    #[test]
    fn model_matches_rejects_a_tampered_ladder_length() {
        let mut row = modeled_row(13);
        row.ladder_buffer_len += 4;
        assert!(!model_matches(&row));
    }

    #[test]
    fn model_matches_rejects_tampered_condensed_min_and_total() {
        let mut by_min = modeled_row(8);
        by_min.condensed.min += 1;
        assert!(!model_matches(&by_min));

        let mut by_total = modeled_row(8);
        by_total.condensed.total += 1;
        assert!(!model_matches(&by_total));
    }

    #[test]
    fn model_matches_is_false_for_zero_leaves() {
        let mut row = modeled_row(1);
        row.leaf_count = 0;
        assert!(!model_matches(&row));
    }

    #[test]
    fn section_from_document_records_source_rows_and_all_match() {
        let document = LadderVerification {
            schema_version: LADDER_VERIFICATION_SCHEMA_VERSION,
            recorded_at: "2026-10-14T00:16:11+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            hash_size: 16,
            seed: 1,
            provenance: LadderProvenance {
                submodules: Vec::new(),
            },
            sizes: vec![modeled_row(4), modeled_row(13)],
        };
        let section = LadderSection::from_document(&document, LadderSource::RecordedFixture);
        assert_eq!(section.source, LadderSource::RecordedFixture);
        assert_eq!(section.recorded_at, "2026-10-14T00:16:11+05:30");
        assert_eq!(section.algorithm, "SLH-DSA-SHA2-128s-MTL-SHA2-128");
        assert!(section.all_match);
        assert_eq!(section.sizes.len(), 2);
        assert_eq!(section.sizes[0].leaf_count, 4);
        assert!(section.sizes.iter().all(|row| row.model_matches));
    }

    #[test]
    fn section_from_document_marks_a_mismatch_and_all_match_false() {
        let mut tampered = modeled_row(8);
        tampered.ladder_buffer_len += 4;
        let document = LadderVerification {
            schema_version: LADDER_VERIFICATION_SCHEMA_VERSION,
            recorded_at: "2026-10-14T00:16:11+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            hash_size: 16,
            seed: 1,
            provenance: LadderProvenance {
                submodules: Vec::new(),
            },
            sizes: vec![modeled_row(4), tampered],
        };
        let section = LadderSection::from_document(&document, LadderSource::Live);
        assert_eq!(section.source, LadderSource::Live);
        assert!(!section.all_match);
        assert!(section.sizes[0].model_matches);
        assert!(!section.sizes[1].model_matches);
    }

    #[test]
    fn ladder_source_serde_uses_kebab_case() {
        assert_eq!(
            serde_json::to_string(&LadderSource::RecordedFixture).unwrap(),
            "\"recorded-fixture\""
        );
        assert_eq!(
            serde_json::to_string(&LadderSource::Live).unwrap(),
            "\"live\""
        );
    }
}
