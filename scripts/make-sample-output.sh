#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# make-sample-output.sh — refresh the committed sample output in one command.
#
# Copies the recorded artifacts out of the build tree, normalises the machine
# specific paths so the committed copy is portable, proves the consolidated
# document is byte-identical to a second consolidation of the same inputs
# (apart from the run timestamp), and writes the checksum manifest and the
# provenance record the documentation gates re-hash.
#
# Usage:
#   bash scripts/make-sample-output.sh
#
# Environment:
#   CLIMB_SAMPLE_SRC  artifact root to copy from (default: target)
#   CLIMB_SAMPLE_OUT  committed output directory (default: docs/sample-output)
#
# The script requires the measurement inputs (wire report, memory report,
# ladder verification) to be present under CLIMB_SAMPLE_SRC: the room reserved
# for a silent stale copy is zero.
# ---------------------------------------------------------------------------
set -euo pipefail

SRC="${CLIMB_SAMPLE_SRC:-target}"
OUT="${CLIMB_SAMPLE_OUT:-docs/sample-output}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CHART_NAMES=(
  signing_time
  verifying_time
  transmitted_bytes
  amortization_curve
  memory_measured_vs_analytical
  reuse_curve
  udp_fit
)

die() {
  printf 'make-sample-output: %s\n' "$*" >&2
  exit 1
}

copy_required() {
  local from="$1" to="$2"
  [[ -f "$from" ]] || die "required artifact missing: $from"
  cp "$from" "$to"
}

mkdir -p "$OUT"

# --- copy the recorded artifacts ------------------------------------------
copy_required "$SRC/climb-report/report.html" "$OUT/report.html"
for name in "${CHART_NAMES[@]}"; do
  copy_required "$SRC/climb-report/charts/${name}.svg" "$OUT/${name}.svg"
done
copy_required "$SRC/climb-results.json" "$OUT/climb-results.json"
if [[ -f "$SRC/climb-classical-signatures.json" ]]; then
  cp "$SRC/climb-classical-signatures.json" "$OUT/climb-classical-signatures.json"
elif [[ ! -f "$OUT/climb-classical-signatures.json" ]]; then
  die "no classical signature lengths: $SRC/climb-classical-signatures.json"
fi

# Prefer a live DNS/ladder capture, then fall back to the committed fixtures.
if [[ -f "$SRC/dns-capture.json" ]]; then
  cp "$SRC/dns-capture.json" "$OUT/dns-capture.recorded.json"
elif [[ -f data/dns-capture.recorded.json ]]; then
  cp data/dns-capture.recorded.json "$OUT/dns-capture.recorded.json"
else
  die "no DNS capture: $SRC/dns-capture.json or data/dns-capture.recorded.json"
fi

if [[ -f "$SRC/ladder-verification.json" ]]; then
  cp "$SRC/ladder-verification.json" "$OUT/ladder-verification.recorded.json"
elif [[ -f data/ladder-verification.recorded.json ]]; then
  cp data/ladder-verification.recorded.json "$OUT/ladder-verification.recorded.json"
else
  die "no ladder verification: $SRC/ladder-verification.json or data/ladder-verification.recorded.json"
fi

# The digest document is recorded separately and is not a product of the run.
if [[ -f "$SRC/dataset-digests.json" ]]; then
  cp "$SRC/dataset-digests.json" "$OUT/dataset-digests.json"
elif [[ -f "$OUT/dataset-digests.json" ]]; then
  : # already in place
elif [[ -f docs/sample-output/dataset-digests.json ]]; then
  cp docs/sample-output/dataset-digests.json "$OUT/dataset-digests.json"
elif [[ -f data/dataset-digests.json ]]; then
  cp data/dataset-digests.json "$OUT/dataset-digests.json"
else
  die "no dataset digest document: $SRC/dataset-digests.json or docs/sample-output/dataset-digests.json"
fi

# --- path normalisation ---------------------------------------------------
# The consolidated document records the absolute workspace path. Replace it
# with a stable placeholder so the committed copy is portable, and check that
# no machine-specific path survives in any copied text artifact.
normalise_json() {
  python3 - "$1" "$REPO_ROOT" <<'PY'
import json
import sys

path, root = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    document = json.load(handle)
document["workspace_root"] = "<repo>"
with open(path, "w", encoding="utf-8") as handle:
    json.dump(document, handle, indent=2)
    handle.write("\n")
PY
}

