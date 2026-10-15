# CLIMB — Cryptographic Ladder Impact Measurement Benchmark

**Team**: [Pratyay Mukherjee](https://github.com/profm0r14rty), [Mohit Kumar Yadav](https://github.com/Mohh12glitch), Dr. Maibam Devi (Faculty Mentor)

**AIORI-3 Hackathon PS-036**: Benchmarking Merkle Tree Ladder (MTL) mode for post-quantum
DNSSEC signing (SLH-DSA), comparing signing with MTL amortization against signing without.


## Baselines

Two signing settings are compared throughout, and the labels are used
consistently across the report, charts, TUI and this README:

- **MTL amortised** — one signed Merkle tree ladder is shared across the
  whole batch; each message carries only a condensed authentication path.
- **No amortisation** — every message carries a complete MTL signature (its
  condensed path concatenated with the ladder).

"No amortisation" is the MTL wrapper's non-amortised mode, **not** a plain
SLH-DSA RRSIG (one full SLH-DSA signature per RRset, with no ladder), which is
a different baseline. See [`docs/BASELINES.md`](docs/BASELINES.md) for the
wire-byte formula of each configuration.

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

## Reproduce

One command runs the whole artifact chain in order and leaves the report
self-check passing:

```bash
cargo xtask reproduce
```

It pins the environment, measures wire sizes and memory, optionally re-runs the
benchmark suite, captures the live DNS pipeline (or falls back to the committed
recorded capture), then consolidates, charts, renders and self-checks the
report. Each step's status and duration are printed, and the run stops at the
first failure with the failing step named. The final step is the report
self-check, so a zero exit means the artifacts and the rendered report agree.

| Flag | Effect |
|---|---|
| `--quick` | Limit every measurement to the Small and Medium datasets |
| `--bench` | Re-run the Criterion benchmark suite (default: reuse the last output) |
| `--skip-dns` | Skip the live DNS pipeline and use the recorded capture |
| `--force` | Accept optional inputs regardless of age when consolidating |

The live DNS step also runs only when Docker is available; otherwise the
recorded capture is used automatically.

