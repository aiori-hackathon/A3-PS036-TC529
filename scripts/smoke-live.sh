#!/usr/bin/env bash
#
# Prove the live dashboard is what the repository claims.
#
#   scripts/smoke-live.sh                        check https://climb-pqc.duckdns.org
#   scripts/smoke-live.sh https://host           check another deployment
#   scripts/smoke-live.sh --budget-ms 1200       raise the response-time budget
#   scripts/smoke-live.sh --require-hsts         fail when HSTS is missing
#
# Nine things are checked, each printed as PASS, FAIL or SKIP with the value it
# saw:
#
#   1. the certificate chain verifies and the leaf does not expire within 14
#      days;
#   2. plain HTTP redirects to HTTPS;
#   3. the response arrives over HTTP/2 or HTTP/3;
#   4. the security headers are present (CSP, nosniff, referrer, permissions,
#      opener policy), and HSTS when it is asked for;
#   5. every route answers 200 and carries its expected document title;
#   6. the dashboard module is served as application/wasm, compressed, and
#      starts with the WebAssembly magic bytes;
#   7. no request on `/` or `/tui/` leaves the site's own origin (needs the
#      site's Playwright package; otherwise reported as a skip, never a pass);
#   8. the time to first byte of `/` stays inside the budget, measured as the
#      median of three requests after one warm-up.
#
# The certificate check is what makes the other seven meaningful: nothing here
# uses --insecure, so a host whose chain does not verify fails every request
# above it.
#
# The exit status is 1 when anything fails, 0 when everything passes or is
# skipped. Nothing is written outside the script's own temporary directory.

set -uo pipefail

base="${CLIMB_LIVE_URL:-https://climb-pqc.duckdns.org}"
budget_ms=800
require_hsts=0
browser=1
timeout_s=20

usage() {
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --budget-ms) budget_ms="${2:?--budget-ms needs a number}"; shift ;;
    --require-hsts) require_hsts=1 ;;
    --skip-browser) browser=0 ;;
    --timeout) timeout_s="${2:?--timeout needs a number}"; shift ;;
    -h | --help) usage; exit 0 ;;
    -*)
      printf 'smoke-live: unknown option: %s\n' "$1" >&2
      exit 2
      ;;
    *) base="$1" ;;
  esac
  shift
done

base="${base%/}"
site_origin="$base"
scheme="${base%%://*}"
authority="${base#*://}"
authority="${authority%%/*}"
host="${authority%%:*}"
plain_port=""
case "$authority" in
  *:*)
    [ "${authority##*:}" = "443" ] || plain_port=":${authority##*:}"
    ;;
esac
plain="http://$host$plain_port/"

work="$(mktemp -d "${TMPDIR:-/tmp}/climb-smoke.XXXXXX")"
cleanup() { rm -rf -- "$work"; }
trap cleanup EXIT

checks=0
failed=0
skipped=0

pass() {
  checks=$((checks + 1))
  printf 'PASS  %-12s %s\n' "$1" "$2"
}
fail() {
  checks=$((checks + 1))
  failed=$((failed + 1))
  printf 'FAIL  %-12s %s\n' "$1" "$2"
}
skip() {
  checks=$((checks + 1))
  skipped=$((skipped + 1))
  printf 'SKIP  %-12s %s\n' "$1" "$2"
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "$2" "$1 is not on PATH"
    return 1
  fi
  return 0
}

printf 'smoke-live: %s\n\n' "$base"

# ---------------------------------------------------------------------------
# 1. Certificate
# ---------------------------------------------------------------------------
if [ "$scheme" != "https" ]; then
  skip certificate "the target is plain HTTP, so there is no certificate to check"
elif need openssl certificate; then
  session="$(printf '' | openssl s_client \
    -connect "$authority" \
    -servername "$host" \
    -verify_return_error 2>&1)"
  verdict="$(printf '%s\n' "$session" | grep -m1 'Verify return code:')"
  not_after="$(printf '%s\n' "$session" | openssl x509 -noout -enddate 2>/dev/null | sed 's/^notAfter=//')"
  if [[ "$verdict" != *"0 (ok)"* ]]; then
    fail certificate "the chain does not verify (${verdict:-no answer from $authority})"
  elif [ -z "$not_after" ]; then
    fail certificate "the handshake presented no certificate"
  elif ! printf '%s\n' "$session" | openssl x509 -noout -checkend 1209600 >/dev/null 2>&1; then
    fail certificate "the leaf expires within 14 days (not after ${not_after})"
  else
    pass certificate "chain verified; not after ${not_after}"
  fi
fi

