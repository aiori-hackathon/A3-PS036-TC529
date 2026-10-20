# Signing baselines

This suite measures Merkle Tree Ladder (MTL) amortisation for post-quantum
DNSSEC signing (SLH-DSA). Three configurations are compared throughout; the
labels below are the ones the report, charts, TUI and README use.

## What each setting signs

| Setting | What is signed | Wire bytes |
|---|---|---|
| **MTL amortised** | One Merkle tree ladder is signed once for the whole batch; each message then carries only a condensed authentication path. | `ladder + Σ condensed_i` |
| **No amortisation** | Every message carries a complete MTL signature: its condensed path concatenated with the ladder. | `Σ condensed_i + N × ladder` |

Both settings sign the same deterministic message set. The only difference is
how many times the shared ladder is counted: once for the batch (amortised) or
once per message (non-amortised). Hence

```
without − with = (N − 1) × ladder
```

## Where "no amortisation" sits among possible baselines

Three configurations are worth keeping distinct:

- **A — plain SLH-DSA RRSIG.** One full SLH-DSA signature per RRset, no ladder
  at all. Wire bytes = `Σ full_slhdsa_signature_i`. This is the DNSSEC status
  quo before any MTL wrapper; the suite measures it directly — wire bytes and
  per-message sign/verify time — as the third configuration.
- **B — MTL full-signature mode** (labelled **No amortisation**). An MTL-aware
  resolver reconstructs a complete signature for every message, so each message
  carries `condensed_i || ladder`. Wire bytes = `Σ condensed_i + N × ladder`,
  which equals `Σ (condensed_i + ladder)`. This is the "without MTL" series in
  the results artifact.
- **C — MTL amortised** (labelled **MTL amortised**). A condensed proof per
  message plus one shared signed ladder. Wire bytes = `ladder + Σ condensed_i`.

Configuration B is a legitimate, conservative comparison — it pays the full
ladder on every message — but it is *not* the same as the plain SLH-DSA RRSIG
baseline A. Where a report or chart says "no amortisation", it means B.

## How the signing benchmark realises each setting

- **MTL amortised**: a single `sign_batch(N)` call — one ladder signed for the
  whole batch.
- **No amortisation**: `N × sign_batch(1)` calls — one complete MTL signature
  per message, so the ladder is signed for every message.

## Numbers and provenance

Every number in the report, charts and TUI is read from the consolidated
artifact `target/climb-results.json` (produced by `cargo xtask consolidate`),
never typed into source. The wire-byte formulas above are the identities the
artifact's `wire_size` section satisfies, and the arithmetic is proven by the
wire-size tests in `crates/climb-bench`.

These are empirical measurements only; no formal security guarantee or proof is
claimed.
