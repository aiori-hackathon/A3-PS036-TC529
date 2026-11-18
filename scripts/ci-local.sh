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
#   bash scripts/ci-local.sh --native     # run the native job, runner-shaped
#   bash scripts/ci-local.sh --list       # print the parsed steps, run nothing
#   bash scripts/ci-local.sh --keep       # keep the worktree after the run
#
# `--public` first asks the publishing sync for a dry-run export (no push),
# then rebuilds that tree as a fresh single-commit repository so its commit
# hashes differ exactly as they do on GitHub, and runs the same viewer steps
# there. This is the mode that catches a history-dependent check.
#
# `--native` runs the `native` job. The host keeps the one shape a runner has
# that this machine lacks (Docker), and reproduces the rest: the workflow's
# `run:` steps are parsed from the same file, a step whose script calls `docker`
# runs on the host, every other step runs in an `ubuntu:24.04` container pinned
# by digest with the runner image's package baseline, the pinned Rust toolchain,
# the workflow's environment, `TZ=UTC`, `LANG=C.UTF-8`, no developer git
# identity, and `--network=host` (this machine cannot create bridge networks).
# The workspace is a fresh `git clone --no-local --recurse-submodules` of the
# public export under `~/tmp`, and `.mtl-install` is extracted from the pinned
# image into that clone rather than borrowed from the developer checkout.
#
# Environment:
#   CLIMB_CI_WORKFLOW      workflow file (default: .github/workflows/ci.yml)
#   CLIMB_CI_JOB           job name (default: viewer, or native with --native)
#   CLIMB_CI_WORKTREE_DIR  worktree path (default: ~/tmp/climb-ci-local)
#   CLIMB_PUBLIC_EXPORT    public replica path (default: ~/tmp/public-export)
#   CLIMB_SYNC_SCRIPT      publishing sync script (default: tools/sync-aiori.sh)
#   CLIMB_CI_NATIVE_CLONE  native clone path (default: ~/tmp/climb-ci-native)
#   CLIMB_CI_NATIVE_IMAGE  rehearsal base image (default: pinned ubuntu:24.04)
# ---------------------------------------------------------------------------
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKFLOW="${CLIMB_CI_WORKFLOW:-$REPO_ROOT/.github/workflows/ci.yml}"
JOB="${CLIMB_CI_JOB:-viewer}"
WORKTREE="${CLIMB_CI_WORKTREE_DIR:-$HOME/tmp/climb-ci-local}"
PUBLIC_EXPORT="${CLIMB_PUBLIC_EXPORT:-$HOME/tmp/public-export}"
NATIVE_CLONE="${CLIMB_CI_NATIVE_CLONE:-$HOME/tmp/climb-ci-native}"
# The runner's own image, pinned the same way the workflow pins its actions.
# `ubuntu:24.04` resolved to this index digest on 2026-11-17; the amd64
# manifest inside it is sha256:77eb101c607d6e3d4f5ad0c8a02c2136c689b98aba1f359e6502ba517e5c57ec.
NATIVE_IMAGE="${CLIMB_CI_NATIVE_IMAGE:-docker.io/library/ubuntu:24.04@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8}"
NATIVE_CONTAINER="${CLIMB_CI_NATIVE_CONTAINER:-climb-ci-native-$$}"
NATIVE_STARTED=0
# The packages the GitHub-hosted `ubuntu-24.04` image already carries, taken
# from actions/runner-images' Ubuntu 24.04 software list
# (https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md,
# Image Version 20261111.370.1, fetched 2026-11-17): GCC/G++ 13, Clang 16/17/18,
# CMake 3.31.6, Ninja 1.13.2, Perl 5.38.2, Git 2.56.1. `pkg-config` is in the
# list because the failing runner log shows it running and failing to find
# fontconfig, which means it was installed. The packages a build needs and this
# list omits are what the rehearsal must reproduce as the failure.
RUNNER_BASELINE=(
  build-essential
  ca-certificates
  clang
  cmake
  curl
  git
  libclang-dev
  ninja-build
  perl
  pkg-config
  python3
  sudo
  wget
)
LIST_ONLY=0
KEEP=0
PUBLIC=0
NATIVE=0

die() {
  printf 'ci-local: %s\n' "$*" >&2
  exit 1
}

