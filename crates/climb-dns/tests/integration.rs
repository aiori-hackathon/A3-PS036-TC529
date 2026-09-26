//! Integration tests for deterministic synthetic DNS dataset generation.
//!
//! These tests exercise the public API: `generate_dataset`, `DatasetSize`,
//! `MAX_MESSAGE_LEN`, and `SyntheticRecord`.  They do not depend on internal
//! module layout.

use climb_dns::{generate_dataset, DatasetSize, MAX_MESSAGE_LEN};
use hickory_proto::op::{Message, MessageType, Query};

// ── Determinism ────────────────────────────────────────────────────────

#[test]
fn dataset_determinism_same_seed_twice_small() {
    // Given: Small size with the same seed
    let d1 = generate_dataset(DatasetSize::Small, 42);
    let d2 = generate_dataset(DatasetSize::Small, 42);

    // When: comparing byte-for-byte
    // Then: identical length and content
    assert_eq!(d1.len(), d2.len());
    for (i, (a, b)) in d1.iter().zip(d2.iter()).enumerate() {
        assert_eq!(a, b, "message {i} differs for same (size, seed)");
    }
}

#[test]
fn dataset_different_seed_produces_different_output() {
    // Given: Small size with two distinct seeds
    let d1 = generate_dataset(DatasetSize::Small, 1);
    let d2 = generate_dataset(DatasetSize::Small, 999);

    // When: checking for any difference
    let any_differ = d1.iter().zip(d2.iter()).any(|(a, b)| a != b);

    // Then: at least one message in the dataset differs
    assert!(
        any_differ,
        "different seeds must produce at least one distinct message"
    );
}

// ── Counts ──────────────────────────────────────────────────────────────

#[test]
fn dataset_counts_match_size() {
    // Given: each DatasetSize variant
    // When: checking count() against generated len()
    // Then: they match for Small and Medium
    // (Large is checked via count()/as_usize() without materialization)

    let small = generate_dataset(DatasetSize::Small, 0);
    assert_eq!(small.len(), DatasetSize::Small.count());
    assert_eq!(DatasetSize::Small.count(), 100);

    let medium = generate_dataset(DatasetSize::Medium, 0);
    assert_eq!(medium.len(), DatasetSize::Medium.count());
    assert_eq!(DatasetSize::Medium.count(), 10_000);

    // Large: test count only, do NOT materialize 1M messages
    assert_eq!(DatasetSize::Large.count(), 1_000_000);
    assert_eq!(DatasetSize::Large.as_usize(), 1_000_000);
}

// ── Length bounds ───────────────────────────────────────────────────────

#[test]
fn every_message_within_length_bounds() {
    // Given: a Small dataset
    let dataset = generate_dataset(DatasetSize::Small, 7);

    // Then: every message is non-empty and ≤ MAX_MESSAGE_LEN
    for (i, msg) in dataset.iter().enumerate() {
        assert!(
            !msg.is_empty(),
            "message at index {i} is empty — all DNS responses must be non-empty"
        );
        assert!(
            msg.len() <= MAX_MESSAGE_LEN,
            "message at index {i} has len {} > MAX_MESSAGE_LEN ({MAX_MESSAGE_LEN})",
            msg.len()
        );
    }
}

// ── RRset invariants ───────────────────────────────────────────────────

fn parsed_dataset(seed: u64) -> Vec<Message> {
    let dataset = generate_dataset(DatasetSize::Small, seed);
    dataset
        .iter()
        .map(|msg| {
            Message::from_vec(msg)
                .unwrap_or_else(|e| panic!("failed to parse generated message: {e}"))
        })
        .collect()
}

#[test]
fn every_message_is_a_response_with_answers() {
    // Given: a parsed Small dataset
    let messages = parsed_dataset(13);

    for (i, msg) in messages.iter().enumerate() {
        // Then: it is a Response
        assert_eq!(
            msg.metadata.message_type,
            MessageType::Response,
            "message {i}: expected Response"
        );
        // Then: it has a non-empty answer section
        assert!(
            !msg.answers.is_empty(),
            "message {i}: answer section is empty"
        );
        // Then: it has exactly one query
        assert_eq!(
            msg.queries.len(),
            1,
            "message {i}: expected 1 query, got {}",
            msg.queries.len()
        );
    }
}

#[test]
fn rrsets_are_homogeneous_per_message() {
    // Given: a parsed Small dataset
    let messages = parsed_dataset(17);

    for (i, msg) in messages.iter().enumerate() {
        let query: &Query = &msg.queries[0];

        // Then: every answer's owner name equals the query's QNAME
        for (j, rr) in msg.answers.iter().enumerate() {
            assert_eq!(
                rr.name, query.name,
                "message {i} answer {j}: owner {:?} != QNAME {:?}",
                rr.name, query.name
            );
            // Then: every answer's record type equals the query's QTYPE
            assert_eq!(
                rr.record_type(),
                query.query_type,
                "message {i} answer {j}: type {:?} != QTYPE {:?}",
                rr.record_type(),
                query.query_type
            );
        }
    }
}

// ── Round-trip parse ────────────────────────────────────────────────────

#[test]
fn generated_messages_parse_as_valid_dns_responses() {
    // Given: a Small dataset
    let dataset = generate_dataset(DatasetSize::Small, 13);

    // When: parsing each message via hickory-proto's wire decoder
    for (i, msg) in dataset.iter().enumerate() {
        let parsed = Message::from_vec(msg);

        // Then: parsing succeeds
        let parsed = parsed.unwrap_or_else(|e| {
            panic!(
                "message at index {i} failed to parse as a DNS message: {e}\n\
                 first 64 bytes: {:02x?}",
                &msg[..msg.len().min(64)]
            )
        });

        // Then: it is a response with a non-empty answer section
        assert_eq!(
            parsed.metadata.message_type,
            MessageType::Response,
            "message at index {i}: expected Response, got {:?}",
            parsed.metadata.message_type
        );
        assert!(
            !parsed.answers.is_empty(),
            "message at index {i}: answer section is empty"
        );
    }
}

#[test]
fn roundtrip_encode_identity() {
    // Given: a Small dataset
    let dataset = generate_dataset(DatasetSize::Small, 19);

    // When: parsing and re-encoding each message
    for (i, msg) in dataset.iter().enumerate() {
        let parsed = Message::from_vec(msg)
            .unwrap_or_else(|e| panic!("message at index {i} failed to parse: {e}"));
        let re_encoded = parsed
            .to_vec()
            .unwrap_or_else(|e| panic!("message at index {i} failed to re-encode: {e}"));

        // Then: re-encoding produces the exact same bytes
        assert_eq!(
            msg, &re_encoded,
            "message at index {i}: round-trip encode produced different bytes"
        );
    }
}
