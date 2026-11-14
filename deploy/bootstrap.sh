#!/usr/bin/env bash
#
# Prepare a fresh Ubuntu 24.04 host to serve the static dashboard.
#
# The script is idempotent: running it twice leaves the same files and units.
# It installs nothing that runs benchmark or DNSSEC code — the host serves
# static files, updates one DuckDNS record and terminates TLS.
#
# What it does, in order:
#   1. refuses any release other than Ubuntu 24.04;
#   2. detects the virtualisation type and branches (a container shares the
#      host kernel, so swap, time sync, sysctl and module changes are skipped);
#   3. installs base packages, creates a sudo user, and installs that user's
#      key before it turns password and root logins off;
#   4. configures ufw, fail2ban, unattended upgrades and the journald budget;
#   5. installs Caddy from its official apt repository and writes the site
#      configuration;
#   6. installs the DuckDNS updater, its timer, and a root-only environment
#      file that holds the token.
#
# Usage:
#   sudo bash deploy/bootstrap.sh --domain climb [--token-file /root/token]
#   sudo bash deploy/bootstrap.sh --dry-run
#   sudo bash deploy/bootstrap.sh --hsts          # after the first HTTPS deploy
#
# The DuckDNS token is read from a file (mode 0600) or typed at a prompt. It is
# never passed as an argument, never printed and never written to a log.
#
# Two environment variables make the branch logic testable against a temporary
# root; leave both unset on a real host:
#   CLIMB_ROOT        prefix for every path the script writes (default: /)
#   CLIMB_CGROUP_ROOT where the cgroup v2 limits are read (default: /sys/fs/cgroup)

set -euo pipefail

root_prefix="${CLIMB_ROOT:-}"
etc="${root_prefix}/etc"
cgroup_root="${CLIMB_CGROUP_ROOT:-/sys/fs/cgroup}"
template_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

site_user="${CLIMB_USER:-climb}"
site_domain=""
token_file=""
dry_run=0
enable_hsts=0
warnings_file="${root_prefix}/var/lib/climb/bootstrap-warnings.txt"
warn_count=0

say() { printf '%s\n' "$*"; }
info() { printf '  %s\n' "$*"; }
die() {
  printf 'bootstrap: %s\n' "$*" >&2
  exit 1
}
ok() { printf '  ok: %s\n' "$*"; }

# Record a condition the human has to know about. Every warning is also
# appended to a state file that deploy/verify-server.sh reads back, so a
# warning is visible after the ssh session that produced it has ended.
warn() {
  local message="$*"
  printf 'WARN  %s\n' "$message"
  warn_count=$((warn_count + 1))
  if [ "$dry_run" -eq 0 ]; then
    mkdir -p "$(dirname "$warnings_file")"
    printf '%s\n' "$message" >>"$warnings_file"
  fi
}

# Run a command, or print it when --dry-run is set. Callers use this for every
# action that changes the system, so a dry run touches nothing.
run() {
  if [ "$dry_run" -eq 1 ]; then
    printf 'would run: %s\n' "$*"
    return 0
  fi
  "$@"
}

# Write a file from stdin. Directories are created as needed and the mode is
# applied to the file, not the umask.
write_file() {
  local path="$1" mode="$2" tmp
  if [ "$dry_run" -eq 1 ]; then
    printf 'would write %s (mode %s)\n' "$path" "$mode"
    cat >/dev/null
    return 0
  fi
  tmp="$(mktemp "${TMPDIR:-/tmp}/climb-bootstrap.XXXXXX")"
  cat >"$tmp"
  install -D -m "$mode" "$tmp" "$path"
  rm -f "$tmp"
}

# Append a line to a file exactly once (used for fstab).
ensure_line() {
  local file="$1" line="$2"
  if [ "$dry_run" -eq 1 ]; then
    printf 'would ensure line in %s: %s\n' "$file" "$line"
    return 0
  fi
  mkdir -p "$(dirname "$file")"
  touch "$file"
  grep -qxF -- "$line" "$file" || printf '%s\n' "$line" >>"$file"
}

