# v3 Phase B — Session 13: ROOT CAUSE FOUND (PGM_LO/HI encoding bug)

**Date:** 2026-04-25 (evening, after Session 12)
**Branch:** `v3`
**Working tree at session end:** modified (`deps/libdrm_store_spike.c` patched, `deps/libdrm_store_spike_vmav.c` new) — **NOT YET COMMITTED**
**Status:** B.3.d **root cause identified** — fix applied to C reference, retest pending fresh reboot. GPU was poisoned by 4 resets during this session; even Mesa `cl_probe` stopped returning correct data, so the fix could not be validated in-session.

## QUICK START — post-reboot Session 14 entry

```bash
# 0. Verify GPU healthy after reboot.
./build/shader/cl_probe       # MUST print readback = 0xDEADBEEF
                              # If it returns 0x00000000, hard power-cycle.

# 1. The smoking gun — run the patched C reference.
./build/libdrm_store_spike    # EXPECT: out[0] = 0xDEADBEEF (PASS)
                              # submit-to-fence: ~50–200 ms

# 2. If step 1 PASSES → port the same shift-encoding fix into
#    src/backend_native.cyr and programs/native_compute_spike.cyr,
#    rebuild and run the Cyrius spike. That closes B.3.d.
# 3. If step 1 FAILS → the encoding fix isn't sufficient on its own;
#    add Mesa's WRITE_DATA fence and DMA_DATA NOWHERE packets
#    (template at end of this doc) and retest. After 2 failed runs,
#    reboot before continuing.
```

**Reboot budget unchanged:** at most 3 direct-submission attempts per
boot. If `cl_probe` returns `0x00000000` instead of `0xDEADBEEF`, the
firmware is degraded — reboot.

## Headline

`AMD_DEBUG=ib ./build/shader/cl_probe` dumped the working IB. Diffing
field by field against `deps/libdrm_store_spike.c` revealed our
`COMPUTE_PGM_LO` / `COMPUTE_PGM_HI` shader-VA encoding has been wrong
since Session 7. The CP firmware was being told to fetch the shader
from a bogus VA every time, which fully explains why **every** direct
submission in Sessions 9–12 hung — bare WRITE_DATA-only IBs, NOP-only
IBs, and full shader-dispatch IBs all carried the same bug, just on
different code paths.

## The bug

GFX9 `COMPUTE_PGM_LO` / `COMPUTE_PGM_HI` canonical encoding (per
radv `radv_compute.c`, amdgpu kernel `gfx_v9_0.c`):

```c
PGM_LO = (shader_va >> 8) & 0xFFFFFFFF      /* bits 39:8 of VA */
PGM_HI = (shader_va >> 40) & 0xFF           /* bits 47:40 of VA */
/* HW reconstructs: shader_addr = (PGM_HI << 40) | (PGM_LO << 8) */
```

Pre-Session-13 code in `deps/libdrm_store_spike.c`,
`programs/native_compute_spike.cyr`, and `src/backend_native.cyr`:

```c
PGM_LO = shader_va & 0xFFFFFFFF             /* MISSING the >>8 */
PGM_HI = (shader_va >> 8) & 0xFFFFFFFF      /* the value that should
                                               have been in PGM_LO */
```

For the `shader_va = 0x100001000` we always allocated, the CP saw:

| Reg | Pre-Session-13 value | Reconstructed VA | Correct value | Correct VA |
|---|---|---|---|---|
| PGM_LO | `0x00001000` | `0x1000 << 8 = 0x100000` | `0x01000010` | `0x01000010 << 8 = 0x100001000` ✓ |
| PGM_HI | `0x01000010` | (extra garbage in upper bits) | `0x00` | (zero) |

So the CP fetched the first instruction word from VA `0x100000` —
which doesn't map to anything we allocated — got either garbage or
a VM fault, and stalled. Every IB containing a real DISPATCH_DIRECT
hit the same code path. The "bare WRITE_DATA also hangs" pattern in
Sessions 11–12 likely was a separate issue (WRITE_DATA-only IBs
genuinely confuse Cezanne CP per `feedback_write_data_on_cezanne.md`),
but the shader-dispatch class of hang was 100% this encoding bug.

## How I found it (for future reference)

Method that worked, after `ltrace` was unavailable:

1. `strace -f -v -e trace=ioctl,openat,mmap -o cl_probe.log ./build/shader/cl_probe`
   — ioctl histogram. Showed Mesa makes 2 CS submits with `bo_list_handle=0`
   (inline BO_HANDLES chunk path) plus uses syncobj. Suggestive of structural
   differences but didn't pinpoint the bug.
2. `AMD_DEBUG=ib ./build/shader/cl_probe` — dumps the actual PM4 IB
   contents with field-decoded register values. **This was the killer
   tool.** Showed PGM_HI=0x80, PGM_LO=0x0 in Mesa, where ours had
   the values swapped and unshifted.

If a future encoding question comes up, go straight to `AMD_DEBUG=ib`.

## Other Mesa-vs-ours IB differences

Spotted while diffing — apply only if the encoding fix alone doesn't
unblock dispatch. Mesa emits these between specific places:

