# v3 Backend Interface — `@internal` dispatch surface

**Status:** Draft, v2.1 (v3 branch, 2026-04-28 — v0 written after Phase B.4 verified; v1 added Step 5.3 with texture slots; v2 added Step 6.7 with render-pipeline + render-pass + render-target slots; v2.1 corrected post-research — clear-color format and the GFX9 PM4 block split)
**Related:**
[ADR 006](../adr/006-native-cyrius-gpu-backend.md) (dual-backend),
[ADR 005](../adr/005-public-api-surface-marking.md) (`@public` boundary),
[v3-native-api-principles](v3-native-api-principles.md) (consumer-facing API),
[B.4 verified handoff](../handoff/2026-04-28-session25-b4-verified.md)

## Framing

ADR 006's follow-up bullet 1 reads: *"Flesh out the `Backend`
interface in a proposal under `docs/proposals/` before writing
`src/backend.cyr`."* This is that proposal.

It is **not** the consumer API design — that's
`v3-native-api-principles.md`. This proposal sets the
`@internal` indirection point both backends will land on. The
public API never sees this surface.

## Why now

Phase B.4 just landed. Both halves of the dual-backend story now
exist in code:

- `src/wgpu_ffi.cyr` (65-slot fn-pointer table) + `src/buffer.cyr`,
  `src/compute.cyr`, `src/texture.cyr`, etc. — the `wgpu` path,
  shipping today.
- `src/backend_native.cyr` (40+ `native_*` primitives) — the
  pure-Cyrius DRM/KMS path, proven end-to-end via
  `programs/native_compute_store.cyr`.

What does **not** exist: a single function-pointer table that
either backend can fill, that the `@public` API calls into without
caring which is wired up. Without it, every `@public` operation
that wants to stay backend-agnostic has to branch internally —
and we lose the ADR 005 promise (the public surface is the
contract; everything below is implementation).

## Anchor — what the working dispatch actually exercised

The Backend interface's first concrete purpose is to host the
operations `native_compute_store` already performed. Naming the
operations after the proven flow, not a speculative superset, is
the only honest starting point.

The flow, in order:

1. **Open device** — `native_open_render_node()` on native;
   `wgpu_create_instance` + adapter request on wgpu.
2. **Create context** — kernel ctx_id on native; device + queue on wgpu.
3. **Allocate BO + map to VA** — five calls in the program:
   shader / stub / output / IB / fence. Each is GTT-allocated,
   memory-mapped for CPU writes, and VA-mapped for GPU reads.
4. **Build IB** — PM4 register-setup + dispatch + post-dispatch
   marker. Native-only artifact; `wgpu` builds equivalent state
   internally via `wgpuComputePassEncoderDispatchWorkgroups`.
5. **Submit + wait** — 4-chunk submit on native; queue submit + fence
   on wgpu.
6. **Readback** — CPU read of `out_va`. Identical on both backends
   from the consumer's perspective.

Every operation above maps to a `Backend` slot. PM4 IB construction
does not — it's a native-implementation detail that hides behind
`backend_compute_dispatch`.

## The Backend struct

`Backend` is a fixed-offset struct of `fnptr` slots. One slot per
operation. `GpuContext` carries a pointer to its `Backend`; every
backend-aware function reads `ctx->backend->slot` and `fncall`s
through it.

### Layout (v0 — minimum to host the proven compute flow)

```text
Backend struct (88 bytes — 11 slots × 8 bytes):
  +0:   ctx_create_from_preinit  (preinit_ptr)        → ctx
  +8:   ctx_release              (ctx)                → 0
  +16:  buffer_create            (ctx, size, usage)   → buf_handle
  +24:  buffer_write             (ctx, buf, off, ptr, n) → 0|err
  +32:  buffer_read              (ctx, buf, off, ptr, n) → 0|err
  +40:  buffer_release           (ctx, buf)           → 0
  +48:  shader_module_create     (ctx, bytes_ptr, n)  → mod
  +56:  shader_module_release    (ctx, mod)           → 0
  +64:  compute_dispatch         (ctx, mod, x, y, z, bindings_ptr) → 0|err
  +72:  device_wait_idle         (ctx)                → 0
  +80:  backend_kind             (ctx)                → BACKEND_WGPU | BACKEND_NATIVE
```

11 slots. Smaller than the wgpu_ffi 65-slot table because that
table is "every wgpu function the codebase calls"; this one is
"every operation the consumer-facing API calls *through the
abstraction*."

`bindings_ptr` is a raw byte buffer of `(slot, kind, handle)`
triples — bindless descriptor shape per `v3-native-api-principles`
non-negotiable #2. Bind-group / layout objects are not in the
interface. The native side packs into USER_DATA registers; the
wgpu side translates to wgpu bind groups internally.

### What's deferred (named, not in v0)

These exist in mabda today on the wgpu path. They're not in v0
because the native path doesn't have them yet, and shipping the
interface with stub-`return GPU_ERR_NOT_IMPLEMENTED` slots that
don't have a native test path is the wrong move:

