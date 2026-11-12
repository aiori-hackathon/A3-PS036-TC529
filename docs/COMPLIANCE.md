# Requirements traceability

One table maps every requirement and expected outcome of the problem
statement to the evidence that satisfies it: the tracked file that implements
or records it, the automation command that reproduces it, the JSON pointer
into the committed results artifact that carries the number, and the
dashboard tab that shows it. `cargo xtask docs-check` resolves every pointer
in this table — file paths must be tracked in the repository, commands must
parse as real automation subcommands, JSON pointers must resolve in the
committed sample artifact, and tab names must be real dashboard tabs — so a
row cannot cite evidence that does not exist.

Status legend: **met** — the evidence is complete and mechanically checked;
**partial** — some evidence exists and a named piece is still missing;
**pending** — nothing shipped yet. A row carrying a gap is never marked met,
and the *Gap closed by* column names the deliverable that will close it.

<!-- compliance:begin -->
| Requirement (problem-statement wording) | Status | Gap closed by | Evidence | Command | Artifact pointer | TUI tab |
| --- | --- | --- | --- | --- | --- | --- |
| use one supported PQC signature algorithm | met | — | `crates/climb-mtl/src/lib.rs` | `cargo xtask bench-all` | `/ladder/algorithm` | Signing |
| use one mentor-tested MTL reference implementation | met | — | `ENVIRONMENT.md` | `cargo xtask check-pins` | `/environment/submodules` | Method |
| test three dataset sizes | met | — | `xtask/src/groups.rs` | `cargo xtask bench-all --sizes all` | `/wire_size` | Signing |
| compare equivalent workloads with and without MTL | met | — | `docs/BASELINES.md` | `cargo xtask bench-all` | `/comparisons` | Wire |
| repeated trials under a pinned software and hardware environment | met | — | `scripts/pin-perf-governor.sh` | `cargo xtask stability` | `/derived/stability` | Method |
| measure signing time, verification time and transmitted bytes | met | — | `crates/climb-bench/benches/signing.rs` | `cargo xtask bench-all` | `/benchmark/signing, /benchmark/verifying, /wire_size` | Signing, Verifying, Wire |
| record and report configuration and workload parameters | met | — | `ENVIRONMENT.md` | `cargo xtask pin-env` | `/environment` | Method |
| separate directly measured cache usage from analytical estimates | met | — | `docs/METHODOLOGY.md` | `cargo xtask dns-cache` | `/cache_use` | Memory |
| no new cryptography | met | — | `crates/climb-mtl-sys/src/lib.rs` | `cargo xtask check-pins` | `/ladder/algorithm` | Method |
| no resolver from scratch | met | — | `docker/Dockerfile.nsd` | `cargo xtask dns-demo` | `/dns_pipeline` | Live |
| no Internet deployment | met | — | `docker/compose.dns-pipeline.yml` | `cargo xtask dns-demo` | `/dns_pipeline` | Live |
| no Internet deployment — hosting the results dashboard (AIORI written reply, 2026-11-10, AIORI-3 hackathon organisers) | partial | the web release of the recorded-results dashboard | `README.md` | — | — | — |
| no claimed security proof | met | — | `docs/METHODOLOGY.md` | `cargo xtask docs-check` | — | Method |
| expected outcome — pinned environment | met | — | `ENVIRONMENT.md` | `cargo xtask pin-env` | `/environment` | Method |
| expected outcome — benchmark harness | met | — | `crates/climb-bench/benches/signing.rs` | `cargo xtask bench-all` | `/benchmark` | Signing |
| expected outcome — MTL and non-MTL configurations | met | — | `docs/BASELINES.md` | `cargo xtask bench-all` | `/benchmark/signing` | Signing |
| expected outcome — three dataset-size test cases | met | — | `xtask/src/groups.rs` | `cargo xtask wire-report` | `/wire_size` | Wire |
| expected outcome — repeated-trial measurements | met | — | `xtask/src/stability.rs` | `cargo xtask stability` | `/derived/stability` | Signing |
| expected outcome — signing and verification results | met | — | `crates/climb-report/src/html.rs` | `cargo xtask report --format html` | `/benchmark` | Signing, Verifying |
| expected outcome — transmitted-byte comparison | met | — | `crates/climb-bench/src/wire_size.rs` | `cargo xtask wire-report` | `/wire_size` | Wire |
| expected outcome — cache-use measurements and analytical estimates | met | — | `xtask/src/dns_cache.rs` | `cargo xtask dns-cache` | `/cache_use` | Memory |
| expected outcome — reproducible results report | met | — | `docs/REPRODUCIBILITY.md` | `cargo xtask reproduce` | `/reproduction_mode` | Integrity |
| expected outcome — technical demonstration | met | — | `docs/media/CLIMB-TUI-demo-video.mp4` | `cargo xtask dist-tui` | `/dns_pipeline` | Live |
| expected outcome — prototype demonstration with a pseudocode snippet to the mentor | partial | the mentor demonstration session, which happens outside the repository | `docs/PSEUDOCODE.md` | `cargo xtask docs-check` | — | Method |
<!-- compliance:end -->

## Cache accounting: what the two measured targets are

The cache rows carry two distinct measurements. The **answer cache**
(`mem.cache.rrset`, `mem.cache.message`) is measured on the reference Unbound
build, which ignores the SigTag and serves the full responses the signed zone
stores; the measured MTL cell is therefore labelled "MTL-signed zone, reference
Unbound (serves full responses)" and the answer cache is identical between the
two measured configurations by construction. The **ladder cache**
(`mem.cache.ladder`) is the MTL-specific structure the patched resolver fills
when its validator verifies a signed ladder; its measured and analytical rows
live under `/cache_use/ladder_analytical` and never absorb the answer-cache
rows. Both targets carry `MEASURED` and `ANALYTICAL` rows separately.

## Hosting reply under "no Internet deployment"

AIORI's written reply of 2026-11-10 (from the AIORI-3 hackathon organisers)
confirmed that hosting the dashboard on a VPS behind a DuckDNS domain is
acceptable provided it is limited to dashboard access and is not an Internet
deployment of the DNSSEC/PQC implementation; the benchmark testbed itself
stays local. The consequence for this repository: the hosted site shows
recorded results and a client-side dashboard only; no signing, DNS or shell
service is exposed, and no measurement runs on the host. The row above stays
**partial** until that release exists; the local-only rule for every
measurement pipeline is unchanged and separately guarded.
