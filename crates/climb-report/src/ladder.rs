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
}
