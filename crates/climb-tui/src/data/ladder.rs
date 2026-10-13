//! Ladder-tab data: loading the recorded ladder-verification artifact and the
//! model-vs-library badge (phase 24.4).
//!
//! Loading never panics: a missing or unparseable artifact becomes a
//! [`LadderLoadState`] variant the UI renders instead of crashing. The badge is
//! a pure comparison between `climb-analytical`'s closed-form model and the
//! recorded libMTL measurement, so it is fully unit-testable without a
//! terminal or a file.

use std::path::{Path, PathBuf};

use climb_analytical::{predicted_condensed_len, predicted_ladder_len};
use climb_report::LadderVerification;

/// How the recorded ladder-verification artifact looks at load time.
#[derive(Debug, Clone, PartialEq)]
pub enum LadderLoadState {
    /// No artifact path is configured (the model still renders alone).
    Absent,
    /// The artifact parsed; the document holds the measured rows.
    Loaded(Box<LadderVerification>),
    /// The file does not exist.
    Missing { path: PathBuf },
    /// The file exists but could not be read or parsed.
    Invalid { path: PathBuf, error: String },
}

impl LadderLoadState {
    /// The parsed verification document, when one loaded.
    #[must_use]
    pub fn verification(&self) -> Option<&LadderVerification> {
        match self {
            LadderLoadState::Loaded(document) => Some(document),
            _ => None,
        }
    }

    /// Short one-line summary for the trace log.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            LadderLoadState::Absent => "absent".to_string(),
            LadderLoadState::Loaded(document) => {
                format!("loaded: {} sizes", document.sizes.len())
            }
            LadderLoadState::Missing { path } => format!("missing: {}", path.display()),
            LadderLoadState::Invalid { path, .. } => format!("invalid: {}", path.display()),
        }
    }
}

/// Load the recorded ladder-verification artifact, never panicking.
pub fn load_ladder(path: &Path) -> LadderLoadState {
    if std::fs::metadata(path).is_err() {
        return LadderLoadState::Missing {
            path: path.to_path_buf(),
        };
    }
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return LadderLoadState::Invalid {
                path: path.to_path_buf(),
                error: error.to_string(),
            };
        }
    };
    match serde_json::from_str::<LadderVerification>(&source) {
        Ok(document) => LadderLoadState::Loaded(Box::new(document)),
        Err(error) => LadderLoadState::Invalid {
            path: path.to_path_buf(),
            error: error.to_string(),
        },
    }
}

/// The kind of model-vs-library statement the badge makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    /// The recorded row equals the closed-form model.
    Match,
    /// The recorded row disagrees with the closed-form model.
    Mismatch,
    /// The artifact loaded but holds no row for this leaf count.
    ModelOnly,
    /// No verification artifact is available.
    NoArtifact,
}

/// A one-line badge comparing the model against the recorded measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct LadderBadge {
    /// Which statement the badge makes.
    pub kind: BadgeKind,
    /// The rendered text.
    pub text: String,
}

/// Compare the closed-form model against the recorded artifact for one ladder.
///
/// `leaf_count` is clamped to at least one leaf and `selected` to the valid
/// range, so the function is total. `ascii` swaps the ✔/✖ marks for ASCII so
/// the badge stays legible without a Unicode font.
#[must_use]
pub fn badge(state: &LadderLoadState, leaf_count: u64, selected: u64, ascii: bool) -> LadderBadge {
    let n = leaf_count.max(1);
    let selected = selected.min(n - 1);
    let model_ladder = predicted_ladder_len(n);

    let Some(document) = state.verification() else {
        return LadderBadge {
            kind: BadgeKind::NoArtifact,
            text: "model only: no verification artifact".to_string(),
        };
    };
    let Some(row) = document.row(n as usize) else {
        return LadderBadge {
            kind: BadgeKind::ModelOnly,
            text: format!("model only: N={n} not in the verification artifact"),
        };
    };

    let model: Vec<usize> = (0..n)
        .map(|index| predicted_condensed_len(n, index).unwrap_or(0))
        .collect();
    let model_min = model.iter().copied().min().unwrap_or(0);
    let model_max = model.iter().copied().max().unwrap_or(0);
    let model_total: u64 = model.iter().map(|&length| length as u64).sum();
    let agrees = model_ladder == row.ladder_buffer_len
        && model_min == row.condensed.min
        && model_max == row.condensed.max
        && model_total == row.condensed.total
        && (model_min == model_max) == row.condensed.uniform;

    let condensed = predicted_condensed_len(n, selected).unwrap_or(0);
    if agrees {
        let mark = if ascii { "[ok]" } else { "✔" };
        let date = recorded_date(&document.recorded_at);
        LadderBadge {
            kind: BadgeKind::Match,
            text: format!(
                "{mark} matches libMTL: condensed {condensed} B, ladder {model_ladder} B (recorded {date})"
            ),
        }
    } else {
        let mark = if ascii { "[x]" } else { "✖" };
        LadderBadge {
            kind: BadgeKind::Mismatch,
            text: format!(
                "{mark} model {model_ladder} B, library {} B",
                row.ladder_buffer_len
            ),
        }
    }
}

