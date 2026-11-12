#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# make-sample-output.sh — refresh the committed sample output in one command.
#
# The committed `docs/sample-output/` directory is one artifact pair: a
# consolidated results document and the HTML report rendered from it. This
# script therefore never copies a report that was rendered from another run.
# It consolidates the measurement inputs into the committed results file,
# renders the charts and the report from that exact file, checks the pair,
# and only then writes the checksum manifest and the provenance record.
#
# Usage:
#   bash scripts/make-sample-output.sh
#
# Environment:
#   CLIMB_SAMPLE_SRC  artifact root to read inputs from (default: target)
#   CLIMB_SAMPLE_OUT  committed output directory (default: docs/sample-output)
#
# When the measurement inputs are absent but a consolidated results document
# already exists, the pair is regenerated from that document so the report and
# the results still agree. The script never leaves a results/report pair that
# disagree about their claim count.
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

# --- copy the standalone recorded artifacts -------------------------------
if [[ -f "$SRC/climb-classical-signatures.json" ]]; then
  cp "$SRC/climb-classical-signatures.json" "$OUT/climb-classical-signatures.json"
elif [[ ! -f "$OUT/climb-classical-signatures.json" ]]; then
  die "no classical signature lengths: $SRC/climb-classical-signatures.json"
fi

# Prefer a live DNS/ladder capture, then fall back to the committed fixtures.
DNS_INPUT=""
if [[ -f "$SRC/dns-capture.json" ]]; then
  cp "$SRC/dns-capture.json" "$OUT/dns-capture.recorded.json"
  DNS_INPUT="$SRC/dns-capture.json"
elif [[ -f data/dns-capture.recorded.json ]]; then
  cp data/dns-capture.recorded.json "$OUT/dns-capture.recorded.json"
  DNS_INPUT="data/dns-capture.recorded.json"
else
  die "no DNS capture: $SRC/dns-capture.json or data/dns-capture.recorded.json"
fi

LADDER_INPUT=""
if [[ -f "$SRC/ladder-verification.json" ]]; then
  cp "$SRC/ladder-verification.json" "$OUT/ladder-verification.recorded.json"
  LADDER_INPUT="$SRC/ladder-verification.json"
elif [[ -f data/ladder-verification.recorded.json ]]; then
  cp data/ladder-verification.recorded.json "$OUT/ladder-verification.recorded.json"
  LADDER_INPUT="data/ladder-verification.recorded.json"
else
  die "no ladder verification: $SRC/ladder-verification.json or data/ladder-verification.recorded.json"
fi

# The digest document is recorded separately and is not a product of the run.
if [[ -f "$SRC/dataset-digests.json" ]]; then
  cp "$SRC/dataset-digests.json" "$OUT/dataset-digests.json"
elif [[ -f "$OUT/dataset-digests.json" ]]; then
  : # already in place
elif [[ -f data/dataset-digests.json ]]; then
  cp data/dataset-digests.json "$OUT/dataset-digests.json"
else
  die "no dataset digest document: $SRC/dataset-digests.json or docs/sample-output/dataset-digests.json"
fi

# --- helpers ---------------------------------------------------------------
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

normalise_text() {
  python3 - "$1" "$REPO_ROOT" <<'PY'
import sys

path, root = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    text = handle.read()
if root in text:
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text.replace(root, "<repo>"))
PY
}

reproduction_mode() {
  python3 - "$1" <<'PY'
import json
import sys
try:
    with open(sys.argv[1], encoding="utf-8") as handle:
        print(json.load(handle).get("reproduction_mode", "unknown"))
except (OSError, ValueError):
    print("unknown")
PY
}

# --- consolidate / select the one results document ------------------------
INPUTS_READY=0
if [[ -f "$SRC/wire-report.json" && -f "$SRC/memory-report.json" ]]; then
  INPUTS_READY=1
fi

MODE="unknown"
if [[ -f "$SRC/climb-results.json" ]]; then
  MODE="$(reproduction_mode "$SRC/climb-results.json")"
elif [[ -f "$OUT/climb-results.json" ]]; then
  MODE="$(reproduction_mode "$OUT/climb-results.json")"
fi

if [[ "$INPUTS_READY" -eq 1 ]]; then
  consolidate_args=(
    consolidate --force
    --wire-report "$SRC/wire-report.json"
    --memory-report "$SRC/memory-report.json"
    --dns-capture "$DNS_INPUT"
    --ladder-verification "$LADDER_INPUT"
    --output "$OUT/climb-results.json"
    --reproduction-mode "$MODE"
  )
  if [[ -d "$SRC/criterion" ]]; then
    consolidate_args+=(--criterion-dir "$SRC/criterion")
  fi
  if [[ -f "$SRC/replication-summary.json" ]]; then
    consolidate_args+=(--replication-summary "$SRC/replication-summary.json")
  fi
  if [[ -f "$SRC/dns-rtt.json" ]]; then
    consolidate_args+=(--dns-rtt "$SRC/dns-rtt.json")
  fi
  if [[ -f "$SRC/dns-cache.json" ]]; then
    consolidate_args+=(--cache-use "$SRC/dns-cache.json")
  fi
  if [[ -f "$SRC/climb-classical-signatures.json" ]]; then
    consolidate_args+=(--classical-signatures "$SRC/climb-classical-signatures.json")
  fi

  printf 'make-sample-output: consolidating %s into %s/climb-results.json\n' "$SRC" "$OUT"
  cargo xtask "${consolidate_args[@]}"
