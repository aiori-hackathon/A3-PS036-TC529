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
# The automation crate carries the documentation, link and workflow checks.
# Running its unit tests here is what turns a broken README link into a
# pre-push failure instead of a GitHub-only one.
cargo test -p xtask --no-default-features --all-targets 2>&1 | tee target/test-logs/viewer-xtask.log
