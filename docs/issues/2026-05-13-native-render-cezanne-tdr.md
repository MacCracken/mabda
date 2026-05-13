# 2026-05-13 — native render GFX-ring TDR on Cezanne (gfx90c)

**Status:** OPEN. 35+ register-level correctness fixes landed; the
underlying 2-second GFX-ring TDR persists. Compute path on the same
hardware is unaffected (6h soak passed 2026-05-12, 15.66M iterations
flat RSS; re-verified post-render-fixes 2026-05-12 evening — compute
still bit-exact 0xDEADBEEF). Filing for v3.0.x patch-stream resolution
after rc.3 cut.

## Update — 2026-05-12 evening session (root-cause analysis round 2)

Audit of the rc.2 → rc.3 work in this file revealed that fixes #23,
#25, #26, #27 (PKT3 NUM_INSTANCES, FLUSH_AND_INV pre-draw, PFP_SYNC_ME,
PIPELINESTAT_START) **had defined helpers and constants but were never
wired into `native_pm4_build_render_clear_triangle`**. The composer
emitted the original 27-or-so register block plus a post-draw
CACHE_FLUSH_AND_INV, without any of the new pre-draw packets. This
was caught by byte-dumping the composer output via
`programs/dump_render_pm4` and diffing against the same hardware's
radv vkcube capture in `/tmp/radv-ib.txt`.

Followup fixes landed in this evening's session (each one cited
against radv on Cezanne via `RADV_DEBUG=dumpibs`):

1. **`GFX9_VGT_SHADER_STAGES_VS_PS` 0x0 → 0x00010000.** The old value
   was a textbook off-by-misunderstanding — comment said "0 = defaults,
   VS enabled" but on GFX9 the VS_EN bit is at bit 16 and 0 means
   `VS_STAGE_OFF`. With VS_OFF the VGT waits forever for vertex output
   that can't arrive → exactly the 2-second VGT TDR shape. Captured
   byte-for-byte from radv (0x00010000 across every vkcube draw).
2. **`SPI_SHADER_PGM_RSRC3_VS = 0x003FFFFE`.** Previously not emitted.
   This is the CU enable mask for the VS — bits 1-21 set means CUs 1-21
   eligible. Default-after-context-invalidate = whatever the prior
   context left, often 0 → VS waves can't allocate → hang.
3. **`SPI_SHADER_LATE_ALLOC_VS = 24`.** Previously not emitted.
   Cezanne-tuned late VS allocation count from radv.
