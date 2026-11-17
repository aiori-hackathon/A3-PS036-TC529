#!/usr/bin/env bash
# Fetch every branch and tag so history-dependent checks see the whole public history.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
git fetch origin "+refs/heads/*:refs/remotes/origin/*"
