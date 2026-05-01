---
title: Session 21 — 0x66d000 fault root-caused (USER_DATA_0/1 packet shape); new HQD-deactive hang revealed
date: 2026-04-27
session: 21
branch: v3
hardware: AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family), Linux 6.18.24-lts
toolchain: cyrius 5.7.12
status: BREAKTHROUGH — fault eliminated; new layer of hang exposed
---

# Summary

**The recurring `0x66d000 / CPC / vmid:0` fault is gone.** Root cause:
emitting `USER_DATA_0` and `USER_DATA_1` as **two separate**
`SET_SH_REG count=1` packets triggered a Cezanne MEC firmware quirk
that derived a kernel-VM write to 0x66d000 during dispatch. Packing
them as a **single `SET_SH_REG count=2`** packet (matching Mesa
cl_probe IB1 byte-exactly) eliminates the fault completely.

This is the bug that consumed Sessions 14–20.

What's now exposed: the dispatch on `comp_1.1.0` **hangs without
faulting**. The kernel detects the timeout, attempts a soft ring
reset (deactivate HQD + reactivate), the deactivate fails
(`fail to wait on hqd deactive`), and the kernel escalates to MODE2
reset. Confirmed identical with the bare `s_endpgm` shader, so the
hang is **not** in shader execution — MEC accepts the IB but the
dispatch never completes (or MEC firmware locks immediately upon
seeing our IB tail).

# What was attempted

| # | Change | Outcome |
|---|--------|---------|
| 1 | `AMDGPU_HW_IP_GFX` instead of `COMPUTE` | **Different** failure mode: `ring gfx timeout, signaled seq=1, emitted seq=2`, **NO UTCL2 fault**, MODE2 reset. Confirms 0x66d000 is MEC/CPC-specific. |
| 2 | Pack USER_DATA_0/1 as `SET_SH_REG count=2` (single packet, two values) instead of two `count=1` packets | **0x66d000 fault GONE**. New mode: `comp_1.1.0 timeout, signaled=32, emitted=33`, `fail to wait on hqd deactive`, MODE2 reset. No page fault logged. |
| 3 | Replace 9-DW store kernel with bare `s_endpgm` | **Same hang** as #2. Wave-engine never completes. Hang is upstream of shader execution. |

cl_probe canary survived all attempts — no permanent damage between
runs, just per-attempt MODE2 resets.

# Why this fix is the right one

The kernel-side mechanism is plausible: GFX9 MEC firmware has an
internal state machine for `SET_SH_REG` packets. When it sees
consecutive `count=1` writes targeting USER_DATA_0 and USER_DATA_1
(i.e., adjacent compute persistent SGPRs), the second packet may
trigger a different code path than a single `count=2` write — and on
gfx90c (Renoir/Cezanne) that path computes a derived VA somewhere in
its CPC bookkeeping that resolves to 0x66d000 in kernel VMID 0,
which is unmapped, hence the fault.

This explains everything we observed in Sessions 14–20:
- Fault was **fixed-VA** (0x66d000) → kernel-internal computation, not user-controlled.
- Fault was **kernel VMID 0** → CPC firmware operating with its own VM, not user's.
- Fault was **independent of submit path** → triggered by IB content, not submit shape.
- Fault appeared with bare `s_endpgm` (Session 18 attempt 5) → triggered before/independent of wave execution.
- Fault was **byte-exact identical across submits** → packet-shape match, not state.

Mesa rusticl avoids it because radeonsi's `si_emit_compute_user_sgprs`
always emits user SGPR loads as `count >= 2` packets — it's a generic
optimization (one PM4 header instead of N), and a side-effect benefit
is dodging this firmware quirk. Our spike's hand-coded preamble used
two `count=1` packets because the helpers were the simpler path.

# What's now blocking dispatch

`comp_1.1.0 timeout, signaled=32, emitted=33` — kernel emitted fence
sequence 33 after our IB, but only seq 32 ever signaled. The wave
either:

1. **Never started** — CPC accepted the IB packets, started processing
   the dispatch packet, but couldn't initiate the wave (HQD or MQD in
   wrong state).
2. **Started but hung in s_waitcnt** — but bare `s_endpgm` has no
   waitcnt and still hung, so this is unlikely.
3. **Completed but the IB never signaled the kernel-emitted EOP fence**
   — MEC firmware lockup while emitting EOP packet.

The `fail to wait on hqd deactive` line points at #1 or #3: kernel
asks MEC "deactivate this HQD"; MEC can't respond because it's stuck.

# Codebase state at session end

- `deps/libdrm_store_spike.c` — modified:
  - Modern submit path (Session 20 carried over).
  - **USER_DATA_0/1 packed as single `SET_SH_REG count=2`** (Session 21 fix).
  - `ip_type` reverted to `AMDGPU_HW_IP_COMPUTE` (we tested GFX briefly).
  - Shader code restored to the 9-DW store kernel (the bare-s_endpgm
    probe was a temporary diagnostic).
  - **Uncommitted.** Session 22 should commit before any further changes.
- `dist/mabda.cyr` — no API change, no regen needed.
- Cyrius backend (`src/backend_native.cyr`, `programs/native_compute_*.cyr`)
  untouched; the packet-shape fix is in C, will need to be ported back.

# Reboot status

GPU reset count: **9** (started this boot at ~5; 4 ring timeouts this
session). Well past policy of 3 MODE2 / boot. Reboot recommended
before Session 22, even though cl_probe canary still passes.

# Where Session 22 should look

The HQD/dispatch-doesn't-complete bug. Order of cheapness:

1. **Audit our PRE-dispatch register sequence vs Mesa IB1 byte-exact.**
   With USER_DATA fixed, the remaining DW deltas (per Session 20 diff
   reconstruction) are PGM_LO/USER_DATA value differences only. Re-run
   the diff to confirm we're now structurally identical.
2. **Add CS_PARTIAL_FLUSH event before DISPATCH_DIRECT.** Mesa IB2 has
   one but not IB1; could try inserting it just before our dispatch to
   force MEC to drain pending state.
3. **Submit Mesa IB1 byte-exact** (with Mesa's literal USER_DATA values
   even though they point at non-mapped VAs). The shader will hang
   silently in s_waitcnt with bad pointers, but if the kernel-side
   dispatch path completes, we know the IB content is OK and the issue
   is in our buffer setup.
4. **Try non-default ring** by setting `.ring = 1` or higher in the IB
   chunk (we currently hit `comp_1.1.0` always; the kernel scheduler
   may map differently with explicit ring index).
5. **Read `/sys/class/drm/card0/device/devcoredump/data`** (kernel
   wrote one per the dmesg). This file contains the full CP/MEC state
   dump at fault time — register values that show what CPC was trying
   to do when it locked up.

The packet-shape fix is the **headline finding** — port it to
`backend_native.cyr` once Session 22 unblocks dispatch.

**Until Session 22 produces a real-dispatch confirmation, do not
treat Phase B.3 as done. Do not present mabda as having a working
native backend. Do not quote dispatch latency.**
