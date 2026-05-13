#!/usr/bin/env bash
# Soak runner for mabda 3.0.0-rc.x burn-in.
#
# Loops a workload program continuously, sampling health at an
# exponentially-spaced schedule of checkpoints that backs off to
# "natural breaks" — 1 minute, 1 hour, then the rc.3 stop at 6 hours.
# Extend the stop to 24h for rc.4 GA-gate runs, or 72h for the
# post-GA observation window (see docs/development/3-0-rc-{3,4}-punchlist.md).
#
# Default workload: programs/native_compute_store — AMD-only but
# needs no DRM master, runs cleanly from any desktop terminal.
#
# Failure policy: stop immediately on non-zero exit, readback
# divergence, or a new amdgpu/drm dmesg line. Captures the last
# iteration's stdout/stderr + a dmesg tail for triage.
#
# Usage:
#   scripts/soak.sh                       # default: native_compute_store, stop at 6h
#   scripts/soak.sh --workload=wgpu       # render_graph_e2e instead
#   scripts/soak.sh --workload=both       # both in parallel
#   scripts/soak.sh --stop=24h            # rc.4 24h gate
#   scripts/soak.sh --stop=72h            # 3-day observation
#   scripts/soak.sh --stop=5m             # smoke the runner itself
#   scripts/soak.sh --logdir=/tmp/soak-X  # override log location

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

# -- args --------------------------------------------------------------

WORKLOAD="compute"
STOP_SPEC="6h"
LOGDIR=""

for arg in "$@"; do
    case "$arg" in
        --workload=*) WORKLOAD="${arg#*=}" ;;
        --stop=*)     STOP_SPEC="${arg#*=}" ;;
        --logdir=*)   LOGDIR="${arg#*=}" ;;
        -h|--help)
            sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//' | head -n -1
            exit 0
            ;;
        *) echo "unknown arg: $arg (try --help)" >&2; exit 2 ;;
    esac
done

# -- duration parser ---------------------------------------------------

parse_duration() {
    local spec="$1"
    case "$spec" in
        *s) echo $(( ${spec%s} )) ;;
        *m) echo $(( ${spec%m} * 60 )) ;;
        *h) echo $(( ${spec%h} * 3600 )) ;;
        *d) echo $(( ${spec%d} * 86400 )) ;;
        *)  echo "$spec" ;;
    esac
}

STOP_S=$(parse_duration "$STOP_SPEC")

# -- checkpoint schedule -----------------------------------------------
#
# Exponential-doubling milestones that snap to natural breaks at
# 1m, 1h, and 6h. Only milestones <= STOP_S fire. Past 6h we keep
# doubling to 12h, then chunk in 12h steps for rc.4 / 72h runs.

CHECKPOINTS_S=(
    1 2 4 8 16 32 60               # seconds → 1 min
    120 240 480 960 1920 3600      # minutes (2,4,8,16,32) → 1 hour
    7200 14400 21600               # hours: 2, 4, 6  (rc.3 stop)
    28800 43200 57600 72000 86400  # 8h, 12h, 16h, 20h, 24h (rc.4 GA gate)
    129600 172800 216000 259200    # 36h, 48h, 60h, 72h  (3-day window)
)

# Build the active schedule: drop anything past STOP_S, append STOP_S
# as the final checkpoint (so we always close cleanly).
SCHEDULE=()
for c in "${CHECKPOINTS_S[@]}"; do
    if [ "$c" -lt "$STOP_S" ]; then SCHEDULE+=("$c"); fi
done
SCHEDULE+=("$STOP_S")

# -- log directory -----------------------------------------------------

if [ -z "$LOGDIR" ]; then
    LOGDIR="$REPO/docs/handoff/soak-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$LOGDIR"

SOAK_LOG="$LOGDIR/soak.log"
SOAK_CSV="$LOGDIR/soak.csv"
ITER_LOG="$LOGDIR/iterations"   # per-program iteration counts/output
DMESG_BASELINE="$LOGDIR/dmesg-baseline.txt"
DMESG_FINAL="$LOGDIR/dmesg-final.txt"
mkdir -p "$ITER_LOG"

echo "elapsed_s,milestone,iters_compute,iters_wgpu,rss_kb,dmesg_amdgpu_delta,status" > "$SOAK_CSV"

log() {
    local msg="$*"
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $msg" | tee -a "$SOAK_LOG"
}

# -- programs to loop --------------------------------------------------

