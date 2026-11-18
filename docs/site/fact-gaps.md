# Fact gaps

Every number the new copy needs that has no fact id in the committed
`site/src/data/generated/facts.json` today. The prototype is the design
authority, so its copy is the target; the numbers below are the ones the new
sections introduce (or the prototype states with no fact behind it). Nothing
here is implemented in MS0: this is the work list for the build-time fact
registry.

Sources named in the last column:

- **pointer** — the value is already in `docs/sample-output/climb-results.json`;
  it only needs a `FactSpec` in `crates/climb-report/src/site_data.rs`
  (`FACT_SPECS` → `build_facts`) so `site/src/data/generated/facts.json` carries
  it and `Fact.astro` can render it.
- **generator** — the value is not a field: it is an arithmetic combination the
  generator must compute (a quotient, a count, a crossing). The variant it needs
  is named.
- **absent** — no artifact produces it yet; the gap is a measurement or a new
  generator, and the page must show its pending state until it exists.

## 1. The gaps

| Fact id | Where the copy needs it | Value source in the artifact | Producer | Evidence class | Why it is a gap |
| --- | --- | --- | --- | --- | --- |
| `F-dns-rtt-plain-50` | DNS replay panel: the plain response at an emulated 50 ms RTT | `/dns_rtt/cells/8/median_ms` (200 samples, udp-then-tcp, full RRSIG) | pointer: `FactSpec { id: "F-dns-rtt-plain-50", value: Pointer("/dns_rtt/cells/8/median_ms") }` | Measured | the cell exists and is recorded, but no fact spec exposes it; the copy currently has no slot for a latency figure |
| `F-dns-rtt-condensed-50` | same panel, the condensed response at 50 ms | `/dns_rtt/cells/9/median_ms` (200 samples, udp, condensed RRSIG) | pointer: same shape as above | Measured | same; the pair is the point of the panel (201.06 ms against 50.48 ms at the recorded tip) |
| `F-dns-rtt-samples` | same panel: the sample count per cell | `/dns_rtt/min_samples_per_cell` | pointer | Constant | the number is recorded with the run and is a design constant, not a result; it needs an id so the copy can print it |
| `F-ladder-cache-measured` | Ladder cache panel: bytes the resolver holds after the first pass | `/cache_use/ladder_analytical/0/measured_bytes` (medium, r = 10000) | pointer | Measured | recorded, not exposed; the panel is new in the skin |
| `F-ladder-cache-analytical` | same panel, the closed-form estimate | `/cache_use/ladder_analytical/0/analytical_bytes` | pointer | Analytical | same |
| `F-ladder-cache-ratio` | same panel, measured over analytical | `/cache_use/ladder_analytical/0/measured_over_analytical` | pointer | Derived | same; the ratio is what the panel argues |
| `F-ladder-signed-medium` | Ladder panel: the signed ladder size for the medium batch | `/cache_use/ladder_analytical/0/signed_ladder_bytes` | pointer | Measured | recorded, not exposed |
| `F-ladder-signed-small` | Ladder panel: the signed ladder size for the small batch | `/cache_use/ladder_analytical/1/signed_ladder_bytes` | pointer | Measured | recorded, not exposed |
| `F-breakeven-bytes` | Calculator: the reuse count where C's bytes cross A's | `/derived/reuse/1/breakeven_reuse_bytes` (2 for medium) | pointer | Derived | the field is an integer crossing computed per size; the copy needs the id and the label `r*` |
| `F-breakeven-signing` | Calculator: the reuse count where C's time crosses A's | `/derived/reuse/1/breakeven_reuse_time` (2 for medium) | pointer | Derived | same |
| `F-message-bytes-a` | Message diagram: bytes one plain message carries | `/wire_size/1/plain_signature_len_bytes` (7856) | pointer | Measured | the diagram is new; A is a recorded length |
| `F-message-bytes-b` | Message diagram: mean bytes one B message carries | `/wire_size/1/without_mtl_total_bytes` ÷ `/wire_size/1/message_count` | generator: a `FactValue::Quotient { numerator, denominator }` variant, rounded to 1 decimal | Derived | no field holds the mean; the prototype drew the bar from a constant |
| `F-message-bytes-c` | Message diagram: mean bytes one C message carries | `/wire_size/1/with_mtl_total_bytes` ÷ `/wire_size/1/message_count` | generator: same quotient variant | Derived | same |
| `F-claims-measured` | Verify: claims by class | `site/src/data/generated/claims.json`, `class == "measured"` over `/claims` | generator: a `FactValue::ClaimsClass("measured")` variant, counted when the claims document is built | Generated | the count is a property of the registry, not of the results artifact; 263 at the recorded tip |
| `F-claims-derived` | Verify: claims by class | `claims.json`, `class == "derived"` | generator: same variant | Generated | 41 at the recorded tip |
| `F-claims-analytical` | Verify: claims by class | `claims.json`, `class == "analytical"` | generator: same variant | Generated | 526 at the recorded tip |
| `F-claims-extrapolated` | Verify: claims by class | `claims.json`, `class == "extrapolated"` | generator: same variant | Generated | 18 at the recorded tip |