usage() {
  cat <<'USAGE'
Prepare an Ubuntu 24.04 host to serve the static dashboard.

  --domain NAME      DuckDNS subdomain label (for example: climb)
  --token-file FILE  file holding the DuckDNS token (mode 0600)
  --user NAME        sudo user to create and use for SSH (default: climb)
  --hsts             emit the Strict-Transport-Security header
  --dry-run          print every action without changing anything
  --help             this text
USAGE
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --domain)
        [ $# -ge 2 ] || die "--domain needs a value"
        site_domain="$2"
        shift 2
        ;;
      --token-file)
        [ $# -ge 2 ] || die "--token-file needs a value"
        token_file="$2"
        shift 2
        ;;
      --user)
        [ $# -ge 2 ] || die "--user needs a value"
        site_user="$2"
        shift 2
        ;;
      --hsts)
        enable_hsts=1
        shift
        ;;
      --dry-run)
        dry_run=1
        shift
        ;;
      --help | -h)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1 (try --help)"
        ;;
    esac
  done
}

require_root() {
  if [ "$(id -u)" -eq 0 ]; then
    return 0
  fi
  if [ -n "$root_prefix" ]; then
    info "CLIMB_ROOT is set; writing under $root_prefix instead of /"
    return 0
  fi
  die "run as root (sudo bash $0)"
}

# ---------------------------------------------------------------------------
# Detection
# ---------------------------------------------------------------------------

os_release_file="${root_prefix}/etc/os-release"
os_id=""
os_version=""
os_pretty=""

read_os_release() {
  [ -r "$os_release_file" ] || die "cannot read $os_release_file"
  # A fixed, root-owned file in a documented format; sourcing it is how
  # os-release is meant to be read.
  # shellcheck disable=SC1090
  . "$os_release_file"
  os_id="${ID:-unknown}"
  os_version="${VERSION_ID:-unknown}"
  os_pretty="${PRETTY_NAME:-unknown}"
}

detect_virt() {
  local detected
  if command -v systemd-detect-virt >/dev/null 2>&1; then
    detected="$(systemd-detect-virt 2>/dev/null || true)"
  else
    detected=""
  fi
  case "$detected" in
    "" | none) printf 'host' ;;
    lxc | lxc-libvirt) printf 'lxc' ;;
    *) printf '%s' "$detected" ;;
  esac
}

# ---------------------------------------------------------------------------
# Steps
# ---------------------------------------------------------------------------

install_base_packages() {
  local packages=(
    ca-certificates
    curl
    debian-archive-keyring
    debian-keyring
    fail2ban
    gnupg
    python3-systemd
    ufw
    unattended-upgrades
  )
  say "== base packages =="
  run env DEBIAN_FRONTEND=noninteractive apt-get update -qq
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${packages[@]}"
}

create_site_user() {
  local home="${root_prefix}/home/${site_user}"
  say "== sudo user =="
  if id -u "$site_user" >/dev/null 2>&1; then
    info "user $site_user already exists"
  else
    run adduser --disabled-password --gecos "" --shell /bin/bash "$site_user"
    ok "created $site_user"
    info "set a password once with 'passwd $site_user' so sudo can authenticate"
  fi
  run usermod -aG sudo "$site_user"
  run install -d -m 700 "$home/.ssh"
  run chown "$site_user:$site_user" "$home" "$home/.ssh"

  local source_keys="${root_prefix}/root/.ssh/authorized_keys"
  local target_keys="$home/.ssh/authorized_keys"
  if [ -s "$target_keys" ]; then
    info "$site_user already has an authorized key"
    return 0
  fi
  if [ -s "$source_keys" ]; then
    if [ "$dry_run" -eq 1 ]; then
      say "would copy: $source_keys -> $target_keys"
    else
      install -m 600 "$source_keys" "$target_keys"
      chown "$site_user:$site_user" "$target_keys"
    fi
    ok "installed the root user's public key for $site_user"
    return 0
  fi
  # Turning password and root logins off without a working key locks everyone
  # out, so this is a hard stop rather than a warning.
  die "no public key found for $site_user: add one to $source_keys first"
}

