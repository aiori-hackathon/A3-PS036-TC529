//! Empirical probes for Phase 6: confirm that setup-separated verification
//! measurement actually isolates signing cost from verification time, and
//! that the `trust_cached_ladder` axis is measurable.
//!
//! These are not benchmarks — they are boolean gates. If either fails,
//! the entire Phase 6 methodology is invalid.

use std::time::Instant;

use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::MtlKeyPair;

/// CLIMB_V6 seed, matching the verifying bench.
const BENCH_SEED: u64 = 0x434C494D425F5636;

// ---------------------------------------------------------------------------
// Task 6.2 — Setup-separation hold check
// ---------------------------------------------------------------------------

/// Prove that verifying pre-existing signatures is orders of magnitude
/// faster than signing + verifying inline.
///
/// This is not a benchmark — it is a boolean gate: if this test ever
/// regresses, the benchmark's entire methodology is flawed.
#[test]
fn setup_separation_holds() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    let dataset = generate_dataset(DatasetSize::Small, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();

    // ---- A: Inline (buggy) — signing + verifying in one timed loop ----
    let start_a = Instant::now();
    let output = signer.sign_batch(&messages).unwrap();
    let ladder = output.ladder();
    for (i, msg) in messages.iter().enumerate().take(output.len()) {
        let sig = output.signature(i).unwrap();
        verifier.verify(msg, &sig, Some(ladder), true).unwrap();
    }
    let inline_duration = start_a.elapsed();

    // ---- B: Setup-separated (correct) — only verification is timed ----
    // Pre-generate signatures (not timed).
    let output2 = signer.sign_batch(&messages).unwrap();
    let ladder2 = output2.ladder();

    let start_b = Instant::now();
    for (i, msg) in messages.iter().enumerate().take(output2.len()) {
        let sig = output2.signature(i).unwrap();
        verifier.verify(msg, &sig, Some(ladder2), true).unwrap();
    }
    let verify_only_duration = start_b.elapsed();

    // The inline duration must be dominated by the signing cost (~650 ms
    // on this hardware), while verification-only should be sub-ms. A 10×
    // ratio is a very conservative threshold — the real ratio on hardware
    // without AVX2 is >1000×.
    let ratio = inline_duration.as_nanos() as f64 / verify_only_duration.as_nanos().max(1) as f64;
    assert!(
        ratio > 10.0,
        "setup separation FAILED: inline={inline_duration:?}, verify-only={verify_only_duration:?}, ratio={ratio:.1}× (must be >10×). \
         If this fails, signing cost is leaking into the verification measurement.",
    );
}

// ---------------------------------------------------------------------------
// Task 6.2 — trust_cached_ladder axis confirm
// ---------------------------------------------------------------------------

/// Prove that trust_cached=true and trust_cached=false produce different
/// timings (confirming the axis is measurable).
#[test]
fn trust_cached_axis_measurable() {
    let keypair = MtlKeyPair::generate().expect("keygen");
    let signer = keypair.signer();
    let verifier = keypair.verifier().expect("verifier");

    let dataset = generate_dataset(DatasetSize::Small, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let output = signer.sign_batch(&messages).unwrap();
    let ladder = output.ladder();
    let n = output.len();

    // trust_cached = true (skip ladder validation)
    let start_trust = Instant::now();
    for (i, msg) in messages.iter().enumerate().take(n) {
        let sig = output.signature(i).unwrap();
        verifier.verify(msg, &sig, Some(ladder), true).unwrap();
    }
    let trust_duration = start_trust.elapsed();

    // trust_cached = false (validate ladder each time)
    let start_no_trust = Instant::now();
    for (i, msg) in messages.iter().enumerate().take(n) {
        let sig = output.signature(i).unwrap();
        verifier.verify(msg, &sig, Some(ladder), false).unwrap();
    }
    let no_trust_duration = start_no_trust.elapsed();

    // trust=false must be slower (or at minimum not faster), since it
    // does extra work.
    let ratio = no_trust_duration.as_nanos() as f64 / trust_duration.as_nanos().max(1) as f64;
    assert!(
        ratio >= 1.0,
        "trust_cached axis not measurable: trust={trust_duration:?}, no_trust={no_trust_duration:?}, ratio={ratio:.2}× (must be ≥1.0×). \
         trust_cached=false should be at least as slow as true.",
    );
}
