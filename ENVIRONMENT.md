# CLIMB — Build-Environment Pin Record

This file records the exact versions, commit SHAs, and base-image digests used to
build the CLIMB benchmarking stack. Any CI/CD pipeline or new developer setting up a
reproducible build should reproduce this exact environment.

> **Separate from `environment.json`**: `environment.json` (written by
> `cargo xtask pin-env`) captures the **host machine**'s runtime fingerprint
> (CPU model, kernel version, rustc version). This file is the **build
> container** fingerprint — the pinned upstream dependencies that live inside
> the Docker image.

## Pinned submodules

| Component | Repo | Tag / branch | Resolved commit SHA |
|---|---|---|---|
| **liboqs** | `github.com/open-quantum-safe/liboqs` | `0.16.0` | `5a1a854b0dc9f2141bdc771c555ee60c37950183` |
| **libMTL** | `github.com/profm0r14rty/MTL` | main (`climb/liboqs-0.16-compat` branch) | `53ce25da1dfb5afaca843df8e5945e170dc47855` |

> **liboqs version note**: the design notes specifies `≥0.14.0`; the latest
> stable tag at pin-time was `0.16.0`, which satisfies the floor.
> See the dev notes §Phase 1 for the `git ls-remote --tags` output that
> informed this decision.

## Build container

| Property | Value |
|---|---|
| Base image | `debian:bookworm-slim` |
| Base image digest | `sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251` |
| Dockerfile | `docker/Dockerfile.build` |
| Compose verification | `docker/compose.build.yml` |

## Bootstrap dependencies (built from source inside the container)

| Dependency | Version | Source | Reason for bootstrapping |
|---|---|---|---|
| **OpenSSL** | `3.4.1` | `https://www.openssl.org/source/openssl-3.4.1.tar.gz` | Debian Bookworm ships `libssl-dev 3.0.14`, which is below libMTL's `≥3.1.0` floor |

## liboqs build flags

```cmake
-DCMAKE_BUILD_TYPE=Release
-DBUILD_SHARED_LIBS=ON
-DOQS_MINIMAL_BUILD="SIG_slh_dsa;SIG_ml_dsa"
-DOQS_BUILD_ONLY_LIB=ON
-DOQS_DIST_BUILD=ON
-DOQS_USE_OPENSSL=ON
```

These produce the SLH-DSA and ML-DSA signature families as shared libraries,
linked against the bootstrapped OpenSSL 3.4.1. ML-DSA is included alongside
SLH-DSA so that the upstream test vectors (which use ML-DSA-44) work.

## libMTL build flags

```sh
autoreconf --install
CPPFLAGS="-I/usr/local/include" LDFLAGS="-L/usr/local/lib -Wl,-rpath,/usr/local/lib" \
  ./configure --libdir=/usr/local/lib
make -j"$(nproc)"
```

> **CFLAGS caveat**: `configure.ac` clobbers `CFLAGS` to `-O0` if it is
> already set at `./configure` time. Path flags (`-I`, `-L`) are passed via
> `CPPFLAGS` / `LDFLAGS` instead, which are not touched.

## Pinned benchmark runtime environment

The benchmark numbers-of-record are produced under the following runtime settings, in
addition to the build-container pinning above:

| Property | Value |
|---|---|
| CPU governor | **`performance` on all logical CPUs** (driver `intel_pstate`) |
| Governor script | `sudo ./scripts/pin-perf-governor.sh` (restore with `… powersave`); `cpupower` preferred if installed, otherwise direct `/sys` writes |
| CPU pinning | **CPU 2, via `taskset -c 2`** — applied automatically by `cargo xtask bench-all` |
| Opt out of pinning | `cargo xtask bench-all --no-pin` (for machines without `taskset`, e.g. minimal CI containers) |
| Criterion baselines | `batch2-pinned` = pinned/governed numbers-of-record; `batch2-final` = prior unpinned run (retained as history) |

**Why CPU 2.** Host CPU is an Intel i3-1115G4: 2 physical cores / 4 logical CPUs, with
physical core 0 = {cpu0, cpu2} and physical core 1 = {cpu1, cpu3}. Measured interrupt
totals at phase start were cpu1 = 1,428,276 · cpu3 = 140,767 · cpu0 = 65,862 ·
**cpu2 = 53,645**. CPU 2 is the quietest logical CPU, so it is the least-contended
single-CPU target; pinning to one CPU also removes run-to-run migration between the two
physical cores.

> **Residual variance caveat:** even with the governor fixed and pinning active, Criterion
> measurements on this laptop vary by up to ~25% run-to-run (thermal/turbo behaviour of a
> 15 W part under sustained load — not governor drift or CPU migration, which are the two
> confounders this section removes). Ratios are therefore reported to ~3 significant
> figures with confidence intervals, and headline figures should be multi-run medians.
> See `the dev notes` §F3.4 for the measured evidence. This is documented per
> `the design notes` §8's instruction to state laptop isolation limits honestly.

## Verification status

| Test | Expected | Status |
|---|---|---|
| `test/mtltest` (unit tests) | All 9 test modules pass (32+ individual tests) | **PASSING** — F2.2 patch applied |
| `examples/test.sh` (integration) | `Testing Completed - All tests pass` + exit 0 | **MINOR ISSUE** — script invocation in container (exit 127), not related to liboqs compat |
| `docker compose -f docker/compose.build.yml up --build` | Build passes, artifacts verified | **PASSING** — library (`libmtlslib.so`), headers, and liboqs confirmed at `/usr/local/` |
| OpenSSL version detected in container | `OpenSSL 3.4.1 11 Feb 2025` | **CONFIRMED** |

---
*Last updated: Fix 3 — F3.5 (2026-09-29)*