harden_ssh() {
  local dropin="${etc}/ssh/sshd_config.d/00-climb-hardening.conf"
  say "== ssh: key-only, no root login =="
  write_file "$dropin" 644 <<'SSHD'
# Managed by the deployment bootstrap; edit the template, not this copy.
# Read first by sshd (00- sorts before the cloud-init drop-in), and sshd keeps
# the first value it obtains for a keyword.
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitEmptyPasswords no
PermitRootLogin no
AuthenticationMethods publickey
MaxAuthTries 3
LoginGraceTime 20
X11Forwarding no
AllowAgentForwarding no
SSHD

  if [ "$dry_run" -eq 1 ]; then
    printf 'would validate: sshd -t\n'
    printf 'would run: systemctl reload ssh\n'
    return 0
  fi

  if ! sshd -t; then
    rm -f "$dropin"
    die "sshd rejected the hardening drop-in; it was removed and nothing was reloaded"
  fi
  local effective
  effective="$(sshd -T 2>/dev/null || true)"
  if [ -n "$effective" ]; then
    printf '%s\n' "$effective" | grep -qx 'passwordauthentication no' ||
      warn "sshd -T still reports password authentication enabled; inspect /etc/ssh/sshd_config"
    printf '%s\n' "$effective" | grep -qx 'permitrootlogin no' ||
      warn "sshd -T still reports a root login policy; inspect /etc/ssh/sshd_config"
  else
    warn "sshd -T produced no output; the effective policy was not confirmed"
  fi
  run systemctl reload ssh
  ok "password and root logins are off"
  info "keep this session open and test $site_user in a second terminal before closing it"
}

netfilter_available() {
  local names="${root_prefix}/proc/net/ip_tables_names"
  if [ -r "$names" ] && [ -s "$names" ]; then
    return 0
  fi
  iptables -w -S >/dev/null 2>&1
}

ufw_is_active() {
  ufw status 2>/dev/null | grep -q '^Status: active'
}

# Listening sockets on addresses that are not loopback. The container case has
# no netfilter, so this list is the evidence that the provider firewall has
# only the three published ports to open.
report_listeners() {
  command -v ss >/dev/null 2>&1 || return 0
  local unexpected="" external
  external="$(ss -H -tulpn 2>/dev/null | awk '{print $5}' || true)"
  local address host port
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
    warn "listening on non-loopback ports other than 22, 80 and 443: $unexpected"
  else
    ok "listening sockets: only 22, 80 and 443 on non-loopback addresses"
  fi
}

configure_firewall() {
  say "== firewall =="
  run ufw default deny incoming
  run ufw default allow outgoing
  run ufw limit 22/tcp
  run ufw allow 80/tcp
  run ufw allow 443/tcp

  if [ "$dry_run" -eq 1 ]; then
    printf 'would run: ufw --force enable\n'
    return 0
  fi

  local enabled=1
  if ! ufw --force enable; then
    enabled=0
  fi
  if [ "$enabled" -eq 1 ] && ufw_is_active && netfilter_available; then
    ok "ufw is active, default deny incoming, 22 rate-limited, 80 and 443 open"
  else
    warn "ufw could not enforce rules here (no usable netfilter in this container)"
    warn "open only 22, 80 and 443 in the provider firewall panel; everything else stays closed at the panel"
  fi
  report_listeners
}

configure_fail2ban() {
  local jail="${etc}/fail2ban/jail.d/climb-sshd.local"
  say "== fail2ban =="
  write_file "$jail" 644 <<'JAIL'
[DEFAULT]
backend = systemd
bantime = 1h
findtime = 10m
maxretry = 5

[sshd]
enabled = true
JAIL

  if [ "$dry_run" -eq 1 ]; then
    printf 'would run: systemctl restart fail2ban\n'
    printf 'would run: fail2ban-client set sshd banip 192.0.2.1\n'
    printf 'would run: fail2ban-client set sshd unbanip 192.0.2.1\n'
    return 0
  fi

  run systemctl restart fail2ban
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    fail2ban-client status sshd >/dev/null 2>&1 && break
    sleep 1
  done

  if fail2ban-client status sshd >/dev/null 2>&1; then
    ok "sshd jail is active"
  else
    warn "the sshd jail did not come up; inspect journalctl -u fail2ban"
  fi

  # 192.0.2.1 is TEST-NET-1 and can never match a real client. A ban that is
  # refused means the jail cannot act, which is a warning: key-only SSH still
  # holds, and the human should look at the log.
  if fail2ban-client set sshd banip 192.0.2.1 >/dev/null 2>&1; then
    if fail2ban-client status sshd 2>/dev/null | grep -q '192\.0\.2\.1'; then
      ok "ban action works (192.0.2.1 is now in the jail)"
    else
      warn "fail2ban accepted the ban but the address is not listed in the jail"
    fi
    fail2ban-client set sshd unbanip 192.0.2.1 >/dev/null 2>&1 ||
      warn "the test address 192.0.2.1 could not be unbanned; ban it manually"
  else
    warn "the fail2ban ban action failed; key-only SSH remains the primary control"
  fi
}

