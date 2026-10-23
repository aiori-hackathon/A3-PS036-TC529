#!/bin/bash
# Opt-in developer setup: install git hooks that check, or publish, the commit
# graph. Nothing is installed unless this script is run.
#
#   scripts/install-hooks.sh               install the pre-push consistency check
#   scripts/install-hooks.sh --autopublish install that check and a post-commit
#                                          hook that runs scripts/publish.sh
#   scripts/install-hooks.sh --remove      remove the hooks this script installed
#
# The pre-push hook runs `cargo xtask commit-graph --check`. It fails only when
# the committed graph is inconsistent (it does not reproduce from the public
# history, or a card disagrees with the document) and merely warns when the
# graph is simply behind HEAD. It never fetches: a checkout that lacks the
# public as-of commit reproduces from the local bare cache, and when neither has
# it the history checks are skipped with a printed reason.
#
# The post-commit hook runs scripts/publish.sh after every commit. That is a
# real side effect, not a preview: each publish with new work adds one small
# graph-refresh commit to the public history. It is off by default because most
# work is better published in batches. Its failure cannot undo the commit; it
# writes a warning and a log instead.
#
# Neither hook amends, rewrites or creates commits on its own, and the
# pre-push hook never pushes anything.
set -euo pipefail

usage() {
  sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
}

mode="check"
case "${1:-}" in
  "")
    mode="check"
    ;;
  --autopublish)
    mode="autopublish"
    ;;
  --remove)
    mode="remove"
    ;;
  --help | -h)
    usage
    exit 0
    ;;
  *)
    echo "error: unknown argument: $1" >&2
    echo "usage: $0 [--autopublish | --remove | --help]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"

hooks_dir="$(git rev-parse --git-path hooks)"
mkdir -p "$hooks_dir"
pre_push="$hooks_dir/pre-push"
post_commit="$hooks_dir/post-commit"
marker="# Installed by scripts/install-hooks.sh."

remove_if_ours() {
  local path="$1"
  if [ ! -e "$path" ]; then
    echo "  absent: $path"
    return
  fi
  if grep -qF "$marker" "$path"; then
    rm -f "$path"
    echo "  removed: $path"
  else
    echo "  left untouched (not installed by this script): $path"
  fi
}

if [ "$mode" = "remove" ]; then
  echo "Removing the hooks installed by this script:"
  remove_if_ours "$pre_push"
  remove_if_ours "$post_commit"
  echo ""
  echo "Done. Commit-graph hooks are no longer installed."
  exit 0
fi

cat >"$pre_push" <<'HOOK'
#!/bin/bash
# Installed by scripts/install-hooks.sh.
#
# Warns when the committed commit-graph artifacts are behind HEAD and fails only
# when they are inconsistent. Never rewrites history and never blocks a push for
# staleness.
set -uo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  echo "pre-push: cargo not found on PATH; skipping the commit-graph check" >&2
  exit 0
fi

if cargo xtask commit-graph --check; then
  exit 0
else
  status=$?
fi

echo "pre-push: the commit-graph check failed (exit $status)." >&2
echo "pre-push: this is a consistency failure, not mere staleness." >&2
echo "pre-push: fix it with scripts/publish.sh, then push again." >&2
echo "pre-push: override with 'git push --no-verify' if you are sure." >&2
exit "$status"
HOOK
chmod +x "$pre_push"
echo "Installed the pre-push consistency check at:"
echo "  $pre_push"

if [ "$mode" = "autopublish" ]; then
  cat >"$post_commit" <<'HOOK'
#!/bin/bash
# Installed by scripts/install-hooks.sh.
#
# Publishes after every commit. A guard stops the refresh commit created by
# scripts/publish.sh from starting another publish. A failure cannot undo the
# commit, so it prints a warning and appends the full output to a log.
set -uo pipefail

if [ "${CLIMB_AUTOPUBLISH_RUNNING:-}" = "1" ]; then
  exit 0
fi

repo_root="$(git rev-parse --show-toplevel)" || exit 0
cd "$repo_root" || exit 0
mkdir -p target

log_file="target/autopublish.log"
{
  printf '\n== %s post-commit autopublish (%s)\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(git rev-parse --short HEAD)"
} >>"$log_file"

scripts/publish.sh >>"$log_file" 2>&1
status=$?
if [ "$status" -ne 0 ]; then
  echo "post-commit: autopublish failed (exit $status); the commit exists." >&2
  echo "post-commit: full output is in $log_file" >&2
  echo "post-commit: publish later with scripts/publish.sh" >&2
  echo "post-commit: remove this hook with scripts/install-hooks.sh --remove" >&2
fi
exit 0
HOOK
  chmod +x "$post_commit"
  echo "Installed the post-commit autopublish hook at:"
  echo "  $post_commit"
fi

echo ""
echo "The pre-push hook, on every push:"
echo "  1. runs: cargo xtask commit-graph --check"
echo "  2. reproduces the card from this checkout or the public-history cache"
echo "  3. warns (does not fail) when the committed graph is behind HEAD"
echo "  4. fails only when the committed graph is inconsistent"
echo ""
if [ "$mode" = "autopublish" ]; then
  echo "The post-commit hook, on every commit:"
  echo "  1. runs: scripts/publish.sh"
  echo "  2. pushes, syncs to the public repository and refreshes the card"
  echo "  3. adds one graph-refresh commit whenever there is new work"
  echo "  4. warns in the terminal and logs to target/autopublish.log on failure"
  echo ""
  echo "Autopublish publishes after every single commit. That is slower and"
  echo "noisier than publishing in batches; if you usually push several commits"
  echo "at once, keep this hook off and run scripts/publish.sh yourself."
  echo ""
fi
echo "It never fetches in the pre-push hook and never fails offline: a missing"
echo "as-of commit skips the history checks with a printed reason."
echo "Remove both hooks with: $0 --remove"