| Operation | Gating | Status |
|-----------|--------|--------|
| `texture_create / write / read / release` | native texture format/tiling work (Phase C) | **v1 — design landed Step 5.3, code landed Steps 5.4–5.9** |
| `render_target_create / release`, `render_pipeline_create / release`, `render_pass_begin / draw / end` | native graphics ring path + GFX9 shader bytes (Phase C 6.x) | **v2 — design landed Step 6.7, code lands Step 6.8 (gated on 6.2 + 6.5 + 6.6)** |
| `surface_configure / acquire / present` | DRM/KMS scanout path (Phase D) | deferred to Phase D 7.x chunks |

Each gets added to `Backend` when its native counterpart has a
working e2e program. Fail-loud (interface compile-error) is better
than fail-silent (`GPU_ERR_NOT_IMPLEMENTED` at runtime).

## v1 expansion — texture slots (Step 5.3)

The native side already has linear 2D RGBA8 texture create/release
primitives (Steps 5.1 + 5.2). Step 5.4 promotes them to first-class
Backend slots so the public API can dispatch through the abstraction
the same way `gpu_buffer_*` does.

### Layout (v1 — appends 4 texture slots)

```text
Backend struct (120 bytes — 14 slots × 8 bytes; grew from v0's 88):
  +0:   ctx_create_from_preinit  (preinit_ptr)              → ctx        (v0)
  +8:   ctx_release              (ctx)                      → 0          (v0)
  +16:  buffer_create            (ctx, size, usage)         → buf        (v0)
  +24:  buffer_write             (ctx, buf, off, ptr, n)    → 0|err      (v0)
  +32:  buffer_read              (ctx, buf, off, ptr, n)    → 0|err      (v0)
  +40:  buffer_release           (ctx, buf)                 → 0          (v0)
  +48:  shader_module_create     (ctx, bytes_ptr, n)        → mod        (v0)
  +56:  shader_module_release    (ctx, mod)                 → 0          (v0)
  +64:  compute_dispatch         (ctx, mod, x, y, z, bp)    → 0|err      (v0)
  +72:  device_wait_idle         (ctx)                      → 0          (v0)
  +80:  kind                     u64 value: BACKEND_KIND_*               (v0)
  +88:  texture_create_2d_rgba8  (ctx, width, height)       → tex_ptr   (v1)
  +96:  texture_write            (ctx, tex_ptr, src, n)     → 0|err     (v1)
  +104: texture_read             (ctx, tex_ptr, dst, n)     → 0|err     (v1)
  +112: texture_release          (ctx, tex_ptr)             → 0         (v1)
```

**Why append after kind:** keeps every v0 slot offset stable
(BACKEND_SLOT_CTX_RELEASE = 8, BACKEND_SLOT_KIND = 80, etc.). All
existing tests that assert v0 layout pass unchanged. Future
expansions (render_pipeline, surface) follow the same rule —
append at the next available offset, never re-shuffle.

### Slot signatures, per backend

#### `texture_create_2d_rgba8 (ctx, width, height) → tex_ptr`

Returns an opaque `tex_ptr` (u64). Layout is **backend-specific**:

- **wgpu**: pointer to a 32-byte struct `{handle: WGPUTexture,
  view: WGPUTextureView, _pad: 0, size_bytes: w*h*4}`. The wrapper
  creates both texture + default 2D view in one shot since
  consumers always want the view.
- **native**: pointer to the existing `NativeTexture` struct from
  Step 5.1 — `{handle: KMS u32, va: u64, addr: u64, size: u64}`.
  32 bytes; same shape as the wgpu side coincidentally, simplifies
  caller assumptions.

Caller treats `tex_ptr` as opaque. Both backends allocate the
struct on the heap; caller releases via `texture_release`.

#### `texture_write (ctx, tex_ptr, src, n) → 0|err`

Write `n` bytes from `src` into the texture's pixel data. Linear
RGBA8 layout means src is a flat array of `width × height × 4`
bytes (top-to-bottom, left-to-right, RGBARGBARGBA…).

- **wgpu**: `wgpu_queue_write_texture` with `WGPUTexelCopyTextureInfo`
  pointing at the texture, `bytesPerRow = width * 4`,
  `rowsPerImage = height`. Same parameters as the existing
  `wgpu_queue_write_texture` consumers.
- **native**: `memcpy(tex.addr, src, n)`. Linear-layout textures
  in GTT are CPU-accessible; no GPU involvement.

`n` must equal `tex.size` for v1; partial writes deferred to a
future iteration (consumer driver: when staging buffers come into
play).

#### `texture_read (ctx, tex_ptr, dst, n) → 0|err`

Read `n` bytes from the texture's pixel data into `dst`. Symmetric
with `texture_write`.

- **wgpu**: copy_texture_to_buffer + map_sync + memcpy + unmap.
  Same staging dance as `_backend_wgpu_buffer_read` (Step 2).
- **native**: `memcpy(dst, tex.addr, n)`. Trivial.

#### `texture_release (ctx, tex_ptr) → 0`

Frees both the GPU resources and the heap struct itself. Caller
must not access `tex_ptr` after release (current code doesn't
zero the caller's pointer; that's a `gpu_texture_release` public
fn responsibility).

