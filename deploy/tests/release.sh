#!/usr/bin/env bash
#
# Exercise the release pipeline against temporary directories and stand-in
# commands: scripts/deploy-local.sh packages a fixture site, and
# scripts/deploy-receive.sh verifies and publishes it.
#
# Nothing here touches the machine or the network. CLIMB_ROOT and CLIMB_ETC are
# temporary directories, `systemctl`, `curl` and `logger` are scripts in a
# temporary PATH, and the payload is a fixture this suite writes. The failing
# health probe reads `current` the way a real server does, so a rollback test
# proves the symlink moved and not just that a message was printed.
#
# Three outcomes the phase asks for are covered by name: a clean publish, a
# corrupted manifest, and an injected failing health check that has to roll
# back.
#
#   bash deploy/tests/release.sh
#
# Exit status is non-zero when any check fails.

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
scripts="$repo/scripts"
work="$(mktemp -d "${TMPDIR:-/tmp}/climb-release-test.XXXXXX")"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT

fake="$work/bin"
log="$work/commands.log"
root="$work/srv/climb"
etc_dir="$work/etc/climb"
out_dir="$work/out"
fixture="$work/site"
mkdir -p "$fake" "$root" "$etc_dir" "$out_dir"
: >"$log"
printf 'CLIMB_SITE_ADDRESS=http://127.0.0.1:8099\n' >"$etc_dir/caddy.env"

checks=0
failures=0

expect_status() {
  local actual="$1" expected="$2" label="$3"
  checks=$((checks + 1))
  if [ "$actual" -eq "$expected" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (expected exit %s, got %s)\n' "$label" "$expected" "$actual"
}

expect_has() {
  local haystack="$1" needle="$2" label="$3"
  checks=$((checks + 1))
  if printf '%s' "$haystack" | grep -qF -- "$needle"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (missing: %s)\n' "$label" "$needle"
}

expect_lacks() {
  local haystack="$1" needle="$2" label="$3"
  checks=$((checks + 1))
  if ! printf '%s' "$haystack" | grep -qF -- "$needle"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (unexpected: %s)\n' "$label" "$needle"
}

expect_true() {
  local label="$1"
  shift
  checks=$((checks + 1))
  if "$@"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s\n' "$label"
}

expect_file() {
  local path="$1" label="$2"
  checks=$((checks + 1))
  if [ -f "$path" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (no file at %s)\n' "$label" "$path"
}

expect_no_file() {
  local path="$1" label="$2"
  checks=$((checks + 1))
  if [ ! -e "$path" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (a file exists at %s)\n' "$label" "$path"
}

expect_equal() {
  local actual="$1" expected="$2" label="$3"
  checks=$((checks + 1))
  if [ "$actual" = "$expected" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (got "%s", expected "%s")\n' "$label" "$actual" "$expected"
}

section() { printf '\n# %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Stand-in commands
# ---------------------------------------------------------------------------

cat >"$fake/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
[ "${FAKE_SYSTEMCTL_FAIL:-0}" = "1" ] && exit 1
exit 0
EOF

cat >"$fake/logger" <<'EOF'
#!/usr/bin/env bash
printf 'logger %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
exit 0
EOF

# The probe has to reflect `current`, not an argument: a release only counts
# as served when the symlink already points at it.
cat >"$fake/curl" <<'EOF'
#!/usr/bin/env bash
printf 'curl %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
url=""
want_code=0
for argument in "$@"; do
  case "$argument" in
    http://* | https://*) url="$argument" ;;
    *'%{http_code}'*) want_code=1 ;;
  esac
done
case "${FAKE_CURL_MODE:-ok}" in
  down)
    printf 'curl: (7) failed to connect\n' >&2
    exit 7
    ;;
esac
if [ "$want_code" -eq 1 ]; then
  printf '200'
  exit 0
fi
case "$url" in
  */release.json)
    if [ -n "${FAKE_SERVED_ID:-}" ]; then
      printf '{"schema":1,"release":"%s"}\n' "$FAKE_SERVED_ID"
      exit 0
    fi
    if [ -r "${CLIMB_ROOT:-/nonexistent}/current/site/release.json" ]; then
      cat "${CLIMB_ROOT}/current/site/release.json"
      exit 0
    fi
    exit 22
    ;;
esac
exit 0
EOF

chmod +x "$fake/systemctl" "$fake/logger" "$fake/curl"

# A git that reports a dirty tree, so the packager's provenance guard is tested
# without depending on the state of the checkout this suite runs in.
mkdir -p "$work/dirty-bin"
cat >"$work/dirty-bin/git" <<'EOF'
#!/usr/bin/env bash
case "$*" in
  *"status --porcelain"*) printf ' M site/src/pages/index.astro\n'; exit 0 ;;
  *"rev-parse --short=7"*) printf 'abc1234\n'; exit 0 ;;
  *"rev-parse HEAD"*) printf 'abc1234abc1234abc1234abc1234abc1234abc1\n'; exit 0 ;;
  *) exec /usr/bin/git "$@" ;;
