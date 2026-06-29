#!/usr/bin/env bash
# Soak runner for mabda 3.0.0-rc.x burn-in.
#
# Loops one or more workload programs continuously and samples
# health at checkpoints — exponential early ramp (catches infant
# mortality), 15-min cadence through the rc.4 24h window
# (~96 samples/24h per the rc.4 punchlist target), coarser past
# 24h up to the 3-day observation horizon.
#
# Aligned with docs/development/3-0-rc-3-punchlist.md (6h gate)
# and docs/development/3-0-rc-4-punchlist.md (24h GA gate, 72h
# observation window).
#
# Workloads (singletons):
#   compute        programs/native_compute_store        AMD render-node, no master
#   render         programs/native_render_e2e           AMD render-node, no master
#   texture        programs/native_texture_e2e          AMD render-node, no master
#   wgpu           programs/render_graph_e2e            wgpu (Vulkan), no master
#   present        programs/native_present_e2e          AMD card-fd, NEEDS DRM master
#   nvidia-compute programs/nvidia_compute_store        nouveau render-node, no master
#   nvidia-render  programs/nvidia_render_e2e           nouveau render-node, no master
#   nvidia-texture programs/nvidia_texture_e2e          nouveau render-node, no master
#   nvidia-present programs/nvidia_surface_present_e2e  nouveau card-fd, NEEDS DRM master
#
# Workloads (bundles, run in parallel):
#   both          compute + wgpu                        legacy alias (pre-rc.3)
#   native        compute + render + texture            all AMD paths sans master
#   all           compute + wgpu + render               rc.3/rc.4 primary post-bug-squash
#   nvidia-native nvidia-compute + nvidia-render + nvidia-texture  all nouveau paths sans master (v4.0)
#
# Default workload: compute — AMD-only but needs no DRM master,
# runs cleanly from any desktop terminal.
#
# Driver-aware dmesg watch: AMD workloads watch `amdgpu|drm`; nvidia-*
# workloads watch `nouveau|drm|Xid|GSP` (a GSP-RM Xid fault is the
# nouveau analogue of an amdgpu ring TDR — the primary failure signal).
#
# The `present` workload requires DRM master, which on a logind-
# managed session is held by the active compositor and not freely
# takable even with sudo (see project_phase_d_master_logind_blocker).
# Run from a clean tty / kiosk session, or via a samvada-wired
# consumer that holds master.
#
# Failure policy: stop immediately on non-zero exit (each program
# self-asserts its readback / pixel / frame-count invariants and
# returns non-zero on mismatch), or a new driver dmesg line (the
# watched driver is workload-aware: amdgpu/drm for AMD, nouveau/drm/Xid/GSP
# for nvidia-* — see "Driver-aware dmesg watch" below).
# Captures the last iteration's stdout/stderr + a dmesg tail
# for triage.
#
# Usage:
#   scripts/soak.sh                       # default: compute, stop at 6h (rc.3)
#   scripts/soak.sh --workload=render     # the previously TDR'd path
#   scripts/soak.sh --workload=all        # compute + wgpu + render in parallel
#   scripts/soak.sh --workload=native     # all AMD paths (compute+render+texture)
#   scripts/soak.sh --workload=present    # master-gated; tty/kiosk only
#   scripts/soak.sh --workload=nvidia-native   # all nouveau paths (compute+render+texture)
#   scripts/soak.sh --workload=nvidia-present  # nouveau present; master-gated; tty/kiosk only
#   scripts/soak.sh --stop=24h            # rc.4 GA gate
#   scripts/soak.sh --stop=72h            # 3-day observation
#   scripts/soak.sh --stop=5m             # smoke the runner itself
#   scripts/soak.sh --logdir=/tmp/soak-X  # override log location

set -euo pipefail

# Survive session/terminal teardown. `nohup sudo soak.sh` only shields
# the outer sudo, not this monitor. A SIGHUP on session close or a
# SIGPIPE on the checkpoint `tee` can silently kill the monitor mid-run
# while the orphaned workload loops keep spinning — that's the
# 2026-06-02 24h-soak failure (monitor died ~15 min from the finish,
# final checkpoint never recorded). See
# docs/issues/2026-06-01-soak-stale-binary.md (monitor-death amendment).
trap '' HUP PIPE

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