- **wgpu**: `wgpu_texture_release(handle)` + `wgpu_texture_view_release(view)` + free struct.
- **native**: `native_texture_release_2d_rgba8(fd, tex)` (Step 5.2,
  zeroes the struct in place) + free(tex_ptr).

### What v1 still defers

- **Mipmaps.** RGBA8 only, no mip levels. Mipped textures + on-GPU
  generation land in v3.1 per the roadmap (consumer catch-up).
- **Other formats.** RGBA32_FLOAT, BGRA8, depth formats, BC/ETC2/
  ASTC compressed — all deferred. RGBA8 covers the
  `examples/stdlib-consumer/` exit criterion; the rest is v3.2.
- **Tiling.** Native side is linear-only; AMD optimal-tiling
  (DCC, micro-tiled, swizzled) is performance work post-v3.0.
- **GPU-side writes.** Compute shaders writing into a texture is a
  v3.x follow-up; v1 only supports CPU-uploaded textures.
- **Multi-texture-per-context.** Step 5.1's VA hardcoding limits
  callers to a single texture per ctx until the multi-texture VA
  allocator lands later in Phase C.

### Migration sequencing

The same 5-step pattern that landed v0:

1. ~~`src/backend.cyr`~~ extend slot constants + struct size — Step 5.4.
2. `src/backend_wgpu.cyr` — fill the 4 new slots with wgpu wrappers — Step 5.5.
3. `src/backend_native.cyr` — fill the 4 new slots with the
   Step 5.1+5.2 primitives — Step 5.6.
4. Public `gpu_texture_*` dispatchers in `src/texture.cyr` — Step 5.7.
5. End-to-end validation: `programs/native_texture_e2e.cyr`
   under native; `programs/phase0.cyr` Test 12 family under wgpu —
   Steps 5.8 + 5.9.

All 5 steps are punch-list chunks 5.4 through 5.9.

## v2 expansion — render-pipeline + render-pass + render-target slots (Step 6.7)

Steps 6.3 + 6.4 already landed the native primitives that earn this
expansion: `native_rt_create_2d_rgba8` + matching release (Step 6.3),
plus PM4 builders for `SET_CONTEXT_REG` and `DRAW_INDEX_AUTO`
(Step 6.4). v2 promotes them to first-class Backend slots so the
public API can dispatch the graphics path through the same
abstraction the compute + texture paths already use.

7 new slots, sized to fit the existing fncall1..fncall5 slot
dispatch shapes. Append-after-kind invariant preserved — every v0
+ v1 offset is unchanged.

### Layout (v2 — appends 7 render slots)

```text
Backend struct (176 bytes — 21 slots × 8 bytes; grew from v1's 120):
  +0..+112   ...same as v1 (compute + texture + kind)...
  +120: render_target_create_2d_rgba8  (ctx, w, h)              → rt_ptr   (v2)
  +128: render_target_release          (ctx, rt_ptr)            → 0        (v2)
  +136: render_pipeline_create         (ctx, vs_mod, fs_mod, color_fmt) → pipe_ptr  (v2)
  +144: render_pipeline_release        (ctx, pipe_ptr)          → 0        (v2)
  +152: render_pass_begin              (ctx, rt_ptr, clear_color_ptr) → pass_ptr (v2)
  +160: render_pass_draw               (ctx, pass_ptr, pipe_ptr, vertex_count, instance_count) → 0|err (v2)
  +168: render_pass_end                (ctx, pass_ptr)           → 0        (v2)

  `clear_color_ptr` points to a 32-byte block of 4×f64 RGBA values
  (mabda's existing f64-backed `Color` shape from `src/color.cyr`).
  WGPUColor in webgpu.h v29 is `{double r,g,b,a;}` — packing the clear
  value as a u32 0xRRGGBBAA arg was an early-draft mistake; loses 56
  bits of precision per channel and breaks sRGB/HDR/wide-gamut clears
  that already work on the wgpu path. Verified against
  `deps/wgpu-native/include/webgpu/webgpu.h:1954` and the existing
  precedent in `src/render_pass.cyr` (`color_write_f64(c, att+40)`).
```

All 7 slots stay within the fncall ceiling (max 5 args + ctx = 6
total to the slot wrapper, dispatched via `fncall5`). No
struct-by-value args. No 7+-arity slot.

Why these seven and not fewer / more:

- **RT create + release as Backend slots, not free fns.** Step 6.3's
  primitives are compiled into the binary either way; promoting them
  to slots costs nothing and gives consumers a single dispatch
  surface. No reason to leave them out and force a separate code
  path for "make me an RT vs make me a texture".
- **Pipeline create + release as a pair.** Same shape as
  `shader_module_create / release` in v0. Pipeline lifetime is
  consumer-managed; the slot is just the gate.
- **Pass begin / draw / end as a triple.** Mirrors wgpu's encoder
  flow exactly. `begin` returns an opaque encoder handle, `draw`
  records a draw, `end` finalizes + submits. Lets consumers issue
  multiple draws in one pass without slot-count creep.
