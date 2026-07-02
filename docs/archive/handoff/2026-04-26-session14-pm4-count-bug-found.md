# v3 Phase B — Session 14: ROOT CAUSE #2 FOUND (PM4 count_minus_1 off-by-one)

**Date:** 2026-04-26
**Branch:** `v3`
**Working tree at session end:** `deps/libdrm_store_spike.c` modified (count_minus_1 fix on every helper + WRITE_DATA + DMA_DATA NOWHERE inserted, NOP padding removed). **Not yet committed.** No Cyrius source touched yet.
**Status:** B.3.d — confirmed Session 13's PGM_LO/HI shift fix is correct but not sufficient; second independent encoding bug found in every PM4 emit helper. Fix prepared but unrunnable this boot — used all 3 attempts.

## QUICK START — post-reboot Session 15 entry

```bash
# 0. Verify GPU healthy after reboot.
./build/shader/cl_probe       # MUST print readback = 0xDEADBEEF.
                              # If 0x00000000, hard power-cycle.

# 1. Run the count-fixed C reference. EXPECT PASS this time.
./build/libdrm_store_spike    # EXPECT: out[0] = 0xDEADBEEF

# 2. If step 1 PASSES → port the count-minus-1 fix into the Cyrius
#    helpers in src/backend_native.cyr (search for native_pm4_*),
#    then port the WRITE_DATA + DMA_DATA + NOP-removal changes to
#    programs/native_compute_spike.cyr and programs/native_compute_store.cyr.
#    Rebuild, run native_compute_spike, expect identical PASS.
# 3. If step 1 FAILS → next candidate is BO list path (Mesa uses inline
#    BO_HANDLES chunk with bo_list_handle=0; we use amdgpu_bo_list_create).
#    See "Open angles" below.
```

**Reboot budget rule still applies:** at most 3 direct-submission attempts
per boot. We took 3 this session and all 3 wedged the GPU. Do not chain.

## Headline

Three resets this session, all rooted in **one pre-existing bug** in the
spike's PM4 emit helpers — every `pm4_*` function passed `body_dws` to
`PACKET3(opcode, count_minus_1)` instead of `body_dws - 1`. The CP firmware
on Cezanne (gfx90c) tolerated the +1 count quietly across IBs that were
*uniformly* over-sized, which is why Sessions 7-13 saw VM faults / hangs
but never "Illegal opcode." Adding the WRITE_DATA + DMA_DATA NOWHERE
packets *correctly sized* mixed two encoding conventions in one IB; the
parser desynced past the new packets and tripped the illegal-opcode trap.

## The bug

PM4 PACKET3 header layout (bits 29:16 = `count_minus_1`):

```
body_dws = count_minus_1 + 1
```

Pre-Session-14 helpers in `deps/libdrm_store_spike.c`:

| Helper            | Body DWs | Passed to PACKET3 | Should be |
|-------------------|----------|-------------------|-----------|
| set_sh_reg_one    | 2        | 2                 | 1         |
| set_sh_reg_n      | n+1      | n+1               | n         |
| set_uconfig_one   | 2        | 2                 | 1         |
| set_uconfig_pair  | 3        | 3                 | 2         |
| acquire_mem_full  | 6        | 6                 | 5         |
| dispatch_direct   | 4        | 4                 | 3         |
| nop_pad           | pad-1    | pad               | pad-2     |

Every helper was +1. Verified against Mesa `AMD_DEBUG=ib ./build/shader/cl_probe`
header bytes in `build/strace/cl_probe_ib.log`:

| Packet              | Mesa header | count field | body_dws |
|---------------------|-------------|-------------|----------|
| SET_SH_REG one      | `c0017600`  | 1           | 2        |
| SET_SH_REG pair     | `c0027600`  | 2           | 3        |
| SET_UCONFIG one     | `c0017900`  | 1           | 2        |
| SET_UCONFIG pair    | `c0027900`  | 2           | 3        |
| ACQUIRE_MEM         | `c0055802`  | 5           | 6        |
| DISPATCH_DIRECT     | `c0031502`  | 3           | 4        |
| WRITE_DATA          | `c0033700`  | 3           | 4        |
| DMA_DATA            | `c0055000`  | 5           | 6        |