BINS_TO_BUILD=()
PROGS=()  # parallel arrays: name + binary path
NAMES=()

case "$WORKLOAD" in
    compute)
        BINS_TO_BUILD+=("build/native_compute_store")
        NAMES+=("compute"); PROGS+=("$REPO/build/native_compute_store")
        ;;
    wgpu)
        BINS_TO_BUILD+=("build/render_graph_e2e")
        NAMES+=("wgpu"); PROGS+=("$REPO/build/render_graph_e2e")
        ;;
    both)
        BINS_TO_BUILD+=("build/native_compute_store" "build/render_graph_e2e")
        NAMES+=("compute"); PROGS+=("$REPO/build/native_compute_store")
        NAMES+=("wgpu");    PROGS+=("$REPO/build/render_graph_e2e")
        ;;
    *) echo "unknown --workload: $WORKLOAD (compute|wgpu|both)" >&2; exit 2 ;;
esac

# -- build prerequisites ----------------------------------------------

log "soak start — workload=$WORKLOAD stop=$STOP_SPEC ($STOP_S s) logdir=$LOGDIR"
log "checkpoint schedule (s): ${SCHEDULE[*]}"

for b in "${BINS_TO_BUILD[@]}"; do
    if [ ! -x "$b" ]; then
        log "building $b"
        make "$b" >> "$SOAK_LOG" 2>&1 || { log "FAIL: $b did not build"; exit 1; }
    fi
done

# -- dmesg baseline ---------------------------------------------------

# Snapshot the amdgpu / drm lines before we start so we can detect
# *new* lines added during the soak. dmesg without -k may need
# CAP_SYSLOG on some kernels; fall back to journalctl if available.

snapshot_dmesg() {
    if dmesg -t 2>/dev/null > "$1.raw"; then :
    elif command -v journalctl >/dev/null && journalctl -k -o cat --no-pager > "$1.raw" 2>/dev/null; then :
    else echo "" > "$1.raw"; fi
    grep -iE 'amdgpu|drm' "$1.raw" > "$1" || true
    rm -f "$1.raw"
}

snapshot_dmesg "$DMESG_BASELINE"
BASELINE_COUNT=$(wc -l < "$DMESG_BASELINE" | tr -d ' ')
log "dmesg amdgpu/drm baseline lines: $BASELINE_COUNT"

# -- per-program loop runner ------------------------------------------
#
# For each program, fork a bash loop that runs the binary, increments
# an iteration counter, captures last stdout/stderr, and exits on
# non-zero. The loop process's PID is held so we can sample its RSS
# and tear it down at stop.

declare -a LOOP_PIDS=()
declare -a ITER_FILES=()
declare -a STATUS_FILES=()
declare -a STDOUT_FILES=()
declare -a STDERR_FILES=()

start_loop() {
    local name="$1" bin="$2"
    local iter_f="$ITER_LOG/$name.iters"
    local stat_f="$ITER_LOG/$name.status"      # "ok" or "FAIL: <reason>"
    local out_f="$ITER_LOG/$name.last.stdout"
    local err_f="$ITER_LOG/$name.last.stderr"
    echo 0 > "$iter_f"
    echo ok > "$stat_f"

    (
        n=0
        while :; do
            if "$bin" > "$out_f" 2> "$err_f"; then
                n=$((n + 1))
                echo "$n" > "$iter_f"
            else
                rc=$?
                echo "FAIL: $bin exited rc=$rc at iteration $((n + 1))" > "$stat_f"
                exit 0   # quiet exit; main loop reads $stat_f
            fi
        done
    ) &

    LOOP_PIDS+=("$!")
    ITER_FILES+=("$iter_f")
    STATUS_FILES+=("$stat_f")
    STDOUT_FILES+=("$out_f")
    STDERR_FILES+=("$err_f")
    log "started $name loop (pid=$!, bin=$bin)"
}

stop_loops() {
    for pid in "${LOOP_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null || true
            # also kill any in-flight child binary
            pkill -P "$pid" 2>/dev/null || true
        fi
    done
    wait 2>/dev/null || true
}

trap 'log "interrupted"; stop_loops; exit 130' INT TERM

# -- sampling helpers --------------------------------------------------

