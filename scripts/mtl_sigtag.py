#!/usr/bin/env python3
"""Compute an MTL SigTag from a signed DNSSEC zone file.

A SigTag is the 32-byte SHAKE-128 hash of an MTL *signed ladder*. A resolver
that already validated and cached a ladder sends this hash in EDNS option 65050
(`MTL_MODE_FULL_CODE`) so the authoritative server can reply with only a
*condensed* signature (Merkle authentication path) instead of re-transmitting
the ~8 KB signed ladder on every response. This is the client half of the
protocol NSD implements in `src/val_pqc_algo.c::val_algo_get_ladder_hash`
(which is `EVP_shake128` over the signed-ladder bytes).

This script performs configuration discovery only — it computes a hash with a
provided primitive and constructs the wire bytes for `dig +ednsopt`. It does
not implement, modify, or reimplement any cryptographic algorithm:

  * SHAKE-128 comes from Python's stdlib `hashlib` (OpenSSL-backed).
  * The condensed-signature layout it walks is the one defined by the MTL IETF
    draft and implemented identically by libMTL (`mtl_auth_path_to_buffer`) and
    NSD (`val_algo_get_condensed_sig_header_size`):

        lead byte (1 = full | 2 = condensed)
        SID                   2 x hash_size
        flags                 2
        randomizer            hash_size
        leaf index            8
        target-left index     8
        target-right index    8
        sibling count         2
        sibling hashes        sibling_count x hash_size

For SLH-DSA-SHA2-128s-MTL-SHA2-128 the hash size is 16 bytes, so the fixed
header is 75 bytes and each sibling is 16 bytes.

Usage:
    python3 scripts/mtl_sigtag.py <signed-zone-file> [owner]

Prints the SigTag as lowercase hex (64 chars) on stdout. `owner` selects which
RRSIG to read; the default is the zone apex SOA RRSIG, which always carries a
full signature and therefore the complete signed ladder. Any other RRSIG in the
same zone shares the same ladder and yields the same SigTag.

Exit status is non-zero on any parse or I/O error.
"""

from __future__ import annotations

import base64
import binascii
import hashlib
import sys

# SLH-DSA-SHA2-128s-MTL-SHA2-128 parameters.
HASH_SIZE = 16
# lead(1) + SID(2*HASH_SIZE) + flags(2) + randomizer(HASH_SIZE) + 3*8 indices.
FIXED_HEADER = 1 + 2 * HASH_SIZE + 2 + HASH_SIZE + 3 * 8
# Sibling count follows the fixed header.
SIBLING_COUNT_OFFSET = FIXED_HEADER


class SigTagError(Exception):
    """Raised when the zone file cannot be parsed into an MTL signed ladder."""


def _split_zone_line(line: str) -> list[str]:
    """Split a presentation-format zone line, tolerating leading whitespace.

    Multi-line records (parenthesised) are not expected for RRSIG; the signed
    zones ldns produces place each RRSIG on a single physical line.
    """
    return line.split()


def find_rrsig_base64(zone_text: str, rtype: str, owner: str | None) -> str:
    """Return the base64 signature of the first matching RRSIG in a zone file.

    `rtype` is the covered type (e.g. "SOA"). `owner`, when given, must match
    the record owner case-insensitively, with or without the trailing dot.
    """
    want_owner = owner.rstrip(".").lower() if owner else None
    for raw in zone_text.splitlines():
        line = raw.split(";", 1)[0].strip()
        if not line:
            continue
        fields = _split_zone_line(line)
        # Typical RRSIG line:
        #   <owner> <ttl> IN RRSIG <type> <alg> <labels> ... <signer> <base64>
        # Find the RRSIG token, then read the covered type after it.
        try:
            idx = fields.index("RRSIG")
        except ValueError:
            continue
        if len(fields) < idx + 2:
            continue
        if fields[idx + 1].upper() != rtype.upper():
            continue
        if want_owner is not None:
            rec_owner = fields[0].rstrip(".").lower()
            if rec_owner != want_owner:
                continue
        if len(fields) < 2:
            continue
        return fields[-1]
    raise SigTagError(f"no RRSIG({rtype}) found for owner {owner!r}")


def _skip_name(data: bytes, offset: int) -> int:
    """Advance past a possibly-compressed DNS name; return the next offset."""
    while True:
        length = data[offset]
        if length & 0xC0:
            return offset + 2
        offset += 1
        if length == 0:
            return offset
        offset += length


def extract_signed_ladder(signature: bytes) -> bytes:
    """Strip the condensed part from a full MTL signature, yielding the ladder.

    Raises SigTagError if the bytes are too short to contain a well-formed
    condensed header.
    """
    if len(signature) < SIBLING_COUNT_OFFSET + 2:
        raise SigTagError(
            f"signature too short ({len(signature)} B) to hold a condensed header"
        )
    sibling_count = int.from_bytes(
        signature[SIBLING_COUNT_OFFSET : SIBLING_COUNT_OFFSET + 2], "big"
    )
    condensed_len = FIXED_HEADER + 2 + sibling_count * HASH_SIZE
    if len(signature) < condensed_len:
        raise SigTagError(
            f"signature length {len(signature)} B < condensed length {condensed_len} B"
        )
    ladder = signature[condensed_len:]
    if not ladder:
        raise SigTagError("signed ladder is empty (signature was already condensed)")
    return ladder


def sigtag_for_rrsig_base64(b64: str) -> str:
    """Decode a base64 RRSIG signature field and return its SigTag hex.

    `b64` is the trailing base64 blob of an RRSIG record, i.e. exactly the
    signature field from the DNS wire format (the fixed RRSIG prefix and the
    signer name are *not* included).
    """
    try:
        signature = base64.b64decode(b64, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise SigTagError(f"signature is not valid base64: {exc}") from exc
    ladder = extract_signed_ladder(signature)
    return hashlib.shake_128(ladder).hexdigest(32)


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(f"usage: {argv[0]} <signed-zone-file> [owner]", file=sys.stderr)
        return 2
    path = argv[1]
    owner = argv[2] if len(argv) == 3 else None
    try:
        with open(path, "r", encoding="utf-8") as handle:
            zone_text = handle.read()
    except OSError as exc:
        print(f"error: cannot read {path}: {exc}", file=sys.stderr)
        return 1
    try:
        b64 = find_rrsig_base64(zone_text, "SOA", owner)
        tag = sigtag_for_rrsig_base64(b64)
    except SigTagError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(tag)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
