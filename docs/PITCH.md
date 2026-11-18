# Pitch and run of show

**One sentence.** CLIMB measures what Merkle Tree Ladder mode actually saves when
post-quantum DNSSEC signs a zone, and what it costs in return: signing time,
verification time, bytes on the wire and cache use, each measured against the
same SLH-DSA workload signed without amortisation.

The problem statement asks for one reference implementation, one supported
algorithm, three dataset sizes, repeated trials on a pinned environment, and a
comparison with and without MTL. The run of show below is the same answer at two
lengths. Every figure quoted here is printed on
[`EVIDENCE-SHEET.html`](EVIDENCE-SHEET.html), generated from the committed
artifact, and every proving command is listed in
[`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md).

**Speakers.** **Pratyay** is Pratyay Mukherjee; **Mohit** is Mohit Kumar Yadav.
Pratyay owns the framing, the evidence chain and the limits. Mohit owns the
harness, the configurations and the live pipeline. Swap individual beats by
agreement, but never leave a beat unowned.

## Before the clock

From the travel kit:

```bash
sh verify.sh
sh serve.sh
./bin/climb-tui --results report/climb-results.json
```

From a source checkout:

```bash
cargo xtask evidence-sheet --check
cargo xtask sample-check
cargo xtask demo --plan
./target/release/climb-tui --results docs/sample-output/climb-results.json
```

`cargo xtask demo` arranges the whole demonstration; `demo/preflight.sh` prints a
GO/NO-GO table before it. If any check fails, switch plans in
[`DEMO-FALLBACKS.md`](DEMO-FALLBACKS.md) rather than improvising.

## The five-minute run

| Clock | Beat | Speaker | PS-036 expectation | Screen, or the command that shows it |
| --- | --- | --- | --- | --- |
| 0:00–0:20 | Open on the dashboard and ask the operator's question: can a zone afford to publish SLH-DSA? | Pratyay | R3, R4 | Overview tab; `./target/release/climb-tui --results docs/sample-output/climb-results.json` |
| 0:20–0:50 | One tool, one algorithm, three configurations: plain SLH-DSA, MTL without amortisation, MTL amortised. | Mohit | R1, R2, R4 | Signing tab; `cargo xtask bench-all` |
| 0:50–1:40 | Bytes: the figure that needs no cache. At Medium, C sends 2,743,352 B against A's 78,560,000 B, a 96.508 % reduction. | Pratyay | R6, O7 | Wire tab; `cargo xtask wire-report` |
| 1:40–2:25 | Signing and verification: 9,434 × signing and 77.34 × cached verification at Medium, with the spread column visible. | Mohit | R5, O5, O6 | Signing and Verifying tabs; `cargo xtask bench-all`, `cargo xtask stability` |
| 2:25–2:55 | The UDP gauge: 11 of 12 modelled responses fit in one packet for C, and the row is labelled analytical, not measured. | Pratyay | O7 | Wire tab, UDP gauge; `cargo xtask wire-report` |
| 2:55–3:35 | The live DNS pipeline, recorded: 24,438 B full against 658 B condensed, a 97.307 % reduction. Say that it is a local capture over TCP. | Mohit | R11, R12 | Live tab; `cargo xtask dns-demo` |
| 3:35–4:15 | Cache measured beside the analytical model, never merged. The answer cache is identical because the reference resolver serves full responses in both modes. | Pratyay | R8 | Memory tab; `cargo xtask dns-cache` |
| 4:15–4:40 | Proof that the numbers travel: the report's claims agree with the artifact, and the pins agree with the toolchain. | Mohit | O9, O1 | Integrity and Method tabs; `cargo xtask sample-check`, `cargo xtask check-pins` |
| 4:40–5:00 | The limits, then the repository address and the evidence sheet on the table. | Pratyay | R13, D1 | Method tab; [`EVIDENCE-SHEET.html`](EVIDENCE-SHEET.html) |

## The ten-minute run

The longer run keeps the same spine and adds the method, the ladder and the
research status. The five-minute beats expand rather than disappear.

| Clock | Beat | Speaker | PS-036 expectation | Screen, or the command that shows it |
| --- | --- | --- | --- | --- |
| 0:00–0:40 | What PS-036 asked, and the one sentence above. Open on Overview. | Pratyay | R1, R3, R4 | Overview tab; `./target/release/climb-tui --results docs/sample-output/climb-results.json` |
| 0:40–1:30 | Method: same tool, key and dataset for A, B and C; only batching differs. Explain A, B and C before any number. | Mohit | R2, R4, O3 | Method and Signing tabs; `docs/BASELINES.md`, `cargo xtask bench-all` |
| 1:30–2:40 | Wire bytes, per size, with the condensed length range. At Medium: 78,560,000 B for A, 83,295,296 B for B, 2,743,352 B for C, a 96.508 % reduction against A. | Pratyay | R6, O7 | Wire tab; `cargo xtask wire-report` |
| 2:40–3:50 | Signing and verifying, cold against cached. Cached verification is 77.34 × against A; the ladder re-validated row is 0.9236 ×, and saying so is the point. | Mohit | R5, O5, O6 | Signing and Verifying tabs; `cargo xtask stability`, `cargo xtask ladder-verify` |
| 3:50–4:40 | The ladder itself: pick N and a leaf, see the authentication path. At N=100 the signed ladder buffer is 132 B and the shape is three rungs. | Pratyay | R4, O3 | Ladder tab; `cargo xtask ladder-verify` |
| 4:40–5:40 | Cache: Unbound's own accounting and container RSS on one side, the libMTL page model on the other. The answer cache is equal by construction; the ladder cache is separate. | Mohit | R8, O8 | Memory tab; `cargo xtask dns-cache` |
| 5:40–6:40 | Live DNS: sign the zone, query it, capture both shapes. 24,438 B full against 658 B condensed, 97.307 %. The pipeline is local-only and the capture is over TCP. | Pratyay | R11, R12 | Live tab; `cargo xtask dns-demo` |
| 6:40–7:30 | Emulated RTT: what a delay on the local bridge does to the query, and why this is not Internet latency. | Mohit | R5, O6 | Live tab, latency rows; `cargo xtask dns-rtt` |
| 7:30–8:20 | The report proves itself: every claim has a JSON pointer, and the drift check fails when two presentations disagree. | Pratyay | O9 | Integrity tab; `cargo xtask sample-check` |
| 8:20–9:00 | Environment and reproduction: the fingerprint, the pins, and the one command that reruns the chain. | Mohit | R7, O1, O9 | Method tab; `cargo xtask pin-env`, `cargo xtask check-pins`, `cargo xtask reproduce` |
| 9:00–9:35 | Research status: experimental Internet-Drafts, a library that mixes draft revisions, and provisional code points. No standard and no security proof is claimed. | Pratyay | R13, D2 | [`RESEARCH.md`](RESEARCH.md), [`COMPLIANCE.md`](COMPLIANCE.md) |
| 9:35–10:00 | What is still partial, then the repository address and the evidence sheet. | Mohit | D1, D3 | [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md), [`EVIDENCE-SHEET.html`](EVIDENCE-SHEET.html) |

## Lines that must be said

These four sentences keep the demonstration honest even when a beat is cut:

- "This is a measurement of cost, not a security proof."
- "Amortisation needs a batch; at one message per batch there is nothing to amortise."
- "The wire figure is counted bytes; the UDP-fit figure is a model."
- "The live DNS result is a local capture over TCP, not Internet latency."

## What the judges should leave with

1. The problem statement's measurement matrix was run, not described: three
   sizes, three configurations, repeated trials on a pinned host.
2. The inconvenient findings are on the screen: cached verification is fast,
   cold verification is not; the reference resolver's answer cache is equal in
   both modes; the UDP gain is a model.
3. Every number can be traced to a JSON pointer and reproduced by a command in
   [`CRITERIA-AUDIT.md`](CRITERIA-AUDIT.md).
4. The submission states its limits, including the partial rows for the PDF
   template and the mentor session.

The travel kit's packing and restore steps are in [`KIT.md`](KIT.md); the
questions and evidence-backed answers are in [`QA.md`](QA.md); failure recovery
is in [`DEMO-FALLBACKS.md`](DEMO-FALLBACKS.md).
