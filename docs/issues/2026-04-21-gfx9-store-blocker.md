# v3 Phase B.4 — GFX9 Compute Store Blocker

**Status:** Open
**Date:** 2026-04-21
**Affects:** Phase B.4 (verifiable compute dispatch on AMDGPU direct-ioctl path)
**Hardware:** AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c), Linux 6.18.22-lts

## Symptom

A minimum "write constant to memory" compute shader consistently triggers
`gfx_v9_0_bad_op_irq` ("Illegal opcode in command stream") and a
`MODE2` GPU reset. The non-store variant of the same shader (v_mov_b32
only, no memory access) dispatches cleanly and signals the sync-obj —
so the CS submission path, PM4 register setup, sync-obj handling, and
everything else in Phase B.1–B.3.d is working.

## What was tried

Every memory-store instruction variant fails identically:

- `global_store_dword v[0:1], v2, off` — with and without `glc slc`
- `flat_store_dword v[0:1], v2` — with and without `glc slc`
- `s_store_dword s3, s[0:1], 0x0` — scalar memory store
- All of the above with both **USER_DATA-loaded** addresses (RSRC2 USER_SGPR=2)
  and **hardcoded-in-shader** addresses (s_mov_b32 to build the address inline)

Every VA tried for the output buffer fails identically:

- `0x00600000` (6 MiB) — initial attempt
- `0x10000000` (256 MiB) — moved above low aperture
- `0x100000000` (4 GiB) — above 32-bit window
- `0x1000000000` (64 GiB) — well clear of 32-bit window

Shader binaries assembled by `clang -target amdgcn--amdhsa -mcpu=gfx90c`;
byte encodings cross-checked against AMD GCN5/Vega ISA documentation.

## Working PM4 register setup (B.3.d, s_endpgm dispatch)

- `COMPUTE_PGM_LO/HI` = shader_va split (VA >> 8 / VA >> 40)
- `COMPUTE_PGM_RSRC1` = 0x00AC0000 (FLOAT_MODE=0xC0 | DX10_CLAMP | IEEE_MODE)
- `COMPUTE_PGM_RSRC2` = 0 (no user SGPRs) or 0x04 (USER_SGPR=2)
- `COMPUTE_RESOURCE_LIMITS` = 0
- `COMPUTE_STATIC_THREAD_MGMT_SE0/SE1` = 0xFFFFFFFF
- `COMPUTE_TMPRING_SIZE` = 0
- `COMPUTE_USER_DATA_0/1` = output VA split (when USER_SGPR=2)
- `COMPUTE_NUM_THREAD_X/Y/Z` = 1
- `DISPATCH_DIRECT 1,1,1, initiator=0x5 (SHADER_EN | FORCE_START_AT_000)`

Submitted via `AMDGPU_HW_IP_COMPUTE`, IB + SYNCOBJ_OUT chunks, 5-second
sync-obj wait.

## Research hypotheses examined

1. **Wrong shader ISA encoding.** Ruled out — clang-assembled bytes
   match AMD GCN5 ISA doc layouts; non-store instructions from the same
   assembly run fine.

2. **`SYNCOBJ_OUT` chunk layout.** Solved separately (B.3.d) and
   captured as a vidya gotcha.

3. **`COMPUTE_STATIC_THREAD_MGMT_SE0..3` defaulting to 0.** Solved in
   B.3.d — CUs now enabled.

4. **GFX9 aperture classification.** Research identified
   `SH_MEM_BASES = 0x60006000` on Cezanne — private and shared
   apertures at VA `0x0006_xxxx_xxxx_xxxx`. Advice was to move the
   output buffer above the 32-bit window. **Empirically this didn't
   resolve it** — moving VA from 256 MiB to 64 GiB still triggers the
   same fault.

5. **Missing `SH_MEM_CONFIG` / `SH_MEM_BASES` register writes.**
   These are privileged (RLC-gated) and cannot be written from a PM4
   IB. They're kernel-programmed per-VMID at init. The agent
   hypothesis was that our VMID hadn't been through `init_compute_vmid`
   — but every other ioctl works, and `STATIC_THREAD_MGMT` wouldn't
   work either without proper VMID init. Unconfirmed.

## Likely next angles (for a future session)

1. **Is there a `COMPUTE_PGM_RSRC3` on GFX9?** GFX10+ has one. Vega
   docs suggest no, but worth double-checking in Mesa radv's
   `radv_emit_compute_shader` path.

2. **`ENABLE_SGPR_PRIVATE_SEGMENT_BUFFER` in PGM_RSRC2.** If this bit
   is implicitly expected to be set and we're not setting it, the SQ
   may refuse to issue any memory op. The "private segment buffer" is
   the scratch V# — without it, the segment check may fail.

3. **Shader wrapper / AMDHSA kernel descriptor.** ROCm's HSA runtime
   prepends a 64-byte `amd_kernel_code_t` descriptor before the ISA
   bytes. Our direct-PM4 path skips this entirely because PM4 writes
   PGM_LO/HI/RSRC1/RSRC2 directly, but maybe on GFX9 the kernel
   descriptor is load-bearing for some other reason (e.g., initializing
   hidden registers the CP reads).

4. **KMD ring selection.** We submit to `AMDGPU_HW_IP_COMPUTE` which
   lands us on `comp_1.*.*` ring (confirmed in dmesg). But maybe
   stores from the non-KFD compute ring require specific USER_DATA
   slots pre-programmed to kernel-provided values the way ROCm's KFD
   path does.

5. **`libdrm_amdgpu` comparison.** Strace a simple OpenCL program
   doing a single memory store to compare the exact CS chunks and
   register writes Mesa emits vs what we emit. The delta would be
   exactly what we're missing.

## Checkpoint state

- `src/backend_native.cyr`: `native_gfx9_shader_store_deadbeef` is
  stubbed to the v_mov+endpgm diagnostic variant. Does not wedge the
  GPU on run; does not write anything either, so
  `programs/native_compute_store.cyr` expectedly fails at the verify
  step (sentinel still in place).
- `programs/native_compute_store.cyr`: output VA = 0x1000000000 (64 GiB).
- All other hardware integration programs (`native_device_enum`,
  `native_gem_roundtrip`, `native_submit_setup`, `native_compute_spike`)
  continue to pass.
- 598 CPU tests + 3 fuzz harnesses green.

## What it means for Phase B

Phase B.3 stated exit ("compute dispatch completes without kernel
error") is MET. Phase B.4 stated exit ("byte-identical compute output
vs wgpu") is NOT met — we can't verify output because stores wedge.

The wgpu backend does compute round-trips in `programs/compute_e2e.cyr`
(shipping since v2.4.0), so consumer-level compute *does* work on this
hardware — just not via our direct-ioctl path with the PM4 state we're
programming. That gap is the open question.
