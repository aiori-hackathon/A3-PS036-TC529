//! Criterion signing benchmarks: MTL-amortized vs individual signing.
//!
//! Compares `sign_batch(&[N messages])` (one shared ladder, one underlying OQS
//! signature) against `N × sign_batch(&[1 message])` (fresh ladder, N underlying
//! OQS signatures).  This is exactly the with-MTL / without-MTL comparison.
//!
//! # What each condition measures
//!
//! | Condition | One measured iteration | Messages per iteration |
//! |---|---|---|
//! | With MTL | `sign_batch(&[N messages])` — the whole batch under one ladder | `N` |
//! | Without MTL | `sign_batch(&[1 message])` — one complete MTL signature | `1` |
//!
//! The non-amortised baseline is measured as **one complete MTL signature per
//! iteration at every dataset size**.  That is the whole point of the
//! baseline: signing `N` messages without amortisation costs `N` independent
//! MTL signatures, so one signature is the unit of work and its cost must not
//! depend on how many messages the dataset holds.  Measuring a whole batch
//! per iteration instead would make the reported per-message cost depend on
//! the batch size, and a batch-shaped row could not be compared against a
//! per-message row at all.
//!
//! The two conditions therefore differ in *how much work one iteration
//! covers*, never in *which operation* is timed.  The shared invariant for
//! every consumer of the results artifact is:
//!
//! > `mean_ns` is the total time of one measured iteration, and that
//! > iteration signed `messages` messages.
//!
//! So the per-message cost is `mean_ns / messages` and the cost of signing a
//! batch of `N` messages is `mean_ns × N / messages`.
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
//! The baseline signs the *first* message of that stream, so the byte string
//! it signs is identical at every dataset size.
//!
//! # Sample-size reasoning
//!
//! On this benchmark machine (no AVX2), one SLH-DSA-128s signature takes
//! ~0.5–1.4 s depending on the machine's clock state.  A whole batch under one
//! ladder costs roughly the same, so the amortised condition is nearly
//! independent of the dataset size while the non-amortised baseline costs one
//! signature per message.
//!
//! | Benchmark               | Iter cost (est.) | Sample size | Rationale |
//! |-------------------------|------------------|-------------|-----------|
//! | With MTL (every size)   | ~0.5 s – 4 s     | 100         | One ladder per iteration; 100 samples keep the mean dominated by the steady state. |
//! | Without MTL (every size)| ~0.5 s – 1.4 s   | 100         | One signature per iteration; identical cost at every size by construction. |
//! | Plain SLH-DSA          | ~0.5 s           | 100         | Configuration A: one full signature per message, no ladder. |
//!
//! Both conditions use the same sample count so the two sides of every
//! ratio are measured with the same statistical weight.
//!
//! # Warm-up
//!
//! A laptop that is not on mains power spends its first seconds of load at a
//! reduced clock, and the first samples of a run are then up to three times
//! slower than the steady state.  The baseline's warm-up is therefore set
//! explicitly and generously: Criterion doubles the iteration count per
//! warm-up round, so a ten-second warm-up runs roughly thirty signatures
//! before the first sample is recorded.  The with-MTL and plain groups use
//! Criterion's default warm-up.
//!
//! # Running
//!
//! ```bash
//! # The whole signing bench (about 20 minutes on this machine):
//! cargo bench -p climb-bench --bench signing
//!
//! # Just the non-amortised baseline (about 6 minutes):
//! cargo bench -p climb-bench --bench signing -- signing_small_without_mtl
//!
//! # Save baseline for CI / regression tracking:
//! cargo bench -p climb-bench -- --save-baseline batch2-signing
//! ```

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use climb_dns::{generate_dataset, generate_dataset_count, DatasetSize};
use climb_mtl::{MtlKeyPair, PlainSlhDsa};

/// Fixed seed for benchmark reproducibility.
///
/// ASCII `CLIMB_V5` encoded as a `u64`.  `generate_dataset(size, BENCH_SEED)`
/// always produces the same byte-identical dataset, making cross-run and
/// baseline comparisons valid.
pub const BENCH_SEED: u64 = 0x434C494D425F5635;

/// Number of measured samples per benchmark.
///
/// Both signing conditions use the same count so neither side of a speed-up
/// ratio carries more statistical weight than the other.  Criterion's default
/// is 100; it is named here so the choice is visible in the source rather
/// than inherited.
const SAMPLE_SIZE: usize = 100;

/// Warm-up of the non-amortised baseline.
///
/// Ten seconds of warm-up is roughly thirty signatures on this machine, which
/// is what it takes for a laptop clock to reach its steady state.  Without it
/// the first samples of a run are inflated, and a mean that mixes the ramp
/// with the steady state overstates the baseline by up to threefold.
const BASELINE_WARM_UP: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn bench_signing(c: &mut Criterion) {
    let keypair = MtlKeyPair::generate().expect("benchmark key generation must succeed");
    let signer = keypair.signer();

    bench_with_mtl(c, &signer, DatasetSize::Small);
    bench_without_mtl(c, &signer, DatasetSize::Small);

    bench_with_mtl(c, &signer, DatasetSize::OneK);
    bench_without_mtl(c, &signer, DatasetSize::OneK);

    bench_with_mtl(c, &signer, DatasetSize::Medium);
    bench_without_mtl(c, &signer, DatasetSize::Medium);

    bench_with_mtl(c, &signer, DatasetSize::Large);
    bench_without_mtl(c, &signer, DatasetSize::Large);

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
/// message.  One iteration signs the whole batch, so the row reports `N`
/// messages.
fn bench_with_mtl(c: &mut Criterion, signer: &climb_mtl::MtlSigner, size: DatasetSize) {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let count = size.count();

    let mut group = c.benchmark_group(format!("signing_{}_with_mtl", label));
    group.throughput(Throughput::Elements(count as u64));
    group.sample_size(SAMPLE_SIZE);

    group.bench_with_input(
        criterion::BenchmarkId::new("sign_batch", count),
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
// Without MTL — one complete MTL signature per iteration
// ---------------------------------------------------------------------------

/// Benchmark one complete MTL signature — the non-amortized baseline.
///
/// The call builds a fresh context, builds a new ladder, pays for one full
/// OQS signature, and emits one full signature (~8 KB).  No amortization.
///
/// The same operation is measured at every dataset size: one signature over
/// one message.  The dataset size only selects the group name, so the
/// per-message cost of the baseline is a property of the algorithm and not of
/// the batch — which is exactly the property the results artifact checks.
fn bench_without_mtl(c: &mut Criterion, signer: &climb_mtl::MtlSigner, size: DatasetSize) {
    let label = size_label(size);

    // The first message of the seeded stream: the same bytes at every size.
    let mut dataset = generate_dataset_count(1, BENCH_SEED);
    let message = dataset
        .pop()
        .expect("seeded dataset of one message is non-empty");
    let message: &[u8] = &message;

    let mut group = c.benchmark_group(format!("signing_{}_without_mtl", label));
    // One message per iteration.
    group.throughput(Throughput::Elements(1));
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(BASELINE_WARM_UP);

    group.bench_function("sign_one", |b| {
        b.iter(|| {
            black_box(
                signer
                    .sign_batch(black_box(&[message]))
                    .expect("individual sign_batch must not fail"),
            );
        })
    });

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
    group.sample_size(SAMPLE_SIZE);
    group.bench_function(criterion::BenchmarkId::from_parameter("sign"), |b| {
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
