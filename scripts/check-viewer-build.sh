#!/bin/bash
set -euo pipefail

# Verify that the dashboard builds and links with no native crypto prefix.
#
# The viewer only reads recorded artifacts, so the path a downloaded binary and
# the container image take must not need libMTL, liboqs or OpenSSL. Any
# `*-sys` binding crate, liboqs or OpenSSL in the resolved dependency tree is a
# failure: it means the feature split regressed and the "no prefix" promise is
# no longer true.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# A prefix exported for the native benchmark build must not leak into this
# check; otherwise a successful build would prove nothing.
unset CLIMB_MTL_PREFIX CLIMB_MTL_INCLUDE CLIMB_MTL_LIB

echo "=== viewer build: climb-tui --no-default-features ==="
cargo build -p climb-tui --no-default-features
echo "--- ok"

echo "=== viewer dependency tree scan ==="
tree="$(cargo tree --no-default-features -p climb-tui -e normal)"

# `linux-raw-sys` is a pure-Rust table of kernel constants pulled in by
# crossterm/rustix. It ships no C code, no build script and no linked library,
# so it is the single allowlisted `*-sys` name. Every real FFI binding crate,
# liboqs and OpenSSL remain banned.
native="$(printf '%s\n' "$tree" \
  | grep -Ei 'liboqs|openssl|climb-mtl|(^|[^a-z0-9_-])[a-z0-9_-]+-sys([^a-z0-9_-]|$)' \
  | grep -v 'linux-raw-sys' || true)"

if [ -n "$native" ]; then
  echo "error: the viewer build resolves native code:" >&2
  printf '%s\n' "$native" >&2
  exit 1
fi

echo "--- ok: no native crypto or FFI crates in the viewer tree"
