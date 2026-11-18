# Judge Q&A — twenty-two questions with evidence

Each answer is written to be said out loud, is at most sixty words, and ends
with the evidence row that supports it and the command that shows it. **Every
answer needs human review** before the mentor session: two rows are partial, one
research table is still pending researcher sign-off, and the wording of AIORI's
written replies should be confirmed by the team.

The first fourteen are the judge-session set. The last eight are derived from
[`COMPLIANCE.md`](COMPLIANCE.md), [`RESEARCH.md`](RESEARCH.md) and the limits the
project states against itself. No answer claims a security proof, a standard, or
a measurement that is not recorded.

## The fourteen question set

### 1. “Your MTL library is a personal fork, not one the mentors supplied. How is it mentor-tested?” — needs human review

AIORI's written reply of 2026-11-10 said mentors would not supply an
implementation. The pinned commit `53ce25d` is the public Verisign libMTL
release tree plus only the liboqs compatibility patch; six conformance tests
and the negative suite run against it.

**Evidence:** row R2 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo test -p climb-mtl --test conformance`.

### 2. “Nine model ratios over five runs is not repeated trials of every benchmark. How do you justify ‘repeated trials’?” — needs human review

Criterion repeats every timed cell with warm-up and confidence intervals; the
stability command repeats the primitive-model ratios five times and records
median, minimum, maximum and coefficient of variation for nine series. The
governor is pinned to `performance` and the process to one CPU.

**Evidence:** row R5 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask stability`.

### 3. “You measure RSS and resolver counters, not L1/L2 cache. How is that ‘cache usage’?” — needs human review

The problem statement does not define cache. We measure resolver cache bytes
from Unbound's own accounting and ladder cache before and after loading, and
keep the libMTL page model as a separate analytical row. CPU hardware counters
were not available.

**Evidence:** row R8 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask dns-cache`.

### 4. “Your dashboard is on the public Internet. Isn't that an Internet deployment?” — needs human review

Only the recorded-results dashboard is hosted, after AIORI's written reply of
2026-11-10 permitted it. Signing, the DNS resolver and the measurement testbed
run locally; the compose file publishes no host port and no measurement runs on
the host.

**Evidence:** row R12 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask docs-check`.

### 5. “Your UDP-fit chart is a model, not a measurement. Why present it?” — needs human review

It is labelled analytical, and its inputs are the captured response profile,
zone and measured signature lengths. Feeding the capture's own row through it
reproduces the recorded full and condensed response sizes with zero-byte error;
the live capture is reported separately as measured over TCP.

**Evidence:** row O7 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask wire-report`.

### 6. “How is this different from the published MTL paper?” — needs human review

The CT-RSA paper establishes amortisation and ladder reuse conceptually. CLIMB
measures the pinned reference implementation end to end: signing, verification,
wire bytes, cache, and a live local DNS capture, each against the same workload
without amortisation. The paper is cited as background, not reimplemented.

**Evidence:** the CT-RSA 2023 source row in [`RESEARCH.md`](RESEARCH.md) · `cargo xtask docs-check`.

### 7. “Your drafts are expired Internet-Drafts. Are you benchmarking a standard?” — needs human review

The MTL mode and SLH-DSA-MTL DNS drafts are Informational Internet-Drafts; the
observed revisions had expired. The pinned library describes itself as draft-00
while its hash and buffer code cite later revisions. CLIMB measures that
implementation and claims no wire compatibility with one published revision.

**Evidence:** the two draft rows in [`RESEARCH.md`](RESEARCH.md) · `cargo xtask check-pins`.

### 8. “Which draft revision does the library actually implement?” — needs human review

Mixed. Its README says draft-00; hash code cites draft-07 constructions and
buffer code cites draft-08 layouts. The DNS side follows the SLH-DSA-MTL
draft's record shapes with provisional algorithm 130 and EDNS option 65050. The
repository records this rather than resolving it.

**Evidence:** the “What the pinned implementation actually follows” table in [`RESEARCH.md`](RESEARCH.md) · `cargo xtask check-pins`.

### 9. “Why only SLH-DSA-SHA2-128s? Why not ML-DSA or SHAKE?” — needs human review

SLH-DSA has the largest signatures and the most to gain from amortisation, and
the 128-bit SHA2 “small” set is the DNS-size-sensitive choice. It is the
library's default and the draft's recommended instantiation. The pinned library
supports other families, but CLIMB reports this one configuration only.

**Evidence:** row R1 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `jq -r '.ladder.algorithm' docs/sample-output/climb-results.json`.

### 10. “Why are the Large plain-SLH-DSA numbers extrapolated?” — needs human review

Plain SLH-DSA at Large would sign one million messages individually; the
recorded per-message cost was measured at Small and Medium and applied with the
`N × per-message` identity. Every affected row is classed EXTRAPOLATED and
marked with `≈`. The OneK size exists to validate that identity.

**Evidence:** row O5 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `jq -r '.plain[] | "\(.size) \(.class)"' docs/sample-output/climb-results.json`.

### 11. “The deck quotes earlier speed-ups. Your sheet quotes 9,434 × and 77.34 ×. Which is right?” — needs human review

The sheet and artifact are current; the deck predates the final run and has
three stale values on slide 8. `deck-delta` lists them as a human punch list and
the PDF is not silently edited. Quote the artifact: 9,434 × signing, 77.34 ×
cached verification.

**Evidence:** row D2 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask deck-delta --deck-text target/deck-text.txt`.

