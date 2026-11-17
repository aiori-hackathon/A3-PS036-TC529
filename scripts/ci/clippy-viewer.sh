#!/usr/bin/env bash
# Lint the viewer-scope crates with every target enabled.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
mkdir -p target/test-logs
cargo clippy -p climb-dns -p climb-analytical -p climb-tui -p xtask --no-default-features --all-targets -- -D warnings 2>&1 | tee target/test-logs/clippy-viewer.log
cargo clippy -p climb-report --no-default-features --all-targets -- -D warnings 2>&1 | tee target/test-logs/clippy-report.log
