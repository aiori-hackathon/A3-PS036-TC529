#!/usr/bin/env bash
#
# Build the browser dashboard, check its size budget, serve the bundle on the
# loopback interface, and prove it is what it says it is: a WebAssembly module
# that answers with the four magic bytes, a page that draws a canvas, and — if
# a headless Chromium is installed — nine views and the tour with no console
# error.
#
# Nothing here leaves the machine. The server binds 127.0.0.1 and exits with
# the script.
#
#   scripts/wasm-smoke.sh              build, check, serve, verify
#   SKIP_BUILD=1 scripts/wasm-smoke.sh verify an existing dist/
#   PORT=8782 scripts/wasm-smoke.sh    use another port

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="$root/crates/climb-web/dist"
port="${PORT:-8781}"

# The budgets the release profile has to stay inside. They live here, next to
# the check, so a build that grows past them fails rather than ships.
raw_budget=6000000
brotli_budget=2500000

say() { printf '%s\n' "$*"; }
fail() { printf 'wasm-smoke: %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required but not on PATH"
}

need cargo
need python3
need curl
need od

if ! command -v trunk >/dev/null 2>&1; then
  fail "trunk is required: cargo install trunk --locked (or use a release binary)"
fi

if [ "${SKIP_BUILD:-0}" != "1" ]; then
  say "== build: trunk build --release =="
  if ! command -v wasm-opt >/dev/null 2>&1; then
    say "note: wasm-opt is not on PATH; trunk will try to fetch Binaryen."
    say "      Install Binaryen or put wasm-opt on PATH for an offline build."
  fi
  (cd "$root/crates/climb-web" && trunk build --release)
fi

wasm="$(find "$dist" -maxdepth 1 -name '*_bg.wasm' -print -quit)"
[ -n "$wasm" ] || fail "no *_bg.wasm in $dist; run trunk build --release"
entry="$(find "$dist" -maxdepth 1 -name '*.js' -print -quit)"
[ -n "$entry" ] || fail "no JS entry in $dist"

magic="$(od -An -tx1 -N4 "$wasm" | tr -d ' \n')"
[ "$magic" = "0061736d" ] || fail "$wasm does not start with the wasm magic bytes (got $magic)"

raw="$(wc -c <"$wasm" | tr -d ' ')"
say "== size =="
say "module raw bytes:    $raw"
if command -v brotli >/dev/null 2>&1; then
  brotli_bytes="$(brotli -q 11 -c "$wasm" | wc -c | tr -d ' ')"
  say "module brotli bytes: $brotli_bytes"
  [ "$brotli_bytes" -le "$brotli_budget" ] ||
    fail "brotli $brotli_bytes exceeds the $brotli_budget budget"
else
  say "module brotli bytes: skipped (brotli is not on PATH)"
fi
say "entry script bytes:  $(wc -c <"$entry" | tr -d ' ')"
[ "$raw" -le "$raw_budget" ] || fail "raw $raw exceeds the $raw_budget budget"

# The page fetches the recordings from its own origin. Serving a copy beside
# the bundle is what turns those requests into 200s instead of logged 404s.
mkdir -p "$dist/data"
for name in climb-results.json ladder-verification.recorded.json dns-capture.recorded.json; do
  cp "$root/docs/sample-output/$name" "$dist/data/$name"
done

say "== serve: http://127.0.0.1:$port =="
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$dist" >"$dist/.smoke-server.log" 2>&1 &
server=$!
cleanup() {
  kill "$server" 2>/dev/null || true
  wait "$server" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 50); do
  if curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.1
done
[ "$ready" = "1" ] || fail "the local server did not answer"

status="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")"
[ "$status" = "200" ] || fail "GET / returned HTTP $status"
say "GET / -> 200"

wasm_name="$(basename "$wasm")"
curl -fsS -o "$dist/.smoke-download.wasm" "http://127.0.0.1:$port/$wasm_name"
served_magic="$(od -An -tx1 -N4 "$dist/.smoke-download.wasm" | tr -d ' \n')"
[ "$served_magic" = "0061736d" ] || fail "GET /$wasm_name did not serve a wasm module"
say "GET /$wasm_name -> 200, magic $served_magic"
rm -f "$dist/.smoke-download.wasm"

# A browser is optional: the size, HTTP and magic checks above are the gate.
browser_root="$root/site/node_modules/playwright-core"
if [ -d "$browser_root" ]; then
  say "== headless Chromium =="
  (
    cd "$root/site"
    PLAYWRIGHT_BROWSERS_PATH=0 SMOKE_URL="http://127.0.0.1:$port/" \
      node --input-type=module <<'NODE'
import { chromium } from 'playwright-core';

const url = process.env.SMOKE_URL;
if (!url) throw new Error('SMOKE_URL is not set');

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('console', (message) => {
  if (message.type() === 'error') errors.push(message.text());
});
page.on('pageerror', (error) => errors.push(error.message));

await page.goto(url, { waitUntil: 'load' });
const canvas = page.locator('#grid canvas').first();
await canvas.waitFor({ state: 'visible', timeout: 30_000 });
const box = await canvas.boundingBox();
if (box === null || box.width < 320 || box.height < 200) {
  throw new Error(`the canvas is too small to be a dashboard: ${JSON.stringify(box)}`);
}
await page.waitForTimeout(1_200);

// Every view has its own number key; the tour opens on its first step.
for (const key of ['1', '2', '3', '4', '5', '6', '7', '8', '9']) {
  await page.keyboard.press(key);
  await page.waitForTimeout(80);
}

await page.goto(`${url}?tour=1`, { waitUntil: 'load' });
await page.locator('#grid canvas').first().waitFor({ state: 'visible', timeout: 30_000 });
await page.waitForTimeout(1_000);

await browser.close();

if (errors.length > 0) {
  console.error(`console errors (${errors.length}):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}
console.log(
  `canvas ${Math.round(box.width)} by ${Math.round(box.height)}; nine views and the tour; zero console errors`,
);
NODE
  )
else
  say "== headless Chromium =="
  say "not installed under site/node_modules/playwright-core; browser smoke skipped"
fi

say "wasm-smoke: ok"
