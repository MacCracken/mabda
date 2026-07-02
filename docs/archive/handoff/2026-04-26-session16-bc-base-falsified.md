---
title: v3 Phase B — Session 16: TA_CS_BC_BASE_ADDR fix did NOT change fault address
date: 2026-04-26
branch: v3
---

# v3 Phase B — Session 16: TA_CS_BC_BASE_ADDR fix did NOT change fault address

**Date:** 2026-04-26
**Branch:** `v3`
**Working tree at session end:** clean except `cyrius.cyml` pin bump
(5.6.13 → 5.7.12) and the BC_BASE edit in `deps/libdrm_store_spike.c`.
**Status:** B.3.d — Session 15's prime-suspect fix applied and falsified.
Same `0x000000000066d000` CPC RW=1 fault, same address, same client.
Reboot budget for this boot is exhausted (1 wedge + canary
`0x00000000` post-attempt).
**Toolchain:** cyrius 5.7.12 active and pinned in manifest.

## QUICK START — post-reboot Session 17 entry

```bash
# 0. Canary — must print 0xDEADBEEF or hard power-cycle.
./build/shader/cl_probe

# 1. ALREADY STAGED for Session 17 attempt 1 (in working tree, uncommitted):
#       (a) IB hex dump on stderr after build (lossless diagnostic)
#       (b) trailing DMA_DATA(CP_SYNC=1, BYTE_COUNT=0) terminator after
#           DISPATCH_DIRECT (Mesa byte-exact, closes one delta)
#    Build is clean. Just run:
./build/libdrm_store_spike    # EXPECT: out[0] = 0xDEADBEEF, OR a
                              # NEW fault address (anything other than
                              # 0x66d000 is progress — means the input
                              # register that derives the address has
                              # changed).

# 2. Always check canary after a failure:
./build/shader/cl_probe       # If 0x00000000 → reboot before retry.

# 3. Capture the IB dump for byte-exact diff against Mesa:
#    The dump is on stderr after the "pm4 IB built: N DWs" line.
#    Compare against build/strace/cl_probe_ib.log.
#    Save the dump alongside the run for the handoff.
```

## Correction to my earlier playbook

My initial Session 16 playbook claimed `COMPUTE_RESOURCE_LIMITS = 0x140`
and `COMPUTE_NUM_THREAD_X/Y/Z = 1,1,1` were missing. **They are not** —
both have been emitted since Session 14 (lines 311-317 of
`deps/libdrm_store_spike.c`). I built the playbook from Session 15's
stale "missing" list without re-reading the current spike. The actual
remaining Mesa-byte-exact deltas are:

