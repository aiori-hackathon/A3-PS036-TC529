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

Every packet's delay is applied by the kernel (tc netem) on the bridge, never
by this script. The probe records, for every query, how many UDP and TCP
messages it sent, the response bytes per transport, the total wall time and
the wall time divided by the configured target RTT.

Usage:
    python3 scripts/mtl_rtt_probe.py --server 192.168.13.2 \
        --name www.climb.example. --qtype A --samples 20 \
        [--sigtag <hex>] [--timeout 5] [--target-rtt-ms 20]

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


def median_int(values: list[int]) -> int:
    if not values:
        return 0
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


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
        "--target-rtt-ms",
        type=float,
        default=0.0,
        help="configured client-observed RTT, used only to report the measured "
        "wall time as a multiple of the target",
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

    latencies: list[float] = []
    queries: list[dict] = []
    udp_attempts: list[int] = []
    tcp_attempts: list[int] = []
    udp_bytes: list[int] = []
    tcp_bytes: list[int] = []
    multiples: list[float] = []
    truncated = 0
    used_tcp = False
    rrsig_kind = "none"
    response_bytes = 0

    for _ in range(max(1, args.samples)):
        try:
            started = time.perf_counter()
            udp_response = send_udp(query, args.server, args.port, args.timeout)
            udp_sent = 1
            tcp_sent = 0
            tcp_response_bytes = 0
            response = udp_response
            if is_truncated(udp_response):
                truncated += 1
                used_tcp = True
                tcp_response = send_tcp(query, args.server, args.port, args.timeout)
                tcp_sent = 1
                tcp_response_bytes = len(tcp_response)
                response = tcp_response
            elapsed_ms = (time.perf_counter() - started) * 1000.0
        except (socket.timeout, OSError, CaptureError) as error:
            raise SystemExit(f"query failed for {args.name} {args.qtype}: {error}")

        response_bytes = len(response)
        rrsig = first_rrsig(response)
        if rrsig is not None:
            rrsig_kind = {1: "full", 2: "condensed"}.get(rrsig[0], "unknown")
        latencies.append(elapsed_ms)
        udp_attempts.append(udp_sent)
        tcp_attempts.append(tcp_sent)
        udp_bytes.append(len(udp_response))
        tcp_bytes.append(tcp_response_bytes)
        multiple = (
            elapsed_ms / args.target_rtt_ms if args.target_rtt_ms > 0 else None
        )
        if multiple is not None:
            multiples.append(multiple)
        queries.append(
            {
                "udp_attempts": udp_sent,
                "tcp_attempts": tcp_sent,
                "udp_response_bytes": len(udp_response),
                "tcp_response_bytes": tcp_response_bytes,
                "total_ms": elapsed_ms,
                "rtt_multiple": multiple,
            }
        )

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
        "median_udp_attempts": median_int(udp_attempts),
        "median_tcp_attempts": median_int(tcp_attempts),
        "udp_response_bytes": median_int(udp_bytes),
        "tcp_response_bytes": median_int(tcp_bytes),
        "median_total_ms": percentile(latencies, 0.5),
        "median_rtt_multiple": percentile(multiples, 0.5) if multiples else None,
        "queries": queries,
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