# ---------------------------------------------------------------------------
# 2. HTTP redirects to HTTPS
# ---------------------------------------------------------------------------
if need curl redirect; then
  answer="$(curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' \
    --max-time "$timeout_s" "$plain" 2>"$work/redirect.err")"
  status="${answer%% *}"
  location="${answer#* }"
  if [ -z "$location" ] && [ "$status" != "000" ]; then
    location="$(curl -sSI --max-time "$timeout_s" "$plain" 2>/dev/null |
      sed -n 's/^[Ll]ocation: *//p' | tr -d '\r' | head -n 1)"
  fi
  case "$status" in
    301 | 308)
      case "$location" in
        https://*) pass redirect "$status to $location" ;;
        *) fail redirect "$status but the location is '${location}'" ;;
      esac
      ;;
    *) fail redirect "$plain answered $status${location:+ to $location}" ;;
  esac
fi

# ---------------------------------------------------------------------------
# 3. HTTP/2 or HTTP/3
# ---------------------------------------------------------------------------
if need curl protocol; then
  if ! curl --version | grep -q 'HTTP2'; then
    skip protocol "this curl has no HTTP/2 support"
  else
    version="$(curl -sS --http2 -o /dev/null -w '%{http_version}' \
      --max-time "$timeout_s" "$base/" 2>"$work/protocol.err")"
    case "$version" in
      2 | 3) pass protocol "HTTP/$version" ;;
      1 | 1.1) fail protocol "HTTP/$version; HTTP/2 is not negotiated" ;;
      *) fail protocol "no answer (${version:-curl failed})" ;;
    esac
  fi
fi

# ---------------------------------------------------------------------------
# 4. Security headers
# ---------------------------------------------------------------------------
if need curl headers; then
  curl -sSI --max-time "$timeout_s" "$base/" >"$work/headers.txt" 2>"$work/headers.err"
  if [ ! -s "$work/headers.txt" ]; then
    fail headers "the response carried no headers"
  else
    missing=""
    grep -qi '^content-security-policy:' "$work/headers.txt" || missing="$missing csp"
    grep -qi '^x-content-type-options: *nosniff' "$work/headers.txt" || missing="$missing nosniff"
    grep -qi '^referrer-policy:' "$work/headers.txt" || missing="$missing referrer-policy"
    grep -qi '^permissions-policy:' "$work/headers.txt" || missing="$missing permissions-policy"
    grep -qi '^cross-origin-opener-policy:' "$work/headers.txt" || missing="$missing opener-policy"
    grep -qi "^content-security-policy:.*default-src 'self'" "$work/headers.txt" ||
      missing="$missing csp-default-src"
    grep -qi "^content-security-policy:.*frame-ancestors 'none'" "$work/headers.txt" ||
      missing="$missing csp-frame-ancestors"
    hsts=0
    grep -qi '^strict-transport-security: *max-age=' "$work/headers.txt" && hsts=1
    if [ "$require_hsts" -eq 1 ] && [ "$hsts" -eq 0 ]; then
      missing="$missing hsts"
    fi
    if [ -z "$missing" ]; then
      detail="csp, nosniff, referrer, permissions, opener"
      if [ "$hsts" -eq 1 ]; then
        detail="$detail, hsts"
      elif [ "$require_hsts" -eq 0 ]; then
        detail="$detail (hsts not required)"
      fi
      pass headers "$detail"
    else
      fail headers "missing:$missing"
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 5. Routes and their titles
# ---------------------------------------------------------------------------
if need curl routes; then
  route_table=(
    "/|CLIMB — post-quantum DNSSEC, measured"
    "/report|CLIMB — the full report"
    "/verify|CLIMB — verify the numbers"
    "/reproduce|CLIMB — rebuild every number"
    "/tui|CLIMB — the recorded dashboard, in a browser tab"
  )
  for entry in "${route_table[@]}"; do
    path="${entry%%|*}"
    title="${entry#*|}"
    body="$work/route.html"
    status="$(curl -sSL -o "$body" -w '%{http_code}' --max-time "$timeout_s" "$base$path" 2>/dev/null)"
    if [ "$status" != "200" ]; then
      fail "route $path" "answered $status"
    elif ! grep -qF "<title>$title</title>" "$body"; then
      found="$(sed -n 's:.*<title>\(.*\)</title>.*:\1:p' "$body" | head -n 1)"
      fail "route $path" "200 but the title is '${found:-none}'"
    else
      pass "route $path" "200, title '$title'"
    fi
  done
fi

