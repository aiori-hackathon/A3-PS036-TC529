# Replication inputs

This directory holds second-host measurements. Each additional host gets one
subdirectory named after the first twelve hex characters of its environment
fingerprint, containing:

- `climb-results.json` — that host's consolidated results document, produced by
  `cargo xtask reproduce` on that machine;
- `environment.json` — the environment fingerprint written by
  `cargo xtask pin-env` on the same machine.

```
data/replication/
  0f98627f768f/          # this host, for reference
  3c1d59a42e77/          # a second host
    climb-results.json
    environment.json
```

`cargo xtask replicate-compare --a <results> --b <results>` compares two
documents without writing anything. `cargo xtask replicate-summary` reads the
lexicographically first host directory that carries a results document and
writes the summary that consolidation embeds; with no host directory present it
writes a pending summary and makes no agreement claim.

The comparison covers only ratios that are already derived in both documents.
No measurement is repeated and no host is treated as the reference. The
relative difference is symmetric, `|B − A| / max(|A|, |B|) × 100`, and the
agreement threshold is declared before the second host is read. A summary that
finds disagreement records the computed maximum and says so; it never rounds a
disagreement into agreement.

Two hosts are not a generality claim. The summary states how many hosts it
compared, and the report and dashboard repeat that count.