### 12. “How do we know the HTML report matches the artifact?” — needs human review

`sample-check` parses the report's claims registry and compares every claim with
the consolidated artifact, then verifies the checksum manifest and provenance
commit. The current sample reports zero drift and zero missing claims. The
Integrity tab runs the same comparison live.

**Evidence:** row O9 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask sample-check`.

### 13. “How reproducible is this on another machine?” — needs human review

Every artifact carries a host fingerprint, and the pinned toolchain, submodule
commits and container digests are enforced by `check-pins`. The full chain is
one command, `reproduce`, and the replication summary compares two hosts'
headline ratios when a second artifact exists. The current host is one laptop.

**Evidence:** rows R7 and O9 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask check-pins`, `cargo xtask reproduce`.

### 14. “What did the live DNS pipeline prove, and what did it not?” — needs human review

It proved that the pinned NSD, Unbound and signer forks serve one signed zone
locally, and that the captured condensed response is smaller than the full
response: 24,438 B against 658 B. It did not prove Internet behaviour: the
capture is local and over TCP.

**Evidence:** rows R11 and R12 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask dns-demo`.

## Eight more from compliance, research and the honest limits

### 15. “You claim no new cryptography. Where is the boundary?” — needs human review

Every cryptographic operation crosses the FFI boundary into the pinned libMTL
and liboqs code. The Rust side owns buffers, lengths and layout checks; it
implements no hash, signature or ladder construction. The wrapper contains only
`mtllib_*` invocations around ownership code.

**Evidence:** row R10 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `git grep -n 'mtllib_' -- crates/climb-mtl/src`.

### 16. “You claim no resolver from scratch. Did you modify NSD or Unbound?” — needs human review

No resolver code is written here. The Docker files build the pinned Verisign and
community forks; CLIMB supplies configuration, zone content and capture tooling.
If Docker is unavailable, the recorded capture drives the same Live tab, and
that limitation is stated.

**Evidence:** row R11 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `git grep -n 'mtl-mode-ldns\|mtl-mode-nsd\|mtl-mode-unbound' -- docker`.

### 17. “Why is the answer cache identical between the two configurations?” — needs human review

The reference Unbound build ignores the SigTag and serves the full response in
both configurations, so its answer cache is equal by construction. The
MTL-specific ladder cache is a separate measured target with its own analytical
row. Labelling the measured cell as a condensed win would be wrong.

**Evidence:** the cache-accounting section of [`COMPLIANCE.md`](COMPLIANCE.md), row R8 · `cargo xtask dns-cache`.

### 18. “Why does cached verification look fast while cold verification does not?” — needs human review

Verification is fast only when the ladder is already trusted. Re-validating the
signed ladder costs roughly what plain verification costs, so the cold ratio is
0.9236 × at Medium. Both rows are published; the cached headline is not
presented as the only verification result.

**Evidence:** row O6 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `jq -r '.comparisons[] | "\(.size) \(.verifying_speedup_cached_vs_a) \(.verifying_speedup_cold_vs_a)"' docs/sample-output/climb-results.json`.

### 19. “What does 11 of 12 UDP rows mean, and what is the twelfth?” — needs human review

It is a closed-form payload model over twelve recorded response profiles, not
packet capture. For C, eleven profiles fit the 1,232-byte payload in every
aggregated row; the twelfth has a response that does not. The gauge and the row
are labelled ANALYTICAL so the distinction survives a screenshot.

**Evidence:** row O7 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask wire-report`.

### 20. “Does the extrapolation identity actually hold?” — needs human review

At OneK the identity was checked against a real intermediate measurement, and
the stability block repeats the primitive-model ratios five times with median,
minimum, maximum and coefficient of variation. The identity is bounded and
checked; it is not a claim that every Large cell was timed.

**Evidence:** rows R5 and O5 in [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md) · `cargo xtask stability`, `cargo xtask verify-numbers`.

### 21. “There is an IPR disclosure on the MTL draft. What does that mean for users?” — needs human review

The draft carries a VeriSign disclosure from 2023 with a royalty-free
standards-development grant and an attribution notice. CLIMB records the
pointer and makes no legal interpretation; the researcher must confirm the
wording. Nothing here relies on the disclosure as a licence grant.

**Evidence:** the IPR note in [`RESEARCH.md`](RESEARCH.md) · `cargo xtask docs-check`.

### 22. “Which rows are still partial, and why?” — needs human review

Three: the PDF's section-by-section match to AIORI's template, the public
dashboard release under the hosting addendum, and the mentor demonstration
itself. Each has evidence in the repository; each needs a human or an external
session that the repository cannot perform. They are marked partial, never met.

**Evidence:** rows D2 and D3 plus the hosting row in [`COMPLIANCE.md`](COMPLIANCE.md) · `cargo xtask docs-check`.
