//! Byte-level wire accounting over one deterministic batch.
//!
//! Three configurations are compared on the *same* messages:
//!
//! | Label | Composition | Source of bytes |
//! |---|---|---|
//! | A | One plain SLH-DSA signature per message | `plain_signature_len()` from liboqs |
//! | B | One complete MTL signature per message (`condensed || ladder`) | reconstructed by the wrapper |
//! | C | One condensed proof per message plus one shared signed ladder | `sum(condensed) + ladder` |
//!
//! A and C are arithmetic over measured lengths; B is the sum of the actual
//! reconstructed byte vectors, so the identity `B - C == (N - 1) * ladder` is
//! checked against real bytes rather than assumed. This is the accounting
//! counterpart to the sign and verify benchmarks: it says what is on the wire,
//! not how long signing took.

use climb_bench::DEFAULT_WIRE_SEED;
use climb_dns::generate_dataset_count;
use climb_mtl::{plain_signature_len, MtlKeyPair};

/// Small enough to reconstruct every full signature, large enough that the
/// ladder is shared more than twice.
const MESSAGES: usize = 5;

#[test]
fn wire_totals_reconcile_byte_for_byte() {
    let dataset = generate_dataset_count(MESSAGES, DEFAULT_WIRE_SEED);
    assert_eq!(dataset.len(), MESSAGES);

    let messages: Vec<&[u8]> = dataset.iter().map(Vec::as_slice).collect();
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let output = signer.sign_batch(&messages).expect("sign_batch");

    let plain_len = plain_signature_len().expect("liboqs reports a signature length");
    let ladder_len = output.ladder().len();
    assert!(ladder_len > 0);

    let mut condensed_total = 0u64;
    let mut full_total = 0u64;
    for index in 0..MESSAGES {
        let condensed_len = output.messages()[index].condensed().len();
        let full_len = output.full_signature(index).expect("full signature").len();
        assert_eq!(
            full_len,
            condensed_len + ladder_len,
            "full signature {index} is not condensed || ladder"
        );
        condensed_total += condensed_len as u64;
        full_total += full_len as u64;
    }

    let plain_total = MESSAGES as u64 * plain_len as u64;
    let amortised_total = condensed_total + ladder_len as u64;

    // B is the actual sum of reconstructed buffers, so this identity is
    // evidence, not a restatement of the model.
    assert_eq!(
        full_total,
        condensed_total + MESSAGES as u64 * ladder_len as u64
    );
    // The only bytes amortisation removes are the repeated ladders.
    assert_eq!(
        full_total - amortised_total,
        (MESSAGES as u64 - 1) * ladder_len as u64
    );

    // A complete MTL signature carries the ladder, and the ladder carries its
    // own SLH-DSA signature, so at this batch size B costs more per message
    // than A. Amortisation is what makes C smaller than A.
    let full_per_message = full_total / MESSAGES as u64;
    assert!(
        full_per_message > plain_len as u64,
        "expected one full MTL signature ({full_per_message} B) to exceed a plain signature ({plain_len} B)"
    );
    assert!(
        amortised_total < plain_total,
        "expected the shared ladder ({amortised_total} B) to beat plain signatures ({plain_total} B)"
    );

    println!(
        "N={MESSAGES} plain={plain_len} ladder={ladder_len} condensed_total={condensed_total}"
    );
    println!(
        "A={plain_total} B={full_total} C={amortised_total} (B-C={} = (N-1)*ladder={})",
        full_total - amortised_total,
        (MESSAGES as u64 - 1) * ladder_len as u64
    );
}

#[test]
fn one_message_makes_the_ladder_a_pure_loss() {
    let dataset = generate_dataset_count(1, DEFAULT_WIRE_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(Vec::as_slice).collect();

    let keypair = MtlKeyPair::generate().expect("keygen");
    let output = keypair.signer().sign_batch(&messages).expect("sign_batch");
    let plain_len = plain_signature_len().expect("liboqs signature length") as u64;

    let condensed = output.messages()[0].condensed().len() as u64;
    let ladder = output.ladder().len() as u64;
    let amortised = condensed + ladder;

    assert_eq!(
        amortised,
        output.full_signature(0).expect("full signature").len() as u64
    );
    assert!(
        amortised > plain_len,
        "with one message the shared ladder has nothing to amortise over"
    );
}