The recorded-tip values in the last column come from the committed
`claims.json` and the committed results artifact read during this phase. They
are recorded here as evidence that the gap is real and the source is present;
they are not copy, and they are not typed into a page. The page will render
them only through a fact id.

## 2. Numbers the prototype states that must not be copied

These are in the prototype's visible copy and are **wrong or mislabelled**
against the current artifacts. The port takes the fact id, never the string.

| Prototype string | Prototype value | Artifact at the recorded tip | Correct fact id | Class |
| --- | --- | --- | --- | --- |
| signing speed-up, C vs A | `≈ 9,405 ×` | `9,434` | `F-sign-medium-c-vs-a` | Derived, no `≈` |
| verifying speed-up, cached ladder | `≈ 215.3 ×` | `77.34` | `F-verify-medium-c-vs-a` | Derived, no `≈` |
| measured over analytical memory, small | `1.90 ×` | `1.628` | `F-mem-small-ratio` | Derived |
| claims verified | `771 of 771` | `852 of 852` | `F-claims-count` | Generated |
| plain message bytes (results row) | `2,743,352 B vs 78,560,000 B` | same | `F-wire-medium-c`, `F-wire-medium-a` | Measured |

The derivation of the two class corrections is in `docs/site/numbers-prototype.md`
§"Class reconciliation": the Medium comparison carries `"class": "derived"` in
the artifact, so the row is Derived and takes no approximation sign.

## 3. Numbers that need no fact id

Recorded so a reviewer does not have to re-derive the classification. These are
protocol constants, identifiers or layout values, and they are either rendered
from a token or are not numbers a reader reads as a result.

| Number | Why no fact id |
| --- | --- |
| `1,232` (the EDNS(0) payload) | a protocol/model constant; already `F-udp-payload` as a Constant |
| `512` (the classic UDP limit) | a protocol constant; `NON_CLAIM_TOKENS` in the checker |
| `SLH-DSA-SHA2-128s-MTL-SHA2-128`, `PS-036`, `A3-PS036-TC529`, `AIORI-3`, `FIPS 205` | identifiers |
| `72`, `56`, `70`, `260`, `84`, `11`, `34`, `130`, `60`, `720`, `360`, `46`, `650`, `380`, `110`, `500`, `280`, `420`, `44` | animation and geometry constants, inventoried in `docs/site/animation-inventory.md` |
| `A`, `B`, `C` configuration labels | names, not quantities |
| section numbers (`/01.00` … `/09.00`) | route labels |

## 4. What the gate enforces now

`scripts/site-check.py docs` reads this document and the checker's `FACT_IDS`
table and fails when:

- a fact id here is not declared in the checker (so a gap cannot be invented in
  prose), or
- a checker entry whose source starts `GAP:` is not described here (so a
  registered gap cannot be forgotten).

`scripts/site-check.py copy` fails when any of these ids appears in `copy.md`
before its producer exists, because a `GAP:` entry prints as
`not in the artifact (GAP: …)` and the copy line then carries no resolvable
value. The intended order is: add the fact spec, regenerate
`site/src/data/generated/facts.json` with `cargo xtask site-data`, then write the
copy line.
