# Site copy deck

The exact words for every route in `docs/site/ia.md`. The design authority is
`docs/site/prototype/climb-site.html`; its strings and numbers are inventoried in
`docs/site/copy-prototype.md` and `docs/site/numbers-prototype.md`, and the
audit is in `docs/site/prototype-audit.md`. Where the prototype and this deck
differ, this deck wins, and the difference is named in
`docs/site/skin-plan.md` §8.

**How to read it.** Each route's copy sits between `<!-- copy:begin -->` and
`<!-- copy:end -->` markers. That is the text the site renders; it is the text
`scripts/site-check.py copy` scans. Everything outside the markers is a rule,
a reference table or a note for the implementer, not copy. Numbers in the copy
are fact-id slots, not literals: the implementer reads the value from the
committed artifact at build time.

## Voice

- Short sentences. One idea per sentence, one idea per screen.
- Plain words. Describe what was measured, not how impressive it is.
- Confident and checkable: `Measured, not claimed.` Every figure carries its
  evidence chip and the baseline it is compared against.
- Numbers reach the page through a fact id. A digit in the copy carries its
  `[F-…]` slot on the same line; the value is read from the artifact, not typed
  by hand.
- No emoji. No exclamation marks. No "AI-powered" or similar framing.
- No superlatives and no absolutes. Say the dataset, the baseline and the
  ratio; let the reader draw the conclusion.
- Two lines are kept verbatim from the design authority because they are the
  project's own words: the home headline `Post-quantum DNSSEC, measured.` and
  the footer headline `Sign once. Prove many.` In that headline "prove" means
  demonstrate at scale, not a formal `proof`; the limits paragraph says so in
  full, and no page claims a security property.

## Banned words and phrases

The scan in `scripts/site-check.py copy` fails on any of these, outside code
spans, anywhere in this file. The second column is why, the third is the
replacement.

| Do not use | Why | Use instead |
| --- | --- | --- |
| `revolutionary`, `seamless`, `cutting-edge`, `game-changing`, `next-generation`, `world-class`, `best-in-class`, `effortless`, `unprecedented`, `powerful` | unverifiable praise | delete, or state the measured effect |
| `leverage`, `unlock` | buzzwords that hide the action | the plain verb: `use`, `read`, `open` |
| `military-grade`, `bulletproof`, `unbreakable`, `hack-proof`, `future-proof`, `quantum-proof`, `quantum-safe` | security absolutes the benchmark does not test | `post-quantum signature`; name the measured behaviour |
| `secure`, `secured` | implies a property this benchmark does not test | name the mechanism: `condensed path`, `smaller response`, `fewer round trips` |
| `proof`, `proves`, `proven`, `guarantee`, `guaranteed`, `certified`, `compliant` | reads as formal verification, audit or certification | `measures`, `records`, `shows`, `documented in` |
| `eliminates`, `zero-risk`, `always`, `never` | absolutes | the measured ratio and its scope |

Words that are allowed but defined once: **claim** (a numeric assertion in the
report, not a security claim), **evidence class**, **pre-validated ladder**,
**condensed path**. The word **trust** is used only in the precise mechanism
phrase "the verifier trusts the cached ladder", not as an unqualified claim
about the system.

## Fact ids the copy needs

Every fact id used below. `scripts/site-check.py copy` resolves each one
against the committed artifact `docs/sample-output/climb-results.json` and
fails on an undeclared or unresolvable id. The ids that are **not** in the
results artifact are listed with their real source rather than passing
silently.

