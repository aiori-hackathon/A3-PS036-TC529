#!/bin/bash
# Refresh the committed commit-graph artifacts and stage only those files.
#
# Runs the generator, stages the three artifacts it writes, prints the staged
# summary, and stops. It never commits and never pushes: the human reviews the
# staged change and writes the commit message.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

cargo xtask commit-graph

# Stage only the generated artifacts; nothing else in the tree is touched.
git add -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-compact.svg

echo ""
echo "Staged commit-graph artifacts:"
git diff --stat --cached -- docs/media/commit-graph.json docs/media/commit-graph.svg docs/media/commit-graph-compact.svg

echo ""
echo "Review the staged change above, then commit it with:"
echo "  git commit -m 'docs: refresh commit graph'"
echo "(This script does not commit or push.)"
