# Pseudocode

Plain-language pseudocode for the four operations a reviewer most needs to
see. Each block names the Rust function or automation command that implements
it. Every cryptographic operation named inside a block is one the pinned
libMTL reference library already exposes (`mtllib_sign_append`,
`mtllib_sign_get_signed_ladder`, `mtllib_sign_get_condensed_sig`,
`mtllib_verify`, `mtllib_verify_signed_ladder`); nothing below is implemented
from scratch.

## Signing a batch with one MTL ladder

Implemented by `climb_mtl::MtlSigner::sign_batch`
(crates/climb-mtl/src/lib.rs), exercised per dataset size by
`cargo xtask bench-all`.

```text
sign_batch(messages):
    for each message in messages:
        append hash(message) to the shared node set        # mtllib_sign_append
    build the Merkle tree over the node set               # library-internal
    build the ladder over the tree                        # library-internal
    sign the ladder once with the underlying signature    # mtllib_sign_get_signed_ladder
    for each message in messages:
        emit its condensed signature (auth path + leaf)   # mtllib_sign_get_condensed_sig
    return (condensed signatures, signed ladder)
```

Calling `sign_batch` with one message at a time is the non-amortised
baseline: each call builds a fresh ladder and pays one full underlying
signature per message.

## Verifying, cold ladder and trusted cached ladder

Implemented by `climb_mtl::MtlVerifier::verify`
(crates/climb-mtl/src/lib.rs), exercised cold and cached by
`cargo xtask ladder-verify` and the verifying benchmark group.

```text
verify(message, signature, ladder, trust_cached_ladder):
    if signature is full:
        verify message against the full signature         # mtllib_verify
    if signature is condensed:
        if no ladder supplied: reject
        if not trust_cached_ladder:            # cold resolver
            verify the ladder's own signature first        # mtllib_verify_signed_ladder
        verify the condensed proof of the message           # mtllib_verify
    return authenticated or mismatch
```

`trust_cached_ladder` mirrors the native verifier's `-t` option: the ladder
is assumed already validated, and only the condensed proof is checked.

## The benchmark loop (configurations A, B, C)

Implemented by the Criterion groups in crates/climb-bench/benches (signing,
verifying, classical) and crates/climb-bench/src/wire_size.rs; run by
`cargo xtask bench-all` and `cargo xtask wire-report`.

```text
for size in dataset sizes:
    dataset = generate_dataset(size, seed)      # excluded from timing
    A (plain SLH-DSA):
        per message, time one plain sign and one plain verify
    B (MTL, no amortisation):
        per message, time sign_batch([that message]); time a full-signature verify
    C (MTL amortised):
        once, time sign_batch(all messages)
        per message, time a condensed verify, cold and cached
    wire bytes:
        A = sum of plain signature lengths
        B = sum of full signature lengths
        C = ladder length + sum of condensed lengths
    Criterion repeats every measurement and reports the mean over its samples
    excluded from timing: dataset generation, key generation, verifier
    construction, result serialization
```

## DNS wire flow for one response, full versus condensed

Driven by the local NSD-MTL / Unbound pipeline in `cargo xtask dns-demo`;
the captures are parsed by climb-dns and recorded in the results artifact.

```text
full response:
    resolver sends an A query with the DO bit set, no sigtag
    NSD returns RRSIG records carrying the full signed ladder
condensed response:
    resolver sends the same query with a sigtag option carrying the ladder hash
    NSD returns RRSIG records carrying only the condensed proofs
    resolver validates the ladder once and caches it as trusted
    later answers are verified against the cached ladder (condensed path only)
record the wire bytes of both responses and each RRSIG's length
```