# Captured so workload subshells can detect monitor death and self-exit
# instead of orphaning into init as silent, unrecorded daemons.
MONITOR_PID=$$

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
# Three phases:
#   1. Exponential early ramp 1s..1h — catches infant mortality
#      (immediate-after-start TDRs, fnptr table misload, mmap leak
#      that doubles every iter, etc.).
#   2. 15-min cadence 1h15m..24h — matches the rc.4 punchlist's
#      "every 15 min ≈ 96 samples / 24h" target. Sampling resolution
#      stays useful across the GA gate window without flooding CSV.
#   3. Coarser past 24h up to 72h — observation window, not a gate.
#
# Only milestones <= STOP_S fire; STOP_S itself is always appended.

CHECKPOINTS_S=(
    1 2 4 8 16 32 60               # seconds → 1 min
    120 240 480 960 1920 3600      # minutes (2,4,8,16,32) → 1 hour
)
# 15-min cadence from 1h15m through 24h (rc.4 GA gate). seq generates
# 4500,5400,...,86400 — 92 checkpoints covering the GA-gate window.
while IFS= read -r t; do CHECKPOINTS_S+=("$t"); done < <(seq 4500 900 86400)
CHECKPOINTS_S+=(
    108000 129600 172800 216000 259200   # 30h, 36h, 48h, 60h, 72h
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

# CSV header is emitted *after* the workload case below populates
# NAMES — schema is dynamic so the column set matches the active
# workload(s) instead of hardcoding compute+wgpu.

log() {
    local msg="$*"
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $msg" | tee -a "$SOAK_LOG"
}

# -- programs to loop --------------------------------------------------

BINS_TO_BUILD=()
PROGS=()  # parallel arrays: name + binary path
NAMES=()

# Map a singleton workload name to its binary, then add it to the
# parallel NAMES / PROGS / BINS_TO_BUILD arrays. Bundles below
# call this once per member.
add_workload() {
    local name="$1"
    local bin
    case "$name" in
        compute) bin="build/native_compute_store" ;;
        render)  bin="build/native_render_e2e" ;;
        texture) bin="build/native_texture_e2e" ;;
        wgpu)    bin="build/render_graph_e2e" ;;
        present) bin="build/native_present_e2e" ;;
        nvidia-compute) bin="build/nvidia_compute_store" ;;
        nvidia-render)  bin="build/nvidia_render_e2e" ;;
        nvidia-texture) bin="build/nvidia_texture_e2e" ;;
        nvidia-present) bin="build/nvidia_surface_present_e2e" ;;
        *) echo "internal: unknown singleton '$name'" >&2; exit 2 ;;
    esac
    BINS_TO_BUILD+=("$bin")
    NAMES+=("$name")
    PROGS+=("$REPO/$bin")
}

# Driver-aware dmesg watch. AMD workloads watch amdgpu ring/TDR lines;
# nvidia-* workloads watch nouveau + GSP-RM Xid faults (the nouveau
# analogue of an amdgpu TDR). Default to AMD; the nvidia branches below
# flip it. `drm` is in both so generic DRM core failures surface either way.
DMESG_RE='amdgpu|drm'
DRIVER_LABEL='amdgpu/drm'

case "$WORKLOAD" in
    compute|render|texture|wgpu|present)
        add_workload "$WORKLOAD"
        ;;
    nvidia-compute|nvidia-render|nvidia-texture|nvidia-present)
        add_workload "$WORKLOAD"
        DMESG_RE='nouveau|drm|Xid|GSP'
        DRIVER_LABEL='nouveau/drm/Xid'
        ;;
    both)
        # Legacy alias from pre-rc.3 — kept for any external invokers
        # / handoffs that still pass --workload=both. New work should
        # use --workload=all (which also includes render).
        add_workload compute
        add_workload wgpu
        ;;
    native)
        add_workload compute
        add_workload render
        add_workload texture
        ;;
    all)
        # rc.3/rc.4 primary parallel set. Excludes present (master-
        # gated); pass --workload=present separately on a tty/kiosk
        # box if you want to add it.
        add_workload compute
        add_workload wgpu
        add_workload render
        ;;
    nvidia-native)
        # v4.0 primary parallel set for the NVIDIA backend — all three
        # masterless nouveau paths (renderD128). Excludes nvidia-present
        # (master-gated); pass --workload=nvidia-present separately on a
        # tty/kiosk box. Mirrors the AMD `native` bundle.
        add_workload nvidia-compute
        add_workload nvidia-render
        add_workload nvidia-texture
        DMESG_RE='nouveau|drm|Xid|GSP'
        DRIVER_LABEL='nouveau/drm/Xid'
        ;;
    *) echo "unknown --workload: $WORKLOAD (compute|render|texture|wgpu|present|both|native|all|nvidia-compute|nvidia-render|nvidia-texture|nvidia-present|nvidia-native)" >&2; exit 2 ;;
esac

# -- CSV header (dynamic columns from NAMES) --------------------------

