//! Criterion signing benchmarks: MTL-amortized vs individual signing.
//!
//! Compares `sign_batch(&[N messages])` (one shared ladder, one underlying OQS
//! signature) against `N × sign_batch(&[1 message])` (fresh ladder, N underlying
//! OQS signatures).  This is exactly the with-MTL / without-MTL comparison.
//!
//! # Benchmark seed
//!
//! All datasets are generated from a fixed seed so every run hits the same byte
//! stream.  The value is the ASCII encoding of `CLIMB_V5`:
//!
//! ```text
//! 0x434C494D425F5635
//! ```
//!
//! # Sample-size reasoning
//!
//! On this benchmark machine (no AVX2), one SLH-DSA-128s signature takes
//! ~740 ms.  This drives the without-MTL iteration cost and the sample-size
//! choices below.  With AVX2, per-signature cost drops to ~5–10 ms and defaults
//! would suffice — the config is deliberately conservative for the slower
//! hardware it was first measured on.
//!
//! | Benchmark               | Iter cost (est.) | Sample size | Rationale |
//! |-------------------------|------------------|-------------|-----------|
//! | Small with-MTL          | ~0.7 s           | default     | Fast.    |
//! | Small without-MTL       | ~74 s            | 10          | Criterion minimum; 10 × 74 s ≈ 12 min. |
//! | Medium with-MTL         | ~seconds         | default     | Fast-ish.   |
//! | Medium without-MTL      | ~2 h             | 10          | Criterion minimum; 10 × 2 h ≈ 20 h total.  **Skippable** — cross‑validate with Small per‑message cost. |
//! | Large with-MTL          | ~seconds–minutes | default     | Fast-ish.   |
//! | Large without-MTL       | ~205 h           | 10          | Criterion minimum; 10 × 205 h ≈ 85 days.  Warmup disabled.  **Always skip** — use analytical estimate. |
//!
//! # Large (1M-message) without-MTL — wall-clock trade-off
//!
//! The without-MTL path for Large does 1,000,000 individual `sign_batch` calls,
//! each paying for one full SLH-DSA-128s signature.  At ~740 ms/sig on this
//! machine, a single iteration takes ~205 hours.  Criterion is configured with
//! `sample_size(1)` and a 3-second `warm_up_time` (shorter than one iteration,
//! so warmup is skipped).  The benchmark is present for completeness but should
//! be skipped in practice — the per-message cost measured on Small / Medium is a
//! more useful estimate.
//!
//! **Practical recommendation:** Derive Large without-MTL analytically from the
//! per-message cost measured on Small/Medium.  The benchmark exists for future
//! hardware validation.
//!
//! # Running
//!
//! ```bash
//! # Small + Medium only (practical default):
//! cargo bench -p climb-bench -- signing_small signing_medium
//!
//! # Save baseline for CI / regression tracking:
//! cargo bench -p climb-bench -- --save-baseline batch2-signing
//! ```

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::{MtlKeyPair, PlainSlhDsa};

/// Fixed seed for benchmark reproducibility.
///
/// ASCII `CLIMB_V5` encoded as a `u64`.  `generate_dataset(size, BENCH_SEED)`
/// always produces the same byte-identical dataset, making cross-run and
/// baseline comparisons valid.
pub const BENCH_SEED: u64 = 0x434C494D425F5635;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn bench_signing(c: &mut Criterion) {
    let keypair = MtlKeyPair::generate().expect("benchmark key generation must succeed");
    let signer = keypair.signer();

    bench_with_mtl(c, &signer, DatasetSize::Small, None);
    bench_without_mtl(c, &signer, DatasetSize::Small, Some(10));

    bench_with_mtl(c, &signer, DatasetSize::OneK, None);
    bench_without_mtl(c, &signer, DatasetSize::OneK, Some(10));

    bench_with_mtl(c, &signer, DatasetSize::Medium, None);
    bench_without_mtl(c, &signer, DatasetSize::Medium, Some(10));

    bench_with_mtl(c, &signer, DatasetSize::Large, None);
    bench_without_mtl(c, &signer, DatasetSize::Large, Some(10));

    bench_plain_slh_dsa(c, DatasetSize::Small);
    bench_plain_slh_dsa(c, DatasetSize::Medium);
}

