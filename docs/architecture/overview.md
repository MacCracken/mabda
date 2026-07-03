# Architecture Overview

## What Mabda Is

Mabda is a GPU foundation library written in Cyrius. As of v4.0 it owns
**three GPU backends behind one public API**: the wgpu-native FFI boundary
(the cross-vendor default), a pure-Cyrius DRM/KMS **native AMD** backend
(direct `amdgpu` ioctls, GFX9 PM4, and — as of the v3.2.x arc — an in-tree
SPIR-V→GFX9 compute compiler), and a pure-Cyrius **native NVIDIA** backend
(nouveau DRM, clc597 push, SM75 SASS — added at v4.0). As of **v4.0.1** the
AMD route through wgpu is **deprecated** — an AMD adapter still works but gets a
one-shot deprecation warning (`gpu_context_from_preinit` vendorID guard), and
`-D MABDA_AMD_WGPU_STRICT` enforces native-only; the wgpu binding stays for
NVIDIA + Intel and full retirement is deferred. It provides shared GPU infrastructure so
every AGNOS consumer builds on one consistent base instead of duplicating
device management, buffer creation, and pipeline setup; the backend choice
is invisible to consumer code (the `@public` API is byte-identical across
all three, routed through an `@internal` `Backend` slot table).

## Module Map

```
┌─────────────────────────────────────────────────────────┐
│ Consumers: soorat, rasa, ranga, bijli, aethersafta, kiran│
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ mabda (30 @public + 19 @internal; 49 domain modules)     │
│                                                          │
│  Core:      error, color, capabilities, context,         │
│             profiler, resource, debug                     │
│  Buffers:   buffer, typed_buffer, compute, gpu_timestamps,│
│             shader_cache, pipeline_cache, bind_group_cache│
│  Graphics:  vertex, blend, sampler, depth, texture,       │
│             texture_format, bind_group, instancing        │
│  Render:    render_target, render_pipeline, render_pass,  │
│             render_graph, surface, surface_v3, queue       │
│                                                          │
│  @internal Backend slot table → routes every @public call │
│  to ONE backend below (native = AMD or NVIDIA; same shape):│
└──────────┬──────────────────────────────────┬───────────┘
           │ wgpu fillers                      │ native AMD/NVIDIA fillers
┌──────────▼─────────────────────┐ ┌──────────▼───────────────────────┐
│ wgpu-native C API (v29) via a   │ │ direct amdgpu DRM ioctls (no      │
│ 65-slot fn table; C launcher    │ │ libdrm): GFX9 PM4 + GEM/syncobj/CS │
│ (deps/wgpu_main.c) — 7 struct-  │ │ + KMS; SPIR-V→GFX9 compute compiler│
│ packing shims                   │ │ (gfx9_encode/spirv_parse/mir/      │
│                                 │ │ spirv_lower/gfx9_isel/regalloc/    │
│                                 │ │ waitcnt/abi/compile)               │
└──────────┬─────────────────────┘ └──────────┬───────────────────────┘
           │                                   │
┌──────────▼─────────────────────┐ ┌──────────▼───────────────────────┐
│ Vulkan / Metal / DX12 driver    │ │ Linux amdgpu kernel driver (AMD)  │
└─────────────────────────────────┘ └───────────────────────────────────┘
```

> The diagram shows the **wgpu-vs-native** split. The native column now has
> **two** backends: **AMD** (amdgpu DRM, shown) and **NVIDIA** (nouveau DRM /
> clc597 push / SM75 SASS — added at v4.0), for three backends total. As of
> **v4.0.1** an AMD adapter on the wgpu column is **deprecated** — it still works
> with a one-shot warning (`-D MABDA_AMD_WGPU_STRICT` →
> `GPU_ERR_AMD_WGPU_DEPRECATED` to enforce); wgpu still serves NVIDIA + Intel.

## Flat Layout

```
src/   (49 modules — see CLAUDE.md "Architecture" for the annotated tree)
├── lib.cyr              — single include chain (stdlib + domain modules)
├── error.cyr, color.cyr, capabilities.cyr, profiler.cyr, resource.cyr, debug.cyr
├── context.cyr          — GpuContext (wgpu instance/adapter/device/queue OR
│                          native fd/ctx_id/BO handles, dual-interpreted)
├── buffer.cyr, typed_buffer.cyr, gpu_timestamps.cyr, compute.cyr
├── shader_cache.cyr, pipeline_cache.cyr, bind_group_cache.cyr
├── vertex.cyr, blend.cyr, sampler.cyr, depth.cyr, bind_group.cyr
├── texture.cyr, texture_format.cyr, render_target.cyr, render_pipeline.cyr
├── render_pass.cyr, render_graph.cyr, queue.cyr, surface.cyr, surface_v3.cyr, instancing.cyr
├── @internal wgpu FFI:  wgpu_types.cyr, wgpu_descriptors.cyr, wgpu_ffi.cyr
├── @internal backend:   backend.cyr, backend_wgpu.cyr, backend_native.cyr,
│                        backend_native_amdgpu.cyr, backend_native_pm4.cyr,
│                        backend_native_shaders.cyr, backend_native_kms.cyr
└── @internal SPIR-V→GFX9 compiler (v3.2.x): gfx9_encode.cyr, spirv_parse.cyr,
                         mir.cyr, spirv_lower.cyr, gfx9_isel.cyr,
                         gfx9_regalloc.cyr, gfx9_waitcnt.cyr, gfx9_abi.cyr, gfx9_compile.cyr
```

