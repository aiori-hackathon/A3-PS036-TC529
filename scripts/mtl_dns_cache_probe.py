#!/usr/bin/env python3
"""Load R distinct DNS names into a resolver's cache and report what arrived.

The probe opens a single UDP socket, sends one query per name sequentially, and
records the response bytes per query. A query with the DO bit and an advertised
EDNS UDP payload of 65,535 bytes is used so even full MTL-signed responses fit
in one UDP datagram and no TCP fallback is needed for speed. When a SigTag is
passed, it is carried in EDNS option 65050 so an MTL server returns condensed
proofs backed by one shared ladder instead of full signatures.

No cryptography is implemented here: a SigTag (hex) is computed elsewhere and
passed in; this script only builds DNS messages, sends them, and sizes the
replies.

Usage:
    python3 scripts/mtl_dns_cache_probe.py \
        --server 192.168.13.3 --count 100 --base-name r \
        [--sigtag <hex>] [--timeout 5]

Output: one JSON object on stdout.
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
import sys
import time

from mtl_dns_capture import (
    QTYPE_NAMES,
    build_query,
    first_rrsig,
)

TC_FLAG = 0x0200


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round(fraction * (len(ordered) - 1))))
    return ordered[index]


def send_udp(payload: bytes, host: str, port: int, timeout: float) -> bytes:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(timeout)
        sock.sendto(payload, (host, port))
        data, _ = sock.recvfrom(65535)
        return data


def send_tcp(payload: bytes, host: str, port: int, timeout: float) -> bytes:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        sock.connect((host, port))
        sock.sendall(struct.pack(">H", len(payload)) + payload)
        (length,) = struct.unpack(">H", recv_exact(sock, 2))
        return recv_exact(sock, length)


def recv_exact(sock: socket.socket, count: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < count:
        piece = sock.recv(count - len(chunks))
        if not piece:
            raise OSError("connection closed before the full response arrived")
        chunks += piece
    return bytes(chunks)


def is_truncated(data: bytes) -> bool:
    if len(data) < 4:
        raise OSError("response shorter than a DNS header")
    (flags,) = struct.unpack(">H", data[2:4])
    return bool(flags & TC_FLAG)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True)
    parser.add_argument("--port", type=int, default=53)
    parser.add_argument(
        "--count", type=int, required=True, help="number of distinct names to query"
    )
    parser.add_argument(
        "--base-name", default="r", help="owner-name prefix; name i is <base><i>.climb.example."
    )
    parser.add_argument("--qtype", default="A")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument(
        "--sigtag", default=None, help="hex-encoded MTL SigTag (condensed mode)"
    )
    parser.add_argument(
        "--edns-buffer-size", type=int, default=65535,
        help="advertised EDNS UDP payload (65535 keeps full responses in one UDP datagram)",
    )
    args = parser.parse_args()

    qtype = QTYPE_NAMES.get(args.qtype.upper(), None)
    if qtype is None:
        try:
            qtype = int(args.qtype)
        except ValueError:
            print(f"unknown query type {args.qtype!r}", file=sys.stderr)
            return 2

    sigtag = bytes.fromhex(args.sigtag) if args.sigtag else None

    latencies: list[float] = []
    udp_bytes: list[int] = []
    tcp_bytes: list[int] = []
    truncated = 0
    tcp_queries = 0
    errors = 0
    response_bytes = 0

    started = time.perf_counter()
    for i in range(max(1, args.count)):
        name = f"{args.base_name}{i}.climb.example."
        query = build_query(name, qtype, sigtag, udp_size=args.edns_buffer_size)
        try:
            query_start = time.perf_counter()
            udp_response = send_udp(query, args.server, args.port, args.timeout)
            udb_len = len(udp_response)
            response = udp_response
            # One entry per query, so the per-query table below stays aligned
            # whether or not this particular response needed the TCP fallback.
            tcp_len = 0
            if is_truncated(udp_response):
                truncated += 1
                tcp_queries += 1
                tcp_response = send_tcp(query, args.server, args.port, args.timeout)
                response = tcp_response
                tcp_len = len(tcp_response)
            udp_bytes.append(udb_len)
            tcp_bytes.append(tcp_len)
            response_bytes = len(response)
            rrsig = first_rrsig(response)
            latencies.append((time.perf_counter() - query_start) * 1000.0)
        except (socket.timeout, OSError) as error:
            errors += 1
            latencies.append(0.0)
            udp_bytes.append(0)
            tcp_bytes.append(0)
            print(f"warning: query {i} ({name}) failed: {error}", file=sys.stderr)
    wall_ms = (time.perf_counter() - started) * 1000.0

    total_udp = sum(udp_bytes)
    total_tcp = sum(tcp_bytes)
    result = {
        "server": args.server,
        "count": args.count,
        "sigtag": args.sigtag,
        "qtype": args.qtype,
        "errors": errors,
        "truncated": truncated,
        "tcp_queries": tcp_queries,
        "wall_ms": wall_ms,
        "median_latency_ms": percentile(latencies, 0.5),
        "udp_response_bytes_total": total_udp,
        "tcp_response_bytes_total": total_tcp,
        "mean_response_bytes": (total_udp // args.count) if args.count else 0,
        "queries": [
            {
                "index": i,
                "name": f"{args.base_name}{i}.climb.example.",
                "udp_response_bytes": udp_bytes[i],
                "tcp_response_bytes": tcp_bytes[i],
            }
            for i in range(len(udp_bytes))
        ],
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
