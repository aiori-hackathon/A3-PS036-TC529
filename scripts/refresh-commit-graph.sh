#!/bin/bash
# Refresh the committed commit-graph artifacts from the public history and
# stage only the graph files.
#
# The generator fetches the public repository into a bare cache, reads the
# public branch, prints the public commit it stamped, writes the three media
# artifacts and updates the README's commit permalink. This script stages those
# files, prints the staged summary, and stops. It never commits and never
# pushes: the human reviews the staged change and writes the commit message.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

cargo xtask commit-graph --source public

public_sha=$(sed -n 's/^  "as_of_sha": "\([0-9a-f]\{40\}\)",$/\1/p' docs/media/commit-graph.json)
if [ -z "$public_sha" ]; then
  echo "error: docs/media/commit-graph.json carries no as_of_sha" >&2
  exit 1
fi

cache="target/commit-graph-source.git"
public_ref=$(git -C "$cache" for-each-ref --format='%(refname:short)' --points-at "$public_sha" refs/heads | head -n 1)
echo ""
echo "Public history used:"
echo "  commit:  $public_sha"
echo "  commits: $(git -C "$cache" rev-list --count "$public_sha") reachable from that commit"
echo "  ref:     ${public_ref:-<none; the commit is not the tip of a branch>}"

# Stage only the generated artifacts and the README permalink it rewrites.
git add -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-narrow.svg README.md

echo ""
echo "Staged commit-graph artifacts:"
git diff --stat --cached -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-narrow.svg README.md

echo ""
echo "Review the staged change above, then commit it with:"
echo "  git commit -m 'docs: refresh commit graph'"
echo "(This script does not commit or push. The next sync publishes the card.)"
