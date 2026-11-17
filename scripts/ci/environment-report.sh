#!/usr/bin/env bash
# Print what the runner actually checked out before any test runs.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
echo "git HEAD: $(git rev-parse HEAD)"
echo "shallow clone: $(git rev-parse --is-shallow-repository)"
if [ "${CLIMB_REPO_ROLE:-}" = "private" ]; then
  echo "sample-check mode: require-commit (CLIMB_REPO_ROLE=private)"
else
  echo "sample-check mode: content identity; the recorded commit is checked only when present"
fi
