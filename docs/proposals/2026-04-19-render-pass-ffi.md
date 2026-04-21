# 2026-04-19 — Render-pass FFI expansion (v2.4.2 dependency)

**Owner:** lang agent (upstream; mabda consumes)
**Consumer:** mabda v2.4.2
**Status:** open — proposal awaiting lang-agent implementation
**Blocks:** v2.4.2 closeout (last item on v1.0 parity checklist),
v2.5.0 render graph

---

## Why

mabda v2.4.0 closed every v1.0-parity criterion the existing FFI
surface could reach. The last unchecked item — **render pipeline
end-to-end draw + readback** — needs FFI slots that don't exist in
the current `deps/wgpu_main.c` fn-table:

```
$ grep -c '\[5[0-9]\]' src/wgpu_ffi.cyr
8       # last slot is 57 = wgpuSurfaceRelease
```

The render-pipeline path can today only **create + release** a
pipeline (slots 52-53). Encoding a render pass, dispatching draw
calls, and copying a texture back to a CPU-readable buffer all
require FFI entries that haven't been wired.

Without this, the v1.0 checklist cannot close, and the v2.5 render
graph (next release after v2.4.2) is blocked because its execution
loop dispatches render passes as a primitive.

## What — concrete deliverables

### 1. C-side fn-table additions (`deps/wgpu_main.c`)

Append the following to `build_fn_table()` and to the function-table
header comment block in `src/wgpu_ffi.cyr`:

| Slot (proposed) | wgpu-native symbol                              | Direct or shim |
|-----------------|--------------------------------------------------|----------------|
| 58              | `wgpu_shim_command_encoder_begin_render_pass`    | **Shim** — `WGPURenderPassDescriptor` packs nested `colorAttachments` array; direct call hits the `fncall6 + wgpu` ABI bug per `feedback_fncall6_wgpu` |
| 59              | `wgpuRenderPassEncoderSetPipeline`               | Direct (2 args) |
| 60              | `wgpuRenderPassEncoderSetBindGroup`              | Direct (5 args — fits under the 6-arg ceiling) |
| 61              | `wgpuRenderPassEncoderDraw`                      | Direct (5 args) |
| 62              | `wgpuRenderPassEncoderEnd`                       | Direct (1 arg) |
| 63              | `wgpuRenderPassEncoderRelease`                   | Direct (1 arg) |
| 64              | `wgpu_shim_command_encoder_copy_texture_to_buffer` | **Shim** — both `WGPUTexelCopyTextureInfo` (src) and `WGPUTexelCopyBufferInfo` (dst) are nested structs |

Total: **7 new slots**, **2 struct-packing C shims**, **5 direct
function-table entries**.

### 2. Struct-packing C shims (in `deps/wgpu_main.c`)

#### 2a. `wgpu_shim_command_encoder_begin_render_pass`

```c
typedef struct {
    void* color_attachments;      // pointer to packed array
    long  color_attachment_count; // i64
    void* depth_stencil;          // pointer or NULL
    void* timestamp_writes;       // pointer or NULL
    const char* label;            // cstr or NULL
} WgpuBeginPassArgs;

void* wgpu_shim_command_encoder_begin_render_pass(
    WGPUCommandEncoder enc, const WgpuBeginPassArgs* args
);
```

Internally builds a real `WGPURenderPassDescriptor` from the packed
args, calls `wgpuCommandEncoderBeginRenderPass(enc, &desc)`, and
returns the resulting `WGPURenderPassEncoder*` handle. Cyrius calls
this via `fncall2(_fp(58), enc, args_ptr)`.

#### 2b. `wgpu_shim_command_encoder_copy_texture_to_buffer`

```c
typedef struct {
    void* src_texture;            // WGPUTexture
    long  src_mip_level;          // u32 widened
    long  src_origin_x, src_origin_y, src_origin_z;
    void* dst_buffer;             // WGPUBuffer
    long  dst_offset;             // u64
    long  dst_bytes_per_row;      // u32 widened
    long  dst_rows_per_image;     // u32 widened
    long  copy_w, copy_h, copy_depth;
} WgpuCopyTexToBufArgs;

void wgpu_shim_command_encoder_copy_texture_to_buffer(
    WGPUCommandEncoder enc, const WgpuCopyTexToBufArgs* args
);
```

Builds `WGPUTexelCopyTextureInfo` (src), `WGPUTexelCopyBufferInfo`
(dst), and `WGPUExtent3D` (copy size), then dispatches. Cyrius calls
via `fncall2(_fp(64), enc, args_ptr)`.

### 3. Cyrius FFI wrappers (`src/wgpu_ffi.cyr`)

```cyr
fn wgpu_command_encoder_begin_render_pass(enc, args) {
    return fncall2(_fp(58), enc, args);
}
fn wgpu_render_pass_encoder_set_pipeline(pass, pip) {
    fncall2(_fp(59), pass, pip); return 0;
}
fn wgpu_render_pass_encoder_set_bind_group(pass, group_idx, bg, dyn_count, dyn_offsets) {
    fncall5(_fp(60), pass, group_idx, bg, dyn_count, dyn_offsets); return 0;
}
fn wgpu_render_pass_encoder_draw(pass, vert_count, inst_count, first_vert, first_inst) {
    fncall5(_fp(61), pass, vert_count, inst_count, first_vert, first_inst); return 0;
}
fn wgpu_render_pass_encoder_end(pass)     { fncall1(_fp(62), pass); return 0; }
fn wgpu_render_pass_encoder_release(pass) { fncall1(_fp(63), pass); return 0; }
fn wgpu_command_encoder_copy_texture_to_buffer(enc, args) {
    return fncall2(_fp(64), enc, args);
}
```