| Fact id | Where used | Chip | JSON pointer / source |
| --- | --- | --- | --- |
| `F-wire-medium-c-vs-a` | `/`, `/dashboard`, `/calculator` | Derived | `/comparisons/2/wire_reduction_c_vs_a_percent` |
| `F-sign-medium-c-vs-a` | `/`, `/dashboard`, `/calculator` | Derived | `/comparisons/2/signing_speedup_c_vs_a` |
| `F-verify-medium-c-vs-a` | `/`, `/dashboard` | Derived | `/comparisons/2/verifying_speedup_cached_vs_a` |
| `F-plain-signature-bytes` | `/` (hero labels) | Measured | **not in the artifact**: `/sizes/7/signature_len` in `docs/sample-output/ladder-verification.recorded.json` |
| `F-condensed-path-bytes` | `/` (hero labels) | Measured | **not in the artifact**: `/sizes/7/condensed/min` in `docs/sample-output/ladder-verification.recorded.json` |
| `F-udp-c-fit` | `/`, `/dashboard` | Analytical | `/derived/udp_fit/aggregate/2/fit_rows` |
| `F-udp-c-total` | `/`, `/dashboard` | Analytical | `/derived/udp_fit/aggregate/2/total_rows` |
| `F-wire-medium-a` | `/`, `/dashboard`, `/calculator` | Measured | `/wire_size/1/plain_total_bytes` |
| `F-wire-medium-b` | `/`, `/dashboard` | Measured | `/wire_size/1/without_mtl_total_bytes` |
| `F-wire-medium-c` | `/`, `/dashboard`, `/calculator` | Measured | `/wire_size/1/with_mtl_total_bytes` |
| `F-wire-medium-c-vs-b` | `/dashboard` | Derived | `/comparisons/2/wire_reduction_c_vs_b_percent` |
| `F-sign-batch-size` | `/calculator` | Measured | `/wire_size/1/message_count` |
| `F-mem-small-ratio` | `/dashboard` | Derived | `/memory/measured/0/measured_analytical_total_ratio` |
| `F-dns-www-a` | `/dashboard` | Derived | `/dns_pipeline/reduction_percent_www_a` |
| `F-dns-rtt-plain-50` | `/dashboard` | Measured | `/dns_rtt/cells/8/median_ms` |
| `F-dns-rtt-condensed-50` | `/dashboard` | Measured | `/dns_rtt/cells/9/median_ms` |
| `F-ladder-cache-measured` | `/dashboard` | Measured | `/cache_use/ladder_analytical/0/measured_bytes` |
| `F-ladder-cache-analytical` | `/dashboard` | Analytical | `/cache_use/ladder_analytical/0/analytical_bytes` |
| `F-ladder-cache-ratio` | `/dashboard` | Derived | `/cache_use/ladder_analytical/0/measured_over_analytical` |
| `F-breakeven-bytes` | `/calculator` | Derived | `/derived/reuse/1/breakeven_reuse_bytes` |
| `F-breakeven-signing` | `/calculator` | Derived | `/derived/reuse/1/breakeven_reuse_time` |
| `F-wire-sizes` | `/report`, `/reproduce`, `/method` | Measured | `/wire_size` (array length) |
| `F-parameter-set` | `/report`, `/reproduce`, `/method`, limits | Measured | `/dns_pipeline/algorithm` |
| `F-udp-payload` | `/calculator`, `/dashboard`, `/method` | Constant | **not in the artifact**: `UDP_MTU_1232` in `crates/climb-analytical/src/udp_fit.rs` |
| `F-claims-count` | `/verify`, footer | Generated | **not in the artifact**: emitted by `cargo xtask readme-badges` |

Six further ids are designed but have no producer yet: the claims-by-class
counts, the mean message bytes for B and C, and the per-message byte labels of
the message diagram. They are **not** listed here, because a gap id is not
allowed in copy until its producer exists. The work list, with the pointer or
generator each one needs, is `docs/site/fact-gaps.md`.

## The scope sentence

Used verbatim in the footer and in Honest limits:

> This website presents recorded results and a client-side dashboard. The
> DNSSEC and PQC benchmark itself is not deployed on the Internet; the DNS
> testbed runs locally.

---

## `/` — Home

The route has eight sections in this order: hero, results, capabilities,
ladder, how it works, architecture, transparency, team.

### Hero

<!-- copy:begin id=home-hero -->
Wordmark alt text: CLIMB, Cryptographic Ladder Impact Measurement Benchmark.

Headline: **Post-quantum DNSSEC, measured.**

Subhead: Measuring how Merkle Tree Ladder amortisation changes the performance
and wire efficiency of post-quantum DNSSEC signing with SLH-DSA.

Buttons: **Explore the benchmark** (primary) · **View on GitHub** (secondary)

