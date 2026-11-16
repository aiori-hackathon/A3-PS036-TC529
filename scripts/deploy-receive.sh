#!/usr/bin/env bash
#
# Receive one packaged site release, verify it, publish it, and roll back when
# the host does not answer with the new release.
#
# This is the only command the deploy key may run: its authorized_keys line
# carries a forced command pointing at this script, so the key cannot open a
# shell, forward a port or run anything else. The archive arrives on stdin and
# the request arrives in SSH_ORIGINAL_COMMAND:
#
#   ssh -l deploy <host> 'receive <release-id> <archive-sha256>' \
#     < climb-site-<release-id>.tar.zst
#
# Publishing is one rename of the `current` symlink, which is why a reader
# never sees a half-written tree: the archive is unpacked and verified in
# `$CLIMB_ROOT/.incoming`, moved into `$CLIMB_ROOT/releases/<release-id>`, and
# only then named by `current`.
#
# Verification chain, cheapest first:
#   1. the stream's SHA-256 equals the digest the sender pinned;
#   2. the archive contains only `manifest.json`, `manifest.sha256` and `site/`,
#      with no absolute path and no `..` component;
#   3. `manifest.sha256` matches `manifest.json`;
#   4. every file the manifest lists is present with the recorded length and
#      digest, and nothing else is present;
#   5. the WebAssembly module starts with the four magic bytes.
#
# After the swap the service is reloaded and `GET /release.json` has to report
# the new release id; when it does not, the previous release is restored and
# the failed tree is kept as `<release-id>.failed` for inspection.
#
# The newest `$CLIMB_KEEP_RELEASES` releases are kept (five by default).
#
# Exit status: 0 published, 2 refused, 3 integrity, 4 unpack, 5 rolled back.
#
# Environment (all optional):
#   CLIMB_ROOT          release root, default /srv/climb
#   CLIMB_ETC           host configuration directory, default /etc/climb
#   CLIMB_SERVICE       service reloaded after the swap, default caddy
#   CLIMB_HEALTH_URL    base URL probed after reload; default is derived from
#                       CLIMB_SITE_ADDRESS in $CLIMB_ETC/caddy.env
#   CLIMB_HEALTH_ATTEMPTS  probe attempts, default 20, half a second apart
#   CLIMB_KEEP_RELEASES releases kept, default 5
#   CLIMB_MAX_BYTES     largest accepted archive, default 67108864
#   CLIMB_ALLOW_UNPRIVILEGED  1 lets a non-root user publish under CLIMB_ROOT;
#                       only the test suite sets it

set -euo pipefail

climb_root="${CLIMB_ROOT:-/srv/climb}"
climb_etc="${CLIMB_ETC:-/etc/climb}"
climb_service="${CLIMB_SERVICE:-caddy}"
health_attempts="${CLIMB_HEALTH_ATTEMPTS:-20}"
keep_releases="${CLIMB_KEEP_RELEASES:-5}"
max_bytes="${CLIMB_MAX_BYTES:-67108864}"

# A release id is the packaging timestamp and the short commit, nothing else.
release_pattern='[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}'
wasm_magic='0061736d'

# Scratch paths, assigned by main and removed by the exit trap. They are
# globals because the trap runs after main's frame is gone.
work=""
staging=""

say() { printf 'deploy-receive: %s\n' "$*"; }
note() { printf 'deploy-receive: %s\n' "$*" >&2; }

# Mirror the important lines into the host journal, so a deployment that the
# CI log loses can still be reconstructed after the fact. `logger` is part of
# util-linux, but the script must not depend on it.
journal() {
  if command -v logger >/dev/null 2>&1; then
    logger -t climb-deploy -- "$*" 2>/dev/null || true
  fi
}

die() {
  local status="$1"
  shift
  note "$*"
  journal "release refused: $*"
  exit "$status"
}

# ---------------------------------------------------------------------------
# Requests
# ---------------------------------------------------------------------------

# Split `receive <id> <sha256>` into release_id and expected_digest. The whole
# request has to match, so a forced-command key cannot smuggle an argument into
# anything this script runs.
parse_request() {
  local request="$1"
  if [[ ! "$request" =~ ^receive[[:space:]]+([^[:space:]]+)[[:space:]]+([0-9a-fA-F]{64})[[:space:]]*$ ]]; then
    return 1
  fi
  release_id="${BASH_REMATCH[1]}"
  expected_digest="${BASH_REMATCH[2],,}"
}

require_release_id() {
  [[ "$1" =~ ^${release_pattern}$ ]] ||
    die 2 "the release id '$1' is not a <timestamp>-<commit> name"
}

# ---------------------------------------------------------------------------
# Archive handling
# ---------------------------------------------------------------------------