All under the 6-arg fncall ceiling — none risk the `fncall6 + wgpu`
crash documented in `feedback_fncall6_wgpu`.

### 4. WGPURenderPassDescriptor field offsets (verify against
   `webgpu.h` v29)

The shim builds `WGPURenderPassDescriptor` internally so the field
offsets live in C. Mabda's job is the **packed args struct** above
(the existing pattern already used for `WgpuMapArgs`,
`WgpuCopyArgs`, etc.). Keep the args layout flat — no nested
structs in the Cyrius-side packed buffer.

Color-attachment array entries match the existing
`COLOR_ATTACHMENT_SIZE = 64` layout in `src/render_pass.cyr`
(verified at lines 14-22 of that file). The shim should consume
that same packed format directly without re-packing.

## Acceptance criteria (mabda-side, for v2.4.2 to close)

After the lang-agent ships, mabda will:

1. Update `src/wgpu_ffi.cyr` header comment to document slots 58-64.
2. Add the 7 wrapper fns above.
3. Add `rpb_pass_begin(encoder, builder)` to `src/render_pass.cyr`
   that allocates a `WgpuBeginPassArgs`, fills it from the builder
   (label + `rpb_pass_color_array` + `rpb_pass_color_count` + depth),
   calls `wgpu_command_encoder_begin_render_pass`, returns the pass
   handle.
4. Add `texture_create_render_target_rgba8(device, w, h, label)`
   helper to `src/texture.cyr` — usage =
   `RENDER_ATTACHMENT | COPY_SRC | COPY_DST`.
5. Author `programs/render_e2e.cyr`:
   - Create RGBA8 render target (256 × 256 to keep readback cheap)
   - Open render pass clearing to a known colour (e.g. cornflower
     0.392 / 0.584 / 0.929)
   - End pass + finish encoder + submit
   - Encode `copy_texture_to_buffer` into a MAP_READ buffer
   - Submit, map, verify pixel(0,0) bytes match the clear colour
     (within 1 LSB tolerance for sRGB conversion if the format is
     `RGBA8_UNORM_SRGB`)
6. Add `make test-render-e2e` (already wired — Makefile pattern rule
   from v2.4.0 covers it once the program exists).
7. Add ~10 CPU assertions in `tests/tcyr/mabda.tcyr`:
   - FFI fn-table slots 58-64 are non-zero after init
   - `WgpuBeginPassArgs` size and field-offset round-trips
   - `WgpuCopyTexToBufArgs` size and field-offset round-trips
   - `rpb_pass_begin(0, builder)` short-circuits on null encoder
     (defensive — matches existing zero-handle conventions)
8. Tick "Render pipeline end-to-end draw + readback" in
   `docs/development/roadmap.md` v1.0 criteria.
9. Standard closeout: VERSION 2.4.1 → 2.4.2, CHANGELOG entry,
   `cyrius distlib`, `make fmt-check`, `make test`,
   `scripts/version-check.sh`.

## Coordination protocol

- Lang agent commits to upstream cyrius (or to wherever the C-shim
  template lives) with a tag.
- Mabda pulls the new shim into `deps/wgpu_main.c` (or accepts the
  shim file via dep), bumps cyrius pin if a toolchain bump is
  needed, and runs the v2.4.2 plan above.
- If the shim signatures change during lang-agent implementation,
  this proposal is the source of truth for what mabda is expecting
  — update it in place (same file, new "Revision" section at top)
  rather than starting a new proposal.

## Out of scope for this proposal (defer to render graph v2.5)

- `wgpuRenderPassEncoderSetVertexBuffer` / `SetIndexBuffer` /
  `DrawIndexed` / `DrawIndirect` — not needed for the clear-only E2E
  test. Add when first consumer needs them.
- `wgpuRenderPassEncoderSetViewport` / `SetScissor` — same.
- Multi-attachment render passes — single colour attachment is
  enough for the v1.0 checklist closure.

## Risk register

| Risk | Mitigation |
|------|------------|
| `WGPURenderPassDescriptor` field offset drift between wgpu-native versions | Shim lives in C, pinned to the same wgpu-native v29 mabda already uses. Layout shifts surface as compile errors, not runtime corruption. |
| Cyrius `fncall6 + wgpu-native` crash regressing | All wrappers stay at fncall1/2/5; shims absorb the rest. `feedback_fncall6_wgpu` and `feedback_cyrius_param_ceiling` memories enforce this. |
| Render target format mismatch (`RGBA8_UNORM` vs `_SRGB`) trips pixel comparison | Use `RGBA8_UNORM` in the E2E program (no sRGB curve — exact pixel match). Document in the program header why. |

## References

- `src/wgpu_ffi.cyr` — current FFI fn-table (slots 0-57)
- `src/render_pass.cyr` — descriptor builder, awaiting dispatcher
- `src/render_pipeline.cyr` — pipeline creation already lands in
  v2.0+; this proposal completes the *execution* half
- `feedback_fncall6_wgpu` memory — struct-packing pattern rationale
- `feedback_cyrius_param_ceiling` memory — 6-arg ceiling rationale
- `docs/issues/2026-04-19-phase0-build-broken.md` — adjacent
  GPU-integration plumbing context
