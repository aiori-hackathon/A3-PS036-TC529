#!/usr/bin/env python3
"""Capture raw DNS wire bytes for the CLIMB MTL live pipeline demo.

This is the *client* half of the MTL-mode protocol exchange: it issues DNSSEC
queries (DO bit set) with an optional MTL SigTag in EDNS option 65050, saves
each raw response to disk, and prints one row per query showing the exact
on-wire size and the first byte / length of the first RRSIG signature field.

That first byte is the whole point of the demo:

    0x01  full signature  — the response carries a freshly signed ladder
    0x02  condensed       — the response carries only a Merkle authentication
                             path, valid against a ladder the client already has

So a single query, run twice with and without the SigTag, shows the ~8 KB full
signature collapsing to a ~140 B condensed proof — the MTL bandwith saving,
captured as literal bytes.

No cryptography is implemented here. SHAKE-128 SigTags are computed by
``scripts/mtl_sigtag.py`` and passed in; this script only builds DNS messages,
sends them, and reads the replies. The DNS message construction is the same
well-known header/question/OPT format any stub resolver uses.

Usage:
    python3 scripts/mtl_dns_capture.py --server <host> --out <dir> \
        [--sigtag <hex>] [--tcp|--udp] \
        <name> <type> [<name> <type> ...]

Example:
    python3 scripts/mtl_dns_capture.py --server nsd --out /captures \
        --sigtag c3f5a117... climb.example. SOA www.climb.example. A

Prints a table to stdout; exits non-zero if any query fails.
"""

from __future__ import annotations

import argparse
import pathlib
import socket
import struct
import sys

QCLASS_IN = 1
TYPE_NAMES = {1: "A", 28: "AAAA", 6: "SOA", 15: "MX", 16: "TXT", 2: "NS", 46: "RRSIG"}
QTYPE_NAMES = {
    "A": 1,
    "NS": 2,
    "SOA": 6,
    "MX": 15,
    "TXT": 16,
    "AAAA": 28,
    "ANY": 255,
}
DO_BIT = 0x8000
EDNS_MTL_SIGTAG = 65050
QUERY_ID = 0x434C  # "CL" — constant so captures are byte-comparable across runs


class CaptureError(Exception):
    """Raised when a query cannot be sent, answered, or parsed."""


def encode_name(name: str) -> bytes:
    """Encode a domain name in uncompressed wire format."""
    out = bytearray()
    for label in name.rstrip(".").split("."):
        if not label:
            raise CaptureError(f"empty label in name {name!r}")
        encoded = label.encode("ascii")
        if len(encoded) > 63:
            raise CaptureError(f"label too long in name {name!r}")
        out.append(len(encoded))
        out += encoded
    out.append(0)
    return bytes(out)


def build_query(name: str, qtype: int, sigtag: bytes | None) -> bytes:
    """Build a query with the DO bit set and an optional MTL SigTag option."""
    question = encode_name(name) + struct.pack(">HH", qtype, QCLASS_IN)
    opts = b""
    if sigtag is not None:
        opts = struct.pack(">HH", EDNS_MTL_SIGTAG, len(sigtag)) + sigtag
    opt_rr = (
        b"\x00"  # root owner name
        + struct.pack(">HH", 41, 65535)  # OPT, advertised UDP payload
        + struct.pack(">I", DO_BIT)  # extended flags: DO
        + struct.pack(">H", len(opts))
        + opts
    )
    header = struct.pack(">HHHHHH", QUERY_ID, 0x0100, 1, 0, 0, 1)  # RD set
    return header + question + opt_rr


def skip_name(data: bytes, offset: int) -> int:
    """Advance past a possibly-compressed DNS name; return the next offset."""
    while True:
        length = data[offset]
        if length & 0xC0:
            return offset + 2
        offset += 1
        if length == 0:
            return offset
        offset += length