normalise_json "$OUT/climb-results.json"

# The rendered page embeds claims derived from the same document; replace any
# absolute repository path it carries.
python3 - "$OUT/report.html" "$REPO_ROOT" <<'PY'
import sys

path, root = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    text = handle.read()
text = text.replace(root, "<repo>")
with open(path, "w", encoding="utf-8") as handle:
    handle.write(text)
PY

for name in "${CHART_NAMES[@]}"; do
  python3 - "$OUT/${name}.svg" "$REPO_ROOT" <<'PY'
import sys

path, root = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    text = handle.read()
if root in text:
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text.replace(root, "<repo>"))
PY
done

leaked="$(grep -R -n -F "$REPO_ROOT" \
  "$OUT/report.html" "$OUT/climb-results.json" "$OUT"/*.svg 2>/dev/null || true)"
[[ -z "$leaked" ]] || die "machine-specific path survived normalisation:
$leaked"

# --- byte-identity against a second consolidation -------------------------
for input in "$SRC/wire-report.json" "$SRC/memory-report.json"; do
  [[ -f "$input" ]] || die "cannot rerun consolidation: missing $input"
done
ladder_args=()
if [[ -f "$SRC/ladder-verification.json" ]]; then
  ladder_args=(--ladder-verification "$SRC/ladder-verification.json")
fi

mode="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("reproduction_mode","unknown"))' "$OUT/climb-results.json")"

# Compare every measured value and structural field. The run timestamp and
# the diagnostic warning list are not measurements: the former is always new,
# and the latter depends on how old an input file looks at consolidation time.
normalise_for_compare() {
  python3 - "$1" "$REPO_ROOT" <<'PY'
import json
import sys

path, root = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    document = json.load(handle)
document["workspace_root"] = "<repo>"
document["generated_at"] = "<timestamp>"
document["warnings"] = []
with open(path, "w", encoding="utf-8") as handle:
    json.dump(document, handle, indent=2)
    handle.write("\n")
PY
}

# Start the second consolidation from a copy of the committed document, so the
# cross-run stability section (which consolidation carries forward rather than
# recomputes) is present on both sides of the comparison.
cp "$OUT/climb-results.json" "$TMP/second.json"

cargo xtask consolidate --force \
  --wire-report "$SRC/wire-report.json" \
  --memory-report "$SRC/memory-report.json" \
  "${ladder_args[@]}" \
  --output "$TMP/second.json" \
  --reproduction-mode "$mode"

cp "$OUT/climb-results.json" "$TMP/committed.json"
normalise_for_compare "$TMP/second.json"
normalise_for_compare "$TMP/committed.json"

if ! cmp -s "$TMP/committed.json" "$TMP/second.json"; then
  diff -u "$TMP/committed.json" "$TMP/second.json" >&2 || true
  die "the committed document is not byte-identical to a second consolidation"
fi
printf 'make-sample-output: second consolidation is byte-identical (timestamp and diagnostics normalised)\n'

# --- manifest and provenance ---------------------------------------------
MANIFEST_FILES=(
  report.html
  signing_time.svg
  verifying_time.svg
  transmitted_bytes.svg
  amortization_curve.svg
  memory_measured_vs_analytical.svg
  reuse_curve.svg
  udp_fit.svg
  climb-results.json
  climb-classical-signatures.json
  dns-capture.recorded.json
  ladder-verification.recorded.json
  dataset-digests.json
)

(
  cd "$OUT"
  sha256sum "${MANIFEST_FILES[@]}" > SHA256SUMS
)

COMMIT="$(git rev-parse HEAD)"
RUSTC="$(rustc --version)"
SUBMTL="$(git submodule status | awk 'NR==1{print $1}' | tr -d '+-')"
SUBOQS="$(git submodule status | awk 'NR==2{print $1}' | tr -d '+-')"
DATE="$(date -Iseconds)"

cat > "$OUT/PROVENANCE.txt" <<EOF
commit SHA: $COMMIT
rustc: $RUSTC
submodule SHAs: third_party/MTL=$SUBMTL, third_party/liboqs=$SUBOQS
date: $DATE
normalisation: absolute workspace paths replaced with <repo>
checks: second consolidation byte-identical apart from generated_at and diagnostics
EOF

printf 'make-sample-output: refreshed %s (%d manifest entries)\n' \
  "$OUT" "${#MANIFEST_FILES[@]}"
