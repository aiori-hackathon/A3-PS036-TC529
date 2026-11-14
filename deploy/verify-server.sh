#!/usr/bin/env bash
#
# Check a prepared dashboard host and print PASS or FAIL for each item:
# the OS release, ufw, fail2ban, Caddy and the TLS certificate. The
# virtualisation type and every warning the bootstrap recorded are printed too.
#
# Run as root after deploy/bootstrap.sh: the DuckDNS environment file that names
# the host is root-only.
#
# Exit status: 0 when no item failed, 1 when at least one did.

set -uo pipefail

root_prefix="${CLIMB_ROOT:-}"
etc="${root_prefix}/etc"
cgroup_root="${CLIMB_CGROUP_ROOT:-/sys/fs/cgroup}"
warnings_file="${root_prefix}/var/lib/climb/bootstrap-warnings.txt"
os_release_file="${etc}/os-release"

passed=0
failed=0
warned=0

pass() {
  printf 'PASS  %-13s %s\n' "$1" "$2"
  passed=$((passed + 1))
}
fail() {
  printf 'FAIL  %-13s %s\n' "$1" "$2"
  failed=$((failed + 1))
}
note() { printf 'INFO  %-13s %s\n' "$1" "$2"; }
warn() {
  printf 'WARN  %-13s %s\n' "$1" "$2"
  warned=$((warned + 1))
}

require_root() {
  if [ "$(id -u)" -eq 0 ] || [ -n "$root_prefix" ]; then
    return 0
  fi
  printf 'verify-server: run as root (sudo bash %s)\n' "$0" >&2
  exit 2
}

read_env_value() {
  local file="$1" key="$2"
  [ -r "$file" ] || return 0
  sed -n "s/^${key}=//p" "$file" | head -n1
}

os_pretty="unknown"
os_id=""
os_version=""

check_os() {
  if [ ! -r "$os_release_file" ]; then
    fail "os-release" "cannot read $os_release_file"
    return
  fi
  # A fixed, root-owned file in a documented format.
  # shellcheck disable=SC1090
  . "$os_release_file"
  os_id="${ID:-unknown}"
  os_version="${VERSION_ID:-unknown}"
  os_pretty="${PRETTY_NAME:-unknown}"
  if [ "$os_id" = "ubuntu" ] && [ "$os_version" = "24.04" ]; then
    pass "os-release" "$os_pretty"
  else
    fail "os-release" "expected Ubuntu 24.04, found $os_id $os_version"
  fi
}

check_virt() {
  local detected
  if command -v systemd-detect-virt >/dev/null 2>&1; then
    detected="$(systemd-detect-virt 2>/dev/null || true)"
  else
    detected="unknown (systemd-detect-virt is not installed)"
  fi
  [ -n "$detected" ] || detected="none"
  note "virtualisation" "$detected"
  case "$detected" in
    lxc | lxc-libvirt)
      note "branch" "container: no swap file, no time sync, no sysctl changes"
      ;;
    *)
      note "branch" "host kernel: swap, time sync and sysctl are configured"
      ;;
  esac
}

check_firewall() {
  if ! command -v ufw >/dev/null 2>&1; then
    fail "ufw" "ufw is not installed"
    return
  fi
  local status
  status="$(ufw status 2>&1 || true)"
  if printf '%s\n' "$status" | grep -q '^Status: active'; then
    if printf '%s\n' "$status" | grep -q '^22/tcp *LIMIT'; then
      pass "ufw" "active; 22 rate-limited, 80 and 443 allowed"
    else
      pass "ufw" "active (the rate-limited 22 rule was not visible in ufw status)"
    fi
  else
    fail "ufw" "inactive; use the provider firewall panel in a container without netfilter"
  fi
}

check_fail2ban() {
  if ! command -v fail2ban-client >/dev/null 2>&1; then
    fail "fail2ban" "fail2ban-client is not installed"
    return
  fi
  if ! systemctl is-active --quiet fail2ban 2>/dev/null; then
    fail "fail2ban" "the service is not active"
    return
  fi
  if fail2ban-client status sshd >/dev/null 2>&1; then
    pass "fail2ban" "active; the sshd jail is loaded"
  else
    fail "fail2ban" "active, but the sshd jail did not answer"
  fi
}