read_archive() {
  local destination="$1" limit size
  limit=$((max_bytes + 1))
  head -c "$limit" >"$destination" || die 3 "reading the archive stream failed"
  size="$(wc -c <"$destination" | tr -d ' ')"
  [ "$size" -gt 0 ] || die 3 "the archive stream was empty"
  [ "$size" -le "$max_bytes" ] ||
    die 3 "the archive is larger than the $max_bytes-byte limit (got $size)"
  say "received $size bytes"
  archive_bytes="$size"
}

# Reject anything that could escape the staging directory before tar writes it.
check_archive_paths() {
  local archive="$1" listing="$2" entry type
  zstd -dc -- "$archive" | tar -tvf - >"$listing" ||
    die 4 "the archive is not a readable zstd-compressed tar"

  while IFS= read -r entry; do
    type="${entry:0:1}"
    case "$type" in
      - | d) ;;
      *) die 4 "the archive carries an entry that is not a file or a directory" ;;
    esac
  done <"$listing"

  zstd -dc -- "$archive" | tar -tf - |
    while IFS= read -r entry; do
      case "$entry" in
        /*) printf 'absolute path: %s\n' "$entry" >&2; exit 1 ;;
        ../* | */../* | */.. | ..) printf 'escaping path: %s\n' "$entry" >&2; exit 1 ;;
        manifest.json | manifest.sha256 | site | site/ | site/?*) ;;
        *) printf 'unexpected path: %s\n' "$entry" >&2; exit 1 ;;
      esac
    done ||
    die 4 "the archive contains a path this station does not accept"
}

unpack_archive() {
  local archive="$1" destination="$2"
  ( umask 022 && zstd -dc -- "$archive" | tar -xf - -C "$destination" --no-same-owner ) ||
    die 4 "unpacking the archive failed"
  chmod -R go-w "$destination"
}

# ---------------------------------------------------------------------------
# Manifest verification: shared with scripts/rollback.sh, which sources this
# file, so a restored release is held to the same evidence as a new one.
# ---------------------------------------------------------------------------

manifest_has() {
  local manifest="$1"
  shift
  jq -e "$@" "$manifest" >/dev/null 2>&1
}

