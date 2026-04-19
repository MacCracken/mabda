# Architecture Overview

## What Mabda Is

Mabda is a GPU foundation library written in Cyrius. It owns the
wgpu-native FFI boundary and provides shared GPU infrastructure so
that every AGNOS consumer builds on one consistent base instead of
duplicating device management, buffer creation, and pipeline setup.

## Module Map

```
┌─────────────────────────────────────────────────────────┐
│ Consumers: soorat, rasa, ranga, bijli, aethersafta, kiran│
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ mabda                                                    │
│                                                          │
│  Core:      error, color, capabilities, context,         │
│             profiler, resource, debug                     │
│                                                          │
│  Buffers:   buffer, typed_buffer, compute, gpu_timestamps │
│             cache_key, shader_cache, pipeline_cache,      │
│             bind_group_cache                              │
│                                                          │
│  Graphics:  vertex, blend, sampler, depth, texture,       │
│             bind_group, instancing                        │
│                                                          │
│  Render:    render_target, render_pipeline,               │
│             render_pass, surface                          │
│                                                          │
│  FFI (@internal):                                         │
│             wgpu_types, wgpu_descriptors, wgpu_ffi        │
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ wgpu-native C API (v29) via function table               │
│ C launcher (deps/wgpu_main.c) handles struct-packing     │
│ shims for the 6+-arg calls that crash fncall6            │
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ Vulkan / Metal / DX12 (GPU driver)                       │
└─────────────────────────────────────────────────────────┘
```

## Flat Layout

```
src/
├── lib.cyr              — single include chain (stdlib + domain)
├── error.cyr            — GpuErr codes + Result helpers
├── color.cyr, capabilities.cyr, profiler.cyr, resource.cyr
├── wgpu_types.cyr, wgpu_descriptors.cyr, wgpu_ffi.cyr   (@internal)
├── context.cyr          — GpuContext (instance/adapter/device/queue)
├── buffer.cyr, typed_buffer.cyr, gpu_timestamps.cyr
├── compute.cyr          — compute pipeline + PingPongBuffer
├── cache_key.cyr, shader_cache.cyr, pipeline_cache.cyr,
├── bind_group_cache.cyr
├── vertex.cyr, blend.cyr, sampler.cyr, depth.cyr
├── bind_group.cyr, texture.cyr, render_target.cyr
├── render_pipeline.cyr, render_pass.cyr, surface.cyr
├── instancing.cyr, debug.cyr
```

Stdlib includes live **only** in `src/lib.cyr`. Domain modules are
flat (zero transitive includes), which is what makes `cyrius distlib`
concatenate them into a compile-clean `dist/mabda.cyr`.

## FFI Architecture

Cyrius cannot call C functions directly (no extern declarations).
Instead:

1. **C launcher** (`deps/wgpu_main.c`) initializes libc, loads
   Vulkan, creates the GPU context
2. **Function table** — C populates an array of 58 wgpu function
   pointers
3. **Cyrius code** receives the table pointer, calls functions via
   `fncall2` / `fncall3` / ... / `fncall5`
