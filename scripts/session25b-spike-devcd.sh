#!/usr/bin/env bash
# Session 25b — second-shot v3 spike with three IB-progress markers.
#
# Context: session 25's first spike confirmed MEC loaded our IB and walked
# 23 DWs in before stalling on a WRITE_DATA(WR_CONFIRM=1) targeting an
# unmapped Mesa-VA (0xFFFF800100600300). The patch:
#   - removes that WRITE_DATA and the DMA_DATA-NOWHERE source (also unmapped)
#   - sets TA_CS_BC_BASE_ADDR/HI to 0/0 (was Mesa-magic VA 0xFFFF800100440000)
#   - inserts three CPU-readable markers via WRITE_DATA(WR_CONFIRM=1) to our
#     mapped fence_va: A at start, B before DISPATCH_DIRECT, C after.
#
# Diagnostic table for marker landings (read fence_cpu+0/+8/+16 post-run):
#   A=stale          -> CP never started fetching IB (ring/ctx/HQD setup bug)
#   A=OK B=stale     -> stalled in preamble; BC_BASE=0/0 likely faulted CPC
#   A=OK B=OK C=stale-> dispatch didn't return (shader fault / scratch / CSA)
#   A=OK B=OK C=OK   -> CP loved the IB; out[0] tells us if shader ran
#
# Budget: burns 1 of 2 remaining MODE2 resets. Don't re-run without rebooting.
#
# Usage:
#   scripts/session25b-spike-devcd.sh
#     -> requires sudo for devcd capture (prompts via sudo -v)
#     -> writes docs/issues/session25b/{spike.log,devcd.bin,dmesg.log,canary.log}

set -u

SPIKE=./build/libdrm_store_spike_v3
CL_PROBE=./build/shader/cl_probe
OUTDIR=docs/issues/session25b
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
