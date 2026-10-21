//! Signing primitives measured in one process, interleaved ABAB with warm-up.

use std::fs;
use std::path::Path;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::Serialize;

use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::{MtlKeyPair, PlainSlhDsa};

const BENCH_SEED: u64 = 0x434C494D425F5635;

fn read_cur_freq(cpu: u32) -> Option<u64> {
    let p = format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq");
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

#[derive(Serialize)]
struct FreqRecord {
    key: String,
    cpu: u32,
    before: Option<u64>,
    after: Option<u64>,
    min_khz: Option<u64>,
    max_khz: Option<u64>,
}

fn bench_primitives(c: &mut Criterion) {
    let out_dir = Path::new("target/climb-primitives");
    fs::create_dir_all(out_dir).expect("create primitives output dir");

    let pin_cpu = std::env::var("CLIMB_PIN_CPU")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .or_else(|| {
            if let Ok(s) = fs::read_to_string("/proc/self/status") {
                for line in s.lines() {
                    if let Some(rest) = line.strip_prefix("Cpus_allowed_list:") {
                        let rest = rest.trim();
                        if let Some(first) = rest.split(&[',', '-'][..]).next() {
                            if let Ok(v) = first.trim().parse::<u32>() {
                                return Some(v);
                            }
                        }
                    }
                }
            }
            None
        });

    // Sample count per group: repeated-stability runs trade per-run sample
    // count for turnaround, so it is overridable. Criterion enforces a floor
    // of 10 samples.
    let samples: usize = std::env::var("CLIMB_PRIMITIVE_SAMPLES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .max(10);

    // Which per-message-count groups to run (comma list of N). Empty = all.
    let sizes_env = std::env::var("CLIMB_PRIMITIVES_SIZES").unwrap_or_default();
    let want_n = |n: usize| -> bool {
        if sizes_env.is_empty() {
            return true;
        }
        sizes_env
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .any(|x| x == n)
    };

    let keypair = MtlKeyPair::generate().expect("mtl keypair");
    let signer = keypair.signer();
    let plain = PlainSlhDsa::generate().expect("plain keypair");

    let small_ds = generate_dataset(DatasetSize::Small, BENCH_SEED);
    let onek_ds = generate_dataset(DatasetSize::OneK, BENCH_SEED);
    let medium_ds = generate_dataset(DatasetSize::Medium, BENCH_SEED);

    let msg_s = small_ds[0].as_slice();

    // t_sig
    {
        let mut rec = FreqRecord {
            key: "t_sig".to_string(),
            cpu: pin_cpu.unwrap_or(0),
            before: pin_cpu.and_then(read_cur_freq),
            after: None,
            min_khz: None,
            max_khz: None,
        };
        let mut g = c.benchmark_group("primitives_t_sig");
        g.warm_up_time(Duration::from_secs(1));
        g.sample_size(samples);
        g.bench_function("plain_slh_dsa", |b| {
            b.iter(|| {
                black_box(plain.sign(black_box(msg_s)).expect("plain sign"));
            })
        });
        g.finish();
        rec.after = pin_cpu.and_then(read_cur_freq);
        rec.min_khz = match (rec.before, rec.after) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        rec.max_khz = match (rec.before, rec.after) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let _ = fs::write(
            out_dir.join("freq_t_sig.json"),
            serde_json::to_string_pretty(&rec).unwrap(),
        );
    }

    // t_mtl_single
    {
        let mut rec = FreqRecord {
            key: "t_mtl_single".to_string(),
            cpu: pin_cpu.unwrap_or(0),
            before: pin_cpu.and_then(read_cur_freq),
            after: None,
            min_khz: None,
            max_khz: None,
        };
        let mut g = c.benchmark_group("primitives_t_mtl_single");
        g.warm_up_time(Duration::from_secs(1));
        g.sample_size(samples);
        g.bench_function("one_message", |b| {
            b.iter(|| {
                black_box(
                    signer
                        .sign_batch(black_box(&[msg_s]))
                        .expect("mtl sign one"),
                );
            })
        });
        g.finish();
        rec.after = pin_cpu.and_then(read_cur_freq);
        rec.min_khz = match (rec.before, rec.after) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        rec.max_khz = match (rec.before, rec.after) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let _ = fs::write(
            out_dir.join("freq_t_mtl_single.json"),
            serde_json::to_string_pretty(&rec).unwrap(),
        );
    }

    // t_tree(N) and t_condensed(N) - interleaved ABAB pattern across sizes? "interleaved ABAB with warm-up"
    // Do in order for each N: t_tree(N), t_condensed(N), t_tree(N), t_condensed(N) or ABAB structure
    for (n, ds) in [(100usize, &small_ds), (1000, &onek_ds), (10000, &medium_ds)] {
        if !want_n(n) {
            continue;
        }
        let messages: Vec<&[u8]> = ds.iter().map(|m| m.as_slice()).collect();

        // t_tree(N)
        {
            let mut rec = FreqRecord {
                key: format!("t_tree_{n}"),
                cpu: pin_cpu.unwrap_or(0),
                before: pin_cpu.and_then(read_cur_freq),
                after: None,
                min_khz: None,
                max_khz: None,
            };
            let mut g = c.benchmark_group(format!("primitives_t_tree_{n}"));
            g.warm_up_time(Duration::from_secs(1));
            g.sample_size(samples);
            g.bench_with_input(
                BenchmarkId::new("tree_excl_sig", n),
                &messages,
                |b, msgs| {
                    b.iter(|| {
                        black_box(signer.sign_batch(black_box(msgs)).expect("mtl sign batch"));
                    })
                },
            );
            g.finish();
            rec.after = pin_cpu.and_then(read_cur_freq);
            rec.min_khz = match (rec.before, rec.after) {
                (Some(a), Some(b)) => Some(a.min(b)),
                _ => rec.before.or(rec.after),
            };
            rec.max_khz = match (rec.before, rec.after) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => rec.before.or(rec.after),
            };
            let _ = fs::write(
                out_dir.join(format!("freq_t_tree_{n}.json")),
                serde_json::to_string_pretty(&rec).unwrap(),
            );
        }

        // t_condensed(N)
        {
            let mut rec = FreqRecord {
                key: format!("t_condensed_{n}"),
                cpu: pin_cpu.unwrap_or(0),
                before: pin_cpu.and_then(read_cur_freq),
                after: None,
                min_khz: None,
                max_khz: None,
            };
            let mut g = c.benchmark_group(format!("primitives_t_condensed_{n}"));
            g.warm_up_time(Duration::from_secs(1));
            g.sample_size(samples);
            g.bench_with_input(BenchmarkId::new("condensed", n), &messages, |b, msgs| {
                b.iter(|| {
                    let out = signer.sign_batch(black_box(msgs)).expect("mtl sign batch");
                    for i in 0..out.len() {
                        black_box(out.condensed_signature(i).expect("condensed"));
                    }
                })
            });
            g.finish();
            rec.after = pin_cpu.and_then(read_cur_freq);
            rec.min_khz = match (rec.before, rec.after) {
                (Some(a), Some(b)) => Some(a.min(b)),
                _ => rec.before.or(rec.after),
            };
            rec.max_khz = match (rec.before, rec.after) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => rec.before.or(rec.after),
            };
            let _ = fs::write(
                out_dir.join(format!("freq_t_condensed_{n}.json")),
                serde_json::to_string_pretty(&rec).unwrap(),
            );
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_secs(1));
    targets = bench_primitives
}
criterion_main!(benches);
