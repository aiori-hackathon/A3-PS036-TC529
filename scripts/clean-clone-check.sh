#!/bin/bash
set -euo pipefail

WORKTREE="/tmp/climb-clean"

if [ -d "$WORKTREE" ]; then
  rm -rf "$WORKTREE"
fi

echo "=== CLONE ==="
git clone --recurse-submodules "file://$PWD" "$WORKTREE"
echo "--- cloned to $WORKTREE"

# Ensure prebuilt artifacts exist if present in source (for local runs)
if [ -d "$PWD/.mtl-install" ]; then
  cp -a "$PWD/.mtl-install" "$WORKTREE/.mtl-install"
fi

cd "$WORKTREE"

# Copy required build artifacts and recorded data if available
mkdir -p target
[ -d "/home/pratyay/Documents/My Codes/climb/target/criterion" ] && cp -a "/home/pratyay/Documents/My Codes/climb/target/criterion" target/
[ -f "/home/pratyay/Documents/My Codes/climb/target/climb-results.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/climb-results.json" target/
[ -f "/home/pratyay/Documents/My Codes/climb/target/wire-report.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/wire-report.json" target/
[ -f "/home/pratyay/Documents/My Codes/climb/target/memory-report.json" ] && cp "/home/pratyay/Documents/My Codes/climb/target/memory-report.json" target/
[ -d "/home/pratyay/Documents/My Codes/climb/target/climb-report" ] && cp -a "/home/pratyay/Documents/My Codes/climb/target/climb-report" target/
[ -d "/home/pratyay/Documents/My Codes/climb/data" ] && cp -a "/home/pratyay/Documents/My Codes/climb/data" .

echo ""
echo "=== STEP 1: cargo build --workspace ==="
cargo build --workspace
echo "--- ok"

echo ""
echo "=== STEP 2: cargo fmt --check ==="
cargo fmt --check
echo "--- ok"

echo ""
echo "=== STEP 3: cargo clippy -D warnings ==="
cargo clippy --all-targets --all-features -- -D warnings
echo "--- ok"

echo ""
echo "=== STEP 4: cargo test --workspace ==="
cargo test --workspace
echo "--- ok"

echo ""
echo "=== STEP 5: cargo xtask reproduce --quick --skip-dns ==="
cargo xtask reproduce --quick --skip-dns
echo "--- ok"

echo ""
echo "=== ALL STEPS PASSED ==="
