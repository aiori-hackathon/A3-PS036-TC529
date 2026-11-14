#!/usr/bin/env bash
#
# Refresh the DuckDNS A record for this host. Installed as
# /usr/local/sbin/climb-duckdns-update and run by climb-duckdns.timer.
#
# The token is read from /etc/climb/duckdns.env (mode 0600, root only). It is
# never passed on a command line: the request URL is fed to curl through a
# configuration on stdin, so a process listing (ps) never shows the token. Any
# error text is redacted before it is written to the journal.

set -euo pipefail

env_file="${CLIMB_DUCKDNS_ENV:-/etc/climb/duckdns.env}"

say() { printf 'duckdns: %s\n' "$*"; }
bad() {
  printf 'duckdns: %s\n' "$*" >&2
  exit 1
}

read_value() {
  local key="$1"
  [ -r "$env_file" ] || return 0
  sed -n "s/^${key}=//p" "$env_file" | head -n1
}

[ -r "$env_file" ] || bad "cannot read $env_file"

domain="$(read_value DUCKDNS_DOMAIN)"
token="$(read_value DUCKDNS_TOKEN)"

if [ -z "$domain" ] && [ -z "$token" ]; then
  say "no domain or token configured in $env_file; nothing to do"
  exit 0
fi
if [ -z "$domain" ]; then
  bad "DUCKDNS_DOMAIN is empty in $env_file"
fi
if [ -z "$token" ]; then
  bad "DUCKDNS_TOKEN is empty in $env_file"
fi

printf '%s' "$domain" | grep -Eq '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$' ||
  bad "DUCKDNS_DOMAIN is not a single DNS label"
printf '%s' "$token" | grep -Eq '^[A-Za-z0-9-]{8,64}$' ||
  bad "DUCKDNS_TOKEN does not have the expected shape"

url="https://www.duckdns.org/update?domains=${domain}&token=${token}&ip="

# umask 077 keeps the error file private while it exists.
umask 077
err_file="$(mktemp "${TMPDIR:-/tmp}/climb-duckdns.XXXXXX")"
cleanup() { rm -f "$err_file"; }
trap cleanup EXIT

body=""
if ! body="$(
  printf 'url = "%s"\n' "$url" |
    curl --silent --show-error --fail \
      --proto '=https' --tlsv1.2 \
      --connect-timeout 10 --max-time 20 \
      --config - 2>"$err_file"
)"; then
  detail="$(tr -d '\r\n' <"$err_file")"
  detail="${detail//$token/***}"
  [ -n "$detail" ] || detail="curl exited without a message"
  printf 'duckdns: update for %s.duckdns.org failed: %s\n' "$domain" "$detail" >&2
  exit 1
fi

answer="$(printf '%s' "$body" | tr -d '\r\n')"
case "$answer" in
  OK*)
    say "record for ${domain}.duckdns.org is current"
    ;;
  KO*)
    bad "DuckDNS rejected the update for ${domain}.duckdns.org (check the token)"
    ;;
  *)
    bad "unexpected answer from DuckDNS for ${domain}.duckdns.org (no token or host in the message)"
    ;;
esac
