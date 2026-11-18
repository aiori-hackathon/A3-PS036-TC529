# CLIMB — the one-page pitch

**One sentence.** CLIMB measures what Merkle Tree Ladder mode actually saves when
post-quantum DNSSEC signs a zone, and what it costs in return: signing time,
verification time, bytes on the wire and cache use, each measured against the
same SLH-DSA workload signed without amortisation.

## The problem

DNSSEC signs every RRset on its own. With SLH-DSA that means one large
signature per record, and the operability question is not whether the signature
is standard — it is whether a zone operator can afford to publish it. MTL mode
changes the shape of the answer: a batch of messages shares one Merkle tree, so
most responses carry a small authentication path and only occasionally does a
response carry a freshly signed ladder. The drafts describe the mechanism. They
do not tell a zone operator what it saves in practice, or where it stops
helping.

## What CLIMB does

- Signs the same deterministic datasets three ways — plain SLH-DSA, MTL without
  amortisation, and MTL with one shared ladder — and measures all three with the
  same harness and the same pinned environment.
- Counts real bytes rather than reasoning about them, from the library's own
  serialized condensed signatures and signed ladders, and cross-checks the
  closed-form ladder model against the measured trees.
- Measures cache use from the resolver's own accounting and from process RSS,
  keeps the analytical model on a separate row, and never merges the two.
- Reports the whole chain on a dashboard, a self-checked HTML report, a quote
  sheet and a static site, all reading the same recorded artifact.

## What it does not claim

No new cryptography is implemented; the pinned libMTL and liboqs trees are used
as supplied. The DNS resolver and authoritative server are the patched
reference forks. Every run is local. Nothing here is a security proof, a formal
verification or a certification — it is a measurement of cost, with the
artifacts and commands that reproduce it.

## Why it is credible

The interesting findings are the inconvenient ones: the ladder model agrees
with the library, but verification speed-up needs a cached ladder to show its
full benefit; the resolver's answer cache is identical between the two
configurations because the reference patched resolver serves the full response
in both; and the smallest gains land where the response already fits one UDP
packet. Those are reported as measured, with the command that proves them.

## The demo in three commands

```bash
cargo xtask demo --plan      # what the four panes will show
cargo xtask demo             # dashboard, verification trio, site, cheat sheet
cargo xtask demo --offline   # the same, with every non-local request blocked
```

The travel kit on the USB drive needs no build and no network: verify it, serve
the site, open the report, run the dashboard. The packing and restore steps are
in [`KIT.md`](KIT.md); the five hardest questions and their evidence-backed
answers are in [`QA.md`](QA.md).
