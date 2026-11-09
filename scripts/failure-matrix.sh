#!/usr/bin/env bash
# Failure-injection matrix for the artifact pipeline.
#
# Each row damages one input in an isolated copy of the repository's artifacts
# and checks that the consumer exits non-zero with an error that names the file
# and the fix. A row passes only when both conditions hold; a panic, a zero
# exit, or a message that does not identify the damaged input fails the row.
#
# Usage: scripts/failure-matrix.sh [path-to-xtask] [path-to-climb-tui]

set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
XTASK="${1:-$ROOT/target/debug/xtask}"
TUI="${2:-$ROOT/target/debug/climb-tui}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/climb-failure-matrix.XXXXXX")"
PASS=0
FAIL=0

cleanup() {
  chmod -R u+w "$WORK" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

if [[ ! -x "$XTASK" ]]; then
  echo "missing xtask binary: $XTASK (run cargo build -p xtask)" >&2
  exit 2
fi
if [[ ! -x "$TUI" ]]; then
  echo "missing climb-tui binary: $TUI (run cargo build -p climb-tui)" >&2
  exit 2
fi

make_sandbox() {
  local dir="$1"
  mkdir -p "$dir/target"
  cp -a "$ROOT/target/criterion" "$dir/target/criterion"
  cp "$ROOT/target/wire-report.json" "$dir/target/wire-report.json"
  if [[ -f "$ROOT/target/memory-report.json" ]]; then
    cp "$ROOT/target/memory-report.json" "$dir/target/memory-report.json"
  else
    cp "$ROOT/target/memory-report.stale.json" "$dir/target/memory-report.json"
  fi
  cp "$ROOT/environment.json" "$dir/environment.json"
  mkdir -p "$dir/data"
  cp "$ROOT/data/dns-capture.recorded.json" "$dir/data/dns-capture.recorded.json"
  cp "$ROOT/data/ladder-verification.recorded.json" "$dir/data/ladder-verification.recorded.json"
  mkdir -p "$dir/docs/sample-output"
  cp "$ROOT/docs/sample-output/climb-results.json" "$dir/docs/sample-output/climb-results.json"
}

# run_expect_fail <name> <expected-regex> <dir> <command...>
run_expect_fail() {
  local name="$1" expected="$2" dir="$3"
  shift 3
  local out rc
  out="$(cd "$dir" && "$@" 2>&1)"
  rc=$?
  if [[ $rc -eq 0 ]]; then
    printf 'FAIL  %-34s exit 0 (expected non-zero)\n' "$name"
    FAIL=$((FAIL + 1))
    return
  fi
  if ! grep -Eqi -- "$expected" <<<"$out"; then
    printf 'FAIL  %-34s exit %d, message did not match /%s/\n' "$name" "$rc" "$expected"
    printf '      output: %s\n' "$(head -n 2 <<<"$out" | tr '\n' ' ')"
    FAIL=$((FAIL + 1))
    return
  fi
  local line
  line="$(grep -Ei -- "$expected" <<<"$out" | head -n 1 | sed 's/[[:space:]]\+/ /g')"
  printf 'PASS  %-34s exit %d  %s\n' "$name" "$rc" "$line"
  PASS=$((PASS + 1))
}

run_expect_ok() {
  local name="$1" dir="$2"
  shift 2
  local out rc
  out="$(cd "$dir" && "$@" 2>&1)"
  rc=$?
  if [[ $rc -ne 0 ]]; then
    printf 'FAIL  %-34s exit %d (expected 0)\n' "$name" "$rc"
    printf '      output: %s\n' "$(head -n 2 <<<"$out" | tr '\n' ' ')"
    FAIL=$((FAIL + 1))
    return
  fi
  printf 'PASS  %-34s exit 0\n' "$name"
  PASS=$((PASS + 1))
}

# 0. Control: undamaged sample artifacts must validate.
D="$WORK/control"
make_sandbox "$D"
run_expect_ok "control: tui --check" "$D" \
  "$TUI" --check \
  --results "$D/docs/sample-output/climb-results.json" \
  --ladder "$D/data/ladder-verification.recorded.json" \
  --live "$D/data/dns-capture.recorded.json"

# 1-4. Corrupt, truncate, remove and field-strip the consolidated results.
for case in truncate badjson missing-field remove; do
  D="$WORK/results-$case"
  make_sandbox "$D"
  R="$D/docs/sample-output/climb-results.json"
  case "$case" in
    truncate) head -c 200 "$R" > "$R.tmp" && mv "$R.tmp" "$R" ;;
    badjson) printf '{not json' > "$R" ;;
    missing-field)
      python3 - "$R" <<'PY'
