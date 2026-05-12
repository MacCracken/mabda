# Consumer Integration Guide

> Written against mabda 3.0.0-rc.2 / Cyrius 5.11.28. Full launcher-wiring
> walk-through in [`docs/stdlib-integration.md`](../stdlib-integration.md).

## How to Depend on Mabda

Declare mabda in your `cyrius.cyml`:

```cyml
[deps.mabda]
git = "https://github.com/MacCracken/mabda.git"
tag = "3.0.0-rc.1"
modules = ["dist/mabda.cyr"]
```

Then include the library entry point in your Cyrius source:

```cyrius
include "lib/mabda.cyr"
```

For GPU access (object mode + C linking), your project needs:

1. A C launcher (`wgpu_main.c`) that pre-initializes the GPU —
   reference implementation in `deps/wgpu_main.c` of the mabda repo
2. wgpu-native library (`libwgpu_native.a`, v29) — downloaded via
   `sh deps/fetch-wgpu.sh`
3. Compilation: `cc5` (object mode, prepend `object;`) + `gcc`
   (linking)

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

    # Dispatch
    var groups = workgroups_1d(size / 4, 64);
    compute_dispatch(device, queue, cp, bind_group, groups, 1, 1);

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
