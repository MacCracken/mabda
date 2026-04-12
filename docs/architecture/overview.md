# Architecture Overview

## What Mabda Is

Mabda is a GPU foundation library written in Cyrius. It owns the wgpu-native FFI boundary and provides shared GPU infrastructure so that every AGNOS consumer builds on one consistent base instead of duplicating device management, buffer creation, and pipeline setup.

## Module Map

```
┌─────────────────────────────────────────────────────────┐
│ Consumers: soorat, rasa, ranga, bijli, aethersafta      │
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ mabda                                                    │
│                                                          │
│  Core:       error, color, capabilities, context,        │
│              profiler, resource, debug                    │
│                                                          │
│  Buffers:    buffer, compute, shader_cache,              │
│              pipeline_cache, bind_group_cache             │
│                                                          │
│  Graphics:   vertex, blend, sampler, depth, texture,     │
│              bind_group, instancing                       │
│                                                          │
│  Render:     render_target, render_pipeline,              │
│              render_pass, surface                         │
│                                                          │
│  FFI:        wgpu_types, wgpu_descriptors, wgpu_ffi      │
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ wgpu-native C API (v29) via function table               │
│ C shim (wgpu_main.c) handles by-value struct callbacks   │
└─────────────────┬───────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────┐
│ Vulkan / Metal / DX12 (GPU driver)                       │
└─────────────────────────────────────────────────────────┘
```

## FFI Architecture

Cyrius cannot call C functions directly (no extern declarations). Instead:

1. **C launcher** (`wgpu_main.c`) initializes libc, loads Vulkan, creates GPU context
2. **Function table** — C populates an array of 40 wgpu function pointers
3. **Cyrius code** receives the table pointer, calls functions via `fncall0-6`
4. **Shim wrappers** handle wgpu functions that pass structs by value (callbacks)

```
C main() → _cyrius_init() → alloc_init() → GPU pre-init → mabda_main(fn_table, preinit)
                                                                  ↓
                                                     fncall2(_fp(8), device, desc)
                                                                  ↓
                                                     wgpuDeviceCreateBuffer(device, desc)
```

## Object Mode Compilation

Cyrius `.o` files are linked with gcc against wgpu-native:

```
test.tcyr → cc3 (object;) → test.o → gcc + wgpu_main.o + libwgpu_native.a → binary
```

Key requirements:
- `_cyrius_init()` must be called before any Cyrius functions (initializes enums/globals)
- `alloc_init()` must be called after `_cyrius_init()` (init resets global state)
- Symbol clashes (memcpy, memset, etc.) resolved via `objcopy -L`

## Struct-packing shim pattern (required for wgpu 6+ arg functions)

Cyrius's `fncall6` correctly passes 6 i64 arguments per the SysV-AMD64 ABI,
but calling wgpu-native v29 entry points this way segfaults reliably — the
same sequence works from pure C. Root cause is unconfirmed, but the fix is
consistent: wrap the call in a C shim that accepts `(primary_handle,
struct_ptr)`, unpack the struct in C, and call wgpu from C. Cyrius then
invokes the shim via `fncall2`.

Two shims live in `deps/wgpu_main.c`:
- `wgpu_shim_copy_buffer_to_buffer(encoder, WgpuCopyArgs*)` — for
  `wgpuCommandEncoderCopyBufferToBuffer` (6 args: encoder, src, src_off, dst,
  dst_off, size).
- `wgpu_shim_buffer_map(device, WgpuMapArgs*)` — for `wgpuBufferMapAsync` +
  `wgpuDevicePoll` (6 args: device, buffer, mode, offset, size, status_ptr).

Any future wgpu entry with 6+ i64 arguments should follow the same pattern.
Entries with 5 or fewer args use the plain `fncall0..fncall5` path directly.

## Data Flow: Compute

```
1. gpu_context_from_preinit(ptr)     → GpuContext (instance/adapter/device/queue)
2. wgpu_buffer_descriptor(...)       → C struct at heap address
3. wgpu_device_create_buffer(d, desc) → WGPUBuffer handle (opaque i64)
4. wgpu_queue_write_buffer(q, b, ...) → data uploaded to GPU
5. compute_pipeline_new(d, wgsl, ...) → ComputePipeline (pipeline/bgl/layout)
6. compute_dispatch(d, q, cp, bg, x,y,z) → GPU execution
7. read_buffer(d, q, b, size)        → heap-allocated copy of GPU data
```

## Data Flow: Render

```
1. rpb_new(device, shader, "vs_main") → RenderPipelineBuilder
2. rpb_color_target(b, format, blend)  → configure output
3. rpb_depth(b, DEPTH32_FLOAT)         → configure depth
4. rpb_build(b)                        → RenderPipeline
5. rpb_pass_new()                      → RenderPassBuilder
6. rpb_pass_color(b, view, color)      → attach color target
7. rpb_pass_depth(b, depth_view)       → attach depth
```

## Consumer Matrix

| Consumer | context | buffer | compute | texture | render | profiler |
|----------|---------|--------|---------|---------|--------|----------|
| soorat   | x       | x      |         | x       | x      | x        |
| rasa     | x       | x      | x       | x       |        |          |
| ranga    | x       | x      | x       |         |        |          |
| bijli    | x       | x      | x       |         |        | x        |
| aethersafta | x    | x      |         | x       | x      |          |