esac
EOF
chmod +x "$work/dirty-bin/git"

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

# A minimal build output: the pages, one hashed stylesheet, the dashboard
# manifest and a module whose first four bytes decide whether the receiver
# accepts it.
make_site() {
  local dir="$1" magic="${2:-0061736d}"
  rm -rf "$dir"
  mkdir -p "$dir/tui" "$dir/_astro" "$dir/data"
  {
    printf '<!doctype html>\n<html lang="en"><head><title>CLIMB — fixture</title></head><body>\n'
    for line in $(seq 1 60); do
      printf '<p>CLIMB fixture line %s of a release payload.</p>\n' "$line"
    done
    printf '</body></html>\n'
  } >"$dir/index.html"
  printf 'body { color: #f5f5f5; }\n' >"$dir/_astro/app.css"
  printf '404 Not Found\n' >"$dir/404.html"
  printf 'console.log("fixture");\n' >"$dir/tui/climb-web-fixture.js"
  printf '{\n  "entry": "/tui/climb-web-fixture.js",\n  "wasm": "/tui/climb-web-fixture_bg.wasm"\n}\n' \
    >"$dir/tui/manifest.json"
  if [ "$magic" = "0061736d" ]; then
    printf '\x00asm\x01\x00\x00\x00' >"$dir/tui/climb-web-fixture_bg.wasm"
  else
    printf 'NOTA' >"$dir/tui/climb-web-fixture_bg.wasm"
  fi
  head -c 2048 /dev/zero >>"$dir/tui/climb-web-fixture_bg.wasm"
  printf '{"rows":[]}\n' >"$dir/data/climb-results.json"
}

make_site "$fixture"

# Package the fixture and record the release id, the digest and the archive.
package_release() {
  local stamp="$1" site_dir="${2:-$fixture}"
  pkg_output="$(CLIMB_SITE_DIST="$site_dir" CLIMB_DEPLOY_STAMP="$stamp" \
    bash "$scripts/deploy-local.sh" --no-build --package-only --out "$out_dir" --allow-dirty 2>&1)"
  pkg_status=$?
  release_id="$(printf '%s\n' "$pkg_output" |
    sed -n 's/^receive-command: receive \([^ ]*\) .*/\1/p' | tail -n 1)"
  digest="$(printf '%s\n' "$pkg_output" |
    sed -n 's/^receive-command: receive [^ ]* \([0-9a-f]\{64\}\)$/\1/p' | tail -n 1)"
  archive="$out_dir/climb-site-$release_id.tar.zst"
}

# Publish one release through the receiver, exactly as the forced command does.
run_receive() {
  local archive_path="$1" id="$2" sha="$3"
  shift 3
  env PATH="$fake:$PATH" \
    CLIMB_ROOT="$root" \
    CLIMB_ETC="$etc_dir" \
    CLIMB_ALLOW_UNPRIVILEGED=1 \
    CLIMB_HEALTH_ATTEMPTS=2 \
    FAKE_LOG="$log" \
    "$@" \
    bash "$scripts/deploy-receive.sh" "$id" "$sha" <"$archive_path" 2>&1
}