configure_unattended_upgrades() {
  say "== unattended security upgrades =="
  write_file "${etc}/apt/apt.conf.d/20auto-upgrades" 644 <<'AUTO'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
AUTO
  write_file "${etc}/apt/apt.conf.d/51climb-unattended-upgrades" 644 <<'UU'
// Security upgrades are installed automatically. The host is never rebooted
// without a human decision; a pending reboot is reported by needrestart or
// /var/run/reboot-required.
Unattended-Upgrade::Automatic-Reboot "false";
UU
  run systemctl enable --now unattended-upgrades
}

configure_journald() {
  say "== journald size limit =="
  write_file "${etc}/systemd/journald.conf.d/50-climb.conf" 644 <<'JOURNAL'
[Journal]
SystemMaxUse=200M
SystemMaxFileSize=50M
RuntimeMaxUse=64M
JOURNAL
  run systemctl restart systemd-journald
}

install_caddy() {
  say "== caddy from the official apt repository =="
  local keyring="${root_prefix}/usr/share/keyrings/caddy-stable-archive-keyring.gpg"
  local repo="${etc}/apt/sources.list.d/caddy-stable.list"
  local url="https://dl.cloudsmith.io/public/caddy/stable"

  if [ "$dry_run" -eq 1 ]; then
    printf 'would fetch %s/gpg.key into %s\n' "$url" "$keyring"
    printf 'would write %s\n' "$repo"
  else
    run install -d -m 755 "$(dirname "$keyring")"
    if ! curl -1sLf --proto '=https' --tlsv1.2 "$url/gpg.key" |
      gpg --batch --yes --dearmor --output "$keyring"; then
      die "could not fetch the Caddy signing key from $url"
    fi
    chmod 644 "$keyring"
  fi

  write_file "$repo" 644 <<REPO
deb [signed-by=${keyring}] ${url}/deb/debian any-version main
REPO

  run env DEBIAN_FRONTEND=noninteractive apt-get update -qq
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq caddy
}

write_site_configuration() {
  local caddyfile="${etc}/caddy/Caddyfile"
  local env_file="${etc}/climb/caddy.env"
  say "== site configuration =="
  run install -d -m 755 "${root_prefix}/srv/climb/current"
  run install -d -m 755 "${root_prefix}/srv/climb/releases"
  run chown "$site_user:$site_user" "${root_prefix}/srv/climb"
  run install -d -m 755 "${etc}/caddy"

  if [ "$dry_run" -eq 1 ]; then
    printf 'would write %s (mode 644)\n' "$caddyfile"
  else
    write_file "$caddyfile" 644 <"$template_dir/Caddyfile"
    if [ "$enable_hsts" -eq 1 ]; then
      sed -i 's|^\([[:space:]]*\)# HSTS-FLAG: |\1|' "$caddyfile"
      ok "HSTS enabled for this deployment"
    else
      info "HSTS stays commented out until the first HTTPS deploy is verified"
    fi
  fi

  local address="http://localhost"
  if [ -n "$site_domain" ]; then
    address="${site_domain}.duckdns.org"
  elif [ -r "${etc}/climb/duckdns.env" ]; then
    local recorded
    recorded="$(sed -n 's/^DUCKDNS_DOMAIN=//p' "${etc}/climb/duckdns.env" | head -n1)"
    [ -n "$recorded" ] && address="${recorded}.duckdns.org"
  fi
  write_file "$env_file" 644 <<ENV
# Read by caddy.service through its drop-in. No secret belongs in this file.
CLIMB_SITE_ADDRESS=${address}
ENV
  write_file "${etc}/systemd/system/caddy.service.d/10-climb.conf" 644 <<'DROPIN'
[Service]
EnvironmentFile=-/etc/climb/caddy.env
DROPIN

  if [ "$dry_run" -eq 1 ]; then
    printf 'would run: caddy validate --config %s\n' "$caddyfile"
    printf 'would run: systemctl daemon-reload\n'
    printf 'would run: systemctl enable caddy\n'
    printf 'would run: systemctl restart caddy\n'
    return 0
  fi

  if command -v caddy >/dev/null 2>&1; then
    if ! CLIMB_SITE_ADDRESS="$address" caddy validate --config "$caddyfile" --adapter caddyfile; then
      die "caddy rejected $caddyfile; the previous configuration is still in place"
    fi
    ok "Caddyfile validates"
  else
    warn "caddy is not on PATH; the configuration was not validated"
  fi
  run systemctl daemon-reload
  run systemctl enable caddy
  run systemctl restart caddy
  ok "caddy is enabled with root /srv/climb/current"
}

