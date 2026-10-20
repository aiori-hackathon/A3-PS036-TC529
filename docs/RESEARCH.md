# Research reading list

A skeleton for the team researcher to complete and commit under their own git
identity. Each entry names the **exact version to verify** before the entry is
marked: open the document on datatracker.org (or the standards body's own
proceedings), confirm the version/date below is still the current one on the
day you check, correct it here if it is not, and only then fill in the status
line. **No entry is marked verified yet** — the `Status` lines are deliberately
blank, and they stay blank until a human has personally read the source and
signed the line. Do not mark an entry verified on the basis of a summary,
including this file.

Format per entry:

- **Version to verify** — the exact draft number / RFC number / document to
  look up, with the date the version was published as last checked here.
- **What it defines** — the normative content CLIMB relies on.
- **Used by** — which part of the workspace consumes that content.
- **Status** — `VERIFIED by <name> on <date>`, or blank.

## Specifications and drafts

### draft-harvey-cfrg-mtl-mode-09

- **Version to verify:** `draft-harvey-cfrg-mtl-mode-09` (March 2026) — check
  datatracker for the CFRG MTL mode draft and confirm -09 is current.
- **What it defines:** Merkle Tree Ladder mode itself: the ladder structure,
  full and condensed signatures, the verification paths, and the security
  considerations of the construction.
