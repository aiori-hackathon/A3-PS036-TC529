#!/usr/bin/env bash
# Check every relative link in the README and docs.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
cargo run -p xtask --no-default-features -- link-check
