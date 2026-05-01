#!/usr/bin/env bash
# Session 23 — sweep the spike across ring/ip_instance combinations to test the
# "fault is ring-specific" hypothesis from Session 22 handoff (Priority 2).
# Also snapshots /sys/class/drm/card0/device/devcoredump/data immediately after
# each fault, before the kernel auto-clears it (default ~5 min lifetime).
#
# Stop at first PASS, OR halt at 2 fault attempts (the kernel's policy is
# 3 MODE2/boot before permanent wedge — so 2 fault attempts max per boot).
#
# Usage:
#   scripts/spike-ring-matrix.sh                       # try (instance=0, ring=0..3)
#   SPIKE_IP_INSTANCE=1 scripts/spike-ring-matrix.sh   # instance=1, ring=0..3
#
# Devcoredump capture requires sudo. The script will prompt once at the top
# (sudo -v) and snapshot to docs/issues/2026-MM-DD-session<N>-devcd-<ring>.bin
# after each faulting attempt. If sudo isn't available, capture is skipped
# but the sweep continues.

set -u

SPIKE=./build/libdrm_store_spike
if [[ ! -x "$SPIKE" ]]; then
    echo "no $SPIKE — build first: cc -O2 -Wall -o $SPIKE deps/libdrm_store_spike.c -ldrm_amdgpu" >&2
    exit 2
fi

INSTANCE=${SPIKE_IP_INSTANCE:-0}
SESSION=${SESSION:-23}
DATE=$(date +%Y-%m-%d)
OUTDIR=docs/issues
mkdir -p "$OUTDIR"

# Try to get sudo upfront so devcoredump capture works without per-fault prompts.
have_sudo=0
if sudo -n true 2>/dev/null || sudo -v; then
    have_sudo=1
    echo "==> sudo cached — devcoredump capture enabled"
else
    echo "==> no sudo — devcoredump capture skipped (sweep continues)"
fi

# cl_probe canary to confirm GPU healthy before we start.
if [[ -x ./build/shader/cl_probe ]]; then
    echo "==> canary: cl_probe pre-sweep"
    out=$(timeout 10 ./build/shader/cl_probe 2>&1 | tail -1)
    echo "    $out"
    if [[ "$out" != *"0xDEADBEEF"* ]]; then
        echo "    canary FAILED — GPU not healthy. Reboot before retry." >&2
        exit 3
    fi
fi

snapshot_devcd() {
    local ring=$1
    local target="$OUTDIR/${DATE}-session${SESSION}-devcd-r${ring}-i${INSTANCE}.bin"
    if [[ $have_sudo -eq 0 ]]; then return; fi
    # Wait briefly for the kernel to write the dump after the fault.
    for _ in 1 2 3 4 5; do
        if sudo test -f /sys/class/drm/card0/device/devcoredump/data; then
            sudo cat /sys/class/drm/card0/device/devcoredump/data > "$target" 2>/dev/null
            echo "    devcoredump → $target ($(stat -c%s "$target" 2>/dev/null) bytes)"
            return
        fi
        sleep 1
    done
    echo "    devcoredump did not appear within 5 s"
}

attempts=0
for ring in 0 1 2 3; do
    echo "==> attempt: ip_instance=$INSTANCE ring=$ring"
    out=$(SPIKE_RING=$ring SPIKE_IP_INSTANCE=$INSTANCE timeout 15 "$SPIKE" 2>&1)
    echo "$out" | grep -E '^(submit|out\[0\]|syncobj_wait|libdrm)'
    attempts=$((attempts + 1))

    if echo "$out" | grep -q 'out\[0\] = 0xDEADBEEF'; then
        echo "==> PASS at ip_instance=$INSTANCE ring=$ring (out[0]=0xDEADBEEF)"
        exit 0
    fi

    echo "==> FAIL at ip_instance=$INSTANCE ring=$ring"
    snapshot_devcd "$ring"

    if [[ $attempts -ge 2 ]]; then
        echo "==> 2 fault attempts reached — REBOOT before continuing." >&2
        exit 1
    fi

    # canary check — if the GPU is wedged, no point trying another ring.
    if [[ -x ./build/shader/cl_probe ]]; then
        c=$(timeout 10 ./build/shader/cl_probe 2>&1 | tail -1)
        echo "    canary post-fail: $c"
        if [[ "$c" != *"0xDEADBEEF"* ]]; then
            echo "==> GPU wedged after $attempts attempts — REBOOT." >&2
            exit 1
        fi
    fi
done

echo "==> all rings tried for ip_instance=$INSTANCE — none worked"
exit 1
