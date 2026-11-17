#!/usr/bin/env bash
# Run the whole workspace test suite and keep the log.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
mkdir -p target/test-logs
cargo test --workspace 2>&1 | tee target/test-logs/workspace.log
