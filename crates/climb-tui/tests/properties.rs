//! Property tests for the dashboard's artifact loaders.
//!
//! The loaders are the last line between a damaged pipeline output and the
//! screen. These properties pin the contract: arbitrary bytes never panic, an
//! explicitly named missing file is a hard `Missing` rather than a silent
//! fall-through, and a present-but-invalid file reports its own path.

use std::path::Path;

use climb_tui::data::{
    load_ladder_auto_with, load_live_auto_with, load_results_auto_from, LadderLoadState,
    LiveLoadState, LoadState,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_embedded_results_never_panic(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let missing = Path::new("this/path/does/not/exist/results.json");
        let _ = load_results_auto_from(None, missing, missing, &bytes);
    }

    #[test]
    fn arbitrary_embedded_ladder_never_panics(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let missing = Path::new("this/path/does/not/exist/ladder.json");
        let _ = load_ladder_auto_with(missing, missing, Some(&bytes));
    }

    #[test]
    fn arbitrary_embedded_capture_never_panics(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let missing = Path::new("this/path/does/not/exist/capture.json");
        let _ = load_live_auto_with(missing, missing, Some(&bytes));
    }
}

#[test]
fn an_explicit_missing_results_file_does_not_fall_through() {
    let missing = Path::new("this/path/does/not/exist/results.json");
    let (source, state) = load_results_auto_from(
        Some(missing),
        Path::new("also/missing/target.json"),
        Path::new("also/missing/sample.json"),
        b"{}",
    );
    assert_eq!(source.origin(), climb_tui::data::ArtifactOrigin::Explicit);
    assert!(
        matches!(state, LoadState::Missing { .. }),
        "explicitly named missing file must stay Missing, got {state:?}"
    );
}

#[test]
fn a_present_but_invalid_results_file_names_its_path() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), b"{not json").expect("write fixture");
    let (_, state) = load_results_auto_from(
        Some(file.path()),
        Path::new("also/missing/target.json"),
        Path::new("also/missing/sample.json"),
        b"{}",
    );
    match state {
        LoadState::Invalid { path, error } => {
            assert_eq!(path, file.path());
            assert!(!error.is_empty());
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn an_explicit_missing_ladder_or_capture_is_reported_as_missing() {
    let missing = Path::new("this/path/does/not/exist/artifact.json");
    assert!(matches!(
        load_ladder_auto_with(missing, missing, None),
        LadderLoadState::Absent
    ));
    assert!(matches!(
        load_live_auto_with(missing, missing, None),
        LiveLoadState::Missing { .. }
    ));
}
