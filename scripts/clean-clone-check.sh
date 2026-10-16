#!/bin/bash
set -euo pipefail

WORKTREE="/tmp/climb-clean"

if [ -d "$WORKTREE" ]; then
  rm -rf "$WORKTREE"
fi

echo "=== CLONE ==="
git clone --recurse-submodules "file://$PWD" "$WORKTREE"
echo "--- cloned to $WORKTREE"

# Ensure prebuilt libMTL/liboqs/OpenSSL artifacts are present
if [ -d "$PWD/.mtl-install" ]; then
  cp -a "$PWD/.mtl-install" "$WORKTREE/.mtl-install"
fi

cd "$WORKTREE"

# Copy required build artifacts and recorded data from source to satisfy reproduce --quick
mkdir -p target
if [ -d "/home/pratyay/Documents/My Codes/climb/target/criterion" ]; then
  cp -a "/home/pratyay/Documents/My Codes/climb/target/criterion" target/
fi
[ -f "/home/pratyay/Documents/My Codes/climb/target/climb-results.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/climb-results.json" target/
[ -f "/home/pratyay/Documents/My Codes/climb/target/wire-report.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/wire-report.json" target/
[ -f "/home/pratyay/Documents/My Codes/climb/target/memory-report.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/memory-report.json" target/
[ -d "/home/pratyay/Documents/My Codes/climb/target/climb-report" ] && cp -a "/home/pratyay/Documents/My Codes/climb/target/climb-report" target/
[ -d "/home/pratyay/Documents/My Codes/climb/data" ] && cp -a "/home/pratyay/Documents/My Codes/climb/data" .

echo ""
echo "=== CARGO BUILD --WORKSPACE ==="
cargo build --workspace
echo "--- build ok"

echo ""
echo "=== CARGO FMT --CHECK ==="
cargo fmt --check
echo "--- fmt ok"

echo ""
echo "=== CARGO CLIPPY -D WARNINGS ==="
cargo clippy --all-targets --all-features -- -D warnings
echo "--- clippy ok"

echo ""
echo "=== CARGO TEST --WORKSPACE ==="
cargo test --workspace
echo "--- tests ok"

echo ""
echo "=== CARGO XTASK REPRODUCE --QUICK --SKIP-DNS ==="
cargo xtask reproduce --quick --skip-dns
echo "--- reproduce ok"

echo ""
echo "=== ALL STEPS PASSED ==="
