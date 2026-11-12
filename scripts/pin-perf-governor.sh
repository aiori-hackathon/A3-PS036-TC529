#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# pin-perf-governor.sh — Pin every CPU core to a cpufreq governor.
#
# This is a benchmark-environment tuning script for the CLIMB suite.  It
# switches all online CPU cores to a target governor (default: performance)
# so that benchmark runs see consistent, reproducible frequency behaviour.
#
# Requirements: root (sudo).  The script assumes it already runs as root and
#               does NOT call sudo internally.
#
# Preflight refusal (exit non-zero, no switching attempted):
#   — The machine is running on battery power (AC offline).
#   — Average core usage over the last minute exceeds 20% (load average
#     check), to avoid fighting system workloads.
#
# The script switches the governor; it does not refuse because the current
# governor differs from the target. The benchmark runner's own preflight
# refuses to start when the governor is not already pinned.
#
# Usage:
#   sudo ./scripts/pin-perf-governor.sh [governor]
#
#   governor  optional — default "performance"
#             common values: performance, powersave, ondemand, conservative,
#             schedutil, userspace
#
# Restore original governor after benchmarking:
#   sudo ./scripts/pin-perf-governor.sh powersave
#   sudo ./scripts/pin-perf-governor.sh schedutil
#
# Switching method (tried in order):
#   1. cpupower frequency-set -g "$GOVERNOR"   (if cpupower is available)
#   2. Direct sysfs write to each scaling_governor file
#
# The script prints a before/after table and exits non-zero if any core
# could not be switched or if the cpufreq sysfs interface is unavailable.
# ---------------------------------------------------------------------------
set -euo pipefail

GOVERNOR="${1:-performance}"

# --- root guard ------------------------------------------------------------
if [[ "${EUID}" -ne 0 ]]; then
    {
        printf 'Error: root privileges required.\n'
        printf 'Run:  sudo "%s"' "$0"
        if [[ $# -gt 0 ]]; then
            printf ' %s' "$@"
        fi
        printf '\n'
    } >&2
    exit 1
fi

# --- Preflight: check if running on battery --------------------------------
# If AC is offline and a battery reports it is discharging, refuse to proceed.
# This preserves battery life and avoids non-reproducible frequency scaling.
AC_STATUS="unknown"
for ac in AC ACAD ADP1; do
    if [[ -f "/sys/class/power_supply/$ac/online" ]]; then
        AC_STATUS="$(<"/sys/class/power_supply/$ac/online" 2>/dev/null || echo "unknown")"
        break
    fi
done

BATTERY_DISCHARGING="false"
for b in /sys/class/power_supply/BAT*; do
    if [[ -f "$b/status" ]]; then
        if [[ "$(<"$b/status")" == "Discharging" ]]; then
            BATTERY_DISCHARGING="true"
        fi
    elif [[ -f "$b/online" ]]; then
        if [[ "$(<"$b/online")" == "1" ]]; then
            BATTERY_DISCHARGING="true"
        fi
    fi
done

if [[ "$AC_STATUS" != "1" ]] && [[ "$BATTERY_DISCHARGING" == "true" ]]; then
    printf 'Error: running on battery power (AC offline).\n' >&2
    printf 'Pin-perf-governor refuses to proceed when running on battery.\n' >&2
    exit 6
fi

# --- Preflight: check core usage > 20% ------------------------------------
# Load average over the last minute as a percentage of logical CPUs. The
# benchmark runner applies the same threshold to its pinned core.
if [[ -r /proc/loadavg ]]; then
    LOAD1="$(awk '{print $1}' /proc/loadavg)"
    LOGICAL_CPUS="$(nproc 2>/dev/null || echo 1)"
    USAGE_PERCENT="$(awk -v l="$LOAD1" -v n="$LOGICAL_CPUS" \
        'BEGIN { printf "%.1f", (n > 0 ? l / n * 100 : 0) }')"
    if awk -v u="$USAGE_PERCENT" 'BEGIN { exit !(u > 20) }'; then
        printf 'Error: average core usage is %s%% (exceeds 20%% threshold).\n' "$USAGE_PERCENT" >&2
        printf 'Pin-perf-governor refuses to proceed when average core usage exceeds 20%%.\n' >&2
        exit 7
    fi
fi

# --- enumerate CPU governor files ------------------------------------------
governor_files=()
for f in /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_governor; do
    [[ -f "$f" ]] || continue
    governor_files+=("$f")
done

if [[ ${#governor_files[@]} -eq 0 ]]; then
    printf 'Error: no cpufreq governor files found.\n' >&2
    printf 'The cpufreq sysfs interface appears unavailable on this system.\n' >&2
    exit 2
fi

# Sort by CPU number for readable output
sorted_files=()
while IFS= read -r f; do
    sorted_files+=("$f")
done < <(printf '%s\n' "${governor_files[@]}" | sort -V)

# Helper: extract "cpuN" label from a governor file path
cpu_label() {
    local f="$1"
    # /sys/devices/system/cpu/cpuN/cpufreq/scaling_governor → cpuN
    basename "$(dirname "$(dirname "$f")")"
}

# --- BEFORE table ----------------------------------------------------------
printf '=== BEFORE (governor per core) ===\n'
for f in "${sorted_files[@]}"; do
    printf '  %-8s  %s\n' "$(cpu_label "$f")" "$(<"$f")"
done

# --- switch ----------------------------------------------------------------
switched_by='direct sysfs write'
if command -v cpupower &>/dev/null; then
    if cpupower frequency-set -g "$GOVERNOR" &>/dev/null; then
        switched_by='cpupower'
    else
        printf 'Warning: cpupower failed, falling back to direct sysfs writes.\n' >&2
    fi
fi

if [[ "$switched_by" != 'cpupower' ]]; then
    write_failures=()
    for f in "${sorted_files[@]}"; do
        if ! printf '%s\n' "$GOVERNOR" > "$f" 2>/dev/null; then
            write_failures+=("$(cpu_label "$f")")
        fi
    done
    if [[ ${#write_failures[@]} -gt 0 ]]; then
        printf 'Error: sysfs write failed for: %s\n' "${write_failures[*]}" >&2
        exit 3
    fi
fi

# --- AFTER table -----------------------------------------------------------
printf '=== AFTER  (governor per core) ===\n'
for f in "${sorted_files[@]}"; do
    printf '  %-8s  %s\n' "$(cpu_label "$f")" "$(<"$f")"
done

# --- verification ----------------------------------------------------------
failed_cpus=()
for f in "${sorted_files[@]}"; do
    read -r current < "$f" || true
    if [[ "$current" != "$GOVERNOR" ]]; then
        failed_cpus+=("$(cpu_label "$f")")
    fi
done

if [[ ${#failed_cpus[@]} -gt 0 ]]; then
    printf 'Error: cores did NOT switch to "%s": %s\n' \
        "$GOVERNOR" "${failed_cpus[*]}" >&2
    exit 4
fi

printf 'All %d CPUs successfully switched to governor: %s\n' \
    "${#sorted_files[@]}" "$GOVERNOR"