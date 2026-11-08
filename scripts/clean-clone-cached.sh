#!/bin/bash
set -euo pipefail

# Cache-assisted clean-clone check.
#
# This is the historical check: it clones the committed tree, then copies the
# working tree's native prefix, Criterion output and generated artifacts into
# the clone so the expensive measurement steps do not have to be repeated.
# It proves that the build, the gates and the report-only regeneration work
# from a clone, but it does NOT prove a fresh checkout. Use
# scripts/clean-clone-fresh.sh for that claim.

START_EPOCH="$(date +%s)"
ROOT="$(git rev-parse --show-toplevel)"
WORKTREE="${CLIMB_CACHED_CLONE_DIR:-/tmp/climb-clean-cached}"

run() {
    local command="$1"
    local step_start
    local step_end
    echo ""
    echo "+ ${command}"
    step_start="$(date +%s)"
    bash -euo pipefail -c "${command}"
    step_end="$(date +%s)"
    echo "--- ok ($((step_end - step_start)) s)"
}

echo "cache-assisted clean-clone check"
echo "repository: ${ROOT}"
echo "clone:      ${WORKTREE}"
echo "detail:     measurement artifacts and the native prefix are copied from the working tree"

if [ -d "${WORKTREE}" ]; then
    run "rm -rf '${WORKTREE}'"
fi

run "git clone --recurse-submodules 'file://${ROOT}' '${WORKTREE}'"

# Cache-assisted by design: copy the working tree's build and measurement
# outputs so the derived chain can run without re-measuring.
if [ -d "${ROOT}/.mtl-install" ]; then
    echo ""
    echo "+ cp -a '${ROOT}/.mtl-install' '${WORKTREE}/.mtl-install'"
    cp -a "${ROOT}/.mtl-install" "${WORKTREE}/.mtl-install"
fi
mkdir -p "${WORKTREE}/target"
for entry in criterion climb-results.json wire-report.json memory-report.json climb-report; do
    if [ -e "${ROOT}/target/${entry}" ]; then
        echo "+ cp -a '${ROOT}/target/${entry}' '${WORKTREE}/target/${entry}'"
        cp -a "${ROOT}/target/${entry}" "${WORKTREE}/target/${entry}"
    fi
done
if [ -d "${ROOT}/data" ]; then
    echo "+ cp -a '${ROOT}/data' '${WORKTREE}/data'"
    cp -a "${ROOT}/data" "${WORKTREE}/data"
fi

cd "${WORKTREE}"

run "cargo build --workspace"
run "cargo fmt --check"
run "cargo clippy --all-targets --all-features -- -D warnings"
run "cargo test --workspace"
run "cargo xtask reproduce --report-only --skip-dns"
run "scripts/check-viewer-build.sh"
run "cargo xtask dist-tui --out dist --skip-smoke"

echo ""
echo "=== optional: full reproduction commands ==="
if [ "${CLIMB_FULL_QUICKSTART:-0}" = "1" ]; then
    run "docker build -f docker/Dockerfile.build -t climb-builder:phase1 ."
    run "docker/extract-libs.sh"
    run "cargo run -p climb-tui"
else
    echo "--- skipped; set CLIMB_FULL_QUICKSTART=1 to build the native stack and open the dashboard"
fi

echo ""
echo "=== optional: viewer container ==="
if [ "${CLIMB_DOCKER_SMOKE:-0}" = "1" ]; then
    run "docker build -f docker/Dockerfile.tui -t climb-tui:check ."
    run "script -qec \"docker run -it --rm climb-tui:check --tour\" /dev/null"
else
    echo "--- skipped; set CLIMB_DOCKER_SMOKE=1 to build and tour the viewer image"
fi

END_EPOCH="$(date +%s)"
echo ""
echo "=== CACHE-ASSISTED CHECK PASSED in $((END_EPOCH - START_EPOCH)) s ==="
echo "This run copied artifacts from the working tree; it does not prove a fresh checkout."
echo "For that claim run: scripts/clean-clone-fresh.sh --viewer-only (or --full)"