import json, sys
path = sys.argv[1]
doc = json.load(open(path))
doc.pop("version", None)
json.dump(doc, open(path, "w"))
PY
      ;;
    remove) rm -f "$R" ;;
  esac
  run_expect_fail "results: $case" 'results: .*climb-results\.json|missing field|expected value' "$D" \
    "$TUI" --check --results "$R"
done

# 5-6. Wire report missing and truncated.
D="$WORK/wire-missing"
make_sandbox "$D"
rm -f "$D/target/wire-report.json"
run_expect_fail "wire report: removed" 'required input missing: target/wire-report\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

D="$WORK/wire-truncated"
make_sandbox "$D"
head -c 100 "$D/target/wire-report.json" > "$D/target/wire-report.json.tmp"
mv "$D/target/wire-report.json.tmp" "$D/target/wire-report.json"
run_expect_fail "wire report: truncated" 'failed to decode JSON in target/wire-report\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 7-8. Memory report corrupt / removed.
D="$WORK/memory-corrupt"
make_sandbox "$D"
printf '{"method": "not-the-right-shape"' > "$D/target/memory-report.json"
run_expect_fail "memory report: corrupt" 'malformed data in target/memory-report\.json|failed to decode JSON in target/memory-report\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

D="$WORK/memory-missing"
make_sandbox "$D"
rm -f "$D/target/memory-report.json"
run_expect_fail "memory report: removed" 'required input missing: target/memory-report\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 9-10. Ladder fixture corrupt / removed.
D="$WORK/ladder-corrupt"
make_sandbox "$D"
printf '{not json' > "$D/data/ladder-verification.recorded.json"
run_expect_fail "ladder fixture: corrupt" 'failed to decode JSON in data/ladder-verification\.recorded\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

D="$WORK/ladder-missing"
make_sandbox "$D"
rm -f "$D/data/ladder-verification.recorded.json"
run_expect_fail "ladder fixture: removed" 'required input missing: .*ladder-verification' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 11-12. DNS capture corrupt / removed.
D="$WORK/capture-corrupt"
make_sandbox "$D"
printf '{not json' > "$D/data/dns-capture.recorded.json"
run_expect_fail "dns capture: corrupt" 'failed to decode JSON in data/dns-capture\.recorded\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

D="$WORK/capture-missing"
make_sandbox "$D"
rm -f "$D/data/dns-capture.recorded.json"
run_expect_fail "dns capture: removed" 'required input missing: .*dns-capture' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 13-14. Environment removed / corrupt.
D="$WORK/environment-missing"
make_sandbox "$D"
rm -f "$D/environment.json"
run_expect_fail "environment: removed" 'required input missing: environment\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

D="$WORK/environment-corrupt"
make_sandbox "$D"
printf '{not json' > "$D/environment.json"
run_expect_fail "environment: corrupt" 'failed to decode JSON in environment\.json' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 15. Criterion directory removed.
D="$WORK/criterion-missing"
make_sandbox "$D"
rm -rf "$D/target/criterion"
run_expect_fail "criterion dir: removed" 'required input missing: target/criterion' "$D" \
  "$XTASK" reproduce --quick --skip-dns

# 16. Docker unavailable (empty PATH so the CLI cannot be found).
D="$WORK/docker-missing"
make_sandbox "$D"
mkdir -p "$WORK/empty-path"
run_expect_fail "docker: unavailable" 'docker is unavailable; .*--skip-dns' "$D" \
  env PATH="$WORK/empty-path" "$XTASK" reproduce --quick --remeasure

# 17. Network unavailable inside a network namespace with a TCP Docker endpoint.
D="$WORK/network-off"
make_sandbox "$D"
run_expect_fail "network: unshare -rn" 'docker is unavailable; .*--skip-dns' "$D" \
  unshare -rn env DOCKER_HOST=tcp://127.0.0.1:2375 PATH="$PATH" "$XTASK" reproduce --quick --remeasure

# 18. Read-only filesystem: outputs cannot be written.
D="$WORK/read-only"
make_sandbox "$D"
chmod -R a-w "$D"
run_expect_fail "filesystem: read-only" 'Permission denied|Read-only file system' "$D" \
  "$XTASK" reproduce --quick --skip-dns
chmod -R u+w "$D"

echo
printf 'failure matrix: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ $FAIL -eq 0 ]]
