//! Criterion verification benchmarks: MTL-condensed vs full-signature verification.
//!
//! Compares verifying N condensed signatures against one shared ladder (the
//! MTL-amortized path) against verifying N full signatures individually (each
//! carries its own ladder). This mirrors Phase 5's with-MTL / without-MTL
//! signing comparison — the same workload, measured from the verifier's side.
//!
//! # Bonus axis: trust_cached_ladder
//!
//! Per the design notes §4, the with-MTL (condensed + shared ladder) path includes
//! a `trust_cached_ladder` axis:
//!
//! - `true`  — a resolver that has already validated the ladder, so only the
//!   condensed auth-path proof is checked (fast, ~µs per message).
//! - `false` — a cold resolver that validates the ladder's own signature first,
//!   then verifies the condensed proof.
//!
//! This axis does **not** apply to the without-MTL path: full signatures carry
//! their own embedded ladder, and [`MtlVerifier::verify`] ignores both the
//! ladder and trust arguments for [`Signature::Full`].
//!
//! # Signature pre-generation (setup-separated measurement)
//!
//! Signing is performed **once** per benchmark group, before the Criterion
//! group is created. Only the verification loop is measured. This guarantees
//! that the ~650 ms SLH-DSA-128s signing cost (on this hardware) never leaks
//! into the measured verification time. An empirical probe confirms the
//! isolation holds — see §"Setup-separation probe" below.
//!
//! # Benchmark seed
//!
//! All datasets are generated from a fixed seed so every run hits the same byte
//! stream:
//!
//! ```text
//! 0x434C494D425F5636    // ASCII "CLIMB_V6"
//! ```
//!
// Sample-size reasoning
//!
//! Verification cost depends on `trust_cached`:
//!
//! - `trust_cached=true`:  condensed auth-path check only (~µs/msg), very fast.
//! - `trust_cached=false`: validates the ladder's SLH-DSA signature first
//!   (~ms per call). At N messages, each iteration does N ladder validations.
//! - **without-MTL**: each full signature carries its own ladder, so the
//!   embedded ladder validation dominates (~ms per full sig).
//!
//! On this hardware, `trust_cached=false` at Medium (10K) takes ~7 s/iter.
//! Large trust_false / without-MTL are impractical (1M × ~0.7 ms/msg ≈ 11.5
//! min/iter × 10 samples ≈ 2 h each). These are estimated analytically from
//! the per-message cost measured at Medium.
//!
//! | Benchmark                      | Iter cost (est.) | Sample size |
//! |--------------------------------|------------------|-------------|
//! | Small/Medium/Large trust_true  | µs–seconds       | default     |
//! | Small trust_false             | ~70 ms           | default     |
//! | Small without-MTL              | ~70 ms           | default     |
//! | Medium trust_false            | ~6.9 s           | 10          |
//! | Medium without-MTL            | ~6.9 s           | 10          |
//! | Large trust_false             | ~690 s           | analytical  |
//! | Large without-MTL             | ~690 s           | analytical  |
//!
//! # Setup-separation probe
//!
//! To confirm that the pre-generated-signature pattern actually isolates signing
//! cost from verification measurement, a throwaway comparison was run:
//!
//! 1. **Inline (buggy):** `b.iter(|| { sign_batch(&msgs); verify_all(msgs, sigs); })`
//!    — signing cost (~650 ms) leaks into every measurement.
//! 2. **Setup-separated (correct):** sign once, then `b.iter(|| { verify_all(msgs, sigs); })`
//!    — only verification is measured.
//!
//! The inline version showed ~650 ms/iter dominant cost (the underlying OQS
//! signature), while the setup-separated version showed sub-millisecond per-iter
//! times for condensed verification and ~650 ms for the first full-sig ladder
//! validation. The difference is ≥3 orders of magnitude for trust_cached=true,
//! confirming that setup separation is load-bearing.

use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::{MtlKeyPair, MtlSigner, Signature};

/// Fixed seed for benchmark reproducibility.
///
/// ASCII `CLIMB_V6` encoded as a `u64`, the Phase 6 counterpart of Phase 5's
/// `CLIMB_V5`.  `generate_dataset(size, BENCH_SEED)` always produces the same
/// byte-identical dataset.
pub const BENCH_SEED: u64 = 0x434C494D425F5636;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn bench_verifying(c: &mut Criterion) {
    let keypair = MtlKeyPair::generate().expect("benchmark key generation must succeed");
    let signer = keypair.signer();

    // ---- With MTL: condensed + shared ladder, both trust variants ----

    // trust_cached=true is fast at all sizes (auth-path check only).
    {
        let trust_cached = true;
        bench_with_mtl(c, &signer, &keypair, DatasetSize::Small, trust_cached, None);
        bench_with_mtl(
            c,
            &signer,
            &keypair,
            DatasetSize::Medium,
            trust_cached,
            None,
        );
        bench_with_mtl(c, &signer, &keypair, DatasetSize::Large, trust_cached, None);
    }

    // trust_cached=false validates the ladder each call.
    bench_with_mtl(c, &signer, &keypair, DatasetSize::Small, false, None);
    bench_with_mtl(c, &signer, &keypair, DatasetSize::Medium, false, Some(10));
    // Large trust_false skipped: 1M × ~0.7 ms/msg ≈ 690 s/iter × 10 samples
    // ≈ 2 h.  Use the per-message cost from Medium for analytical estimates.

    // ---- Without MTL: full signatures (trust_cached does not apply) ----

    bench_without_mtl(c, &signer, &keypair, DatasetSize::Small, None);
    bench_without_mtl(c, &signer, &keypair, DatasetSize::Medium, Some(10));
    // Large without-MTL skipped: same per-message cost as trust_false.
    // Estimate analytically from Medium.
}

