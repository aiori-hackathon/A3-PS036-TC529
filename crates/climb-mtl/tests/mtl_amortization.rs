//! The empirical proof that MTL mode does what it claims.
//!
//! For a batch larger than one, the condensed signature — the small form MTL
//! mode emits and amortizes — must be strictly smaller than the full signature,
//! which carries a freshly signed ladder. If this ever fails, the "with MTL"
//! condition is not actually saving anything and no Batch 2 benchmark built on
//! it would be trustworthy.

use climb_mtl::{MtlKeyPair, Signature};

fn messages(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("ladder-size message {i:04}").into_bytes())
        .collect()
}

#[test]
fn condensed_signature_is_smaller_than_full_for_batch_larger_than_one() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();

    for n in [2usize, 10, 100] {
        let owned = messages(n);
        let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        let output = signer.sign_batch(&refs).expect("sign_batch");

        let condensed_len = output.condensed_signature(0).expect("condensed").len();
        let full_len = output.full_signature(0).expect("full").len();

        assert!(
            condensed_len < full_len,
            "batch of {n}: condensed ({condensed_len} B) must be smaller than full ({full_len} B)"
        );
        assert_eq!(
            full_len,
            condensed_len + output.ladder().len(),
            "batch of {n}: the full signature must be the condensed bytes followed by the ladder"
        );
    }
}

#[test]
fn condensed_signature_beats_the_full_baseline_signature() {
    // The fair "without MTL" baseline reuses the same tool: one message per call,
    // each paying for a freshly signed ladder. Its condensed signature is the
    // N=1 case. For N > 1, the shared-ladder condensed signature of a message in
    // the batch must be smaller than the full signature any single-message call
    // would transmit.
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    const N: usize = 100;
    let owned = messages(N);
    let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let batch = signer.sign_batch(&refs).expect("sign_batch");

    // Without MTL: N separate calls, one message each.
    let baseline = signer.sign_batch(&[refs[0]]).expect("baseline sign");
    let baseline_full_len = baseline.full_signature(0).expect("full").len();

    let amortized_condensed_len = batch.condensed_signature(0).expect("condensed").len();
    assert!(
        amortized_condensed_len < baseline_full_len,
        "with-MTL condensed ({amortized_condensed_len} B) must beat the without-MTL full signature ({baseline_full_len} B)"
    );

    // And the amortized condensed signature must still actually verify.
    let signature = Signature::Condensed(batch.condensed_signature(0).unwrap().clone());
    assert!(
        verifier
            .verify(refs[0], &signature, Some(batch.ladder()), false)
            .expect("verify"),
        "the smaller condensed signature must still verify"
    );
}

#[test]
fn one_shared_ladder_serves_the_whole_batch() {
    // Adding messages grows the *ladder* by at most a constant number of rungs,
    // while each message's full signature would otherwise carry a whole new
    // ~8 KB signed ladder. The proof that amortization is real is that the batch
    // has exactly one ladder regardless of message count.
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();

    let single = signer.sign_batch(&[b"one"]).expect("sign");
    let many_owned = messages(64);
    let many_refs: Vec<&[u8]> = many_owned.iter().map(Vec::as_slice).collect();
    let many = signer.sign_batch(&many_refs).expect("sign");

    assert_eq!(many.len(), 64);
    // The ladder does not multiply with the message count; it stays on the order
    // of a single underlying signature (~8 KB), not 64 of them.
    assert!(
        many.ladder().len() < 2 * single.ladder().len(),
        "64 messages must share one ladder, not accumulate one per message: {} vs {}",
        many.ladder().len(),
        single.ladder().len()
    );
}

#[test]
fn per_message_condensed_size_is_far_smaller_than_the_ladder() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let owned = messages(1000);
    let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let output = signer.sign_batch(&refs).expect("sign_batch");

    let condensed = output.condensed_signature(0).expect("condensed").len();
    let ladder = output.ladder().len();

    // A 1000-message batch shares one ~8 KB ladder; each response carries only a
    // small authentication path (tens to low hundreds of bytes), an order of
    // magnitude smaller than the ladder it authenticates against.
    assert!(
        condensed * 10 < ladder,
        "condensed ({condensed} B) should be an order of magnitude smaller than the shared ladder ({ladder} B)"
    );

    // Every message in the batch must verify.
    let verifier = keypair.verifier().expect("verifier");
    for (index, message) in refs.iter().enumerate() {
        let signature = Signature::Condensed(output.condensed_signature(index).unwrap().clone());
        assert!(
            verifier
                .verify(message, &signature, Some(output.ladder()), false)
                .expect("verify"),
            "message {index} must verify in a 1000-message batch"
        );
    }
}
