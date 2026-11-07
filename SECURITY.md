# Security Policy

## Scope

This repository contains **CLIMB** (Cryptographic Ladder Impact Measurement Benchmark), a research benchmark suite for measuring the operational impact of Merkle Tree Ladder (MTL) mode applied to post-quantum DNSSEC signing (SLH-DSA). 

**This is a research benchmark, not a production system.**

## No Formal Guarantee

- This project makes **no formal security guarantee, proof, or certification** for MTL mode, SLH-DSA, or any component.
- Results are **empirical measurements only** under controlled conditions.
- Do not use this code in production environments where security guarantees are required.

## Reporting Issues

If you discover a security issue or vulnerability:

1. **Do not** open a public GitHub issue for security-sensitive findings.
2. Report via the project's designated contact channels as specified in the hackathon submission.
3. Include:
   - Description of the issue
   - Steps to reproduce
   - Potential impact
   - Affected components/versions

We will acknowledge receipt and work to assess and address valid security concerns. Given this is a research benchmark, response timelines may vary.

## Dependency advisories

The workspace's supply-chain policy is `deny.toml`. Run both scanners from the
repository root:

```sh
cargo deny check
cargo audit
```

`cargo deny check` is the gate: it fails on an advisory that is neither ignored
nor fixed. `cargo audit` shares the same advisory database but has no
per-advisory policy file, so it prints the ignored items below as ordinary
vulnerabilities. Three advisories are explicitly ignored in `deny.toml`, each
with its rationale recorded there and repeated here:

- **RUSTSEC-2023-0071 (`rsa`)**: `rsa` is a dev-dependency of `climb-bench`,
  used only to measure the RSA-2048 classical baseline for context. No shipped
  binary links it, and the benchmark never receives a private key over a
  network. There is no fixed upgrade.
- **RUSTSEC-2026-0387 (`crypto-bigint`)**: reached only through the
  dev-dependency-only P-256/ECDSA classical baseline in `climb-bench`. The
  benchmark signs fixed local test messages and ships no binary that links it.
- **RUSTSEC-2026-0192 (`ttf-parser`, unmaintained)**: reached only through
  `plotters` when rendering the repository's own SVG/PNG charts. The fonts are
  bundled with this repository; no caller supplies a font file. There is no
  safe upgrade within the pinned `plotters` release.

`cargo deny check` also warns (does not fail) that `yoke-derive 0.8.3` is
yanked. It is a transitive dependency of `hickory-dns`'s IDNA support and was
pinned by the lockfile; removing it requires a dependency bump outside this
benchmark's scope.

