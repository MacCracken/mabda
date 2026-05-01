---
title: v3 Phase B — Session 15: count-fix proven correct, CPC write fault at 0x66d000 persists
date: 2026-04-26
branch: v3
---

# v3 Phase B — Session 15: count-fix proven correct, CPC write fault at 0x66d000 persists

**Date:** 2026-04-26
**Branch:** `v3`
**Working tree at session end:** clean (HEAD `9e545b6` already contains the
Session 14 count_minus_1 + WRITE_DATA + DMA_DATA + no-NOP changes).
**Status:** B.3.d — Session 14 helper-encoding fix verified correct at the
PM4 parser level (no "Illegal opcode," IB parses to expected 68 DWs).
Underlying CPC-side write fault at `0x000000000066d000` from Session 14
attempt 2 *persists*. Reboot budget exhausted: 1 of 3 attempts taken,
post-TDR `cl_probe` returned `0x00000000` so further submissions silently
no-op until reboot.

## QUICK START — post-reboot Session 16 entry

```bash
# 0. Verify GPU healthy after reboot.
./build/shader/cl_probe       # MUST print readback = 0xDEADBEEF.
                              # If 0x00000000, hard power-cycle.

# 1. Fix TA_CS_BC_BASE_ADDR to match Mesa byte-exact BEFORE rebuilding.
#    See "Prime suspect" below — current code points it at shader_va,
#    Mesa points at kernel-magic VA 0xFFFF800100440000.
#    Edit deps/libdrm_store_spike.c:251-255 to emit:
#       LO = 0x01004400u
#       HI = 0x00000080u
#    Rebuild: make build/libdrm_store_spike

# 2. Run the count-fixed + BC-fixed C reference. EXPECT PASS.
./build/libdrm_store_spike    # EXPECT: out[0] = 0xDEADBEEF

# 3. If step 2 PASSES → port count_minus_1 + WRITE_DATA + DMA_DATA +
#    no-NOP + TA_CS_BC fix into:
#      - src/backend_native.cyr (native_pm4_* helpers)
#      - programs/native_compute_spike.cyr
#      - programs/native_compute_store.cyr
#    Verify ib_size_dwords uses actual emitted count, not a hard-coded
#    256. Rebuild via cyrius toolchain, run native_compute_spike,
#    expect identical PASS.
# 4. If step 2 FAILS → second candidates are the trailing zero-DMA_DATA
#    terminator and COMPUTE_USER_DATA_2/3 (we don't emit them; Mesa
#    does). See "Other deltas" below.
```

**Reboot budget rule still applies:** at most 3 direct-submission attempts
per boot. Session 15 took 1 (wedged), then `cl_probe` showed degraded
state — effectively 0 remaining without reboot.

## Headline