# The forced-command path: no argv, one request string, archive on stdin.
run_receive_ssh() {
  local archive_path="$1" request="$2"
  shift 2
  env PATH="$fake:$PATH" \
    CLIMB_ROOT="$root" \
    CLIMB_ETC="$etc_dir" \
    CLIMB_ALLOW_UNPRIVILEGED=1 \
    CLIMB_HEALTH_ATTEMPTS=2 \
    FAKE_LOG="$log" \
    SSH_ORIGINAL_COMMAND="$request" \
    "$@" \
    bash "$scripts/deploy-receive.sh" <"$archive_path" 2>&1
}

run_rollback() {
  env PATH="$fake:$PATH" \
    CLIMB_ROOT="$root" \
    CLIMB_ETC="$etc_dir" \
    CLIMB_ALLOW_UNPRIVILEGED=1 \
    CLIMB_HEALTH_ATTEMPTS=2 \
    FAKE_LOG="$log" \
    bash "$scripts/rollback.sh" "$@" 2>&1
}

# Repack a published archive after editing one member, with a fresh digest, so
# the transport check passes and the verification has to catch the edit.
repack_edit() {
  local source_archive="$1" replacement="$2" target="$3" target_digest
  local scratch="$work/repack"
  rm -rf "$scratch"
  mkdir -p "$scratch"
  tar -C "$scratch" -xf "$source_archive" --zstd
  ( cd "$scratch" && eval "$replacement" )
  tar -C "$scratch" -cf - manifest.json manifest.sha256 site | zstd -q -19 -T0 -o "$target" -f
  target_digest="$(sha256sum "$target" | cut -d' ' -f1)"
  printf '%s' "$target_digest"
}

current_target() { readlink "$root/current" 2>/dev/null || printf 'unset'; }

count_releases() {
  find "$root/releases" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null |
    grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$' || true
}

# ---------------------------------------------------------------------------
section "the packager names the release and pins its bytes"
# ---------------------------------------------------------------------------
package_release 20260101T000001Z
expect_status "$pkg_status" 0 "packaging the fixture exits 0"
expect_has "$pkg_output" 'deploy-local: packaged' "the archive path is printed"
expect_equal "${release_id:0:9}" "20260101T" "the release id starts with the stamp"
expect_equal "${#release_id}" 24 "the release id is a stamp, a dash and a short commit"
expect_equal "${#digest}" 64 "the digest is 64 hexadecimal characters"
expect_file "$archive" "the archive is written"
expect_has "$pkg_output" 'package only: nothing was sent' "package-only sends nothing"
expect_file "$out_dir/climb-site-$release_id.tar.zst" "the archive name carries the release id"

listing="$(tar -tf "$archive" --zstd 2>/dev/null)"
expect_has "$listing" 'manifest.json' "the archive carries the manifest"
expect_has "$listing" 'manifest.sha256' "the archive carries the manifest hash"
expect_has "$listing" 'site/index.html' "the archive carries the page"
expect_has "$listing" 'site/index.html.br' "the page has a brotli sibling"
expect_has "$listing" 'site/index.html.gz' "the page has a gzip sibling"
expect_lacks "$listing" 'site/_astro/app.css.br' "a file below the compression threshold is not precompressed"

rm -rf "$work/inspect"
mkdir -p "$work/inspect"
tar -C "$work/inspect" -xf "$archive" --zstd
manifest="$work/inspect/manifest.json"
expect_true "manifest.sha256 matches the manifest" \
  bash -c "cd '$work/inspect' && sha256sum --status -c manifest.sha256"
expect_equal "$(jq -r '.release' "$manifest")" "$release_id" "the manifest names the release"
expect_equal "$(jq -r '.schema' "$manifest")" 1 "the manifest is schema 1"
expect_equal "$(jq -r '.wasm.path' "$manifest")" 'site/tui/climb-web-fixture_bg.wasm' \
  "the manifest names the module"
