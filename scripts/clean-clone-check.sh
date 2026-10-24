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
echo "=== STEP 6: viewer build without a native prefix ==="
scripts/check-viewer-build.sh
echo "--- ok"

echo ""
echo "=== STEP 7: prebuilt viewer tarball ==="
cargo xtask dist-tui --out dist --skip-smoke
echo "--- ok"

echo ""
echo "=== STEP 8: full reproduction commands (opt-in) ==="
# These are the three commands the README's full-reproduction quickstart
# documents. The native toolchain build is expensive and the dashboard is
# interactive, so they run only when CLIMB_FULL_QUICKSTART=1.
if [ "${CLIMB_FULL_QUICKSTART:-0}" = "1" ]; then
  docker build -f docker/Dockerfile.build -t climb-builder:phase1 . && docker/extract-libs.sh
  cargo run -p climb-tui
else
  echo "--- skipped (set CLIMB_FULL_QUICKSTART=1 to run the native build and the dashboard)"
fi

echo ""
echo "=== STEP 9: viewer container (opt-in) ==="
# The fastest-path container: build it and open the guided tour under a
# pseudo-terminal, quitting with Esc.
if [ "${CLIMB_DOCKER_SMOKE:-0}" = "1" ]; then
  docker build -f docker/Dockerfile.tui -t climb-tui:check .
  script -qec "docker run -it --rm climb-tui:check --tour" /dev/null
else
  echo "--- skipped (set CLIMB_DOCKER_SMOKE=1 to build and run the viewer image)"
fi

echo ""
echo "=== ALL STEPS PASSED ==="