sum_rss_kb() {
    local total=0
    for pid in "${LOOP_PIDS[@]}"; do
        if [ -r "/proc/$pid/statm" ]; then
            # statm fields are in pages; field 2 is resident
            local pages
            pages=$(awk '{print $2}' "/proc/$pid/statm" 2>/dev/null || echo 0)
            local pg_kb
            pg_kb=$(getconf PAGESIZE 2>/dev/null || echo 4096)
            pg_kb=$((pg_kb / 1024))
            total=$((total + pages * pg_kb))
        fi
    done
    echo "$total"
}

dmesg_amdgpu_delta() {
    snapshot_dmesg "$DMESG_FINAL"
    local current
    current=$(wc -l < "$DMESG_FINAL" | tr -d ' ')
    echo $((current - BASELINE_COUNT))
}

iters_for() {
    local idx="$1"
    cat "${ITER_FILES[$idx]}" 2>/dev/null || echo 0
}

status_for() {
    local idx="$1"
    cat "${STATUS_FILES[$idx]}" 2>/dev/null || echo unknown
}

# -- format helpers ----------------------------------------------------

fmt_elapsed() {
    local s="$1"
    if   [ "$s" -lt 60 ]; then printf "%ds" "$s"
    elif [ "$s" -lt 3600 ]; then printf "%dm" "$((s / 60))"
    elif [ "$s" -lt 86400 ]; then printf "%dh" "$((s / 3600))"
    else printf "%dd%dh" "$((s / 86400)) $((s % 86400 / 3600))"
    fi
}

# -- start workload ---------------------------------------------------

for i in "${!NAMES[@]}"; do
    start_loop "${NAMES[$i]}" "${PROGS[$i]}"
done

START_T=$(date +%s)

# -- main checkpoint loop ----------------------------------------------

EXIT_CODE=0

for ms in "${SCHEDULE[@]}"; do
    # sleep until milestone ms (relative to START_T), waking briefly to
    # check loop health so a failure inside a long inter-checkpoint
    # interval doesn't go undetected for hours.
    while :; do
        now=$(date +%s)
        elapsed=$((now - START_T))
        if [ "$elapsed" -ge "$ms" ]; then break; fi
        # check for early failures every <= 10s
        early_fail=""
        for i in "${!NAMES[@]}"; do
            s=$(status_for "$i")
            if [ "$s" != "ok" ]; then early_fail="${NAMES[$i]}: $s"; break; fi
        done
        if [ -n "$early_fail" ]; then break; fi
        sleep_s=$((ms - elapsed))
        [ "$sleep_s" -gt 10 ] && sleep_s=10
        sleep "$sleep_s"
    done

    now=$(date +%s)
    elapsed=$((now - START_T))
    rss=$(sum_rss_kb)
    dmesg_d=$(dmesg_amdgpu_delta)

    # gather per-loop iter counts + status
    iter_c=0; iter_w=0
    for i in "${!NAMES[@]}"; do
        case "${NAMES[$i]}" in
            compute) iter_c=$(iters_for "$i") ;;
            wgpu)    iter_w=$(iters_for "$i") ;;
        esac
    done

    # status determination
    status="PASS"
    fail_reason=""
    for i in "${!NAMES[@]}"; do
        s=$(status_for "$i")
        if [ "$s" != "ok" ]; then status="FAIL"; fail_reason="${NAMES[$i]}: $s"; break; fi
    done
    if [ "$dmesg_d" -gt 0 ]; then
        status="FAIL"; fail_reason="new amdgpu/drm dmesg lines: $dmesg_d"
    fi

    label=$(fmt_elapsed "$ms")
    printf "[%s] t=%-6s rss=%-8sKB iters: compute=%-6s wgpu=%-6s dmesg_Δ=%s  %s\n" \
        "$(date -u +%H:%M:%SZ)" "$label" "$rss" "$iter_c" "$iter_w" "$dmesg_d" "$status" \
        | tee -a "$SOAK_LOG"
    echo "$elapsed,$label,$iter_c,$iter_w,$rss,$dmesg_d,$status" >> "$SOAK_CSV"

    if [ "$status" = "FAIL" ]; then
        log "REGRESSION DETECTED: $fail_reason"
        log "last-iter stdout / stderr / dmesg-diff captured in $LOGDIR"
        diff "$DMESG_BASELINE" "$DMESG_FINAL" > "$LOGDIR/dmesg.diff" 2>/dev/null || true
        EXIT_CODE=1
        break
    fi
done

# -- teardown ---------------------------------------------------------

stop_loops

log "soak end — exit=$EXIT_CODE — see $LOGDIR/soak.csv for milestone table"

exit "$EXIT_CODE"
