//! Conformance evidence for the safe MTL wrapper.
//!
//! Two independent questions are answered here:
//!
//! 1. **What test material does the pinned library ship?** The vendored test
//!    directory is inventoried and checked for a known-answer vector for the
//!    scheme CLIMB measures. It ships vectors for other schemes only, so a
//!    known-answer test against upstream bytes is not possible for the default
//!    scheme; that absence is asserted rather than glossed over.
//! 2. **Does the wrapper still accept bytes the pinned library produced?** A
//!    one-time fixture pins the public half of a keypair plus deterministic
//!    messages, the shared ladder, every condensed signature and one full
//!    signature. Regenerating the keypair is intentionally not part of the
//!    test: libMTL's key generation is not seedable through the public API, so
//!    the fixture pins outputs, not secrets.
//!
//! The fixture contains only public values. It is a regression pin for the
//! serialized layout and the wrapper's acceptance path, not a claim that the
//! scheme is secure or standardised.

use climb_mtl::{CondensedSignature, FullSignature, Ladder, MtlVerifier, Signature};

/// The one-time fixture written against the pinned library revision.
const FIXTURE: &str = include_str!("fixtures/slh_dsa_sha2_128s_mtl_sha2_128_53ce25d.json");

const TEST_VECTORS: &str = include_str!("../../../third_party/MTL/test/mtltest_test_vectors.h");
const SIGNED_LADDER: &str = include_str!("../../../third_party/MTL/test/mtltest_signed_ladder.h");
const FULL_SIGNATURE: &str = include_str!("../../../third_party/MTL/test/mtltest_full_signature.h");

#[derive(Debug, serde::Deserialize)]
struct Fixture {
    algorithm: String,
    library_revision: String,
    public_key_hex: String,
    series_id_hex: String,
    messages_hex: Vec<String>,
    ladder_hex: String,
    condensed_hex: Vec<String>,
    full_signature_hex: String,
}

fn decode_hex(text: &str) -> Vec<u8> {
    assert!(
        text.len().is_multiple_of(2),
        "hex string has an odd length: {} characters",
        text.len()
    );
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push(((hi << 4) | lo) as u8);
    }
    out
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("committed fixture parses")
}

fn verifier(fixture: &Fixture) -> MtlVerifier {
    MtlVerifier::from_public_key(
        &fixture.algorithm,
        &decode_hex(&fixture.public_key_hex),
        &decode_hex(&fixture.series_id_hex),
    )
    .expect("fixture public key and series id form a verifier")
}

#[test]
fn the_pinned_library_ships_no_vector_for_the_measured_scheme() {
    // Inventory, as found in the pinned checkout.
    assert!(
        TEST_VECTORS.contains("ML-DSA-44-MTL-SHAKE-128"),
        "expected the ML-DSA test-vector scheme in mtltest_test_vectors.h"
    );
    assert!(
        SIGNED_LADDER.contains("MTL_TEST_SIGNED_LADDER_ML_DSA_44_MTL_SHAKE_128")
            && SIGNED_LADDER.contains("MTL_TEST_SIGNED_LADDER_SLH_DSA_MTL_SHAKE_128F"),
        "expected the two schemes the signed-ladder header defines"
    );
    assert!(
        FULL_SIGNATURE.contains("MTL_TEST_FULL_SIGNATURE_SLH_DSA_MTL_SHAKE_128S"),
        "expected the full-signature vector header's scheme"
    );

    // None of the three vector headers carries the default scheme. The
    // algorithm *name table* mentions it (it lists every compiled scheme), but
    // that is not a known-answer vector.
    let target = climb_mtl::DEFAULT_ALGORITHM;
    for (name, text) in [
        ("mtltest_test_vectors.h", TEST_VECTORS),
        ("mtltest_signed_ladder.h", SIGNED_LADDER),
        ("mtltest_full_signature.h", FULL_SIGNATURE),
    ] {
        assert!(
            !text.contains(target),
            "{name} unexpectedly carries a vector for {target}; update the conformance fixture and this inventory"
        );
    }
}

#[test]
fn the_fixture_names_the_pinned_library_revision() {
    let fixture = fixture();
    assert_eq!(
        fixture.library_revision, "53ce25dcc35c15e051c5263e41aaeaede18a8aa7",
        "the fixture must name the revision that produced it"
    );
    assert_eq!(fixture.library_revision.len(), 40);
}

#[test]
fn the_fixture_uses_the_default_algorithm() {
    assert_eq!(fixture().algorithm, climb_mtl::DEFAULT_ALGORITHM);
}

#[test]
fn every_recorded_condensed_signature_verifies_against_the_recorded_ladder() {
    let fixture = fixture();
    let verifier = verifier(&fixture);
    let ladder = Ladder::from_bytes(decode_hex(&fixture.ladder_hex));

    assert_eq!(
        fixture.messages_hex.len(),
        fixture.condensed_hex.len(),
        "one condensed signature per message"
    );

    for (index, (message_hex, condensed_hex)) in fixture
        .messages_hex
        .iter()
        .zip(fixture.condensed_hex.iter())
        .enumerate()
    {
        let message = decode_hex(message_hex);
        let signature =
            Signature::Condensed(CondensedSignature::from_bytes(decode_hex(condensed_hex)));

        assert!(
            verifier
                .verify(&message, &signature, Some(&ladder), false)
                .unwrap_or_else(|error| {
                    panic!("fixture message {index} failed structurally: {error}")
                }),
            "fixture message {index} must verify against the recorded ladder"
        );
        assert!(
            verifier
                .verify(&message, &signature, Some(&ladder), true)
                .expect("trusted-ladder verify"),
            "fixture message {index} must verify when the ladder is trusted"
        );
    }
}

#[test]
fn the_recorded_full_signature_verifies_standalone() {
    let fixture = fixture();
    let verifier = verifier(&fixture);
    let message = decode_hex(&fixture.messages_hex[0]);
    let full = Signature::Full(FullSignature::from_bytes(decode_hex(
        &fixture.full_signature_hex,
    )));

    assert!(
        verifier
            .verify(&message, &full, None, false)
            .expect("full-signature verify"),
        "the recorded full signature must verify standalone"
    );
}

#[test]
fn the_full_signature_is_the_condensed_prefix_plus_the_ladder() {
    let fixture = fixture();
    let condensed = decode_hex(&fixture.condensed_hex[0]);
    let ladder = decode_hex(&fixture.ladder_hex);
    let full = decode_hex(&fixture.full_signature_hex);

    assert_eq!(
        full.len(),
        condensed.len() + ladder.len(),
        "a full signature is exactly condensed || ladder"
    );
    assert_eq!(&full[..condensed.len()], &condensed[..]);
    assert_eq!(&full[condensed.len()..], &ladder[..]);
}