Microcopy: AIORI-3 Hackathon, problem statement PS-036. Experimental research
benchmark.
<!-- copy:end -->

### Results

<!-- copy:begin id=home-figures -->
Headline: **Results**

Subhead: Every figure carries its evidence class and baseline. Values come from
the repository's recorded artifacts, Medium dataset.

Figure 1. Wire reduction, C against A: **96.508 %** (Derived) `[F-wire-medium-c-vs-a]`. From measured bytes: **2,743,352 B** (Measured) `[F-wire-medium-c]` against **78,560,000 B** (Measured) `[F-wire-medium-a]`.

Figure 2. Signing speed-up, C against A: **9,434 ×** (Derived) `[F-sign-medium-c-vs-a]`. Per-message cost scaled to batch size.

Figure 3. Verification speed-up with a pre-validated ladder, C against A: **77.34 ×** (Derived) `[F-verify-medium-c-vs-a]`.

Figure 4. Modelled UDP response profiles that fit the payload in configuration C: **11 of 12** (Analytical) `[F-udp-c-fit]` `[F-udp-c-total]`.

Evidence legend: Measured: read from a measurement artifact · Derived: computed
from recorded values · Analytical: closed-form model · Extrapolated: unit cost
times count, shown with `≈`.

Microcopy: Definitions are in Method. The full table is the quote sheet.
<!-- copy:end -->

### Capabilities

<!-- copy:begin id=home-capabilities -->
Headline: **Capabilities**

Subhead: Seven things the benchmark measures, each reproducible from one
command. Select a card to open it in the ladder.

Card (a). Signing performance. Criterion timings of SLH-DSA signing, with and
without MTL. `cargo xtask bench-all`

Card (b). Verification performance. Per-message verification, including a
pre-validated ladder. `cargo xtask bench-all`

Card (c). Wire efficiency. Wire-size and DNS packet analysis plus an analytical
UDP-fit model. `cargo xtask wire-report`

Card (d). Memory analysis. Measured memory compared with a closed-form model.
`cargo xtask memory-report`

Card (e). Ladder visualisation. Ladder shape and condensed paths in the terminal
dashboard. `cargo run -p climb-tui`

Card (f). DNS packet replay. A recorded capture from patched NSD and Unbound,
replayed locally. `cargo xtask dns-demo`

Card (g). Integrity and provenance. Claims registry, drift checks and a
checksummed release bundle. `cargo xtask report --check`

Closing card: All seven capabilities are in the open repository. **View on
GitHub ↗**
<!-- copy:end -->

### Explore the ladder

<!-- copy:begin id=home-ladder -->
Headline: **Explore the ladder**

Subhead: Choose a capability to open its branches, then a leaf to see its
evidence class and the command that reproduces it.

Group label: Capabilities. Group label: Branches. Ladder root label: CLIMB
ladder.

Signing performance · SLH-DSA signing. Criterion benchmark groups time signing
for plain SLH-DSA (A) and MTL with and without amortisation (B, C). (Measured)
`cargo xtask bench-all`

Signing performance · Batch speed-up. Medium dataset, C against A: **9,434 ×** (Derived) `[F-sign-medium-c-vs-a]`. Per-message cost multiplied by message count, not a whole-batch run. `cargo xtask deck-facts`

Verification performance · Per-message verify. Verification cost per message
under each configuration. (Measured) `cargo xtask bench-all`

Verification performance · Pre-validated ladder. With a verifier that trusts an
already-checked ladder, C against A is **77.34 ×** (Derived) `[F-verify-medium-c-vs-a]`. `cargo xtask deck-facts`

Wire efficiency · Wire bytes, Medium. A: **78,560,000 B** (Measured) `[F-wire-medium-a]`. B: **83,295,296 B** (Measured) `[F-wire-medium-b]`. C: **2,743,352 B** (Measured) `[F-wire-medium-c]`. `cargo xtask wire-report`

Wire efficiency · Wire reduction. C against A is **96.508 %** (Derived) `[F-wire-medium-c-vs-a]`, computed from the measured bytes. C against B is **96.706 %** (Derived) `[F-wire-medium-c-vs-b]`. `cargo xtask wire-report`

