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
| **libMTL** | `github.com/verisign/MTL` | main (pinned commit) | `566c5184cd1cf09deed1da8817cadd94bbe52585` |

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
-DOQS_MINIMAL_BUILD=SIG_slh_dsa
-DOQS_BUILD_ONLY_LIB=ON
-DOQS_DIST_BUILD=ON
-DOQS_USE_OPENSSL=ON
```

These produce only the SLH-DSA (SPHINCS+) signature family as a shared
library, linked against the bootstrapped OpenSSL 3.4.1.

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

## Verification status

| Test | Expected | Status |
|---|---|---|
| `test/mtltest` (unit tests) | `MTL Test completed successfully!` + exit 0 | **SKIPPED** — liboqs 0.16.0 removed `_length_public_key` macros; test cannot compile |
| `examples/test.sh` (integration) | `Testing Completed - All tests pass` + exit 0 | **SKIPPED** — same API incompatibility |
| `docker compose -f docker/compose.build.yml up --build` | Build passes, artifacts verified | **PASSING** — library (`libmtlslib.so`), headers, and liboqs confirmed at `/usr/local/` |
| OpenSSL version detected in container | `OpenSSL 3.4.1 11 Feb 2025` | **CONFIRMED** |

---
*Last updated: Phase 1 (2026-09-26)*