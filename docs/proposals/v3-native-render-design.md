# v3 Native Render Pipeline — design

**Status:** Draft (v3 branch, 2026-04-28 — written immediately after Phase C textures landed)
**Related:**
[ADR 006](../adr/006-native-cyrius-gpu-backend.md) (dual-backend),
[v3-backend-interface](v3-backend-interface.md) (the slot signatures this design will extend),
[B.4 verified handoff](../handoff/2026-04-28-session25-b4-verified.md)
(the working compute-dispatch lineage we're extending)

## Framing

Phase C textures done on both backends. **Phase C render path is
the next chunk** — the punch list breaks it into sub-steps 6.1–6.9.
This is the design doc that 6.2 onward implement against. It is
NOT exhaustive — only covers what's needed for a *minimum viable
triangle clear* matching `programs/render_e2e.cyr` shape. Format
breadth, MSAA, depth/stencil, instancing, indexed draws — all
deferred.

The compute path (Steps 4a–4e) gives us the foundation: PM4
packet builders, BO + va_map primitives, 4-chunk submit, syncobj
wait, the cached IB+fence ring. Render extends rather than replaces;
roughly 70% of the infrastructure is shared.

## What's different from compute

The compute path runs on the **MEC** (compute micro-engine):

```
amdgpu_cs ioctl (ip_type = AMDGPU_HW_IP_COMPUTE)
  → MEC ring (comp_1.x.y in dmesg)
  → CP processes PM4 packets:
      SET_SH_REG (compute shader state)
      SET_UCONFIG_REG (TA_CS_BC_BASE, RESOURCE_LIMITS, etc.)
      ACQUIRE_MEM (cache invalidation)
      DISPATCH_DIRECT (workgroup launch)
  → Compute wave executes in CUs
  → DISPATCH_DIRECT completion → user fence + syncobj signal
```

The render path runs on the **graphics ring** (gfx in dmesg) via
the **ME (main engine)** + **PFP (prefetch parser)**:

```
amdgpu_cs ioctl (ip_type = AMDGPU_HW_IP_GFX)
  → graphics ring
  → CP-PFP prefetches packets, CP-ME executes:
      SET_CONTEXT_REG (graphics-only register family)
      SET_SH_REG (vertex + fragment shader state)
      SET_UCONFIG_REG (graphics top-level state)
      EVENT_WRITE (PIPELINESTAT_START etc.)
      DRAW_INDEX_AUTO (or DRAW_INDEX_2 for indexed)
  → Vertex shader → primitive assembly → rasterizer → fragment shader
  → Color/depth targets
  → DRAW_* completion → user fence + syncobj signal
```

**Three big additions over compute:**

1. **`SET_CONTEXT_REG` packet family.** Graphics has a "context"
   state (rasterizer, viewport, target binding) tracked separately
   from per-shader state. PM4 opcode `0x69` (`PKT3_SET_CONTEXT_REG`).
   Compute didn't need this — only `SET_SH_REG` + `SET_UCONFIG_REG`.
2. **Two shaders per pipeline** instead of one. Vertex shader
   (mapped through SPI_SHADER_PGM_*_VS) and fragment shader
   (SPI_SHADER_PGM_*_PS). Each has its own `LO/HI/RSRC1/RSRC2`.
3. **Render target attachment.** `CB_COLOR0_*` registers point at
   a render-target BO with format/dimensions/swizzle. Without this,
   the fragment shader's writes go nowhere.

## Required register state — minimum viable triangle

The shape that produces `programs/render_e2e.cyr`'s "fill 256x256
RGBA8 with red" output, byte-exact, on GFX9. References to the
Mesa `radv` state setup throughout — radv emits these packets via
`AMD_DEBUG=ib` and we mirror byte-exact, same protocol as Phase B.4.

### SH (per-shader) registers

| Register | Purpose |
|----------|---------|
| `SPI_SHADER_PGM_LO_VS / _HI_VS` | Vertex shader VA (>>8 / >>40 split, same encoding as compute PGM_LO/HI) |
| `SPI_SHADER_PGM_RSRC1_VS` | VGPRS / SGPRS / FLOAT_MODE for VS (mirror RSRC1_MIN from compute) |
| `SPI_SHADER_PGM_RSRC2_VS` | USER_SGPR / VGT_PRIMITIVEID_EN — we want USER_SGPR=4 for vertex-buffer-binding-table-via-SGPR |
| `SPI_SHADER_USER_DATA_VS_0..3` | Vertex shader USER_DATA — points at the vertex buffer or the fragment shader's color uniform |
| `SPI_SHADER_PGM_LO_PS / _HI_PS` | Fragment shader VA |
| `SPI_SHADER_PGM_RSRC1_PS / _RSRC2_PS` | RSRC for PS |
| `SPI_SHADER_USER_DATA_PS_0..3` | PS USER_DATA — points at the color buffer or push-constants |

Offsets: see Mesa `src/amd/common/sid.h` in `radv` — every
`SPI_SHADER_*` constant has a byte offset; `native_sh_reg_offset`
in `src/backend_native.cyr` already does the
`(offset - 0xB000) / 4` translation for the PM4 wire format.

### Context (graphics-only) registers

| Register | Value for minimum-viable triangle |
|----------|-----------------------------------|
| `PA_SC_SCREEN_SCISSOR_TL` | (0, 0) — no scissor |
| `PA_SC_SCREEN_SCISSOR_BR` | (W, H) — render target dims |
| `PA_CL_VPORT_XSCALE`, `_XOFFSET`, `_YSCALE`, `_YOFFSET` | Viewport mapping NDC → pixel space |
| `PA_CL_CLIP_CNTL` | `0x90000` — DX_CLIP_SPACE_DEF + Z_CLIP_NEAR_DISABLE typical |
| `CB_TARGET_MASK` | `0xF` — write all 4 channels of color target 0 |
| `CB_COLOR_CONTROL` | `0xCC` — DEFAULT_BLEND_OP_DEST=NONE, NORMAL ROP |
| `CB_COLOR0_BASE` | Render target BO VA `>> 8` |
| `CB_COLOR0_INFO` | Format + tile mode (`COLOR_8_8_8_8 | NUMBER_UNORM | RGB`) |
| `CB_COLOR0_PITCH` | Render target row pitch — `(width / 8) - 1` for linear |
| `CB_COLOR0_SLICE` | `(width * height / 64) - 1` |
| `CB_COLOR0_VIEW` | 0 (no array slicing for 2D single-layer) |
| `CB_COLOR0_ATTRIB` | Tile mode + swizzle |
| `DB_HTILE_SURFACE` | 0 — no HTILE for first cut |
| `PA_SC_RASTER_CONFIG` | Per-shader-engine raster mask — Cezanne has 1 SE, so just `0` |

### UConfig registers (also used by compute)

These mostly carry over from `native_pm4_build_compute_store_deadbeef`:

| Register | Notes |
|----------|-------|
| `CP_COHER_START_DELAY` | Same as compute |
| `TA_CS_BC_BASE_ADDR / _HI` | Same as compute (verified `0/0` works on Cezanne) |
| `VGT_PRIMITIVE_TYPE` | `4` = `PT_TRIANGLELIST` |
| `VGT_NUM_INSTANCES` | 1 |
| `VGT_INDEX_TYPE` | n/a for `DRAW_INDEX_AUTO` |

## PM4 packet families

| Opcode | Name | Used by |
|--------|------|---------|
| `0x76` | `PKT3_SET_SH_REG` | Already in `native_pm4_set_sh_reg` |
| `0x68` | `PKT3_SET_CONFIG_REG` | Compute ACQUIRE_MEM uses this — already present |
| `0x79` | `PKT3_SET_UCONFIG_REG` | Already in `native_pm4_set_uconfig_reg_one/_pair` |
| `0x69` | `PKT3_SET_CONTEXT_REG` | **NEW** — graphics-only context-register write |
| `0x46` | `PKT3_EVENT_WRITE` | Graphics events (PIPELINESTAT_START, FLUSH_AND_INV_*) |
| `0x2D` | `PKT3_DRAW_INDEX_AUTO` | **NEW** — indirect-instance-count draw |
| `0x36` | `PKT3_DRAW_INDEX_2` | Indexed draws (deferred — 6.x+) |
| `0x10` | `PKT3_NOP` | Already in `native_pm4_nop` |

The two new opcodes — `SET_CONTEXT_REG` and `DRAW_INDEX_AUTO` —
land as new builders in `src/backend_native.cyr` during step 6.4.
Same byte-exact protocol: build via Cyrius, diff against
`AMD_DEBUG=ib` from `radv` running an equivalent test, fix any
off-by-one until they match.

### `SET_CONTEXT_REG` byte layout

```
+0:  PKT3 header — type=3, count_minus_1=N, opcode=0x69, predicate=0
+4:  reg_offset_dwords — (byte_offset - 0xA000) / 4
+8:  value_0
+12: value_1
...
```

Where `0xA000` is the context-register base (vs `0xB000` for SH
registers and `0x30000` for UConfig). Same encoding pattern as
the existing SH/UConfig packets.

### `DRAW_INDEX_AUTO` byte layout

```
+0:  PKT3 header — type=3, count_minus_1=1, opcode=0x2D, predicate=0
+4:  index_count (number of vertices)
+8:  draw_initiator (DI_SRC_SEL_AUTO_INDEX | DI_PARTIAL_VS_WAVE_OFF)
```

3 dwords total. Caller emits this AFTER all SH/UConfig/Context state.

## Vertex + fragment shader ABI (GFX9, amdgcn)

Vertex shader (`-target amdgcn--amdhsa -mcpu=gfx90c`):

- Inputs in SGPRs: USER_DATA_VS_0..3 (4 SGPRs) + system VGPRs
  (vertex_id in v0, instance_id in v1).
- Outputs to **export instructions** (`exp pos0 v0,v1,v2,v3` for
  position; `exp param0 ...` for varyings).
- Minimum-viable shader for a hardcoded full-screen triangle:
  3 vertices output as positions `(-1,-1)`, `(1,-1)`, `(0, 1)` —
  pulled from a `switch (vertex_id)`-style sequence. Mirror the
  WGSL `programs/phase0.cyr` Test 8 shader.

Fragment shader:

- Inputs in VGPRs: barycentric coordinates (interpolated by HW),
  plus interpolated varyings.
- Output via `exp mrt0 v0,v1,v2,v3` (4 channels, NONE = format
  conversion happens here).
- Minimum-viable shader: write `(1.0, 0.0, 0.0, 1.0)` to mrt0.

Bytes generated via `clang -target amdgcn--amdhsa -mcpu=gfx90c -O2`
on small `.cl` files, same toolchain that produced
`native_gfx9_shader_store_deadbeef`. Step 6.2 freezes the byte
strings as `var GFX9_SHADER_VS_FULLSCREEN_TRIANGLE` and
`var GFX9_SHADER_PS_SOLID_RED` constants.

## Render target binding

`CB_COLOR0_*` registers point at a **render-target BO** that
needs:

1. **Format encoding:** `CB_COLOR0_INFO` packs format + number-format
   + comp_swap. For RGBA8 UNORM: format=`COLOR_8_8_8_8`,
   number=`NUMBER_UNORM`, comp_swap=`SWAP_STD` (RGBA byte order).
2. **Pitch:** `CB_COLOR0_PITCH` carries `(width / 8) - 1` for
   linear-layout RT. Linear means "no tiling" — same as the texture
   path's `_NATIVE_PERM_DATA` BOs from Step 5.x.
3. **Tile mode:** `CB_COLOR0_ATTRIB` encodes the AMD tile mode
   (linear, micro-tiled, swizzled, DCC). Linear is the simplest
   and what we use; matches Mesa radv's "linear path" used for
   readback BOs.

Render target BO layout (proposed for 6.3):

```
NativeRenderTarget struct (32 bytes — same shape as NativeTexture
on purpose; render targets ARE textures with a specific usage):
  +0:  handle (u64, KMS handle)
  +8:  va     (u64, GPU VA)
  +16: addr   (u64, CPU mmap)
  +24: size   (u64, bytes)
```

`native_render_target_create_2d_rgba8(fd, w, h, out)` reuses the
texture allocator pattern from Step 5.1 — RT VA range at, say,
`0xFFFF800101000000` (next 16 MiB slot after textures). Larger
default headroom because RTs scale with display resolution.

## Hardware references

Source-of-truth for byte-exact PM4 sequences:

- **Mesa `radv`** (`src/amd/vulkan/`) — Vulkan driver, emits PM4
  for graphics dispatches. `radv_emit_graphics_pipeline_state`,
  `radv_emit_color_target`, `radv_set_context_state`. Inspect
  via `RADV_DEBUG=spirv` + `AMD_DEBUG=ib` running a Vulkan
  triangle program.
- **Mesa `radeonsi`** (`src/gallium/drivers/radeonsi/`) — older
  Gallium driver, also emits PM4 for OpenGL+. `si_emit_*` family.
- **amdgpu kernel** (`drivers/gpu/drm/amd/amdgpu/`) —
  `gfx_v9_0.c` / `gfx_v9_0_ring.c` for ring init, register
  ranges, and reset paths. Ground truth for what register
  values the firmware accepts.
- **AMD GFX9 shader ISA spec** (public PDF, "GCN5 ISA Reference
  Guide" / "Vega 10 Shader ISA") — ISA encoding for vertex /
  fragment shader instruction sets. We already use this for
  `native_gfx9_shader_store_deadbeef`.

The Phase B.4 protocol applies: build PM4 stream, run with
`AMD_DEBUG=ib` to dump our IB, dump radv's IB for an equivalent
operation, diff byte-by-byte. Mismatches resolve before hardware
testing — Phase B.4 burned 10+ sessions on byte-mismatches that a
careful diff would have caught.

## Migration sequencing — chunks 6.2–6.9

Already in the punch list; restating with cross-references to this
doc:

- **6.2** — Vertex+fragment shader bytes (`GFX9_SHADER_VS_*`,
  `_PS_*`). Same `clang -target amdgcn` workflow. Frozen as
  `var` constants. CPU regression test: byte-exact vs
  hand-written reference.
- **6.3** — `native_render_target_create_2d_rgba8` primitive
  (this doc § Render target binding). Mirrors Step 5.1 texture
  allocator pattern.
- **6.4** — `native_pm4_set_context_reg` builder + `native_pm4_draw_index_auto`
  builder (this doc § PM4 packet families). New PM4 emitters,
  byte-exact tested vs radv IB dump.
- **6.5** — `native_pm4_build_render_clear_triangle(buf, …)`
  composes the full PM4 stream for a minimum-viable triangle
  clear. Mirror of `native_pm4_build_compute_store_deadbeef`
  shape from Step 4a. Tests assert byte sequence vs radv reference.
- **6.6** — `native_render_dispatch_simple(ctx, …)` analogous
  to Step 4b's `native_compute_dispatch_cached` — uses the same
  cached IB+fence from `gpu_context_new_native` on the **GFX
  ring** (`AMDGPU_HW_IP_GFX` instead of `AMDGPU_HW_IP_COMPUTE`).
- **6.7** — Backend interface render-pipeline + render-pass
  slots. Six new slots: `render_pipeline_create`,
  `render_pipeline_release`, `render_pass_begin`, `render_pass_end`,
  `draw`, `render_target_create_2d_rgba8`. Backend struct grows
  120 → 168 bytes.
- **6.8** — `_backend_wgpu_*` and `_backend_native_*` slot
  fillers. wgpu side mostly delegates to existing render
  primitives; native side wraps 6.3–6.6.
- **6.9** — `programs/native_render_e2e.cyr` (mirror of
  `programs/render_e2e.cyr`). Hardware byte-exact test:
  64×32 RT cleared to red, CPU-readback verifies pixel(0,0)
  == `(0xFF, 0x00, 0x00, 0xFF)`.

## What 6.x deliberately defers (to Phase D / v3.x)

- **Indexed draws** (`DRAW_INDEX_2`). Auto-index + hardcoded triangle
  is enough for the clear-triangle exit criterion.
- **Vertex buffers.** Hardcoded vertex shader emits positions
  inline; no VBO binding required for the minimum-viable program.
- **Multiple render targets** (MRT). `CB_TARGET_MASK = 0xF` writes
  to color target 0 only; `CB_COLOR1+` regs left zero.
- **Depth + stencil.** `DB_*` registers left at HW reset values;
  `db_format = INVALID` skips the depth path entirely.
- **MSAA.** Sample count = 1; `PA_SC_AA_CONFIG` left zero.
- **Tiling beyond linear.** Optimal-tiled / DCC / micro-tiled
  render targets are post-v3.0 perf work.
- **Vulkan-style render passes.** `render_pass_begin/end` slots
  emit the minimal "load color → clear → render → store"
  sequence for v3.0; full subpass / dependency tracking is
  v3.x consumer catch-up.
- **Surface acquire / present.** That's Phase D 7.x. 6.x renders
  to an offscreen RT only — readback via `copy_texture_to_buffer`
  (compute path) for the hardware-test program.

## Open questions

These get answered during 6.2–6.9 implementation, not now:

1. **Linear vs micro-tiled RT?** Linear simpler, slower. Spec-
   wise GFX9 supports linear RTs; Mesa radv uses linear for
   readback BOs. Going linear; revisit if we hit a HW wall.
2. **Vertex shader USER_DATA layout.** The ABI for "I'm a self-
   contained shader with no vertex buffers" is a bit fuzzy. Mesa
   radv's hardcoded fullscreen triangle path is the reference;
   diff our SPI_SHADER_USER_DATA_VS_* writes against radv's.
3. **Position output format.** GFX9's vertex export takes 4
   floats (xyzw) at position 0. Verify clang emits them in the
   right order via `objdump -d` on the compiled .so.

## Filing trail

Filed as Step 6.1 of the v3.0 punch list (2026-04-28). Lays the
contract for steps 6.2–6.9. Update this doc as 6.2+ uncover
deviations from the design; the Phase B.4 handoff has a strong
"document falsified hypotheses" tradition that pays off when a
later session needs to know what we tried and why it didn't
work.