- **Used by:** `climb-mtl` (wrapper semantics follow the draft's operations),
  `climb-analytical` (ladder shape: rungs, authentication paths, signed-ladder
  size), `climb-bench` (the two verification conditions).
- **Status:** VERIFIED by __________ on __________

### draft-fregly-dnsop-slh-dsa-mtl-dnssec-06

- **Version to verify:** `draft-fregly-dnsop-slh-dsa-mtl-dnssec-06`
  (30 March 2026) — confirm -06 is current on datatracker.
- **What it defines:** How MTL mode is applied to DNSSEC with SLH-DSA: the
  RRSIG payload layouts for full and condensed signatures, what an
  MTL-aware resolver must cache and verify, and the zone-signing workflow.
- **Used by:** `climb-dns` (condensed-vs-full RRSIG wire layout in the capture
  parser and the wire-size measurement), the signer and server images in
  `docker/` (the patched NSD/Unbound implement this draft), `climb-bench`
  (transmitted-byte conditions A/B/C).
- **Status:** VERIFIED by __________ on __________

### draft-kaizer-dnsop-ml-dsa-mtl-dnssec

- **Version to verify:** `draft-kaizer-dnsop-ml-dsa-mtl-dnssec` — the version
  aligned with libMTL 1.3.1 is **-02**; check datatracker for anything newer
  before relying on -02.
- **What it defines:** The ML-DSA (rather than SLH-DSA) application of MTL
  mode to DNSSEC. CLIMB measures SLH-DSA, but the reference implementation
  supports both families, and this draft is the reason why.
- **Used by:** context for `climb-mtl` (libMTL's algorithm-family
  parameterisation); not exercised by any measurement in this repository.
- **Status:** VERIFIED by __________ on __________

### draft-hdong-dnsop-ml-dsa-mtl-dnssec-sigtag-ext-00

- **Version to verify:** `draft-hdong-dnsop-ml-dsa-mtl-dnssec-sigtag-ext-00`
  (September 2026) — ML-DSA-specific signature-tag extension; confirm -00 is
  current on datatracker.
- **What it defines:** An extension to the ML-DSA MTL DNSSEC draft around
  signature tags. Out of scope for CLIMB's measurements (SLH-DSA only), listed
  so the researcher tracks how the draft set is evolving.
- **Used by:** nothing in the workspace; tracked for context only.
- **Status:** VERIFIED by __________ on __________

### FIPS 205 (SLH-DSA)

- **Version to verify:** FIPS 205, *SLH-DSA: Stateless Hash-Based Digital
  Signature Standard* — confirm the current revision on nvlpubs / NIST's
  publication page.
- **What it defines:** SLH-DSA itself: parameters, key and signature sizes,
  and the security statement. CLIMB's public-key size cross-check against the
  published parameter table comes from here.
- **Used by:** `climb-mtl` (parameter/size sanity checks), `climb-bench`
  (plain SLH-DSA configuration A measured through liboqs, which implements
  FIPS 205).
- **Status:** VERIFIED by __________ on __________

### RFC 4033, RFC 4034, RFC 4035 (DNSSEC)

- **Version to verify:** RFC 4033 (introduction), RFC 4034 (resource
  records), RFC 4035 (protocol changes) — confirm on datatracker; these are
  immutable RFCs, so verification means confirming the numbers and reading
  them.
- **What it defines:** The DNSSEC baseline CLIMB amends: RRsets, RRSIG
  records, the zone-signing and validation model that MTL mode modifies.
- **Used by:** `climb-dns` (RRset/RRSIG terminology and wire parsing),
  `climb-report` and `climb-tui` (response-anatomy labels), the report's
  configuration-A baseline definition.
- **Status:** VERIFIED by __________ on __________

### RFC 6891 (EDNS(0))

- **Version to verify:** RFC 6891 — confirm on datatracker.
- **What it defines:** The EDNS(0) mechanism, including the UDP payload-size
  field in queries and responses that bounds how big a DNS response may be
  over UDP before TCP fallback.
- **Used by:** `climb-analytical` (the UDP-fit model's buffer-size axis),
  `climb-dns` (OPT-record parsing in the captured responses), the TUI Wire
  tab's UDP-fit gauge.
- **Status:** VERIFIED by __________ on __________

### RFC 7766 (DNS transport)

- **Version to verify:** RFC 7766 — confirm on datatracker.
- **What it defines:** DNS transport over TCP: persistent connections and the
  general expectation that oversized responses fall back to TCP.
- **Used by:** `climb-analytical` (the TCP-fallback branch of the UDP-fit
  model: a response that does not fit is modelled as needing a TCP exchange).
- **Status:** VERIFIED by __________ on __________

### DNS Flag Day 2020

- **Version to verify:** The DNS Flag Day 2020 article (dnsflagday.uk /
  its datatracker / ISOC mirrors) — confirm the document you read is the
  Flag Day 2020 one, not a later year's.
- **What it defines:** The operational consensus to raise the standard EDNS
  UDP buffer size and stop defending small-buffer fragmentation black holes —
  the origin of the buffer-size markers on the TUI Wire tab's UDP-fit gauge.
- **Used by:** `climb-analytical` (the recommended buffer size used as the
  gauge's reference marker), `climb-tui` (the gauge markers themselves).
- **Status:** VERIFIED by __________ on __________

## Papers and presentations

### CT-RSA 2023 MTL paper

- **Version to verify:** The CT-RSA 2023 paper on Merkle Tree Ladder mode
  (Kaliski et al., *Merkle Tree Ladder Mode: Reducing the Size and
  Verification Cost of Log Proofs*) — confirm title, authors and pages on the
  Springer / ePrint record.
- **What it defines:** The construction and analysis behind MTL mode as
  published at CT-RSA, including the amortisation reasoning that CLIMB's
  ladder-reuse model empirically probes.
- **Used by:** `climb-analytical` (the reuse break-even model's conceptual
  basis), the report's framing of what amortisation does and does not save.
- **Status:** VERIFIED by __________ on __________

### Tran and Chung evaluation (IETF 122, pq-dnssec side meeting)

- **Version to verify:** The evaluation presentation by Tran and Chung at the
  IETF 122 pq-dnssec side meeting — confirm via the IETF 122 proceedings /
  materials page (side meetings are not datatracker drafts, so verify through
  the IETF meeting archive and note the exact URL you used).
- **What it defines:** An independent evaluation of MTL-mode DNSSEC
  performance; the closest published comparison point for CLIMB's numbers.
- **Used by:** context for interpreting CLIMB's results (methodology
  comparison); no workspace module consumes it directly.
- **Status:** VERIFIED by __________ on __________

## How to mark an entry verified

1. Open the source at its authoritative location and read it.
2. Confirm the version line above; correct it here if the world moved.
3. Fill the status line with your name and the date you actually read it, in
   the same commit that corrects the version line if it changed.
4. Never batch-mark entries you have not personally read.
