---
title: Session 22 — 0x66d000 CPC fault is NOT eliminated; Session 21 hypothesis falsified
date: 2026-04-27
session: 22
branch: v3
hardware: AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family), Linux 6.18.24-lts
toolchain: cyrius 5.7.12
status: REGRESSION CONFIRMED — Session 21 fix did not eliminate the fault
---

# Summary

**Session 21 was wrong.** The 0x66d000 CPC / vmid:0 / RW=1 fault is
still firing after the USER_DATA_0/1 packet-shape fix landed in
commit 6e0ea08. Confirmed on a fresh, post-reboot, healthy-canary GPU.
Fault signature is byte-identical to Sessions 14–20.

Session 21's "fault gone" reading was a measurement artifact: their
submission landed at fence seq~33 on a hot queue with multiple prior
submits, and the fault interrupt got coalesced into the MODE2 reset
cycle. On a cold-boot first-submission (seq=4/5 here), the IH stream
reports the fault clearly.

The packet-shape fix was still **good hygiene** (Mesa byte-exact for
that packet shape — even though Mesa's USER_DATA pattern is actually
count=1 single value, see below) but it did not fix the bug.

# What was done this session

| # | Action | Result |
|---|--------|--------|
| 1 | cl_probe canary on fresh boot | 0xDEADBEEF, GPU healthy |
| 2 | Rebuild + run libdrm_store_spike (current HEAD = 6e0ea08) | syncobj timeout 5145 ms; out[0] = 0xBAADF00D |
| 3 | Capture dmesg | **Identical 0x66d000 / CPC / vmid:0 fault, RW=1, PERMISSION_FAULTS=0x5** |
| 4 | cl_probe canary post-MODE2 | readback = 0x00000000 → comp_1.2.0 also wedged → reboot needed |

Captured IB hex (75 DWs) saved at `docs/issues/2026-04-27-session22-spike-ib.log`.

# Verified facts (refines what we knew after Session 21)

1. **Fault is post-IB-content, ring-related.** Address 0x66d000 is
   fixed across all sessions, all IB content variations, and all
   submit paths (legacy CS, raw2 + syncobj, GFX vs COMPUTE ip_type).
   Faulty client = CPC. VMID = 0 (kernel). RW=1.

2. **Mesa's `cl_probe` IB1 does NOT match Session 21's claim.**
   Re-reading `build/strace/cl_probe_ib.log`, Mesa emits
   `USER_DATA_0` as `count_minus_1=1` (header `c0017600`, body=2:
   offset DW + 1 value). This is **count=1, single value** — not
   count=2 with two values as Session 21 reported. Mesa never sets
   `USER_DATA_1` at all.

   Mesa's pattern uses a different shader ABI (kernel-arg-buffer
   pointer in s0, args loaded via s_load_dwordx2). Our shader ABI
   needs the full 64-bit pointer in s0:s1, so we do need to set both
   USER_DATA_0 and USER_DATA_1.

3. **Our IB is structurally Mesa-byte-exact for everything except**:
   - `USER_DATA_0/1` (count=2 packet, vs Mesa's count=1 single
     value). This is the trade-off that goes with our shader ABI.
   - PGM_LO value differs (we have `0x01000000` because shader_va
     is at `0xFFFF800100000000`; Mesa has `0` because shader_va is
     at `0xFFFF800000000000` exactly). Both reconstruct correctly
     to the actual mapped shader VA via PGM_HI<<40 | PGM_LO<<8.
   - USER_DATA_2/3 values (we set stub_va; Mesa sets 0x00200000/0
     which is its own kernel-arg buffer descriptor).

4. **Same fault occurs with bare `s_endpgm` shader** (Session 18
   attempt 5). So the fault is not in shader execution. It is in
   CPC's pre-dispatch wave-init or post-dispatch CSA save.

5. **Fault is ring-related.** Our submissions land on `comp_1.1.0`
   (kernel scheduler choice). Mesa's cl_probe lands on `comp_1.2.0`.
   We have not yet tested whether forcing a specific
   `ip_instance` / `ring` index avoids the fault.

# Why the bug survived 8 sessions of investigation

- Session 14 fixed PACKET3 count_minus_1 — necessary fix, real bug,
  did not eliminate 0x66d000.
- Session 16 fixed `TA_CS_BC_BASE_ADDR` (was pointing at shader_va,
  causing CPC to walk shader bytes as descriptors). Necessary fix,
  but the fault address was 0x66d000 even *with* the bad BC_BASE,
  because BC_BASE feeds a different code path. The fix didn't change
  the fault.
- Session 17 hypothesized shader VA half (lower vs upper canonical).
  Switched to `AMDGPU_VA_RANGE_HIGH`. Fault stayed.
