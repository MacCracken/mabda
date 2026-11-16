# A render tier for consumers that draw geometry: what soorat's Cyrius port needs from the public `gpu_*` API

**Status:** ✅ **CLOSED — shipped in mabda 4.3.0 (MB1) and 4.3.1 (MB2–MB5), 2026-11-16; every milestone HW-verified.** MB1: the pipeline descriptor, bind groups / sampler objects, the draw-time calls, public constant names, the error channel. MB2–MB5: depth targets and passes (shadow maps, comparison sampling, read-only depth); RGBA8 sRGB / BGRA8 / RGBA16F textures and targets, 4x MSAA with resolve, several colour targets, views (cube faces and mip levels as attachments), mip generation on wgpu, texture copies, render-target readback; compute pipelines and passes on wgpu plus `gpu_compute_dispatch`; a `WGPUSurface` from a Wayland surface with capabilities, BGRA8 formats, AUTO_VSYNC / AUTO_NO_VSYNC, frames as texture objects and resize. The launcher went to 83 slots in 4.3.1 (the compute pass encoder's release and the compute pipeline's group layout were missing from 4.3.0's revision). Measured with soorat's probes against the 4.3.1 bundle: P6/MB1 (11 pipelines, the error cases, the sprite frame), P6/MB2-5 (`soorat/scripts/probes/gpu/src/p6_mb25.cyr`: shadow map + pbr group 2, bloom's HDR threshold, an IBL cube with generated mips + pbr group 3, an sRGB copy, a 4x MSAA sprite, gpu_particles), P8 (100,000 frames, RSS flat, 0 B/frame); MB5 by `programs/surface_wayland_e2e.cyr`, 12 / 12 in a Hyprland session on the Cezanne (7 formats, 3 present modes; AUTO_NO_VSYNC → Immediate, AUTO_VSYNC → Fifo; log `build/logs/surface_wayland_e2e-20261116-092215.log`). Left for later (soorat ledger L7, roadmap): the native render tier beyond the vertex-less RGBA8 subset.
**Filed:** 2026-11-15, from soorat's Cyrius port:
- the plan: `soorat/docs/development/soorat-port-plan.md` §2.1, §8 (MB1–MB5) and ledger rows L2–L6;
- the measurement: probe P6 (`soorat/scripts/probes/gpu/src/p6_wgsl.cyr`; logs in
  `soorat/scripts/probes/gpu/build/logs/p6-*.log`; write-up `soorat/docs/development/port-parity.md` §1.5).

**Placement:**
- `src/render_pipeline.cyr`, `src/render_pass.cyr`, `src/bind_group.cyr`, `src/sampler.cyr`,
  `src/texture_format.cyr` and `src/surface_v3.cyr`;
- the wgpu backend slots in `src/backend_wgpu.cyr`;
- `deps/wgpu_main.c`, the function table and its shims.

**Severity:** **High for soorat.** soorat reaches wgpu **only through mabda**, with no direct wgpu-native FFI
(the maintainer's rule, 2026-11-15), so it has no local stand-in. Its GPU half cannot start until this lands:
about 2,700 of its 7,600 code lines (sprites, lines, meshes, shadows, IBL, PBR and skinning, HDR, bloom and
SSAO, particles, screenshots, the window surface). Its CPU half is porting meanwhile.
**Affects:** mabda 4.2.0, public tier, wgpu backend first. Native-backend parity is mabda's call per item;
soorat's 2.0 targets the wgpu backend (its plan, D3).

## Summary

soorat 1.2.0's nine WGSL shaders, through mabda 4.2.0 on the wgpu backend (AMD Cezanne, RADV, wgpu-native
v29.0.1.1):

| Step | Result |
|---|---|
| `gpu_shader_module_create` | **9 / 9 succeed** |
| `gpu_render_pipeline_create(ctx, vs, fs, fmt)` | builds only **ssao** and **postprocess**, the two full-screen shaders without vertex buffers. sprite, line, pbr, pbr_skinned and shadow: `Location[0] … is not provided by the previous stage outputs`. bloom: `Unable to find entry point 'fs_main'`. gpu_particles: `Unable to find entry point 'vs_main'` |
| `rpb_new` + `rpb_vertex_layout(vertex2d_attributes())` + `rpb_build` | the same `Location[0]` failure. `rpb_build` delegates to `render_pipeline_create_simple`, and its own comment says the builder fields beyond the minimal path "will still compile but only the minimal-path fields take effect" |
| Drawing with a built pipeline | no way to bind a uniform buffer: `gpu_render_pass_bind_texture` makes a 2-entry group, and postprocess's layout has 3 |
| `gpu_compute_dispatch` | returns `GPU_ERR_OTHER` (99) on wgpu, unconditionally |
| `compute_pipeline_new(device, src, "cs_update", 2)` | fails: it builds **storage-only** layouts, and soorat's particle shader binds a storage buffer plus a uniform |
| Any validation error | aborts the process (wgpu-native's default uncaptured-error handler, exit 134). The launcher installs no error callback, and the public tier has no error scope |

## What soorat's shaders bind

Each shader's Rust pipeline in soorat 1.2.0 (`src/pipeline.rs`, `mesh_pipeline.rs`, `shadow.rs`, `hdr.rs`,
`postprocess.rs`, `ssao.rs`, `debug_draw.rs`, `gpu_particles.rs`) is the wgpu-29 descriptor the Cyrius port
must express.

| Shader | Entry points | Vertex buffer | Bindings |
|---|---|---|---|
| `sprite.wgsl` | `vs_main`, `fs_main` | `Vertex2D` (32 B), u16 or u32 indices | g0: uniform (mat4); g1: texture_2d + sampler. Alpha blending |
| `line.wgsl` | `vs_main`, `fs_main` | `LineVertex` (28 B: f32x3 + f32x4) | g0: uniform. **LineList**, depth test LessEqual |
| `pbr.wgsl` | `vs_main`, `fs_main` | `Vertex3D` (48 B), u32 indices | g0: 4 uniforms (192, 544, 32 and 80 B). g1: texture_2d + sampler. g2: **texture_depth_2d + sampler_comparison**. g3: **2 × texture_cube** + 2 samplers, plus texture_2d + sampler (IBL). Depth32Float, `Less`, back-face culling |
| `pbr_skinned.wgsl` | `vs_main`, `fs_main` | `SkinnedVertex3D` (96 B, joints **Uint32x4**) | as pbr, plus g1 has two textures, and g3: **read-only storage** (8,208 B joint palette) |
| `shadow.wgsl` | `vs_main` **only** (depth-only pass) | `Vertex3D` | g0: uniform (128 B). Depth32Float, LessEqual, back-face culling |
| `ssao.wgsl` | `vs_main`, `fs_main` | none (full-screen triangle) | g0: texture_depth_2d + sampler, texture_2d + sampler, uniform (144 B) |
| `bloom.wgsl` | `vs_main`, **`fs_threshold`, `fs_blur_h`, `fs_blur_v`** | none | g0: texture_2d + sampler + uniform (32 B). **Rgba16Float** targets |
| `postprocess.wgsl` | `vs_main`, `fs_main`, **`fs_no_bloom`** | none | g0: texture_2d + sampler + uniform; g1 (bloom): texture_2d + sampler |
| `gpu_particles.wgsl` | **`cs_update`** (compute) | — | g0: **read-write storage** (array of 48 B particles) + uniform (16 B) |

## Request

The order follows soorat's milestones:
- MB1 + MB2 give the first pixels (soorat M6);
- MB3 gives IBL and post-processing (M7, M8);
- MB4 gives particles;
- MB5 gives windowed applications (M9).

**MB1: pipeline descriptor and draw-time binding**
- **Pipeline descriptor:**
  - vertex buffer layouts: stride, step mode and attributes, including `Uint32x4`;
  - **explicit vertex and fragment entry-point names**;
  - an **optional fragment stage**, for a depth-only pass;
  - topology (TriangleList, LineList), cull mode and front face;
  - per-target blend state (alpha blending at least);
  - **explicit bind-group layouts**: uniform; storage read and read-write; texture with a float, unfilterable
    or depth sample type and a 2D or cube view dimension; sampler, filtering or comparison;
  - several bind groups per pipeline.
- **Draw time:**
  - set vertex buffer(s) and an index buffer (u16 and u32);
  - `draw_indexed`;
  - set a bind group, with dynamic offsets: soorat moves to per-draw model uniforms;
  - viewport (float arguments, so a C shim) and scissor;
  - creating bind groups from uniform/storage buffers, texture views and samplers on the public tier.
- **An error channel:** an error scope (push/pop) or an uncaptured-error callback set at device creation, so
  that an invalid shader or pipeline returns a `GPU_ERR_*` instead of aborting the host application.
- **Public names** for the constants public calls already take. Today buffer usage is `WGPU_BUFFER_USAGE_*`,
  `gpu_sampler_create` takes `NATIVE_SAMP_*`, and a pipeline's format is a raw `WGPUTextureFormat`; all of
  these are defined only in `@internal` files.

**MB2: depth and shadows**
- Depth32Float render targets.
- Depth state: compare function (`Less`, `LessEqual`), write enable, depth bias.
- Binding a depth texture with a depth sample type.
- **Comparison samplers** (`LessEqual`) on the public tier.
- A render pass's depth load/clear choice, which soorat's API also gains.

**MB3: formats, mips, copies and readback**
- `Rgba16Float` render targets and textures.
- `Rgba8UnormSrgb` kept as sRGB. Today the sRGB variants collapse to the UNORM id: a documented, detectable
  limitation (mabda has no `_SRGB` ids yet; tracked as 3-3 punchlist AL.0). soorat's sprite and environment
  textures sample as sRGB.
- MSAA with resolve, and several colour targets.
- Rendering into a cube face or mip level, i.e. a view of one layer and level as an attachment.
- Mip generation on wgpu: `gpu_texture_generate_mipmaps` returns NOT_IMPLEMENTED there today.
- Texture-to-texture copy (soorat uses it 3 times).
- Render-target readback on the public tier.
- **Related:** `gpu_buffer_read` on wgpu ends in the stdlib's byte-loop `memcpy`, 2.7 ms per 128 KB in P7.
  This is the same root cause as `archived/2026-11-02-native-buffer-copies-are-byte-loops.md` (fixed in 4.3.0 with `gpu_copy_bytes`), here on the wgpu
  backend; soorat's screenshots and tests read back through it.

**MB4: compute on the public tier for wgpu**
- `gpu_compute_dispatch` implemented on wgpu.
- Compute pipelines with mixed layouts: soorat's particles bind read-write storage plus a uniform.
- Storage-buffer readback.

**MB5: a window surface**
- Create a `WGPUSurface` from a Wayland `wl_display` and `wl_surface` (a chained
  `WGPUSurfaceSourceWaylandSurface` descriptor, so a C shim).
- Surface capabilities.
- BGRA8 formats, Unorm and Srgb; today the surface format is forced to RGBA8_UNORM.
- Present modes AutoVsync and AutoNoVsync.

soorat 2.0 owns no window (its plan, D5): the application passes a context and a surface. A shared Wayland
client library is a separate, future project.

**Launcher.** If the scope is known up front, land every new slot in **one** `deps/wgpu_main.c` revision,
together with the command-buffer release from
`archived/2026-11-15-wgpu-render-loop-leaks-command-buffers-and-pass-records.md` (done in 4.3.0), so every consumer rebuilds once. New
slots, roughly:

| Area | Slots |
|---|---|
| Vertex and index | SetVertexBuffer, SetIndexBuffer, DrawIndexed |
| Viewport | SetViewport (shim), SetScissorRect |
| Bind groups | SetBindGroup with dynamic offsets, if the existing slot doesn't take them |
| Copies | CopyTextureToTexture |
| Errors | PushErrorScope / PopErrorScope, or an error callback on the device descriptor |
| Surface | InstanceCreateSurface (shim), SurfaceGetCapabilities |
| Release | CommandBufferRelease, or the release inside the submit shim |

## Out of scope here

- Running soorat on the **native** backends needs a render-capable shader path: a WGSL front end, or SPIR-V
  render lowering. That is soorat's ledger L7, after its 2.0. It bears on mabda's v5.1 retirement gate, which
  lists soorat among the consumers that must run native.

## Measure / acceptance

From `soorat/scripts/probes/gpu/`, `sh build.sh && sh run_p6.sh`. Extended for each MB, it should:
- build pipelines for all nine shaders with soorat's real layouts;
- render the soorat reference scenes (sprite grid, lines, a lit, a shadowed and a skinned mesh, IBL, the post
  chain, particles) through the public tier alone;
- read them back.

soorat compares those readbacks with its Rust 1.2.0 renders on the same GPU (its parity class G).
