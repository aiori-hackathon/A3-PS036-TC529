//! Criterion classical signature baselines (context only, not PQC).
//!
//! Ed25519, ECDSA P-256 and RSA-2048 sign and verify over the seeded Small
//! and Medium datasets. These are classical algorithms: they are *not*
//! quantum-resistant, and are measured only to put the post-quantum
//! configurations in context. Signature lengths are read back from produced
//! signatures and recorded beside Criterion's own output for the
//! consolidation step to consume — no length is typed as a constant here.
//!
//! Dataset seeds are reproducible (`CLIMB_V7`); the Ed25519 and P-256 key
//! seeds are fixed bytes, and the RSA-2048 keypair is generated once per run
//! before any timing is taken (signature lengths do not depend on the key
//! for any of the three schemes).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ed25519_dalek::{
    // `Signer`/`Verifier` come from the shared `signature` crate, which the
    // P-256 and RSA crates re-export at the same version — one import covers
    // all three algorithms.
    Signer as _,
    SigningKey as Ed25519SigningKey,
    Verifier as _,
    VerifyingKey as Ed25519VerifyingKey,
};
use p256::ecdsa::{
    Signature as P256Signature, SigningKey as P256SigningKey, VerifyingKey as P256VerifyingKey,
};
use rand_core::OsRng;
use rsa::{
    pkcs1v15::{SigningKey as RsaSigningKey, VerifyingKey as RsaVerifyingKey},
    signature::{Keypair as _, SignatureEncoding as _},
};
use sha2::Sha256;

use climb_dns::{generate_dataset, DatasetSize};

/// ASCII `CLIMB_V7` encoded as a `u64`. Every algorithm signs the same
/// seeded message set so the timings are comparable across groups.
pub const BENCH_SEED_CLASSICAL: u64 = 0x434C494D425F5637;

/// Fixed 32-byte Ed25519 signing-key seed. Benchmark key material only — it
/// never protects anything outside this bench.
const ED25519_KEY_SEED: [u8; 32] = *b"CLIMB_V7_ed25519_bench_key_seed!";

/// Fixed 32-byte ECDSA P-256 scalar seed (ASCII bytes are far below the
/// curve order, so this decodes to a valid scalar). Benchmark key material.
const P256_KEY_SEED: [u8; 32] = *b"CLIMB_V7_p256_bench_key_seed_007";

fn size_label(size: DatasetSize) -> &'static str {
    match size {
        DatasetSize::Small => "small",
        DatasetSize::OneK => "onek",
        DatasetSize::Medium => "medium",
        DatasetSize::Large => "large",
    }
}

fn bench_classical(c: &mut Criterion) {
    let ed25519 = bench_ed25519(c, DatasetSize::Small);
    let _ = bench_ed25519(c, DatasetSize::Medium);
    let ecdsa_p256 = bench_ecdsa_p256(c, DatasetSize::Small);
    let _ = bench_ecdsa_p256(c, DatasetSize::Medium);
    let rsa2048 = bench_rsa2048(c, DatasetSize::Small);
    let _ = bench_rsa2048(c, DatasetSize::Medium);
    write_signature_lengths(ed25519, ecdsa_p256, rsa2048);
}

fn bench_ed25519(c: &mut Criterion, size: DatasetSize) -> usize {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED_CLASSICAL);
    let signing_key = Ed25519SigningKey::from_bytes(&ED25519_KEY_SEED);
    let verifying_key = Ed25519VerifyingKey::from(&signing_key);

    // Correctness gate before any timing is taken: a fresh signature must
    // verify, and one flipped signature byte must not.
    let probe = signing_key.sign(&dataset[0]);
    assert!(
        verifying_key.verify(&dataset[0], &probe).is_ok(),
        "ed25519: a fresh signature must round-trip"
    );
    let mut tampered = probe.to_bytes();
    tampered[0] ^= 0x01;
    let rejected = verifying_key
        .verify(
            &dataset[0],
            &ed25519_dalek::Signature::from_bytes(&tampered),
        )
        .is_err();
    assert!(rejected, "ed25519: a tampered signature must not verify");

    let mut group = c.benchmark_group(format!("classical_ed25519_{label}"));
    group.throughput(Throughput::Elements(1));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(3));

    group.bench_function(BenchmarkId::from_parameter("sign"), |b| {
        let mut index = 0usize;
        b.iter(|| {
            let message = &dataset[index % dataset.len()];
            index += 1;
            black_box(signing_key.sign(black_box(message.as_slice())));
        })
    });

    let signature = signing_key.sign(&dataset[0]);
    group.bench_function(BenchmarkId::from_parameter("verify"), |b| {
        b.iter(|| {
            let _ = black_box(
                verifying_key.verify(black_box(dataset[0].as_slice()), black_box(&signature)),
            );
        })
    });
    group.finish();
    signature.to_bytes().len()
}

