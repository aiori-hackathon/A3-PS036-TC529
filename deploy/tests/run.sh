#!/usr/bin/env bash
#
# Exercise deploy/bootstrap.sh, deploy/duckdns-update.sh and
# deploy/verify-server.sh against a temporary root with stand-in commands.
#
# Nothing here touches the machine it runs on: every mutating command is a
# script in a temporary PATH, every path the bootstrap writes lives under a
# temporary CLIMB_ROOT, and the virtualisation type is supplied by a fake
# systemd-detect-virt. The kvm and lxc branches are both driven.
#
#   bash deploy/tests/run.sh
#
# Exit status is non-zero when any check fails.

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
deploy_dir="$(dirname "$here")"
work="$(mktemp -d "${TMPDIR:-/tmp}/climb-deploy-test.XXXXXX")"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT

fake="$work/bin"
log="$work/commands.log"
token='11111111-2222-3333-4444-555555555555'
mkdir -p "$fake" "$work/cgroup"
printf '%s\n' "$token" >"$work/token"
chmod 600 "$work/token"
printf '2147483648\n' >"$work/cgroup/memory.max"
printf '100000 100000\n' >"$work/cgroup/cpu.max"

checks=0
failures=0

expect_status() {
  local actual="$1" expected="$2" label="$3"
  checks=$((checks + 1))
  if [ "$actual" -eq "$expected" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (expected exit %s, got %s)\n' "$label" "$expected" "$actual"
}

expect_has() {
  local haystack="$1" needle="$2" label="$3"
  checks=$((checks + 1))
  if printf '%s' "$haystack" | grep -qF -- "$needle"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (missing: %s)\n' "$label" "$needle"
}

expect_lacks() {
  local haystack="$1" needle="$2" label="$3"
  checks=$((checks + 1))
  if ! printf '%s' "$haystack" | grep -qF -- "$needle"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (unexpected: %s)\n' "$label" "$needle"
}

expect_true() {
  local label="$1"
  shift
  checks=$((checks + 1))
  if "$@"; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s\n' "$label"
}

expect_mode() {
  local path="$1" mode="$2" label="$3" actual
  checks=$((checks + 1))
  actual="$(stat -c %a "$path" 2>/dev/null || printf 'missing')"
  if [ "$actual" = "$mode" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (mode %s, expected %s)\n' "$label" "$actual" "$mode"
}

expect_lines() {
  local path="$1" needle="$2" expected="$3" label="$4" actual
  checks=$((checks + 1))
  actual="$(grep -cF -- "$needle" "$path" 2>/dev/null || true)"
  actual="${actual:-0}"
  if [ "$actual" -eq "$expected" ]; then
    return 0
  fi
  failures=$((failures + 1))
  printf 'FAIL  %s (%s lines, expected %s)\n' "$label" "$actual" "$expected"
}

section() { printf '\n# %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Stand-in commands
# ---------------------------------------------------------------------------

cat >"$fake/systemd-detect-virt" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${FAKE_VIRT:-none}"
[ "${FAKE_VIRT:-none}" != "none" ]
EOF

cat >"$fake/id" <<'EOF'
#!/usr/bin/env bash
if [ "$#" -eq 1 ] && [ "$1" = "-u" ]; then exec /usr/bin/id -u; fi
if [ "$#" -eq 2 ] && [ "$1" = "-u" ]; then
  if [ "${FAKE_USER_EXISTS:-1}" = "1" ]; then printf '1000\n'; exit 0; fi
  exit 1
fi
exec /usr/bin/id "$@"
EOF

cat >"$fake/log-only" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$(basename "$0") $*" >>"${FAKE_LOG:-/dev/null}"
exit 0
EOF

for name in adduser usermod chown mkswap swapon sysctl apt-get dpkg; do
  cp "$fake/log-only" "$fake/$name"
done

cat >"$fake/timedatectl" <<'EOF'
#!/usr/bin/env bash
printf 'timedatectl %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
printf 'yes\n'
exit 0
EOF

cat >"$fake/fallocate" <<'EOF'
#!/usr/bin/env bash
printf 'fallocate %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
for last; do :; done
[ -n "${last:-}" ] && : >"$last"
exit 0
EOF

cat >"$fake/gpg" <<'EOF'
#!/usr/bin/env bash
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output) out="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done
[ -n "$out" ] || exit 1
cat >"$out"
EOF

cat >"$fake/curl" <<'EOF'
#!/usr/bin/env bash
printf 'curl %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
case " $* " in
  *" --config "*)
    cat >"${FAKE_CURL_STDIN:-/dev/null}"
    [ -z "${FAKE_CURL_ERR:-}" ] || printf '%s\n' "$FAKE_CURL_ERR" >&2
    [ "${FAKE_CURL_EXIT:-0}" -eq 0 ] || exit "${FAKE_CURL_EXIT}"
    printf '%s\n' "${FAKE_CURL_BODY:-OK}"
    exit 0
    ;;
  *" --write-out "*)
    printf '%s' "${FAKE_HTTP_CODE:-200}"
    exit 0
    ;;
  *)
    printf 'FAKEKEY\n'
    exit 0
    ;;
