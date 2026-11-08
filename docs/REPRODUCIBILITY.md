# Reproducibility — inputs and the checks that enforce them

Every input that can change a CLIMB number, where it is pinned, and which check fails when the
pin and the artifact disagree. Two kinds of entries appear below: inputs a gate re-checks on every
run, and inputs that are recorded and left to review. The second group is named honestly rather
than dressed up as enforced.

## Inputs

| Input | Kind | Where it is pinned | Enforced by |
|---|---|---|---|
| Rust toolchain | version | `rust-toolchain.toml`; the running versions are recorded under `toolchain` in `environment.json` | `cargo xtask pin-env` records the host; the channel is a tracked file, so a change is a reviewable diff. No gate compares the two. |
| Crate versions | lockfile | `Cargo.lock` | Cargo resolves builds from the committed lockfile; a changed resolution appears as a diff to that file. |
| Docker base images | digest | every `FROM` line in `docker/Dockerfile*` | `cargo xtask check-pins` fails on any `FROM` that is not `image@sha256:<64 hex>` (`every_shipped_dockerfile_from_is_digest_pinned`). |
| Build-tool apt packages | version | `package=version` arguments in `docker/Dockerfile.build` | `cargo xtask check-pins` (`the_toolchain_dockerfile_pins_every_apt_package`). |
| OpenSSL release archive | checksum | `ARG OPENSSL_SHA256` in every Dockerfile that downloads it | `sha256sum -c -` inside the image; `every_dockerfile_that_downloads_openssl_verifies_the_archive` asserts every such Dockerfile still carries the digest and the check. |
| libMTL and liboqs | commit | `third_party/` submodules, restated in `docker/Dockerfile.build` comments and `environment.json.submodules` | `cargo xtask check-pins` fails when the checkout, the comment and the fingerprint disagree. |
| ldns and Unbound MTL forks | commit | `ARG LDNS_MTL_SHA` and `ARG UNBOUND_MTL_SHA` in their Dockerfiles | the image's `git checkout <sha>` fails unless the commit resolves at build time. No project-level gate re-checks these commits. |
| Dataset seeds | constant | `DEFAULT_WIRE_SEED` plus the explicit `seed` argument on each measurement entry point | `climb-bench` unit test `default_seed_matches_wire_seed`; these paths read no OS entropy. |
| DNS capture fixture | checksum | `docs/sample-output/dns-capture.recorded.json`, source copy under `data/` | `docs/sample-output/SHA256SUMS` is re-hashed by `cargo xtask docs-check`. |
| Ladder verification fixture | checksum | `docs/sample-output/ladder-verification.recorded.json`, source copy under `data/` | the same manifest check under `cargo xtask docs-check`. |
| Committed sample artifacts | checksum | `docs/sample-output/SHA256SUMS` | `cargo xtask docs-check` re-hashes every listed file and names a stale digest. |
| CPU governor and core pinning | host, time | `benchmark_runtime` in `environment.json` | `cargo xtask pin-env` records the governor per CPU and the `taskset` expression; benchmark runs use that expression. Not re-verified by a gate. |
| Kernel, CPU and hostname | host | `system` and `cpu` in `environment.json` | `cargo xtask pin-env` records them; the file is regenerated per machine and is not committed. |
| Clock | time | `timestamp` in `environment.json`; `recorded_at` in the report | `cargo xtask pin-env` records it; every report and the TUI render it. |
| Reproduction mode | provenance | `reproduction_mode` in `target/climb-results.json` | written by `cargo xtask reproduce --report-only` or `--remeasure`, carried through `consolidate` and rendered by the report. |

## Clean-clone checks

Two scripts prove two different things, and their names say which:

- **`scripts/clean-clone-cached.sh`** clones the committed tree and copies build artifacts from the
  working tree. It proves the derived artifact chain runs from a warm target.
- **`scripts/clean-clone-fresh.sh`** clones a new checkout with submodules and copies **nothing**
  from the working tree. It proves a cold checkout builds and verifies itself.

`scripts/clean-clone-fresh.sh --viewer-only` is the fast path: it builds, lints and smoke-runs only
the read-only viewer with `--no-default-features`, checks that the resolved dependency tree contains
no FFI crate, and asserts the checkout stayed clean. It needs no native prefix and no Docker.

`scripts/clean-clone-fresh.sh --full` is the complete path: it builds the pinned native toolchain in
Docker, extracts it, then runs the workspace format, build, lint, test, documentation, hygiene, pin
and reproduction gates in the clone. It is the check to run on mains power.

## Reproduction modes

`cargo xtask reproduce` defaults to `--report-only`, which rebuilds the derived chain (consolidate,
charts, report, TUI data) from measurements that already exist. It reuses measurements; it does not
claim to have re-measured them.

`cargo xtask reproduce --remeasure` re-runs the benchmark and measurement steps first. `--bench`
remains as a hidden alias for `--remeasure` so older notes keep working. Every run prints a banner
naming the mode, and the mode is written into the artifact so a reader can tell which claim the
numbers support.

## Notes

- `environment.json` is generated by `cargo xtask pin-env`, is gitignored, and is never committed;
  the committed sample artifact carries the fingerprint of the machine that produced it.
- Test-only domain names are exempt from the source-hygiene scan only on lines that carry the
  `source-hygiene:allow` marker; there is no blanket exemption for documentation.
- A fixture edited without refreshing `docs/sample-output/SHA256SUMS` now fails
  `cargo xtask docs-check`, so the recorded digest cannot silently describe an older file.
- The dataset seed is a constant, not a recorded input: changing it changes the synthetic zone and
  therefore every measured number, which is why the constant is called out here.