Wire efficiency · UDP payload fit. A closed-form model in climb-analytical. In configuration C, **11 of 12** modelled response profiles fit the payload (Analytical) `[F-udp-c-fit]` `[F-udp-c-total]`. `cargo xtask wire-report`

Memory analysis · Memory probe. Memory probes in climb-bench record real usage.
(Measured) `cargo xtask memory-report`

Memory analysis · Measured against model. Small dataset: measured memory is
**1.628 ×** the analytical estimate (Derived) `[F-mem-small-ratio]`. `cargo xtask memory-report`

Ladder visualisation · Ladder verification. Checks a recorded ladder fixture
from the data folder. (Measured) `cargo xtask ladder-verify`

Ladder visualisation · Terminal dashboard. climb-tui shows ladder shape, results
and the Integrity tab. It opens on committed sample results. (Measured) `cargo run -p climb-tui`

DNS packet replay · Recorded capture. Live response for `www.climb.example. A`:
condensed against full is **97.307 %** smaller (Derived) `[F-dns-www-a]`, baseline B. `cargo xtask dns-demo`

DNS packet replay · Local pipeline. Patched NSD and Unbound in local Docker
capture real wire bytes. Local only, no Internet deployment. (Measured) `cargo xtask dns-demo`

Integrity and provenance · Claims registry. Every claim embeds a JSON pointer
into the results, its expected value and a pass or fail mark. (Generated)
`cargo xtask report --check`

Integrity and provenance · Docs drift check. Fails if any number in the README
or the docs is not in the quote sheet. (Generated) `cargo xtask docs-check`

Integrity and provenance · Release bundle. Report, results, fixtures,
SHA256SUMS and a PROVENANCE.txt naming the commit. (Generated) `cargo xtask package`
<!-- copy:end -->

### How it works

<!-- copy:begin id=home-mechanism -->
Headline: **How it works**

Subhead: In C, one signed ladder is shared across a batch. Each response carries
only a small condensed authentication path, not the full ladder.

Tab label: **A · Plain SLH-DSA**. Tab label: **B · MTL, no amortisation**. Tab
label: **C · MTL amortised**.

Wire bytes, Medium dataset. A: **78,560,000 B** (Measured) `[F-wire-medium-a]`. B: **83,295,296 B** (Measured) `[F-wire-medium-b]`. C: **2,743,352 B** (Measured) `[F-wire-medium-c]`.

A heading: A: one full signature per RRset. Body: No ladder. The DNSSEC status
quo before any MTL wrapper. Diagram caption: one full signature each.

B heading: B: full MTL signature per message. Body: Every message carries its
condensed path joined to the whole ladder. Diagram caption: path plus full
ladder, repeated.

C heading: C: one ladder, many short paths. Body: The ladder is signed once for
the batch. Each message carries only its path. Diagram caption: shared ladder,
short path each.

Diagram labels: three message boxes · one signed ladder.
Legend: signature or ladder · condensed path.

Microcopy: The diagram is illustrative; the byte figures are measured. The
per-message sizes are read from the wire facts.
<!-- copy:end -->

### Architecture

<!-- copy:begin id=home-architecture -->
Headline: **Architecture**

Body: A Rust workspace links OpenSSL, liboqs and libMTL, benchmarks with
Criterion, and captures live DNS from patched NSD and Unbound in Docker.

Diagram labels: OpenSSL · liboqs · libMTL · Docker build · Rust workspace ·
Criterion benchmarks · NSD + Unbound · DNS capture · consolidate ·
climb-results.json · HTML report · TUI dashboard · deck-facts.md.

Diagram alt text: Native libraries and the DNS pipeline feed the Rust workspace,
which feeds consolidated results, the report, the dashboard and the quote sheet.
<!-- copy:end -->

### Transparency

<!-- copy:begin id=home-transparency -->
Headline: **Transparency**

Subhead: Nothing on this page is typed in by hand. Each number is read from a
recorded artifact and checked against a claims registry.

First panel · Reproducible datasets. Deterministic datasets in **3** sizes (Measured) `[F-wire-sizes]` and repeated trials. One command, `cargo xtask reproduce`, rebuilds the artifact chain.

