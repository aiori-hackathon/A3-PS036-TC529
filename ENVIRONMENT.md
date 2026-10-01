# CLIMB — Build-Environment Pin Record

This file records the exact versions, commit SHAs, and base-image digests used to
build the CLIMB benchmarking stack. Any CI/CD pipeline or new developer setting up a
reproducible build should reproduce this exact environment.

> **Relationship to `environment.json`.** As of Phase 11, `environment.json`
> (written by `cargo xtask pin-env`) is the **single, machine-readable,
> authoritative fingerprint** PS-036 asks for: host identity, toolchain,
> resolved submodule SHAs (read programmatically from `git submodule status`),
> Docker images/digest, and the governor + `taskset` runtime state. This file
> retains the **provenance and build-recipe narrative** — repo URLs, branch
> names, compile flags, the reasoning behind each pin, and verification status —
> which is not machine-derivable. The resolved SHA values below are mirrored
> from `environment.json`; if the two ever disagree, `environment.json` (which
> is generated, not hand-typed) wins.

## Pinned submodules

| Component | Repo | Tag / branch | Resolved commit SHA |
|---|---|---|---|
| **liboqs** | `github.com/open-quantum-safe/liboqs` | `0.16.0` | `5a1a854b0dc9f2141bdc771c555ee60c37950183` |
| **libMTL** | `github.com/profm0r14rty/MTL` | `climb/liboqs-0.16-compat` branch | `53ce25dcc35c15e051c5263e41aaeaede18a8aa7` |

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

> **Correction (Phase 11.1).** Earlier revisions of this table recorded the
> libMTL resolved SHA as `53ce25da1dfb5afaca843df8e5945e170dc47855`. That value
> was hand-typed and is **not a real object** in the fork (`git cat-file` on it
> fails). The authoritative SHA at the superproject's submodule pointer is
> `53ce25dcc35c15e051c5263e41aaeaede18a8aa7` (same 7-char prefix, different
> remainder). This is exactly the class of drift that motivated moving the
> resolved values into the generated `environment.json`; the values above are
> now mirrored from there.

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

## Docker images (Batch 4 — live DNS pipeline)

| Image | Tag | Dockerfile | Source | Source SHA |
|---|---|---|---|---|
| **climb-nsd** | `phase12` | `docker/Dockerfile.nsd` | `verisign/mtl-mode-nsd`, branch `IETF-126-Interim` | `772d31732dcda4c75a062e26ba631f8bcb477471` |
| **climb-unbound** | `phase13` | `docker/Dockerfile.unbound` | `verisign/mtl-mode-unbound`, branch `IETF-126-Interim` | `d47c042a72a7d26b2099db6f43f93cbf4f51b8ff` |
| **climb-ldns-signer** | `phase14` | `docker/Dockerfile.ldns-signer` | `verisign/mtl-mode-ldns`, branch `IETF-126-Interim` | `1de55d90709ef37e5c5ba81e450761cea31c6fc7` |
| **climb-dig** | `phase14` | `docker/Dockerfile.dig` | `alpine:3.20` + `bind-tools`/`openssl`/`python3` | `alpine@sha256:d9e853e8…` |

`climb-nsd`, `climb-unbound`, and `climb-ldns-signer` share the same Debian Bookworm base
image (digest `sha256:3783cc01…`) and the same OpenSSL 3.4.1 + liboqs 0.16.0 + libMTL
(pro fm0r14rty/MTL @ `53ce25dcc3…`) build chain. NSD only needs OpenSSL (EVP SHAKE-128 for
the MTL ladder hash); Unbound and the signer additionally link liboqs + libmtlslib — for
Unbound this is validation/ladder-cache management, for the signer it is actual MTL
signature generation. `climb-dig` is an Alpine-based query tool used by `cargo xtask
dns-demo`.

**Networking (compose):** all services run on a single shared `climb-net` internal bridge
(`192.168.13.0/24`, `internal: true`, no Internet access per the project conventions §2.3). NSD at
`192.168.13.2:53`, Unbound at `192.168.13.3:53`. Compose file:
`docker/compose.dns-pipeline.yml` (supersedes `compose.nsd.yml` and `compose.batch4.yml`).

- **Nothing is published to the host.** There are deliberately no `ports:` mappings. Docker
  does not create host listeners for a service whose only network is `internal: true`, so
  the localhost port mappings the earlier compose files declared were inert; the query path
  now runs inside `climb-net` as a short-lived `dig` container instead. See
  `docker/compose.dns-pipeline.yml` for the documented localhost-only fallback if host
  access is ever required.
- **Zone state:** `climb.example.` is MTL-signed (SLH-DSA-SHA2-128s-MTL-SHA2-128, alg 130)
  by `climb-ldns-signer`. `cargo xtask dns-demo` signs it live, serves it via NSD, and
  demonstrates condensed-signature responses; Unbound validates it and returns `ad`. The
  committed `docker/zones/climb.example.zone` remains the unsigned source zone that the
  demo signs.

---
*Last updated: Phase 14 (2026-09-30)*