#!/usr/bin/env bash
# GO/NO-GO table for a demonstration. Prints one row per check and the exact
# fix for every NO-GO, then exits non-zero when a required check failed.
# Nothing here touches the network.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for candidate in target/release/xtask target/debug/xtask; do
    if [[ -x "$candidate" ]]; then
        exec "$candidate" demo --preflight-only "$@"
    fi
done

exec cargo run -q -p xtask -- demo --preflight-only "$@"