- **Single `render_pass_draw` slot, no separate `set_pipeline`.**
  `draw` takes both the pass and the pipeline. Avoids encoder-state
  machine semantics in the slot interface; the pipeline is bound
  per-draw in the slot impl. Trade-off: every draw re-binds the
  pipeline on the wgpu side (free; wgpu handles it). On native, the
  PM4 builder caches the last-bound pipeline_ptr and skips the
  state writes if unchanged.

### Slot signatures, per backend

#### `render_target_create_2d_rgba8 (ctx, width, height) → rt_ptr`

Returns an opaque `rt_ptr` (u64) — same 32-byte struct shape as
texture handles for the reasons noted in v1.

- **wgpu**: pointer to a `{handle, view, sampler, w_h_packed}`
  struct, where `handle` is a `WGPUTexture` created with
  `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC` usage and
  `view` is the default 2D view. Same shape as v1 textures plus
  the render-attachment usage bit.
- **native**: pointer to the existing `NativeRenderTarget` struct
  from Step 6.3 — `{handle, va, addr, size, w_h_pitch_packed}`.
  RT VA range at `0xFFFF800101000000`+, separate from the texture
  range so future tiling/format divergence has somewhere to land.

#### `render_target_release (ctx, rt_ptr) → 0`

Frees the GPU resources + heap struct. Same shape as
`texture_release` in v1.

- **wgpu**: `wgpu_texture_view_release` + `wgpu_texture_release` +
  `free(rt_ptr)`.
- **native**: `native_rt_release_2d_rgba8(fd, rt)` (Step 6.3) +
  `free(rt_ptr)`.

#### `render_pipeline_create (ctx, vs_mod, fs_mod, color_fmt) → pipe_ptr`

Builds a graphics pipeline from a vertex + fragment shader module
pair, targeting the given color format. Pipeline state for v2 is
**fixed-function-implicit**: full-screen viewport, no depth, no
blend, no MSAA, single color attachment, triangle list topology.
Consumers needing other states use the v2.1 expansion (deferred).

- **wgpu**: `wgpu_device_create_render_pipeline` with a descriptor
  filled in from the args. The 7+-arg descriptor is already
  struct-packed via a C shim (existing pattern). Returns a u64
  pointer to a small struct holding the `WGPURenderPipeline` handle.
- **native**: builds a `NativeRenderPipeline` struct holding the
  shader VAs + **two** precomputed PM4 byte buffers:
  - `pipeline_sh_block` — `SET_SH_REG` writes for SPI_SHADER_PGM_LO/HI
    + RSRC1/2 (VS + PS), SPI_SHADER_USER_DATA layout, SPI_SHADER_POS_FORMAT,
    SPI_SHADER_Z_FORMAT, SPI_SHADER_COL_FORMAT.
  - `pipeline_ctx_block` — `SET_CONTEXT_REG` writes for the
    pipeline-static context regs only: PA_SC_LINE_CNTL, PA_SC_AA_CONFIG,
    PA_SC_MODE_CNTL, PA_SU_SC_MODE_CNTL (cull/front-face),
    PA_CL_CLIP_CNTL, PA_CL_VS_OUT_CNTL, VGT_SHADER_STAGES_EN,
    SPI_PS_INPUT_*, SPI_VS_OUT_CONFIG, CB_SHADER_MASK,
    CB_TARGET_MASK, CB_COLOR_CONTROL, CB_BLEND0_CONTROL,
    DB_SHADER_CONTROL, DB_RENDER_CONTROL.

  RT-dependent context regs (CB_COLOR0_BASE/PITCH/SLICE/INFO,
  DB_*, scissor, viewport) are **NOT** in this pipeline block —
  they get regenerated at `render_pass_begin` because they depend
  on the RT being bound, which the pipeline doesn't own. Per-draw
  fixups (USER_DATA values, VGT_NUM_INSTANCES, the
  DRAW_INDEX_AUTO packet itself) get appended at the IB tail by
  `render_pass_draw`.

  This 2-block-per-pipeline + 1-block-per-pass + 1-tail-per-draw
  shape matches Mesa radv's `ctx_cs` / `cs` / dynamic-state cut
  exactly, which is the byte-exact reference for our verification
  protocol (`feedback_pm4_verify_against_mesa_ib`). Earlier draft's
  "single PM4 block per pipeline" was wrong — there are
  fundamentally three lifetimes of PM4 state in a graphics
  dispatch.

`color_fmt` is a u32 enum: `0 = RGBA8_UNORM` for v2. Other formats
land alongside the v3.2 texture format expansion.

#### `render_pipeline_release (ctx, pipe_ptr) → 0`

Frees the pipeline + heap struct.

- **wgpu**: `wgpu_render_pipeline_release(handle)` + `free(pipe_ptr)`.
- **native**: `free(pipe_ptr)` — no GPU-side teardown needed; the
  PM4 bytes are owned by the struct and freed with it.

#### `render_pass_begin (ctx, rt_ptr, clear_color_ptr) → pass_ptr`