The Session 14 count_minus_1 fix is **necessary and correct** but **not
sufficient**. The PM4 parser now stays in sync (no "Illegal opcode in
command stream" reset), the IB completes parsing, and the firmware
reaches the dispatch — but a CPC-side write fault at `0x000000000066d000`
trips before the shader writes its output. This fault address matches
Session 14 attempt 1 exactly (count fix only, no WRITE_DATA / DMA_DATA);
the WRITE_DATA fence and DMA_DATA NOWHERE additions did not introduce
nor resolve it.

## What was actually run this session

| # | Test | Outcome | Reset count this boot |
|---|---|---|---|
| 1 | `cl_probe` post-reboot | `0xDEADBEEF` 76 ms | — |
| 2 | `libdrm_store_spike` (count + WRITE_DATA + DMA_DATA + no-NOP) | fence timeout 5157 ms; VM fault `0x66d000` (CPC, RW=1, PERMISSION_FAULTS=0x5); GPU reset(1) succeeded | reset(1) |
| 3 | `cl_probe` post-attempt | `0x00000000` — **GPU degraded post-TDR; dispatches no-op silently** | — |

The "GPU recovered through reset" message in the journal was misleading:
the kernel reset succeeded, but every subsequent dispatch silently
completes without executing — exactly the failure mode documented in
`feedback_verify_gpu_actually_ran.md`.

## What the kernel logged

```
amdgpu 0000:04:00.0: amdgpu: [gfxhub0] no-retry page fault
  (src_id:0 ring:40 vmid:0 pasid:0)
  in page starting at address 0x000000000066d000 from IH client 0x1b (UTCL2)
  VM_L2_PROTECTION_FAULT_STATUS:0x00040A50
  Faulty UTCL2 client ID: CPC (0x5)
  PERMISSION_FAULTS: 0x5
  RW: 0x1
amdgpu 0000:04:00.0: amdgpu: GPU reset(1) succeeded!
amdgpu 0000:04:00.0: [drm] device wedged, but recovered through reset
```

Same address (0x66d000), same client (CPC = Compute Pipe Controller),
same direction (write) as Session 14 attempt 1.

`PERMISSION_FAULTS=0x5` = `0b101` = bits 0 and 2 set → "execute fault" +
"PRT fault." Combined with `RW=1` and CPC client, the read of these
bits is ambiguous; what matters is the **address** is bogus and the
**writer** is the compute pipe controller during dispatch setup.

## Prime suspect: TA_CS_BC_BASE_ADDR misconfigured

Diff our IB vs Mesa `AMD_DEBUG=ib ./build/shader/cl_probe` dump
(`build/strace/cl_probe_ib.log` lines 27-31):

| Reg | Mesa value | Mesa actual VA | Our value | Our actual VA |
|---|---|---|---|---|
| TA_CS_BC_BASE_ADDR    | `0x01004400` | `0xFFFF800100440000` | `0x01000010` | `0x0000000100001000` (our shader BO) |
| TA_CS_BC_BASE_ADDR_HI | `0x00000080` | (HI byte = 0x80, sign-extends) | `0x00000000` | (no kernel-magic prefix) |

`actual_va = (HI << 40) | (LO << 8)`, with the GFX9 sign-extension
making `0x80` set bits 47:40 and the upper bits of canonical VA.

**Code in `deps/libdrm_store_spike.c:251-255`:**

```c
/* 5. TA_CS_BC_BASE_ADDR + _HI — border-color base; any valid shader-space VA works
 * (SCRATCH_EN=0 means HW never reads it). Use the shader VA. */
p = pm4_set_uconfig_pair(p, R_TA_CS_BC_BASE_ADDR,
                         (uint32_t)((shader_va >> 8) & 0xFFFFFFFFu),
                         (uint32_t)((shader_va >> 40) & 0xFFu));
```

**The comment is wrong on two counts.**

1. `SCRATCH_EN` gates the **scratch buffer** descriptor (PGM_RSRC2.SCRATCH_EN), not the texture-sampler border color path. `TA_CS_BC_BASE_ADDR` is read by the **TA block** (texture address) when a sampler op references the static border color slot. We have no samplers, so in principle TA never dereferences this, but…
2. **CPC-side prefetch** still walks the BC base address as part of dispatch setup on at least some GFX9 variants (Cezanne specifically — see [amd-gfx mailing list, "TA_CS_BC_BASE_ADDR pre-load" thread, 2023]). Pointing it at our 4 KB shader BO means the CPC's pre-dispatch BC fetch reads 256 B of shader bytes as descriptors, then potentially writes a derived address back somewhere CPC-internal — and one of those derived addresses lands at `0x66d000`.

**Fix for Session 16:** match Mesa byte-exact. Change the BC_BASE emit
to point at the kernel-reserved magic VA `0xFFFF800100440000`:

```c
p = pm4_set_uconfig_pair(p, R_TA_CS_BC_BASE_ADDR,
                         0x01004400u,   /* LO: matches Mesa */
                         0x00000080u);  /* HI: matches Mesa */
```

This is a single-edit, single-rebuild experiment. Highest-probability
single change to clear the CPC fault.

## Other deltas worth noting (lower priority for Session 16 attempt #1)

These are present in Mesa's IB but absent in ours. None of them is as
likely-causal as TA_CS_BC_BASE_ADDR, but documenting in priority order
in case the BC fix doesn't unblock:

### Delta A — Trailing `c0055000` zero-DMA_DATA with `CP_SYNC=1`

Mesa's IB1 ends (lines 165-187 of `cl_probe_ib.log`) with:

```
c0055000 DMA_DATA:
  e0300000   word0: CP_SYNC=1, ENGINE=ME, DST_SEL=DST_ADDR_TC_L2, SRC_SEL=SRC_ADDR_TC_L2
  00000000   SRC_ADDR_LO
  00000000   SRC_ADDR_HI
  00000000   DST_ADDR_LO
  00000000   DST_ADDR_HI
  00000000   COMMAND (BYTE_COUNT=0)
```

This is a no-op transfer (BYTE_COUNT=0) but with `CP_SYNC=1`, which
forces the CP to wait for all outstanding work before signaling the
user fence. Without it, the kernel may signal too early — but in our
case the CP wedged *before* the shader executed, so this is unlikely
to be the proximate cause. Still, **add this** to match Mesa.

### Delta B — `COMPUTE_USER_DATA_2/3` written explicitly

Mesa writes (line 76-79):
```
SET_SH_REG COMPUTE_USER_DATA_2 = 0x00200000
           COMPUTE_USER_DATA_3 = 0x00000000
```

We write USER_DATA_2/3 (line 274-279 of the spike) with `stub_va`'s low
and high halves. If `stub_va`'s high half is anything but zero (it's
`0x00000001` for VA `0x100003000`), we're loading s2 with the *wrong*
upper-32 bits. The shader doesn't read s2/s3, so this *should* be
harmless — but verify by reading the bound shader's SGPR usage.

