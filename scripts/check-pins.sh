#!/bin/bash
set -euo pipefail

# Verify every supply-chain pin that a build depends on:
#
#   * every FROM line in every docker/Dockerfile* names an image by digest;
#   * every direct apt package in docker/Dockerfile.build carries an explicit
#     `=version`;
#   * the libMTL/liboqs submodule commits agree with the toolchain Dockerfile
#     comment and the generated environment fingerprint.
#
# The rules live in `cargo xtask check-pins` so they are unit-tested; this
# script is the stable entry point used by CI and by a human.

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

exec cargo xtask check-pins "$@"
