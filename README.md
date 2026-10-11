# CLIMB — Cryptographic Ladder Impact Measurement Benchmark

**AIORI-3 Hackathon PS-036**: Benchmarking Merkle Tree Ladder (MTL) mode for post-quantum
DNSSEC signing (SLH-DSA), comparing signing with MTL amortization against signing without.


## Build

```bash
cargo build --workspace
```

## Development tools

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run  # if nextest is installed
```

## Environment pinning

```bash
cargo xtask pin-env
```

Writes `environment.json` at the repository root with CPU model, kernel version,
Rust/Cargo versions, and other build-environment metadata.