Second panel · Pinned and fingerprinted. liboqs and libMTL are pinned submodules. `environment.json` records CPU, kernel and toolchain.

Third panel · Claims registry. Each claim stores its JSON pointer, expected value and pass or fail mark. The README badge reports **852 of 852** verified (Generated) `[F-claims-count]`.

Known experimental limits. Results are empirical. No formal security
assessment, verification or certification is claimed for MTL mode, SLH-DSA or
any component. They cover one parameter set
(**SLH-DSA-SHA2-128s-MTL-SHA2-128**, Measured) `[F-parameter-set]`, one reference
implementation and one laptop-class machine. Native SLH-DSA is not SIMD-tuned in
the pinned build. The DNS pipeline runs locally only.

Link: Read the methodology ↗
<!-- copy:end -->

### Team

<!-- copy:begin id=home-team -->
Headline: **Team CLIMB**

Subhead: AIORI-3 Hackathon, problem statement PS-036, team A3-PS036-TC529.

Body: Pratyay Mukherjee — Team Lead and Developer.

Body: Mohit Kumar Yadav — Lead Researcher, UI and Logo Designer.

Body: Dr. Maibam Devi — Faculty Mentor.

Microcopy: Contact and profile links are on the repository.
<!-- copy:end -->

## `/dashboard` — Dashboard

<!-- copy:begin id=dashboard-head -->
Eyebrow: `/02.00 Recorded results`

Headline: **The dashboard**

Subhead: Every recorded result, on one page, with its evidence class. Read
from the committed artifact; nothing here is typed in.

Dataset control, labels: **Small** · **Medium** · **Large**

View control, labels: **Total** · **Per message**

Microcopy: Switching the dataset re-renders the figures; it does not re-run
the benchmark.
<!-- copy:end -->

<!-- copy:begin id=dashboard-wire -->
Panel headline: **Wire bytes**

Body: Configuration A sends **78,560,000 B** (Measured) `[F-wire-medium-a]`.

Body: Configuration B sends **83,295,296 B** (Measured) `[F-wire-medium-b]`.

Body: Configuration C sends **2,743,352 B** (Measured) `[F-wire-medium-c]`.

Body: C against A is a **96.508 %** reduction (Derived) `[F-wire-medium-c-vs-a]`; C against B is **96.706 %** (Derived) `[F-wire-medium-c-vs-b]`.

Microcopy: Medium dataset. The bar for B is the full width; A and C are drawn
against it. A hatched bar means a modelled value.
<!-- copy:end -->

<!-- copy:begin id=dashboard-signing -->
Panel headline: **Signing**

Body: Batch signing is **9,434 ×** faster in C than in A on the Medium dataset (Derived) `[F-sign-medium-c-vs-a]`. The ratio compares the recorded plain per-message cost with the recorded MTL per-message cost.

Microcopy: Log scale. The shaded band is the recorded spread over five runs.
<!-- copy:end -->

<!-- copy:begin id=dashboard-verifying -->
Panel headline: **Verifying**

Body: With a verifier that trusts an already-checked ladder, verification in C
is **77.34 ×** faster than in A (Derived) `[F-verify-medium-c-vs-a]`.

Microcopy: This is the pre-validated-ladder case, not a cold verification.
<!-- copy:end -->

<!-- copy:begin id=dashboard-memory -->
Panel headline: **Memory**

Body: On the Small dataset the measured memory is **1.628 ×** the closed-form estimate (Derived) `[F-mem-small-ratio]`.

Microcopy: Measured is peak resident memory; the estimate is the library's own
page-size model plus the harness buffers. They are reported side by side, not
forced to agree.
<!-- copy:end -->

<!-- copy:begin id=dashboard-udp -->
Panel headline: **Does it fit a UDP packet?**

Body: In configuration C, **11 of 12** modelled response profiles fit the payload (Analytical) `[F-udp-c-fit]` `[F-udp-c-total]`.

Microcopy: The payload size is an EDNS(0) constant, not a measurement. The gauge marks the 512 B DNS limit and the **1,232 B** EDNS(0) payload (Constant) `[F-udp-payload]`. A modelled fit is not a captured packet.
<!-- copy:end -->