Starts a render pass on a single color attachment. `clear_color_ptr`
points to 32 bytes of f64 RGBA (mabda's `Color` shape). The wrapper
copies the f64 quartet into the right offset on each backend.

- **wgpu**: builds a `WGPURenderPassDescriptor` with one color
  attachment. `loadOp = WGPULoadOp_Clear`, `storeOp =
  WGPUStoreOp_Store` (both must be set explicitly — `Undefined`
  is invalid per webgpu.h v29; struct-zeroing alone is not enough).
  `clearValue` is a `WGPUColor { r, g, b, a }` 32-byte struct
  embedded in the attachment at offset 40 — `memcpy(att+40,
  clear_color_ptr, 32)`. Calls `wgpu_command_encoder_begin_render_pass`
  (already through a struct-packing C shim in
  `deps/wgpu_main.c:182`). `depthStencilAttachment = NULL`,
  `occlusionQuerySet = NULL`, `timestampWrites = NULL`. The
  encoder + pass + RT get stuffed into a `NativeRenderPass`-shaped
  opaque struct.
- **native**: allocates a per-pass IB-fragment buffer (separate
  from the per-context cached IB so multiple passes can stage
  in flight), writes the **`pass_target_block`** (RT-extent-derived
  PM4 — CB_COLOR0_BASE/PITCH/SLICE/INFO/ATTRIB, DB_* if depth,
  PA_SC_SCREEN_SCISSOR_TL/BR, PA_SC_WINDOW_SCISSOR_TL/BR,
  PA_CL_VPORT_*) followed by the clear-color write, returns a
  pointer to the pass struct. Submission happens at
  `render_pass_end`; `draw` calls append to the pass's IB-fragment
  in between. See "GFX9 PM4 block split" section below.

The clear is **always** issued at pass begin. Load-op = preserve
is a v2.1 expansion; v2 forces clear-on-begin to keep the slot
count down.

#### `render_pass_draw (ctx, pass_ptr, pipe_ptr, vertex_count, instance_count) → 0|err`

Records a draw into the pass with the given pipeline. No vertex
buffers in v2 — the shader synthesizes positions from
`vertex_index` (the gfx9_graphics_shader_abi_research entry's
"clear-shader fast path"). Instance count >1 supported.

- **wgpu**: `wgpu_render_pass_encoder_set_pipeline` +
  `wgpu_render_pass_encoder_draw(vertex_count, instance_count,
  0, 0)` (first_vertex + first_instance pinned at 0 for v2).
- **native**: appends the pipeline's `pipeline_sh_block` and
  `pipeline_ctx_block` (memcpy from the pipeline struct, no
  re-encoding), then writes any per-draw USER_DATA values +
  VGT_NUM_INSTANCES (if instance_count > 1), then a
  `DRAW_INDEX_AUTO` packet. `count_minus_1` derivation per
  `feedback_pm4_count_minus_1_naming.md`. If the pipeline_ptr
  matches the pass's last-bound pipeline_ptr, the SH and CTX
  blocks are skipped (state caching — same pattern radv uses
  to avoid context rolls between back-to-back draws of the same
  pipeline; see GPUOpen "Understanding GPU context rolls").

5 args after the slot ptr — stays at fncall5, the established
ceiling for ergonomic slot dispatch.

#### `render_pass_end (ctx, pass_ptr) → 0`

Finalizes and submits the pass.

- **wgpu**: `wgpu_render_pass_encoder_end` + `wgpu_command_encoder_finish`
  + `wgpu_queue_submit` + free the pass struct. The RT contents
  are then valid for `texture_read` (v1 slot) or further pipeline
  consumption.
- **native**: appends a fence WRITE_DATA + EOS marker to the IB-
  fragment, splices the fragment into a fresh `cs` ioctl chunk,
  submits with the per-context fence BO, waits for sync-obj
  signal, frees the pass struct. Matches the established Phase B.4
  submit flow.

### GFX9 PM4 block split (native side, mandatory reading before Step 6.8c)

The graphics dispatch state is **not** a single PM4 block per
pipeline. It has three distinct lifetimes, which the Backend
interface honors:

| Block | Lifetime | Lives in | Approx size |
|-------|----------|----------|-------------|
| `pipeline_sh_block`  | per-pipeline (built at `render_pipeline_create`) | NativeRenderPipeline struct | ~40-80 dwords |
| `pipeline_ctx_block` | per-pipeline (built at `render_pipeline_create`) | NativeRenderPipeline struct | ~80-160 dwords |
| `pass_target_block`  | per-pass (rebuilt at `render_pass_begin`) | NativeRenderPass struct | ~40-80 dwords |
| `draw_tail`          | per-draw (appended in `render_pass_draw`) | IB-fragment, inline | ~10-20 dwords |

Why each lifetime is forced (not chosen):

- **`pipeline_sh_block`** — Shader VAs (SPI_SHADER_PGM_LO/HI for VS+PS),
  RSRC1/2 control fields, USER_DATA window layout. These are 100%
  determined by the shader bytes and the pipeline's resource
  bindings. Pipeline change → re-emit. RT bind → no re-emit needed.
- **`pipeline_ctx_block`** — Context regs that are pipeline-static:
  blend, raster mode, depth-test mode, primitive topology, AA mask.
  Vulkan/Mesa calls these "pipeline state"; radv bakes them into
  `ctx_cs` at pipeline-create time.
- **`pass_target_block`** — Context regs that depend on the RT
  being bound: CB_COLOR0_* (BASE, PITCH, SLICE, INFO, ATTRIB),
  DB_* (if depth), PA_SC_SCREEN_SCISSOR, PA_SC_WINDOW_SCISSOR,
  PA_CL_VPORT_*. The pipeline doesn't own these — the RT does —
  so they cannot be cached at pipeline-create. radv emits them
  via `radv_emit_fb_color_state` at command-buffer time.
- **`draw_tail`** — VGT_NUM_INSTANCES (if instancing),
  per-draw USER_DATA values (push constants, dynamic descriptor
  pointers), DRAW_INDEX_AUTO packet itself. Per-draw by definition.

**Earlier draft was wrong.** The v2 proposal initially said the
pipeline state could be encoded in a single PM4 buffer at
`render_pipeline_create` and spliced in at draw time. That
collapses three distinct lifetimes into one and would have broken
RT-bind, viewport-change, and per-draw uniforms. Verified against
Mesa radv's `radv_pipeline_graphics.c` (pipeline `ctx_cs`),
`radv_cmd_buffer.c` (`radv_emit_fb_color_state` for RT-dependent
context regs), and AMD GPUOpen's "Understanding GPU context
rolls" article (why context-reg writes are batched separately
from SH-reg writes).

