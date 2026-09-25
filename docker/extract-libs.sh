#!/usr/bin/env bash
# docker/extract-libs.sh — extract the pinned libMTL/liboqs/OpenSSL artifacts
# built by docker/Dockerfile.build into <workspace>/.mtl-install.
#
# The FFI crate (climb-mtl-sys) resolves `<workspace>/.mtl-install` by default,
# so run this once after a fresh checkout to make `cargo build` work on the host
# without installing anything system-wide.
#
# Usage:
#   docker/extract-libs.sh [image-tag]
#
# Default image tag: climb-builder:phase1
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
IMAGE="${1:-climb-builder:phase1}"
DEST="${WORKSPACE_ROOT}/.mtl-install"

echo "==> Extracting ${IMAGE} -> ${DEST}"

if ! docker image inspect "${IMAGE}" >/dev/null 2>&1; then
    echo "error: Docker image '${IMAGE}' not found." >&2
    echo "       Build it first with:" >&2
    echo "       docker build -f docker/Dockerfile.build -t ${IMAGE} ." >&2
    exit 1
fi

container_id="$(docker create "${IMAGE}")"
trap 'docker rm -f "${container_id}" >/dev/null 2>&1 || true' EXIT

rm -rf "${DEST}"
mkdir -p "${DEST}"
docker cp "${container_id}:/usr/local/include" "${DEST}/include"
docker cp "${container_id}:/usr/local/lib" "${DEST}/lib"

echo "==> Done. Artifacts:"
ls -1 "${DEST}/include"
ls -1 "${DEST}/lib" | grep -E 'lib(mtlslib|oqs|crypto|ssl)' || true
