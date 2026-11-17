#!/usr/bin/env bash
# Test the viewer-scope crates with the same feature sets they are linted with.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
mkdir -p target/test-logs
cargo test -p climb-dns -p climb-analytical -p climb-tui --no-default-features --all-targets 2>&1 | tee target/test-logs/viewer-dns-analytical-tui.log
cargo test -p climb-report --no-default-features --all-targets 2>&1 | tee target/test-logs/viewer-report.log