# The publishing sync rewrites every published blob with literal rules before
# the mirror is pushed, and it also removes the private planning files. An
# overlay copies files straight from the working tree, so without this pass a
# dirty tree would be tested with text the published tree never carries, and a
# check that depends on the rewrite would pass locally and fail on the mirror.
# The rules are read from the file next to the sync script; only the literal
# `from==>to` form is applied, and any other form is refused rather than
# silently skipped.
publication_rules_file() {
  local rules="${CLIMB_REDACTION_FILE:-}"
  if [[ -z "$rules" && -n "${SYNC_SCRIPT:-}" ]]; then
    rules="$(dirname "$SYNC_SCRIPT")/aiori-redactions.txt"
  fi
  [[ -n "$rules" ]] || die \
    "cannot locate the publication rewrite rules; set CLIMB_REDACTION_FILE"
  [[ -f "$rules" ]] || die "publication rewrite rules not found: $rules"
  printf '%s\n' "$rules"
}

apply_publication_rewrite() {
  local tree="$1" rules="$2"
  python3 - "$tree" "$rules" <<'PY'
import os
import sys

tree, rules_path = sys.argv[1], sys.argv[2]
rules = []
with open(rules_path, encoding="utf-8") as handle:
    for number, line in enumerate(handle, 1):
        line = line.rstrip("\n")
        if not line or line.lstrip().startswith("#"):
            continue
        if "==>" not in line:
            sys.exit(f"ci-local: {rules_path}:{number}: a rewrite rule without `==>`")
        source, _, replacement = line.partition("==>")
        if source.startswith(("regex:", "glob:")):
            sys.exit(
                f"ci-local: {rules_path}:{number}: this replica applies literal rules only, "
                f"not `{source.split(':', 1)[0]}:`"
            )
        rules.append((source.encode(), replacement.encode()))

rewritten = 0
for root, dirs, files in os.walk(tree):
    dirs[:] = [name for name in dirs if name != ".git"]
    for name in files:
        path = os.path.join(root, name)
        with open(path, "rb") as handle:
            data = handle.read()
        updated = data
        for source, replacement in rules:
            if source in updated:
                updated = updated.replace(source, replacement)
        if updated != data:
            with open(path, "wb") as handle:
                handle.write(updated)
            rewritten += 1
print(
    f"ci-local: applied {len(rules)} publication rewrite rule(s) to {rewritten} overlaid file(s)"
)
PY
}

for arg in "$@"; do
  case "$arg" in
    --list) LIST_ONLY=1 ;;
    --keep) KEEP=1 ;;
    --public) PUBLIC=1 ;;
    --native) NATIVE=1 ;;
    -h|--help)
      sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) die "unknown argument: $arg" ;;
  esac
done

# `--native` selects the native job unless the caller named one explicitly.
if [[ "$NATIVE" -eq 1 && -z "${CLIMB_CI_JOB:-}" ]]; then
  JOB=native
fi

# The rewrite pass is also reachable on its own, so the fidelity test can run it
# against a fixture tree without a Docker toolchain or a public export.
if [[ -n "${CLIMB_REDACT_TREE:-}" ]]; then
  apply_publication_rewrite "$CLIMB_REDACT_TREE" "$(publication_rules_file)"
  exit 0
fi

[[ -f "$WORKFLOW" ]] || die "workflow not found: $WORKFLOW"
command -v python3 >/dev/null || die "python3 is required to parse $WORKFLOW"

TMP="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP"
  if [[ "$NATIVE_STARTED" -eq 1 ]]; then
    docker rm -f "$NATIVE_CONTAINER" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP" -eq 0 && "$LIST_ONLY" -eq 0 && ! ( "$PUBLIC" -eq 1 && "$NATIVE" -eq 0 ) ]]; then
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
skipped = []
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
        skipped.append(name)
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
for name in skipped:
    print(f"  GitHub action, not run: {name}")
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
  SYNC_SCRIPT="$sync_script"
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
    # The overlay copied the working tree verbatim; the published blobs carry
    # the rewrite, so apply the same rules before this tree is committed.
    apply_publication_rewrite "$PUBLIC_EXPORT" "$(publication_rules_file)"
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