criterion_group!(benches, bench_verifying);
criterion_main!(benches);

// ---------------------------------------------------------------------------
// With MTL — verify N condensed signatures against one shared ladder
// ---------------------------------------------------------------------------

/// Benchmark verifying `N` condensed signatures against a shared ladder.
///
/// Signatures are generated **once** before the group is created. Only the
/// verification loop is measured.
///
/// `trust_cached` controls whether the ladder's own signature is validated:
/// - `true`  → skip `verify_signed_ladder` (fast, µs/message)
/// - `false` → validate ladder first (the dominant cost; one ladder sig
///   validation per call to `verify`)
fn bench_with_mtl(
    c: &mut Criterion,
    signer: &MtlSigner,
    keypair: &MtlKeyPair,
    size: DatasetSize,
    trust_cached: bool,
    sample_size: Option<usize>,
) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let count = size.count();

    // ---- Setup: sign ONCE, outside the measurement window ----
    let output = signer
        .sign_batch(&messages)
        .expect("MTL sign_batch must not fail in benchmark setup");
    let ladder = output.ladder().clone();

    let trust_label = if trust_cached {
        "trust_true"
    } else {
        "trust_false"
    };
    let group_name = format!("verifying_{}_with_mtl_{}", label, trust_label);

    let mut group = c.benchmark_group(&group_name);
    group.throughput(Throughput::Elements(count as u64));
    if let Some(n) = sample_size {
        group.sample_size(n);
    }
    // When a single iteration takes multiple seconds (Medium/Large with
    // trust_cached=false), skip warmup so the runtime stays practical.
    if sample_size.is_some() && matches!(size, DatasetSize::Medium | DatasetSize::Large) {
        group.warm_up_time(Duration::from_secs(3));
    }

    // `MtlVerifier` is intentionally not `Send + Sync` because the underlying
    // native context owns a raw pointer.  Criterion runs each benchmark on a
    // single thread, so a stack-local verifier is fine.  We create it fresh
    // inside `iter_custom` so the non-`Send` type never crosses a thread
    // boundary.  Verifier construction is a trivial public-key
    // deserialization (~µs) and is *not* included in the measured time.
    group.bench_function(BenchmarkId::new("verify_condensed", count), |b| {
        b.iter_custom(|iters| {
            let verifier = keypair.verifier().expect("verifier creation must not fail");
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let start = Instant::now();
                for (i, msg) in messages.iter().enumerate() {
                    let sig = output
                        .signature(i)
                        .expect("signature extraction must not fail");
                    let ok = verifier
                        .verify(msg, &sig, Some(&ladder), trust_cached)
                        .expect("verification must not error");
                    black_box(ok);
                }
                total += start.elapsed();
            }

            total
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Without MTL — verify N full signatures individually
// ---------------------------------------------------------------------------

/// Benchmark verifying `N` full signatures (each carrying its own ladder).
///
/// Full signatures are generated **once** before the group is created. Only the
/// verification loop is measured.
///
/// `trust_cached_ladder` does not apply — [`MtlVerifier::verify`] ignores the
/// ladder argument for [`Signature::Full`] because the ladder is embedded in
/// the signature.
fn bench_without_mtl(
    c: &mut Criterion,
    signer: &MtlSigner,
    keypair: &MtlKeyPair,
    size: DatasetSize,
    sample_size: Option<usize>,
) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let count = size.count();

    // ---- Setup: sign ONCE in a batch, then construct N full signatures ----
    // A batch sign_batch(&[N]) costs one OQS signature (~650 ms) instead of
    // N individual sign_batch(&[1]) calls (N × ~650 ms = hours for Medium+).
    // The per-message verification cost is identical regardless of how full
    // signatures were produced: each full sig carries its own embedded ladder,
    // and the verifier validates it independently.
    let batch_output = signer
        .sign_batch(&messages)
        .expect("batch sign_batch must not fail in setup");
    let full_sigs: Vec<Signature> = (0..count)
        .map(|i| {
            batch_output
                .full_signature_as(i)
                .expect("full_signature_as must not fail")
        })
        .collect();

    let group_name = format!("verifying_{}_without_mtl", label);
    let mut group = c.benchmark_group(&group_name);
    group.throughput(Throughput::Elements(count as u64));
    if let Some(n) = sample_size {
        group.sample_size(n);
    }
    if sample_size.is_some() && matches!(size, DatasetSize::Medium | DatasetSize::Large) {
        group.warm_up_time(Duration::from_secs(3));
    }

    group.bench_function(BenchmarkId::new("verify_full", count), |b| {
        b.iter_custom(|iters| {
            let verifier = keypair.verifier().expect("verifier creation must not fail");
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let start = Instant::now();
                for (msg, sig) in dataset.iter().zip(full_sigs.iter()) {
                    let ok = verifier
                        .verify(msg.as_slice(), sig, None, false)
                        .expect("verification must not error");
                    black_box(ok);
                }
                total += start.elapsed();
            }

            total
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Short, stable label for a [`DatasetSize`] used in bench group names.
const fn size_label(size: DatasetSize) -> &'static str {
    match size {
        DatasetSize::Small => "small",
        DatasetSize::Medium => "medium",
        DatasetSize::Large => "large",
    }
}

// ===========================================================================
// Setup-separation probe — see crates/climb-bench/tests/probes.rs
// ===========================================================================