expect_equal "$(jq -r '.wasm.magic' "$manifest")" '0061736d' "the manifest records the magic bytes"
expect_equal "$(jq -r '.files | length' "$manifest")" "$(find "$work/inspect/site" -type f | wc -l | tr -d ' ')" \
  "every payload file is listed"
expect_true "the payload names the release" \
  test "$(jq -r '.release' "$work/inspect/site/release.json")" = "$release_id"

# ---------------------------------------------------------------------------
section "the packager refuses an uncommitted tree"
# ---------------------------------------------------------------------------
dirty_output="$(PATH="$work/dirty-bin:$PATH" CLIMB_SITE_DIST="$fixture" \
  CLIMB_DEPLOY_STAMP=20260101T000002Z \
  bash "$scripts/deploy-local.sh" --no-build --package-only --out "$out_dir" 2>&1)"
expect_status "$?" 1 "a dirty tree is refused"
expect_has "$dirty_output" 'uncommitted changes under site/' "the refusal names the tree"
expect_has "$dirty_output" 'M site/src/pages/index.astro' "the refusal shows the file"

# ---------------------------------------------------------------------------
section "the first release is published through the forced command"
# ---------------------------------------------------------------------------
mkdir -p "$root/current" # the bootstrap's empty directory
: >"$log"
out="$(run_receive_ssh "$archive" "receive $release_id $digest")"
status=$?
expect_status "$status" 0 "the forced command publishes the first release"
expect_has "$out" "receiving release $release_id" "the release id is printed"
expect_has "$out" 'verified ' "file verification is reported"
expect_has "$out" "serves release $release_id" "the health probe reports the new release"
expect_equal "$(current_target)" "$root/releases/$release_id" "current is a symlink to the release"
expect_true "current is a symlink, not a directory" test -L "$root/current"
expect_file "$root/releases/$release_id/manifest.json" "the release keeps its manifest"
expect_file "$root/releases/$release_id/site/index.html.br" "the release keeps its brotli sibling"
expect_has "$(cat "$log")" 'systemctl reload caddy' "the service is reloaded"
expect_has "$(cat "$log")" 'climb-deploy -- release' "the deployment reaches the journal"
expect_no_file "$root/.incoming/$release_id" "the staging tree is removed"
expect_equal "$(find "$root/.incoming" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" 0 \
  "no staging directory is left behind"

# ---------------------------------------------------------------------------
section "the receiver refuses what it cannot verify"
# ---------------------------------------------------------------------------
first_release="$release_id"
first_archive="$archive"
first_digest="$digest"
package_release 20260101T000002Z
out="$(run_receive "$archive" "$release_id" "$(printf '0%.0s' $(seq 1 64))")"
expect_status "$?" 3 "a wrong archive digest is refused"
expect_has "$out" 'is not the pinned' "the refusal names the digest mismatch"
expect_equal "$(current_target)" "$root/releases/$first_release" "a refused digest leaves current alone"
expect_equal "$(count_releases)" 1 "a refused digest publishes no release"

out="$(run_receive_ssh "$archive" "receive $release_id short")"
expect_status "$?" 2 "a malformed request is refused"
expect_has "$out" "only accepted request" "the refusal names the accepted request"

out="$(run_receive_ssh "$archive" "rm -rf /")"
expect_status "$?" 2 "an unrelated forced command is refused"
expect_has "$out" "only accepted request" "the refusal does not run the command"

out="$(run_receive "$archive" "not-a-release" "$digest")"
expect_status "$?" 2 "a release id that is not a stamp is refused"
expect_has "$out" 'is not a <timestamp>-<commit> name' "the refusal names the shape"

out="$(env PATH="$fake:$PATH" CLIMB_ROOT="$root" CLIMB_ETC="$etc_dir" \
  CLIMB_ALLOW_UNPRIVILEGED=1 bash "$scripts/deploy-receive.sh" only-one-argument 2>&1)"