All have `count_minus_1 = body_dws - 1`. Confirmed.

## Why earlier sessions saw VM fault, this session saw illegal opcode

1. **Sessions 7-12 (broken PGM encoding + uniformly +1 count):** every
   packet over-claimed body length by 1. The CP read each "next
   packet's header" DW as the previous packet's tail-data, then
   tried the *following* DW as the next header. Most packets in our
   IB are SET_SH_REG / SET_UCONFIG — small register-offset DWs as
   first body word — these have bits 31:30 = 00, which the firmware
   treats as PKT0 (single-register write). PKT0 to a random offset
   appears to be silently absorbed or no-oped on Cezanne, so the
   parser stayed alive long enough to dispatch. Bad shader VA → VM
   fault while shader fetched.
2. **Session 13 (PGM fix only, count still +1):** same parser
   behaviour but shader VA now correct. The +1 absorption redirected
   one of the "no-op writes" to VA `0x66d000`, which faulted on the
   write side (UTCL2 client = CPC, RW=1). Different fault, same root
   cause as below.
3. **Session 14 attempt 2 (PGM fix + my correctly-sized WRITE_DATA +
   DMA_DATA):** my new packets had `count_minus_1 = body_dws - 1`
   (right). Mixed with the existing +1 helpers: parser stayed in
   "+1" mode across set_sh_reg / acquire_mem etc., then hit my
   correct WRITE_DATA — no extra absorbed DW — parser cursor now
   shifted by 1 relative to where it was guessing. Eventually a DW
   at the new cursor position decoded as "type=00 with non-zero
   reserved bits" or similar invalid header → "Illegal opcode in
   command stream" + MODE2 reset.
4. **Session 14 attempt 3 (same as 2 but no NOP padding):** identical
   failure mode — proves NOPs were not the trigger; the helpers are.

## What changed in `deps/libdrm_store_spike.c`

Diff vs HEAD (`7c30939`):

1. **All six helpers fixed** — `count_minus_1` arg is now `body_dws - 1`
   in every case. Header for ACQUIRE_MEM is now `0xC0055802` exactly
   (was `0xC0065802`); DISPATCH_DIRECT is `0xC0031502` exactly
   (was `0xC0041502`); etc.
2. **WRITE_DATA zero-fence inserted** between TA_CS_BC_BASE_ADDR and
   PGM_LO. Body bytes copied byte-exact from Mesa IB1 dump — fence VA
   `0xFFFF800100600300` (kernel-magic sync VA, no userspace BO).
3. **DMA_DATA NOWHERE inserted** between ACQUIRE_MEM and
   RESOURCE_LIMITS. Body bytes byte-exact from Mesa IB1 — SRC and DST
   both `0xFFFF800000000000`, BYTE_COUNT=0x60, DISABLE_WR_CONFIRM=1.
4. **NOP padding removed.** `IB_DW_TOTAL` (256) replaced by the
   actual emitted DW count via `ib_dws_used = p - ib`; this is
   passed straight into `ib_info.size`. Mesa's IB1 is 74 DWs; ours
   should be ~68 DWs after the changes.
5. Comments at every helper explain why count_minus_1 = body - 1 and
   reference this session.

## What was actually run this session

| # | Test | Outcome | Reset count this boot |
|---|---|---|---|
| 1 | `cl_probe` post-reboot | `0xDEADBEEF` 76 ms | — |
| 2 | `libdrm_store_spike` (encoding fix only) | `-ECANCELED`, VM fault `0x66d000` (CPC, RW=1) | reset(1) |
| 3 | `cl_probe` between attempts | still `0xDEADBEEF` | — |
| 4 | `libdrm_store_spike` (+ WRITE_DATA + DMA_DATA, 256-DW NOP pad) | `-ECANCELED`, **Illegal opcode**, MODE2 reset | reset(2) |
| 5 | `cl_probe` between attempts | still `0xDEADBEEF` | — |
| 6 | `libdrm_store_spike` (+ WRITE_DATA + DMA_DATA, no NOP pad) | `-ECANCELED`, **Illegal opcode**, MODE2 reset | reset(3) |

