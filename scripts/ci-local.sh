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
#   bash scripts/ci-local.sh              # run the viewer steps in a worktree
#   bash scripts/ci-local.sh --public     # run them in a public-export replica
#   bash scripts/ci-local.sh --list       # print the parsed steps, run nothing
#   bash scripts/ci-local.sh --keep       # keep the worktree after the run
#
# `--public` first asks the publishing sync for a dry-run export (no push),
# then rebuilds that tree as a fresh single-commit repository so its commit
# hashes differ exactly as they do on GitHub, and runs the same viewer steps
# there. This is the mode that catches a history-dependent check.
#
# Environment:
#   CLIMB_CI_WORKFLOW      workflow file (default: .github/workflows/ci.yml)
#   CLIMB_CI_JOB           job name (default: viewer)
#   CLIMB_CI_WORKTREE_DIR  worktree path (default: ~/tmp/climb-ci-local)
#   CLIMB_PUBLIC_EXPORT    public replica path (default: ~/tmp/public-export)
#   CLIMB_SYNC_SCRIPT      publishing sync script (default: tools/sync-aiori.sh)
# ---------------------------------------------------------------------------
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKFLOW="${CLIMB_CI_WORKFLOW:-$REPO_ROOT/.github/workflows/ci.yml}"
JOB="${CLIMB_CI_JOB:-viewer}"
WORKTREE="${CLIMB_CI_WORKTREE_DIR:-$HOME/tmp/climb-ci-local}"
PUBLIC_EXPORT="${CLIMB_PUBLIC_EXPORT:-$HOME/tmp/public-export}"
LIST_ONLY=0
KEEP=0
PUBLIC=0

die() {
  printf 'ci-local: %s\n' "$*" >&2
  exit 1
}

for arg in "$@"; do
  case "$arg" in
    --list) LIST_ONLY=1 ;;
    --keep) KEEP=1 ;;
    --public) PUBLIC=1 ;;
    -h|--help)
      sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'
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
  if [[ "$KEEP" -eq 0 && "$LIST_ONLY" -eq 0 && "$PUBLIC" -eq 0 ]]; then
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

# --- reproduce the toolchain action's environment -------------------------
# The pinned setup action writes these into GITHUB_ENV before the first step
# runs, and one of them is `RUSTFLAGS=-D warnings`. A local run that omits them
# lints and tests a different build than CI does. Parse the workflow's explicit
# `rustflags` input, then apply the action's documented default for every other
# variable, keeping a value that is already set exactly as the action does.
python3 - "$WORKFLOW" "$JOB" "$TMP" <<'PY'
import re
import shlex
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

rustflags = None
action = "actions-rust-lang/setup-rust-toolchain@"
for index in range(start, end):
    line = lines[index]
    if action in line and "uses:" in line:
        step_start = index
        while step_start > start and not lines[step_start - 1].startswith("      - "):
            step_start -= 1
        step_end = index + 1
        while step_end < end and not lines[step_end].startswith("      - "):
            step_end += 1
        in_with = False
        for body in lines[step_start:step_end]:
            stripped = body.strip()
            if stripped == "with:":
                in_with = True
                continue
            if in_with and stripped == "":
                continue
            if in_with and not body.startswith("          "):
                in_with = False
                continue
            if in_with:
                key, _, value = stripped.partition(":")
                if key == "rustflags":
                    value = value.strip()
                    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                        value = value[1:-1]
                    rustflags = value
        break
if rustflags is None:
    # The action's own default. The workflow contract test requires the input
    # to be written out, so this branch is a warning, not the normal path.
    print(
        "ci-local: warning: no `rustflags` input on the setup-rust-toolchain step; "
        "using the action default `-D warnings`",
        file=sys.stderr,
    )
    rustflags = "-D warnings"

with open(f"{outdir}/setup-env.sh", "w", encoding="utf-8") as handle:
    handle.write(f"SETUP_RUSTFLAGS={shlex.quote(rustflags)}\n")
