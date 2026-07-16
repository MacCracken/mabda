# Consumer Integration Guide

> Written against mabda 4.0.7 / Cyrius 6.4.64. Full launcher-wiring
> walk-through in [`docs/stdlib-integration.md`](../stdlib-integration.md).
>
> mabda ships **three backends** behind one public API: wgpu-native
> (cross-vendor default), native AMD (amdgpu DRM/GFX9), and native
> NVIDIA (nouveau DRM/SM75). The backend is picked at compile time via
> `MABDA_BACKEND_KIND`. The C-launcher wiring in **this** guide is the
> **wgpu path**; the native backends need no C launcher (see the note
> under "GPU access").

## How to Depend on Mabda

Declare mabda in your `cyrius.cyml`:

```cyml
[deps.mabda]
git = "https://github.com/MacCracken/mabda.git"
tag = "4.0.7"
modules = ["dist/mabda.cyr"]
```

`chitra` (PNG decode, opt-in under `-D MABDA_PNG`) and `samvada`
(logind master delegation, opt-in under `-D MABDA_LOGIND`) are
**not** unconditional deps — add them only when you enable the
corresponding flag. [`docs/stdlib-integration.md`](../stdlib-integration.md)
is the authoritative manifest for the full `cyrius.cyml` block
(stdlib modules, pinned tags, and per-flag opt-ins).

Then include the library entry point in your Cyrius source:

```cyrius
include "lib/mabda.cyr"
```

### GPU access — the wgpu path (object mode + C linking)

The C-launcher wiring below is specific to the **wgpu-native**
backend. Your project needs:

1. A C launcher (`wgpu_main.c`) that pre-initializes the GPU —
   reference implementation in `deps/wgpu_main.c` of the mabda repo
2. wgpu-native library (`libwgpu_native.a`, v29) — downloaded via
   `sh deps/fetch-wgpu.sh`
3. Compilation: `cc5` (object mode, prepend `object;`) + `gcc`
   (linking)

The two **native** backends (AMD amdgpu/GFX9, NVIDIA nouveau/SM75)
do **not** use the C launcher or wgpu-native at all — they drive DRM
ioctls directly from pure Cyrius. Select them at compile time with
`MABDA_BACKEND_KIND` and enter through
`gpu_context_new_native()` / `gpu_context_new_native_nvidia()`
instead of the wgpu `gpu_context_from_preinit(preinit_ptr)` entry.
The public API surface (buffers, compute, textures, render, present)
is identical across all three. **AMD-on-wgpu is deprecated as of
4.0.1** (warn-and-allow by default; `-D MABDA_AMD_WGPU_STRICT`
hard-rejects it) — new AMD consumers should take the native path.

## Shared GpuContext Pattern

Create the context once and pass it to all subsystems:

```cyrius
fn init_renderer(ctx) {
    var device = gpu_ctx_device(ctx);
    var queue = gpu_ctx_queue(ctx);
    # Create pipelines, buffers, textures using device/queue
    return renderer;
}

fn init_compute(ctx) {
    var device = gpu_ctx_device(ctx);
    # Create compute pipelines
    return compute_engine;
}
```

## Compute Consumer Pattern (bijli, ranga)

```cyrius
fn run_simulation(ctx, input_data, size) {
    var device = gpu_ctx_device(ctx);
    var queue = gpu_ctx_queue(ctx);

    # Upload input
    var gpu_buf = create_storage_buffer(device, queue, input_data, size, "sim-input");

    # Create compute pipeline
    var cp = compute_pipeline_new(device, wgsl_source, "main", 1);

    # Dispatch — compute_dispatch takes a pointer to 12 bytes holding
    # three packed u32 workgroup counts (x@+0, y@+4, z@+8). The pointer
    # form keeps the fn at 5 params; a 7-param fn that fncalls into
    # wgpu segfaults (feedback_cyrius_param_ceiling).
    var dims[12];
    store32(&dims, workgroups_1d(size / 4, 64));  # x
    store32(&dims + 4, 1);                          # y
    store32(&dims + 8, 1);                          # z
    compute_dispatch(device, queue, cp, bind_group, &dims);

    # Readback
    var result = read_buffer(device, queue, gpu_buf, size);
    return result;
}
```

## Render Consumer Pattern (soorat, aethersafta)

```cyrius
fn render_frame(ctx, scene) {
    var device = gpu_ctx_device(ctx);
    var queue = gpu_ctx_queue(ctx);

    # Begin profiling
    profiler_begin_frame(prof);

    # Create vertex data
    var verts[96];  # 3 vertices
    vertex2d_write(&verts, ...);

    # Build render pass
    var pass = rpb_pass_new();
    rpb_pass_color(pass, target_view, COLOR_CORNFLOWER_BLUE());

    # Submit + present
    profiler_end_frame(prof);
}
```

## Error Handling

All GPU operations return tagged Results. Check before using:

```cyrius
var res = gpu_context_from_preinit(ptr);
if (is_err_result(res) == 1) {
    var code = gpu_err_code(payload(res));
    if (gpu_err_is_recoverable(code) == 1) {
        # Retry (surface timeout, outdated)
    } else {
        # Fatal (no adapter, device lost)
    }
}
```

## Migration from Raw wgpu-native

If you're calling wgpu-native directly, mabda provides:

| Direct wgpu-native | Mabda equivalent |
|---------------------|------------------|
| `wgpuDeviceCreateBuffer(d, &desc)` | `create_storage_buffer(d, q, data, size, label)` |
| `wgpuBufferMapAsync + poll + read` | `read_buffer(d, q, buf, size)` |
| Manual bind group layout entries | `bglb_new() + bglb_storage_buffer(...)` |
| Manual compute pipeline setup | `compute_pipeline_new(d, wgsl, entry, n)` |
| Manual render pass descriptors | `rpb_pass_new() + rpb_pass_color(...)` |
