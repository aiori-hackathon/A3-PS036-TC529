# Prototype copy inventory

Source: `docs/site/prototype/climb-site.html`
429 lines, 69,950 bytes, SHA-256 `0b0822338e004daba100bde1f98bcd9f2e5aa4c1f7b08198973d966a9570f108`.

This file records the prototype's visible strings by section, with the prototype's wording
unchanged. It is an input to the copy deck (`docs/site/copy.md`), not publishable copy.

**Method.** Static extraction only: an HTML parser over the served file plus a manual read of the
strings built in the inline scripts (the seven capability cards and the ladder leaf copy are
assembled from the `T` and `D` arrays, so they do not appear as text nodes). No browser was
launched, so nothing here is a statement about rendering, layout or assistive-technology output.

## Claim markers

Every row that makes a factual assertion carries a marker. Rows marked `—` are navigation,
labels, or interaction text and assert nothing about the project.

| Marker | Meaning |
| --- | --- |
| `M` | Measurement claim: a number, ratio or fit that must be backed by a recorded artifact. |
| `C` | Technical/capability claim: what the benchmark measures, links or contains. |
| `P` | Provenance/reproducibility claim: what is pinned, registered, checked or reproducible. |
| `S` | Scope or limits claim: what the results do and do not cover. |
| `X` | Security-adjacent phrasing: reads as a security property or benefit. |
| `—` | No claim (label, navigation, button, identifier). |

## Page metadata

| Line | String | Claim |
| --- | --- | --- |
| 4 | `CLIMB: Cryptographic Ladder Impact Measurement Benchmark` | — |
| 5 | `CLIMB is a reproducible Rust benchmark measuring how Merkle Tree Ladder amortisation changes the performance and wire efficiency of post-quantum DNSSEC signing with SLH-DSA.` | C, P |

## Header and page index

| Line | String | Claim |
| --- | --- | --- |
| 224 | `CLIMB home` (accessible name) | — |
| 225 | `Menu` | — |
| 226 | `Primary` (landmark label) | — |
| 226 | `Results` | — |
| 226 | `Capabilities` | — |
| 226 | `Ladder` | — |
| 226 | `How it works` | — |
| 226 | `Architecture` | — |
| 226 | `Transparency` | — |
| 226 | `Team` | — |
| 226 | `GitHub` | — |
| 227 | `Page index` (landmark label) | — |
| 227 | `0%` | — |

## Hero

| Line | String | Claim |
| --- | --- | --- |
| 230 | `CLIMB, Cryptographic Ladder Impact Measurement Benchmark` (logo accessible name) | — |
| 231 | `Post-quantum DNSSEC, measured.` | C |
| 232 | `Measuring how Merkle Tree Ladder amortisation changes the performance and wire efficiency of post-quantum DNSSEC signing with SLH-DSA.` | C |
| 233 | `Explore the benchmark` | — |
| 233 | `View on GitHub` | — |
| 234 | `AIORI-3 Hackathon, problem statement PS-036. Experimental research benchmark.` | S |

The hero string quoted in the audit brief, `The next layer of trust.`, does **not** occur anywhere
in the delivered file. The closest security-adjacent line is the footer headline
`Sign once. Prove many.` (line 290). See `docs/site/prototype-audit.md` §7.

## Results

| Line | String | Claim |
| --- | --- | --- |
| 236 | `Results` | — |
| 236 | `Every figure carries its evidence class and baseline. Values come from the repository’s recorded artifacts, Medium dataset.` | P |
| 238 | `Derived` | — |
| 238 | `96.508` `%` | M |
| 238 | `Wire reduction, C vs A. From measured bytes: 2,743,352 B vs 78,560,000 B.` | M |
| 239 | `Extrapolated ≈` | — |
| 239 | `≈` `9,405` `×` | M |
| 239 | `Signing speed-up, C vs A. Per-message cost scaled to batch size.` | M |
| 240 | `Extrapolated ≈` | — |
| 240 | `≈` `215.3` `×` | M |
| 240 | `Verification speed-up, C vs A, with a trusted cached ladder.` | M |
| 241 | `Analytical` | — |
| 241 | `11` `of 12` | M |
| 241 | `Modelled UDP response profiles that fit 1,232 bytes in configuration C.` | M |
| 242 | `Measured: recorded directly` | — (legend) |
| 242 | `Derived: computed from measurements` | — (legend) |
| 242 | `Analytical: closed-form model` | — (legend) |
| 242 | `Extrapolated: unit cost × count` | — (legend) |
| 243 | `Definitions in docs/METHODOLOGY.md. Full table in docs/deck-facts.md.` | P |

