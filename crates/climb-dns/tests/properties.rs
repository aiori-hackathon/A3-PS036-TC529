//! Property tests for the DNS wire parser.
//!
//! The parser is the boundary between captured network bytes and every byte
//! count the report publishes. These properties pin the two guarantees the
//! anatomy code claims: arbitrary input never panics, and a tiny input cannot
//! make the parser reserve memory proportional to a forged section count.

use climb_dns::{
    generate_dataset, generate_dataset_count, parse_response_anatomy, DatasetSize, DnsError,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parse_response_anatomy(&data, None);
    }

    #[test]
    fn arbitrary_bytes_with_transport_never_panic(
        data in prop::collection::vec(any::<u8>(), 0..4096),
        transport in prop::option::of("[A-Z]{3,4}"),
    ) {
        let _ = parse_response_anatomy(&data, transport.as_deref());
    }

    #[test]
    fn every_truncation_is_handled(
        cut in 0usize..2048,
    ) {
        let dataset = generate_dataset(DatasetSize::Small, 7);
        let message = &dataset[0];
        let end = cut.min(message.len());
        let _ = parse_response_anatomy(&message[..end], Some("UDP"));
    }

    #[test]
    fn generated_messages_are_fully_accounted_for(seed in any::<u64>(), count in 1usize..32) {
        let dataset = generate_dataset_count(count, seed);
        for message in dataset {
            let anatomy = parse_response_anatomy(&message, Some("UDP"))
                .expect("a generated message is a well-formed DNS response");
            prop_assert_eq!(anatomy.parts_sum, anatomy.wire_length);
            prop_assert_eq!(anatomy.wire_length, message.len());
            prop_assert_eq!(anatomy.header_bytes, climb_dns::HEADER_LEN);
        }
    }
}

#[test]
fn a_forged_section_count_is_rejected_before_allocation() {
    // 12-byte header: response, QD=0, AN=65535, NS=65535, AR=65535.
    let mut message = vec![0x00, 0x01, 0x84, 0x00];
    message.extend_from_slice(&[0x00, 0x00]); // QDCOUNT
    message.extend_from_slice(&[0xff, 0xff]); // ANCOUNT
    message.extend_from_slice(&[0xff, 0xff]); // NSCOUNT
    message.extend_from_slice(&[0xff, 0xff]); // ARCOUNT
    let error = parse_response_anatomy(&message, None).expect_err("must be rejected");
    assert!(matches!(error, DnsError::SectionCount { .. }), "{error}");
}

#[test]
fn trailing_bytes_are_rejected() {
    let dataset = generate_dataset(DatasetSize::Small, 11);
    let mut message = dataset[0].clone();
    message.push(0xaa);
    let error = parse_response_anatomy(&message, None).expect_err("must be rejected");
    assert!(matches!(error, DnsError::TrailingBytes { .. }), "{error}");
}