The two illegal-opcode failures with and without NOP padding are the
clean experiment that ruled out the NOP path as the culprit.

## Cyrius port plan (Session 15 step 2)

Files that mirror the same broken helpers:

- `src/backend_native.cyr` — search for `native_pm4_set_sh_reg_one`,
  `native_pm4_set_sh_reg_n`, `native_pm4_set_uconfig_one`,
  `native_pm4_set_uconfig_pair`, `native_pm4_acquire_mem`,
  `native_pm4_dispatch_direct`, `native_pm4_nop_pad` (or whatever the
  equivalents are named). Each has the same +1.
- `programs/native_compute_spike.cyr:173` — `pgm_lo = shader_va & 0xFFFFFFFF`
  must become `pgm_lo = (shader_va / 0x100) & 0xFFFFFFFF` (Cyrius `>>` 8 form).
  `pgm_hi` is already correct (`/ 0x10000000000` = `>> 40`).
- `programs/native_compute_spike.cyr` — also needs the WRITE_DATA fence
  (between TA_CS_BC_BASE_ADDR and PGM_LO) and DMA_DATA NOWHERE
  (between ACQUIRE_MEM and RESOURCE_LIMITS), byte-exact bodies as
  in the C reference.
- `programs/native_compute_store.cyr:180` — same encoding fix.
- IB submit path — verify `ib_size_dwords` is set to the actual emitted
  DW count, not a hard-coded 256.

After porting, run `programs/native_compute_spike` (built via the Cyrius
toolchain) and expect the same `0xDEADBEEF` readback.

## Open angles if Session 15 step 1 still fails

We've ruled out a lot, but if PASS doesn't happen even with both fixes:

- **BO list submission shape.** Mesa uses the inline BO_HANDLES chunk
  (`bo_list_handle = 0`) per the strace from Session 13. We use
  `amdgpu_bo_list_create` then pass the list handle. Probably equivalent
  but worth confirming if illegal-opcode persists.
- **VM mapping flags.** `libdrm_store_spike_vmav.c` already tested
  `AMDGPU_GEM_CREATE_VM_ALWAYS_VALID = 0x40` — negative result. Not
  this.
- **Trailing zero-DMA_DATA terminator.** Mesa's IB1 ends with a
  `c0055000` DMA_DATA where every body DW is zero (BYTE_COUNT=0,
  no-op). We omit it. Could be required.
- **Compute ring vs gfx ring.** Both Mesa and we use compute ring; ruled
  out.
- **Magic VA `0xFFFF8000xxxx` accessibility.** If WRITE_DATA fence to
  `0xFFFF800100600300` faults from our context (not Mesa's), the
  firmware sync would fail. Failure mode would be VM fault, not
  illegal opcode — so probably not the issue, but worth checking the
  kernel ring on the next attempt.

## Don't lose

- `feedback_pm4_verify_against_mesa_ib.md` reinforced again — header
  byte-diff against Mesa IB found this. **Always go to AMD_DEBUG=ib
  before any PM4 hypothesis.**
- After Session 15 confirms the helper fix works, write a feedback
  memory: *"PACKET3 count_minus_1 = body_dws - 1; always derive
  via Mesa AMD_DEBUG=ib byte-diff before claiming a helper is right.
  Naming the macro arg `count_minus_1` and then passing `body_dws`
  was lethal once we mixed encoding conventions in the same IB."*
- The Session 13 PGM_LO/HI shift fix is *real* and stays. It is
  necessary; it just wasn't sufficient on its own.