## Capabilities (seven cards)

Section intro, line 246:

| Line | String | Claim |
| --- | --- | --- |
| 246 | `Capabilities` | — |
| 246 | `Seven things the benchmark measures, each reproducible from one command. Select a card to open it in the ladder.` | C, P |

The cards are generated from `T` (lines 305–311) and `D` (line 332). Card copy below is the
rendered title, body and command for each card, in order; the `(a)`–`(g)` markers are generated
by line 333.

| Card | String | Claim |
| --- | --- | --- |
| (a) | `Signing performance` | — |
| (a) | `Criterion timings of SLH-DSA signing, with and without MTL.` | C |
| (a) | `cargo xtask bench-all` | P |
| (b) | `Verification performance` | — |
| (b) | `Per-message verification, including a trusted cached ladder.` | C |
| (b) | `cargo xtask bench-all` | P |
| (c) | `Wire efficiency` | — |
| (c) | `Wire-size and DNS packet analysis plus an analytical UDP-fit model.` | C |
| (c) | `cargo xtask wire-report` | P |
| (d) | `Memory analysis` | — |
| (d) | `Measured memory compared with a closed-form model.` | C |
| (d) | `cargo xtask memory-report` | P |
| (e) | `Ladder visualisation` | — |
| (e) | `Ladder shape and condensed paths in the terminal dashboard.` | C |
| (e) | `cargo run -p climb-tui` | P |
| (f) | `DNS packet replay` | — |
| (f) | `A recorded capture from patched NSD and Unbound, replayed locally.` | C, S |
| (f) | `cargo xtask dns-demo` | P |
| (g) | `Integrity and provenance` | — |
| (g) | `Claims registry, drift checks and a checksummed release bundle.` | C, P |
| (g) | `cargo xtask report --check` | P |
| 334 | `All seven capabilities are in the open repository.` | C |
| 334 | `View on GitHub ↗` | — |

## Explore the ladder

| Line | String | Claim |
| --- | --- | --- |
| 248 | `Explore the ladder` | — |
| 248 | `Choose a capability to open its branches, then a leaf to see its evidence class and the command that reproduces it.` | P |
| 249 | `CLIMB` `ladder` | — |
| 249 | `Capabilities` (group label) | — |
| 249 | `Branches` (group label) | — |

All 16 leaves, in the order the `T` array presents them (line 305–311). The evidence-class badge
is generated from `EV` (line 304).

