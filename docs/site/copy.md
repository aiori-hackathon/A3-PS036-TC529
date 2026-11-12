# Site copy deck

Derived from `docs/site/copy-prototype.md` (the prototype's strings) and
`docs/site/numbers-prototype.md` (its numbers). This is the copy the site should ship; it is
not yet implemented.

## The W0 rules are missing

The brief refers to "the W0 rules" for banned words, fact ids and the scope sentence. **No W0
document exists in this repository.** A search for `W0`, `banned`, `scope sentence` and `copy
deck` outside `target/` and `third_party/` returns no policy file, and the project progress log has
no W0 entry. Rather than invent a rulebook and call it W0, this deck applies rules derived from
evidence that does exist:

| Rule | Source | How it is applied here |
| --- | --- | --- |
| No security claim, proof or certification | `README.md:506–507`; `docs/deck-facts.md:18–19` | banned-word list below; the disclaimer is kept verbatim |
| Every number is read from a recorded artifact | `README.md:85–92`; `docs/deck-facts.md:3–6` | every numeric slot carries a fact id from `docs/site/numbers-prototype.md` |
| Evidence class travels with the number | `README.md:87–91`; `docs/deck-facts.md:8–13` | class is shown next to each figure; `≈` only for `EXTRAPOLATED` |
| Do not present a model as a measurement | `README.md:119` | the UDP-fit row stays `ANALYTICAL`; the DNS rows keep their local-capture scope |
| A website is a viewer, not a deployment | `README.md:509–511` | no live service, no implied deployment; the DNS card says "local only" |
| No hardcoded results in source | project coding rules | the deck gives fact ids, not literals for the implementer to retype |

If a W0 policy is later added, reconcile it against this table and record the difference rather
than silently re-styling the deck.

## Banned words and phrases

| Do not use | Reason | Use instead |
| --- | --- | --- |
| secure, security, secured | implies a property the benchmark does not test | name the measured behaviour: "condensed path", "smaller response", "fewer round trips" |
| proof, proves, proven | reads as formal verification | "measures", "records", "shows" |
| guarantee, guaranteed | absolute | "reports", "records" |
| trust, trusted (unqualified) | hides a verifier assumption | "pre-validated ladder (the verifier trusts the cached ladder)" |
| unbreakable, quantum-proof, future-proof, hack-proof | marketing absolute | delete |
| certified, complies, compliant | implies audit/certification | "measured against", "documented in" |
| eliminates, zero-risk, 100 %, always, never fails | absolute | give the measured ratio and its scope |
| up to (as a headline multiplier) | cherry-picks the best case | state the dataset and the baseline |
| revolutionary, seamless, effortless, world-class, best-in-class, cutting-edge, game-changing, unprecedented, military-grade | unverifiable praise | delete |
| "the next layer of trust" | security framing; also absent from the delivered prototype | one of the two hero alternatives below |

Words that are allowed but must be defined once: **claim** (a numeric assertion in the report,
not a security claim), **evidence class**, **trust cached** (always as the defined verifier
assumption), **condensed path**.

## Keep / change / add by section

### Hero

| Keep | Change | Add |
| --- | --- | --- |
| `Post-quantum DNSSEC, measured.` | nothing in the H1 | a one-line "What this page is" note: recorded results viewer, not a service |
| the lede's mechanism wording | replace "changes the performance and wire efficiency" with the concrete metrics when the site has room | link to the methodology next to the lede |
| `Explore the benchmark`, `View on GitHub` | — | — |
| `AIORI-3 Hackathon, problem statement PS-036.` | drop "Experimental research benchmark." from the meta line if the scope block is visible | scope block link |

### Results

| Keep | Change | Add |
| --- | --- | --- |
| four-row layout and evidence labels | `≈ 9,405 ×` → `9,434 ×`, class `Derived` | the scope sentence verbatim (below) |
| `96.508 %`, the byte totals, `11 of 12`, `1,232 B` | `≈ 215.3 ×` → `77.34 ×`, class `Derived` | a "read from the artifact at build time" provenance line |
| evidence legend | "Per-message cost scaled to batch size" → "Ratio of recorded per-message costs" | link to `docs/deck-facts.md` as the full table |
| `Every figure carries its evidence class and baseline.` | "Values come from the repository's recorded artifacts, Medium dataset." → "Values are read from the recorded artifacts; this page shows the Medium dataset." | — |

### Capabilities (seven cards)

| Keep | Change | Add |
| --- | --- | --- |
| all seven titles and the one-command promise | "Seven things the benchmark measures" → "Seven capabilities, each with the command that reproduces it" | a note that the commands run locally |
| the seven commands | tighten card bodies to one sentence each | a link from card (f) to the local-only scope |
| the `(a)`–`(g)` markers | move "All seven capabilities are in the open repository" out of the card grid | — |

### Explore the ladder

| Keep | Change | Add |
| --- | --- | --- |
| the instruction sentence | "Choose a capability to open its branches" → "Choose a capability, then a leaf" | keyboard hint: "Use Tab to move; Enter or Space to open" |
| the 16 leaves and their commands | `9,405 ×` → `9,434 ×`; `215.3 ×` → `77.34 ×`; `1.90 ×` → `1.628 ×`; remove the `Extrapolated` badge from both Medium speed-ups | a visible label for the canvas: "Diagram: selected capability and branch" (the drawing itself stays decorative) |
| evidence badges | "Trusted cached ladder" → "Pre-validated cached ladder" | — |

### How it works (A, B, C)

| Keep | Change | Add |
| --- | --- | --- |
| A/B/C tab names and the three explanations | "In C, one signed ladder is shared across a batch" is accurate; keep | "The diagram is illustrative; the byte bars are measured." |
| the byte bars and their labels | replace `data-w` hardcoded percentages with values derived from the byte facts | a per-tab accessible description that changes with the tab |
| `signature or ladder`, `condensed path` legend | — | — |

### Architecture

| Keep | Change | Add |
| --- | --- | --- |
| one sentence naming the workspace, OpenSSL, liboqs, libMTL, Criterion, NSD/Unbound | cut the repository-map restatement; the README already owns it | a link to `docs/ARCHITECTURE.md` |
| the diagram | keep the crate/binary names exactly as the workspace spells them | an accessible long description or a table equivalent |

### Transparency

| Keep | Change | Add |
| --- | --- | --- |
| "Reproducible datasets" and `cargo xtask reproduce` | "Nothing on this page is typed in by hand" → "No result on this page is typed in by hand" (only true once the site reads the artifact) | a "What this is not" block |
| "Pinned and fingerprinted" | — | link to `environment.json` |
| "Claims registry" | `771 of 771` → the generated count (`852 of 852` at audit time) | a sentence defining "claim" as a numeric assertion |
| "Known experimental limits" | do not restate the README's full limits list; keep three sentences and link | the verbatim security disclaimer |
| the methodology link | — | the scope sentence, or a link back to it |

### Team

| Keep | Change | Add |
| --- | --- | --- |
| names, roles, profile links | — | a "team code" line if the event needs it |
| — | do not duplicate the README table by hand if it can be generated from one source | — |

### Footer

| Keep | Change | Add |
| --- | --- | --- |
| the clone command, `Copy`, `Open the repository` | `Sign once. Prove many.` → one of the alternatives below | the `Measured, not claimed.` tagline from the README |
| the keyword list | drop `1,232-byte UDP payload` from the keyword list; it is a model constant, not a capability | — |
| the project/method/event link columns | `771 of 771 claims verified` → generated count or no count | the MIT licence line, which matches the repository's `LICENSE` |

## Final copy deck

Annotations in `[F-…]` are fact ids; they are not rendered. Every number below is a slot, not a
literal for the implementer to type.

### Hero

> **Post-quantum DNSSEC, measured.**
>
> Measuring how Merkle Tree Ladder amortisation changes the performance and wire efficiency of
> post-quantum DNSSEC signing with SLH-DSA.
>
> [Explore the benchmark] [View on GitHub]
>
> AIORI-3 Hackathon, problem statement PS-036. Experimental research benchmark.

### Results

> **Results**
>
> Every figure carries its evidence class and baseline. Values are read from the recorded
> artifacts; this page shows the Medium dataset.
>
> **96.508 %** — Derived. Wire reduction, C vs A. From measured bytes: 2,743,352 B vs
> 78,560,000 B. `[F-wire-medium-c-vs-a]`
>
> **9,434 ×** — Derived. Signing speed-up, C vs A. Ratio of the recorded plain per-message cost
> to the recorded MTL per-message cost. `[F-sign-medium-c-vs-a]`
>
> **77.34 ×** — Derived. Verification speed-up, C vs A, with a pre-validated cached ladder.
> `[F-verify-medium-c-vs-a]`
>
> **11 of 12** — Analytical. Modelled UDP response profiles that fit the 1,232-byte payload in
> configuration C. `[F-udp-c-fit]`
>
> Measured: recorded directly · Derived: computed from measurements · Analytical: closed-form
> model · Extrapolated: unit cost × count, shown with `≈`.
>
> Definitions in `docs/METHODOLOGY.md`. Full table in `docs/deck-facts.md`.

**Scope and limits** (the README's scope sentence, verbatim):

> **Scope.** Live DNS bytes were measured over TCP in a local capture; the UDP-fit rows are an
> analytical payload model, not a measurement; signing and verifying speed-ups are derived from
> per-message measurements. The DNS-latency rows are a `tc netem` delay on the local Docker
> bridge, not Internet latency, and the Unbound control served the full response in both modes,
> so its multiple is not a condensed win. No row in this table is extrapolated by the
> `N × per-message` identity; the OneK and Large comparisons in the artifact are. The spread
> column is the median (min–max) of the per-run primitive model ratio over n=5 runs, not a re-run
> of the headline benchmark.

> Results are empirical measurements from one parameter set
> (SLH-DSA-SHA2-128s-MTL-SHA2-128), one reference implementation and one laptop-class machine.
> No formal security guarantee, proof or certification is claimed for MTL mode, SLH-DSA or any
> component here.

### Capabilities

> **Capabilities**
>
> Seven capabilities, each with the command that reproduces it. Select a card to open it in the
> ladder. Every command runs locally.
>
> **(a) Signing performance.** Criterion timings of SLH-DSA signing, with and without MTL.
> `cargo xtask bench-all`
>
> **(b) Verification performance.** Per-message verification, including a pre-validated cached
> ladder. `cargo xtask bench-all`
>
> **(c) Wire efficiency.** Wire-size and DNS packet analysis plus an analytical UDP-fit model.
> `cargo xtask wire-report`
>
> **(d) Memory analysis.** Measured memory compared with a closed-form model.
> `cargo xtask memory-report`
>
> **(e) Ladder visualisation.** Ladder shape and condensed paths in the terminal dashboard.
> `cargo run -p climb-tui`
>
> **(f) DNS packet replay.** A recorded capture from patched NSD and Unbound, replayed locally.
> `cargo xtask dns-demo`
>
> **(g) Integrity and provenance.** Claims registry, drift checks and a checksummed release
> bundle. `cargo xtask report --check`
>
> All seven capabilities are in the open repository. [View on GitHub ↗]

### Explore the ladder

> **Explore the ladder**
>
> Choose a capability, then a leaf, to see its evidence class and the command that reproduces
> it. Use Tab to move; Enter or Space to open.
>
> *Diagram: the selected capability, its branches, and the selected leaf.*

Leaf copy, corrected:

| Leaf | Copy |
| --- | --- |
| Batch speed-up | Medium dataset, C vs A: `9,434 ×` `[F-sign-medium-c-vs-a]`. Ratio of recorded per-message costs. |
| Trusted cached ladder | With a verifier that trusts an already-checked ladder, C vs A is `77.34 ×` `[F-verify-medium-c-vs-a]`. |
| Measured vs model | Small dataset: measured memory is `1.628 ×` the analytical estimate `[F-mem-small-ratio]`. |
| Recorded capture | Live response for `www.climb.example. A`: condensed vs full is a `97.307 %` reduction, baseline B `[F-dns-www-a]`. |

All other leaves keep the prototype's wording from `docs/site/copy-prototype.md`; their numbers
already match the artifacts.

### How it works

> **How it works**
>
> In C, one signed ladder is shared across a batch. Each response carries only a small condensed
> authentication path, not the full ladder. The diagram is illustrative; the byte bars are
> measured.
>
> **A · Plain SLH-DSA.** No ladder. The DNSSEC status quo before any MTL wrapper.
>
> **B · MTL, no amortisation.** Every message carries its condensed path joined to the whole
> ladder.
>
> **C · MTL amortised.** The ladder is signed once for the batch. Each message carries only its
> path.
>
> Wire bytes, Medium dataset: A `78,560,000 B` `[F-wire-medium-a]`, B `83,295,296 B`
> `[F-wire-medium-b]`, C `2,743,352 B` `[F-wire-medium-c]`.

### Architecture

> **Architecture**
>
> A Rust workspace links OpenSSL, liboqs and libMTL, benchmarks with Criterion, and captures
> live DNS from patched NSD and Unbound in Docker. The full repository map is in
> `docs/ARCHITECTURE.md`.

### Transparency

> **Transparency**
>
> No result on this page is typed in by hand. Each number is read from a recorded artifact and
> checked against a claims registry.
>
> **Reproducible datasets.** Deterministic datasets at three sizes with repeated trials. One
> command, `cargo xtask reproduce`, rebuilds the artifact chain.
>
> **Pinned and fingerprinted.** liboqs and libMTL are pinned submodules. `environment.json`
> records CPU, kernel and toolchain.
>
> **Claims registry.** Each claim stores its JSON pointer, expected value and pass/fail mark.
> The generated badge currently reports `852 of 852` claims verified `[F-claims-count]`.
>
> **Known experimental limits.** Results are empirical. They cover one parameter set
> (SLH-DSA-SHA2-128s-MTL-SHA2-128), one reference implementation and one laptop-class machine.
> Native SLH-DSA is not SIMD-tuned in the pinned build. The DNS pipeline runs locally only. The
> full list is in the README's "Honest limits" section.
>
> **What this is not.** This page is a viewer for recorded results, not a DNS service, a signing
> service or a shell. No formal security guarantee, proof or certification is claimed.
>
> [Read the methodology ↗]

### Team

> **Team CLIMB**
>
> AIORI-3 Hackathon, problem statement PS-036, team code A3-PS036-TC529.
>
> **Pratyay Mukherjee** — Team Lead and Developer — GitHub profile ↗
>
> **Mohit Kumar Yadav** — Lead Researcher, UI and Logo Designer — GitHub profile ↗
>
> **Dr. Maibam Devi** — Faculty Mentor — NFSU profile ↗

### Footer

> **One signed ladder. Many short paths.**
>
> Clone it, rebuild every number, check the claims yourself.
>
> `git clone --recurse-submodules https://github.com/aiori-hackathon/A3-PS036-TC529 climb`
> [Copy] [Open the repository]
>
> *Measured, not claimed.*

Alternative headline if a shorter line is wanted: **`Sign a batch. Measure the difference.`**
Neither alternative claims anything about security.

## Number slots

The implementer renders these from the artifact; the deck never carries the digits as source.

| Slot | Fact id | Class | Value at audit time |
| --- | --- | --- | --- |
| Wire reduction, C vs A | `F-wire-medium-c-vs-a` | DERIVED | 96.508 % |
| A wire bytes | `F-wire-medium-a` | MEASURED | 78,560,000 B |
| B wire bytes | `F-wire-medium-b` | MEASURED | 83,295,296 B |
| C wire bytes | `F-wire-medium-c` | MEASURED | 2,743,352 B |
| Wire reduction, C vs B | `F-wire-medium-c-vs-b` | DERIVED | 96.706 % |
| Signing speed-up, C vs A | `F-sign-medium-c-vs-a` | DERIVED | 9,434 × |
| Verifying speed-up, cached, C vs A | `F-verify-medium-c-vs-a` | DERIVED | 77.34 × |
| UDP fit, configuration C | `F-udp-c-fit` | ANALYTICAL | 11 of 12 |
| UDP payload constant | `F-udp-payload` | model constant | 1,232 B |
| Live DNS reduction | `F-dns-www-a` | DERIVED | 97.307 % |
| Small memory ratio | `F-mem-small-ratio` | DERIVED | 1.628 × |
| Claims verified | `F-claims-count` | generated | 852 of 852 |
| Dataset count | `F-wire-sizes` | MEASURED | 3 |
| Parameter set | `F-parameter-set` | recorded | SLH-DSA-SHA2-128s-MTL-SHA2-128 |