esac
EOF

cat >"$fake/sshd" <<'EOF'
#!/usr/bin/env bash
case " $* " in
  *" -T "*)
    printf 'passwordauthentication no\npermitrootlogin no\n'
    exit 0
    ;;
  *) exit 0 ;;
esac
EOF

cat >"$fake/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
quiet=0
for argument in "$@"; do
  [ "$argument" = "--quiet" ] && quiet=1
done
case "${1:-}" in
  is-active)
    case "$*" in
      *caddy*) [ "${FAKE_ACTIVE_CADDY:-1}" = "1" ] || exit 3 ;;
      *fail2ban*) [ "${FAKE_ACTIVE_FAIL2BAN:-1}" = "1" ] || exit 3 ;;
    esac
    [ "$quiet" -eq 1 ] || printf 'active\n'
    exit 0
    ;;
  is-enabled)
    [ "$quiet" -eq 1 ] || printf 'enabled\n'
    exit 0
    ;;
  *) exit 0 ;;
esac
EOF

cat >"$fake/ufw" <<'EOF'
#!/usr/bin/env bash
printf 'ufw %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
case "${1:-}" in
  status)
    if [ "${FAKE_UFW_ACTIVE:-1}" = "1" ]; then
      printf 'Status: active\n\n22/tcp                     LIMIT       IN      Anywhere\n80/tcp                     ALLOW       IN      Anywhere\n443/tcp                    ALLOW       IN      Anywhere\n'
    else
      printf 'Status: inactive\n'
    fi
    exit 0
    ;;
  --force) exit "${FAKE_UFW_ENABLE:-0}" ;;
  *) exit 0 ;;
esac
EOF

cat >"$fake/iptables" <<'EOF'
#!/usr/bin/env bash
exit "${FAKE_IPTABLES:-0}"
EOF

cat >"$fake/fail2ban-client" <<'EOF'
#!/usr/bin/env bash
printf 'fail2ban-client %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
case "${1:-} ${2:-}" in
  "status sshd")
    [ "${FAKE_FAIL2BAN:-ok}" = "ok" ] || exit 1
    printf 'Status for the jail: sshd\n|- Banned IP list:\t%s\n' "$(cat "${FAKE_BAN_LIST:-/dev/null}" 2>/dev/null || true)"
    exit 0
    ;;
  "set sshd")
    case "${3:-}" in
      banip)
        [ "${FAKE_FAIL2BAN:-ok}" = "ok" ] || exit 1
        printf '%s\n' "${4:-}" >"${FAKE_BAN_LIST:-/dev/null}"
        exit 0
        ;;
      unbanip)
        : >"${FAKE_BAN_LIST:-/dev/null}"
        exit 0
        ;;
    esac
    ;;
esac
exit 0
EOF