verify_release_tree() {
  local dir="$1" id="$2" work="$3" wasm path digest bytes actual_digest actual_bytes recorded

  [ -f "$dir/manifest.json" ] || die 3 "the archive has no manifest.json"
  [ -f "$dir/manifest.sha256" ] || die 3 "the archive has no manifest.sha256"

  ( cd "$dir" && sha256sum --status -c manifest.sha256 ) ||
    die 3 "manifest.sha256 does not match manifest.json"

  manifest_has "$dir/manifest.json" \
    '.schema == 1 and (.files | type == "array") and (.files | length > 0)' ||
    die 3 "manifest.json is not a schema-1 manifest with a file list"
  recorded="$(jq -r '.release // ""' "$dir/manifest.json")"
  [ "$recorded" = "$id" ] ||
    die 3 "manifest.json describes release '$recorded', not $id"

  wasm="$(jq -r '.wasm.path // ""' "$dir/manifest.json")"
  case "$wasm" in
    site/*.wasm) ;;
    *) die 3 "manifest.json does not name a WebAssembly module under site/" ;;
  esac

  # Per-file length and digest. The manifest is the only claim trusted here,
  # and the archive digest above pins the manifest.
  while IFS=$'\t' read -r path digest bytes; do
    case "$path" in
      "" | /* | *..* | *[[:space:]]*)
        die 3 "manifest.json lists a path this station does not accept: '$path'"
        ;;
    esac
    case "$path" in
      site/*) ;;
      *) die 3 "manifest.json lists '$path', which is outside site/" ;;
    esac
    [ -f "$dir/$path" ] || die 3 "manifest.json lists $path, which the archive does not contain"
    actual_bytes="$(stat -c %s "$dir/$path")"
    [ "$actual_bytes" = "$bytes" ] ||
      die 3 "$path is $actual_bytes bytes, the manifest records $bytes"
    actual_digest="$(sha256sum "$dir/$path" | cut -d' ' -f1)"
    [ "$actual_digest" = "$digest" ] ||
      die 3 "$path has digest $actual_digest, the manifest records $digest"
  done < <(jq -r '.files[] | [.path, .sha256, (.bytes | tostring)] | @tsv' "$dir/manifest.json")

  # Nothing outside the manifest, and nothing listed twice.
  ( cd "$dir" && find . -type f | sed 's|^\./||' | LC_ALL=C sort ) >"$work/on-disk.txt"
  {
    jq -r '.files[].path' "$dir/manifest.json"
    printf '%s\n' manifest.json manifest.sha256
  } | LC_ALL=C sort >"$work/in-manifest.txt"
  LC_ALL=C sort -u "$work/in-manifest.txt" >"$work/in-manifest-unique.txt"
  cmp -s "$work/in-manifest.txt" "$work/in-manifest-unique.txt" ||
    die 3 "manifest.json lists the same path more than once"
  if ! cmp -s "$work/in-manifest.txt" "$work/on-disk.txt"; then
    note "the archive and its manifest disagree:"
    diff -u "$work/in-manifest.txt" "$work/on-disk.txt" | sed -n '1,12p' >&2 || true
    die 3 "the archive carries files manifest.json does not list, or the reverse"
  fi

  [ -f "$dir/site/index.html" ] || die 3 "the archive has no site/index.html"

  local magic files
  magic="$(od -An -tx1 -N4 "$dir/$wasm" | tr -d ' \n')"
  [ "$magic" = "$wasm_magic" ] ||
    die 3 "$wasm does not start with the WebAssembly magic bytes (found $magic)"
  files="$(($(wc -l <"$work/in-manifest.txt") - 2))"
  say "verified $files files, the manifest hash and the module magic bytes"
}

# ---------------------------------------------------------------------------
# Publishing
# ---------------------------------------------------------------------------

# Replace `current` with one rename. `ln -sfn` would unlink and create, which
# leaves a window with no symlink at all; rename(2) does not. Returns non-zero
# instead of exiting, so the caller decides whether that is a rollback.
swap_current() {
  local target="$1" link="$climb_root/current" staged="$climb_root/.current.$$"

  if [ -d "$link" ] && [ ! -L "$link" ]; then
    if ! rmdir "$link" 2>/dev/null; then
      note "$link is a directory with contents; move it aside before deploying"
      return 1
    fi
  fi

  rm -f "$staged"
  if ! ln -s "$target" "$staged"; then
    note "could not create the new symlink under $climb_root"
    return 1
  fi
  if ! mv -T "$staged" "$link"; then
    note "could not replace $link"
    rm -f "$staged"
    return 1
  fi
  return 0
}

reload_service() {
  if command -v systemctl >/dev/null 2>&1; then
    if ! systemctl reload "$climb_service"; then
      note "reloading $climb_service failed"
      return 1
    fi
  else
    note "systemctl is not on PATH; the service was not reloaded"
  fi
  return 0
}

health_base() {
  local address="" env_file="$climb_etc/caddy.env"
  if [ -n "${CLIMB_HEALTH_URL:-}" ]; then
    printf '%s' "${CLIMB_HEALTH_URL%/}"
    return 0
  fi
  if [ -r "$env_file" ]; then
    address="$(sed -n 's/^CLIMB_SITE_ADDRESS=//p' "$env_file" | tail -n 1)"
  fi
  address="${address:-http://localhost}"
  case "$address" in
    http://* | https://*) printf '%s' "${address%/}" ;;
    *) printf 'https://%s' "$address" ;;
  esac
}

# Ask the running site which release it serves. A cached or stale answer is a
# failure, which is the point: the probe is about the bytes a reader receives.
health_probe() {
  local expected="$1" base authority host port rest body code
  base="$(health_base)"
  rest="${base#*://}"
  authority="${rest%%/*}"
  host="${authority%%:*}"
  port=443
  case "$base" in
    http://*) port=80 ;;
  esac
  case "$authority" in
    *:*) port="${authority##*:}" ;;
  esac

  local -a args=(-fsS --max-time 10)
  case "$base" in
    https://*)
      case "$host" in
        127.0.0.1 | localhost | "[::1]") ;;
        *) args+=(--resolve "$host:$port:127.0.0.1") ;;
      esac
      ;;
  esac

  body="$(curl "${args[@]}" "$base/release.json" 2>/dev/null)" || return 1
  [ "$(printf '%s' "$body" | jq -r '.release // ""' 2>/dev/null)" = "$expected" ] || return 1

  code="$(curl "${args[@]}" -o /dev/null -w '%{http_code}' "$base/" 2>/dev/null)" || return 1
  [ "$code" = 200 ] || return 1
  return 0
}

health_check() {
  local expected="$1" attempt=1
  while [ "$attempt" -le "$health_attempts" ]; do
    if health_probe "$expected"; then
      say "$(health_base) serves release $expected"
      return 0
    fi
    attempt=$((attempt + 1))
    sleep 0.5
  done
  return 1
}

# Keep the newest releases and never touch the one `current` names. The count
# includes the current release, so five means five directories on disk.
prune_releases() {
  local current target name kept=0
  current="$(readlink "$climb_root/current" 2>/dev/null || true)"

  while IFS= read -r name; do
    [ -n "$name" ] || continue
    target="$climb_root/releases/$name"
    kept=$((kept + 1))
    [ "$target" = "$current" ] && continue
    [ "$kept" -le "$keep_releases" ] && continue
    rm -rf -- "$target" && say "pruned $name"
  done < <(
    find "$climb_root/releases" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null |
      grep -E "^${release_pattern}$" | LC_ALL=C sort -r || true
  )
}

previous_release() {
  local current
  current="$(readlink "$climb_root/current" 2>/dev/null || true)"
  printf '%s' "$current"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
  local request="${SSH_ORIGINAL_COMMAND:-}" release_dir archive
  local archive_bytes="" previous_id="" previous=""

  if [ -n "$request" ]; then
    parse_request "$request" || die 2 "refused: the only accepted request is 'receive <id> <sha256>'"
  else
    if [ "$#" -eq 0 ] && [ -n "${SUDO_USER:-}" ]; then
      note "the request string is missing: sudo drops SSH_ORIGINAL_COMMAND unless it is kept"
      note "add 'Defaults:${SUDO_USER} env_keep += \"SSH_ORIGINAL_COMMAND\"' to the sudoers entry"
    fi
    [ "$#" -eq 2 ] || die 2 "usage: deploy-receive.sh <release-id> <archive-sha256> (archive on stdin)"
    release_id="$1"
    expected_digest="${2,,}"
    [[ "$expected_digest" =~ ^[0-9a-f]{64}$ ]] || die 2 "the digest must be 64 hexadecimal characters"
  fi
  require_release_id "$release_id"

  if [ "${CLIMB_ALLOW_UNPRIVILEGED:-0}" != 1 ] && [ "$(id -u)" -ne 0 ]; then
    die 2 "publishing needs root; point a forced-command key at this script instead"
  fi
  for tool in jq zstd tar sha256sum od curl flock; do
    command -v "$tool" >/dev/null 2>&1 || die 2 "$tool is required but not on PATH"
  done

  umask 022
  mkdir -p "$climb_root/releases" "$climb_root/.incoming"

  exec 9>"$climb_root/.deploy.lock"
  flock -n 9 || die 2 "another deployment is in progress"

  release_dir="$climb_root/releases/$release_id"
  [ ! -e "$release_dir" ] || die 2 "release $release_id is already published"

  # The bootstrap creates an empty `current` directory; a directory with
  # contents is somebody's work and is never removed here.
  if [ -d "$climb_root/current" ] && [ ! -L "$climb_root/current" ] &&
    [ -n "$(ls -A "$climb_root/current" 2>/dev/null)" ]; then
    die 2 "$climb_root/current is a directory with contents; move it aside before deploying"
  fi

  work="$(mktemp -d "$climb_root/.incoming/work.XXXXXX")"
  staging="$climb_root/.incoming/$release_id.$$"
  archive="$work/archive.tar.zst"
  previous="$(previous_release)"

  cleanup() {
    rm -rf -- "$work"
    if [ -d "$staging" ]; then
      rm -rf -- "$staging"
    fi
    return 0
  }
  trap cleanup EXIT HUP INT TERM

  say "receiving release $release_id"
  read_archive "$archive"

  local actual_digest
  actual_digest="$(sha256sum "$archive" | cut -d' ' -f1)"
  [ "$actual_digest" = "$expected_digest" ] ||
    die 3 "archive digest $actual_digest is not the pinned $expected_digest"

  check_archive_paths "$archive" "$work/tar-list.txt"
  mkdir -p "$staging"
  unpack_archive "$archive" "$staging"
  verify_release_tree "$staging" "$release_id" "$work"

  mv -T "$staging" "$release_dir" || die 4 "could not move the release into place"
  say "staged $release_dir"

  # From here on a failure has to leave the previous release serving, so the
  # recovery path runs with `set +e`.
  rollback() {
    local failed="$1"
    set +e
    note "$failed; restoring the previous release"
    if [ -n "$previous" ]; then
      previous_id="$(basename "$previous")"
      swap_current "$previous"
      reload_service
      if health_check "$previous_id"; then
        say "rolled back to $previous_id"
      else
        note "the previous release did not answer either; inspect $climb_service"
      fi
    else
      note "this was the first release; there is nothing to restore"
      rm -f "$climb_root/current"
    fi
    if [ -d "$release_dir" ]; then
      mv -T "$release_dir" "$release_dir.failed" 2>/dev/null || true
      note "the failed tree is kept at $release_dir.failed"
    fi
    journal "release $release_id rolled back"
    exit 5
  }

  if ! swap_current "$release_dir"; then
    rollback "the current symlink could not be replaced"
  fi
  if ! reload_service; then
    rollback "reloading $climb_service failed"
  fi

  if ! health_check "$release_id"; then
    rollback "$release_id did not answer at $(health_base)"
  fi

  prune_releases

  say "published $release_id ($archive_bytes bytes)"
  say "current -> $(readlink "$climb_root/current" 2>/dev/null || printf 'unset')"
  journal "release $release_id published"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