Stdlib includes live **only** in `src/lib.cyr`. Domain modules are
flat (zero transitive includes), which is what makes `cyrius distlib`
concatenate them into a compile-clean `dist/mabda.cyr`.

## FFI Architecture

Cyrius cannot call C functions directly (no extern declarations).
Instead:

1. **C launcher** (`deps/wgpu_main.c`) initializes libc, selects the
   Vulkan backend via `WGPUInstanceExtras { backends = Vulkan }`
   (the default `All` crashes on headless boxes — see v2.4.2), and
   creates the GPU context pre-Cyrius.
2. **Function table** — C populates an array of 65 wgpu function
   pointers covering buffer / shader / pipeline / texture / render
   pass / render pipeline / surface / timestamp query / copy
   operations.
3. **Cyrius code** receives the table pointer and calls functions
   via `fncall1` .. `fncall6` for scalar-arg entry points (never
   `fncall6` with a struct-by-value / float arg — see next section).
4. **Struct-packing shims** handle wgpu calls where the C callee
   takes a struct-by-value / float / variadic argument (NOT by arg
   count). Cyrius allocates a packed-args struct and calls the shim
   via `fncall2(shim_fp, subject_handle, args_ptr)`; the shim unpacks
   in C.
5. **Callback wrappers** handle wgpu functions that pass callback
   descriptors by value (request-adapter / request-device /
   buffer-map-async).

```
C main() → _cyrius_init() → alloc_init() → preinit_gpu(Vulkan) → mabda_main(fn_table, preinit)
                                                                       │
                                                                       ▼
                                                         fncall2(_fp(8), device, desc)
                                                                       │
                                                                       ▼
                                                      wgpuDeviceCreateBuffer(device, desc)
```

## Object Mode Compilation

Cyrius `.o` files are linked with gcc against wgpu-native. The four
GPU integration programs (`phase0`, `compute_e2e`, `render_e2e`,
`render_graph_e2e`) and the benchmark program (`benchmarks`) use
this path; the rest of the library runs through `cyrius test` /
`cyrius bench` / `cyrius build` directly.

```
programs/render_graph_e2e.cyr → printf 'object;\n' | cc5 → build/render_graph_e2e.o
build/render_graph_e2e.o + deps/wgpu_main.o + libwgpu_native.a → gcc → build/render_graph_e2e
```

Key requirements (all validated in v2.4.x runtime sweep):

- `_cyrius_init()` must be called before any Cyrius functions
  (initializes enums / globals).
- `alloc_init()` must be called after `_cyrius_init()` (init resets
  global state).
- Symbol clashes (`memcpy`, `memset`, `strlen`, `strstr`, …) resolved
  via `objcopy -L <sym>` (Makefile's `LOCALIZE_SYMS`). `strstr` was
  added to the list in v2.4.2 after it was found interposing libc's
  strstr and crashing Mesa's driver-string probing.

For non-GPU modules, `cyrius build programs/smoke.cyr` or
`make test` (globs `tests/tcyr/*.tcyr`, 16 domain suites) drives
everything; no object mode, no C linker.

## Struct-Packing Shim Pattern

The historical framing of this was "fncall6 + wgpu crashes"; cyrius
6.3.26 settled it. Two things were conflated:

- **Real, permanent**: wgpu entry points that accept a **struct-by-value**
  (most descriptor-taking functions), a `float`/`double`, or are variadic
  require the callee to read fields from places Cyrius's register-only
  `fncallN` doesn't set up (SysV §3.2.3 / AAPCS64 §B.4 aggregate rules;
  xmm/v regs for floats). Those need a C shim.
- **Misdiagnosis, now retired**: "6+ i64 args" / "7+ params" were never
  the problem. cyrius `fncall0..fncall8` are correct for all-scalar args;
  the apparent crashes were a TLS/`%fs` init fault (a stack-protected C
  callee reads its canary from `%fs:0x28`) the glibc C launcher already
  prevents. The all-scalar wrappers `copy_buffer_to_buffer` /
  `queue_write_texture` / `resolve_query_set` / `buffer_map_sync` were
  un-shimmed at v4.0.2 and now call the raw wgpu functions directly via
  `fncall6`.