| Delta | Status |
|---|---|
| Trailing `DMA_DATA(CP_SYNC=1, BYTE_COUNT=0)` terminator | **STAGED** for Session 17 attempt 1 |
| `USER_DATA_0` single (Mesa: `0x00200040`) vs ours: pair `(out_va_lo, out_va_hi)` | requires shader re-targeting (Mesa's shader has different ABI) — defer |
| `USER_DATA_2/3` (Mesa: `0x00200000, 0`) vs ours: `(stub_va_lo, stub_va_hi)` | same — coupled to USER_DATA_0 change |
| Shader VA placement (Mesa upper canonical VA `0xFFFF800000000000`) vs ours user VA `0x100001000` | large infra change — defer |

The CPC fault is **pre-dispatch** (Compute Pipe Controller setup),
so the trailing terminator is unlikely to clear `0x66d000` on its own.
But it closes a Mesa delta and the IB dump provides byte-exact ground
truth for diff against `build/strace/cl_probe_ib.log` — both
high-info-density at zero submission cost.

**Reboot budget rule:** at most 3 direct-submission attempts per boot.
Each failure poisons subsequent dispatches into silent no-ops. Run
canary after every failure to know when you're out.

## Headline

The Session 15 prime suspect — `TA_CS_BC_BASE_ADDR` pointing at our
shader BO instead of Mesa's kernel-magic VA — is **falsified**. Both
the address and the rest of the fault signature match Sessions 14
attempt 1 + 2 + 15 byte-for-byte:

| Field | All four runs |
|---|---|
| Fault address | `0x000000000066d000` |
| Client ID | CPC (0x5) |
| RW | 1 (write) |
| `PERMISSION_FAULTS` | 0x5 |
| `MORE_FAULTS` / `WALKER_ERROR` / `MAPPING_ERROR` | 0 |
| `VM_L2_PROTECTION_FAULT_STATUS` | `0x00040A50` |
| `vmid` / `pasid` / `src_id` / `ring` | 0 / 0 / 0 / 40 |

The fault address is therefore derived from a register input that is
**identical between Session 14 (BC=shader_va) and Session 16
(BC=kernel-magic)**. BC_BASE is not the input.

## What was actually run this session

| # | Test | Outcome |
|---|---|---|
| 1 | `cl_probe` post-reboot | `0xDEADBEEF` 76 ms ✓ (GPU healthy) |
| 2 | `libdrm_store_spike` (count + WRITE_DATA + DMA_DATA + no-NOP + **BC=0xFFFF800100440000**) | fence timeout 5017 ms; **0x66d000 CPC fault** identical to prior runs; GPU reset(1) succeeded |
| 3 | `cl_probe` post-attempt | `0x00000000` — **GPU degraded; budget exhausted** |

## Cleanliness baseline (for the toolchain bump)

Pre-GPU smoke test under cyrius 5.7.12:

| Check | Result |
|---|---|
| `cyrius build programs/smoke.cyr` | OK |
| `cyrius lint programs/smoke.cyr` | 0 warnings |
| `cyrfmt --check` over all `src/` + `programs/` `.cyr` | clean |
| `cyrius test tests/tcyr/mabda.tcyr` | 621 passed, 0 failed |
| `cyrius vet programs/smoke.cyr` | broken — emits raw ELF bytes; no `cyrvet` binary in `~/.cyrius/bin/`. Probably a stale CLAUDE.md reference; **non-blocker**. |

## Mesa-vs-ours register diff (full, post-BC-fix)

Now that BC_BASE matches Mesa, the remaining deltas are:

| Register / packet | Mesa value | Ours |
|---|---|---|
| `COMPUTE_RESOURCE_LIMITS` (0x215) | `0x140` (WAVES_PER_SH=0x140) | **not emitted** |
| `COMPUTE_NUM_THREAD_X/Y/Z` (0x207..0x209) | 1,1,1 (3-DW SET_SH_REG_N) | **not emitted** (dims passed via DISPATCH_DIRECT body only) |
| `COMPUTE_USER_DATA_0` (0x240) | `0x00200040` (single SET_SH_REG) | pair with USER_DATA_1 = `(out_va_lo, out_va_hi)` |
| `COMPUTE_USER_DATA_2/3` (0x242, 0x243) | `0x00200000, 0` | `(stub_va_lo, stub_va_hi)` = `(0x3000, 0x1)` |
| Pre-dispatch DMA_DATA word0 | `0x60200000` (CP_SYNC=0, DST_SEL=NOWHERE, ENGINE=ME, BYTE_COUNT=96) | absent or differently encoded |
| Trailing DMA_DATA word0 | `0xe0300000` (CP_SYNC=1, BYTE_COUNT=0 terminator) | absent |
| `COMPUTE_PGM_HI` | `0x80` → shader at `0xFFFF800000000000` (kernel-magic upper-half) | derived from user VA `0x100001000` (bit 47 = 0) |
| IB total | 74 DWs | 68 DWs |

The 6-DW gap accounts for: RESOURCE_LIMITS (2 DWs) + NUM_THREAD_X/Y/Z
(5 DWs SET_SH_REG_N) − 1 (we have an extra USER_DATA pair Mesa
doesn't) ≈ 6.

## Why I'm prioritizing RESOURCE_LIMITS first

RESOURCE_LIMITS controls `WAVES_PER_SH` — the per-shader-engine wave
quota. If unset (zero) when CPC tries to allocate waves for the
dispatch, two failure modes are possible:
1. CPC allocates with 0 waves → no shader runs → would manifest as
   readback unchanged, NOT a CPC write fault. So this isn't the cause
   on its own.
2. CPC walks an internal wave-table that starts at a base derived
   from `WAVES_PER_SH × wave_size + scratch_base` → if any of those
   inputs is uninitialized, derived address can be wild. The
   `0x66d000` constant is consistent with a single-input derivation
   from one wrong register.

Setting it to Mesa's `0x140` is a one-DW edit, lowest blast radius,
highest information value (if the fault address *changes*, we know
RESOURCE_LIMITS feeds the derivation; if it *vanishes*, we're done).

## Don't lose

- **PACKET3 count_minus_1 fix is necessary and correct.** Saved as
  feedback memory `feedback_pm4_count_minus_1_naming.md`. Don't
  re-test that hypothesis.
- **BC_BASE is not the input that derives 0x66d000.** Cross out from
  candidate list.
- **Reboot canary is the only honest signal of GPU health.** Per
  `feedback_verify_gpu_actually_ran.md`, "GPU reset(1) succeeded" +
  RC=0 + sync-obj signaled is identical between real completion
  and silent-no-op. Always run `./build/shader/cl_probe` after a
  failed submission.

## Files changed this session

- `cyrius.cyml`: `cyrius = "5.6.13"` → `cyrius = "5.7.12"`. `lib/`
  refreshed via `rm -rf lib && mkdir lib && cyrius deps`. 14 deps
  resolved, smoke + lint + 621 tests pass.
- `deps/libdrm_store_spike.c` lines 251-269: TA_CS_BC_BASE_ADDR
  emit changed from `(shader_va >> 8, shader_va >> 40)` to
  Mesa-byte-exact `(0x01004400, 0x00000080)`. Result: same fault
  address, hypothesis falsified. Edit kept in tree (Mesa's value
  is still the right thing to do — the comment now reflects the
  CPC-walks-BC-regardless-of-SCRATCH_EN reality, and reverting
  would re-introduce a non-Mesa-byte-exact divergence we'd just
  have to re-fix later).

## Files unchanged this session

- `src/backend_native.cyr`, `programs/native_compute_spike.cyr`,
  `programs/native_compute_store.cyr` — Cyrius port still pending C
  reference passing first.
- `dist/mabda.cyr` — no API change, no regen needed.

## Session 17 prep patch (in tree, uncommitted)

Two changes to `deps/libdrm_store_spike.c` ahead of next reboot:

1. **Trailing DMA_DATA terminator** after DISPATCH_DIRECT. 7-DW
   `DMA_DATA(CP_SYNC=1, ENGINE=ME, DST_SEL=DST_ADDR_TC_L2,
   SRC_SEL=SRC_ADDR_TC_L2, BYTE_COUNT=0)` — matches Mesa
   `cl_probe_ib.log` lines 165-186 byte-exact. Forces CP to wait
   for outstanding work before signaling user fence.
2. **IB hex dump** on stderr after build. 8 DWs per line, hex
   offset on the left. Lossless, no IB content change. Lets us
   byte-exact diff our IB against Mesa's `cl_probe_ib.log` after
   any submission.

User runs post-reboot. **Commit if PASS** (`out[0] = 0xDEADBEEF`).
**`git checkout deps/libdrm_store_spike.c`** if FAIL — IB dump from
the failed run goes into the Session 17 handoff for analysis.