<!-- copy:begin id=dashboard-dns -->
Panel headline: **Live DNS**

Body: In the local capture, the response for `www.climb.example. A` is
**97.307 %** smaller condensed than full (Derived) `[F-dns-www-a]`.

Microcopy: One local capture over TCP, replayed from the committed file. Not
an Internet measurement.
<!-- copy:end -->

<!-- copy:begin id=dashboard-rtt -->
Panel headline: **Round trip at 50 ms**

Body: At an emulated 50 ms round trip, the plain response takes **201.056 ms** (Measured) `[F-dns-rtt-plain-50]` and the condensed response takes **50.475 ms** (Measured) `[F-dns-rtt-condensed-50]`.

Microcopy: Kernel network emulation on the local bridge, half the delay on each egress, **200** samples per cell (Constant) `[F-dns-rtt-samples]`. The plain response is truncated and retried over TCP; the condensed response fits one UDP exchange. This is a local testbed figure, not an Internet path.
<!-- copy:end -->

<!-- copy:begin id=dashboard-ladder-cache -->
Panel headline: **Ladder cache**

Body: After the first pass the resolver holds **16,912 B** of ladder state (Measured) `[F-ladder-cache-measured]` against a closed-form estimate of **8,120 B** (Analytical) `[F-ladder-cache-analytical]`, a ratio of **2.083 ×** (Derived) `[F-ladder-cache-ratio]`.

Microcopy: Measured is the ladder-cache delta from the resolver's own counters;
the estimate is one signed ladder per distinct ladder. They are reported side by
side, and the cache does not grow on the second pass.
<!-- copy:end -->

<!-- copy:begin id=dashboard-states -->
Empty state (no artifact): No results artifact was found. Run the reproduce
command, or open the committed sample.

Error state (artifact present but unreadable): The results artifact at the
expected path could not be parsed. The path is named below. Nothing on this
page is being guessed from a partial file.
<!-- copy:end -->

## `/calculator` — Break-even calculator

<!-- copy:begin id=calculator-head -->
Eyebrow: `/03.00 Break-even`

Headline: **Where does the ladder start to win?**

Subhead: Put your own message count against the recorded per-message costs and
see where configuration C crosses configuration A.

Input label: Messages per batch

Input label: Response profile

Button: **Compute**

Microcopy: The per-message costs and the byte totals are read from the
artifact; only your message count is your own input.
<!-- copy:end -->

<!-- copy:begin id=calculator-output -->
Result headline: **Your batch**

Body: At your message count, C sends **2,743,352 B** (Measured) `[F-wire-medium-c]` against A's **78,560,000 B** (Measured) `[F-wire-medium-a]`, a **96.508 %** reduction (Derived) `[F-wire-medium-c-vs-a]`.

Body: The recorded signing ratio is **9,434 ×** (Derived) `[F-sign-medium-c-vs-a]`, measured on the 10,000-message batch (Measured) `[F-sign-batch-size]`.

Body: The ladder pays for itself from reuse **2** onwards, by bytes (Derived) `[F-breakeven-bytes]` and by time (Derived) `[F-breakeven-signing]`.

Microcopy: The payload constant used by the fit model is **1,232 B** (Constant) `[F-udp-payload]`; it is a constant, not a measurement. Reuse means the number of responses that share one signed ladder.
<!-- copy:end -->

<!-- copy:begin id=calculator-states -->
Empty state (no input yet): Enter a message count to see the crossing.

Error state (count is zero or not a number): Enter a whole number of
messages, one or more.

Microcopy: The calculator is client-side. Your input is not sent anywhere.
<!-- copy:end -->

## `/report` — Report

<!-- copy:begin id=report-head -->
Eyebrow: `/04.00 The record`

Headline: **The full report**

Subhead: The rendered report and the machine-readable results artifact, from
the same recorded run.

Body: The run covers **3** dataset sizes (Measured) `[F-wire-sizes]` with the parameter set **SLH-DSA-SHA2-128s-MTL-SHA2-128** (Measured) `[F-parameter-set]`.