def first_rrsig(data: bytes) -> tuple[int, int] | None:
    """Return (lead_byte, signature_len) of the first RRSIG in the answer."""
    try:
        qd, an, _, _ = struct.unpack(">HHHH", data[4:12])
        offset = 12
        for _ in range(qd):
            offset = skip_name(data, offset) + 4
        for _ in range(an):
            name_end = skip_name(data, offset)
            rtype, _, _, rdlength = struct.unpack(">HHIH", data[name_end : name_end + 10])
            rdata = name_end + 10
            if rtype == 46:
                cursor = rdata + 18  # fixed RRSIG prefix before the signer name
                cursor = skip_name(data, cursor)
                sig = data[cursor : rdata + rdlength]
                if not sig:
                    return None
                return sig[0], len(sig)
            offset = rdata + rdlength
    except (struct.error, IndexError):
        return None
    return None


def rcode(data: bytes) -> int:
    return data[3] & 0x0F


def exchange(payload: bytes, host: str, port: int, tcp: bool, timeout: float) -> bytes:
    """Send one query and return the raw response bytes."""
    if tcp:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(timeout)
            sock.connect((host, port))
            sock.sendall(struct.pack(">H", len(payload)) + payload)
            header = _recv_exact(sock, 2)
            (length,) = struct.unpack(">H", header)
            return _recv_exact(sock, length)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(timeout)
        sock.sendto(payload, (host, port))
        data, _ = sock.recvfrom(65535)
        return data


def _recv_exact(sock: socket.socket, count: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < count:
        piece = sock.recv(count - len(chunks))
        if not piece:
            raise CaptureError("connection closed before the full response arrived")
        chunks += piece
    return bytes(chunks)


def parse_pairs(tokens: list[str]) -> list[tuple[str, int]]:
    if len(tokens) % 2 != 0:
        raise CaptureError("queries must be given as <name> <type> pairs")
    pairs = []
    for name, qtype in zip(tokens[::2], tokens[1::2]):
        key = qtype.upper()
        if key not in QTYPE_NAMES:
            raise CaptureError(f"unsupported query type {qtype!r}")
        pairs.append((name, QTYPE_NAMES[key]))
    return pairs


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True)
    parser.add_argument("--port", type=int, default=53)
    parser.add_argument("--out", required=True)
    parser.add_argument("--sigtag", default=None, help="hex SHAKE-128 ladder hash")
    parser.add_argument("--timeout", type=float, default=8.0)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--tcp", action="store_true", default=True)
    mode.add_argument("--udp", action="store_true")
    parser.add_argument("queries", nargs="+", help="<name> <type> [<name> <type> ...]")
    args = parser.parse_args(argv[1:])

    sigtag = None
    if args.sigtag is not None:
        try:
            sigtag = bytes.fromhex(args.sigtag)
        except ValueError as exc:
            print(f"error: --sigtag is not valid hex: {exc}", file=sys.stderr)
            return 2
        if len(sigtag) % 32 != 0:
            print("error: --sigtag must be a multiple of 32 bytes", file=sys.stderr)
            return 2

    try:
        pairs = parse_pairs(args.queries)
    except CaptureError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    tag_suffix = "sigtag" if sigtag is not None else "full"

    print(f"\n  wire capture → {out_dir}  (server {args.server}, "
          f"{'TCP' if args.tcp and not args.udp else 'UDP'}, mode={tag_suffix})\n")
    header = f"  {'query':<28} {'type':<5} {'bytes':>7}  {'rcode':<6} {'rrsig':<16} file"
    print(header)
    print("  " + "-" * (len(header) - 2))

    failures = 0
    for index, (name, qtype) in enumerate(pairs):
        label = TYPE_NAMES.get(qtype, str(qtype))
        payload = build_query(name, qtype, sigtag)
        try:
            response = exchange(payload, args.server, args.port, not args.udp, args.timeout)
        except (OSError, CaptureError) as exc:
            print(f"  {name:<28} {label:<5} {'FAIL':>7}  {exc}")
            failures += 1
            continue
        rrsig = first_rrsig(response)
        if rrsig is None:
            sig_cell = "—"
        else:
            lead, length = rrsig
            kind = {1: "full", 2: "condensed"}.get(lead, f"0x{lead:02x}")
            sig_cell = f"{kind} {length}B"
        filename = f"{index:02d}-{label.lower()}-{tag_suffix}.bin"
        (out_dir / filename).write_bytes(response)
        print(f"  {name:<28} {label:<5} {len(response):>7}  {rcode(response):<6} "
              f"{sig_cell:<16} {filename}")

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