PY
# shellcheck source=/dev/null
source "$TMP/setup-env.sh"

# Environment contract of the pinned setup action. Keep this marker in sync
# with the action: the workflow tests fail when a variable is missing here.
if [[ ! -v RUSTFLAGS && -n "$SETUP_RUSTFLAGS" ]]; then
  export RUSTFLAGS="$SETUP_RUSTFLAGS"
fi
# Everything else (zone, locale, the action's remaining variables, git identity)
# comes from the single file the workflow also reads.
# shellcheck source=scripts/ci/env.sh
source "$REPO_ROOT/scripts/ci/env.sh"

printf 'ci-local: setup-rust-toolchain env: RUSTFLAGS=%s CARGO_INCREMENTAL=%s RUST_BACKTRACE=%s\n' \
  "${RUSTFLAGS:-<unset>}" "$CARGO_INCREMENTAL" "$RUST_BACKTRACE"

# --- public-export replica ------------------------------------------------
# The public repository is a rewritten copy: private-only paths are removed
# from every commit, so every hash differs. Rebuild that situation locally by
# asking the publishing sync for its dry-run tree and then committing that tree
# once in a fresh repository. A check that resolves a development commit fails
# here exactly as it fails on GitHub.
prepare_public_export() {
  local sync_script="${CLIMB_SYNC_SCRIPT:-}"
  if [[ -z "$sync_script" ]]; then
    local candidate
    for candidate in \
      "$REPO_ROOT/tools/sync-aiori.sh" \
      "$REPO_ROOT/../tools/sync-aiori.sh"; do
      if [[ -x "$candidate" ]]; then
        sync_script="$candidate"
        break
      fi
    done
  fi
  [[ -n "$sync_script" && -x "$sync_script" ]] || die \
    "cannot find the publishing sync script; set CLIMB_SYNC_SCRIPT to its path"
  command -v git-filter-repo >/dev/null || die \
    "git-filter-repo is required for --public (the publishing sync uses it)"

  local gitlinks
  gitlinks="$(git -C "$REPO_ROOT" ls-files -s third_party \
    | awk '$1 == "160000" {print $2, $4}')"
  [[ -n "$gitlinks" ]] || die \
    "the checkout records no submodule gitlinks under third_party/"

  printf 'ci-local: building the public-export replica from private HEAD %s\n' \
    "$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
  rm -rf "$PUBLIC_EXPORT"
  if ! DRY_RUN=1 PRIVATE_URL="$REPO_ROOT" KEEP_DRY_RUN_DIR="$PUBLIC_EXPORT" \
    bash "$sync_script"; then
    die "the publishing sync dry run failed; no public replica was built"
  fi

  # A fresh repository from the exported tree, so its hashes differ from the
  # development history in the same way GitHub's do.
  rm -rf "$PUBLIC_EXPORT/.git"
  git -C "$PUBLIC_EXPORT" init -q -b main

  # Overlay uncommitted work so this rehearsal checks what is about to be
  # published, not only the last commit. Private-only paths and build outputs
  # stay out, exactly as the sync removes them.
  if [[ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]]; then
    printf 'ci-local: working tree is dirty; overlaying its public files onto %s\n' \
      "$PUBLIC_EXPORT"
    command -v rsync >/dev/null || die "the working tree is dirty and rsync is not installed"
    # The publishing sync removes these planning files from every published
    # commit; the replica must not copy them back in.
    local -a private_excludes=(
      --exclude 'the project conventions'    # source-hygiene:allow
      --exclude 'the design notes' # source-hygiene:allow
      --exclude 'PROGRESS*.md' # source-hygiene:allow
    )
    rsync -a --delete \
      --exclude '.git' \
      --exclude 'target' \
      --exclude '.mtl-install' \
      --exclude 'dist' \
      --exclude 'site/node_modules' \
      --exclude 'site/dist' \
      --exclude 'crates/climb-web/dist' \
      --exclude 'scripts/__pycache__' \
      --exclude 'environment.json' \
      "${private_excludes[@]}" \
      --exclude '.github/dependabot.yml' \
      --exclude 'third_party' \
      "$REPO_ROOT/" "$PUBLIC_EXPORT/"
  fi

  git -C "$PUBLIC_EXPORT" add -A
  # `git add -A` cannot see a gitlink whose working tree is empty; restore the
  # submodule pins recorded by the export so the supply-chain check reads them.
  local sha path
  while read -r sha path; do
    git -C "$PUBLIC_EXPORT" update-index --add --cacheinfo "160000,$sha,$path"
  done <<< "$gitlinks"
  git -C "$PUBLIC_EXPORT" -c user.name='CLIMB CI' \
    -c user.email='ci@example.invalid' commit -qm 'public export replica'

  # GitHub gives the checkout an `origin`; give the replica one too, so the
  # workflow's fetch steps run against a real repository instead of failing on
  # a missing remote.
  local origin_bare="${PUBLIC_EXPORT}.origin.git"
  rm -rf "$origin_bare"
  git init --bare -q "$origin_bare"
  git -C "$PUBLIC_EXPORT" remote add origin "$origin_bare"
  git -C "$PUBLIC_EXPORT" push -q -u origin main

  local private_head public_head
  private_head="$(git -C "$REPO_ROOT" rev-parse HEAD)"
  public_head="$(git -C "$PUBLIC_EXPORT" rev-parse HEAD)"
  printf 'ci-local: private HEAD %s\n' "$private_head"
  printf 'ci-local: public replica HEAD %s (single commit, rewritten history)\n' "$public_head"
  [[ "$private_head" != "$public_head" ]] || die \
    "the public replica unexpectedly has the private commit hash"
  WORKTREE="$PUBLIC_EXPORT"
}

