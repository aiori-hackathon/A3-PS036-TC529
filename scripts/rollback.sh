#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
#
# Point the live site at an earlier release.
#
#   scripts/rollback.sh --list          releases on the host, newest first
#   scripts/rollback.sh <release-id>    verify that release, publish it, reload
#   scripts/rollback.sh previous        the release before the current one
#
# Run on the host, as root, after a bad deploy. It shares the receiver's
# verification and publication functions, so a rollback target is held to the
# same evidence a new release is: a manifest hash that matches, every listed
# file present with the recorded digest, and a module that starts with the
# WebAssembly magic bytes. A target that fails verification is refused and
# `current` is left alone.
#
# Running the release receiver instead of this file is what publishes new
# releases; this file only moves `current` backwards.
#
# Exit status: 0 published, 2 refused, 3 verification failed, 5 served but the
# health probe did not report the release.

set -euo pipefail

# shellcheck source-path=SCRIPTDIR
# shellcheck source=deploy-receive.sh
CLIMB_ALLOW_UNPRIVILEGED="${CLIMB_ALLOW_UNPRIVILEGED:-0}"
export CLIMB_ALLOW_UNPRIVILEGED
# The receiver runs its main function only when it is executed, not sourced,
# so every helper below comes from that one implementation.
source "$(dirname "$0")/deploy-receive.sh"

usage() {
  sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'
}

release_names() {
  find "$climb_root/releases" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null |
    grep -E "^${release_pattern}$" | LC_ALL=C sort -r || true
}

list_releases() {
  local current name marker
  current="$(readlink "$climb_root/current" 2>/dev/null || true)"
  if [ ! -d "$climb_root/releases" ]; then
    note "no releases under $climb_root"
    return 0
  fi
  printf '  %s\n' "release"
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    marker=" "
    if [ "$climb_root/releases/$name" = "$current" ]; then
      marker="*"
    fi
    printf '%s %s\n' "$marker" "$name"
  done < <(release_names)
  printf '\n* is the release current points at\n'
}

run_rollback() {
  local target="${1:-}" work current target_dir

  case "$target" in
    --list | -l) list_releases; exit 0 ;;
    -h | --help | "") usage; exit 0 ;;
  esac
  shift || true
  [ "$#" -eq 0 ] || die 2 "unexpected extra arguments: $*"

  if [ "${CLIMB_ALLOW_UNPRIVILEGED:-0}" != 1 ] && [ "$(id -u)" -ne 0 ]; then
    die 2 "this runs on the host as root"
  fi
  for tool in jq zstd sha256sum od curl; do
    command -v "$tool" >/dev/null 2>&1 || die 2 "$tool is required but not on PATH"
  done

  if [ "$target" = "previous" ]; then
    current="$(basename "$(readlink "$climb_root/current" 2>/dev/null || printf 'none')")"
    target="$(release_names | awk -v current="$current" 'seen { print; exit } $0 == current { seen = 1 }')"
    [ -n "$target" ] || die 2 "there is no release before ${current}"
  fi

  require_release_id "$target"
  target_dir="$climb_root/releases/$target"
  [ -d "$target_dir" ] || die 2 "release $target is not on this host"

  work="$(mktemp -d "${TMPDIR:-/tmp}/climb-rollback.XXXXXX")"
  trap 'rm -rf -- "$work"' EXIT

  say "verifying $target"
  verify_release_tree "$target_dir" "$target" "$work"

  swap_current "$target_dir" || die 5 "the current symlink could not be replaced"
  say "current -> $(readlink "$climb_root/current")"
  reload_service || die 5 "the reload failed after the swap; inspect $climb_service"
  if ! health_check "$target"; then
    note "the site does not report $target after the swap; inspect $climb_service"
    exit 5
  fi
  journal "rollback to $target"
  say "rolled back to $target"
}

run_rollback "$@"
