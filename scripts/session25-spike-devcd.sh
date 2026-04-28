#!/usr/bin/env bash
# Session 25 — single-shot v3 spike with devcoredump capture.
#
# Goal: the v3 (4-chunk Mesa-shape) spike eliminated the 0x66d000 fault in
# Session 24 but the IB still doesn't execute (out[0] stays 0xBAADF00D).
# Get a devcoredump on the new failure mode and read CP_HQD_IB_BASE_ADDR:
#   * non-zero -> MEC loaded our IB, wave hung in shader
#   * zero     -> MEC scheduler still rejects submission before IB load
# That answer routes Session 25 to either the preamble-IB hypothesis (path
# 2 in handoff) or the dispatch-isolation hypothesis (path 3).
#
# Budget: this burns 1 of the 3 MODE2/boot resets the kernel allows
# before permanent wedge. Don't re-run without rebooting.
#
# Usage:
#   scripts/session25-spike-devcd.sh
#     -> requires sudo for devcd capture (prompts via sudo -v)
#     -> writes docs/issues/session25/{spike.log,devcd.bin,dmesg.log,canary.log}

set -u

SPIKE=./build/libdrm_store_spike_v3
CL_PROBE=./build/shader/cl_probe
OUTDIR=docs/issues/session25
DEVCD=/sys/class/drm/card0/device/devcoredump/data

if [[ ! -x "$SPIKE" ]]; then
    echo "no $SPIKE — build first:" >&2
    echo "  cc -O2 -Wall -o $SPIKE deps/libdrm_store_spike_v3.c -ldrm_amdgpu" >&2
    exit 2
fi
if [[ ! -x "$CL_PROBE" ]]; then
    echo "no $CL_PROBE — canary unavailable; cannot calibrate budget" >&2
    exit 2
fi

mkdir -p "$OUTDIR"

# ---- sudo upfront (devcd capture needs root, want one prompt total) ----
if ! sudo -v; then
    echo "no sudo — devcd capture impossible; aborting before burning budget" >&2
    exit 3
fi

# ---- pre-canary: GPU healthy? ----
echo "==> pre-canary"
pre=$(timeout 10 "$CL_PROBE" 2>&1 | tail -1)
echo "    $pre"
if [[ "$pre" != *"0xDEADBEEF"* ]]; then
    echo "    canary FAILED before spike — GPU already wedged. Reboot." >&2
    exit 3
fi

# ---- mark dmesg cursor for post-spike slice ----
DMESG_BOOKMARK=$(date +'%Y-%m-%d %H:%M:%S')
echo "==> dmesg bookmark: $DMESG_BOOKMARK"

# ---- the one shot ----
echo "==> spike v3 (single attempt)"
"$SPIKE" 2>&1 | tee "$OUTDIR/spike.log"
echo "    spike rc=${PIPESTATUS[0]}"

# ---- snapshot devcoredump (kernel ~5 min lifetime, copy now) ----
echo "==> waiting for devcoredump"
captured=0
for i in 1 2 3 4 5 6 7 8 9 10; do
    if sudo test -f "$DEVCD"; then
        sudo cat "$DEVCD" > "$OUTDIR/devcd.bin" 2>/dev/null
        sz=$(stat -c%s "$OUTDIR/devcd.bin" 2>/dev/null || echo 0)
        echo "    devcd captured ($sz bytes) -> $OUTDIR/devcd.bin"
        captured=1
        break
    fi
    sleep 1
done
if [[ $captured -eq 0 ]]; then
    echo "    devcd did not appear within 10s — possible no reset triggered"
fi

# ---- dmesg slice ----
echo "==> dmesg slice since bookmark"
sudo journalctl -k --since "$DMESG_BOOKMARK" \
    | grep -E "amdgpu|comp_|gfx|0066d000|66d000|MES|MEC|TDR|reset|ring" \
    > "$OUTDIR/dmesg.log" || true
wc -l "$OUTDIR/dmesg.log"

# ---- post-canary (verifies MODE2 reset succeeded; burns a budget slot) ----
echo "==> post-canary (verifies reset succeeded)"
post=$(timeout 10 "$CL_PROBE" 2>&1 | tail -1)
echo "    $post" | tee -a "$OUTDIR/canary.log"
if [[ "$post" != *"0xDEADBEEF"* ]]; then
    echo "    GPU wedged post-spike — REBOOT before further work." >&2
    exit 1
fi

echo "==> done — artifacts in $OUTDIR"
echo "    spike.log     spike output (out[0] = 0x????????)"
echo "    devcd.bin     devcoredump (binary, parse for CP_HQD_IB_BASE_ADDR)"
echo "    dmesg.log     kernel ring messages since spike start"
echo "    canary.log    post-spike cl_probe result"