if [[ "$PUBLIC" -eq 1 ]]; then
  prepare_public_export
else
  # --- clean worktree -----------------------------------------------------
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
fi

printf 'ci-local: worktree %s (no target/, no .mtl-install/)\n' "$WORKTREE"

# --- execute every parsed step in order -----------------------------------
# The public replica mirrors CI, where the private role marker is never set.
step_env=()
if [[ "$PUBLIC" -eq 1 ]]; then
  step_env=(env -u CLIMB_REPO_ROLE)
fi
failed=0
while IFS=$'\t' read -r number name; do
  [[ -n "$number" ]] || continue
  printf '\n==> [%s] %s\n' "$number" "$name"
  if ! ( cd "$WORKTREE" && "${step_env[@]}" bash -euo pipefail "$TMP/$number.sh" ); then
    failed=1
    printf 'ci-local: step [%s] %s FAILED\n' "$number" "$name" >&2
    break
  fi
done < "$TMP/steps"

if [[ "$failed" -ne 0 ]]; then
  printf '\nci-local: FAILED — the viewer workflow would fail on this tree\n' >&2
  exit 1
fi

# --- full feature matrix ---------------------------------------------------
# The viewer job runs the check+clippy subset because a public runner has no
# native crypto toolchain. Locally, when the source checkout has the extracted
# prefix, point the matrix at it so the default and all-features cells really
# compile as well. Without a prefix the matrix names those cells as skipped.
matrix_env=()
if [[ -z "${CLIMB_MTL_PREFIX:-}" ]]; then
  prefix_candidate="$REPO_ROOT/.mtl-install"
  if [[ -f "$prefix_candidate/include/mtllib/mtllib.h" ]]; then
    matrix_env=(env "CLIMB_MTL_PREFIX=$prefix_candidate")
    printf 'ci-local: feature matrix uses the extracted native prefix %s\n' \
      "$prefix_candidate"
  fi
fi
printf '\n==> [matrix] full feature matrix (none, default, all)\n'
if ! ( cd "$WORKTREE" && "${matrix_env[@]}" cargo run -p xtask --no-default-features -- feature-matrix ); then
  printf '\nci-local: FAILED — the full feature matrix failed\n' >&2
  exit 1
fi

printf '\nci-local: all viewer steps passed in a clean worktree\n'
