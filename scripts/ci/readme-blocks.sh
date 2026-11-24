#!/usr/bin/env bash
# Check every generated README block against a fresh generation.
#
# The badge block, the one-line live link and the Live demo section are all
# written by `cargo xtask readme-badges`; `--check` regenerates them and fails
# when the committed copy differs, naming the marker pair and the fix command.
# It also checks that the workflow, the script default and the two deployment
# documents carry the same live address as the single configuration value.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"
cargo run -p xtask --no-default-features -- readme-badges --check