{
    printf "elapsed_s,milestone"
    for n in "${NAMES[@]}"; do printf ",iters_%s" "$n"; done
    printf ",rss_kb,dmesg_drm_delta,status\n"
} > "$SOAK_CSV"

# -- build prerequisites ----------------------------------------------

log "soak start — workload=$WORKLOAD stop=$STOP_SPEC ($STOP_S s) logdir=$LOGDIR"
log "active programs: ${NAMES[*]}"
log "checkpoint schedule (s): ${SCHEDULE[*]}"

# Always invoke make so it rebuilds on *staleness*, not just absence.
# A pre-existing-but-stale binary (older than its src/*.cyr deps, or
# built on a superseded toolchain) silently soaks the wrong bundle —
# see docs/issues/2026-06-01-soak-stale-binary.md. `make` no-ops when the
# target is genuinely up to date, so this is cheap on the common path.
for b in "${BINS_TO_BUILD[@]}"; do
    log "building $b (make resolves up-to-date)"
    make "$b" >> "$SOAK_LOG" 2>&1 || { log "FAIL: $b did not build"; exit 1; }
done

# -- dmesg baseline ---------------------------------------------------

# Snapshot the amdgpu / drm lines before we start so we can detect
# *new* lines added during the soak. dmesg without -k may need
# CAP_SYSLOG on some kernels; fall back to journalctl if available.

# Filter to the active driver's lines ($DMESG_RE, set per-workload). For
# nvidia-* this is `nouveau|drm|Xid|GSP` so a GSP-RM Xid fault trips the
# regression detector the same way an amdgpu ring TDR does for AMD.
snapshot_dmesg() {
    if dmesg -t 2>/dev/null > "$1.raw"; then :
    elif command -v journalctl >/dev/null && journalctl -k -o cat --no-pager > "$1.raw" 2>/dev/null; then :
    else echo "" > "$1.raw"; fi
    grep -iE "$DMESG_RE" "$1.raw" > "$1" || true
    rm -f "$1.raw"
}

snapshot_dmesg "$DMESG_BASELINE"
BASELINE_COUNT=$(wc -l < "$DMESG_BASELINE" | tr -d ' ')
log "dmesg $DRIVER_LABEL baseline lines: $BASELINE_COUNT"

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
            # If the monitor died (signal, crash), don't orphan into a
            # silent forever-loop — exit so the run tears down cleanly.
            kill -0 "$MONITOR_PID" 2>/dev/null || exit 0
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

dmesg_drm_delta() {
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
    dmesg_d=$(dmesg_drm_delta)

    # gather per-loop iter counts (parallel to NAMES order)
    iter_values=()
    for i in "${!NAMES[@]}"; do
        iter_values+=("$(iters_for "$i")")
    done

    # status determination
    status="PASS"
    fail_reason=""
    for i in "${!NAMES[@]}"; do
        s=$(status_for "$i")
        if [ "$s" != "ok" ]; then status="FAIL"; fail_reason="${NAMES[$i]}: $s"; break; fi
    done
    if [ "$dmesg_d" -gt 0 ]; then
        status="FAIL"; fail_reason="new $DRIVER_LABEL dmesg lines: $dmesg_d"
    fi

    # human-readable iter chunk: "compute=123 render=456 wgpu=789"
    iter_human=""
    for i in "${!NAMES[@]}"; do
        iter_human+=$(printf "%s=%-6s " "${NAMES[$i]}" "${iter_values[$i]}")
    done

    # CSV iter chunk: ",123,456,789"
    iter_csv=""
    for v in "${iter_values[@]}"; do iter_csv+=",$v"; done

    label=$(fmt_elapsed "$ms")
    printf "[%s] t=%-6s rss=%-8sKB iters: %sdmesg_Δ=%s  %s\n" \
        "$(date -u +%H:%M:%SZ)" "$label" "$rss" "$iter_human" "$dmesg_d" "$status" \
        | tee -a "$SOAK_LOG"
    echo "$elapsed,$label$iter_csv,$rss,$dmesg_d,$status" >> "$SOAK_CSV"

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

# If launched under sudo (required for dmesg capture), hand the logdir
# back to the invoking user. Otherwise the artifacts are root-owned, and
# committing them makes a later `git checkout`/merge fail on the untracked
# root-owned copies — the 3.0.0 GA-merge papercut. See
# docs/issues/2026-06-01-soak-stale-binary.md.
if [ -n "${SUDO_USER:-}" ]; then
    chown -R "$SUDO_USER" "$LOGDIR" 2>/dev/null || true
fi

exit "$EXIT_CODE"
