//! Round-trip sign → verify tests across batch sizes.
//!
//! Each batch is signed once (one shared ladder) and every message is then
//! verified in both the condensed form (against the ladder) and the full form
//! (standalone, with its embedded ladder).

use climb_mtl::{MtlKeyPair, Signature};

fn messages(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("climb round-trip message number {i:04}").into_bytes())
        .collect()
}

#[test]
fn round_trip_batch_size_1() {
    round_trip(1);
}

#[test]
fn round_trip_batch_size_2() {
    round_trip(2);
}

#[test]
fn round_trip_batch_size_10() {
    round_trip(10);
}

fn round_trip(n: usize) {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    let owned = messages(n);
    let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();

    let output = signer.sign_batch(&refs).expect("sign_batch");
    assert_eq!(output.len(), n, "one signed message per input");
    assert!(!output.ladder().is_empty(), "ladder must not be empty");

    for (index, message) in refs.iter().enumerate() {
        let signed = output.get(index).expect("message present");
        assert_eq!(
            signed.leaf_index(),
            index as u64,
            "leaf index must follow input order"
        );

        let condensed = Signature::Condensed(output.condensed_signature(index).unwrap().clone());
        assert!(
            verifier
                .verify(message, &condensed, Some(output.ladder()), false)
                .expect("condensed verify"),
            "condensed signature {index} must verify against the shared ladder"
        );

        let full = output.full_signature_as(index).expect("full signature");
        assert!(
            verifier
                .verify(message, &full, None, false)
                .expect("full verify"),
            "full signature {index} must verify standalone"
        );
    }
}

#[test]
fn trust_cached_ladder_accepts_the_same_condensed_signature() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    let owned = messages(4);
    let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let output = signer.sign_batch(&refs).expect("sign_batch");

    for (index, message) in refs.iter().enumerate() {
        let condensed = output.signature(index).expect("condensed signature");
        assert!(
            verifier
                .verify(message, &condensed, Some(output.ladder()), true)
                .expect("trusted-ladder verify"),
            "message {index} must verify when the ladder is trusted as cached"
        );
        assert!(
            verifier
                .verify(message, &condensed, Some(output.ladder()), false)
                .expect("revalidated-ladder verify"),
            "message {index} must verify when the ladder is revalidated"
        );
    }
}
