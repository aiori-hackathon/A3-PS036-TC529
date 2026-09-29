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