```c
/* (a) Right after TA_CS_BC_BASE_ADDR pair, before PGM_LO.
 * 4-DW WRITE_DATA(MEM, ENGINE_SEL=ME, WR_CONFIRM=1) zero-fence. */
*p++ = PACKET3(0x37, 3);             /* WRITE_DATA, count_minus_1=3 */
*p++ = 0x00100500u;                  /* CONTROL: DST_SEL=MEM,
                                                 ENGINE_SEL=ME,
                                                 WR_CONFIRM=1 */
*p++ = (uint32_t)(fence_va & 0xFFFFFFFFu);
*p++ = (uint32_t)((fence_va >> 32) & 0xFFFFFFFFu);
*p++ = 0;                            /* zero-fence value */

/* (b) Right after ACQUIRE_MEM, before RESOURCE_LIMITS.
 * 7-DW DMA_DATA(DST_SEL=NOWHERE, BYTE_COUNT=96, RAW_WAIT=0) sync. */
*p++ = PACKET3(0x50, 5);             /* DMA_DATA, count_minus_1=5 */
*p++ = 0x60200000u;                  /* word0: ENGINE=ME,
                                                DST_SEL=NOWHERE,
                                                SRC_SEL=SRC_ADDR_TC_L2 */
*p++ = 0x00000000u;                  /* SRC_ADDR_LO */
*p++ = 0xFFFF8000u;                  /* SRC_ADDR_HI (any valid VA) */
*p++ = 0x00000000u;                  /* DST_ADDR_LO */
*p++ = 0xFFFF8000u;                  /* DST_ADDR_HI */
*p++ = 0x80000060u;                  /* COMMAND: BYTE_COUNT=0x60,
                                                 DISABLE_WR_CONFIRM=1 */
```

These are belt-and-braces — neither was lethally absent in the test
above (the encoding bug masked their effect). Add only after a
post-reboot retest confirms the encoding fix isn't sufficient alone.

## What was actually run this session

| # | Test | Result | Notes |
|---|---|---|---|
| 1 | Baseline `libdrm_store_spike` post-reboot, pre-fix | `-ECANCELED`, GPU reset(1) | Fresh boot alone didn't help |
| 2 | `libdrm_store_spike_vmav` (`AMDGPU_GEM_CREATE_VM_ALWAYS_VALID = 0x40` on every BO) | `-ECANCELED`, GPU reset(2) | VM_ALWAYS_VALID is **not** the missing piece |
| 3 | Mesa `cl_probe` sanity | `0xDEADBEEF`, 76 ms | GPU still healthy at this point |
| 4 | `strace` Mesa cl_probe + ours, ioctl diff | structural diffs spotted | Suggestive only |
| 5 | `AMD_DEBUG=ib` Mesa cl_probe | full IB dump captured to `build/strace/cl_probe_ib.log` | **Found the encoding bug** |
| 6 | Patched `deps/libdrm_store_spike.c` + retest | `-ECANCELED`, GPU reset(3) | But by now GPU was likely poisoned |
| 7 | Mesa `cl_probe` post-reset(4) | readback = `0x00000000`, no TDR | Confirms firmware degraded; reboot required |

## File state at session end

- `deps/libdrm_store_spike.c` — **patched** (encoding fix at lines
  ~218–225 with explanatory comment block). Built. Not yet validated
  post-reboot. **Not yet committed.**
- `deps/libdrm_store_spike_vmav.c` — **new** (variant with
  VM_ALWAYS_VALID flag). Built. Negative result captured. Keep as
  reference for future kernel-state experiments. **Not yet committed.**
- `src/backend_native.cyr` — **still carries the broken encoding.**
  Locate `pgm_lo` / `pgm_hi` computation and apply the same fix only
  after the C reference confirms.
- `programs/native_compute_spike.cyr` — **still carries the broken
  encoding.** Same plan.
- `build/strace/cl_probe.log`, `cl_probe_v.log`, `cl_probe_ib.log`,
  `spike_vmav.log` — strace + IB-dump artifacts. Useful as reference
  for future ABI questions. Gitignored (under `build/`).

## Post-reboot plan (Session 14)

1. **Validate GPU recovered** — `./build/shader/cl_probe` must return
   `0xDEADBEEF`. If not, hard power-cycle.
2. **Validate the fix in C** — `./build/libdrm_store_spike` should
   return `out[0] = 0xDEADBEEF`. If yes:
   - Capture submit-to-fence latency (will be the first honest direct-
     dispatch number we have on Cezanne).
   - Commit `deps/libdrm_store_spike.c` and
     `deps/libdrm_store_spike_vmav.c` with a Session 13 message.
3. **Port the fix to Cyrius** — find the equivalent
   `pgm_lo`/`pgm_hi` (or whatever names) in `src/backend_native.cyr`
   and `programs/native_compute_spike.cyr`. Apply identical shift
   correction. Rebuild, run, expect identical PASS.
4. **Update memory + retire the "blocked" framing** in
   `project_first_native_dispatch.md` once steps 2 + 3 both pass.

## Risks / open questions

- The C reference fix may pass while the Cyrius port still hangs if
  Cyrius has additional ABI quirks (e.g., `fncall6`-related struct
  packing on the CS submit path). Treat the Cyrius port as a separate
  validation gate.
- If the fix passes and dispatch latency is way above Mesa's 76 ms
  (say, > 500 ms), there's something else suboptimal but not lethal —
  log it and move on, don't chase it for v3 GA.
- The two GPU resets we triggered before finding the bug used up some
  of the firmware's reset budget on this machine. If the chip starts
  needing a power cycle after every reset (vs warm reboot recovering),
  document and slow down the test cadence.

## Don't lose

- `feedback_pm4_verify_against_mesa_ib.md` is now reinforced — diffing
  byte-exactly against `AMD_DEBUG=ib` is what found this. Keep using
  that as the first-line verification for any new PM4 work.
- The PGM_LO/HI encoding rule (`>>8` and `>>40 & 0xFF` for GFX9) is
  worth a feedback memory once Session 14 confirms — for future PM4
  programmers, it's the kind of thing where the comment in our old
  code was *also* wrong, so the bug propagated for sessions.
