#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# ci-local.sh — run the viewer job's `run:` steps in a clean worktree.
#
# The steps are parsed out of .github/workflows/ci.yml at invocation time, so
# this script cannot silently drift from the workflow: adding a step, renaming
# one, or changing its command changes what runs here too. Only `run:` steps
# run; `uses:` steps (checkout, toolchain, cache) are performed by the runner
# and have no local equivalent.
#
# The worktree starts without `target/`, `.mtl-install/` or any other build
# output, which is what makes the run a real check that no test accidentally
# depends on a stale generated artifact.
#
# Usage:
#   bash scripts/ci-local.sh              # run the viewer steps
#   bash scripts/ci-local.sh --list       # print the parsed steps, run nothing
#   bash scripts/ci-local.sh --keep       # keep the worktree after the run
#
# Environment:
#   CLIMB_CI_WORKFLOW      workflow file (default: .github/workflows/ci.yml)
#   CLIMB_CI_JOB           job name (default: viewer)
#   CLIMB_CI_WORKTREE_DIR  worktree path (default: ~/tmp/climb-ci-local)
# ---------------------------------------------------------------------------
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKFLOW="${CLIMB_CI_WORKFLOW:-$REPO_ROOT/.github/workflows/ci.yml}"
JOB="${CLIMB_CI_JOB:-viewer}"
WORKTREE="${CLIMB_CI_WORKTREE_DIR:-$HOME/tmp/climb-ci-local}"
LIST_ONLY=0
KEEP=0

die() {
  printf 'ci-local: %s\n' "$*" >&2
  exit 1
}

for arg in "$@"; do
  case "$arg" in
    --list) LIST_ONLY=1 ;;
    --keep) KEEP=1 ;;
    -h|--help)
      sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) die "unknown argument: $arg" ;;
  esac
done

[[ -f "$WORKFLOW" ]] || die "workflow not found: $WORKFLOW"
command -v python3 >/dev/null || die "python3 is required to parse $WORKFLOW"

TMP="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP"
  if [[ "$KEEP" -eq 0 && "$LIST_ONLY" -eq 0 ]]; then
    git -C "$REPO_ROOT" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
    rm -rf "$WORKTREE"
  fi
}
trap cleanup EXIT

# --- parse the job's run steps out of the workflow -------------------------
python3 - "$WORKFLOW" "$JOB" "$TMP" <<'PY'
import os
import re
import sys

workflow, job, outdir = sys.argv[1], sys.argv[2], sys.argv[3]
with open(workflow, encoding="utf-8") as handle:
    lines = handle.read().splitlines()

job_re = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
start = None
for index, line in enumerate(lines):
    match = job_re.match(line)
    if match and match.group(1) == job:
        start = index + 1
        break
if start is None:
    sys.exit(f"ci-local: job `{job}` not found in {workflow}")

end = len(lines)
for index in range(start, len(lines)):
    if job_re.match(lines[index]):
        end = index
        break

blocks = []
current = None
for line in lines[start:end]:
    if line.startswith("      - "):
        if current is not None:
            blocks.append(current)
        current = [line]
    elif current is not None:
        current.append(line)
if current is not None:
    blocks.append(current)

os.makedirs(outdir, exist_ok=True)
names = []
index = 0
for block in blocks:
    text = "\n".join(block)
    name_match = re.search(r"^      - name:\s*(.+?)\s*$", text, re.MULTILINE)
    if name_match is None:
        continue
    name = name_match.group(1)
    run_match = re.search(r"^        run:\s*(.*)$", text, re.MULTILINE)
    if run_match is None:
        # `uses:` step: performed by the runner, nothing to run locally.
        continue
    first = run_match.group(1)
    if first not in ("", "|", ">"):
        commands = first
    else:
        run_line = text[: run_match.start()].count("\n")
        collected = []
        for extra in block[run_line + 1 :]:
            if extra.strip() == "":
                collected.append("")
                continue
            indent = len(extra) - len(extra.lstrip(" "))
            if indent < 10:
                break
            collected.append(extra[10:])
        commands = "\n".join(collected).rstrip() + "\n"
    index += 1
    path = os.path.join(outdir, f"{index:02d}.sh")
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("set -euo pipefail\n" + commands)
    names.append(f"{index:02d}\t{name}")

with open(os.path.join(outdir, "steps"), "w", encoding="utf-8") as handle:
    handle.write("\n".join(names) + "\n")
print(f"ci-local: parsed {len(names)} run step(s) for job `{job}`")
for row in names:
    print("  " + row.split("\t", 1)[1])
PY

if [[ "$LIST_ONLY" -eq 1 ]]; then
  exit 0
fi

# --- clean worktree -------------------------------------------------------
case "$WORKTREE" in
  "$REPO_ROOT"/*) die "the worktree must live outside the repository: $WORKTREE" ;;
esac
if [[ -e "$WORKTREE" ]]; then
  git -C "$REPO_ROOT" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
  rm -rf "$WORKTREE"
fi
mkdir -p "$(dirname "$WORKTREE")"
git -C "$REPO_ROOT" worktree add --detach "$WORKTREE" HEAD >/dev/null

# A committed worktree cannot contain uncommitted local edits. When the tree
# is dirty, overlay the working copy (excluding build outputs and VCS data) so
# this local rehearsal checks what is about to be committed, not only HEAD.
if [[ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]]; then
  printf 'ci-local: working tree is dirty; overlaying it onto %s (HEAD %s)\n' \
    "$WORKTREE" "$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
  if command -v rsync >/dev/null; then
    rsync -a \
      --exclude '.git' \
      --exclude 'target' \
      --exclude '.mtl-install' \
      --exclude 'dist' \
      "$REPO_ROOT/" "$WORKTREE/"
  else
    die "the working tree is dirty and rsync is not installed"
  fi
fi

printf 'ci-local: worktree %s (no target/, no .mtl-install/)\n' "$WORKTREE"

# --- execute every parsed step in order -----------------------------------
failed=0
while IFS=$'\t' read -r number name; do
  [[ -n "$number" ]] || continue
  printf '\n==> [%s] %s\n' "$number" "$name"
  if ! ( cd "$WORKTREE" && bash -euo pipefail "$TMP/$number.sh" ); then
    failed=1
    printf 'ci-local: step [%s] %s FAILED\n' "$number" "$name" >&2
    break
  fi
done < "$TMP/steps"

if [[ "$failed" -ne 0 ]]; then
  printf '\nci-local: FAILED — the viewer workflow would fail on this tree\n' >&2
  exit 1
fi
printf '\nci-local: all viewer steps passed in a clean worktree\n'
