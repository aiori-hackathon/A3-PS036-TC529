# Research status and standards provenance

> **One-sentence answer for judges** *(draft - researcher to approve)*:
> CLIMB benchmarks an experimental Internet-Draft implementation of Merkle
> Tree Ladder mode with SLH-DSA for DNSSEC, not a ratified standard, and this
> file records the exact revision, status and verification state of every
> source the project relies on.

## How to read this file

The middle columns record what was observed on the IETF Datatracker, the NIST
publication page or the publisher record on the date named in each row. The
`Revision implemented` column records what the pinned code actually does,
whether or not that matches the newest published revision. The `Verified by`
and `Verification date` columns are for the human researcher only: every row
stays `PENDING - researcher` until that person has opened the source and
signed the row. No row in this file is marked verified on the basis of a
summary, an API response or an agent's reading.

The right-hand `Notes` column names the evidence path inside this repository
or the exact upstream record used for the observation. Draft numbers, RFC
numbers and FIPS numbers are identifiers, not claims about status.

## Source status table

| Source | Revision implemented | Status on 24 Oct 2026 | Newer revision? | Verified by | Verification date | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `draft-harvey-cfrg-mtl-mode` (Merkle Tree Ladder Mode Signatures) | `-00` is named in the pinned library's README, while hash code cites `-07` and buffer/wire code cites `-08`; the library is therefore a mixed-revision implementation, not cleanly one draft | Rev `09`, posted 2026-03-24, expired 2026-09-25; Internet-Draft, Informational, no RFC | No; `09` was still the latest revision observed on 2026-11-07 | PENDING - researcher | PENDING - researcher | Pinned source evidence: `third_party/MTL/README.md:2`, `third_party/MTL/src/mtl_hash.c:52`, `third_party/MTL/src/mtl_buffer.c:175`, `third_party/MTL/src/mtllib.c:1122`. Datatracker document record and document events. |
| `draft-harvey-cfrg-mtl-mode-considerations` (Integrating MTL Mode into Applications) | None directly; design guidance only, no code path derives from it | Rev `03`, posted 2026-02-02, expired 2026-08-06; Internet-Draft, no RFC | No; `03` was still the latest revision observed on 2026-11-07 | PENDING - researcher | PENDING - researcher | Datatracker document record. The draft discusses grouping, ordering and batch-signing options without fixing a batch size. |
| `draft-fregly-dnsop-slh-dsa-mtl-dnssec` (SLH-DSA-MTL for DNSSEC) | The condensed/full RRSIG layout and DNSKEY encoding are parsed in `climb-dns`; the live pipeline uses pinned patched DNS tools. The draft leaves the DNSSEC algorithm number and the EDNS option code as TBD | Rev `06`, posted 2026-03-30, expired 2026-10-01; Internet-Draft, no RFC | No; `06` was still the latest revision observed on 2026-11-07 | PENDING - researcher | PENDING - researcher | `crates/climb-dns/src/anatomy.rs`; `docker/Dockerfile.nsd`; `docker/Dockerfile.ldns-signer`. Draft Sections 3 to 6 define the DNSKEY, RRSIG and `mtl-mode-full` shapes but assign no code points. |
| `draft-sheth-pqc-dnssec-strategy` (PQC strategy for DNSSEC) | None; cited for the dual-algorithm strategy only | Rev `02`, posted 2026-10-16, active, expires 2027-04-19; Internet-Draft, no RFC | No; `02` was still the latest revision observed on 2026-11-07 | PENDING - researcher | PENDING - researcher | Datatracker document record. |
| `draft-kaizer-dnsop-ml-dsa-mtl-dnssec` (ML-DSA-MTL for DNSSEC) | None exercised by a CLIMB measurement; the pinned library supports ML-DSA families, but every reported row uses SLH-DSA | Rev `03`, posted 2026-10-19, active, expires 2027-04-22; Internet-Draft, no RFC | No; `03` was still the latest revision observed on 2026-11-07 | PENDING - researcher | PENDING - researcher | Datatracker document record. Context for `third_party/MTL/src/mtllib_schemes.h`; not a measured configuration. |
| `draft-hdong-dnsop-ml-dsa-mtl-dnssec-sigtag-ext` (ML-DSA-MTL DNSSEC SigTag extension) | The live capture helper reuses the SigTag EDNS option shape with a provisional option code; no benchmark measurement uses the ML-DSA SigTag mechanism | Rev `00` was current on 2026-10-24 (posted 2026-09-28, expires 2027-04-01); Internet-Draft, no RFC | Yes; `01` was posted 2026-10-27, after the 24 Oct cut-off (now expires 2027-04-30) | PENDING - researcher | PENDING - researcher | `scripts/mtl_sigtag.py`; `scripts/mtl_dns_capture.py`; `crates/climb-dns/src/anatomy.rs`. Draft Section 4.1 leaves the option code as TBD. |
| FIPS 205, *SLH-DSA: Stateless Hash-Based Digital Signature Standard* | SLH-DSA-SHA2-128s (pure) through liboqs; MTL wrapper `SLH-DSA-SHA2-128s-MTL-SHA2-128` | Final, published 2024-08-13; NIST standard, DOI `10.6028/NIST.FIPS.205` | No newer revision observed | PENDING - researcher | PENDING - researcher | `third_party/MTL/src/mtllib_schemes.h:52`; `crates/climb-mtl/src/lib.rs:57`. NIST CSRC final-publication record. |
| RFC 4033, *DNS Security Introduction and Requirements* | The DNSSEC baseline model used by the live pipeline and the analytical baseline | Proposed Standard; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record and RFC Editor metadata. |
| RFC 4034, *Resource Records for the DNS Security Extensions* | DNSKEY, DS, RRSIG and NSEC terminology and wire layout used in `climb-dns` | Proposed Standard; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record and RFC Editor metadata. |
| RFC 4035, *Protocol Modifications for the DNS Security Extensions* | Zone-signing and validation model that MTL mode amends; parsing and report labels | Proposed Standard; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record and RFC Editor metadata. |
| RFC 6891, *Extension Mechanisms for DNS (EDNS(0))* | EDNS(0) OPT-record parsing and the response-size axis of the UDP-fit model | Internet Standard; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record and RFC Editor metadata. |
| RFC 7766, *DNS Transport over TCP - Implementation Requirements* | TCP-fallback branch of the UDP-fit model | Proposed Standard; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record. |
| RFC 8179, *Intellectual Property Rights in IETF Technology* | The IPR-disclosure process referenced only by the note below; no code path | BCP 79; immutable | No; RFCs do not gain revisions | PENDING - researcher | PENDING - researcher | Datatracker document record. |
| DNS Flag Day 2020 | The recommended EDNS buffer size used as a reference marker in the UDP-fit view | Operational recommendation dated 2020-10-01; not a numbered standard | No newer Flag Day replaces the 2020 recommendation | PENDING - researcher | PENDING - researcher | `https://www.dnsflagday.net/2020/`; `climb-analytical` and the dashboard gauge markers. |
| Fregly, Harvey, Kaliski and Sheth, *Merkle Tree Ladder Mode: Reducing the Size Impact of NIST PQC Signature Algorithms in Practice*, CT-RSA 2023 | Conceptual basis for the amortisation and ladder-reuse model only | Published in *Topics in Cryptology - CT-RSA 2023*, Lecture Notes in Computer Science; DOI `10.1007/978-3-031-30872-7_16` | No newer edition observed | PENDING - researcher | PENDING - researcher | Crossref bibliographic record. Note that an older reading list in this file gave a different title; the Crossref title above is the observed one. |
| Tran and Chung, MTL-mode DNSSEC evaluation, IETF 122 pq-dnssec side meeting | Context only; no workspace module consumes it | Not located in the IETF 122 materials or side-meeting pages when checked | Unknown; the presentation was not found | PENDING - researcher | PENDING - researcher | The claim is retained only as an unverified pointer. The researcher must locate the exact presentation URL or remove the entry. |