Button: **Download the results artifact**

Button: **Open the HTML report**

Microcopy: The artifact is the source; the HTML report is rendered from it.
Both are committed.
<!-- copy:end -->

<!-- copy:begin id=report-states -->
Empty state (no report built): No report has been rendered from this artifact
yet. The download is still available.

Error state (render failed): The report could not be rendered from the
artifact at the named path. The artifact itself is unchanged.
<!-- copy:end -->

## `/verify` — Verify

<!-- copy:begin id=verify-head -->
Eyebrow: `/05.00 Check it yourself`

Headline: **Verify the numbers**

Subhead: Every number the site shows is a claim in the registry. Each claim
stores its pointer, its expected value and its pass mark.

Body: The generated badge reports **852 of 852** claims verified (Generated) `[F-claims-count]`.

Button: **Copy the verify command**

Microcopy: Run the command locally to re-resolve every claim against the
committed artifact. The count is generated by the badge step, not typed in.
<!-- copy:end -->

<!-- copy:begin id=verify-claims-by-class -->
Panel headline: **Claims by evidence class**

Body: Each claim carries the class of the value it checks. The registry is
divided into measured, derived, analytical and extrapolated claims, and each
count is read from the generated claims document.

Microcopy: A claim's class is the class of the fact it checks, so the totals
add up to the badge count. The four counts are generated from the same document
that the badge reads.
<!-- copy:end -->

<!-- copy:begin id=verify-states -->
Empty state (registry absent): No claims registry is available for this
artifact.

Error state (a claim fails): A claim did not resolve to its recorded value.
The failing pointer is named. A failed claim is reported, not hidden.
<!-- copy:end -->

## `/reproduce` — Reproduce

<!-- copy:begin id=reproduce-head -->
Eyebrow: `/06.00 Rebuild it`

Headline: **Rebuild every number**

Subhead: The pinned environment and the commands that regenerate the artifacts
from a clean clone.

Body: The run uses **3** dataset sizes (Measured) `[F-wire-sizes]` and the parameter set **SLH-DSA-SHA2-128s-MTL-SHA2-128** (Measured) `[F-parameter-set]`.

Button: **Copy the command**

Microcopy: The commands run locally. The benchmark is not deployed on the
Internet; the DNS testbed runs in Docker on your machine.
<!-- copy:end -->

<!-- copy:begin id=reproduce-states -->
Empty state (no environment recorded): No environment fingerprint is present
for this checkout.

Error state (a pinned dependency is missing): A pinned dependency is not
present. Fetch the submodules and run the build again; the command is printed
with the failing step named.
<!-- copy:end -->

## `/method` — Method

<!-- copy:begin id=method-head -->
Eyebrow: `/07.00 Definitions`

Headline: **Method**

Subhead: The baselines, the evidence classes and the limits, in one place.

Body: The results cover **3** dataset sizes (Measured) `[F-wire-sizes]` with the parameter set **SLH-DSA-SHA2-128s-MTL-SHA2-128** (Measured) `[F-parameter-set]`.

Body: The UDP-fit model uses a **1,232 B** payload constant (Constant) `[F-udp-payload]`. It is an EDNS(0) size, not a measured packet.

Microcopy: A condensed response is a Merkle authentication path. A full
response is the whole signed ladder. A pre-validated ladder is one the
verifier has already checked.
<!-- copy:end -->

<!-- copy:begin id=method-classes -->
Headline: **Evidence classes**

Measured: read from a measurement artifact.

Derived: computed from other recorded values, such as a ratio or byte
arithmetic.

Analytical: from a closed-form model.

Extrapolated: unit cost times count, applied to a measured per-message cost
rather than a whole-batch measurement, shown with `≈`.
<!-- copy:end -->

## `/team` — Team

<!-- copy:begin id=team -->
Eyebrow: `/08.00 Who`

Headline: **Team CLIMB**

Subhead: AIORI-3 Hackathon, problem statement PS-036, team A3-PS036-TC529.

Body: Pratyay Mukherjee — Team Lead and Developer.

Body: Mohit Kumar Yadav — Lead Researcher, UI and Logo Designer.