expect_status "$?" 2 "a direct run with one argument prints usage"
expect_has "$out" 'usage: deploy-receive.sh' "the usage line names the arguments"

out="$(env PATH="$fake:$PATH" CLIMB_ROOT="$root" CLIMB_ETC="$etc_dir" \
  CLIMB_ALLOW_UNPRIVILEGED=1 SUDO_USER=deploy bash "$scripts/deploy-receive.sh" 2>&1)"
expect_status "$?" 2 "a run through sudo with no request is refused"
expect_has "$out" 'sudo drops SSH_ORIGINAL_COMMAND' "the refusal explains the sudo environment"

out="$(run_receive "$first_archive" "$first_release" "$first_digest")"
expect_status "$?" 2 "an existing release is refused"
expect_has "$out" 'already published' "the refusal names the existing release"

# A `current` that is a directory with contents is somebody's tree.
mv "$root/current" "$work/current-link"
mkdir -p "$root/current"
printf 'keep me\n' >"$root/current/notes.txt"
package_release 20260101T000003Z
out="$(run_receive "$archive" "$release_id" "$digest")"
expect_status "$?" 2 "a current directory with contents is refused"
expect_has "$out" 'move it aside' "the refusal says how to proceed"
rm -rf "$root/current"
mv "$work/current-link" "$root/current"

# ---------------------------------------------------------------------------
section "a corrupted manifest is refused"
# ---------------------------------------------------------------------------
package_release 20260101T000004Z
corrupt="$out_dir/work-corrupt.tar.zst"
corrupt_digest="$(repack_edit "$archive" \
  "sed -i 's/\"payload\": \"site\"/\"payload\": \"site\", \"extra\": true/' manifest.json" \
  "$corrupt")"
out="$(run_receive "$corrupt" "$release_id" "$corrupt_digest")"
expect_status "$?" 3 "a manifest whose hash does not match is refused"
expect_has "$out" 'manifest.sha256 does not match manifest.json' "the refusal names the manifest"
expect_equal "$(current_target)" "$root/releases/$first_release" "the corrupted release is not published"
expect_no_file "$root/releases/$release_id" "no directory is created for the corrupted release"

# A manifest that is not the release it claims.
package_release 20260101T000005Z
wrong_id="$out_dir/work-wrong-id.tar.zst"
wrong_digest="$(repack_edit "$archive" \
  "jq '.release = \"20260101T000006Z-abc1234\"' manifest.json > manifest.json.new && mv manifest.json.new manifest.json && sha256sum manifest.json > manifest.sha256" \
  "$wrong_id")"
out="$(run_receive "$wrong_id" "$release_id" "$wrong_digest")"
expect_status "$?" 3 "a manifest naming another release is refused"
expect_has "$out" 'describes release' "the refusal names the recorded release"

# ---------------------------------------------------------------------------
section "tampered payloads are refused"
# ---------------------------------------------------------------------------
package_release 20260101T000007Z
tampered="$out_dir/work-tampered.tar.zst"
tampered_digest="$(repack_edit "$archive" \
  "printf 'tampered\n' >> site/index.html" "$tampered")"
out="$(run_receive "$tampered" "$release_id" "$tampered_digest")"
expect_status "$?" 3 "a modified file is refused"
expect_has "$out" 'site/index.html' "the refusal names the file"

extra="$out_dir/work-extra.tar.zst"
extra_digest="$(repack_edit "$archive" \
  "printf 'extra\n' > site/extra.txt" "$extra")"
out="$(run_receive "$extra" "$release_id" "$extra_digest")"
expect_status "$?" 3 "an unlisted file is refused"
expect_has "$out" 'site/extra.txt' "the refusal names the unlisted file"