criterion_group!(benches, bench_signing);
criterion_main!(benches);

// ---------------------------------------------------------------------------
// With MTL — one sign_batch(N)
// ---------------------------------------------------------------------------

/// Benchmark `sign_batch(&[N messages])` — the amortized path.
///
/// One shared ladder, one underlying OQS signature, one condensed signature per
/// message.  `sample_size` uses criterion's default when `None`.
fn bench_with_mtl(
    c: &mut Criterion,
    signer: &climb_mtl::MtlSigner,
    size: DatasetSize,
    sample_size: Option<usize>,
) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let count = size.count();

    let mut group = c.benchmark_group(format!("signing_{}_with_mtl", label));
    group.throughput(Throughput::Elements(count as u64));
    if let Some(n) = sample_size {
        group.sample_size(n);
    }

    group.bench_with_input(
        BenchmarkId::new("sign_batch", count),
        &messages,
        |b, msgs| {
            b.iter(|| {
                signer
                    .sign_batch(msgs)
                    .expect("MTL sign_batch must not fail in benchmark");
            })
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Without MTL — N × sign_batch(1)
// ---------------------------------------------------------------------------

/// Benchmark `N × sign_batch(&[1 message])` — the non-amortized baseline.
///
/// Each call creates a fresh context, builds a new ladder, pays for one full
/// OQS signature, and emits one full signature (~8 KB).  No amortization.
///
/// `sample_size` is **required** because the per-iteration cost is `N ×
/// SLH-DSA-128s signing time`, which can be multiple hours for Medium/Large.
fn bench_without_mtl(
    c: &mut Criterion,
    signer: &climb_mtl::MtlSigner,
    size: DatasetSize,
    sample_size: Option<usize>,
) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let count = size.count();

    let mut group = c.benchmark_group(format!("signing_{}_without_mtl", label));
    group.throughput(Throughput::Elements(count as u64));

    let sample_size = sample_size.unwrap_or(10);
    group.sample_size(sample_size);

    // Without MTL at large sizes can take hours. Skip warmup.
    if matches!(
        size,
        DatasetSize::OneK | DatasetSize::Medium | DatasetSize::Large
    ) {
        group.warm_up_time(Duration::from_secs(3));
    }

    group.bench_with_input(
        BenchmarkId::new("sign_batch_x1", count),
        &messages,
        |b, msgs| {
            b.iter(|| {
                for msg in msgs.iter() {
                    black_box(
                        signer
                            .sign_batch(&[msg])
                            .expect("individual sign_batch must not fail"),
                    );
                }
            })
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Plain SLH-DSA (configuration A) — one full signature per message, no ladder
// ---------------------------------------------------------------------------

/// Benchmark one plain SLH-DSA signature (configuration A).
///
/// Configuration A signs each RRset with its own full SLH-DSA signature and no
/// ladder, so its per-message cost is independent of the batch size. One message
/// per iteration is measured at Small and Medium, giving `k = 2` samples of that
/// per-message cost; a batch of any size is then obtained with the project's
/// existing `n x per-message mean` extrapolation. Large is not run — at
/// `1e6 x ~0.74 s` a single iteration would take days.
fn bench_plain_slh_dsa(c: &mut Criterion, size: DatasetSize) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let message = dataset
        .first()
        .expect("seeded dataset is non-empty")
        .clone();

    let keypair = PlainSlhDsa::generate().expect("plain SLH-DSA keypair");

    let mut group = c.benchmark_group(format!("signing_{}_plain_slh_dsa", label));
    group.throughput(Throughput::Elements(1));
    group.bench_function(BenchmarkId::from_parameter("sign"), |b| {
        b.iter(|| {
            black_box(
                keypair
                    .sign(black_box(&message))
                    .expect("plain sign must not fail"),
            )
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
        DatasetSize::OneK => "onek",
        DatasetSize::Medium => "medium",
        DatasetSize::Large => "large",
    }
}
