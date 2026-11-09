//! Negative-case table for the safe MTL wrapper.
//!
//! Every row names the malformed input, the expected outcome, and why that
//! outcome is the honest one. Two outcomes are expected:
//!
//! * **typed layout error** — the buffer does not match the serialized layout
//!   pinned by the vendored library (wrong series id, non-zero flags, a length
//!   field that disagrees with the buffer, truncation, trailing bytes);
//! * **`Ok(false)`** — the buffer is structurally valid but does not
//!   authenticate (a message or signature bit was changed).
//!
//! Neither outcome is `Ok(true)`. The split matters because it tells a caller
//! whether the peer sent garbage or a signature that simply failed.

use climb_mtl::{
    CondensedSignature, FullSignature, Ladder, MtlError, MtlKeyPair, MtlVerifier, Signature,
    MAX_MESSAGE_LEN,
};

const HASH_SIZE: usize = 16;
const SID_LEN: usize = 2 * HASH_SIZE;

struct Baseline {
    verifier: MtlVerifier,
    messages: Vec<Vec<u8>>,
    ladder: Ladder,
    condensed: CondensedSignature,
    full: FullSignature,
}

fn baseline() -> Baseline {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let messages: Vec<Vec<u8>> = (0..3)
        .map(|i| format!("climb negative case {i}").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = messages.iter().map(Vec::as_slice).collect();
    let output = signer.sign_batch(&refs).expect("sign_batch");

    Baseline {
        verifier: keypair.verifier().expect("verifier"),
        messages,
        ladder: output.ladder().clone(),
        condensed: output.condensed_signature(0).expect("condensed").clone(),
        full: output.full_signature(0).expect("full signature"),
    }
}

/// Replace `len` bytes at `offset` with `value`.
fn patch(bytes: &[u8], offset: usize, value: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[offset..offset + value.len()].copy_from_slice(value);
    out
}

fn appended(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.push(0);
    out
}

fn expect_malformed_condensed(result: Result<bool, MtlError>) {
    assert!(
        matches!(result, Err(MtlError::MalformedCondensedSignature { .. })),
        "expected MalformedCondensedSignature, got {result:?}"
    );
}

fn expect_malformed_ladder(result: Result<bool, MtlError>) {
    assert!(
        matches!(result, Err(MtlError::MalformedLadder { .. })),
        "expected MalformedLadder, got {result:?}"
    );
}

fn expect_crypto_rejection(result: Result<bool, MtlError>) {
    assert!(
        matches!(result, Ok(false)),
        "expected Ok(false), got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Message-level cases
// ---------------------------------------------------------------------------

#[test]
fn an_empty_message_is_rejected_before_any_native_call() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        b"",
        &Signature::Condensed(baseline.condensed),
        Some(&baseline.ladder),
        false,
    );
    assert!(
        matches!(result, Err(MtlError::EmptyMessage { index: 0 })),
        "got {result:?}"
    );
}

#[test]
fn a_message_over_the_u16_limit_is_rejected_before_any_native_call() {
    let baseline = baseline();
    let oversized = vec![0u8; MAX_MESSAGE_LEN + 1];
    let result = baseline.verifier.verify(
        &oversized,
        &Signature::Condensed(baseline.condensed),
        Some(&baseline.ladder),
        false,
    );
    assert!(
        matches!(
            result,
            Err(MtlError::MessageTooLong { len, max, .. })
                if len == MAX_MESSAGE_LEN + 1 && max == MAX_MESSAGE_LEN
        ),
        "got {result:?}"
    );
}

#[test]
fn a_tampered_message_is_a_cryptographic_rejection_not_a_layout_error() {
    let baseline = baseline();
    let mut tampered = baseline.messages[0].clone();
    tampered[0] ^= 0x01;
    expect_crypto_rejection(baseline.verifier.verify(
        &tampered,
        &Signature::Condensed(baseline.condensed),
        Some(&baseline.ladder),
        false,
    ));
}

// ---------------------------------------------------------------------------
// Condensed-signature layout cases
// ---------------------------------------------------------------------------

#[test]
fn an_emptied_condensed_signature_is_a_layout_error() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(Vec::new())),
        Some(&baseline.ladder),
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_foreign_series_id_in_a_condensed_signature_is_a_layout_error() {
    let baseline = baseline();
    let bytes = patch(baseline.condensed.as_bytes(), 0, &[0u8; SID_LEN]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(bytes)),
        Some(&baseline.ladder),
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_non_zero_flags_field_in_a_condensed_signature_is_a_layout_error() {
    let baseline = baseline();
    let bytes = patch(baseline.condensed.as_bytes(), SID_LEN, &[0x00, 0x01]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(bytes)),
        Some(&baseline.ladder),
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_condensed_signature_with_a_mismatched_sibling_count_is_a_layout_error() {
    let baseline = baseline();
    let sibling_offset = SID_LEN + 2 + HASH_SIZE + 3 * 8;
    let count = u16::from_be_bytes(
        baseline.condensed.as_bytes()[sibling_offset..sibling_offset + 2]
            .try_into()
            .expect("two bytes"),
    );
    let bytes = patch(
        baseline.condensed.as_bytes(),
        sibling_offset,
        &(count + 1).to_be_bytes(),
    );
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(bytes)),
        Some(&baseline.ladder),
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_condensed_signature_with_a_trailing_byte_is_a_layout_error() {
    let baseline = baseline();
    let bytes = appended(baseline.condensed.as_bytes());
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(bytes)),
        Some(&baseline.ladder),
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_tampered_sibling_hash_is_a_cryptographic_rejection() {
    let baseline = baseline();
    // The final sibling hash is structurally opaque; flipping it keeps the
    // shape valid and lets the native verifier reject the proof.
    let last = baseline.condensed.len() - 1;
    let bytes = patch(baseline.condensed.as_bytes(), last, &[0xff]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(CondensedSignature::from_bytes(bytes)),
        Some(&baseline.ladder),
        false,
    );
    expect_crypto_rejection(result);
}

#[test]
fn a_condensed_signature_without_a_ladder_is_a_typed_error() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        None,
        false,
    );
    assert!(
        matches!(result, Err(MtlError::LadderRequired)),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Ladder layout cases
// ---------------------------------------------------------------------------

#[test]
fn an_emptied_ladder_is_a_layout_error() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(Vec::new())),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_foreign_series_id_in_a_ladder_is_a_layout_error() {
    let baseline = baseline();
    let bytes = patch(baseline.ladder.as_bytes(), 2, &[0u8; SID_LEN]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_non_zero_flags_field_in_a_ladder_is_a_layout_error() {
    let baseline = baseline();
    let bytes = patch(baseline.ladder.as_bytes(), 0, &[0x00, 0x01]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_ladder_with_a_mismatched_rung_count_is_a_layout_error() {
    let baseline = baseline();
    let offset = 2 + SID_LEN;
    let count = u16::from_be_bytes(
        baseline.ladder.as_bytes()[offset..offset + 2]
            .try_into()
            .expect("two bytes"),
    );
    let bytes = patch(
        baseline.ladder.as_bytes(),
        offset,
        &(count + 1).to_be_bytes(),
    );
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_truncated_ladder_signature_is_a_layout_error() {
    let baseline = baseline();
    let bytes = baseline.ladder.as_bytes()[..baseline.ladder.len() - 1].to_vec();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_ladder_with_a_trailing_byte_is_a_layout_error() {
    // The native verifier's bounds check is `ladder_len + sig_len + 4 >
    // buffer_len`, so bytes appended after the signature are ignored by the
    // native code. The wrapper rejects them instead: a signed ladder has
    // exactly one length.
    let baseline = baseline();
    let bytes = appended(baseline.ladder.as_bytes());
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_ladder_with_a_tampered_declared_signature_length_is_a_layout_error() {
    let baseline = baseline();
    let rung_count = u16::from_be_bytes(
        baseline.ladder.as_bytes()[2 + SID_LEN..4 + SID_LEN]
            .try_into()
            .expect("two bytes"),
    ) as usize;
    let unsigned = 4 + SID_LEN + rung_count * (2 * 8 + HASH_SIZE);
    let declared = u32::from_be_bytes(
        baseline.ladder.as_bytes()[unsigned..unsigned + 4]
            .try_into()
            .expect("four bytes"),
    );
    let bytes = patch(
        baseline.ladder.as_bytes(),
        unsigned,
        &(declared + 1).to_be_bytes(),
    );
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_tampered_ladder_signature_is_a_cryptographic_rejection() {
    let baseline = baseline();
    let last = baseline.ladder.len() - 1;
    let bytes = patch(baseline.ladder.as_bytes(), last, &[0xff]);
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Condensed(baseline.condensed),
        Some(&Ladder::from_bytes(bytes)),
        false,
    );
    expect_crypto_rejection(result);
}

// ---------------------------------------------------------------------------
// Full-signature layout cases
// ---------------------------------------------------------------------------

#[test]
fn a_full_signature_with_a_trailing_byte_is_a_layout_error() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Full(FullSignature::from_bytes(appended(
            baseline.full.as_bytes(),
        ))),
        None,
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn a_truncated_full_signature_is_a_layout_error() {
    let baseline = baseline();
    let bytes = baseline.full.as_bytes()[..baseline.full.len() - 1].to_vec();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Full(FullSignature::from_bytes(bytes)),
        None,
        false,
    );
    expect_malformed_ladder(result);
}

#[test]
fn an_emptied_full_signature_is_a_layout_error() {
    let baseline = baseline();
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Full(FullSignature::from_bytes(Vec::new())),
        None,
        false,
    );
    expect_malformed_condensed(result);
}

#[test]
fn a_full_signature_from_another_keypair_is_rejected() {
    let baseline = baseline();
    let other = MtlKeyPair::generate().expect("second keygen");
    let other_output = other
        .signer()
        .sign_batch(&[baseline.messages[0].as_slice()])
        .expect("sign");
    let result = baseline.verifier.verify(
        &baseline.messages[0],
        &Signature::Full(other_output.full_signature(0).expect("full")),
        None,
        false,
    );
    // The series id is part of the serialized shape, so a foreign signature is
    // caught structurally. If it were not, the native verifier would still
    // reject it cryptographically; neither path returns `Ok(true)`.
    assert!(
        matches!(
            result,
            Err(MtlError::MalformedCondensedSignature { .. } | MtlError::MalformedLadder { .. })
                | Ok(false)
        ),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Batch-size boundary
// ---------------------------------------------------------------------------

#[test]
fn an_empty_batch_is_a_typed_error() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let result = keypair.signer().sign_batch(&[]);
    assert!(
        matches!(result, Err(MtlError::EmptyBatch)),
        "got {result:?}"
    );
}

#[test]
fn an_empty_message_in_a_batch_is_a_typed_error() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let messages: [&[u8]; 2] = [b"ok", b""];
    let result = keypair.signer().sign_batch(&messages);
    assert!(
        matches!(result, Err(MtlError::EmptyMessage { index: 1 })),
        "got {result:?}"
    );
}

/// The wrapper imposes no numeric batch-size cap, so the largest benchmark size
/// is used as the empirical boundary. This signs a million messages and is
/// therefore ignored by default; run it with
/// `cargo test -p climb-mtl --test negative -- --ignored`.
#[test]
#[ignore = "signs a million leaves; run explicitly to exercise the largest benchmark batch"]
fn the_largest_benchmark_batch_signs_and_verifies() {
    const N: usize = 1_000_000;
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let message = vec![b'm'; 1];
    let messages = vec![message.as_slice(); N];

    let output = signer.sign_batch(&messages).expect("sign a million leaves");
    assert_eq!(output.len(), N);

    let verifier = keypair.verifier().expect("verifier");
    for index in [0usize, N / 2, N - 1] {
        let signature = Signature::Condensed(
            output
                .condensed_signature(index)
                .expect("condensed")
                .clone(),
        );
        assert!(
            verifier
                .verify(&message, &signature, Some(output.ladder()), false)
                .expect("verify"),
            "leaf {index} must verify"
        );
    }
}