# ---------------------------------------------------------------------------
section "a module that is not WebAssembly is refused"
# ---------------------------------------------------------------------------
not_wasm="$work/not-wasm"
make_site "$not_wasm" 4e4f5441
package_release 20260101T000008Z "$not_wasm"
out="$(run_receive "$archive" "$release_id" "$digest")"
expect_status "$?" 3 "a payload whose module lacks the magic bytes is refused"
expect_has "$out" 'WebAssembly magic bytes' "the refusal names the magic bytes"
expect_no_file "$root/releases/$release_id" "the refused release leaves no directory"

# ---------------------------------------------------------------------------
section "an escaping path is refused"
# ---------------------------------------------------------------------------
package_release 20260101T000009Z
escape="$out_dir/work-escape.tar.zst"
zstd -q -dc "$archive" >"$work/plain.tar"
python3 - "$work/plain.tar" "$work/escape-plain.tar" <<'PY'
import sys
import tarfile

source, target = sys.argv[1], sys.argv[2]
with tarfile.open(source) as archive:
    members = [
        (member, archive.extractfile(member).read() if member.isfile() else None)
        for member in archive.getmembers()
    ]
with tarfile.open(target, "w") as archive:
    for member, data in members:
        if data is None:
            archive.addfile(member)
        else:
            import io

            archive.addfile(member, io.BytesIO(data))
    evil = tarfile.TarInfo("../escape.txt")
    evil.size = 4
    import io

    archive.addfile(evil, io.BytesIO(b"evil"))
PY
zstd -q -19 -T0 -o "$escape" -f "$work/escape-plain.tar"
escape_digest="$(sha256sum "$escape" | cut -d' ' -f1)"
out="$(run_receive "$escape" "$release_id" "$escape_digest")"
expect_status "$?" 4 "an archive with a parent-directory path is refused"
expect_has "$out" 'does not accept' "the refusal names the archive"
expect_no_file "$work/escape.txt" "nothing is written outside the staging directory"

# ---------------------------------------------------------------------------
section "a failing health check rolls the release back"
# ---------------------------------------------------------------------------
package_release 20260101T000010Z
second_release="$release_id"
: >"$log"
out="$(run_receive "$archive" "$release_id" "$digest" FAKE_CURL_MODE=stale FAKE_SERVED_ID="$first_release")"
status=$?
expect_status "$status" 5 "a release the site does not serve fails the deploy"
expect_has "$out" 'restoring the previous release' "the rollback is announced"
expect_has "$out" "rolled back to $first_release" "the previous release answers again"
expect_equal "$(current_target)" "$root/releases/$first_release" "current points at the previous release"
expect_true "the failed tree is kept for inspection" \
  test -d "$root/releases/$second_release.failed"
expect_no_file "$root/releases/$second_release" "the failed release does not keep a live name"
expect_has "$(cat "$log")" 'climb-deploy -- release' "the rollback reaches the journal"

# Nothing at all answers: the swap is still undone.
package_release 20260101T000011Z
out="$(run_receive "$archive" "$release_id" "$digest" FAKE_CURL_MODE=down)"
expect_status "$?" 5 "an unreachable site fails the deploy"
expect_has "$out" 'did not answer either' "the rollback reports that the probe failed twice"
expect_equal "$(current_target)" "$root/releases/$first_release" "the previous release is restored anyway"

# A reload that fails is also a rollback.
package_release 20260101T000012Z
out="$(run_receive "$archive" "$release_id" "$digest" FAKE_SYSTEMCTL_FAIL=1)"
expect_status "$?" 5 "a failing reload fails the deploy"
expect_has "$out" 'reloading caddy failed' "the refusal names the reload"
expect_equal "$(current_target)" "$root/releases/$first_release" "current survives a failed reload"

# ---------------------------------------------------------------------------
section "rollback.sh moves current backwards"
# ---------------------------------------------------------------------------
package_release 20260101T000013Z
third_release="$release_id"
out="$(run_receive "$archive" "$release_id" "$digest")"
expect_status "$?" 0 "a second good release is published"
expect_equal "$(current_target)" "$root/releases/$third_release" "current points at the new release"

: >"$log"
out="$(run_rollback --list)"
expect_has "$out" "* $third_release" "the listing marks the current release"
expect_has "$out" "  $first_release" "the listing carries the earlier release"

