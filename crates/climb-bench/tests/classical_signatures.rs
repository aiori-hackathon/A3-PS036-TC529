//! Round-trip and tamper tests for the classical signature baselines that
//! the Criterion suite measures (Ed25519, ECDSA P-256, RSA-2048).
//!
//! These are the named correctness gates for the context baselines: every
//! algorithm must verify a signature it produced over a message, must reject
//! a single flipped signature byte, and the signature length recorded for the
//! consolidation step must be read back from a produced signature rather
//! than typed.
//!
//! The classical crates are dev-dependencies of this crate, so this file is
//! where they may be used; no shipped binary links them.

use ed25519_dalek::{
    Signature as Ed25519Signature, Signer as _, SigningKey as Ed25519SigningKey, Verifier as _,
    VerifyingKey as Ed25519VerifyingKey,
};
use p256::ecdsa::{
    Signature as P256Signature, SigningKey as P256SigningKey, VerifyingKey as P256VerifyingKey,
};
use rand_core::OsRng;
use rsa::{
    pkcs1v15::{
        Signature as RsaSignature, SigningKey as RsaSigningKey, VerifyingKey as RsaVerifyingKey,
    },
    // `Signer`/`Verifier` are already in scope from `ed25519_dalek` (the
    // shared `signature` crate); only RSA-specific traits are needed here.
    signature::{Keypair as _, SignatureEncoding as _},
};
use sha2::Sha256;

/// Test data: fixed key seeds so every run exercises the same keypairs.
/// Any 32 bytes are a valid Ed25519 seed; `[11; 32]` is a valid P-256
/// scalar (far below the curve order).
const ED25519_TEST_SEED: [u8; 32] = [7u8; 32];
const P256_TEST_SEED: [u8; 32] = [11u8; 32];

/// Test data: signature sizes fixed by the algorithms' specifications
/// (RFC 8032 Ed25519, FIPS 186-4 ECDSA over P-256 with fixed-width `r || s`,
/// RFC 8017 RSASSA-PKCS1-v1_5 with SHA-256 over a 2048-bit key). The
/// produced-signature test asserts a freshly signed message matches these.
const ED25519_SPEC_SIGNATURE_BYTES: usize = 64;
const P256_SPEC_SIGNATURE_BYTES: usize = 64;
const RSA2048_SPEC_SIGNATURE_BYTES: usize = 256;

const TEST_MESSAGE: &[u8] = b"classical baseline round-trip test message";

#[test]
fn ed25519_signature_round_trips() {
    let signing_key = Ed25519SigningKey::from_bytes(&ED25519_TEST_SEED);
    let verifying_key = Ed25519VerifyingKey::from(&signing_key);
    let signature = signing_key.sign(TEST_MESSAGE);
    assert!(
        verifying_key.verify(TEST_MESSAGE, &signature).is_ok(),
        "a fresh Ed25519 signature must verify"
    );
}

#[test]
fn ed25519_verify_rejects_tampered_signature() {
    let signing_key = Ed25519SigningKey::from_bytes(&ED25519_TEST_SEED);
    let verifying_key = Ed25519VerifyingKey::from(&signing_key);
    let signature = signing_key.sign(TEST_MESSAGE);
    let mut tampered = signature.to_bytes();
    tampered[0] ^= 0x01;
    let rejected = verifying_key
        .verify(TEST_MESSAGE, &Ed25519Signature::from_bytes(&tampered))
        .is_err();
    assert!(rejected, "a flipped signature byte must not verify");
}

#[test]
fn ecdsa_p256_signature_round_trips() {
    let signing_key = P256SigningKey::from_slice(&P256_TEST_SEED).expect("valid P-256 scalar");
    let verifying_key = P256VerifyingKey::from(&signing_key);
    let signature: P256Signature = signing_key.sign(TEST_MESSAGE);
    assert!(
        verifying_key.verify(TEST_MESSAGE, &signature).is_ok(),
        "a fresh ECDSA P-256 signature must verify"
    );
}

#[test]
fn ecdsa_p256_verify_rejects_tampered_signature() {
    let signing_key = P256SigningKey::from_slice(&P256_TEST_SEED).expect("valid P-256 scalar");
    let verifying_key = P256VerifyingKey::from(&signing_key);
    let signature: P256Signature = signing_key.sign(TEST_MESSAGE);
    let mut tampered = signature.to_bytes();
    tampered[63] ^= 0x01;
    let rejected = match P256Signature::from_slice(&tampered) {
        Ok(tampered) => verifying_key.verify(TEST_MESSAGE, &tampered).is_err(),
        // A decode failure is itself a rejection.
        Err(_) => true,
    };
    assert!(rejected, "a flipped signature byte must not verify");
}