The shim accepts `(subject_handle, struct_ptr)` and unpacks in C; Cyrius
invokes it via `fncall2`. The remaining **struct-packing** shims in
`deps/wgpu_main.c`:

- `wgpu_shim_command_encoder_begin_render_pass(encoder, WgpuBeginPassArgs*)`
  — v2.4.3; packs the render-pass descriptor + color-attachment array.
- `wgpu_shim_command_encoder_copy_texture_to_buffer(encoder, WgpuCopyTexToBufArgs*)`
  — v2.4.3; packs src `WGPUTexelCopyTextureInfo` + dst
  `WGPUTexelCopyBufferInfo` + `WGPUExtent3D`.

(The launcher also carries non-struct-packing shims: callback bridges for
request-adapter / request-device / buffer-map-async, label-descriptor
builders for create/finish command encoder, single-buffer queue-submit,
the timestamp-period reinterpret, and the raw-SPIR-V passthrough. And
`wgpu_shim_buffer_map` is now a natural 6-arg async→sync bridge, not a
struct-packer.)

Any future wgpu entry that takes a struct-by-value (most descriptor
parameters), a float, or is variadic should follow the same pattern —
the ABI handshake is what matters, not the arg count.

## Data Flow: Compute

```
1. gpu_context_from_preinit(ptr)            → GpuContext (instance/adapter/device/queue)
2. wgpu_buffer_descriptor(...)              → C struct on heap
3. wgpu_device_create_buffer(d, desc)       → WGPUBuffer handle (opaque i64)
4. wgpu_queue_write_buffer(q, b, 0, data, size)  → data uploaded to GPU
5. compute_pipeline_new(d, wgsl, "main", n) → ComputePipeline (pipeline/bgl/layout)
6. compute_dispatch(d, q, cp, bg, dims_xyz) → GPU execution (dims is 12-byte u32 triple)
7. read_buffer(d, q, b, size)               → heap-allocated copy of GPU data
```

## Data Flow: Render

```
1. render_pipeline_create_simple(device, module, color_format)  → RenderPipeline
   (full-screen-triangle path; no vertex buffers, no depth, no MSAA, no blend)

Render pass construction:
2. rpb_pass_new()                           → RenderPassBuilder
3. rpb_pass_color(b, view, clear_color)     → attach color target (72-byte entry)
4. rpb_pass_depth(b, depth_view)            → attach depth (optional)

Render pass execution:
5. enc = wgpu_device_create_command_encoder(device, label)
6. pass = rpb_pass_begin(enc, b)            → opens the pass via the shim
7. wgpu_render_pass_encoder_set_pipeline(pass, rp)
8. wgpu_render_pass_encoder_draw(pass, verts, 1, 0, 0)
9. wgpu_render_pass_encoder_end(pass)
10. wgpu_render_pass_encoder_release(pass)
11. cmd = wgpu_command_encoder_finish(enc, label)
12. wgpu_queue_submit_one(queue, cmd)
```

The v29 render-pipeline descriptor layout lives in the header
comment of `src/render_pipeline.cyr` (168 bytes). The render-pass
color-attachment layout is 72 bytes (v29 — adds `nextInChain` at
offset 0; v2.4.3 fixed a pre-v29 layout bug).

## Data Flow: Render Graph (v2.5.0)

For frames with multiple passes (compute + render + copy), the
render graph collapses everything into a single command encoder:

```
1. g = rg_new()
2. rg_add_compute(g, cp, bg, dims_xyz, label)            → node_id
3. rg_add_render(g, pass_builder, pipeline, verts, label) → node_id
4. rg_add_copy_tex_buf(g, packed_args_72B)               → node_id
5. rg_add_transient_buffer(g, size, usage, label)        → res_id
6. rg_node_writes(g, node_id, res_id) / rg_node_reads(...)
7. rg_build(g, device)        → toposort + allocate transient handles
8. rg_execute(g, device, queue) → ONE encoder, ONE submit
9. rg_release(g)              → release transient handles
```

See [`../guides/render-graph.md`](../guides/render-graph.md) for the
full authoring guide.

## Consumer Matrix

| Consumer     | context | buffer | compute | texture | render | render_graph | surface | profiler |
|--------------|---------|--------|---------|---------|--------|--------------|---------|----------|
| soorat       | x       | x      |         | x       | x      | (opt-in v2.5)| x       | x        |
| rasa         | x       | x      | x       | x       |        | (opt-in v2.5)|         |          |
| ranga        | x       | x      | x       |         |        |              |         |          |
| bijli        | x       | x      | x       |         |        |              |         | x        |
| aethersafta  | x       | x      |         | x       | x      |              | x       |          |
| kiran        | via soorat                                                                                  |
