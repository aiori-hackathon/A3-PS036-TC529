# Demonstration walkthrough

One page, in the order a mentor would want to see it: start the dashboard,
then one stop per pseudocode block in [`PSEUDOCODE.md`](PSEUDOCODE.md). The
dashboard reads the committed results artifact, so every tab below is
populated without re-running anything; the commands beside each stop
re-produce the same numbers from source when a live run is wanted.

## Start

```bash
cargo run --release -p climb-tui --no-default-features
```

The dashboard opens on the **Overview** tab, which shows the headline table:
signing speed-up at Medium of 9,434 × for the amortised configuration against
plain SLH-DSA, verifying speed-up of 77.34 × with a trusted cached ladder, and
the wire reduction rows. Keys `1` through `9` switch tabs.

## Stop one — signing a batch with one MTL ladder

Show the **Ladder** tab and the **Signing** tab.

- **Ladder** shows the measured ladder shape: at N=100 the signed ladder
  buffer is 132 B, three rungs — the structure the whole amortisation rests
  on, parsed back from the library's own serialized bytes.
- **Signing** shows per-size signing time for the amortised batch against
  the non-amortised baselines, including the 9,434 × speed-up at Medium.

Live command (slow, runs every benchmark group):

```bash
cargo xtask bench-all
```

## Stop two — verifying, cold and cached

Show the **Verifying** tab: verification time per dataset size for the full
ladder, the non-MTL baseline, and the trusted cached ladder, including the
77.34 × cached-ladder speed-up at Medium.

Live command:

```bash
cargo xtask ladder-verify
```

## Stop three — the benchmark loop, bytes counted

Show the **Wire** tab: bytes on the wire per dataset size for the three
configurations — at Medium, 78,560,000 B for plain SLH-DSA, 83,295,296 B for
MTL without amortisation, 2,743,352 B for the amortised ladder, a 96.508 %
reduction for the amortised configuration against plain.

Live command:

```bash
cargo xtask wire-report
```

## Stop four — the DNS wire flow, full versus condensed

Show the **Live** tab: the recorded DNS pipeline run — one A query answered
with the full signed ladder (24,438 B) and the same query answered with a
condensed proof once the ladder hash is attached (658 B), a 97.307 %
reduction.

Live command (brings up the local-only NSD/Unbound pipeline):

```bash
cargo xtask dns-demo
```

## Close — everything agrees

Show the **Integrity** tab: the claims drift check that fails when any two
presentations of the same number disagree, the artifact inventory (the
results file as reference, its generated outputs and inputs), and the
provenance block naming where each recorded artifact came from.