/// The leading `YYYY-MM-DD` of an RFC 3339 timestamp, or the whole string when
/// it is too short to contain one.
fn recorded_date(timestamp: &str) -> &str {
    timestamp.get(..10).unwrap_or(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use climb_bench::{CondensedStats, LadderRung};
    use climb_report::{LadderProvenance, LadderRow};

    fn row_for(leaf_count: usize) -> LadderRow {
        let n = leaf_count as u64;
        let model: Vec<usize> = (0..n)
            .map(|index| predicted_condensed_len(n, index).unwrap_or(0))
            .collect();
        let min = model.iter().copied().min().unwrap_or(0);
        let max = model.iter().copied().max().unwrap_or(0);
        let total: u64 = model.iter().map(|&length| length as u64).sum();
        LadderRow {
            leaf_count,
            rung_count: climb_analytical::rungs(n).len(),
            rungs: climb_analytical::rungs(n)
                .iter()
                .map(|rung| LadderRung {
                    left: rung.left,
                    right: rung.right,
                })
                .collect(),
            ladder_buffer_len: predicted_ladder_len(n),
            signed_ladder_len: predicted_ladder_len(n) + 4 + 7856,
            signature_len: 7856,
            condensed: CondensedStats {
                min,
                max,
                mean: total as f64 / n as f64,
                total,
                uniform: min == max,
            },
            full_len_min: min + predicted_ladder_len(n) + 4 + 7856,
            full_len_max: max + predicted_ladder_len(n) + 4 + 7856,
            with_mtl_signatures: 1,
            without_mtl_signatures: leaf_count,
        }
    }

    fn document(rows: Vec<LadderRow>) -> LadderLoadState {
        LadderLoadState::Loaded(Box::new(LadderVerification {
            schema_version: 1,
            recorded_at: "2026-10-14T00:16:11+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            hash_size: 16,
            seed: 1,
            provenance: LadderProvenance {
                submodules: Vec::new(),
            },
            sizes: rows,
        }))
    }

    #[test]
    fn load_of_missing_path_is_missing_state() {
        let path = std::env::temp_dir().join("climb-tui-definitely-missing-ladder.json");
        let _ = std::fs::remove_file(&path);
        let state = load_ladder(&path);
        assert!(matches!(state, LadderLoadState::Missing { .. }));
        assert!(state.verification().is_none());
    }

    #[test]
    fn load_of_garbage_artifact_is_invalid_and_never_panics() {
        let dir = std::env::temp_dir().join(format!("climb-tui-ladder-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("ladder.json");
        std::fs::write(&path, "{\"sizes\": not json").expect("write garbage");
        let state = load_ladder(&path);
        assert!(matches!(state, LadderLoadState::Invalid { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn badge_without_an_artifact_says_model_only() {
        let badge = badge(&LadderLoadState::Absent, 8, 3, false);
        assert_eq!(badge.kind, BadgeKind::NoArtifact);
        assert!(badge.text.contains("no verification artifact"));
    }

    #[test]
    fn badge_reports_a_match_against_an_agreeing_row() {
        let state = document(vec![row_for(13)]);
        let badge = badge(&state, 13, 5, false);
        assert_eq!(badge.kind, BadgeKind::Match);
        assert!(badge.text.contains("matches libMTL"), "{}", badge.text);
        assert!(badge.text.contains("recorded 2026-10-14"), "{}", badge.text);
        // predicted condensed for leaf 5 of N=13 (rung [0..7], depth 3) = 76 + 3*16.
        assert!(badge.text.contains("condensed 124 B"), "{}", badge.text);
        assert!(badge.text.contains("ladder 132 B"), "{}", badge.text);
    }

    #[test]
    fn badge_reports_a_mismatch_when_the_ladder_length_differs() {
        let mut row = row_for(8);
        row.ladder_buffer_len += 4;
        let state = document(vec![row]);
        let badge = badge(&state, 8, 0, false);
        assert_eq!(badge.kind, BadgeKind::Mismatch);
        assert!(badge.text.contains("library"), "{}", badge.text);
        assert!(badge.text.contains('✖'), "{}", badge.text);
    }

    #[test]
    fn badge_reports_model_only_when_the_size_is_missing() {
        let state = document(vec![row_for(8)]);
        let badge = badge(&state, 13, 0, false);
        assert_eq!(badge.kind, BadgeKind::ModelOnly);
        assert!(badge.text.contains("N=13"), "{}", badge.text);
    }

    #[test]
    fn badge_uses_ascii_marks_in_ascii_mode() {
        let state = document(vec![row_for(8)]);
        let badge = badge(&state, 8, 0, true);
        assert_eq!(badge.kind, BadgeKind::Match);
        assert!(badge.text.starts_with("[ok]"), "{}", badge.text);
        assert!(!badge.text.contains('✔'), "{}", badge.text);
    }

    #[test]
    fn badge_clamps_leaf_count_and_selection() {
        let state = document(vec![row_for(13)]);
        let low = badge(&state, 0, 0, false);
        let high = badge(&state, 13, 999, false);
        assert_eq!(
            low.kind,
            BadgeKind::ModelOnly,
            "N=0 clamps to 1, not measured"
        );
        assert_eq!(high.kind, BadgeKind::Match, "leaf index clamps into range");
    }
}