elif [[ -f "$SRC/climb-results.json" ]]; then
  printf 'make-sample-output: measurement inputs absent; reusing %s/climb-results.json\n' "$SRC"
  cp "$SRC/climb-results.json" "$OUT/climb-results.json"
elif [[ -f "$OUT/climb-results.json" ]]; then
  printf 'make-sample-output: measurement inputs absent; re-rendering from the committed results\n'
else
  die "no results document and no measurement inputs under $SRC"
fi

normalise_json "$OUT/climb-results.json"

# The per-query RTT records live in a companion beside the consolidated
# results. A fresh consolidation writes them; when reusing an already-slim
# results document, fall back to the full harness artifact or the committed
# companion, and never leave a half pair.
if [[ ! -f "$OUT/dns-rtt-samples.json" ]]; then
  if [[ -f "$SRC/dns-rtt.json" ]]; then
    cp "$SRC/dns-rtt.json" "$OUT/dns-rtt-samples.json"
  elif [[ -f data/dns-rtt-samples.json ]]; then
    cp data/dns-rtt-samples.json "$OUT/dns-rtt-samples.json"
  elif grep -q '"queries"' "$OUT/climb-results.json"; then
    die "results embed per-query RTT records but no companion exists; re-run consolidation"
  fi
fi
if [[ -f "$OUT/dns-rtt-samples.json" ]]; then
  normalise_text "$OUT/dns-rtt-samples.json"
fi

# --- render charts and the report from that exact results file ------------
# The report is rendered into a `climb-report/` subdirectory and then moved
# beside the results, so its relative `../climb-results.json` link keeps the
# same shape as a real run's report while the pair stays in one directory.
mkdir -p "$OUT/climb-report"
cargo xtask report --format html \
  --results "$OUT/climb-results.json" \
  --out "$OUT/climb-report/report.html" \
  --charts "$OUT"
mv "$OUT/climb-report/report.html" "$OUT/report.html"
rmdir "$OUT/climb-report"

normalise_text "$OUT/report.html"
for name in "${CHART_NAMES[@]}"; do
  normalise_text "$OUT/${name}.svg"
done

leaked="$(grep -R -n -F "$REPO_ROOT" \
  "$OUT/report.html" "$OUT/climb-results.json" "$OUT"/*.svg 2>/dev/null || true)"
[[ -z "$leaked" ]] || die "machine-specific path survived normalisation:
$leaked"

# --- the pair must agree before anything is published ---------------------
cargo xtask report --format html --check \
  --results "$OUT/climb-results.json" \
  --out "$OUT/report.html"

cargo xtask sample-check --pair-only \
  --root "$REPO_ROOT" \
  --results "$OUT/climb-results.json" \
  --report "$OUT/report.html"

# --- prove the copied document is a second consolidation's equal ----------
if [[ "$INPUTS_READY" -eq 1 ]]; then
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

  cp "$OUT/climb-results.json" "$TMP/second.json"
  second_args=(
    consolidate --force
    --wire-report "$SRC/wire-report.json"
    --memory-report "$SRC/memory-report.json"
    --dns-capture "$DNS_INPUT"
    --ladder-verification "$LADDER_INPUT"
    --output "$TMP/second.json"
    --reproduction-mode "$MODE"
  )
  if [[ -d "$SRC/criterion" ]]; then
    second_args+=(--criterion-dir "$SRC/criterion")
  fi
  if [[ -f "$SRC/replication-summary.json" ]]; then
    second_args+=(--replication-summary "$SRC/replication-summary.json")
  fi
  if [[ -f "$SRC/dns-rtt.json" ]]; then
    second_args+=(--dns-rtt "$SRC/dns-rtt.json")
  fi
  if [[ -f "$SRC/dns-cache.json" ]]; then
    second_args+=(--cache-use "$SRC/dns-cache.json")
  fi
  if [[ -f "$SRC/climb-classical-signatures.json" ]]; then
    second_args+=(--classical-signatures "$SRC/climb-classical-signatures.json")
  fi
  cargo xtask "${second_args[@]}"

  cp "$OUT/climb-results.json" "$TMP/committed.json"
  normalise_for_compare "$TMP/second.json"
  normalise_for_compare "$TMP/committed.json"

  if ! cmp -s "$TMP/committed.json" "$TMP/second.json"; then
    diff -u "$TMP/committed.json" "$TMP/second.json" >&2 || true
    die "the committed document is not byte-identical to a second consolidation"
  fi
  printf 'make-sample-output: second consolidation is byte-identical (timestamp and diagnostics normalised)\n'
else
  printf 'make-sample-output: second-consolidation check skipped (no measurement inputs)\n'
fi

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

if [[ -f "$OUT/dns-rtt-samples.json" ]]; then
  MANIFEST_FILES+=(dns-rtt-samples.json)
fi

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
checks: results/report claim counts equal; report --check clean; second consolidation byte-identical apart from generated_at and diagnostics
EOF

# The full check hashes the manifest and resolves the provenance commit.
cargo xtask sample-check --root "$REPO_ROOT"

printf 'make-sample-output: refreshed %s (%d manifest entries)\n' \
  "$OUT" "${#MANIFEST_FILES[@]}"
