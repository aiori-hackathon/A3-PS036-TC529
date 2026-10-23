#!/bin/bash
# Refresh the committed commit-graph artifacts from the public history and
# stage only the generated files.
#
# The generator fetches the public repository into a bare cache, reads the
# public branch, prints the public commit it stamped, writes the five media
# artifacts and updates the README permalinks. Because the card counts the
# refresh commit that will carry it, the staged count is one more than the
# commit count at the stamped as-of commit.
#
# This script stages the files, prints the staged summary, and stops. It never
# commits and never pushes: the human reviews the staged change and writes the
# commit message. `scripts/publish.sh` does the whole cycle in one step and is
# the usual way to publish; this script is the manual fallback.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

cargo xtask commit-graph --source public

public_sha=$(sed -n 's/^  "as_of_sha": "\([0-9a-f]\{40\}\)",$/\1/p' docs/media/commit-graph.json)
if [ -z "$public_sha" ]; then
  echo "error: docs/media/commit-graph.json carries no as_of_sha" >&2
  exit 1
fi

cache="target/commit-graph-source.git"
as_of_count=$(git -C "$cache" rev-list --count "$public_sha")
card_count=$(sed -n 's/^[[:space:]]*"commits": \([0-9][0-9]*\),$/\1/p' docs/media/commit-graph.json | head -n 1)
public_ref=$(git -C "$cache" for-each-ref --format='%(refname:short)' --points-at "$public_sha" refs/heads | head -n 1)
echo ""
echo "Public history used:"
echo "  commit:  $public_sha"
echo "  commits: $as_of_count reachable from that commit"
echo "  ref:     ${public_ref:-<none; the commit is not the tip of a branch>}"
echo "  card:    $card_count commits, counting the refresh commit that will carry it"

# Stage only the generated artifacts and the README permalink it rewrites.
git add -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-narrow.svg docs/media/commit-graph-compact.svg docs/media/commit-graph-compact-narrow.svg README.md

echo ""
echo "Staged commit-graph artifacts:"
git diff --stat --cached -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-narrow.svg docs/media/commit-graph-compact.svg docs/media/commit-graph-compact-narrow.svg README.md

echo ""
echo "Review the staged change above, then commit it with:"
echo "  git commit -m 'docs: refresh commit graph'"
echo ""
echo "Then publish it with tools/sync-aiori.sh, or let scripts/publish.sh do"
echo "the sync, the refresh and the convergence check in one command."
