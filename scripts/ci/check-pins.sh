#!/usr/bin/env bash
# Check every supply-chain pin: actions, images, submodules and copy sources.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
cargo run -p xtask --no-default-features -- check-pins
