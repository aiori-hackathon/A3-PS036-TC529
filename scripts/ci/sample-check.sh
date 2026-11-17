#!/usr/bin/env bash
# Check the committed sample pair without the native toolchain.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
cargo run -p xtask --no-default-features -- sample-check
