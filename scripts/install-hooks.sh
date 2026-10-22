#!/bin/bash
# Opt-in developer setup: install a pre-push hook that checks the commit graph.
#
# This never runs on its own. It installs one hook, .git/hooks/pre-push, which
# runs `cargo xtask commit-graph --check` before a push. That check fails only
# when the committed graph is inconsistent (it does not reproduce from its own
# as-of commit, or a card disagrees with the document) and merely warns when the
# graph is simply behind HEAD, so a stale graph never blocks a push. The hook
# does not amend, rewrite or create commits, and it does not push anything.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

hooks_dir="$(git rev-parse --git-path hooks)"
mkdir -p "$hooks_dir"
hook_path="$hooks_dir/pre-push"

cat >"$hook_path" <<'HOOK'
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
echo "pre-push: fix it with scripts/refresh-commit-graph.sh, then push again." >&2
echo "pre-push: override with 'git push --no-verify' if you are sure." >&2
exit "$status"
HOOK

chmod +x "$hook_path"

echo "Installed the pre-push hook at:"
echo "  $hook_path"
echo ""
echo "What it does on every push:"
echo "  1. runs: cargo xtask commit-graph --check"
echo "  2. warns (does not fail) when the committed graph is behind HEAD"
echo "  3. fails only when the committed graph is inconsistent"
echo ""
echo "What it never does: amend, rewrite or create commits, or push anything."
echo "Remove it with: rm '$hook_path'"
