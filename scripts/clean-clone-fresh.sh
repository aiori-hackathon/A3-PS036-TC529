#!/bin/bash
set -euo pipefail

# Fresh clean-clone check.
#
# This script clones the committed history into a temporary directory, with
# submodules, and copies NOTHING from the working tree: no target directory, no
# native prefix, no measured artifacts and no uncommitted edits. It therefore
# proves that a new checkout can build and verify itself, which the
# cache-assisted script deliberately does not prove.
#
#   scripts/clean-clone-fresh.sh --viewer-only
#       Fast, no native libraries. Clones, then builds, lints and smoke-runs
#       only the read-only viewer with `--no-default-features`, checks that the
#       resolved dependency tree contains no FFI crate, and confirms the
#       checkout is clean.
#
#   scripts/clean-clone-fresh.sh --full
#       Long. Clones, builds the pinned native toolchain in Docker, extracts it,
#       builds and tests the whole workspace, runs every documentation and pin
#       gate, then re-measures the artifact chain and builds the viewer.
#
# Both modes print every command and the wall time. Because a clone contains
# committed objects only, uncommitted working-tree edits are NOT tested here:
# commit first, then run this script to sign off a change. The cache-assisted
# script is the one that exercises the working tree.

usage() {
    cat <<'EOF'
usage: scripts/clean-clone-fresh.sh MODE

  --viewer-only   clone and verify the viewer with no native libraries (fast)
  --full          clone and verify the full native stack in Docker (long)

environment:
  CLIMB_FRESH_CLONE_DIR   clone destination (default: a fresh temporary directory)
  CLIMB_KEEP_CLONE=1      keep the clone after the run and print its path
EOF
}

MODE=""
case "${1:-}" in
    --viewer-only) MODE="viewer-only" ;;
    --full) MODE="full" ;;
    -h|--help) usage; exit 0 ;;
    *)
        usage >&2
        echo "error: pass exactly one of --viewer-only or --full" >&2
        exit 2
        ;;
esac
if [ "$#" -ne 1 ]; then
    usage >&2
    echo "error: unexpected extra arguments" >&2
    exit 2
fi

START_EPOCH="$(date +%s)"
ROOT="$(git rev-parse --show-toplevel)"
COMMIT="$(git -C "${ROOT}" rev-parse HEAD)"
CREATED_CLONE=0
if [ -n "${CLIMB_FRESH_CLONE_DIR:-}" ]; then
    WORKTREE="${CLIMB_FRESH_CLONE_DIR}"
    if [ -e "${WORKTREE}" ]; then
        rm -rf "${WORKTREE}"
    fi
    mkdir -p "$(dirname "${WORKTREE}")"
else
    WORKTREE="$(mktemp -d "${TMPDIR:-/tmp}/climb-fresh.XXXXXX")"
    rmdir "${WORKTREE}"
    CREATED_CLONE=1
fi

cleanup() {
    if [ "${CLIMB_KEEP_CLONE:-0}" = "1" ] || [ "${CREATED_CLONE}" = "0" ]; then
        echo ""
        echo "clone kept at: ${WORKTREE}"
    else
        rm -rf "${WORKTREE}"
    fi
}
trap cleanup EXIT

# Print the exact command, run it, and report its wall time. `bash -c` keeps
# quoting readable in the log while still failing on the first error.
run() {
    local command="$1"
    local step_start step_end
    echo ""
    echo "+ ${command}"
    step_start="$(date +%s)"
    bash -euo pipefail -c "${command}"
    step_end="$(date +%s)"
    echo "--- ok ($((step_end - step_start)) s)"
}

echo "fresh clean-clone check (${MODE})"
echo "repository: ${ROOT}"
echo "commit:     ${COMMIT}"
echo "clone:      ${WORKTREE}"
echo "detail:     nothing is copied from the working tree; the clone contains committed objects only"

run "git clone --recurse-submodules 'file://${ROOT}' '${WORKTREE}'"
run "git -C '${WORKTREE}' submodule update --init --recursive"

# A fresh clone must not inherit build output. If it does, either the clone
# source is dirty or the script copied something; fail rather than measure a
# warm tree.
if [ -e "${WORKTREE}/target" ] || [ -e "${WORKTREE}/.mtl-install" ]; then
    echo "error: fresh clone already contains target/ or .mtl-install/" >&2
    exit 1
fi
if [ -n "$(git -C "${WORKTREE}" status --porcelain)" ]; then
    echo "error: fresh clone is not clean:" >&2
    git -C "${WORKTREE}" status --short >&2
    exit 1
fi
echo ""
echo "--- ok: no target/, no .mtl-install/, clean working tree"

if [ "${MODE}" = "viewer-only" ]; then
    # Formatting is limited to the viewer crate here: an older checkout may
    # have unrelated formatting drift, which the full mode still checks.
    run "cd '${WORKTREE}' && cargo fmt -p climb-tui --check"
    # The viewer path must not need a native prefix. `check-viewer-build.sh`
    # builds with `--no-default-features` and scans the resolved tree for FFI.
    # Only bin targets are linted here: a test build compiles dev-dependencies,
    # and an older checkout may still pull the native stack through one of
    # them. The full mode runs the workspace test suite.
    run "cd '${WORKTREE}' && scripts/check-viewer-build.sh"
    run "cd '${WORKTREE}' && cargo clippy -p climb-tui --no-default-features --bins -- -D warnings"
    run "cd '${WORKTREE}' && ./target/debug/climb-tui --help >/dev/null"
else
    # Full mode builds the pinned toolchain in Docker. The image is tagged with
    # the checked-out commit so a stale local image cannot be mistaken for a
    # fresh build; `docker/extract-libs.sh` then populates `.mtl-install/`.
    run "cd '${WORKTREE}' && docker build -f docker/Dockerfile.build -t climb-builder:fresh-${COMMIT} ."
    run "cd '${WORKTREE}' && docker/extract-libs.sh 'climb-builder:fresh-${COMMIT}'"

    run "cd '${WORKTREE}' && cargo fmt --check"
    run "cd '${WORKTREE}' && cargo build --workspace"
    run "cd '${WORKTREE}' && cargo clippy --all-targets --all-features -- -D warnings"
    run "cd '${WORKTREE}' && cargo test --workspace"

    # Documentation, hygiene and supply-chain gates. These run after the native
    # prefix exists because `xtask` links the measurement stack.
    run "cd '${WORKTREE}' && cargo xtask docs-check"
    run "cd '${WORKTREE}' && cargo xtask source-hygiene"
    run "cd '${WORKTREE}' && cargo xtask check-pins"

    # Re-measure the whole artifact chain; report-only would reuse nothing in a
    # fresh clone, so it is not a valid freshness check.
    run "cd '${WORKTREE}' && cargo xtask reproduce --remeasure --skip-dns"

    run "cd '${WORKTREE}' && scripts/check-viewer-build.sh"
    run "cd '${WORKTREE}' && cargo xtask dist-tui --out dist --skip-smoke"
fi

END_EPOCH="$(date +%s)"
echo ""
echo "=== FRESH ${MODE} CHECK PASSED in $((END_EPOCH - START_EPOCH)) s ==="
if [ "${MODE}" = "viewer-only" ]; then
    echo "Proves: the committed tree builds, lints and smoke-runs the viewer with no native prefix."
    echo "Does not prove: the native stack, measurements, or uncommitted edits."
    echo "For the full claim run: scripts/clean-clone-fresh.sh --full"
else
    echo "Proves: the committed tree builds the pinned native stack, passes every gate,"
    echo "re-measures the artifact chain and builds the viewer from a cold checkout."
fi
