//! Property tests for the artifact parsers.
//!
//! Every file these parsers read can be corrupted, replaced or truncated by an
//! interrupted run. The properties below pin the only acceptable response:
//! return a typed error, never panic, and never turn malformed input into a
//! plausible-looking empty result.

use climb_report::{parse_memory_artifact, ClimbResults, LadderVerification, MemoryReportJson};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic_in_the_results_parser(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let _ = serde_json::from_slice::<ClimbResults>(&bytes);
    }

    #[test]
    fn arbitrary_bytes_never_panic_in_the_ladder_parser(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let _ = serde_json::from_slice::<LadderVerification>(&bytes);
    }

    #[test]
    fn arbitrary_bytes_never_panic_in_the_memory_parser(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let _ = serde_json::from_slice::<MemoryReportJson>(&bytes);
    }

    #[test]
    fn arbitrary_text_never_panics_in_the_memory_shape_detector(
        text in ".{0,4096}"
    ) {
        let _ = parse_memory_artifact(&text);
    }

    #[test]
    fn a_json_object_is_never_accepted_as_a_complete_results_document(
        members in prop::collection::vec(("[a-z_]{1,24}", any::<u64>()), 0..8)
    ) {
        let object: serde_json::Map<String, serde_json::Value> = members
            .into_iter()
            .map(|(key, value)| (key, serde_json::Value::from(value)))
            .collect();
        let text = serde_json::to_string(&object).expect("a JSON object serializes");
        prop_assert!(serde_json::from_str::<ClimbResults>(&text).is_err());
    }
}

#[test]
fn a_missing_field_is_named_in_the_error() {
    // `schema_version` is a required field of the consolidated document.
    let error = serde_json::from_str::<ClimbResults>("{}").expect_err("must be rejected");
    let message = error.to_string();
    assert!(
        message.contains("missing field"),
        "error should name the missing field: {message}"
    );
}
