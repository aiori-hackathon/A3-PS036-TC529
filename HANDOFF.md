# HANDOFF (private)
Phase: F29 - Recompute every published number from raw rows; fix the headline rows
Started in the tool (quota ended mid-phase). Resumed in Freebuff on <date>. Baseline commit: <short sha before F29>.
Sub-tasks:
29.1 Table of record: for each headline metric, the raw rows, a hand recomputation, and the value on every surface (comparisons[], derived.*, docs/deck-facts.md, README, docs/sample-output/report.html, the TUI Overview data function). Any disagreement is a defect: fix it by regenerating from one run, never by editing a value.
29.2 cargo xtask verify-numbers: recompute each headline metric from raw rows (never from a derived field), compare with every surface, exit non-zero with a table of disagreements; call it from docs-check and preflight; tests with the real fixture and an injected disagreement on each surface.
29.3 Headline fixes: replace the single UDP-fit row showing '0.000 %' with three rows A, B, C ('k of n modelled rows within the UDP payload limit', ANALYTICAL) plus one explanatory sentence; clarify or remove the 'Ladder shape' row; add one scope sentence under the table (live DNS rows are measured over TCP in a local capture; UDP fit is analytical; which rows are extrapolated). No digits beyond what docs-check already approves.
29.4 Spread: run cargo xtask stability --runs 5 --sizes small,onek,medium; show median and range for the signing and verifying rows; 'pending' when missing; add --require-spread to verify-numbers.
29.5 Path hygiene: normalise absolute paths at consolidation time (<repo>, ~); scan tracked text and JSON for /home/, /Users/, C:\Users, the developer username and email-like strings; docs-check fails on a hit; regenerate docs/sample-output and re-stamp provenance.
29.6 Classical baselines (Ed25519, ECDSA P-256, RSA-2048): a compact README block with its own markers, generated like the headline block; if a baseline is missing from the artifact, say so, do not invent it.
Where I stopped: <paste what the last agent said it finished>
Known problems: <failing tests, if known>
Environment note: the OS changed from Omarchy to Arch + sway on <date>; measurements taken after that date come from a different environment.