# --- native rehearsal -----------------------------------------------------
# A fresh clone of the public export with its pinned submodules, so nothing in
# this checkout (a stale `.mtl-install`, an ignored file, a local Cargo target)
# can make the rehearsal pass or fail for a reason the runner does not have.
native_clone_from_export() {
  command -v docker >/dev/null || die "docker is required for --native"
  case "$NATIVE_CLONE" in
    "$REPO_ROOT"/*) die "the native clone must live outside the repository: $NATIVE_CLONE" ;;
  esac
  rm -rf "$NATIVE_CLONE"
  mkdir -p "$(dirname "$NATIVE_CLONE")"
  printf 'ci-local: cloning the public export into %s (--no-local, --recurse-submodules)\n' \
    "$NATIVE_CLONE"
  git clone --no-local --recurse-submodules "$PUBLIC_EXPORT" "$NATIVE_CLONE" >/dev/null
  WORKTREE="$NATIVE_CLONE"
}

# Start the container and give it the runner image's package baseline and the
# pinned toolchain. The steps run as a user whose uid matches the caller's, so
# the clone is owned by the user that reads it, exactly as a runner's checkout
# is, and `git` reports no ownership mismatch. That user gets passwordless
# `sudo`, which the runner image provides and the base image does not.
# `--network=host` is this machine's limitation, not the runner's; see
# docs/CI-NATIVE-REHEARSAL.md.
start_native_container() {
  local uid gid
  uid="$(id -u)"
  gid="$(id -g)"
  docker rm -f "$NATIVE_CONTAINER" >/dev/null 2>&1 || true
  docker run -d --name "$NATIVE_CONTAINER" --network=host --hostname climb-ci \
    -v "$NATIVE_CLONE":/workspace -w /workspace \
    "$NATIVE_IMAGE" sleep infinity >/dev/null
  NATIVE_STARTED=1
  printf 'ci-local: container %s from %s\n' "$NATIVE_CONTAINER" "$NATIVE_IMAGE"

  printf 'ci-local: installing the runner image package baseline (%s)\n' \
    "${RUNNER_BASELINE[*]}"
  docker exec --user 0 -e DEBIAN_FRONTEND=noninteractive "$NATIVE_CONTAINER" \
    bash -euo pipefail -c \
    'apt-get update -qq && apt-get install -y --no-install-recommends "$@"' \
    ci-baseline "${RUNNER_BASELINE[@]}" >/dev/null
  docker exec "$NATIVE_CONTAINER" bash -c \
    'for package in "$@"; do dpkg-query -W "$package"; done' \
    ci-baseline "${RUNNER_BASELINE[@]}"

  # The runner image ships a passwordless `sudo`; the base image needs the
  # package before the entry can be written.
  docker exec --user 0 "$NATIVE_CONTAINER" bash -euo pipefail -c '
    uid="$1"
    name="$(getent passwd "$uid" | cut -d: -f1)"
    if [ -z "$name" ]; then
      name=runner
      useradd --uid "$uid" --create-home --shell /bin/bash "$name"
    fi
    printf "%s ALL=(ALL) NOPASSWD:ALL\n" "$name" > /etc/sudoers.d/ci-runner
    chmod 0440 /etc/sudoers.d/ci-runner
  ' ci-user "$uid"
  NATIVE_HOME="$(docker exec --user 0 "$NATIVE_CONTAINER" getent passwd "$uid" \
    | cut -d: -f6)"
  [[ -n "$NATIVE_HOME" ]] || die "the container has no home directory for uid $uid"

  # The pinned setup action installs rustup and then the toolchain named by
  # rust-toolchain.toml, with that file's components, as the runner user.
  docker exec --user "$uid:$gid" -e "HOME=$NATIVE_HOME" "$NATIVE_CONTAINER" \
    bash -euo pipefail -c \
    'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain none' >/dev/null
  local channel
  channel="$(sed -n 's/^channel[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' \
    "$NATIVE_CLONE/rust-toolchain.toml")"
  [[ -n "$channel" ]] || die "rust-toolchain.toml carries no channel"
  docker exec --user "$uid:$gid" -e "HOME=$NATIVE_HOME" "$NATIVE_CONTAINER" \
    bash -euo pipefail -c \
    ". "$NATIVE_HOME/.cargo/env" && rustup toolchain install $channel --profile minimal \
      --component rustfmt,clippy,rust-analyzer && rustup default $channel" >/dev/null
  docker exec --user "$uid:$gid" -e "HOME=$NATIVE_HOME" "$NATIVE_CONTAINER" \
    bash -c ". \"$NATIVE_HOME/.cargo/env\" && rustc --version && cargo --version"
}

# A step whose script invokes Docker runs on the host, because this machine has
# the one dependency a container cannot provide to itself.
native_step_uses_docker() {
  local number="$1" target
  target="$(sed -n 's|.*\(\./scripts/ci/[A-Za-z0-9_-]*\.sh\).*|\1|p' "$TMP/$number.sh" \
    | head -n 1)"
  [[ -n "$target" ]] || return 1
  grep -qE '(^|[[:space:]])docker[[:space:]]' "$NATIVE_CLONE/$target"
}

if [[ "$PUBLIC" -eq 1 || "$NATIVE" -eq 1 ]]; then
  prepare_public_export
fi

if [[ "$NATIVE" -eq 1 ]]; then
  native_clone_from_export
  start_native_container
elif [[ "$PUBLIC" -eq 0 ]]; then
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
if [[ "$NATIVE" -eq 1 ]]; then
  printf 'ci-local: workspace %s (fresh clone of the public export, its own target/)\n' \
    "$WORKTREE"
fi

# --- execute every parsed step in order -----------------------------------
# The public replica mirrors CI, where the private role marker is never set.
step_env=()
if [[ "$PUBLIC" -eq 1 ]]; then
  step_env=(env -u CLIMB_REPO_ROLE)
fi
failed=0
# The container gets the environment the workflow publishes before its first
# step. The host passes no developer git identity; the CI scripts source
# `scripts/ci/env.sh` in both places, which is where CI's own identity comes
# from, and the container has no `~/.gitconfig`.
native_env_args=()
if [[ "$NATIVE" -eq 1 ]]; then
  while IFS= read -r env_line; do
    native_env_args+=(-e "$env_line")
  done < <(bash "$REPO_ROOT/scripts/ci/env.sh")
  native_env_args+=(-e "CARGO_TARGET_DIR=/workspace/target")
  native_env_args+=(-e "HOME=$NATIVE_HOME")
  native_env_args+=(-e "PATH=$NATIVE_HOME/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")
  # A host step that builds an image needs the host network on this machine,
  # which cannot create Docker bridge networks. The runner uses the default
  # bridge and leaves the variable unset.
  export CLIMB_DOCKER_NETWORK="${CLIMB_DOCKER_NETWORK:-host}"
fi
durations=()
while IFS=$'\t' read -r number name; do
  [[ -n "$number" ]] || continue
  printf '\n==> [%s] %s\n' "$number" "$name"
  started=$SECONDS
  status=0
  where=""
  if [[ "$NATIVE" -eq 1 ]]; then
    log="$TMP/native-$number.log"
    if native_step_uses_docker "$number"; then
      where="host"
      printf 'ci-local: host step (its script invokes docker)\n'
      ( cd "$NATIVE_CLONE" && "${step_env[@]}" bash -euo pipefail "$TMP/$number.sh" ) \
        2>&1 | tee "$log" || status=$?
    else
      where="container"
      printf 'ci-local: container step in %s\n' "$NATIVE_CONTAINER"
      docker exec -i --user "$(id -u):$(id -g)" "${native_env_args[@]}" \
        -w /workspace "$NATIVE_CONTAINER" bash -s < "$TMP/$number.sh" 2>&1 | tee "$log" \
        || status=$?
    fi
  else
    ( cd "$WORKTREE" && "${step_env[@]}" bash -euo pipefail "$TMP/$number.sh" ) || status=$?
  fi
  elapsed=$((SECONDS - started))
  durations+=("$number"$'\t'"$name"$'\t'"$elapsed")
  printf 'ci-local: [%s] took %ss\n' "$number" "$elapsed"
  # Disk headroom on a runner is small; the two steps that fill it are measured.
  if [[ "$NATIVE" -eq 1 && ( "$name" == "Build workspace" || "$name" == "Test workspace" ) ]]; then
    printf 'ci-local: disk after %s:\n' "$name"
    docker exec "$NATIVE_CONTAINER" df -h /workspace
    docker exec "$NATIVE_CONTAINER" du -sh /workspace/target /workspace/.mtl-install
  fi
  if [[ "$status" -ne 0 ]]; then
    failed=1
    if [[ "$NATIVE" -eq 1 ]]; then
      printf 'ci-local: step [%s] %s FAILED (%s)\n' "$number" "$name" "$where" >&2
      printf 'ci-local: last 60 lines:\n' >&2
      tail -n 60 "$log" >&2
    else
      printf 'ci-local: step [%s] %s FAILED\n' "$number" "$name" >&2
    fi
    break
  fi
done < "$TMP/steps"

# The per-step durations are the budget evidence: a warm native job is the sum
# of these plus the actions the runner performs.
if [[ "${#durations[@]}" -gt 0 ]]; then
  printf '\nci-local: step durations\n'
  for row in "${durations[@]}"; do
    IFS=$'\t' read -r number name seconds <<< "$row"
    printf '  [%s] %-50s %4ss\n' "$number" "$name" "$seconds"
  done
fi

if [[ "$failed" -ne 0 ]]; then
  if [[ "$NATIVE" -eq 1 ]]; then
    printf '\nci-local: FAILED — the native workflow would stop at step [%s]\n' "$number" >&2
  else
    printf '\nci-local: FAILED — the viewer workflow would fail on this tree\n' >&2
  fi
  exit 1
fi

if [[ "$NATIVE" -eq 1 ]]; then
  printf '\nci-local: every native step passed in the runner-shaped container\n'
  exit 0
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
