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
    after_kb: u64,
    hwm_kb: u64,
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

    // Generate dataset and keypair outside the delta window
    let dataset = generate_dataset(size, seed);
    let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
    let keypair = match MtlKeyPair::generate() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("keypair generation failed: {:?}", e);
            process::exit(2);
        }
    };

    let baseline = match read_status_self() {
        Ok(s) => match parse_status_kb(&s) {
            Some((r, _h)) => r,
            None => {
                eprintln!("failed to parse /proc/self/status");
                process::exit(3);
            }
        },
        Err(e) => {
            eprintln!("failed to read /proc/self/status: {}", e);
            process::exit(3);
        }
    };

    // Sign
    let signer = keypair.signer();
    let _output = match signer.sign_batch(&messages) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sign failed: {:?}", e);
            process::exit(4);
        }
    };

    let after = match read_status_self() {
        Ok(s) => match parse_status_kb(&s) {
            Some((r, h)) => (r, h),
            None => {
                eprintln!("failed to parse /proc/self/status after");
                process::exit(3);
            }
        },
        Err(e) => {
            eprintln!("failed to read /proc/self/status: {}", e);
            process::exit(3);
        }
    };

    let res = ProbeResult {
        size: size_str.to_ascii_lowercase(),
        baseline_kb: baseline,
        after_kb: after.0,
        hwm_kb: after.1,
    };
    let json = serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string());
    println!("{}", json);
}
