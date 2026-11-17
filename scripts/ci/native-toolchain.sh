#!/usr/bin/env bash
# Build and extract the pinned native toolchain from its Docker recipe.
#
# `CLIMB_DOCKER_NETWORK` is passed to `docker build` when it is set. A machine
# that cannot create Docker bridge networks sets it to `host`; continuous
# integration leaves it unset and builds on the default bridge.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
network=()
if [ -n "${CLIMB_DOCKER_NETWORK:-}" ]; then
  network=("--network=${CLIMB_DOCKER_NETWORK}")
fi
docker build "${network[@]}" -f docker/Dockerfile.build -t climb-builder:phase1 .
./docker/extract-libs.sh climb-builder:phase1
