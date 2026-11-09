//! Property tests for the signing/verification path.
//!
//! Examples in the other integration tests pin specific cases. These tests
//! quantify over arbitrary byte strings instead:
//!
//! * a batch of arbitrary non-empty messages always round-trips;
//! * flipping any one byte of a signed message never makes it verify;
//! * no arbitrary byte string is ever accepted as a full signature, and no
//!   arbitrary byte string is accepted as a condensed signature against a
//!   valid ladder.
//!
//! Failure persistence is disabled: a generated regression file is not part of
//! this project's artifact set, and an arbitrary-input failure here would be a
//! real defect that the ordinary test suite should reproduce.

use proptest::prelude::*;

use climb_mtl::{FullSignature, Ladder, MtlKeyPair, MtlVerifier, Signature};

fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Signing costs one SLH-DSA signature per batch, so the case count is small.
const SIGNING_CASES: u32 = 4;

proptest! {
    #![proptest_config(config(SIGNING_CASES))]

    #[test]
    fn arbitrary_non_empty_messages_round_trip(
        messages in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..=48), 1..=4),
    ) {
        let keypair = MtlKeyPair::generate().expect("keygen");
        let signer = keypair.signer();
        let verifier = keypair.verifier().expect("verifier");
        let refs: Vec<&[u8]> = messages.iter().map(Vec::as_slice).collect();
        let output = signer.sign_batch(&refs).expect("sign_batch");

        for (index, message) in messages.iter().enumerate() {
            let condensed = Signature::Condensed(
                output.condensed_signature(index).expect("condensed").clone(),
            );
            prop_assert!(
                verifier
                    .verify(message, &condensed, Some(output.ladder()), false)
                    .expect("condensed verify"),
                "message {} failed the condensed round trip",
                index
            );
            let full = Signature::Full(output.full_signature(index).expect("full"));
            prop_assert!(
                verifier.verify(message, &full, None, false).expect("full verify"),
                "message {} failed the full round trip",
                index
            );
        }
    }

    #[test]
    fn any_single_byte_message_mutation_is_rejected(
        message in prop::collection::vec(any::<u8>(), 1..=32),
        index in any::<prop::sample::Index>(),
    ) {
        let keypair = MtlKeyPair::generate().expect("keygen");
        let signer = keypair.signer();
        let verifier = keypair.verifier().expect("verifier");
        let output = signer.sign_batch(&[message.as_slice()]).expect("sign_batch");
        let signature = Signature::Condensed(
            output.condensed_signature(0).expect("condensed").clone(),
        );

        let mut tampered = message.clone();
        let position = index.index(tampered.len());
        tampered[position] ^= 0x01;

        let result = verifier
            .verify(&tampered, &signature, Some(output.ladder()), false)
            .expect("verify must not hard-error on a tampered message");
        prop_assert!(!result, "byte {} of the message was accepted", position);
    }
}

proptest! {
    #![proptest_config(config(64))]

    #[test]
    fn no_arbitrary_bytes_are_accepted_as_a_full_signature(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
        let keypair = MtlKeyPair::generate().expect("keygen");
        let verifier = keypair.verifier().expect("verifier");
        let result = verifier.verify(
            b"property message",
            &Signature::Full(FullSignature::from_bytes(bytes)),
            None,
            false,
        );
        prop_assert!(
            !matches!(result, Ok(true)),
            "an arbitrary byte string was accepted as a full signature"
        );
    }
}

/// A valid ladder is needed to reach the condensed verification path, so it is
/// built once and the arbitrary bytes vary underneath it.
#[test]
fn no_arbitrary_bytes_are_accepted_as_a_condensed_signature() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let verifier: MtlVerifier = keypair.verifier().expect("verifier");
    let output = keypair
        .signer()
        .sign_batch(&[b"property message"])
        .expect("sign_batch");
    let ladder: Ladder = output.ladder().clone();

    proptest!(config(64), |(bytes in prop::collection::vec(any::<u8>(), 0..=256))| {
        let result = verifier.verify(
            b"property message",
            &Signature::Condensed(climb_mtl::CondensedSignature::from_bytes(bytes)),
            Some(&ladder),
            false,
        );
        prop_assert!(
            !matches!(result, Ok(true)),
            "an arbitrary byte string was accepted as a condensed signature"
        );
    });
}