out="$(run_rollback previous)"
expect_status "$?" 0 "the release before the current one is restored"
expect_has "$out" "verifying $first_release" "the target is verified before the swap"
expect_has "$out" "rolled back to $first_release" "the rollback is reported"
expect_equal "$(current_target)" "$root/releases/$first_release" "current points backwards"
expect_has "$(cat "$log")" 'systemctl reload caddy' "the rollback reloads the service"

out="$(run_rollback "$third_release")"
expect_status "$?" 0 "a named release can be restored forwards"
expect_equal "$(current_target)" "$root/releases/$third_release" "current points at the named release"

out="$(run_rollback 20200101T000000Z-0000000)"
expect_status "$?" 2 "an absent release is refused"
expect_has "$out" 'is not on this host' "the refusal names the missing release"

mv "$root/releases/$first_release/manifest.sha256" "$work/manifest.sha256.kept"
out="$(run_rollback "$first_release")"
status=$?
mv "$work/manifest.sha256.kept" "$root/releases/$first_release/manifest.sha256"
expect_status "$status" 3 "a target with no manifest hash is refused"
expect_has "$out" 'no manifest.sha256' "the refusal names the missing hash"
expect_equal "$(current_target)" "$root/releases/$third_release" "a refused rollback leaves current alone"

# ---------------------------------------------------------------------------
section "only the newest five releases are kept"
# ---------------------------------------------------------------------------
: >"$log"
last_release=""
for stamp in 20260101T000020Z 20260101T000021Z 20260101T000022Z 20260101T000023Z; do
  package_release "$stamp"
  last_release="$release_id"
  out="$(run_receive "$archive" "$release_id" "$digest")"
  expect_status "$?" 0 "release $release_id is published"
done
expect_equal "$(count_releases)" 5 "five release directories remain"
expect_has "$out" 'pruned 20260101T000001Z' "the release that fell out is pruned"
expect_true "the current release is never pruned" test -d "$(current_target)"
expect_equal "$(current_target)" "$root/releases/$last_release" "the newest release is current"

# ---------------------------------------------------------------------------
section "the smoke script's route table matches the site"
# ---------------------------------------------------------------------------
sed -n '/^  route_table=(/,/^  )/p' "$scripts/smoke-live.sh" >"$work/routes.txt"
for page in index report verify reproduce tui; do
  case "$page" in
    index) path="/" ;;
    *) path="/$page" ;;
  esac
  title="$(grep -o 'title="[^"]*"' "$repo/site/src/pages/$page.astro" | head -n 1 | sed 's/^title="//; s/"$//')"
  checks=$((checks + 1))
  if [ -z "$title" ]; then
    failures=$((failures + 1))
    printf 'FAIL  site/src/pages/%s.astro carries no title\n' "$page"
  elif ! grep -qF -- "\"$path|$title\"" "$work/routes.txt"; then
    failures=$((failures + 1))
    printf 'FAIL  the smoke script does not expect "%s|%s"\n' "$path" "$title"
  fi
done

# ---------------------------------------------------------------------------
section "static checks"
# ---------------------------------------------------------------------------
for script in "$scripts/deploy-local.sh" "$scripts/deploy-receive.sh" \
  "$scripts/rollback.sh" "$scripts/smoke-live.sh" "$here/release.sh"; do
  expect_true "bash -n $(basename "$script")" bash -n "$script"
done
if command -v shellcheck >/dev/null 2>&1; then
  expect_true "shellcheck is clean" shellcheck -x \
    "$scripts/deploy-local.sh" \
    "$scripts/deploy-receive.sh" \
    "$scripts/rollback.sh" \
    "$scripts/smoke-live.sh" \
    "$here/release.sh"
else
  printf 'note: shellcheck is not installed; the lint check was skipped\n'
fi

# ---------------------------------------------------------------------------

printf '\n%d checks, %d failed\n' "$checks" "$failures"
[ "$failures" -eq 0 ]
