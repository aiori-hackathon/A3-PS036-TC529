use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// CLIMB task automation — pin the environment, run benchmarks, generate reports.
#[derive(Parser)]
#[command(name = "xtask", about = "CLIMB automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Capture the current build environment and write it to environment.json.
    PinEnv,
    /// Run all benchmarks (stub — Batch 2).
    BenchAll,
    /// Generate the final report from benchmark data (stub — Batch 5).
    Report,
}

/// A snapshot of the build environment, written to `environment.json`.
#[derive(Serialize)]
struct Environment {
    system: SystemInfo,
    toolchain: ToolchainInfo,
    cpu: CpuInfo,
    timestamp: String,
}

#[derive(Serialize)]
struct SystemInfo {
    uname: String,
}

#[derive(Serialize)]
struct ToolchainInfo {
    rustc_version: String,
    cargo_version: String,
}

#[derive(Serialize)]
struct CpuInfo {
    model_name: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::PinEnv => cmd_pin_env(),
        Commands::BenchAll => cmd_bench_all(),
        Commands::Report => cmd_report(),
    }
}

fn cmd_pin_env() -> Result<()> {
    let uname = run_cmd("uname", &["-a"])?;
    let rustc_version = run_cmd("rustc", &["--version"])?;
    let cargo_version = run_cmd("cargo", &["--version"])?;
    let model_name = read_cpu_model()?;
    let timestamp = chrono_like_timestamp();

    let env = Environment {
        system: SystemInfo { uname },
        toolchain: ToolchainInfo {
            rustc_version,
            cargo_version,
        },
        cpu: CpuInfo { model_name },
        timestamp,
    };

    let json = serde_json::to_string_pretty(&env).context("failed to serialize environment")?;

    let output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is always inside the workspace")
        .join("environment.json");

    fs::write(&output_path, json).context("failed to write environment.json")?;

    eprintln!("wrote environment snapshot to {}", output_path.display());
    Ok(())
}

fn cmd_bench_all() -> Result<()> {
    println!("not yet implemented, see Batch 2");
    Ok(())
}

fn cmd_report() -> Result<()> {
    println!("not yet implemented, see Batch 5");
    Ok(())
}

/// Run a command and capture its stdout trimmed.
fn run_cmd(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        anyhow::bail!("{program} exited with status {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Read the CPU model name from /proc/cpuinfo.
fn read_cpu_model() -> Result<String> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").context("failed to read /proc/cpuinfo")?;
    for line in cpuinfo.lines() {
        if let Some(model) = line.strip_prefix("model name\t: ") {
            return Ok(model.trim().to_string());
        }
    }
    anyhow::bail!("model name not found in /proc/cpuinfo")
}

/// Return an ISO 8601 timestamp without pulling in the `chrono` crate for this stub phase.
fn chrono_like_timestamp() -> String {
    // Use the `date` command to get a real ISO 8601 timestamp.
    // This avoids adding a chrono dependency just for one field in Phase 0.
    run_cmd("date", &["--iso-8601=seconds"]).unwrap_or_else(|_| "unknown".to_string())
}
