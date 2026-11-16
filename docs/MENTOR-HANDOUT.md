# Mentor handout — PS-036 prototype demonstration

A one-page path through the prototype: the pseudocode the mentor asked for, the
three commands that drive the demonstration, and where each expected outcome is
visible. Full walkthrough: [`DEMO-PROTOTYPE.md`](DEMO-PROTOTYPE.md); longer
pseudocode with per-block source references: [`PSEUDOCODE.md`](PSEUDOCODE.md).

## Pseudocode (short form, library calls only)

```text
sign_batch(messages):                      # climb_mtl::MtlSigner::sign_batch
    for m in messages:
        append hash(m)                      # mtllib_sign_append
    ladder = build_tree_and_ladder()        # library-internal
    signed_ladder = sign(ladder)            # mtllib_sign_get_signed_ladder
    for m in messages:
        proof[m] = auth_path(m)             # mtllib_sign_get_condensed_sig
    return proofs, signed_ladder

verify(message, signature, ladder, cached):
    if full(signature):
        return mtllib_verify(message, signature)
    if ladder is None:
        return reject
    if not cached:
        if not mtllib_verify_signed_ladder(ladder):
            return reject
    return mtllib_verify(message, signature, ladder)

wire_bytes(proofs, signed_ladder):
    return len(signed_ladder) + sum(len(p) for p in proofs)
```

## The demo's three commands

```bash
# 1. Open the dashboard. Reads the committed artifacts; no native crypto stack.
cargo run --release -p climb-tui --no-default-features

# 2. Re-measure the ladder shape and the signing/verification matrix.
cargo xtask bench-all

# 3. Run the local-only DNS pipeline and record full versus condensed responses.
cargo xtask dns-demo
```

Command 3 needs Docker. If the demo host has no Docker, the recorded capture in
`docs/sample-output/` drives the same Live tab and the command can be skipped
with that limitation stated. The optional `cargo xtask ladder-verify`,
`wire-report` and `dns-cache` re-measurements in
[`DEMO-PROTOTYPE.md`](DEMO-PROTOTYPE.md) are deeper stops, not the spine.

## Where each expected outcome is shown

| Expected outcome | Dashboard | Command or artifact |
| --- | --- | --- |
| Pinned benchmark environment | Method tab, environment block | `environment.json`, `cargo xtask check-pins` |
| Benchmark harness | Signing and Verifying tabs | `crates/climb-bench/benches/`, command 2 |
| MTL and non-MTL configurations | Signing, Verifying and Wire tabs | A/B/C rows in `docs/sample-output/climb-results.json` |
| Three dataset sizes | Signing and Wire tabs | Small, Medium, Large rows in the same artifact |
| Repeated-trial measurements | Signing tab, spread column | `derived.stability` rows, `cargo xtask stability` |
| Signing and verification results | Signing and Verifying tabs | Command 2, `target/criterion/` |
| Transmitted-byte comparison | Wire tab, UDP-fit gauge | `wire_size` rows, `cargo xtask wire-report` |
| Cache use, measured and analytical | Memory tab | `cache_use` rows, `cargo xtask dns-cache` |
| Reproducible results report | Integrity tab | `docs/sample-output/report.html`, `cargo xtask sample-check` |
| Technical demonstration | All tabs, `?` help, `t` tour | Command 1, browser build on the public site |
| Pseudocode snippet | Ladder tab shows the real tree | This page and [`PSEUDOCODE.md`](PSEUDOCODE.md) |

## What to say while it runs

The dashboard is a read-only viewer over recorded evidence: every number comes
from `docs/sample-output/climb-results.json` or a recorded capture, and the
Integrity tab fails when two presentations disagree. The benchmark measures
one parameter set, one reference implementation and one laptop-class host; it
claims no security proof and no new cryptography. The live DNS pipeline is
local-only and the public site serves recorded results and a client-side
dashboard only.
