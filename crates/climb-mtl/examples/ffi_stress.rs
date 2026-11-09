//! Dynamic FFI stress harness for the safe MTL wrapper.
//!
//! Exercises key generation, batch signing, condensed and full verification,
//! tamper rejection, typed error paths, the plain SLH-DSA baseline, and the
//! verify-only key path in a loop. Used with a leak/UB checker:
//!
//! ```text
//! cargo run -p climb-mtl --example ffi_stress
//! ```
//!
//! `CLIMB_FFI_STRESS_ITERS` overrides the default iteration count (5). The
//! harness exits non-zero on the first failed assertion.

use std::error::Error;

use climb_mtl::{
    CondensedSignature, MtlKeyPair, MtlVerifier, PlainSlhDsa, Signature, DEFAULT_ALGORITHM,
};

fn iterations() -> usize {
    std::env::var("CLIMB_FFI_STRESS_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5)
}

fn main() -> Result<(), Box<dyn Error>> {
    let iterations = iterations();
    let keypair = MtlKeyPair::generate()?;
    let signer = keypair.signer();
    let verifier = keypair.verifier()?;

    let messages: Vec<Vec<u8>> = (0..13)
        .map(|index| format!("climb.example. IN TXT \"stress message {index}\"").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = messages.iter().map(Vec::as_slice).collect();

    for iteration in 0..iterations {
        let output = signer.sign_batch(&refs)?;
        assert_eq!(output.len(), messages.len());

        for (index, message) in messages.iter().enumerate() {
            let condensed = output.signature(index)?;
            assert!(
                verifier.verify(message, &condensed, Some(output.ladder()), false)?,
                "cold condensed verification failed at iteration {iteration}, message {index}"
            );
            assert!(
                verifier.verify(message, &condensed, Some(output.ladder()), true)?,
                "cached condensed verification failed at iteration {iteration}, message {index}"
            );

            let full = output.full_signature_as(index)?;
            assert!(
                verifier.verify(message, &full, None, false)?,
                "full verification failed at iteration {iteration}, message {index}"
            );

            let mut tampered = message.clone();
            tampered[0] ^= 0x01;
            assert!(
                !verifier.verify(&tampered, &condensed, Some(output.ladder()), true)?,
                "a tampered message verified at iteration {iteration}, message {index}"
            );
        }

        // Typed error paths, all before or at the native boundary.
        assert!(signer.sign_batch(&[]).is_err(), "empty batch accepted");
        assert!(signer.sign_batch(&[&[]]).is_err(), "empty message accepted");
        let oversized = vec![0u8; climb_mtl::MAX_MESSAGE_LEN + 1];
        assert!(
            signer.sign_batch(&[oversized.as_slice()]).is_err(),
            "oversized message accepted"
        );
        assert!(
            MtlKeyPair::generate_with_algorithm("NOT_A_REAL_MTL_ALGORITHM").is_err(),
            "unknown algorithm accepted"
        );
        assert!(
            MtlVerifier::from_public_key(DEFAULT_ALGORITHM, keypair.public_key(), &[]).is_err(),
            "empty series id accepted"
        );

        // A malformed condensed signature must be a typed shape error, not a
        // native call and not a panic.
        let empty_signature = Signature::Condensed(CondensedSignature::from_bytes(Vec::new()));
        assert!(
            verifier
                .verify(&messages[0], &empty_signature, Some(output.ladder()), true)
                .is_err(),
            "empty condensed signature accepted"
        );

        // Plain SLH-DSA baseline: scheme allocation, keypair, sign, verify.
        let plain = PlainSlhDsa::generate()?;
        let signature = plain.sign(b"plain baseline stress message")?;
        assert!(plain.verify(b"plain baseline stress message", &signature)?);
        assert!(!plain.verify(b"tampered", &signature)?);
    }

    println!("ffi_stress: {iterations} iteration(s), no assertion failures");
    Ok(())
}