read_env_value() {
  local file="$1" key="$2"
  [ -r "$file" ] || return 0
  sed -n "s/^${key}=//p" "$file" | head -n1
}

valid_label() {
  printf '%s' "$1" | grep -Eq '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'
}

write_duckdns_env() {
  local env_file="${etc}/climb/duckdns.env"
  local dir
  dir="$(dirname "$env_file")"

  if [ "$dry_run" -eq 1 ]; then
    printf 'would write %s (mode 600)\n' "$env_file"
    [ -n "$site_domain" ] && printf 'would set DUCKDNS_DOMAIN=%s\n' "$site_domain"
    [ -n "$token_file" ] && printf 'would read the token from %s\n' "$token_file"
    return 0
  fi

  install -d -m 700 "$dir"
  run chown root:root "$dir"
  chmod 700 "$dir"

  local current_domain current_token token=""
  current_domain="$(read_env_value "$env_file" DUCKDNS_DOMAIN)"
  current_token="$(read_env_value "$env_file" DUCKDNS_TOKEN)"

  if [ -n "$site_domain" ]; then
    valid_label "$site_domain" ||
      die "--domain must be a single DNS label such as 'climb' (no dots)"
  else
    site_domain="$current_domain"
  fi

  if [ -n "$token_file" ]; then
    [ -r "$token_file" ] || die "cannot read the token file $token_file"
    token="$(tr -d '[:space:]' <"$token_file")"
  elif [ -t 0 ]; then
    read -r -s -p "DuckDNS token (input hidden, leave empty to skip): " token
    printf '\n'
  fi
  if [ -z "$token" ]; then
    token="$current_token"
  fi

  if [ -n "$token" ]; then
    printf '%s' "$token" | grep -Eq '^[A-Za-z0-9-]{8,64}$' ||
      die "the DuckDNS token does not look like a token (letters, digits and dashes only)"
  fi

  local tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/climb-duckdns.XXXXXX")"
  chmod 600 "$tmp"
  {
    printf '# Read by climb-duckdns-update.service. Keep this file mode 0600.\n'
    printf 'DUCKDNS_DOMAIN=%s\n' "$site_domain"
    printf 'DUCKDNS_TOKEN=%s\n' "$token"
  } >"$tmp"
  install -m 600 "$tmp" "$env_file"
  rm -f "$tmp"
  run chown root:root "$env_file"
  ok "wrote $env_file (mode 600)"
}

install_duckdns() {
  say "== duckdns =="
  write_duckdns_env
  write_file "${root_prefix}/usr/local/sbin/climb-duckdns-update" 755 \
    <"$template_dir/duckdns-update.sh"
  write_file "${etc}/systemd/system/climb-duckdns.service" 644 \
    <"$template_dir/systemd/climb-duckdns.service"
  write_file "${etc}/systemd/system/climb-duckdns.timer" 644 \
    <"$template_dir/systemd/climb-duckdns.timer"

  if [ "$dry_run" -eq 1 ]; then
    if [ -n "$site_domain" ]; then
      printf 'would run: systemctl enable --now climb-duckdns.timer\n'
    else
      printf 'would keep DuckDNS unarmed until a domain and token are configured\n'
    fi
    return 0
  fi

  local token
  token="$(read_env_value "${etc}/climb/duckdns.env" DUCKDNS_TOKEN)"
  if [ -n "$site_domain" ] && [ -n "$token" ]; then
    run systemctl daemon-reload
    run systemctl enable --now climb-duckdns.timer
    ok "updater timer enabled for ${site_domain}.duckdns.org"
  else
    warn "DuckDNS is not armed: set DUCKDNS_DOMAIN and DUCKDNS_TOKEN in /etc/climb/duckdns.env, then systemctl enable --now climb-duckdns.timer"
  fi
}