| Group | Leaf | Leaf copy | Command | Badge | Claim |
| --- | --- | --- | --- | --- | --- |
| Signing performance | `SLH-DSA signing` | `Criterion benchmark groups time signing for plain SLH-DSA (A) and MTL with and without amortisation (B, C).` | `cargo xtask bench-all` | Measured | C |
| Signing performance | `Batch speed-up` | `Medium dataset, C vs A: about 9,405×. Per-message cost multiplied by message count, not a whole-batch run.` | `cargo xtask deck-facts` | Extrapolated ≈ | M |
| Verification performance | `Per-message verify` | `Verification cost per message under each configuration.` | `cargo xtask bench-all` | Measured | C |
| Verification performance | `Trusted cached ladder` | `With a verifier that trusts an already-checked ladder, C vs A is about 215.3×.` | `cargo xtask deck-facts` | Extrapolated ≈ | M |
| Wire efficiency | `Wire bytes, Medium` | `A: 78,560,000 B. B: 83,295,296 B. C: 2,743,352 B.` | `cargo xtask wire-report` | Measured | M |
| Wire efficiency | `Wire reduction` | `C vs A is 96.508%, computed from the measured bytes. C vs B is 96.706%.` | `cargo xtask wire-report` | Derived | M |
| Wire efficiency | `UDP payload fit` | `A closed-form model in climb-analytical. In configuration C, 11 of 12 modelled response profiles fit the 1,232-byte payload.` | `cargo xtask wire-report` | Analytical | M |
| Memory analysis | `Memory probe` | `Memory probes in climb-bench record real usage.` | `cargo xtask memory-report` | Measured | C |
| Memory analysis | `Measured vs model` | `Small dataset: measured memory is 1.90× the analytical estimate.` | `cargo xtask memory-report` | Derived | M |
| Ladder visualisation | `Ladder verification` | `Checks a recorded ladder fixture from the data folder.` | `cargo xtask ladder-verify` | (none) | C |
| Ladder visualisation | `Terminal dashboard` | `climb-tui shows ladder shape, results and the Integrity tab. It opens on committed sample results.` | `cargo run -p climb-tui` | (none) | C |
| DNS packet replay | `Recorded capture` | `Live response for www.climb.example. A: condensed vs full is a 97.307% reduction, baseline B.` | `cargo xtask dns-demo` | Derived | M |
| DNS packet replay | `Live pipeline` | `Patched NSD and Unbound in local Docker capture real wire bytes. Local only, no Internet deployment.` | `cargo xtask dns-demo` | (none) | C, S |
| Integrity and provenance | `Claims registry` | `Every claim embeds a JSON pointer into the results, its expected value and a pass or fail mark.` | `cargo xtask report --check` | (none) | P |
| Integrity and provenance | `Docs drift check` | `Fails if any number in the README or docs is not in the quote sheet.` | `cargo xtask docs-check` | (none) | P |
| Integrity and provenance | `Release bundle` | `Report, results, fixtures, SHA256SUMS and a PROVENANCE.txt naming the commit.` | `cargo xtask package` | (none) | P |

## How it works (A, B, C)

| Line | String | Claim |
| --- | --- | --- |
| 252 | `How it works` | — |
| 252 | `In C, one signed ladder is shared across a batch. Each response carries only a small condensed authentication path, not the full ladder.` | C |
| 253 | `Configurations` (tab-list label) | — |
| 253 | `A · Plain SLH-DSA` | — |
| 253 | `B · MTL, no amortisation` | — |
| 253 | `C · MTL amortised` | — |
| 255 | `Measured` | — |
| 255 | `Wire bytes, Medium dataset` | C |
| 256 | `A` `78,560,000 B` | M |
| 257 | `B` `83,295,296 B` | M |
| 258 | `C` `2,743,352 B` | M |
| 259 | `Diagram of what each of three messages carries in the selected configuration` (SVG accessible name) | C |
| 261 | `message 1` `message 2` `message 3` | — |
| 264 | `one signed ladder` | C |
| 265 | `signature or ladder` | — (legend) |
| 265 | `condensed path` | — (legend) |

Per-tab copy (from `HW`, line 337):

| Tab | Heading | Paragraph | Diagram caption | Claim |
| --- | --- | --- | --- | --- |
| A | `A: one full signature per RRset` | `No ladder. The DNSSEC status quo before any MTL wrapper.` | `one full signature each` | C |
| B | `B: full MTL signature per message` | `Every message carries its condensed path joined to the whole ladder.` | `path plus full ladder, repeated` | C |
| C | `C: one ladder, many short paths` | `The ladder is signed once for the batch. Each message carries only its path.` | `shared ladder, short path each` | C |

## Architecture

| Line | String | Claim |
| --- | --- | --- |
| 268 | `Architecture` | — |
| 268 | `A Rust workspace links OpenSSL, liboqs and libMTL, benchmarks with Criterion, and captures live DNS from patched NSD and Unbound in Docker.` | C |
| 269 | `Native libraries and DNS pipeline feed the Rust workspace, which feeds consolidated results, report, dashboard and quote sheet` (SVG accessible name) | C |
| 272 | `OpenSSL` `liboqs` `libMTL` `Docker build` | — (diagram labels) |
| 272 | `Rust workspace` `Criterion benchmarks` `NSD + Unbound` `DNS capture` | — (diagram labels) |
| 272 | `consolidate` `climb-results.json` `HTML report` `TUI dashboard` `deck-facts.md` | — (diagram labels) |

## Transparency