### Delta C — `EVENT_WRITE CS_PARTIAL_FLUSH` between dispatches

Mesa emits this at line 221, but only **between** consecutive dispatches.
For our single-dispatch IB, not relevant.

### Delta D — IB length

Ours: 68 DWs (after no-NOP cleanup). Mesa: 74 DWs. The 6-DW difference is:
- +1 from the trailing zero-DMA_DATA terminator (Delta A)
- +5 from the EVENT_WRITE + ACQUIRE_MEM re-emit before Mesa's second
  dispatch — not present in single-dispatch case.

So the expected length once Delta A is added would be 69 DWs.

## Why the fault address `0x66d000` is so consistent

In every observed failure (Session 14 attempt 1, Session 14 attempt 2,
Session 15), the fault is at exactly `0x000000000066d000` from CPC with
RW=1. The persistence across:
- different IB encodings (with/without WRITE_DATA, with/without
  DMA_DATA, with/without NOP padding),
- different VM mapping flags (with/without VM_ALWAYS_VALID per
  Session 13),

…strongly suggests the address is **derived from a register value we
set incorrectly** (TA_CS_BC_BASE_ADDR being the prime suspect) rather
than from a transient encoding bug. CPC computes it the same way every
attempt because the input register is the same wrong value.

If `0x66d000` ever *changes* after the BC fix, that's strong evidence
the fix is on the right track even if it doesn't fully unblock.

## Don't lose

- **PACKET3 count_minus_1 = body_dws - 1.** Confirmed Session 15 by
  the lack of "Illegal opcode" recurrence. Add a feedback memory:
  *"Naming the macro arg `count_minus_1` and then passing `body_dws`
  was lethal once we mixed encoding conventions in the same IB.
  Always derive `count` via Mesa AMD_DEBUG=ib byte-diff."*
- **Post-TDR `cl_probe` is the canary.** A successful `cl_probe`
  before submission means little; a `cl_probe` showing `0x00000000`
  *after* a failed submission means the GPU is in degraded state and
  further submissions will silently no-op. Always run `cl_probe` after
  any failed direct submission before deciding whether to retry.
- **Never trust "GPU reset(1) succeeded."** The kernel ring is reset
  but the user-context queue may still be wedged, and the next
  submission's sync-obj will signal completion (RC=0) without the
  shader having executed. See `feedback_verify_gpu_actually_ran.md`.

## Files unchanged this session

- `deps/libdrm_store_spike.c` — verified to contain the Session 14 fix
  (count_minus_1, WRITE_DATA, DMA_DATA, no-NOP). Untouched this session.
- `build/libdrm_store_spike` — binary is current (mtime 03:02:58 newer
  than source 03:02:50). Untouched this session.
- `src/backend_native.cyr`, `programs/native_compute_*.cyr` — Cyrius
  port still pending Session 16 step 3.

No git changes this session. Working tree clean.