### RLC-gated / kernel-only registers — userspace must NOT write

The Phase B.4 close-out ruled out a "missing queue preamble" theory,
but the underlying caution stands: GFX9 has registers reserved for
the kernel and the RLC microcontroller. Writing them from a userspace
IB is undefined-behavior territory. The Step 6.8c PM4 builders must
filter against this denylist:

- `GRBM_GFX_INDEX`, `GRBM_GFX_CNTL` — broadcast / per-SE selectors,
  RLC-mediated.
- `RLC_*` family entirely — RLC microcontroller's domain.
- `SH_MEM_CONFIG`, `SH_MEM_BASES` — kernel sets per-VMID at submit.
- `COMPUTE_STATIC_THREAD_MGMT_SE0..3` — kernel-managed (already
  set up correctly at ctx-init per the resolved B.3 entry; do not
  re-write in userspace IB).
- `PA_SC_TILE_STEERING_OVERRIDE` — kernel-only on GFX9.
- `MC_VM_*`, `ATC_*` — kernel-only memory controller.
- `CP_HQD_*`, `CP_MQD_*` — queue-descriptor regs, kernel-set at
  queue init.

Safe rule: only write registers radv writes from its userspace IB.
Verify with `AMD_DEBUG=ib` and the byte-exact protocol
(`feedback_pm4_verify_against_mesa_ib.md`) — anything radv doesn't
emit, we don't emit either.

### Verification & citations

This section landed in v2.1 (2026-04-28) after two research agents
verified the v2 design against external sources. Findings:

**Claim 1 — wgpu function names + signatures: VERIFIED.** All 8
wrapper names exist in `deps/wgpu-native/include/webgpu/webgpu.h`
v29 and are already wired into `src/wgpu_ffi.cyr`. Two notes:
(a) mabda already exposes `wgpu_queue_submit_one` (slot 35,
`src/wgpu_ffi.cyr:124`) — single-cmd convenience wrapper around
`wgpuQueueSubmit(queue, 1, &cmd_buf)`. Use it for `render_pass_end`.
(b) `wgpuCommandEncoderFinish`'s descriptor argument is
`WGPU_NULLABLE`; pass NULL.

**Claim 2 — WGSL `@builtin(vertex_index)` with no vertex buffer:
VERIFIED.** Already shipping. `programs/phase0.cyr:311-315` and
`programs/benchmarks.cyr:367-371` use this exact pattern. The
`WGPUVertexState` struct (webgpu.h:4727) explicitly allows
`bufferCount = 0, buffers = NULL` — `WGPU_VERTEX_STATE_INIT`
default. Mabda's `_vertex_state_init` (`src/render_pipeline.cyr:80-87`)
zero-fills which yields the right shape.

