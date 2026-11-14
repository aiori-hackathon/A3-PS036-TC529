#!/usr/bin/env bash
#
# Package the built site and publish it to a Climb host.
#
#   scripts/deploy-local.sh                       build, package, ship
#   scripts/deploy-local.sh --no-build            package the current build, ship
#   scripts/deploy-local.sh --package-only        package into target/deploy, do not ship
#   scripts/deploy-local.sh --dry-run             build and package, print the command
#
# The package is one `tar.zst` holding the built site, a `site/release.json`
# naming the release, a `manifest.json` with a length and a SHA-256 for every
# file, and `manifest.sha256` over the manifest itself. Its digest is printed
# and passed to the far side, which refuses an archive that does not match, so
# the bytes are pinned end to end without trusting the transport.
#
# There is no rsync and no shell on the far side: the archive is piped into a
# forced-command receiver (scripts/deploy-receive.sh), which verifies and
# publishes it. The known-hosts file is required and is never created here: a
# first connection that accepts whatever key answers is the failure this
# avoids.
#
# Environment:
#   CLIMB_DEPLOY_HOST        server name or address
#   CLIMB_DEPLOY_USER        deploy account, default deploy
#   CLIMB_DEPLOY_PORT        SSH port, default 22
#   CLIMB_DEPLOY_KEY         private key, default ~/.ssh/climb_deploy_ed25519
#   CLIMB_DEPLOY_KNOWN_HOSTS known-hosts file, default ~/.ssh/climb_deploy_known_hosts
#   CLIMB_SITE_DIST          built site directory, default site/dist
#   CLIMB_MODULE_DIST        dashboard build directory, default crates/climb-web/dist
#   BROTLI_QUALITY           brotli level for text, default 11
#
# Nothing here runs on a server; the laptop and the CI runner run this same
# script, which is the point.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
site_dir="$repo/site"
dist="${CLIMB_SITE_DIST:-$site_dir/dist}"
module_dir="${CLIMB_MODULE_DIST:-$repo/crates/climb-web/dist}"

host="${CLIMB_DEPLOY_HOST:-}"
user="${CLIMB_DEPLOY_USER:-deploy}"
port="${CLIMB_DEPLOY_PORT:-22}"
key="${CLIMB_DEPLOY_KEY:-$HOME/.ssh/climb_deploy_ed25519}"
known_hosts="${CLIMB_DEPLOY_KNOWN_HOSTS:-$HOME/.ssh/climb_deploy_known_hosts}"
out_dir="$repo/target/deploy"
brotli_quality="${BROTLI_QUALITY:-11}"

build=1
ship=1
dry_run=0
keep=0
allow_dirty=0

usage() {
  sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --build) build=1 ;;
    --no-build) build=0 ;;
    --package-only) ship=0 ;;
    --no-ship) ship=0 ;;
    --dry-run) dry_run=1 ;;
    --keep) keep=1 ;;
    --allow-dirty) allow_dirty=1 ;;
    --host) host="${2:?--host needs a value}"; shift ;;
    --user) user="${2:?--user needs a value}"; shift ;;
    --port) port="${2:?--port needs a value}"; shift ;;
    --key) key="${2:?--key needs a value}"; shift ;;
    --known-hosts) known_hosts="${2:?--known-hosts needs a value}"; shift ;;
    --out) out_dir="${2:?--out needs a directory}"; shift ;;
    -h | --help) usage; exit 0 ;;
    *) printf 'deploy-local: unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done

say() { printf 'deploy-local: %s\n' "$*"; }
die() {
  printf 'deploy-local: %s\n' "$*" >&2
  exit 1
}

for tool in python3 sha256sum tar zstd brotli gzip git; do
  command -v "$tool" >/dev/null 2>&1 || die "$tool is required but not on PATH"
done
[ -d "$site_dir" ] || die "no site/ directory in $repo"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

if [ "$build" -eq 1 ]; then
  command -v trunk >/dev/null 2>&1 || die "trunk is required: cargo install trunk --locked"
  say "building the browser dashboard"
  ( cd "$repo/crates/climb-web" && trunk build --release )
  [ -d "$site_dir/node_modules" ] || die "site/node_modules is missing; run 'cd site && npm ci' once"
  say "building the site"
  ( cd "$site_dir" && npm run build )
fi

[ -f "$dist/index.html" ] || die "$dist/index.html is missing; run a build first"
[ -d "$module_dir" ] || die "$module_dir is missing; run 'cd crates/climb-web && trunk build --release'"
[ -n "$(find "$module_dir" -maxdepth 1 -name '*_bg.wasm' -print -quit)" ] ||
  die "$module_dir has no wasm build; run 'cd crates/climb-web && trunk build --release'"

# Provenance is a claim about the commit that produced the bytes. A dirty tree
# makes the claim untrue, so it is refused unless the operator says otherwise.
if [ "$allow_dirty" -eq 0 ]; then
  dirty="$(git -C "$repo" status --porcelain -- site crates/climb-web docs/sample-output | head -n 5)"
  if [ -n "$dirty" ]; then
    printf '%s\n' "$dirty" >&2
    die "the tree has uncommitted changes under site/, crates/climb-web/ or docs/sample-output/; commit them or pass --allow-dirty"
  fi
fi

short_sha="$(git -C "$repo" rev-parse --short=7 HEAD)"
commit="$(git -C "$repo" rev-parse HEAD)"
stamp="${CLIMB_DEPLOY_STAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"
release_id="$stamp-$short_sha"
[[ "$release_id" =~ ^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]] ||
  die "the release id '$release_id' is not a <timestamp>-<commit> name"

# ---------------------------------------------------------------------------
# Package
# ---------------------------------------------------------------------------