fn bench_ecdsa_p256(c: &mut Criterion, size: DatasetSize) -> usize {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED_CLASSICAL);
    let signing_key = P256SigningKey::from_slice(&P256_KEY_SEED).expect("P-256 seed scalar");
    let verifying_key = P256VerifyingKey::from(&signing_key);

    // Correctness gate before any timing is taken: a fresh signature must
    // verify, and one flipped signature byte must not.
    let probe: P256Signature = signing_key.sign(&dataset[0]);
    assert!(
        verifying_key.verify(&dataset[0], &probe).is_ok(),
        "ecdsa p256: a fresh signature must round-trip"
    );
    let mut tampered = probe.to_bytes();
    tampered[63] ^= 0x01;
    let rejected = match P256Signature::from_slice(&tampered) {
        Ok(tampered) => verifying_key.verify(&dataset[0], &tampered).is_err(),
        // A decode failure is itself a rejection.
        Err(_) => true,
    };
    assert!(rejected, "ecdsa p256: a tampered signature must not verify");

    let mut group = c.benchmark_group(format!("classical_ecdsa_p256_{label}"));
    group.throughput(Throughput::Elements(1));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(3));

    group.bench_function(BenchmarkId::from_parameter("sign"), |b| {
        let mut index = 0usize;
        b.iter(|| {
            let message = &dataset[index % dataset.len()];
            index += 1;
            let signature: P256Signature = signing_key.sign(black_box(message.as_slice()));
            black_box(signature);
        })
    });

    let signature: P256Signature = signing_key.sign(&dataset[0]);
    group.bench_function(BenchmarkId::from_parameter("verify"), |b| {
        b.iter(|| {
            let _ = black_box(
                verifying_key.verify(black_box(dataset[0].as_slice()), black_box(&signature)),
            );
        })
    });
    group.finish();
    signature.to_bytes().len()
}

fn bench_rsa2048(c: &mut Criterion, size: DatasetSize) -> usize {
    let label = size_label(size);
    let dataset = generate_dataset(size, BENCH_SEED_CLASSICAL);
    let signing_key =
        RsaSigningKey::<Sha256>::random(&mut OsRng, 2048).expect("RSA-2048 key generation");
    let verifying_key: RsaVerifyingKey<Sha256> = signing_key.verifying_key();

    // Correctness gate before any timing is taken: a fresh signature must
    // verify, and one flipped signature byte must not.
    let probe = signing_key.sign(&dataset[0]);
    assert!(
        verifying_key.verify(&dataset[0], &probe).is_ok(),
        "rsa2048: a fresh signature must round-trip"
    );
    let mut tampered = probe.to_bytes();
    tampered[255] ^= 0x01;
    let rejected = match rsa::pkcs1v15::Signature::try_from(tampered.as_ref()) {
        Ok(tampered) => verifying_key.verify(&dataset[0], &tampered).is_err(),
        // A decode failure is itself a rejection.
        Err(_) => true,
    };
    assert!(rejected, "rsa2048: a tampered signature must not verify");

    let mut group = c.benchmark_group(format!("classical_rsa2048_{label}"));
    group.throughput(Throughput::Elements(1));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(3));

    group.bench_function(BenchmarkId::from_parameter("sign"), |b| {
        let mut index = 0usize;
        b.iter(|| {
            let message = &dataset[index % dataset.len()];
            index += 1;
            black_box(signing_key.sign(black_box(message.as_slice())));
        })
    });

    let signature = signing_key.sign(&dataset[0]);
    group.bench_function(BenchmarkId::from_parameter("verify"), |b| {
        b.iter(|| {
            let _ = black_box(
                verifying_key.verify(black_box(dataset[0].as_slice()), black_box(&signature)),
            );
        })
    });
    group.finish();
    signature.to_bytes().len()
}

/// Record the measured signature lengths as JSON so downstream tooling reads
/// them instead of typing them. One document, one field per algorithm.
fn write_signature_lengths(ed25519: usize, ecdsa_p256: usize, rsa2048: usize) {
    let path = signature_lengths_path();
    let document = serde_json::json!({
        "ed25519": ed25519,
        "ecdsa_p256": ecdsa_p256,
        "rsa2048": rsa2048,
    });
    let encoded = match serde_json::to_vec_pretty(&document) {
        Ok(encoded) => encoded,
        Err(e) => {
            eprintln!("warning: could not encode classical signature lengths: {e}");
            return;
        }
    };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("warning: could not create {}: {e}", dir.display());
            return;
        }
    }
    match std::fs::write(&path, &encoded) {
        Ok(()) => eprintln!("recorded classical signature lengths in {}", path.display()),
        Err(e) => eprintln!(
            "warning: could not write classical signature lengths to {}: {e}",
            path.display()
        ),
    }
}

/// Where the measured signature lengths land: the cargo target directory,
/// resolved the same way Criterion resolves its own output directory, so the
/// document sits next to `target/criterion`.
fn signature_lengths_path() -> PathBuf {
    target_directory().join("climb-classical-signatures.json")
}

fn target_directory() -> PathBuf {
    if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    if let Ok(output) = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .output()
    {
        if let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            if let Some(dir) = metadata.get("target_directory").and_then(|v| v.as_str()) {
                return PathBuf::from(dir);
            }
        }
    }
    PathBuf::from("target")
}

criterion_group!(benches, bench_classical);
criterion_main!(benches);
