//! Tamper-detection tests.
//!
//! Flipping a byte in the message or in any part of the signature must make
//! verification return `Ok(false)`, never `Ok(true)`. The wrapper maps every
//! "rejected" native status to `Ok(false)`, so these tests pin the property that
//! a tampered input can never be reported as valid.

use climb_mtl::{FullSignature, MtlKeyPair, Signature};

const MESSAGE: &[u8] = b"climb tamper-detection message";

fn setup() -> (
    MtlKeyPair,
    Vec<u8>,
    FullSignature,
    climb_mtl::Ladder,
    climb_mtl::CondensedSignature,
) {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let output = signer.sign_batch(&[MESSAGE]).expect("sign_batch");

    let full = output.full_signature(0).expect("full signature");
    let condensed = output.condensed_signature(0).expect("condensed").clone();
    let ladder = output.ladder().clone();
    (keypair, MESSAGE.to_vec(), full, ladder, condensed)
}

/// Flip one bit in a single byte, returning a copy.
fn flip_byte(bytes: &[u8], index: usize) -> Vec<u8> {
    let mut tampered = bytes.to_vec();
    tampered[index] ^= 0x01;
    tampered
}

#[test]
fn full_signature_accepts_untampered_input() {
    let (keypair, message, full, _ladder, _condensed) = setup();
    let verifier = keypair.verifier().expect("verifier");
    assert!(
        verifier
            .verify(&message, &Signature::Full(full), None, false)
            .expect("verify"),
        "the untampered signature must verify (validates the test itself)"
    );
}

#[test]
fn full_signature_rejects_every_single_byte_message_flip() {
    let (keypair, message, full, _ladder, _condensed) = setup();
    let verifier = keypair.verifier().expect("verifier");

    for index in 0..message.len() {
        let tampered = flip_byte(&message, index);
        let result = verifier
            .verify(&tampered, &Signature::Full(full.clone()), None, false)
            .expect("verify must not hard-error on a tampered message");
        assert!(
            !result,
            "a message tampered at byte {index} must be rejected, not accepted"
        );
    }
}

#[test]
fn full_signature_rejects_flips_across_the_whole_signature() {
    let (keypair, message, full, _ladder, _condensed) = setup();
    let verifier = keypair.verifier().expect("verifier");
    let bytes = full.as_bytes();
    assert!(!bytes.is_empty());

    // The signature is ~8 KB; sampling evenly keeps the test fast while still
    // covering the condensed prefix, the length field, and the ladder signature.
    let step = (bytes.len() / 64).max(1);
    let mut checked = 0usize;
    for index in (0..bytes.len()).step_by(step) {
        let tampered = FullSignature::from_bytes(flip_byte(bytes, index));
        let result = verifier
            .verify(&message, &Signature::Full(tampered), None, false)
            .expect("verify must not hard-error on a tampered signature");
        assert!(
            !result,
            "a signature tampered at byte {index} must be rejected, not accepted"
        );
        checked += 1;
    }
    assert!(checked >= 32, "expected a broad sample of byte positions");
}

#[test]
fn full_signature_rejects_a_signature_from_another_keypair() {
    let (_keypair, message, _full, _ladder, _condensed) = setup();
    let other = MtlKeyPair::generate().expect("second keygen");
    let other_output = other.signer().sign_batch(&[&message]).expect("sign");

    let verifier = _keypair.verifier().expect("verifier");
    let result = verifier
        .verify(
            &message,
            &Signature::Full(other_output.full_signature(0).unwrap()),
            None,
            false,
        )
        .expect("verify");
    assert!(
        !result,
        "a signature from a different keypair must be rejected"
    );
}

#[test]
fn condensed_signature_rejects_message_and_ladder_tampering() {
    let (keypair, message, _full, ladder, condensed) = setup();
    let verifier = keypair.verifier().expect("verifier");

    let signature = Signature::Condensed(condensed.clone());
    assert!(
        verifier
            .verify(&message, &signature, Some(&ladder), false)
            .expect("baseline verify"),
        "the untampered condensed signature must verify"
    );

    let tampered_message = flip_byte(&message, 0);
    assert!(
        !verifier
            .verify(&tampered_message, &signature, Some(&ladder), false)
            .expect("verify"),
        "a tampered message must be rejected for a condensed signature"
    );

    let tampered_ladder = climb_mtl::Ladder::from_bytes(flip_byte(ladder.as_bytes(), 0));
    assert!(
        !verifier
            .verify(&message, &signature, Some(&tampered_ladder), false)
            .expect("verify"),
        "a tampered ladder must be rejected when it is revalidated"
    );

    let tampered_signature = Signature::Condensed(climb_mtl::CondensedSignature::from_bytes(
        flip_byte(condensed.as_bytes(), 0),
    ));
    assert!(
        !verifier
            .verify(&message, &tampered_signature, Some(&ladder), false)
            .expect("verify"),
        "a tampered condensed signature must be rejected"
    );
}

#[test]
fn condensed_signature_without_a_ladder_is_an_error() {
    let (keypair, message, _full, _ladder, condensed) = setup();
    let verifier = keypair.verifier().expect("verifier");
    let error = verifier
        .verify(&message, &Signature::Condensed(condensed), None, false)
        .expect_err("verifying a condensed signature without a ladder must error");
    assert!(
        matches!(error, climb_mtl::MtlError::LadderRequired),
        "expected LadderRequired, got {error:?}"
    );
}

#[test]
fn cross_batch_condensed_signatures_do_not_verify() {
    // Two separate single-message batches produce independent ladders with
    // independent leaf 0 entries. A condensed signature from one batch must not
    // verify against the other batch's ladder.
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    let first = signer.sign_batch(&[b"first batch message"]).expect("sign");
    let second = signer.sign_batch(&[b"first batch message"]).expect("sign");

    let result = verifier
        .verify(
            b"first batch message",
            &first.signature(0).unwrap(),
            Some(second.ladder()),
            false,
        )
        .expect("verify");
    assert!(
        !result,
        "a condensed signature must not verify against an unrelated batch's ladder"
    );
}