stage="$(mktemp -d "${TMPDIR:-/tmp}/climb-release.XXXXXX")"
cleanup() {
  if [ "$keep" -eq 1 ]; then
    say "kept the staging tree at $stage"
  else
    rm -rf -- "$stage"
  fi
}
trap cleanup EXIT

say "staging release $release_id"
mkdir -p "$stage/site"
cp -a "$dist/." "$stage/site/"

# Caddy serves a `.br` or `.gz` sibling directly and encodes on the fly only
# when neither exists. Compressed formats and small files gain nothing here.
compressed=0
while IFS= read -r -d '' file; do
  case "$file" in
    *.br | *.gz | *.zst | *.woff2 | *.png | *.jpg | *.jpeg | *.webp | *.ico | *.map) continue ;;
    *.html | *.css | *.js | *.json | *.svg | *.wasm | *.txt | *.xml) ;;
    *) continue ;;
  esac
  [ "$(stat -c %s "$file")" -ge 1024 ] || continue
  case "$file" in
    *.wasm) brotli -q 9 -f -k "$file" ;;
    *) brotli -q "$brotli_quality" -f -k "$file" ;;
  esac
  gzip -9 -f -k "$file"
  compressed=$((compressed + 1))
done < <(find "$stage/site" -type f -print0)
say "wrote brotli and gzip siblings for $compressed file(s)"

python3 - "$stage" "$release_id" "$commit" <<'PY'
"""Name the release, then hash every file into the manifest."""
import datetime
import hashlib
import json
import os
import sys

stage, release_id, commit = sys.argv[1:4]

now = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)
release = {
    "schema": 1,
    "release": release_id,
    "commit": commit,
    "created_at": now.isoformat().replace("+00:00", "Z"),
}
with open(os.path.join(stage, "site", "release.json"), "w", encoding="utf-8") as handle:
    json.dump(release, handle, indent=2, sort_keys=True)
    handle.write("\n")

manifest_path = os.path.join(stage, "site", "tui", "manifest.json")
try:
    with open(manifest_path, encoding="utf-8") as handle:
        module = json.load(handle)
except FileNotFoundError:
    sys.exit("site/tui/manifest.json is missing: the site build published no dashboard module")
wasm_url = module.get("wasm")
if not isinstance(wasm_url, str) or not wasm_url.startswith("/tui/"):
    sys.exit("site/tui/manifest.json does not name a wasm module under /tui/")
wasm_path = "site" + wasm_url

files = []
for base, dirs, names in os.walk(stage):
    dirs.sort()
    for name in sorted(names):
        full = os.path.join(base, name)
        rel = os.path.relpath(full, stage).replace(os.sep, "/")
        if rel in ("manifest.json", "manifest.sha256"):
            continue
        with open(full, "rb") as handle:
            data = handle.read()
        files.append(
            {"path": rel, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        )
files.sort(key=lambda item: item["path"])

paths = {item["path"] for item in files}
for required in ("site/index.html", wasm_path):
    if required not in paths:
        sys.exit(f"{required} is not in the payload")

wasm_digest = next(item["sha256"] for item in files if item["path"] == wasm_path)
with open(os.path.join(stage, wasm_path), "rb") as handle:
    magic = handle.read(4).hex()

manifest = {
    "schema": 1,
    "release": release_id,
    "commit": commit,
    "created_at": release["created_at"],
    "payload": "site",
    "bytes": sum(item["bytes"] for item in files),
    "files": files,
    "wasm": {"path": wasm_path, "sha256": wasm_digest, "magic": magic},
    "generator": "scripts/deploy-local.sh",
}
with open(os.path.join(stage, "manifest.json"), "w", encoding="utf-8") as handle:
    json.dump(manifest, handle, indent=2)
    handle.write("\n")

digest = hashlib.sha256(
    open(os.path.join(stage, "manifest.json"), "rb").read()
).hexdigest()
with open(os.path.join(stage, "manifest.sha256"), "w", encoding="utf-8") as handle:
    handle.write(f"{digest}  manifest.json\n")

print(f"deploy-local: {len(files)} files, {manifest['bytes']} payload bytes")
PY

mkdir -p "$out_dir"
archive="$out_dir/climb-site-$release_id.tar.zst"
tar -C "$stage" -cf - manifest.json manifest.sha256 site |
  zstd -q -19 -T0 -o "$archive" -f
digest="$(sha256sum "$archive" | cut -d' ' -f1)"
bytes="$(stat -c %s "$archive")"
say "packaged $archive"
say "archive sha256 $digest ($bytes bytes)"

if [ "$ship" -eq 0 ]; then
  say "package only: nothing was sent"
  printf 'receive-command: %s\n' "receive $release_id $digest"
  exit 0
fi

# ---------------------------------------------------------------------------
# Ship
# ---------------------------------------------------------------------------

[ -n "$host" ] || die "no host: set CLIMB_DEPLOY_HOST or pass --host"
[ -r "$key" ] || die "no readable key at $key (set CLIMB_DEPLOY_KEY or pass --key)"
[ -r "$known_hosts" ] || die "no readable known-hosts file at $known_hosts; pin the host key first:
  ssh-keyscan -p $port $host > $known_hosts
  ssh-keygen -lf $known_hosts   # compare with the fingerprint the provider shows"

ssh_command=(
  ssh
  -i "$key"
  -p "$port"
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$known_hosts"
  -o ConnectTimeout=15
  -o ServerAliveInterval=15
  "$user@$host"
  "receive $release_id $digest"
)

if [ "$dry_run" -eq 1 ]; then
  say "dry run: would run"
  printf '  %q ' "${ssh_command[@]}"
  printf '< %q\n' "$archive"
  exit 0
fi

say "shipping to $user@$host:$port"
"${ssh_command[@]}" <"$archive"
say "published $release_id"