4. **Struct-packing shims** handle wgpu calls with 6+ i64 arguments
   (Cyrius's `fncall6` reliably crashes against wgpu-native)
5. **Callback wrappers** handle wgpu functions that pass structs by
   value (request-adapter / request-device / buffer-map-async
   callbacks)

```
C main() → _cyrius_init() → alloc_init() → GPU pre-init → mabda_main(fn_table, preinit)
                                                                  │
                                                                  ▼
                                                    fncall2(_fp(8), device, desc)
                                                                  │
                                                                  ▼
                                                 wgpuDeviceCreateBuffer(device, desc)
```

## Object Mode Compilation

Cyrius `.o` files are linked with gcc against wgpu-native. The GPU
integration test `programs/phase0.cyr` is the only file that uses
this path today; the rest of the library runs through
`cyrius test` / `cyrius bench` / `cyrius build` directly.

```
programs/phase0.cyr → printf 'object;\n' | cc5 → build/phase0.o
build/phase0.o + deps/wgpu_main.o + libwgpu_native.a → gcc → build/phase0
```

Key requirements:
- `_cyrius_init()` must be called before any Cyrius functions
  (initializes enums / globals)
- `alloc_init()` must be called after `_cyrius_init()` (init resets
  global state)
- Symbol clashes (`memcpy`, `memset`, `strlen`, …) resolved via
  `objcopy -L <sym>` (Makefile's `LOCALIZE_SYMS`)

For non-GPU modules, `cyrius build programs/smoke.cyr` or
`cyrius test tests/tcyr/mabda.tcyr` drives everything; no object
mode, no C linker.

## Struct-Packing Shim Pattern (Required for wgpu 6+ Arg Functions)

Cyrius's `fncall6` correctly passes 6 i64 arguments per the
SysV-AMD64 ABI, but calling wgpu-native v29 entry points this way
segfaults reliably — the same sequence works from pure C. Root
cause is unconfirmed, but the fix is consistent: wrap the call in a
C shim that accepts `(primary_handle, struct_ptr)`, unpack the struct
in C, and call wgpu from C. Cyrius then invokes the shim via
`fncall2`.

Shims that live in `deps/wgpu_main.c`:

- `wgpu_shim_copy_buffer_to_buffer(encoder, WgpuCopyArgs*)` —
  `wgpuCommandEncoderCopyBufferToBuffer` (6 args: encoder, src,
  src_off, dst, dst_off, size)
- `wgpu_shim_buffer_map(device, WgpuMapArgs*)` —
  `wgpuBufferMapAsync` + `wgpuDevicePoll` (6 args: device, buffer,
  mode, offset, size, status_ptr)
- `wgpu_shim_queue_write_texture(queue, WgpuWriteTextureArgs*)` —
  `wgpuQueueWriteTexture` (6 args: queue, destination, data,
  data_size, layout, size)
- `wgpu_shim_resolve_query_set(encoder, WgpuResolveArgs*)` —
  `wgpuCommandEncoderResolveQuerySet` (6 args)
- Label-taking wrappers for the encoder / command-buffer descriptors
  that v29 is sensitive about
- `wgpu_shim_queue_submit_one(queue, cmd)` — convenience for
  single-command submit

Any future wgpu entry with 6+ i64 arguments must follow the same
pattern. Entries with 5 or fewer args use the plain `fncall0..fncall5`
path directly.

Related: the **6-parameter ceiling for Cyrius functions that fncall
into wgpu** — pure Cyrius functions can take 12+ args without issue,
but the moment one internally `fncall*`s into wgpu-native, any
signature with 7+ params reliably segfaults. Fold into a struct
pointer or split into helpers. See `feedback_cyrius_param_ceiling.md`.

## Data Flow: Compute

```
1. gpu_context_from_preinit(ptr)         → GpuContext (instance/adapter/device/queue)
2. wgpu_buffer_descriptor(...)           → C struct on heap
3. wgpu_device_create_buffer(d, desc)    → WGPUBuffer handle (opaque i64)
4. wgpu_queue_write_buffer(q, b, ...)    → data uploaded to GPU
5. compute_pipeline_new(d, wgsl, ...)    → ComputePipeline (pipeline/bgl/layout)
6. compute_dispatch(d, q, cp, bg, x,y,z) → GPU execution
7. read_buffer(d, q, b, size)            → heap-allocated copy of GPU data
```

## Data Flow: Render

```
1. render_pipeline_create_simple(device, module, color_format)  → RenderPipeline
   (full-screen-triangle path; no vertex buffers, no depth, no MSAA, no blend)

Or, for anything beyond the simple case, the legacy rpb_* builder
(kept for back-compat) delegates to the same simple path:
1. rpb_new(device, shader_source, "vs_main")   → RenderPipelineBuilder
2. rpb_fragment_entry(b, "fs_main")            → configure fragment
3. rpb_color_target(b, format, blend_ptr)      → configure output
4. rpb_label(b, "my-pipeline")                 → optional label (2.3.0+ safe)
5. rpb_build(b)                                → RenderPipeline

Render pass construction:
6. rpb_pass_new()                              → RenderPassBuilder
7. rpb_pass_color(b, view, clear_color)        → attach color target
8. rpb_pass_depth(b, depth_view)               → attach depth (optional)
```

The v29 render-pipeline descriptor layout lives in the header comment
of `src/render_pipeline.cyr` — 168 bytes, verified against `webgpu.h`.

## Consumer Matrix

| Consumer     | context | buffer | compute | texture | render | surface | profiler |
|--------------|---------|--------|---------|---------|--------|---------|----------|
| soorat       | x       | x      |         | x       | x      | x       | x        |
| rasa         | x       | x      | x       | x       |        |         |          |
| ranga        | x       | x      | x       |         |        |         |          |
| bijli        | x       | x      | x       |         |        |         | x        |
| aethersafta  | x       | x      |         | x       | x      | x       |          |
| kiran        | via soorat                                                             |
