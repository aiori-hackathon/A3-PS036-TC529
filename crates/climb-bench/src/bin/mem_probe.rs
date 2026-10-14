use climb_dns::{generate_dataset, DatasetSize};
use climb_mtl::MtlKeyPair;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::process;

#[derive(Debug, serde::Serialize)]
struct ProbeResult {
    size: String,
    baseline_kb: u64,
    after_dataset_kb: u64,
    after_keypair_kb: u64,
    after_kb: u64,
    hwm_kb: u64,
    key_image_bytes: u64,
}

fn parse_size(s: &str) -> Option<DatasetSize> {
    match s.to_ascii_lowercase().as_str() {
        "small" => Some(DatasetSize::Small),
        "medium" => Some(DatasetSize::Medium),
        "large" => Some(DatasetSize::Large),
        "onek" | "one_k" => Some(DatasetSize::OneK),
        _ => None,
    }
}

fn parse_status_kb(text: &str) -> Option<(u64, u64)> {
    let mut rss: Option<u64> = None;
    let mut hwm: Option<u64> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with("VmRSS:") {
            if let Some(kb) = extract_kb(line) {
                rss = Some(kb);
            }
        } else if line.starts_with("VmHWM:") {
            if let Some(kb) = extract_kb(line) {
                hwm = Some(kb);
            }
        }
    }
    match (rss, hwm) {
        (Some(r), Some(h)) => Some((r, h)),
        _ => None,
    }
}

fn extract_kb(line: &str) -> Option<u64> {
    let mut parts = line.split_whitespace();
    let _key = parts.next()?;
    let num_str = parts.next()?;
    let unit = parts.next().unwrap_or("");
    let num: u64 = num_str.parse().ok()?;
    match unit.to_ascii_lowercase().as_str() {
        "kb" | "kib" => Some(num),
        "mb" | "mib" => Some(num * 1024),
        "gb" | "gib" => Some(num * 1024 * 1024),
        _ => Some(num),
    }
}

fn read_status_self() -> Result<String, io::Error> {
    let mut f = fs::File::open("/proc/self/status")?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    Ok(buf)
}

/// Sample RSS and peak RSS (kilobytes) from `/proc/self/status`, exiting the
/// process on failure: a sample is the measurement, so a missing reading is a
/// hard error rather than a silent zero.
fn sample_rss() -> (u64, u64) {
    match read_status_self() {
        Ok(s) => match parse_status_kb(&s) {
            Some(pair) => pair,
            None => {
                eprintln!("failed to parse /proc/self/status");
                process::exit(3);
            }
        },
        Err(e) => {
            eprintln!("failed to read /proc/self/status: {}", e);
            process::exit(3);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: mem_probe <size> <seed>");
        process::exit(1);
    }
    let size_str = &args[1];
    let seed_str = &args[2];

    let size = match parse_size(size_str) {
        Some(s) => s,
        None => {
            eprintln!("invalid size: {}", size_str);
            process::exit(1);
        }
    };
    let seed: u64 = match seed_str.parse() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("invalid seed: {}", seed_str);
            process::exit(1);
        }
    };

    // Four RSS sample points, each measured from process start so every stage
    // delta (dataset, keypair, sign) is attributable. `hwm` is the running peak.
    let (baseline, _) = sample_rss();

    // Stage 1: after dataset generation.
    let dataset = generate_dataset(size, seed);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let (after_dataset, _) = sample_rss();

    // Stage 2: after keypair generation.
    let keypair = match MtlKeyPair::generate() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("keypair generation failed: {:?}", e);
            process::exit(2);
        }
    };
    let (after_keypair, _) = sample_rss();

    // Stage 3: after sign_batch returns (its output is still alive).
    let signer = keypair.signer();
    let _output = match signer.sign_batch(&messages) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sign failed: {:?}", e);
            process::exit(4);
        }
    };
    let (after, hwm) = sample_rss();

    let res = ProbeResult {
        size: size_str.to_ascii_lowercase(),
        baseline_kb: baseline,
        after_dataset_kb: after_dataset,
        after_keypair_kb: after_keypair,
        after_kb: after,
        hwm_kb: hwm,
        key_image_bytes: keypair.key_image_len() as u64,
    };
    let json = serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string());
    println!("{}", json);
}
