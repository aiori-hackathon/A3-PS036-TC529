#!/usr/bin/env bash
# The one definition of the continuous-integration environment.
#
# The workflow publishes these values into `GITHUB_ENV` before the first step
# runs, and every script under `scripts/ci/` sources this file, so a local run
# and a runner see the same zone, locale, lint flags, colour handling and git
# identity. Changing a value here changes both sides at once.
#
# Usage:
#   . scripts/ci/env.sh            export the variables in the current shell
#   scripts/ci/env.sh              print the KEY=VALUE lines
#   scripts/ci/env.sh --github-env append them to $GITHUB_ENV (workflow only)
#
# A variable that is already set is kept, because the pinned toolchain action
# and the local replica both set `RUSTFLAGS` from the workflow's `rustflags`
# input and that value must win.

# ci-setup-rust-toolchain-env: CARGO_INCREMENTAL CARGO_PROFILE_DEV_DEBUG CARGO_TERM_COLOR RUST_BACKTRACE RUSTFLAGS CARGO_UNSTABLE_SPARSE_REGISTRY CARGO_REGISTRIES_CRATES_IO_PROTOCOL
climb_ci_env() {
  printf '%s\n' \
    "TZ=${TZ:-UTC}" \
    "LANG=${LANG:-C.UTF-8}" \
    "LC_ALL=${LC_ALL:-C.UTF-8}" \
    "RUSTFLAGS=${RUSTFLAGS:--D warnings}" \
    "CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-0}" \
    "CARGO_PROFILE_DEV_DEBUG=${CARGO_PROFILE_DEV_DEBUG:-0}" \
    "CARGO_TERM_COLOR=${CARGO_TERM_COLOR:-always}" \
    "RUST_BACKTRACE=${RUST_BACKTRACE:-short}" \
    "CARGO_UNSTABLE_SPARSE_REGISTRY=${CARGO_UNSTABLE_SPARSE_REGISTRY:-true}" \
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL=${CARGO_REGISTRIES_CRATES_IO_PROTOCOL:-sparse}" \
    "GIT_AUTHOR_NAME=${GIT_AUTHOR_NAME:-CLIMB CI}" \
    "GIT_AUTHOR_EMAIL=${GIT_AUTHOR_EMAIL:-ci@example.invalid}" \
    "GIT_COMMITTER_NAME=${GIT_COMMITTER_NAME:-CLIMB CI}" \
    "GIT_COMMITTER_EMAIL=${GIT_COMMITTER_EMAIL:-ci@example.invalid}"
}

if [ "${BASH_SOURCE[0]}" != "$0" ]; then
  while IFS= read -r climb_ci_line; do
    export "$climb_ci_line"
  done < <(climb_ci_env)
  unset climb_ci_line
  return 0
fi

case "${1:-}" in
  "")
    climb_ci_env
    ;;
  --github-env)
    if [ -n "${GITHUB_ENV:-}" ]; then
      climb_ci_env >>"$GITHUB_ENV"
    else
      climb_ci_env
    fi
    ;;
  *)
    echo "usage: $0 [--github-env]" >&2
    exit 2
    ;;
esac
