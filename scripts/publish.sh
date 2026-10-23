#!/usr/bin/env bash
# Publish the current branch, refresh the public commit graph, and prove the
# card and the public repository agree.
#
# The flow, in order:
#   1. push the commits the sync will read to the private upstream;
#   2. run the sync script that publishes a cleaned copy to the public
#      repository;
#   3. ask the graph generator whether the card is current. A stale card is
#      regenerated so that it counts the refresh commit that will carry it; the
#      generated files are committed with a fixed message and published;
#   4. ask again and require "up to date", which proves the cycle converged;
#   5. print the public commit count next to the number on the card and fail if
#      they differ.
#
# The script never force-pushes, never rewrites history and never edits the sync
# script. The only commit it creates is the generated graph refresh, and its
# message carries no tooling names. Every step is appended to
# target/publish.log.
#
# The graph refresh touches the card JSON, its four SVG cards and README.md,
# because the README permalink must name the same as-of commit the card stamps.
# Publishing in batches is cheaper than after every commit: each publish with
# new work adds one small refresh commit to the public history.
set -euo pipefail

# The autopublish hook runs this script after a commit. Refuse to re-enter when
# a publish is already in progress: the refresh commit this script creates would
# otherwise start another publish, and so on. The variable is exported below so
# the post-commit hook sees it and stays out of the way.
if [ "${CLIMB_AUTOPUBLISH_RUNNING:-}" = "1" ]; then
  echo "error: a publish is already running in this process tree." >&2
  exit 1
fi
export CLIMB_AUTOPUBLISH_RUNNING=1

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
mkdir -p target

log_file="target/publish.log"
exec > >(tee -a "$log_file") 2>&1

lock_file="target/publish.lock"
exec 9>"$lock_file"
if ! flock -n 9; then
  echo "error: another publish is already running (lock: $lock_file)" >&2
  exit 1
fi

echo "publish: repository $repo_root"
echo "publish: log $log_file"

if [ -n "$(git status --porcelain)" ]; then
  echo "error: the working tree is not clean; publish only committed work." >&2
  git status --short >&2
  echo "Commit or stash the changes above, then run this script again." >&2
  exit 1
fi

sync_script="${CLIMB_SYNC_SCRIPT:-}"
if [ -z "$sync_script" ]; then
  for candidate in \
    "$repo_root/tools/sync-aiori.sh" \
    "$repo_root/../tools/sync-aiori.sh"; do
    if [ -x "$candidate" ]; then
      sync_script="$candidate"
      break
    fi
  done
fi
if [ -z "$sync_script" ] || [ ! -x "$sync_script" ]; then
  echo "error: cannot find the script that publishes to the public repository." >&2
  echo "Set CLIMB_SYNC_SCRIPT to its path (it lives outside this repository and is never modified)." >&2
  exit 1
fi

if [ -z "$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)" ]; then
  echo "error: the current branch has no upstream, so its commits cannot reach the sync." >&2
  echo "Set one with: git branch --set-upstream-to=origin/<branch>" >&2
  exit 1
fi

xtask_cmd=(cargo xtask)
if [ -n "${CLIMB_XTASK:-}" ]; then
  # One executable path, never a command line: paths may contain spaces.
  xtask_cmd=("$CLIMB_XTASK")
fi

echo ""
echo "==> 1/5 push the private commits the sync will read"
git push --quiet
echo "    private upstream is now at $(git rev-parse --short HEAD)"

echo ""
echo "==> 2/5 publish to the public repository"
echo "    sync: $sync_script"
if ! "$sync_script"; then
  echo "error: the sync failed, so its publish gate refused or the push failed." >&2
  echo "Nothing was committed or pushed by this script after that point." >&2
  exit 1
fi

echo ""
echo "==> 3/5 refresh the card if it is stale"
auto_status=0
auto_output="$("${xtask_cmd[@]}" commit-graph --auto --source public 2>&1)" || auto_status=$?
printf '%s\n' "$auto_output"

if [ "$auto_status" -eq 0 ]; then
  echo "    the card already describes the public tip; no refresh commit needed"
  echo ""
  echo "==> 4/5 skipped: there is no refresh commit to publish"
elif [ "$auto_status" -eq 10 ]; then
  git add -- \
    docs/media/commit-graph.json \
    docs/media/commit-graph.svg \
    docs/media/commit-graph-narrow.svg \
    docs/media/commit-graph-compact.svg \
    docs/media/commit-graph-compact-narrow.svg \
    README.md
  unexpected="$(git diff --cached --name-only \
    | grep -vE '^(README\.md|docs/media/commit-graph(-narrow|-compact|-compact-narrow)?\.(json|svg))$' \
    || true)"
  if [ -n "$unexpected" ]; then
    echo "error: the refresh staged files outside the generated surface:" >&2
    printf '%s\n' "$unexpected" >&2
    exit 1
  fi
  if git diff --cached --quiet; then
    echo "error: the generator reported a change but staged nothing." >&2
    exit 1
  fi
  git commit --quiet -m 'docs: refresh commit graph'
  echo "    committed $(git rev-parse --short HEAD) (generated graph files only)"
  git push --quiet
  echo "    pushed the refresh commit"

  echo ""
  echo "==> 4/5 publish the refresh commit and require convergence"
  if ! "$sync_script"; then
    echo "error: the second sync failed; the public tip may not carry the new card." >&2
    exit 1
  fi
  auto_status=0
  auto_output="$("${xtask_cmd[@]}" commit-graph --auto --source public 2>&1)" || auto_status=$?
  printf '%s\n' "$auto_output"
  if [ "$auto_status" -ne 0 ] || [[ "$auto_output" != *"up to date"* ]]; then
    echo "error: the card did not converge after the refresh commit (exit $auto_status)." >&2
    echo "Compare the numbers printed above and inspect the public branch before retrying." >&2
    exit 1
  fi
else
  echo "error: the generator failed (exit $auto_status)." >&2
  exit 1
fi

echo ""
echo "==> 5/5 compare the card with the public repository"
source_ref="$(sed -n 's/^[[:space:]]*source_ref = "\(.*\)"$/\1/p' docs/readme-config.toml | head -n 1)"
source_cache="$(sed -n 's/^[[:space:]]*source_cache = "\(.*\)"$/\1/p' docs/readme-config.toml | head -n 1)"
source_ref="${source_ref:-main}"
source_cache="${source_cache:-target/commit-graph-source.git}"
case "$source_cache" in
  /*) ;;
  *) source_cache="$repo_root/$source_cache" ;;
esac
card_count="$(sed -n 's/^[[:space:]]*"commits": \([0-9][0-9]*\),$/\1/p' docs/media/commit-graph.json | head -n 1)"
public_count="$(git -C "$source_cache" rev-list --count "refs/heads/$source_ref")"
if [ -z "$card_count" ]; then
  echo "error: could not read totals.commits from docs/media/commit-graph.json" >&2
  exit 1
fi
echo "    card says:   $card_count commits"
echo "    public shows: $public_count commits (refs/heads/$source_ref)"
if [ "$card_count" != "$public_count" ]; then
  echo "error: the card and the public repository disagree." >&2
  echo "Regenerate with: cargo xtask commit-graph --auto --source public" >&2
  exit 1
fi
echo ""
echo "publish: done; the card and the public commit count agree."
