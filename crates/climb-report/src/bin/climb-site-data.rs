//! Generate the website's build-time data documents from the recorded
//! artifacts.
//!
//! The binary is native-free: it reads JSON and text files, computes the six
//! documents under `site/src/data/generated/` and never links the measurement
//! stack. `cargo xtask site-data` calls the same library function.

use std::path::PathBuf;
use std::process::ExitCode;

use climb_report::site_data::{write, DEFAULT_OUT_DIR};

fn main() -> ExitCode {
    let mut root = PathBuf::from(".");
    let mut out = PathBuf::from(DEFAULT_OUT_DIR);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => match args.next() {
                Some(value) => root = PathBuf::from(value),
                None => {
                    eprintln!("climb-site-data: --root needs a value");
                    return ExitCode::FAILURE;
                }
            },
            "--out" => match args.next() {
                Some(value) => out = PathBuf::from(value),
                None => {
                    eprintln!("climb-site-data: --out needs a value");
                    return ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                println!("usage: climb-site-data [--root DIR] [--out DIR]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("climb-site-data: unknown argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }

    match write(&root, &out) {
        Ok(paths) => {
            println!(
                "climb-site-data: wrote {} document(s) to {}",
                paths.len(),
                out.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("climb-site-data: {error}");
            ExitCode::FAILURE
        }
    }
}
