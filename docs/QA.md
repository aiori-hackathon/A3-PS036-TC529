# Judge Q&A — the five hardest questions

Each answer is written to be said out loud, is backed by a file or a command in
this repository, and is at most fifty words. **These answers need human review**
before the mentor session: confirm the AIORI statement, the fork story and the
cache framing in your own words.

## 1. “Your MTL library is a personal fork, not one the mentors supplied. How is it mentor-tested?” — needs human review

AIORI's written reply of 2026-11-10 said mentors would not supply an
implementation. The pinned commit `53ce25d` is the public Verisign libMTL
release tree plus only the liboqs compatibility patch; six conformance tests
and the negative suite run against it. Evidence: `ENVIRONMENT.md`,
`crates/climb-mtl/tests/conformance.rs`, `crates/climb-mtl/tests/negative.rs`.

## 2. “Nine model ratios over five runs is not repeated trials of every benchmark. How do you justify ‘repeated trials’?” — needs human review

Criterion repeats every timed cell with warm-up and confidence intervals; the
stability command repeats the primitive-model ratios five times and records
median, minimum, maximum and coefficient of variation for nine series. The
governor is pinned to `performance` and the process to one CPU. Evidence:
`xtask/src/stability.rs`, `environment.json`.

## 3. “You measure RSS and resolver counters, not L1/L2 cache. How is that ‘cache usage’?” — needs human review

The problem statement does not define cache. We measure resolver cache bytes
from Unbound's own accounting and ladder cache before and after loading, and
keep the libMTL page model as a separate analytical row. CPU hardware counters
were not available. Evidence: `docs/METHODOLOGY.md`, `xtask/src/dns_cache.rs`.

## 4. “Your dashboard is on the public Internet. Isn't that an Internet deployment?” — needs human review

Only the recorded-results dashboard is hosted, after AIORI's written reply of
2026-11-10 permitted it. Signing, the DNS resolver and the measurement testbed
run locally; the compose file publishes no host port and no measurement runs on
the host. Evidence: `docker/compose.dns-pipeline.yml`, `docs/COMPLIANCE.md`.

## 5. “Your UDP-fit chart is a model, not a measurement. Why present it?” — needs human review

It is labelled analytical, and its inputs are the captured response profile,
zone and measured signature lengths. Feeding the capture's own row through it
reproduces the recorded full and condensed response sizes with zero-byte error;
the live capture is reported separately as measured over TCP. Evidence:
`crates/climb-analytical`, `docs/sample-output/climb-results.json`.