- Session 18 ruled out wave-execution as the cause (bare s_endpgm
  also faults).
- Session 19 hypothesized submit-path (legacy ioctl shape). Switched
  to raw2 + syncobj. Fault stayed.
- Session 20 hypothesized fence VA in legacy submit. Same change as
  Session 19. Fault stayed.
- Session 21 hypothesized USER_DATA_0/1 packet-shape (two count=1
  packets vs single count=2). Made the change. Reported "fault
  gone" — **measurement artifact, not real fix**.

# Session 23 — what to test next

Order of cheapness, all from the same starting code (HEAD = 6e0ea08):

## Priority 1 — Read devcoredump

The kernel writes `/sys/class/drm/card0/device/devcoredump/data` (4 KB)
at fault time. File is root:0600 and **the kernel auto-deletes it
after a timeout (~5 min default)**. Session 22 missed the window
trying to read it after the spike + canary already finished.

The matrix runner now snapshots it automatically right after each
fault. If running the spike directly, capture immediately:

```bash
sudo cat /sys/class/drm/card0/device/devcoredump/data > /tmp/devcd.bin
```

This dump contains:
- All MEC SR/MQD register state at fault time
- IB pointer the CPC was processing (so we can see EXACTLY which
  packet was being processed when the fault hit)
- Faulting wave's PC (if a wave was active)
- HQD configuration

This is the highest-information move. Until now we've been guessing
based on the dmesg one-line summary; the devcoredump tells us the
register state.

## Priority 2 — Force `ring=2` and `ip_instance=1`

If the fault is ring-specific (some MEC pipe/queue has a stale MQD
in vmid 0 due to a kernel bug or boot-time setup quirk), forcing a
different ring may dodge it.

```c
struct drm_amdgpu_cs_chunk_ib ib_chunk_data = {
    ...
    .ip_instance = 1,
    .ring = 2,
};
```

Test the matrix: ip_instance ∈ {0, 1}, ring ∈ {0, 1, 2, 3} →
8 combinations. If one works → ring-specific MQD bug. If all fault
at 0x66d000 → not ring.

## Priority 3 — Try GFX ip_type with full graphics-context preamble

Session 21 attempt 1 tried `AMDGPU_HW_IP_GFX` and got
`ring gfx timeout` with NO UTCL2 fault. That's interesting — the
fault is CPC-specific. If we can get a working dispatch via GFX
(needs more registers — DB_RENDER_CONTROL, CB stuff, etc, even for
a no-FB compute-on-gfx pass), it sidesteps CPC entirely.

This is more work, but it's a meaningful escape hatch.

## Priority 4 — KMS path with `--force-mec-pipe`

Examine kernel `gfx_v9_0_compute_mqd_init` to see what the MQD writes
at queue setup. If the MQD's user-visible region maps somewhere with
0x66d000 in vmid 0... that'd nail it.

`/sys/kernel/debug/dri/0/amdgpu_mqd` (if exists) might dump the MQD
of each compute queue.

## Priority 5 — Try a single-process kernel-managed-queue (KCQ) bypass

KCQ-bypass means: instead of submitting via `amdgpu_cs_submit_*`, use
the user-mode-queue (UMQ) interface in newer kernels. This skips the
HWS/MEC-managed path. Cezanne kernel 6.18 might not support UMQ for
compute though — needs verification.

# Important: stop running on the wedged GPU

The GPU is now in post-MODE2 degraded state. cl_probe canary returns
`readback = 0x00000000`. comp_1.2.0 also wedged. **Reboot required
before Session 23 starts running test code.**

Reset count: ~2 this session (1 from spike, 1 from canary retry).
Combined with Session 21's residual = ~11 lifetime resets.

# Codebase state at session end

- `deps/libdrm_store_spike.c` — unchanged, current HEAD = 6e0ea08
- `dist/mabda.cyr` — clean
- Cyrius backend (`src/backend_native.cyr`) — unchanged
- New files this session:
  - `docs/handoff/2026-04-27-session22-66d000-fault-still-present.md` (this file)
  - `docs/issues/2026-04-27-session22-spike-ib.log` (75-DW IB hex from this session's run)

# Calibration note for Session 23

When evaluating "did the fault go away", do not trust dmesg-grep alone.
The IH stream can coalesce events around MODE2 reset cycles.
Confirm with **all** of:

1. `journalctl -k --since "Xmin ago" | grep -E "0066d000|66d000"` → no hits
2. `out[0] == 0xDEADBEEF` (real readback, not just RC=0)
3. cl_probe canary still passes after the spike run (no shared-state wedge)

Anything less is hopium.

**Until Session 23 produces a real out[0]=0xDEADBEEF readback with a
clean dmesg, do not present mabda native dispatch as working. Do not
quote latency. Phase B.3 is not done.**
