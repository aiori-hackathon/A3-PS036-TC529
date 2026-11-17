#!/usr/bin/env bash
# Install the system packages documented in docs/BUILD-DEPS.md.
#
# The list is read out of the document instead of being repeated here, so the
# README's prerequisites, this step and the explanatory table cannot drift
# apart. A runner executes this step as a user with sudo; the rehearsal
# container executes it as root.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/env.sh
source "$repo_root/scripts/ci/env.sh"
cd "$repo_root"

document="docs/BUILD-DEPS.md"
packages=()
in_block=0
while IFS= read -r line; do
  line="${line%$'\r'}"
  case "$line" in
    '<!-- build-deps:begin -->')
      in_block=1
      continue
      ;;
    '<!-- build-deps:end -->')
      in_block=0
      continue
      ;;
  esac
  [ "$in_block" -eq 1 ] || continue
  trimmed="${line#"${line%%[![:space:]]*}"}"
  trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
  case "$trimmed" in
    '' | '```'*) continue ;;
  esac
  packages+=("$trimmed")
done <"$document"

if [ "${#packages[@]}" -eq 0 ]; then
  printf 'error: no packages between the build-deps markers in %s\n' "$document" >&2
  exit 1
fi

sudo=()
if [ "$(id -u)" -ne 0 ]; then
  sudo=(sudo)
fi

printf '==> Installing %s documented package(s) from %s\n' "${#packages[@]}" "$document"
DEBIAN_FRONTEND=noninteractive "${sudo[@]}" apt-get update
DEBIAN_FRONTEND=noninteractive "${sudo[@]}" apt-get install -y --no-install-recommends \
  "${packages[@]}"

printf '==> Installed versions:\n'
for package in "${packages[@]}"; do
  dpkg-query -W "$package"
done
printf '==> pkg-config fontconfig: %s\n' "$(pkg-config --modversion fontconfig)"
