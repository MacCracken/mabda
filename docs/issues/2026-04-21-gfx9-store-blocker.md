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

## Session 2 additions (after three more research agents)

Additional fixes applied — **none of which have unblocked the store**:

- `TRAP_PRESENT=1` in PGM_RSRC2 (bit 6). Was missing; now `RSRC2 = 0x44` for the store-shader path.
- 16 dwords of `s_nop 0` trailing padding after `s_endpgm` (AMDGPU ABI requirement; SQ prefetches past shader end).
- `IT_ACQUIRE_MEM` packet with `coher_cntl = 0x38800000` (SH_ICACHE | SH_KCACHE | TC | TCL1 action_ena) emitted before `DISPATCH_DIRECT`.
- `SET_UCONFIG_REG CP_COHER_START_DELAY = 0` (GFX9-required queue preamble register that radv emits once per queue init).
- Per-SE CU masks now write all four: `COMPUTE_STATIC_THREAD_MGMT_SE0..SE3` at `0xB85C`, `0xB860`, `0xB864`, `0xB868` (previously had SE0 + SE1 + a WRONG register at 0xB864 labeled as `TMPRING_SIZE` but actually `SE2`; was silently disabling SE2 every dispatch).
- `RSRC1.SGPRS = 1` (16 SGPRs allocated, vs the previous 0 → 8 SGPRs, insufficient for USER_SGPR + VCC + flat_scratch per radv's accounting).

Failure mode evolution across all the attempts:
1. **Original**: `bad_op_irq` + GPU reset on any store.
2. **After VA aperture moves to 4 GiB / 64 GiB**: unchanged.
3. **After TRAP_PRESENT + NOP padding**: unchanged.
4. **After ACQUIRE_MEM + CP_COHER_START_DELAY + SE2/SE3 writes + SGPRS=1**: on some runs `bad_op_irq` still fires; on some runs dispatch "completes" but output buffer reads sentinel. It's possible the "completion" is actually a timeout-then-reset-signal cycle rather than a genuine dispatch success — dmesg still shows resets under the same timestamp window.

## Session 3 — GROUND TRUTH from Mesa AMD_DEBUG=ib (2026-04-21 evening)

Installed opencl-mesa + clinfo + strace. Built a minimal OpenCL "store 0xDEADBEEF to buf[0]" probe, ran it with `AMD_DEBUG=ib`. Mesa's rusticl dumped the full PM4 IB it emits for a successful compute dispatch on this exact Cezanne/gfx90c hardware.

**Biggest finding: my register offsets were wrong by a whole-register shift.** Every register from PGM_LO through TMPRING_SIZE was at the wrong byte offset. B.3.d's "dispatch completed" in earlier sessions was a false positive — the sync-obj was firing via the GPU-reset recovery path, not from actual shader execution. My "PGM_LO" writes were going to RSRC1; the real PGM_LO retained kernel-stale state, and the "dispatched shader" was whatever kernel-installed default lived at that address.

Corrected GFX9 register offsets (verified against Mesa IB dump):

| Register | Actual offset | What I had (wrong) |
|----------|--------------|--------------------|
| `COMPUTE_PGM_LO` | **0xB830** | 0xB848 (actually RSRC1) |
| `COMPUTE_PGM_HI` | **0xB834** | 0xB84C (actually RSRC2) |
| `COMPUTE_PGM_RSRC1` | **0xB848** | 0xB850 (reserved?) |
| `COMPUTE_PGM_RSRC2` | **0xB84C** | 0xB854 (actually RESOURCE_LIMITS) |
| `COMPUTE_RESOURCE_LIMITS` | **0xB854** | 0xB858 (actually SE0) |
| `COMPUTE_STATIC_THREAD_MGMT_SE0` | **0xB858** | 0xB85C (actually SE1) |
| `COMPUTE_STATIC_THREAD_MGMT_SE1` | **0xB85C** | 0xB860 (actually TMPRING_SIZE) |
| `COMPUTE_TMPRING_SIZE` | **0xB860** | not written |
| `COMPUTE_STATIC_THREAD_MGMT_SE2` | 0xB864 | 0xB864 (accidentally correct) |
| `COMPUTE_STATIC_THREAD_MGMT_SE3` | 0xB868 | 0xB868 (accidentally correct) |

Offsets corrected. Tests updated to match. `make test` = 602/602 pass.

**With offsets corrected, ALL hardware dispatches wedge the GPU** — even the `s_endpgm`-only `compute_spike`. This is actually the HONEST state. There's additional setup Mesa does that we don't, which this session identified in the IB dump but hasn't yet applied fully:

1. **SET_SH_REG `PGM_HI = 0x80`** unconditionally (Mesa's shaders live at VA `0x800000000000`, but user VA range on gfx90c caps below that — needs more investigation to find the valid high-VA shader slot).
2. **SET_UCONFIG_REG `TA_CS_BC_BASE_ADDR = 0x01004400` + `_HI = 0x80`** (border color base).
3. **`WRITE_DATA`** to VA `0xffff800100600300` (kernel-tracked, purpose unclear).
4. **`COMPUTE_PGM_RSRC2 = 0x8`** (USER_SGPR=4, not 2) — Mesa ABI expects USER_DATA_0..3 to hold something specific (likely scratch V# descriptor in s[0:3]).
5. **Scratch V# setup** — Mesa sets `USER_DATA_0 = 0x00200040` and `USER_DATA_2 = 0x00200000`. USER_DATA_0/1 is almost certainly the private_segment_buffer V# (scratch descriptor, 16-byte buffer resource). Even though SCRATCH_EN=0 in RSRC2, the hardware may validate s[0:3] as a V#.
6. **`COMPUTE_TMPRING_SIZE = 0x100`** (Mesa allocates 256 waves of scratch even for trivial kernels).
7. **`ACQUIRE_MEM CP_COHER_CNTL = 0xa8c40000`** (Mesa uses this exact value; we've been using 0x38800000).
8. **`DISPATCH_INITIATOR = 0x45`** (bits 0, 2, 6 — bit 6 is ORDER_MODE).

Next session's first move: match Mesa's complete PM4 stream byte-for-byte on a scratch-less GFX9 test, iterate until the dispatch actually runs our shader (verifiable by store succeeding).

## Session 4 — Mesa strace + more register semantics (2026-04-21 late evening)

Installed packages (strace, clinfo, opencl-mesa). Ran `strace -f -e trace=ioctl -v` on the working OpenCL compute probe. Full log preserved at `docs/issues/2026-04-21-mesa-cl-ioctl-trace.log`.

**New ground truth from the strace:**

1. **Mesa maps ALL its BOs in the canonical-high VA aperture**, specifically starting at `0xFFFF_8001_0000_0000` with 2 MiB strides: handle 1 → `0xFFFF_8001_0000_0000`, handle 3 → `0xFFFF_8001_0020_0000`, etc. My mabda code had been mapping at low VAs (2 MiB / 4 MiB) which the kernel accepts but the GPU's compute path doesn't actually run shaders from.

2. **The apparent "PGM_HI = 0x80 = 128 TiB shader VA" that made no physical sense turns out to be encoding bits[47:40] of a canonical-high VA `0xFFFF_8001_xxxxxxxx`.** The 0x80 + the implicit bits 39:32 = 0x01 (from the GFX9 shader aperture) combine to give the full 48-bit canonical VA — sign-extended upward by the hardware. PGM_LO holds the low 32 bits, PGM_HI holds bits 47:40, and bits 39:32 are an implicit 0x01 tied to the aperture. My PGM encoding had been assuming a much lower VA range.

3. **Mesa uses `DRM_IOWR` (dir=3) for `AMDGPU_GEM_VA`, not `DRM_IOW` (dir=1)** that the header declares. Kernel accepts both — not a blocker but worth noting.

**Applied this session:**
- Shader and IB VAs moved to canonical-high aperture.
- Both `compute_spike.cyr` and `compute_store.cyr` updated.
- PGM_LO/HI encoding confirmed: `PGM_LO = va & 0xFFFFFFFF`, `PGM_HI = (va >> 40) & 0xFF`.

**Result:** still wedges. So the VA aperture was *a* missing piece but not the final one. The next most likely remaining delta is Mesa's `WRITE_DATA` preamble packet (writing 4 bytes to `0xFFFF_8001_0060_0300`) which appears to be a kernel-tracked progress marker. Without it, the kernel may not know our submission actually advanced, and consider it stuck.

**Saved artifacts for next session:**
- Full Mesa ioctl trace: `docs/issues/2026-04-21-mesa-cl-ioctl-trace.log`
- Mesa PM4 IB dump (not saved, but reproducible via `AMD_DEBUG=ib ./build/shader/cl_probe`)
- OpenCL probe source: `build/shader/cl_probe.c` (used to reproduce the working-dispatch trace)

Concrete next moves (in order of probability):
1. Emit Mesa's `WRITE_DATA` packet (opcode 0x37) writing 0 to `0xFFFF_8001_0060_0300` in the preamble.
2. Set `TA_CS_BC_BASE_ADDR` to point at a canonical-high VA matching Mesa's `0x80_01004400`.
3. Capture Mesa's AMDGPU_CS ioctl payload byte-for-byte; build our CS ioctl to match chunk-by-chunk.
4. If still stuck, check whether Mesa uses `AMDGPU_VM_OP_RESERVE_VMID` via a separate VM ioctl before GEM_VA.

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