## What the pinned implementation actually follows

The pinned submodule is `third_party/MTL` at commit
`53ce25dcc35c15e051c5263e41aaeaede18a8aa7` (library version declared as
`mtllib 1.2.1`). Its self-description is mixed:

- `third_party/MTL/README.md:2` says the implementation is based on
  `draft-harvey-cfrg-mtl-mode-00`.
- `third_party/MTL/src/mtl.h:208-273` labels the node-set operations as
  draft-00 Sections 8.3 to 8.8.
- `third_party/MTL/src/mtl_hash.c:52`, `:131` and `:230` label the tweakable
  hash functions as draft-07 constructions.
- `third_party/MTL/src/mtl_buffer.c:175` and `:334` label the signature and
  ladder buffer layouts as draft-08 Sections 7.3 and 7.1.
- `third_party/MTL/src/mtllib.c:1122-1135` labels the API behaviour as
  "MTL Mode Draft #8".

For the measured parameter set, `third_party/MTL/src/mtllib_schemes.h:52`
maps `SLH-DSA-SHA2-128s-MTL-SHA2-128` to security parameter 16, the SHA2 hash
family, sampled randomizer and `OQS_SIG_alg_slh_dsa_pure_sha2_128s`. The
workspace default in `crates/climb-mtl/src/lib.rs:57` is that same MTL
algorithm name. The pinned test vectors exercise the same shape with
`ML-DSA-44-MTL-SHAKE-128` and security parameter 16
(`third_party/MTL/test/mtltest_test_vectors.h:36-42`), and the full-signature
fixture fixes the SLH-DSA-MTL-SHAKE-128s signature length at
`third_party/MTL/test/mtltest_full_signature.h:36`.

Two consequences follow, and both belong in any honest answer:

1. The project's measurement is against a reference library that describes
   itself as draft-00 but contains later-draft layouts. The benchmark does not
   claim wire compatibility with any single published revision of the MTL
   draft.
2. The library's provisional identifiers are not IANA assignments. DNSSEC
   algorithm 130 and EDNS option 65050 are used by the pinned reference tools
   and by this repository's live pipeline, but the drafts leave both code
   points as TBD.

## What the drafts specify and what CLIMB decided

| Topic | Specified by the cited drafts | Chosen in this implementation |
| :--- | :--- | :--- |
| Batch sizes (how many messages share a ladder) | The MTL draft fixes neither a batch size nor a signing cadence; it defines a message series and the node-set operations over it. The considerations draft lists grouping and occasional-signing approaches without selecting one. | CLIMB uses four synthetic dataset sizes and treats each dataset as one signing batch for the amortised configuration: 100, 1,000, 10,000 and 1,000,000 messages, plus exact-count prefixes for ladder-shape checks (`crates/climb-dns/src/dataset.rs`). These are benchmark fixtures, not deployment recommendations. |
| Rung strategy (how a ladder is decomposed) | The binary rung strategy is defined in draft-09 Section 6.6: rungs are the maximal perfect binary subtrees given by the set bits of the leaf count, laid out left to right. The rung count is therefore a function of the message count, not a tunable. | CLIMB implements that decomposition in `climb-analytical` and checks it against the library's ladder output in `climb-bench` (`crates/climb-analytical/src/ladder.rs`). The reported "three rungs" for a 100-message dataset is a consequence of the specified binary strategy, not a CLIMB choice. CLIMB does choose to report ladder shape per dataset size rather than to invent a single representative rung count. |
| Dataset generation (what the signed messages are) | The drafts describe DNS RRsets, canonical ordering and DNSSEC record shapes; they do not define a benchmark corpus or a synthetic message generator. | CLIMB generates deterministic synthetic DNS responses from a seed, varying the RRset type across messages; the same seed reproduces byte-identical input (`crates/climb-dns/src/dataset.rs`, `crates/climb-dns/src/synthetic.rs`). This is a deliberate reproducibility choice, not a claim that the corpus matches a real zone. |
| Parameter choice (which algorithm and security parameter) | Draft-09 Section 10 defines the naming convention and a table of recommended instantiations. For the SHA2 family at the 128-bit level it lists `SLH-DSA-SHA2-128s-MTL-SHA2-128` with security parameter 16. FIPS 205 defines the underlying SLH-DSA parameter set. | CLIMB measures exactly the 128-bit SHA2 "small" instantiation as the single reported configuration and does not extrapolate to the SHAKE, 192-bit or 256-bit sets. That narrow scope is a CLIMB decision; the parameter set follows the draft's table. |
| DNS record mapping (how MTL data appears in DNSSEC) | The SLH-DSA-MTL DNSSEC draft defines DNSKEY encoding, the one-octet full/condensed MTL-Type prefix in RRSIG, the `mtl-mode-full` EDNS(0) request option, and resolver behaviour for requesting a signed ladder. The MTL draft defines the full and condensed signature structures. | CLIMB parses that layout in `climb-dns` and measures both condensed and full responses. The live pipeline uses the pinned patched NSD and signer forks with provisional algorithm number 130 and provisional EDNS option 65050, because the drafts assign neither. The zone content, the query set and the choice to compare condensed against full are benchmark decisions. |
| Ladder retrieval and caching | The considerations draft discusses when to refresh a ladder and the DNSSEC draft defines the `mtl-mode-full` fallback. | CLIMB models "trust cached ladder" as one verification condition and "fetch the full signature" as another; the cache-hit assumption is made explicit in the model rather than presented as a deployment result. |

## Intellectual-property note on the MTL mode draft

The MTL mode draft has an IPR disclosure on the IETF Datatracker from
VeriSign, Inc., submitted 2023-07-19 under the IETF process described by
RFC 8179. The disclosure page observed is
`https://datatracker.ietf.org/ipr/6072/`, titled "VeriSign, Inc.'s Statement
about IPR related to draft-harvey-cfrg-mtl-mode", and it records a patent
application and a royalty-free Standards Development Grant with an attribution
notice. **The researcher must open that page, confirm the exact wording and
link, and record the confirmation here; this paragraph is a pointer, not a
legal interpretation.** CLIMB does not rely on the disclosure as a licence
grant and makes no claim about its effect.

## How to mark a row verified

1. Open the source at its authoritative location (Datatracker, NIST CSRC, the
   RFC Editor or the publisher) and read the relevant sections.
2. Confirm the revision, date and status written in the row; correct any stale
   cell in the same change.
3. Replace `PENDING - researcher` in `Verified by` with your name and put the
   date you actually opened the source in `Verification date`.
4. Never mark a row from a summary, an API response or another person's
   reading. If a source cannot be located, say so in `Notes` and leave the row
   pending.