# ---------------------------------------------------------------------------
# 6. Module
# ---------------------------------------------------------------------------
if need curl module; then
  manifest="$(curl -sSf --max-time "$timeout_s" "$base/tui/manifest.json" 2>/dev/null)"
  wasm_path=""
  if command -v jq >/dev/null 2>&1; then
    wasm_path="$(printf '%s' "$manifest" | jq -r '.wasm // ""' 2>/dev/null)"
  else
    wasm_path="$(printf '%s\n' "$manifest" | sed -n 's/.*"wasm": *"\([^"]*\)".*/\1/p' | head -n 1)"
  fi
  if [ -z "$wasm_path" ]; then
    fail module "/tui/manifest.json names no module: the dashboard was not published"
  else
    curl -sS -D "$work/module.headers" -o /dev/null -H 'Accept-Encoding: br, gzip' \
      --max-time "$timeout_s" "$base$wasm_path" >/dev/null 2>&1
    curl -sS -o "$work/module.wasm" --max-time "$timeout_s" "$base$wasm_path" >/dev/null 2>&1
    magic="$(od -An -tx1 -N4 "$work/module.wasm" 2>/dev/null | tr -d ' \n')"
    type_ok=0
    grep -qi '^content-type: *application/wasm' "$work/module.headers" && type_ok=1
    encoding="$(sed -n 's/^[Cc]ontent-[Ee]ncoding: *//p' "$work/module.headers" | tr -d '\r' | head -n 1)"
    if [ "$magic" != "0061736d" ]; then
      fail module "$wasm_path does not start with the WebAssembly magic bytes (found ${magic:-nothing})"
    elif [ "$type_ok" -eq 0 ]; then
      fail module "$wasm_path is not served as application/wasm"
    elif [ -z "$encoding" ]; then
      fail module "$wasm_path is served uncompressed"
    else
      pass module "$wasm_path, application/wasm, $encoding"
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 7. Same-origin requests only
# ---------------------------------------------------------------------------
if [ "$browser" -eq 1 ] && [ -d "$(dirname "${BASH_SOURCE[0]}")/../site/node_modules/playwright-core" ]; then
  if ! command -v node >/dev/null 2>&1; then
    skip origin "node is not on PATH, so the browser check cannot run"
  elif (
    cd "$(dirname "${BASH_SOURCE[0]}")/../site" &&
      SMOKE_URL="$base" PLAYWRIGHT_BROWSERS_PATH=0 node --input-type=module <<'NODE'
import { chromium } from 'playwright-core';

const url = new URL(process.env.SMOKE_URL);
const target = url.origin;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const foreign = new Set();
const errors = [];
page.on('request', (request) => {
  const origin = new URL(request.url()).origin;
  if (origin !== target) foreign.add(origin);
});
page.on('console', (message) => {
  if (message.type() === 'error') errors.push(message.text());
});
page.on('pageerror', (error) => errors.push(error.message));

await page.goto(`${target}/`, { waitUntil: 'load' });
await page.waitForTimeout(1_000);
await page.goto(`${target}/tui/`, { waitUntil: 'load' });
await page.waitForTimeout(3_000);
await browser.close();

if (foreign.size > 0) {
  console.error(`requests left the origin: ${[...foreign].join(', ')}`);
  process.exit(1);
}
if (errors.length > 0) {
  console.error(`console errors (${errors.length}): ${errors[0]}`);
  process.exit(1);
}
console.log(`no request left ${target} and no console error was logged`);
NODE
  ) >"$work/origin.log" 2>&1; then
    pass origin "no request left $site_origin on / or /tui/, no console error"
  else
    fail origin "$(tail -n 2 "$work/origin.log" | tr '\n' ' ')"
  fi
else
  if [ "$browser" -eq 0 ]; then
    skip origin "--skip-browser was passed, so no request was inspected"
  else
    skip origin "site/node_modules/playwright-core is absent; run 'cd site && npm ci' to enable it"
  fi
fi

# ---------------------------------------------------------------------------
# 8. Response-time budget
# ---------------------------------------------------------------------------
if need curl budget; then
  curl -sS -o /dev/null --max-time "$timeout_s" "$base/" >/dev/null 2>&1
  : >"$work/timings.txt"
  samples=0
  while [ "$samples" -lt 3 ]; do
    timing="$(curl -sS -o /dev/null -w '%{time_starttransfer}' --max-time "$timeout_s" "$base/" 2>/dev/null)"
    case "$timing" in
      "" | 0.000000) ;;
      *) printf '%s\n' "$timing" >>"$work/timings.txt" ;;
    esac
    samples=$((samples + 1))
  done
  median="$(LC_ALL=C sort -n "$work/timings.txt" | awk 'NF { value[count++] = $1 } END { if (count == 0) exit; print value[int(count / 2)] }')"
  if [ -z "$median" ]; then
    fail budget "no timing was recorded"
  else
    millis="$(awk -v seconds="$median" 'BEGIN { printf "%d", seconds * 1000 + 0.5 }')"
    if [ "$millis" -le "$budget_ms" ]; then
      pass budget "time to first byte ${millis} ms of ${budget_ms} ms"
    else
      fail budget "time to first byte ${millis} ms exceeds ${budget_ms} ms"
    fi
  fi
fi

# ---------------------------------------------------------------------------
printf '\n%d checks: %d passed, %d failed, %d skipped\n' \
  "$checks" "$((checks - failed - skipped))" "$failed" "$skipped"
[ "$failed" -eq 0 ]