| Line | String | Claim |
| --- | --- | --- |
| 275 | `Transparency` | — |
| 275 | `Nothing on this page is typed in by hand. Each number is read from a recorded artifact and checked against a claims registry.` | P |
| 276 | `Reproducible datasets` | — |
| 276 | `Deterministic datasets in three sizes and repeated trials. One command, cargo xtask reproduce, rebuilds the artifact chain.` | C, P |
| 277 | `Pinned and fingerprinted` | — |
| 277 | `liboqs and libMTL are pinned submodules. environment.json records CPU, kernel and toolchain.` | P |
| 278 | `Claims registry` | — |
| 278 | `Each claim stores its JSON pointer, expected value and pass or fail mark. The README badge reports 771 of 771 verified.` | P, M |
| 279 | `Known experimental limits` | — |
| 279 | `Results are empirical. No formal security guarantee, proof or certification is claimed for MTL mode, SLH-DSA or any component. They cover one parameter set (SLH-DSA-SHA2-128s-MTL-SHA2-128), one reference implementation and one laptop-class machine. Native SLH-DSA is not SIMD-tuned in the pinned build. The DNS pipeline runs locally only.` | S |
| 280 | `Read the methodology ↗` | — |

## Team

| Line | String | Claim |
| --- | --- | --- |
| 283 | `Team CLIMB` | — |
| 283 | `AIORI-3 Hackathon, problem statement PS-036, team code A3-PS036-TC529.` | — (identifier) |
| 284 | `Pratyay Mukherjee` | — |
| 284 | `Team Lead and Developer` | C |
| 284 | `GitHub profile ↗` | — |
| 285 | `Mohit Kumar Yadav` | — |
| 285 | `Lead Researcher, UI and Logo Designer` | C |
| 285 | `GitHub profile ↗` | — |
| 286 | `Dr. Maibam Devi` | — |
| 286 | `Faculty Mentor` | C |
| 286 | `NFSU profile ↗` | — |

## Footer

| Line | String | Claim |
| --- | --- | --- |
| 290 | `Sign once. Prove many.` | X |
| 290 | `Clone it, rebuild every number, check the claims yourself.` | P |
| 290 | `git clone --recurse-submodules https://github.com/aiori-hackathon/A3-PS036-TC529 climb` | — |
| 290 | `Copy` | — |
| 290 | `Open the repository` | — |
| 291 | `Project keywords` (list label) | — |
| 291 | `post-quantum` | — |
| 291 | `SLH-DSA` | — |
| 291 | `Merkle Tree Ladder` | — |
| 291 | `DNSSEC` | — |
| 291 | `amortisation` | — |
| 291 | `condensed authentication path` | — |
| 291 | `wire bytes` | — |
| 291 | `1,232-byte UDP payload` | — (also appears as an analytical constant) |
| 291 | `libMTL` | — |
| 291 | `liboqs` | — |
| 291 | `Criterion` | — |
| 291 | `reproducible` | — |
| 291 | `evidence classes` | — |
| 291 | `claims registry` | — |
| 291 | `NSD and Unbound` | — |
| 291 | `Rust` | — |
| 292 | `CLIMB` | — |
| 292 | `Cryptographic Ladder Impact Measurement Benchmark. A reproducible look at what Merkle Tree Ladder amortisation buys post-quantum DNSSEC.` | C, P |
| 293 | `Project` | — |
| 293 | `GitHub repository` | — |
| 293 | `Presentation deck (PDF)` | — |
| 293 | `Introduction video` | — |
| 294 | `Method` | — |
| 294 | `Research methodology` | — |
| 294 | `Baselines A, B, C` | — |
| 294 | `Documentation` | — |
| 295 | `Event` | — |
| 295 | `AIORI-3 guidelines` | — |
| 295 | `Track 2, Cyber Security` | — |
| 295 | `PS-036 · A3-PS036-TC529` | — |
| 296 | `MIT licence. 771 of 771 claims verified.` | P, M |
| 296 | `Experimental research benchmark. No formal security guarantee is claimed.` | S |

## Summary

- Every text node, metadata value and accessible name extracted from the served HTML appears
  above, together with the 7 capability cards, 16 ladder leaves and 3 tab paragraphs that the
  inline script assembles.
- No visible string states or implies a formal security proof. One phrase (`Sign once. Prove
  many.`) is security-adjacent because "prove" can be misread as proof rather than demonstration.
- The five measurement labels the prototype marks `Extrapolated ≈` are classified in
  `docs/site/numbers-prototype.md`; two of them are stale and two are the wrong evidence class.