**Claim 3 — clear-color format: CORRECTED.** WGPUColor is `{double
r,g,b,a;}` (32 bytes) per `webgpu.h:1954`. The earlier draft's
"packed u32 0xRRGGBBAA" arg was wrong. Slot signature is now
`(ctx, rt_ptr, clear_color_ptr) → pass_ptr` where `clear_color_ptr`
is a 32-byte f64 RGBA block (mabda's `Color` shape). Existing
precedent: `color_write_f64(c, att+40)` in `src/render_pass.cyr`.

**Claim 4 — pipeline descriptor mandatory fields: VERIFIED with
explicit defaults.** `WGPURenderPipelineDescriptor`: `layout = NULL`
is legal (auto layout from shader reflection); `depthStencil = NULL`
is legal (no depth); `fragment = NULL` is legal but useless for
color output (the v2 proposal requires `fragment` non-NULL with
`targetCount = 1`). `WGPURenderPassColorAttachment`: `loadOp` and
`storeOp` must be set explicitly to `Clear`/`Store` etc. — `Undefined`
is invalid; struct-zeroing alone is not enough. `colorAttachmentCount
≥ 1` unless using a depth-only pass.

**Claim 5 — GFX9 pipeline state PM4 serializability: REJECTED as
originally stated.** Replaced with the 3-block split documented
above. Sources: Mesa docs ("RADV — The Mesa 3D Graphics Library"),
Mesa source at `src/amd/vulkan/radv_pipeline_graphics.c` and
`radv_cmd_buffer.c` (`radv_emit_fb_color_state`), AMD GPUOpen
"Understanding GPU context rolls", and the kernel's
`drivers/gpu/drm/amd/amdgpu/gfx_v9_0.c` for the RLC save/restore
list.

### What v2 still defers

- **Multiple color attachments.** Single attachment per pass for
  v2; MRT ships with v3.1.
- **Depth / stencil.** No depth attachment, no z-test, no stencil
  ops in v2. Lands in v3.1 alongside the depth-format texture
  expansion. The 32-byte RT struct already has space reserved at
  +24 for a future depth-handle slot.
- **MSAA.** Sample count = 1, no resolve attachment. v3.x
  follow-up.
- **Vertex buffers.** Vertex pulling via `vertex_index` only —
  enough for full-screen triangles, blits, simple effects. Vertex
  buffers + index buffers in a v3.1 expansion (re-uses the
  existing v0 buffer slots; just adds `set_vertex_buffer` /
  `set_index_buffer` slots to the pass).
- **Pipeline state variants.** Blend mode, raster mode, depth
  state, viewport overrides — all locked to fixed defaults in v2.
  When consumers need them, they land as a `pipeline_create_v2`
  slot taking a packed state-descriptor byte buffer.
- **Multi-pass-per-encoder.** Each `render_pass_begin` =
  fresh native CS submit on the native side. Multiple passes in
  one submit (the wgpu encoder pattern) is a v3.1 perf
  optimization.

### Migration sequencing

Mirrors the v1 5-step pattern:

1. **Step 6.7 — this proposal revision.** Doc-only. Adds the v2
   section, defines slot offsets + signatures.
2. **Step 6.8 (split a)** — `src/backend.cyr`: 7 new slot offset
   constants, struct size 120 → 176 bytes, layout asserts.
3. **Step 6.8 (split b)** — `src/backend_wgpu.cyr`: 7 wgpu slot
   wrappers around existing wgpu render helpers. `programs/phase0.cyr`
   continues passing.
4. **Step 6.8 (split c)** — `src/backend_native.cyr`: 7 native
   slot wrappers. RT slots wrap Step 6.3's primitives directly;
   the pass slots are gated on Step 6.5's
   `native_pm4_build_render_clear_triangle` PM4 stream and Step
   6.6's `native_render_dispatch_simple` submit path landing.
   Split c does NOT cache pipeline state in a single PM4 block —
   it builds two pipeline blocks (`pipeline_sh_block` +
   `pipeline_ctx_block`) at pipeline_create time, plus a
   `pass_target_block` at pass_begin time, plus a per-draw tail.
   See "GFX9 PM4 block split" above. Honor the RLC-gated denylist.
5. **Step 6.9** — public `gpu_render_*` dispatchers in
   `src/render_pipeline.cyr`, `src/render_pass.cyr`,
   `src/render_target.cyr`. Coexist with v2.x APIs; old names
   delegate to the new dispatch path.
6. **Step 6.9 close** — `programs/native_render_e2e.cyr` mirroring
   `programs/render_e2e.cyr`. Runs end-to-end on both backends.

Pre-condition for step 4 above: 6.2 (graphics shader bytes) +
6.5 + 6.6 must land first. Slots 1, 2, 3, 4 (RT + pipeline
create/release) can land before that on the native side because
they're pure struct allocation; slots 5, 6, 7 (the pass triple)
need a working PM4 dispatch path. The wgpu side has no such
gating — all 7 land together.

## File layout

```text
src/backend.cyr                — @internal: Backend struct, BACKEND_KIND_*,
                                  null-slot helpers, generic dispatch wrappers
src/backend_wgpu.cyr           — @internal: fills Backend with wgpu impls
src/backend_native.cyr         — @internal: fills Backend with native impls
                                  (already exists; gains a builder fn)
src/context.cyr                — @public: GpuContext gains a backend ptr at +32
                                  (preserves existing +0..+24 layout)
src/buffer.cyr, compute.cyr    — @public: bodies become 1-line dispatchers
                                  through ctx->backend
```

`GpuContext` grows from 32 → 40 bytes:

```text
GpuContext struct (40 bytes):
  +0..+24: instance / adapter / device / queue   (existing — wgpu only,
                                                  native leaves zero)
  +24..+32: queue                                (existing)
  +32..+40: backend_ptr                          (new — Backend struct ptr)
```

The `+24` queue slot stays for ABI compatibility with consumers
that read it; on native, `backend_ptr->queue_id` carries the
amdgpu ctx_id.

## Selector

Per ADR 006, the selector ergonomics are open. This proposal
recommends a **compile-time constant** in `src/lib.cyr`:

```cyrius
# src/lib.cyr
var MABDA_BACKEND_KIND = BACKEND_KIND_WGPU;   # change to BACKEND_KIND_NATIVE
```

…and a `gpu_context_from_preinit` body that reads it and wires
the right `Backend` into `ctx->backend`. The reasons to prefer
constant over `cyrius.cyml` flag at this stage:

- One `cyrius distlib` regenerate gives you a bundle pinned to
  one backend — the v3.0 bench harness goal needs both bundles
  side by side, no per-build conditional logic.
- Cyrius doesn't yet have first-class build-time conditionals
  beyond constants; emulating one with a manifest flag pushes
  parsing into `src/lib.cyr` for no real win.
- `cyrius.cyml` flag remains an option for v3.1+ once consumer
  CI matrices need it; promoting from constant to flag is a
  one-commit change.

`MABDA_BACKEND_KIND` is `@internal`. Consumers never see it —
they see the same `gpu_context_from_preinit` they always have.

## Migration sequencing

Five steps, each verifiable end-to-end before the next:

1. **Land `src/backend.cyr` with the Backend struct + the 11
   v0 slots.** No callers yet. Tests assert the struct layout
   (`sizeof Backend == 88`, slot offsets at multiples of 8).
2. **Land `src/backend_wgpu.cyr`.** Fill all 11 slots with
   wrappers around the existing wgpu functions. `programs/phase0.cyr`
   continues to pass when `MABDA_BACKEND_KIND = BACKEND_KIND_WGPU`.
3. **Refactor `src/buffer.cyr`, `src/compute.cyr`, etc.** to
   dispatch through `ctx->backend`. Public API surface unchanged;
   ADR 005 `@public` audit still passes.
4. **Land `src/backend_native.cyr` builder.** Wire the existing
   `native_*` primitives into the 11 slots. Lift the working
   `native_compute_store` dispatch into `native_backend_compute_dispatch`.
   `programs/phase0.cyr` is then expected to pass when
   `MABDA_BACKEND_KIND = BACKEND_KIND_NATIVE` — that's the v3.0
   exit criterion for the abstraction layer.
5. **Dual-backend bench harness.** `make bench-gpu` recompiles
   the bundle once per backend, runs the 13-bench suite under
   each, emits CSV columns for both.

Each step is a CLAUDE.md "small bite" — assertions added, lint
clean, distlib regenerated.

## Risks and what we'll watch for

- **Slot creep before native parity.** Tempting to add
  `texture_*` slots to the v0 interface and stub them on native.
  Don't. Adds maintenance surface that doesn't earn its keep.
- **`fnptr` calling-convention bugs.** Cyrius `fnptr` works at
  5.4.x, but all fnptr-heavy code in mabda today goes through
  `wgpu_ffi.cyr`'s `fncall*` wrappers against extern C. A
  Cyrius-fn-to-Cyrius-fn `fncall` through a struct slot is a
  shape we haven't extensively exercised; a smoke test should be
  step 0 of this work.
- **GpuContext layout migration.** Existing `gpu_ctx_*` getters
  read fixed offsets. The `+32` extension is additive but every
  `alloc(32)` in the codebase has to grow to `alloc(40)`. Single
  greppable transform.
- **Native-side BO ownership.** The working `native_compute_store`
  allocates 5 BOs per dispatch and frees them at end of program.
  Real consumer flows allocate buffers once and dispatch many
  times. The `Backend.buffer_*` slot signature anticipates this
  (`buf_handle` is opaque; native impl carries the `(handle, va)`
  pair). The `compute_dispatch` slot doesn't allocate the IB BO
  for the consumer — it pulls it from a per-context IB ring buffer.
  IB-ring lifecycle is gated on the same Phase B follow-up that
  lets `native_compute_store` dispatch twice without leaking.

## Out of scope for this proposal

- Surface / present path. Phase D's `surface_configure / acquire /
  present` slots get their own revision once the native KMS path
  (`programs/native_present_e2e.cyr`) is working.
- Multi-queue. v3.1 work; the `Backend` struct layout above
  accommodates extension via a future slot at the next available
  offset (`+176` after v2) when the time comes.
- WGSL → ISA lowering. The interface takes `bytes_ptr + n` for
  shader modules; the bytes are SPIR-V on wgpu and pre-compiled
  GFX9 ISA on native. The lowering decision (ADR 006 open
  question) doesn't touch this proposal — it changes what bytes
  consumers feed, not the slot signature.

## Decision required from the user

Before step 1 lands:

- **Sign-off on the v0 11-slot list.** Is this the right anchor
  set, or do you want to defer one and pull another forward?
- **Sign-off on compile-time constant selector** (vs `cyrius.cyml`
  flag) for v3.0. The constant is reversible at zero cost; the
  question is whether it's the right shape for the bench harness.
- **Is there an active feature flag / kill switch you want?** ADR 006
  says `wgpu` stays the default; this proposal honors that. If
  you want a runtime override (e.g., environment variable) for
  CI debugging, name it now so step 1 can include it.