#[test]
fn rsa2048_signature_round_trips() {
    let signing_key =
        RsaSigningKey::<Sha256>::random(&mut OsRng, 2048).expect("RSA-2048 key generation");
    let verifying_key: RsaVerifyingKey<Sha256> = signing_key.verifying_key();
    let signature = signing_key.sign(TEST_MESSAGE);
    assert!(
        verifying_key.verify(TEST_MESSAGE, &signature).is_ok(),
        "a fresh RSA-2048 signature must verify"
    );
}

#[test]
fn rsa2048_verify_rejects_tampered_signature() {
    let signing_key =
        RsaSigningKey::<Sha256>::random(&mut OsRng, 2048).expect("RSA-2048 key generation");
    let verifying_key: RsaVerifyingKey<Sha256> = signing_key.verifying_key();
    let signature = signing_key.sign(TEST_MESSAGE);
    let mut tampered = signature.to_bytes();
    tampered[255] ^= 0x01;
    let rejected = match RsaSignature::try_from(tampered.as_ref()) {
        Ok(tampered) => verifying_key.verify(TEST_MESSAGE, &tampered).is_err(),
        // A decode failure is itself a rejection.
        Err(_) => true,
    };
    assert!(rejected, "a flipped signature byte must not verify");
}

/// The signature lengths that feed the consolidation step must be read back
/// from produced signatures: sign with two independent keypairs per
/// algorithm, assert the length is stable across keys and matches the
/// algorithm's specified size, then round-trip the exact JSON document the
/// benchmark writes and confirm the parsed numbers are those measurements.
#[test]
fn classical_signature_lengths_are_read_from_produced_signatures() {
    // Test data: signatures produced here, keys fresh per signing pass.
    let ed25519_a = Ed25519SigningKey::from_bytes(&ED25519_TEST_SEED).sign(TEST_MESSAGE);
    let ed25519_b = Ed25519SigningKey::from_bytes(&[9u8; 32]).sign(TEST_MESSAGE);
    let p256_a: P256Signature = P256SigningKey::from_slice(&P256_TEST_SEED)
        .expect("valid P-256 scalar")
        .sign(TEST_MESSAGE);
    let p256_b: P256Signature = P256SigningKey::from_slice(&[13u8; 32])
        .expect("valid P-256 scalar")
        .sign(TEST_MESSAGE);
    let rsa_key_a = RsaSigningKey::<Sha256>::random(&mut OsRng, 2048).expect("keygen");
    let rsa_key_b = RsaSigningKey::<Sha256>::random(&mut OsRng, 2048).expect("keygen");
    let rsa_a = rsa_key_a.sign(TEST_MESSAGE);
    let rsa_b = rsa_key_b.sign(TEST_MESSAGE);

    let measured = [
        (
            "ed25519",
            ed25519_a.to_bytes().len(),
            ed25519_b.to_bytes().len(),
            ED25519_SPEC_SIGNATURE_BYTES,
        ),
        (
            "ecdsa_p256",
            p256_a.to_bytes().len(),
            p256_b.to_bytes().len(),
            P256_SPEC_SIGNATURE_BYTES,
        ),
        (
            "rsa2048",
            rsa_a.to_bytes().len(),
            rsa_b.to_bytes().len(),
            RSA2048_SPEC_SIGNATURE_BYTES,
        ),
    ];
    for (algorithm, first, second, spec) in measured {
        assert_eq!(
            first, second,
            "{algorithm}: signature length must not depend on the keypair"
        );
        assert_eq!(
            first, spec,
            "{algorithm}: produced signature length must match the algorithm's specified size"
        );
        assert!(first > 0, "{algorithm}: a produced signature has bytes");
    }

    // The JSON document the benchmark writes: exactly these three fields,
    // each the measured length of a freshly produced signature.
    let document = serde_json::json!({
        "ed25519": measured[0].1,
        "ecdsa_p256": measured[1].1,
        "rsa2048": measured[2].1,
    });
    let encoded = serde_json::to_vec(&document).expect("document encodes");
    let parsed: serde_json::Value = serde_json::from_slice(&encoded).expect("document parses");
    let object = parsed.as_object().expect("object document");
    assert_eq!(object.len(), 3, "exactly one field per algorithm");
    assert_eq!(object["ed25519"].as_u64(), Some(measured[0].1 as u64));
    assert_eq!(object["ecdsa_p256"].as_u64(), Some(measured[1].1 as u64));
    assert_eq!(object["rsa2048"].as_u64(), Some(measured[2].1 as u64));
}
