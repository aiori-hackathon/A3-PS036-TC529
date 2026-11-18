#!/usr/bin/env bash
# Print the pinned submodule commit ids as one hash, for cache keys.
#
# A cache key built from `.gitmodules` alone would not change when a submodule
# pin moves: the file carries the remote URL, the commit lives in the index. The
# extracted native toolchain is a function of those commits, so the key must
# change with them.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
git submodule status
sha="$(git submodule status | awk '{print $1}' | LC_ALL=C sort | sha256sum | cut -d' ' -f1)"
printf 'submodule pins: %s\n' "$sha"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  printf 'sha=%s\n' "$sha" >>"$GITHUB_OUTPUT"
fi