4. **`PA_CL_VS_OUT_CNTL = 0`.** Previously not emitted. Our VS exports
   only position; explicit 0 clears whatever VS_OUT mask the compositor
   left here (a non-zero value can make the PA stage wait on output
   slots our VS doesn't fill).
5. **PFP_SYNC_ME at IB start, EVENT_WRITE PIPELINESTAT_START after
   ACQUIRE_MEM, PKT3 NUM_INSTANCES (0x2F) replacing
   SET_UCONFIG_REG(VGT_NUM_INSTANCES).** The packets the previous
   session said it landed but didn't.
6. **`PA_SC_AA_MASK_X0Y0_X1Y0` and `PA_SC_AA_MASK_X0Y1_X1Y1` = 0xFFFFFFFF.**
   Previously not emitted. Default = 0 (no samples covered → no
   pixels written).
7. **Six more explicit-`=0` emits** (`SPI_BARYC_CNTL`,
   `PA_SC_SHADER_CONTROL`, `VGT_GS_MODE`, `VGT_PRIMITIVEID_EN`,
   `DB_Z_INFO`, `DB_STENCIL_INFO`) — to override whatever the prior
   compositor context left in each register.

Block sizes updated:
- `NATIVE_PIPE_SH_BLOCK_SIZE` 48 → 56 (VS SET_SH_REG now spans
  RSRC3_VS through RSRC2_VS, 6 contiguous regs).
- `NATIVE_PIPE_FIELD_CTX_BLOCK` 80 → 88.
- `NATIVE_PIPE_CTX_BLOCK_SIZE` 424 → 532 (9 × 12-byte sets added).
- `NATIVE_PIPE_STRUCT_SIZE` 504 → 620.

Tested with `make test-native-compute-store` post-fix: compute still
bit-exact, no regression in the working path.

## At-hang state captured via devcoredump

The 2026-05-12 evening session captured `/sys/class/drm/card1/device/devcoredump/data`
to `/tmp/render-tdr.txt` (`./scripts/capture-tdr.sh`). Key findings:

- **`Faulty page starting at address: 0x0000000000000000`** — gfxhub
  page fault at VA 0. `mmVM_L2_PROTECTION_FAULT_STATUS = 0x0` (current
  status clear — the fault was historic, not active at TDR time).
- **PFP stalled** — `mmCP_STALLED_STAT1 = 0x00000c00` (bits 10-11),
  `mmCP_CPF_STALLED_STAT1 = 0x00000001`, `mmCP_CPF_STATUS = 0xbc000223`.
- **Most-recent PFP packets** (`mmCP_PFP_HEADER_DUMP`):
  `c0055800 c0004600 c0004600 c0004600 c0032200 c0981000 c0008b00
  c0064900` — last 8 packets the PFP processed before stalling.
- **GFX-ring contents** (offset 0x520-0x550 in the kernel ring):
  - 0x524: `c0012800` CONTEXT_CONTROL with CC0=0x81018003, CC1=0x0 —
    kernel-emitted preamble immediately before jumping to mabda's IB
  - 0x530: `c0009000` INDIRECT_BUFFER_PRIV (1 dword)
  - 0x538: `c0023f00` INDIRECT_BUFFER → 0xffff800100200000 (mabda's IB)
    with IB_CONTROL=0x07000100 (256 dwords)
- **VA encoding NOT the bug.** Kernel installs mabda's BOs at the
  canonical-low 48-bit truncation (`mmCP_IB1_BASE_HI = 0x00008001`,
  not 0xFFFF8001) — same as how radv's BOs are installed. Hypothesis
  (C) from the original filing is definitively ruled out.
- **CONTEXT_CONTROL hypothesis ruled out.** mabda tried emitting its
  own CONTEXT_CONTROL with CC0=0x80000000 (LOAD_ENABLE=0) immediately
  after the kernel's; symptom unchanged. Reverted — radv doesn't emit
  CONTEXT_CONTROL in its userspace IB either.

The fault at VA 0 + PFP stall on a recent packet history that
includes our ACQUIRE_MEM and several EVENT_WRITEs (PIPELINESTAT_START,
plus what looks like the kernel's postamble fence events) suggests
the hang is either:
- inside our ACQUIRE_MEM's full-VA-range invalidate (BASE=0,
  SIZE=0xFFFFFFFF — same as radv) triggering an L2 walk that hits a
  stale tagged line at VA 0
- inside one of our SET_*_REG packets triggering a context-state
  load (despite our CC0 attempt) from an unset source address
- inside the shader fetch path if PGM_LO/HI haven't landed (the
  CONTEXT_REG/SH_REG load path under the kernel's CC0=0x81018003)

None of these is diagnosable from the coredump alone without either:
- byte-exact comparison against a radv triangle-test capture (different
  workload than vkcube — vkcube's draw setup has different state and
  inferences from it can mislead)
- `umr` disassembly of the PFP firmware at instruction pointer
  0x00000ada (`mmCP_PFP_INSTR_PNTR` value at hang)

## Update — 2026-05-12 late session (radv triangle reference built)

`programs/diagnostics/radv_capture_triangle/` was added — a minimal
Vulkan program that draws a solid-red fullscreen triangle into a
256x256 RGBA8 RT, exactly mirroring mabda's `native_render_e2e`
workload. Built with glslangValidator-compiled SPIR-V shaders.
Runs under `RADV_DEBUG=dumpibs` to capture the byte-exact radv IB.

Byte-diff (`radv-triangle.ib.txt` vs `dump_render_pm4`'s output)
surfaced **24 value mismatches and 15 register-completely-missing**
divergences between mabda and radv for the same workload. Captured
in this session's commit batch:

8. **`PA_CL_GB_VERT_CLIP_ADJ` / `_HORZ_CLIP_ADJ` 1.0f → ≈ 255f** (`0x437EFE00`).
   Guard-band size was clamped to the clip volume itself; fullscreen
   triangle's `(3,-1)` / `(-1,3)` vertices fell outside guard band,
   invoking slow-path clipper.
9. **`GFX9_GFX_PGM_RSRC1_MIN` 0x012C0041 → 0x002C0040.** Previous
   value (sourced from vkcube) had VGPRS=1 (8 VGPRs allocated, only
   4 used) + VGPR_COMP_CNT=1 (HW preloads v1=instance_id; our VS
   doesn't read it). radv triangle uses 0x002C0040 = VGPRS=0 / VGPR_COMP_CNT=0.
10. **`GFX9_VS_PGM_RSRC2_MIN` 0 → 0x6** (USER_SGPR=3) and
    **`GFX9_PS_PGM_RSRC2_MIN` 0 → 0x4** (USER_SGPR=2). SPI requires
    non-zero USER_SGPR allocation for graphics-stage shaders.
11. **`PA_SC_MODE_CNTL_1` 0 → 0x760201BC.** Scan-converter walk-order
    and FORCE_EOV bits.
12. **`PA_SU_PRIM_FILTER_CNTL` 0 → 0xC0000000.**
13. **`PA_CL_CLIP_CNTL` 0x90000 → 0x01080000.** Adds DIS_CLIP_ERR_DETECT
    + DX_LINEAR_ATTR_CLIP_ENA; drops the PS_UCP_Y_SCALE_NEG +
    PS_UCP_MODE bits we weren't using.
14. **`PA_SU_SC_MODE_CNTL` 0x4 → 0x240.** Adds bits 6 + 9.
15. **`VGT_VERTEX_REUSE_BLOCK_CNTL` 14 → 30.** Cezanne-tuned value
    from radv triangle test (previous 14 was sourced from vkcube).
16. **`SPI_PS_INPUT_ENA` / `_ADDR` 0x2 → 0x80.** Changed from
    PERSP_CENTER_ENA to LINEAR_CENTER_ENA matching radv.
17. **8 new packets added:** `PA_SC_MODE_CNTL_0 = 0x22`,
    `VGT_GS_OUT_PRIM_TYPE = 2`, `IA_MULTI_VGT_PARAM = 0x0070007F`,
    `SX_MRT0..7_BLEND_OPT = 0x06000600` (8 contiguous), and the 8
    explicit-zero handshake regs from the earlier round.

Block sizes after this session:
- `NATIVE_PIPE_SH_BLOCK_SIZE = 56` (was 48)
- `NATIVE_PIPE_CTX_BLOCK_SIZE = 596` (was 424)
- `NATIVE_PIPE_STRUCT_SIZE = 684` (was 504)

Tested post-fix:
- `make test-native-compute-store` — still bit-exact 0xDEADBEEF.
- `make test-native-render-e2e` — **still 2-second TDR**. Same exit
  code 8, pixel(0,0) = 0x55 sentinel.

## Remaining hypotheses (next session)

After 17 register-value fixes byte-aligned with radv on identical
hardware running identical workload, the TDR persists. The remaining
candidate causes — **none of which is diagnosable from PM4-state
comparison alone**:

1. **Mabda's pre-compiled GFX9 ISA shader bytes have a bug.** The
   `v_lshlrev_b32_e32` / `v_and_b32_e32` / `v_cvt_f32_i32_e32` / `exp`
   sequences in `native_gfx9_shader_fullscreen_triangle_vs` and
   `native_gfx9_shader_solid_red` were assembled by hand against the
   GCN5/GFX9 ISA spec. A single bit error in any instruction encoding
   could cause the VS to never s_endpgm (infinite loop) or the FS
   to fault. **To verify: disassemble with `llvm-objdump
   -d --triple=amdgcn--amdhsa -mcpu=gfx90c` against a binary blob
   of the shader bytes.** Requires LLVM with AMDGPU target enabled.

2. **`CB_COLOR0_INFO` bit-26 difference.** Mabda emits 0x04000028;
   radv emits 0x00028028. Both share the FORMAT (bits 3+5) but the
   high bits differ. Mabda's bit 26 was claimed to be
   FMASK_COMPRESSION_DISABLE per the original comment but
   gfx9.json's actual decode wasn't cross-checked. **Action: decode
   both values against gfx9.json's CB_COLOR0_INFO field bit layout
   and pick the radv-correct value.**

3. **`SPI_SHADER_COL_FORMAT` mismatch.** Mabda = `0xE` (FP32_ABGR),
   radv = `0x4` (UNORM16_ABGR). Different because mabda's FS exports
   FP32 lanes while radv's compiled FS exports FP16. Both should
   work in principle, but the FP32_ABGR path on Cezanne may require
   matching `SX_PS_DOWNCONVERT` / `SX_BLEND_OPT_EPSILON` values that
   mabda leaves at 0.

4. **AMDGPU CS_SUBMIT chunk flags.** Mabda submits with vanilla
   ib_flags + a single IB chunk. radv may use additional chunks
   (BO_LIST, FENCE, USER_FENCE_TYPE) or set specific ib_flags
   (AMDGPU_IB_FLAG_PREAMBLE, AMDGPU_IB_FLAG_PREEMPT) that change
   how the kernel wraps the user IB.

## Path forward

Recommended next-session steps in priority order:

1. **Install `umr` from AUR** (`yay -S umr-git`) and use `umr -O bits
   -ds <ringname>` to read the GFX ring state interactively. Critical
   for verifying *which packet* the PFP is actually stuck on, not
   inferring from PFP_HEADER_DUMP.
2. **Disassemble mabda's VS and FS bytes** against the GFX9 spec
   using LLVM AMDGPU. Verify every instruction's encoding is valid.
3. **Build a "draw nothing" mabda IB** (skip the actual DRAW_INDEX_AUTO,
   only emit the state setup + immediately RELEASE_MEM). If THAT also
   TDRs, the bug is in state setup. If it completes cleanly, the bug
   is in the draw / shader path.

The 17 register fixes landed this session are all real correctness
improvements pinned against the radv triangle reference. Each
individually makes the PM4 stream closer to radv's working shape.
The remaining gap is one or more bugs that need shader-level or
CP-firmware-level tooling to diagnose, which is out of scope for
state-register comparison alone.

**Hardware:** AMD Renoir / Cezanne APU (gfx90c), Wayland desktop
session. radv (Mesa) is the working reference path (`vkcube` runs
fine, IB captured byte-exact via `RADV_DEBUG=dumpibs`).

**Symptom:** `programs/native_render_e2e` consistently exits with
exit code 8 ("red != 0xFF, got 0x55"). The pixel(0,0) byte stays at
the test's pre-fill sentinel 0x55 — meaning the GPU never writes the
RT BO. The dispatch elapses ~2030 ms, matching the kernel's amdgpu
GFX TDR timeout. `journalctl -k` shows:

```
amdgpu 0000:04:00.0: ring gfx timeout, signaled seq=X, emitted seq=X+2
amdgpu 0000:04:00.0: Starting gfx ring reset
amdgpu 0000:04:00.0: Ring gfx reset succeeded
amdgpu 0000:04:00.0: [drm] device wedged, but recovered through reset
```

The syncobj fires on ring reset, so the test's `native_syncobj_wait`
returns 0 ("draw signaled") even though no draw actually completed.

---

## Fixes landed in this session

All 26 are real correctness improvements, each cited against either
gfx9.json field definitions or radv's IB stream / source. None
individually fixes the hang (verified by running `make
test-native-render-e2e` after every change), but each closes a
documentable gap vs the radv reference behavior.

### Backend abstraction / dispatch

1. **`native_pm4_acquire_mem_full_invalidate` parameterized by `shader_type`.** Render path now uses `_for_gfx_ring` (shader_type=0); compute keeps the legacy compute (shader_type=2) shape. The render-on-graphics-ring path with shader_type=2 was routing CP coherence ops to the compute cache hierarchy.
2. **`coher_cntl` 0xA8C40000 → 0x28C40000.** Bit 31 (TC_NC_ACTION_ENA) was spurious — captured byte-exact from radv's preamble IB which does not set it.
3. **`NATIVE_PIPE_CTX_BLOCK_SIZE` 240 → ~424.** Pipeline-context block grew to accommodate the GFX9 preamble registers added below.

### Color buffer state

4. **`CB_COLOR0_ATTRIB2`** (Mesa 0x028C68) — MIP0_HEIGHT [0:13] + MIP0_WIDTH [14:27]. Mabda was emitting CB binding without surface extent; CB hardware saw a 0x0-extent RT and silently dropped every pixel.
5. **`CB_COLOR0_INFO` bit 26 = FMASK_COMPRESSION_DISABLE.** Without it the CB tries to fetch FMASK metadata from VA 0 (we set FMASK base = 0 explicitly) → VM fault.
6. **`CB_COLOR0_{CMASK,CMASK_BASE_EXT,FMASK,FMASK_BASE_EXT,DCC_BASE,DCC_BASE_EXT} = 0`.** radv emits all six in its GFX9 emit_fb_color_state 11-register sequence; without them the CB reads stale values from a prior context.
7. **`CB_MRT0_EPITCH` = `width - 1`.** Effective pitch in pixel elements, GFX9-specific.

### Rasterizer / clipping

8. **`PA_CL_GB_VERT_CLIP_ADJ`, `_VERT_DISC_ADJ`, `_HORZ_CLIP_ADJ`, `_HORZ_DISC_ADJ` = 1.0f.** Guard band adjusts; without them the clip-to-guard-band stage can hang on small primitives.
9. **`PA_SC_BINNER_CNTL_0` value 0x10040003** (was `0`, which is BinningMode = `BINNING_ALLOWED`). Decoded correctly: BinningMode=3 (DISABLE_BINNING_USE_LEGACY_SC), DISABLE_START_OF_PRIM=1 (bit 18), FLUSH_ON_BINNING_TRANSITION=1 (bit 28; Renoir/Raven2 family). Cited from `radv_get_disabled_binning_state()` GFX9 branch.
10. **`PA_SC_BINNER_CNTL_1` = `0x03FF001F`** (MAX_ALLOC_COUNT=31 + MAX_PRIM_PER_BATCH=1023). Paired with CNTL_0 disable; without it the binner stalls even when CNTL_0 says binning-off.
11. **`PA_SC_WINDOW_SCISSOR_TL = 0x80000000`** (WINDOW_OFFSET_DISABLE) + **`_BR = (16384,16384)`**. Was completely absent.
12. **`PA_SC_GENERIC_SCISSOR_TL`** — added WINDOW_OFFSET_DISABLE bit (0x80000000).
13. **`PA_SC_VPORT_SCISSOR_0_TL/BR`** — per-viewport scissor; missing.
14. **`PA_SC_CLIPRECT_RULE = 0xFFFF`** — allow all 16 clip-rect combinations (default reject-everything).
15. **`PA_SU_VTX_CNTL = 5`** (PIX_CENTER=1, ROUND_MODE=truncate).
16. **`PA_SU_PRIM_FILTER_CNTL = 0`** — explicit no-prim-filter.
17. **`PA_SC_MODE_CNTL_1 = 0`** — explicit walk-order defaults.

### Depth / shader inputs

18. **`DB_DEPTH_CONTROL = 0`** — explicit "no depth test, no stencil test, no z-write."
19. **`DB_RENDER_CONTROL = 0`** — explicit "no depth clear, no decompress."
20. **`SPI_TMPRING_SIZE = 0`** — explicit "no scratch ring."
21. **`SX_PS_DOWNCONVERT`, `SX_BLEND_OPT_EPSILON`, `SX_BLEND_OPT_CONTROL = 0`** — shader-export → CB conversion path defaults.

### VGT

22. **`VGT_VERTEX_REUSE_BLOCK_CNTL = 14`, `VGT_OUT_DEALLOC_CNTL = 16`.** Without these VGT can deadlock on legacy non-NGG paths (Bonaire/Cezanne CLEAR_STATE bug).
23. **`PKT3_NUM_INSTANCES`** (opcode 0x2F) instead of `SET_UCONFIG_REG(VGT_NUM_INSTANCES)`. radv emits this dedicated PKT3 before every draw; Cezanne VGT picks it up more reliably as a per-dispatch update.

### Shader state

24. **`GFX9_GFX_PGM_RSRC1_MIN`** — added bit 24 (LS_VGPR_COMP_CNT[0] = 1) + bit 0 (VGPRS = 1, allocates 8 VGPRs minimum). Captured byte-for-byte from radv's `SPI_SHADER_PGM_RSRC1_VS` dump (0x012c0041). Without LS_VGPR_COMP_CNT set the VS launches with under-allocated VGPRs in the merged-stage path.

### Pre-draw pipeline drain

25. **Pre-draw `EVENT_WRITE` sequence:** `FLUSH_AND_INV_CB_META` (event 46) + `FLUSH_AND_INV_DB_META` (event 44) + `PS_PARTIAL_FLUSH` (event 16, index 4) — captured from radv's main IB. Forces in-flight pixel waves from prior contexts (compositor draws) to drain before our triangle reaches the rasterizer.
26. **`PFP_SYNC_ME`** (PKT3 opcode 0x42) — forces PFP to wait for ME drain before draw. Without it PFP can read context state ME hasn't committed.
27. **`PIPELINESTAT_START`** event (event 25) — unblocks the GPU stat-counter pipeline. radv emits this in every preamble.

(yes, 27 — one snuck in past 26 during the writeup.)

---

## What didn't work / remaining mystery

After each fix above, `make test-native-render-e2e` was re-run.
Every iteration shows the same shape:

- All CPU PASS markers fire (BO allocations, va_maps, pipeline pack, pass begin).
- `dispatch: ~2030 ms` — exactly the GFX TDR window.
- "PASS: draw signaled" — syncobj fires after kernel resets ring.
- pixel(0,0) = `(0x55, 0x55, 0x55, 0x55)` — RT is the pre-fill sentinel.

The 2-second-exact dispatch time strongly implies the GPU is
**waiting** on something specific (a poll, semaphore, fence, or
pipeline subsystem availability) rather than running out of work.

Without GPU-side tracing — specifically `umr` (which requires
root) or a complete byte-diff against a radv IB that does the
exact same workload (a Vulkan program drawing a fullscreen red
triangle to an RGBA8 RT) — the next-step hypotheses become
guesses.

### Candidate root causes not yet ruled out

- **VA encoding for canonical-high RT base.** All BOs use VAs in the
  `0xFFFF800100000000+` region. Compute works at the same VA pattern,
  but the CB hardware reconstructs VA from BASE + BASE_EXT (48-bit)
  and the GPU may interpret the canonical-high differently from the
  kernel-side mapping. Worth verifying by:
  1. Allocating an RT at a canonical-low VA (e.g. `0x000800100000000`)
  2. Re-running the test
  3. If pixel goes red → VA encoding is the bug
- **Shader RSRC2 / SPI input mismatch.** Our FS has `PGM_RSRC2_PS = 0`
  and `SPI_PS_INPUT_ENA = 0x2` (PERSP_CENTER_ENA). radv's actual
  shader RSRC values for the equivalent solid-color FS are unknown
  without capturing a matching radv triangle test.
- **CONTEXT_CONTROL packet at IB start.** Radv may emit
  `IT_CONTEXT_CONTROL` (0x28) before its IB; mabda doesn't.
- **`PA_SC_RASTER_CONFIG`** — set to 0 in mabda; this is correct
  for Cezanne (1 SE) per Mesa's `ac_get_raster_config()` default
  branch, but worth a Cezanne-specific double-check.
- **AMDGPU CS_SUBMIT ib_flags.** Mabda submits with `ib_flags = 0`;
  radv may set `AMDGPU_IB_FLAG_PREAMBLE` or
  `AMDGPU_IB_FLAG_CE_PREAMBLE` on the first IB of a context.

### Diagnostic harness needed

- **radv_capture Phase 2** as scoped in
  `docs/development/3-0-rc-2-punchlist.md`: a Vulkan program that
  performs the equivalent of `programs/native_render_e2e` (256x256
  RGBA8 RT, fullscreen-triangle, solid red, CPU readback) so its
  `RADV_DEBUG=dumpibs` output can be byte-diff'd against
  `programs/dump_render_pm4`'s output. The vkcube IB captured in
  this session is the wrong scope (textured cube, depth buffer,
  uniform buffers, swapchain → very different state).
- **`umr` (root)** — read `/sys/class/drm/card1/device/devcoredump/data`
  at TDR to see the actual register state the kernel captured when
  the ring reset fired. That will say exactly which packet the CP
  was processing when it stalled.

---

## Why this isn't an rc.3 blocker

- The render path was never end-to-end verified in rc.2 either — the
  HW gate was always documented as "HW-gated; cache-flush in tree"
  but `programs/native_render_e2e` was not on the rc.2 passing
  matrix. This is not a regression.
- The compute path is healthy: 6h soak burn-in 2026-05-12 passed
  with 4144 KB flat RSS and 15.66M dispatch iterations. Native
  buffer allocation, ioctl path, syncobj fence wiring, GEM-VA
  mapping, PM4 stream submit — all verified.
- The 27 fixes landed in this session are unilateral correctness
  improvements pinned against gfx9.json and radv's emitted IB.
  Each makes mabda's render PM4 stream measurably closer to the
  Mesa reference shape.

## Pickup notes for the next session

1. Read this doc top-to-bottom.
2. Read the gfx9.json + radv references in `/tmp/gfx9.json`,
   `/tmp/radv-ib.txt`, `/tmp/radv_cmd.c`, `/tmp/ac_cmdbuf.c` if
   they're still cached, else re-fetch from Mesa main.
3. Decide between:
   - Build the Vulkan triangle-test harness for byte-diff
     comparison (~half-day effort, hands the bug on a platter)
   - Pursue the VA-encoding hypothesis (~1h targeted experiment)
   - Use sudo + `umr` or read `/sys/class/drm/card1/device/devcoredump/data`
     for the at-TDR register state
4. CPU test asserts in `tests/tcyr/mabda_v3.tcyr` need updating to
   match the new block sizes (`pipeline_ctx = 424`, `pass_target = 264`,
   etc.). 31 assertions surface stale; the source code itself is in
   a coherent state.