configure_swap() {
  local swapfile="${root_prefix}/swapfile"
  say "== swap =="
  if [ -r "${root_prefix}/proc/swaps" ] && grep -qF "$swapfile" "${root_prefix}/proc/swaps"; then
    ok "swap is already active on $swapfile"
    return 0
  fi
  if [ "$dry_run" -eq 0 ] && [ -e "$swapfile" ]; then
    info "$swapfile exists"
  else
    if ! run fallocate -l 1G "$swapfile"; then
      run dd if=/dev/zero of="$swapfile" bs=1M count=1024 status=none
    fi
    run chmod 600 "$swapfile"
    run mkswap "$swapfile"
  fi
  ensure_line "${etc}/fstab" "/swapfile none swap sw 0 0"
  run swapon "$swapfile"
  ok "1 GB swap file"
}

configure_time_sync() {
  say "== time sync =="
  run systemctl enable --now systemd-timesyncd
  if [ "$dry_run" -eq 1 ]; then
    printf 'would run: timedatectl show -p NTPSynchronized --value\n'
    return 0
  fi
  local synced
  synced="$(timedatectl show -p NTPSynchronized --value 2>/dev/null || true)"
  if [ "$synced" = "yes" ]; then
    ok "clock is synchronised"
  else
    warn "the clock is not reported as synchronised yet; check timedatectl"
  fi
}

configure_host_sysctl() {
  say "== sysctl (host kernel only) =="
  write_file "${etc}/sysctl.d/60-climb.conf" 644 <<'SYSCTL'
# Kept small and reversible. A container shares the host kernel, so this file
# is only written when the guest owns its kernel.
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.all.send_redirects = 0
net.ipv4.tcp_syncookies = 1
vm.swappiness = 10
SYSCTL
  run sysctl --system
}

read_cgroup_limits() {
  local memory cpu
  memory="$(cat "${cgroup_root}/memory.max" 2>/dev/null || true)"
  cpu="$(cat "${cgroup_root}/cpu.max" 2>/dev/null || true)"
  if [ -z "$memory" ]; then
    memory="unknown"
  elif [ "$memory" != "max" ]; then
    memory="$(awk -v bytes="$memory" 'BEGIN { printf "%s bytes (%.2f GiB)", bytes, bytes / 1073741824 }')"
  fi
  if [ -z "$cpu" ]; then
    cpu="unknown"
  fi
  printf '  memory limit: %s\n' "$memory"
  printf '  cpu limit:    %s\n' "$cpu"
}

# ---------------------------------------------------------------------------

main() {
  parse_args "$@"
  require_root

  say "== preflight =="
  read_os_release
  info "os: $os_pretty (ID=$os_id VERSION_ID=$os_version)"
  if [ "$os_id" != "ubuntu" ] || [ "$os_version" != "24.04" ]; then
    die "this host is not Ubuntu 24.04 (found $os_id $os_version)"
  fi
  local virt
  virt="$(detect_virt)"
  info "virtualisation: $virt"
  read_cgroup_limits

  if [ "$dry_run" -eq 0 ]; then
    : >"$warnings_file" 2>/dev/null || {
      mkdir -p "$(dirname "$warnings_file")"
      : >"$warnings_file"
    }
  fi

  install_base_packages
  create_site_user
  harden_ssh
  configure_firewall
  configure_fail2ban
  configure_unattended_upgrades
  configure_journald
  install_caddy
  write_site_configuration
  install_duckdns

  if [ "$virt" = "lxc" ]; then
    say "== container branch =="
    info "skip: swap (a container has no swap of its own; the host owns it)"
    info "skip: systemd-timesyncd (the host owns the clock)"
    info "skip: sysctl and kernel-module changes (shared kernel)"
    info "skip: Docker (this host serves static files only)"
  else
    configure_swap
    configure_time_sync
    configure_host_sysctl
  fi

  say "== summary =="
  if [ "$dry_run" -eq 1 ]; then
    say "dry run: nothing was changed"
  else
    info "warnings: $warn_count (kept in $warnings_file)"
    info "next: bash $template_dir/verify-server.sh"
  fi
}

main "$@"