Body: Dr. Maibam Devi — Faculty Mentor.

Microcopy: Contact and profile links are on the repository.
<!-- copy:end -->

## `/commits` — Commits

<!-- copy:begin id=commits -->
Eyebrow: `/09.00 Built in the open`

Headline: **Commits**

Subhead: The public commit graph, regenerated from the public history.

Alt text: A cumulative commit-count staircase with daily bars and milestone
flags, drawn from the public commit history.

Microcopy: The graph describes public commits only. It carries no e-mail
address and no claim that any milestone was met on time.
<!-- copy:end -->

## `/404`

<!-- copy:begin id=notfound -->
Eyebrow: `/404`

Headline: **That page is not here.**

Body: The address did not match a route. The dashboard and the method are the
two places most readers want.

Buttons: **Open the dashboard** · **Read the method**
<!-- copy:end -->

## Footer

<!-- copy:begin id=footer -->
Headline: **Sign once. Prove many.**

Tagline: Measured, not claimed.

Body: Clone it, rebuild every number, check the claims yourself.

Command: `git clone --recurse-submodules https://github.com/aiori-hackathon/A3-PS036-TC529 climb`

Button: **Copy**. Button: **Open the repository**

Keyword list: post-quantum · SLH-DSA · Merkle Tree Ladder · DNSSEC · amortisation · condensed authentication path · **1,232-byte** UDP payload (Constant) `[F-udp-payload]` · libMTL · liboqs · Criterion · reproducible · evidence classes · claims registry · NSD and Unbound · Rust

Column heading: CLIMB. Cryptographic Ladder Impact Measurement Benchmark. A
reproducible look at what Merkle Tree Ladder amortisation buys post-quantum
DNSSEC.

Column heading: Project. Links: GitHub repository · Presentation deck (PDF) ·
Introduction video.

Column heading: Method. Links: Research methodology · Baselines A, B, C ·
Documentation.

Column heading: Event. Links: AIORI-3 guidelines · Track 2, Cyber Security ·
PS-036 · A3-PS036-TC529.

Scope: This website presents recorded results and a client-side dashboard. The
DNSSEC and PQC benchmark itself is not deployed on the Internet; the DNS
testbed runs locally.

Legal: MIT licence.

Badge line: **852 of 852** claims verified (Generated) `[F-claims-count]`.

Microcopy: Experimental research benchmark. No formal security assessment or
certification is claimed.
<!-- copy:end -->

## Honest limits

<!-- copy:begin id=limits -->
Headline: **Honest limits**

Body: These results are empirical measurements from one parameter set
(**SLH-DSA-SHA2-128s-MTL-SHA2-128**, Measured) `[F-parameter-set]`, one reference
implementation and one laptop-class machine. They are not a formal
verification, an audit or a certification, and nothing here should be read as
one.

Body: The DNS pipeline runs locally, on a Docker testbed, not on the Internet.
The live-DNS figures come from a local capture, replayed from a committed
file. The round-trip figures come from kernel network emulation on the local
bridge.

Body: Signing and verification speed-ups are ratios of recorded per-message
costs. The UDP-fit rows are an analytical payload model, not a measurement.
The spread on a speed-up is the recorded spread over five runs, not a re-run
of the headline benchmark.

Body: This website presents recorded results and a client-side dashboard. The
DNSSEC and PQC benchmark itself is not deployed on the Internet; the DNS
testbed runs locally.
<!-- copy:end -->

## What we did not measure

<!-- copy:begin id=not-measured -->
Headline: **What we did not measure**

Body: No Internet latency. The DNS delay is a local network-emulation delay on
the Docker bridge, not a path across the Internet.

Body: No hardware-cache counters on the website. The optional CPU cache
counters are a Linux-only bonus metric and are not part of these figures.

Body: No multi-machine replication. The results come from one laptop-class
machine, and the machine is the environment the fingerprint records.

Body: No third-party audit and no certification. Nothing here has been
reviewed by anyone outside the team.

Body: No security property. The benchmark measures time and bytes; it does not
test whether MTL mode or SLH-DSA is hard to break, and it does not make that
kind of assertion.
<!-- copy:end -->