check_caddy() {
  local config="${etc}/caddy/Caddyfile"
  if [ ! -r "$config" ]; then
    fail "caddy" "no configuration at $config"
    return
  fi
  if ! systemctl is-active --quiet caddy 2>/dev/null; then
    fail "caddy" "the service is not active"
    return
  fi
  if command -v caddy >/dev/null 2>&1; then
    if ! caddy validate --config "$config" --adapter caddyfile >/dev/null 2>&1; then
      fail "caddy" "active, but $config does not validate"
      return
    fi
  fi
  pass "caddy" "active and the configuration validates"
}

check_certificate() {
  local domain fqdn code
  domain="$(read_env_value "${etc}/climb/duckdns.env" DUCKDNS_DOMAIN)"
  if [ -z "$domain" ]; then
    fail "certificate" "no DuckDNS name in ${etc}/climb/duckdns.env"
    return
  fi
  fqdn="${domain}.duckdns.org"

  if ! command -v openssl >/dev/null 2>&1; then
    fail "certificate" "openssl is not installed"
    return
  fi

  local certificate
  certificate="$(printf '' | openssl s_client -connect 127.0.0.1:443 -servername "$fqdn" 2>/dev/null || true)"
  if [ -z "$certificate" ]; then
    fail "certificate" "nothing answered TLS on 127.0.0.1:443 for $fqdn"
    return
  fi

  local summary
  summary="$(printf '%s\n' "$certificate" | openssl x509 -noout -issuer -enddate 2>/dev/null | tr '\n' ' ' || true)"
  if ! printf '%s\n' "$certificate" | openssl x509 -noout -checkend 604800 >/dev/null 2>&1; then
    fail "certificate" "$fqdn has no certificate valid for another seven days"
    return
  fi

  if command -v curl >/dev/null 2>&1; then
    code="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      --resolve "${fqdn}:443:127.0.0.1" --max-time 15 "https://${fqdn}/" 2>/dev/null || true)"
    if [ "$code" != "200" ]; then
      fail "certificate" "TLS is valid but https://$fqdn/ answered $code"
      return
    fi
  fi
  pass "certificate" "$fqdn; ${summary:-no details}"
}

check_listeners() {
  command -v ss >/dev/null 2>&1 || return 0
  local unexpected="" address host port external
  external="$(ss -H -tulpn 2>/dev/null | awk '{print $5}' || true)"
  while IFS= read -r address; do
    [ -n "$address" ] || continue
    host="${address%:*}"
    port="${address##*:}"
    case "$host" in
      127.* | "[::1]" | ::1) continue ;;
    esac
    case "$port" in
      22 | 80 | 443) continue ;;
    esac
    unexpected="${unexpected}${address} "
  done <<<"$external"
  if [ -n "$unexpected" ]; then
    warn "listeners" "non-loopback ports beyond 22, 80 and 443: $unexpected"
  else
    note "listeners" "only 22, 80 and 443 on non-loopback addresses"
  fi
}

check_warnings() {
  if [ ! -e "$warnings_file" ]; then
    note "warnings" "no bootstrap warning was recorded"
    return 0
  fi
  local line
  while IFS= read -r line; do
    [ -n "$line" ] && warn "recorded" "$line"
  done <"$warnings_file"
}

check_limits() {
  local memory cpu
  memory="$(cat "${cgroup_root}/memory.max" 2>/dev/null || true)"
  cpu="$(cat "${cgroup_root}/cpu.max" 2>/dev/null || true)"
  [ -n "$memory" ] && note "memory limit" "$memory"
  [ -n "$cpu" ] && note "cpu limit" "$cpu"
}

main() {
  require_root
  printf 'CLIMB server verification\n'
  check_os
  check_virt
  check_firewall
  check_fail2ban
  check_caddy
  check_certificate
  check_listeners
  check_limits
  printf '\n'
  check_warnings
  printf '\n%d checks: %d passed, %d failed\n' "$((passed + failed))" "$passed" "$failed"
  [ "$failed" -eq 0 ] || exit 1
}

main "$@"
