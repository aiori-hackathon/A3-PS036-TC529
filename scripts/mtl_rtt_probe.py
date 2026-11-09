#!/usr/bin/env python3
"""Measure DNS query latency for one (server, response mode) cell.

The probe runs *inside* the pipeline's Docker network, so the only latency it
observes is the client/server exchange plus any delay the harness applied to
the bridge. It sends each query over UDP with an advertised EDNS payload of
1,232 bytes; when the server sets the TC bit the probe retries the same query
over TCP and measures the whole exchange.

No cryptography is implemented here. A SigTag is computed elsewhere and passed
in as hex; this script only builds DNS messages, sends them, and times the
replies.

Usage:
    python3 scripts/mtl_rtt_probe.py --server 192.168.13.2 \
        --name www.climb.example. --qtype A --samples 20 \
        [--sigtag <hex>] [--timeout 5] [--userspace-delay-ms 20]

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
    CaptureError,
    QTYPE_NAMES,
    build_query,
    first_rrsig,
)

UDP_PAYLOAD = 1232
TC_FLAG = 0x0200


def recv_exact(sock: socket.socket, count: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < count:
        piece = sock.recv(count - len(chunks))
        if not piece:
            raise CaptureError("connection closed before the full response arrived")
        chunks += piece
    return bytes(chunks)


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


def is_truncated(data: bytes) -> bool:
    if len(data) < 4:
        raise CaptureError("response shorter than a DNS header")
    (flags,) = struct.unpack(">H", data[2:4])
    return bool(flags & TC_FLAG)


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round(fraction * (len(ordered) - 1))))
    return ordered[index]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True)
    parser.add_argument("--port", type=int, default=53)
    parser.add_argument("--name", required=True)
    parser.add_argument("--qtype", default="A")
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--sigtag", default=None, help="hex-encoded MTL SigTag")
    parser.add_argument(
        "--userspace-delay-ms",
        type=float,
        default=0.0,
        help="sleep half this before the query and half after the reply, used "
        "only when the bridge cannot apply a kernel delay",
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
    query = build_query(args.name, qtype, sigtag, udp_size=UDP_PAYLOAD)
    half_sleep = max(0.0, args.userspace_delay_ms) / 2000.0

    latencies: list[float] = []
    truncated = 0
    used_tcp = False
    rrsig_kind = "none"
    response_bytes = 0

    for _ in range(max(1, args.samples)):
        try:
            if half_sleep:
                time.sleep(half_sleep)
            started = time.perf_counter()
            response = send_udp(query, args.server, args.port, args.timeout)
            if is_truncated(response):
                truncated += 1
                used_tcp = True
                response = send_tcp(query, args.server, args.port, args.timeout)
            elapsed_ms = (time.perf_counter() - started) * 1000.0
            if half_sleep:
                time.sleep(half_sleep)
                elapsed_ms += half_sleep * 2000.0
        except (socket.timeout, OSError, CaptureError) as error:
            raise SystemExit(f"query failed for {args.name} {args.qtype}: {error}")

        response_bytes = len(response)
        rrsig = first_rrsig(response)
        if rrsig is not None:
            rrsig_kind = {1: "full", 2: "condensed"}.get(rrsig[0], "unknown")
        latencies.append(elapsed_ms)

    result = {
        "server": args.server,
        "name": args.name,
        "qtype": args.qtype,
        "samples": latencies,
        "count": len(latencies),
        "truncated": truncated,
        "transport": "udp-then-tcp" if used_tcp else "udp",
        "rrsig_kind": rrsig_kind,
        "query_bytes": len(query),
        "response_bytes": response_bytes,
        "median_ms": percentile(latencies, 0.5),
        "p95_ms": percentile(latencies, 0.95),
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
