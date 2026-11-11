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
#   — Governor is not already set to the target (not currently performance
#     on a performance-capable core, or mixed/non-performance when performance
#     is expected).
#   — The machine is running on battery power (AC offline).
#   — Average core usage over the last minute exceeds 20% (load average
#     check), to avoid fighting system workloads.
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

# --- Preflight: check governor state ---------------------------------------
# Refuse to proceed if the current governor is not already the target.
# Check the first online CPU's governor as the representative reading.
CURRENT_GOV=""
for f in /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_governor; do
    [[ -f "$f" ]] || continue
    CURRENT_GOV="$(<"$f")"
    break
done

if [[ -n "$CURRENT_GOV" && "$CURRENT_GOV" != "$GOVERNOR" ]]; then
    printf 'Error: governor is currently "%s", not "%s".\n' "$CURRENT_GOV" "$GOVERNOR" >&2
    printf 'Pin-perf-governor refuses to proceed when the governor is not already set to the target.\n' >&2
    exit 5
fi

# --- Preflight: check if running on battery --------------------------------
# If AC is offline and a battery supply is online, refuse to proceed.
# This preserves battery life and avoids non-reproducible frequency scaling.
AC_STATUS="$(</sys/class/power_supply/AC/online 2>/dev/null || echo "unknown")"
BATTERY_ONLINE="false"
for b in /sys/class/power_supply/BAT*; do
    if [[ -f "$b/online" ]]; then
        BATTERY_ONLINE="$(<"$b/online" 2>/dev/null || echo "unknown")"
    fi
done

if [[ "$AC_STATUS" != "1" ]] && [[ "$BATTERY_ONLINE" == "1" ]]; then
    printf 'Error: running on battery power (AC offline).\n' >&2
    printf 'Pin-perf-governor refuses to proceed when running on battery.\n' >&2
    exit 6
fi

# --- Preflight: check core usage > 20% ------------------------------------
# Load average over the last minute as a percentage of logical CPUs.
LOAD1="$(</proc/loadavg 2>/dev/null | awk '{print $1}')"
if [[ -n "$LOAD1" ]]; then
    LOGICAL_CPUS="$(nproc 2>/dev/null || echo "1")"
    # Use bc or awk for floating point; fall back to integer math.
    if command -v bc &>/dev/null; then
        USAGE_PERCENT="$(echo "scale=1; $LOAD1 / $LOGICAL_CPUS * 100" | bc)"
    else
        # Integer-only: compare load1 * 100 <=> logical_cpus * load_ceil
        # Simple ceiling division: (load1 + logical_cpus - 1) / logical_cpus
        LOAD_CEIL="$(($(LOAD1 + LOGICAL_CPUS - 1) / LOGICAL_CPUS))"
        USAGE_PERCENT="$(echo "$LOAD_CEIL * 100 / 1" | bc 2>/dev/null || echo "0")"
    fi
    USAGE_INT="${USAGE_PERCENT%.*}"  # strip fractional part
    if [[ "$USAGE_INT" -gt 20 ]]; then
        printf 'Error: average core usage is %s%% (exceeds 20%% threshold).\n' "$USAGE_INT" >&2
        printf 'Pin-perf-governor refuses to proceed when average core usage exceeds 20%%.%s\n' >&2
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