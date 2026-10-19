#!/bin/bash
set -euo pipefail

REPRO_OUT=/var/tmp/climb-clean-finaltest
OUT=docs/sample-output

mkdir -p "$OUT"

cp target/climb-report/report.html "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/report.html" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/signing_time.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/signing_time.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/verifying_time.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/verifying_time.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/transmitted_bytes.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/transmitted_bytes.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/amortization_curve.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/amortization_curve.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/memory_measured_vs_analytical.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/memory_measured_vs_analytical.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/reuse_curve.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/reuse_curve.svg" "$OUT/" 2>/dev/null || true
cp target/climb-report/charts/udp_fit.svg "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-report/charts/udp_fit.svg" "$OUT/" 2>/dev/null || true
cp target/climb-results.json "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-results.json" "$OUT/" 2>/dev/null || true
cp target/climb-classical-signatures.json "$OUT/" 2>/dev/null || cp "$REPRO_OUT/target/climb-classical-signatures.json" "$OUT/" 2>/dev/null || true
cp data/dns-capture.recorded.json "$OUT/" 2>/dev/null || cp "$REPRO_OUT/data/dns-capture.recorded.json" "$OUT/" 2>/dev/null || true
cp data/ladder-verification.recorded.json "$OUT/" 2>/dev/null || cp "$REPRO_OUT/data/ladder-verification.recorded.json" "$OUT/" 2>/dev/null || true

cd "$OUT"
sha256sum report.html signing_time.svg verifying_time.svg transmitted_bytes.svg amortization_curve.svg memory_measured_vs_analytical.svg reuse_curve.svg udp_fit.svg climb-results.json climb-classical-signatures.json dns-capture.recorded.json ladder-verification.recorded.json > SHA256SUMS 2>&1 || true

COMMIT=$(git rev-parse HEAD)
RUSTC=$(rustc --version)
SUBMTL=$(git submodule status | awk 'NR==1{print $1}' | tr -d '+-')
SUBOQS=$(git submodule status | awk 'NR==2{print $1}' | tr -d '+-')
DATE=$(date -Iseconds)

cat > PROVENANCE.txt <<EOF
commit SHA: $COMMIT
rustc: $RUSTC
submodule SHAs: third_party/MTL=$SUBMTL, third_party/liboqs=$SUBOQS
date: $DATE
EOF
echo "done"
