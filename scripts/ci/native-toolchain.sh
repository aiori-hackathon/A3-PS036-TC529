#!/usr/bin/env bash
# Build and extract the pinned native toolchain from its Docker recipe.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
docker build -f docker/Dockerfile.build -t climb-builder:phase1 .
./docker/extract-libs.sh climb-builder:phase1