cat >"$fake/caddy" <<'EOF'
#!/usr/bin/env bash
printf 'caddy %s\n' "$*" >>"${FAKE_LOG:-/dev/null}"
case "${1:-}" in
  validate) exit "${FAKE_CADDY_VALIDATE:-0}" ;;
  *) exit 0 ;;
esac
EOF

cat >"$fake/openssl" <<'EOF'
#!/usr/bin/env bash
case " $* " in
  *" s_client "*)
    printf -- '-----BEGIN CERTIFICATE-----\nMIIBFAKE\n-----END CERTIFICATE-----\n'
    exit 0
    ;;
  *" -checkend "*) exit "${FAKE_CERT_VALID:-0}" ;;
  *" -issuer "*) printf 'issuer=CN=FAKE-CA\nnotAfter=Jan  1 00:00:00 2030 GMT\n'; exit 0 ;;
  *) exit 0 ;;
esac
EOF

cat >"$fake/ss" <<'EOF'
#!/usr/bin/env bash
cat "${FAKE_SS:-/dev/null}"
EOF

chmod +x "$fake"/*

new_root() {
  root="$work/root.$RANDOM"
  mkdir -p "$root/etc" "$root/root/.ssh" "$root/var/lib/climb"
  cat >"$root/etc/os-release" <<'EOF'
ID=ubuntu
VERSION_ID="24.04"
PRETTY_NAME="Ubuntu 24.04 LTS"
EOF
  printf 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFAKEkey fake@test\n' \
    >"$root/root/.ssh/authorized_keys"
  chmod 600 "$root/root/.ssh/authorized_keys"
  : >"$work/ban-list"
  : >"$log"
}

run_boot() {
  env PATH="$fake:$PATH" \
    CLIMB_ROOT="$root" \
    CLIMB_CGROUP_ROOT="$work/cgroup" \
    FAKE_LOG="$log" \
    FAKE_BAN_LIST="$work/ban-list" \
    "$deploy_dir/bootstrap.sh" "$@"
}

run_verify() {
  env PATH="$fake:$PATH" \
    CLIMB_ROOT="$root" \
    CLIMB_CGROUP_ROOT="$work/cgroup" \
    FAKE_LOG="$log" \
    "$deploy_dir/verify-server.sh"
}

# ---------------------------------------------------------------------------
section "the Caddyfile states the contract"
# ---------------------------------------------------------------------------
caddyfile="$deploy_dir/Caddyfile"
text="$(cat "$caddyfile")"
expect_has "$text" 'root * /srv/climb/current' "document root"
expect_has "$text" 'encode zstd gzip' "on-the-fly zstd and gzip"
expect_has "$text" 'precompressed br gzip' "precompressed brotli and gzip"
expect_has "$text" 'application/wasm' "wasm media type"
expect_has "$text" 'public, max-age=31536000, immutable' "hashed assets cached for a year"
expect_has "$text" 'header ?Cache-Control "no-cache"' "HTML revalidates by default"
expect_has "$text" "connect-src 'self'" "CSP connect-src"
expect_has "$text" "'wasm-unsafe-eval'" "CSP allows WebAssembly"
expect_has "$text" "frame-ancestors 'none'" "CSP frame ancestors"
expect_has "$text" 'X-Content-Type-Options "nosniff"' "nosniff"
expect_has "$text" 'Referrer-Policy' "referrer policy"
expect_has "$text" 'Permissions-Policy' "permissions policy"
expect_has "$text" 'handle_errors 404' "error handler"
expect_has "$text" '/404.html' "a real 404 document"
expect_has "$text" 'respond "404 Not Found" 404' "a missing 404 page still answers 404"
expect_has "$text" 'output discard' "access log discarded"
expect_has "$text" '# HSTS-FLAG: Strict-Transport-Security' "HSTS is behind a flag"
expect_lacks "$text" 'reverse_proxy' "no reverse proxy"
expect_lacks "$(printf '%s\n' "$text" | grep -v '^[[:space:]]*#')" \
  'Strict-Transport-Security' "HSTS is not emitted before the flag is set"
expect_has "$text" $'\t\t# HSTS-FLAG: Strict-Transport-Security' \
  "the HSTS flag sits inside the header block"

if command -v caddy >/dev/null 2>&1; then
  cp "$caddyfile" "$work/Caddyfile"
  expect_true "caddy validates the shipped Caddyfile" \
    env CLIMB_SITE_ADDRESS=climb.duckdns.org \
    caddy validate --config "$work/Caddyfile" --adapter caddyfile
  sed 's|^\([[:space:]]*\)# HSTS-FLAG: |\1|' "$work/Caddyfile" >"$work/Caddyfile.hsts"
  expect_true "caddy validates the HSTS variant" \
    env CLIMB_SITE_ADDRESS=climb.duckdns.org \
    caddy validate --config "$work/Caddyfile.hsts" --adapter caddyfile
else
  printf 'note: caddy is not installed; the Caddyfile was not parsed by caddy\n'
fi

# ---------------------------------------------------------------------------
section "kvm dry run plans the host branch"
# ---------------------------------------------------------------------------
new_root
out="$(FAKE_VIRT=kvm run_boot --dry-run --domain climb 2>&1)"
expect_status $? 0 "kvm dry run exits 0"
expect_has "$out" 'virtualisation: kvm' "virtualisation is printed"
expect_has "$out" 'would run: fallocate -l 1G' "swap file is planned"
expect_has "$out" 'would run: systemctl enable --now systemd-timesyncd' "time sync is planned"
expect_has "$out" 'would run: sysctl --system' "sysctl is planned"
expect_has "$out" 'would run: ufw --force enable' "firewall is planned"
expect_has "$out" 'would run: fail2ban-client set sshd banip 192.0.2.1' "ban test is planned"
expect_lacks "$out" 'skip: swap' "kvm does not skip swap"
expect_true "dry run writes no sshd drop-in" test ! -e "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf"
expect_true "dry run writes no warning file" test ! -e "$root/var/lib/climb/bootstrap-warnings.txt"

# ---------------------------------------------------------------------------
section "lxc dry run skips what a container cannot own"
# ---------------------------------------------------------------------------
out="$(FAKE_VIRT=lxc run_boot --dry-run --domain climb 2>&1)"
expect_status $? 0 "lxc dry run exits 0"
expect_has "$out" 'virtualisation: lxc' "virtualisation is printed"
expect_has "$out" 'skip: swap' "swap is skipped"
expect_has "$out" 'skip: systemd-timesyncd' "time sync is skipped"
expect_has "$out" 'skip: sysctl and kernel-module changes' "sysctl and modules are skipped"
expect_has "$out" 'skip: Docker' "Docker is skipped"
expect_lacks "$out" 'would run: fallocate' "no swap file is planned"
expect_lacks "$out" 'would run: systemctl enable --now systemd-timesyncd' "timesyncd is not enabled"
expect_lacks "$out" 'would run: sysctl --system' "sysctl is not applied"
expect_lacks "$out" 'modprobe' "no kernel module is touched"
expect_lacks "$out" 'docker' "docker never appears in the plan"

# ---------------------------------------------------------------------------
section "lxc without netfilter warns and still exits 0"
# ---------------------------------------------------------------------------
new_root
out="$(FAKE_VIRT=lxc FAKE_UFW_ACTIVE=0 FAKE_UFW_ENABLE=1 FAKE_IPTABLES=1 \
  run_boot --domain climb --token-file "$work/token" 2>&1)"
status=$?
expect_status "$status" 0 "a container without netfilter still exits 0"
expect_has "$out" 'WARN  ufw could not enforce rules here' "the firewall warning is printed"
expect_has "$out" 'provider firewall panel' "the provider firewall is named"
expect_true "the warning is recorded for verification" \
  test -s "$root/var/lib/climb/bootstrap-warnings.txt"
expect_has "$(cat "$root/var/lib/climb/bootstrap-warnings.txt")" \
  'provider firewall panel' "the warning file carries the advice"
expect_true "the sshd drop-in exists" \
  test -f "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf"
expect_mode "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf" 644 "sshd drop-in mode"
expect_has "$(cat "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf")" \
  'PermitRootLogin no' "root login is off"
expect_has "$(cat "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf")" \
  'PasswordAuthentication no' "passwords are off"
expect_true "the journald limit is written" \
  test -f "$root/etc/systemd/journald.conf.d/50-climb.conf"
expect_has "$(cat "$root/etc/systemd/journald.conf.d/50-climb.conf")" \
  'SystemMaxUse=200M' "journald size limit"
expect_has "$(cat "$root/etc/climb/caddy.env")" \
  'CLIMB_SITE_ADDRESS=climb.duckdns.org' "the site address is the DuckDNS name"
expect_true "the duckdns environment file exists" test -f "$root/etc/climb/duckdns.env"
expect_mode "$root/etc/climb/duckdns.env" 600 "the duckdns environment file is root-only"
expect_mode "$root/etc/climb" 700 "the duckdns directory is root-only"
expect_has "$(cat "$root/etc/climb/duckdns.env")" "DUCKDNS_TOKEN=$token" "the token is stored"
expect_true "the updater is installed" \
  test -x "$root/usr/local/sbin/climb-duckdns-update"
expect_true "the service is installed" test -f "$root/etc/systemd/system/climb-duckdns.service"
expect_true "the timer is installed" test -f "$root/etc/systemd/system/climb-duckdns.timer"
expect_has "$(cat "$log")" 'systemctl enable --now climb-duckdns.timer' "the timer is armed"
expect_lacks "$out" "$token" "the token is never printed"
expect_lacks "$(cat "$root/var/lib/climb/bootstrap-warnings.txt")" "$token" \
  "the token is never written to the warning file"
expect_true "no swap file in a container" test ! -e "$root/swapfile"
expect_true "no sysctl drop-in in a container" test ! -e "$root/etc/sysctl.d/60-climb.conf"
expect_lacks "$(cat "$log")" 'docker' "Docker is never installed"
expect_lacks "$(cat "$log")" 'modprobe' "no kernel module is loaded"

# ---------------------------------------------------------------------------
section "no key means no lockout"
# ---------------------------------------------------------------------------
new_root
rm -f "$root/root/.ssh/authorized_keys"
out="$(FAKE_VIRT=lxc run_boot --domain climb --token-file "$work/token" 2>&1)"
status=$?
expect_status "$status" 1 "the run stops when no key exists"
expect_has "$out" 'no public key found' "the reason names the missing key"
expect_true "the sshd drop-in was not written" \
  test ! -e "$root/etc/ssh/sshd_config.d/00-climb-hardening.conf"

new_root
mkdir -p "$root/home/climb/.ssh"
printf 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIUSERkey user@test\n' \
  >"$root/home/climb/.ssh/authorized_keys"
out="$(FAKE_VIRT=lxc FAKE_USER_EXISTS=1 run_boot --dry-run --domain climb 2>&1)"
expect_status $? 0 "an existing key for the sudo user is accepted"
expect_has "$out" 'already has an authorized key' "the existing key is reported"

# ---------------------------------------------------------------------------
section "kvm run is complete and idempotent"
# ---------------------------------------------------------------------------
new_root
out="$(FAKE_VIRT=kvm run_boot --domain climb --token-file "$work/token" 2>&1)"
expect_status $? 0 "the first kvm run succeeds"
out2="$(FAKE_VIRT=kvm run_boot --domain climb --token-file "$work/token" 2>&1)"
expect_status $? 0 "the second kvm run succeeds"
expect_true "the swap file exists" test -f "$root/swapfile"
expect_has "$(cat "$log")" 'mkswap' "the swap file was formatted"
expect_has "$(cat "$log")" 'swapon' "the swap file was activated"
expect_lines "$root/etc/fstab" '/swapfile none swap sw 0 0' 1 "one fstab line"
expect_has "$(cat "$log")" 'systemctl enable --now systemd-timesyncd' "time sync was enabled"
expect_true "the sysctl drop-in exists" test -f "$root/etc/sysctl.d/60-climb.conf"
expect_lines "$root/etc/apt/sources.list.d/caddy-stable.list" 'deb [signed-by=' 1 "one Caddy apt line"
expect_has "$(cat "$log")" 'caddy validate' "the Caddyfile was validated"
expect_true "the cgroup memory limit is printed" \
  test -n "$(printf '%s' "$out" | grep 'memory limit:')"
expect_has "$out2" 'warnings: 0' "the second run records no warning"

# ---------------------------------------------------------------------------
section "HSTS is enabled only when asked"
# ---------------------------------------------------------------------------
new_root
FAKE_VIRT=lxc run_boot --domain climb --token-file "$work/token" --hsts >/dev/null 2>&1
installed="$root/etc/caddy/Caddyfile"
expect_true "the Caddyfile is installed" test -f "$installed"
expect_has "$(cat "$installed")" 'Strict-Transport-Security "max-age=31536000; includeSubDomains"' \
  "the HSTS header is emitted"
expect_lacks "$(cat "$installed")" 'HSTS-FLAG' "the flag marker is gone"

# ---------------------------------------------------------------------------
section "the DuckDNS updater keeps the token out of argv and the log"
# ---------------------------------------------------------------------------
env_file="$work/duckdns.env"
printf 'DUCKDNS_DOMAIN=climb\nDUCKDNS_TOKEN=%s\n' "$token" >"$env_file"
: >"$work/curl-args"
out="$(PATH="$fake:$PATH" FAKE_LOG="$work/curl-args" FAKE_CURL_STDIN="$work/curl-stdin" FAKE_CURL_BODY=OK \
  CLIMB_DUCKDNS_ENV="$env_file" "$deploy_dir/duckdns-update.sh" 2>&1)"
expect_status $? 0 "a successful update exits 0"
expect_has "$out" 'record for climb.duckdns.org is current' "the success line names the record"
expect_has "$(cat "$work/curl-stdin")" 'domains=climb' "the request carries the domain"
expect_has "$(cat "$work/curl-stdin")" "token=$token" "the request carries the token on stdin"
expect_has "$(cat "$work/curl-stdin")" 'https://www.duckdns.org/update' "the request is HTTPS"
expect_lacks "$(cat "$work/curl-args")" "$token" "the token is absent from the process arguments"

out="$(PATH="$fake:$PATH" FAKE_LOG="$work/curl-args" FAKE_CURL_STDIN="$work/curl-stdin" FAKE_CURL_BODY=KO \
  CLIMB_DUCKDNS_ENV="$env_file" "$deploy_dir/duckdns-update.sh" 2>&1)"
expect_status $? 1 "a rejected update exits 1"
expect_has "$out" 'rejected the update' "a rejection is reported"

out="$(PATH="$fake:$PATH" FAKE_LOG="$work/curl-args" FAKE_CURL_STDIN="$work/curl-stdin" \
  FAKE_CURL_EXIT=7 FAKE_CURL_ERR="$token could not be sent" \
  CLIMB_DUCKDNS_ENV="$env_file" "$deploy_dir/duckdns-update.sh" 2>&1)"
expect_status $? 1 "a transport failure exits 1"
expect_has "$out" '***' "the token is redacted in the error"
expect_lacks "$out" "$token" "the token never reaches the error text"

bad_env="$work/duckdns-bad.env"
printf 'DUCKDNS_DOMAIN=climb\nDUCKDNS_TOKEN=short\n' >"$bad_env"
out="$(PATH="$fake:$PATH" FAKE_LOG="$work/curl-args" FAKE_CURL_STDIN="$work/curl-stdin" \
  CLIMB_DUCKDNS_ENV="$bad_env" "$deploy_dir/duckdns-update.sh" 2>&1)"
expect_status $? 1 "a malformed token is refused"
expect_has "$out" 'expected shape' "the refusal says why"

out="$(PATH="$fake:$PATH" FAKE_LOG="$work/curl-args" FAKE_CURL_STDIN="$work/curl-stdin" \
  CLIMB_DUCKDNS_ENV="$work/absent.env" "$deploy_dir/duckdns-update.sh" 2>&1)"
expect_status $? 1 "a missing environment file is refused"

# ---------------------------------------------------------------------------
section "verify-server reports every item"
# ---------------------------------------------------------------------------
new_root
mkdir -p "$root/etc/climb" "$root/etc/caddy"
printf 'DUCKDNS_DOMAIN=climb\nDUCKDNS_TOKEN=%s\n' "$token" >"$root/etc/climb/duckdns.env"
cp "$deploy_dir/Caddyfile" "$root/etc/caddy/Caddyfile"
printf 'ufw could not enforce rules here (no usable netfilter in this container)\n' \
  >"$root/var/lib/climb/bootstrap-warnings.txt"
printf 'tcp LISTEN 0 4096 0.0.0.0:22 0.0.0.0:*\ntcp LISTEN 0 4096 *:80 *:*\ntcp LISTEN 0 4096 *:443 *:*\n' \
  >"$work/ss"
out="$(FAKE_VIRT=lxc FAKE_SS="$work/ss" FAKE_CERT_VALID=0 FAKE_HTTP_CODE=200 run_verify 2>&1)"
expect_status $? 0 "a healthy host exits 0"
expect_has "$out" 'PASS  os-release' "the OS release passes"
expect_has "$out" 'PASS  ufw' "ufw passes"
expect_has "$out" 'PASS  fail2ban' "fail2ban passes"
expect_has "$out" 'PASS  caddy' "Caddy passes"
expect_has "$out" 'PASS  certificate' "the certificate passes"
expect_has "$out" 'INFO  virtualisation' "the virtualisation type is printed"
expect_has "$out" 'WARN  recorded' "the recorded warning is reprinted"
expect_has "$out" 'only 22, 80 and 443' "the listening ports are named"
expect_has "$out" '5 checks: 5 passed, 0 failed' "the summary counts five items"

new_root
cat >"$root/etc/os-release" <<'EOF'
ID=ubuntu
VERSION_ID="22.04"
PRETTY_NAME="Ubuntu 22.04 LTS"
EOF
mkdir -p "$root/etc/caddy"
cat >"$root/etc/caddy/Caddyfile" <<'CADDY'
{$CLIMB_SITE_ADDRESS:http://localhost} {
}
CADDY
out="$(FAKE_VIRT=kvm FAKE_UFW_ACTIVE=0 FAKE_ACTIVE_FAIL2BAN=0 FAKE_ACTIVE_CADDY=1 \
  FAKE_CADDY_VALIDATE=1 run_verify 2>&1)"
status=$?
expect_status "$status" 1 "a failing host exits 1"
expect_has "$out" 'FAIL  os-release' "the wrong OS release fails"
expect_has "$out" 'FAIL  ufw' "an inactive firewall fails"
expect_has "$out" 'FAIL  fail2ban' "an inactive fail2ban fails"
expect_has "$out" 'FAIL  caddy' "an invalid Caddyfile fails"
expect_has "$out" 'FAIL  certificate' "a missing domain fails"
expect_has "$out" '5 checks: 0 passed, 5 failed' "the summary counts five failures"

# ---------------------------------------------------------------------------
section "static checks"
# ---------------------------------------------------------------------------
for script in "$deploy_dir/bootstrap.sh" "$deploy_dir/duckdns-update.sh" \
  "$deploy_dir/verify-server.sh" "$here/run.sh"; do
  expect_true "bash -n $(basename "$script")" bash -n "$script"
done
if command -v shellcheck >/dev/null 2>&1; then
  expect_true "shellcheck is clean" shellcheck \
    "$deploy_dir/bootstrap.sh" \
    "$deploy_dir/duckdns-update.sh" \
    "$deploy_dir/verify-server.sh" \
    "$here/run.sh"
else
  printf 'note: shellcheck is not installed; the lint check was skipped\n'
fi

# ---------------------------------------------------------------------------

printf '\n%d checks, %d failed\n' "$checks" "$failures"
[ "$failures" -eq 0 ]
