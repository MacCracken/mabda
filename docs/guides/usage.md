# Usage Guide

> Written against mabda 2.3.0 / Cyrius 5.4.7. See
> [`docs/stdlib-integration.md`](../stdlib-integration.md) for
> consumer-project setup (manifest, deps, launcher build rule).

## Getting Started

Pull mabda in as a dep in your `cyrius.cyml`:

```cyml
[deps.mabda]
git = "https://github.com/MacCracken/mabda.git"
tag = "2.3.0"
modules = ["dist/mabda.cyr"]
```

Then include it in your Cyrius source:

```cyrius
include "lib/mabda.cyr"
```

For GPU access, the C launcher compiles your source in object mode
(prepend `object;` — the Makefile handles this), initialises the GPU,
builds a function table, and calls `mabda_main(fn_table, preinit)`:

```cyrius
object;
include "lib/mabda.cyr"

fn mabda_main(fn_table_ptr, preinit_ptr) {
    # _cyrius_init() + alloc_init() already ran inside the launcher
    color_init();
    wgpu_ffi_init_table(fn_table_ptr);
    # ... your GPU code ...
    return 0;
}
```

## GPU Context

The C launcher pre-initializes the GPU and passes handles to Cyrius:

```cyrius
var res = gpu_context_from_preinit(preinit_ptr);
if (is_err_result(res) == 1) {
    # No GPU available
    return 1;
}
var ctx = payload(res);
var device = gpu_ctx_device(ctx);
var queue = gpu_ctx_queue(ctx);
```

## Buffers

### Storage Buffer (read-write GPU memory)

```cyrius
var data[64];
store64(&data, 42);
store64(&data + 8, 99);

var buf = create_storage_buffer(device, queue, &data, 64, "my-storage");
```

### Uniform Buffer (read-only, 16-byte aligned)

```cyrius
var uniforms[16];
store32(&uniforms, f64_to_f32(F64_1));  # time = 1.0
var ubuf = create_uniform_buffer(device, queue, &uniforms, 16, "my-uniform");
```

### Buffer Readback

```cyrius
var result = read_buffer(device, queue, gpu_buf, size);
if (result != 0) {
    var value = load64(result);
}
```

### GrowableBuffer

```cyrius
var usage = WGPU_BUFFER_USAGE_STORAGE | WGPU_BUFFER_USAGE_COPY_DST;
var gb = growable_buffer_new(device, 1024, usage);
var grew = growable_buffer_update(gb, device, queue, data, new_size);
if (grew == 1) {
    # Buffer was reallocated — rebind bind groups
}
```

## Compute Pipelines

```cyrius
var wgsl = "@group(0) @binding(0) var<storage, read_write> data: array<f32>;\n@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) id: vec3<u32>) {\n  data[id.x] = data[id.x] * 2.0;\n}";

var cp = compute_pipeline_new(device, wgsl, "main", 1);
# Create bind group, then dispatch:
compute_dispatch(device, queue, cp, bind_group, workgroups_1d(count, 64), 1, 1);
```

## Vertex Types

```cyrius
# Vertex2D: position(2) + tex_coords(2) + color(4) = 32 bytes
var v = vertex2d_new(F64_0, F64_0, F64_0, F64_0, F64_1, F64_1, F64_1, F64_1);

# Write directly to buffer memory
var buf[96];  # 3 vertices
vertex2d_write(&buf, px, py, tx, ty, cr, cg, cb, ca);
vertex2d_write(&buf + 32, ...);
vertex2d_write(&buf + 64, ...);

# Get attribute layout for pipeline
var attrs = vertex2d_attributes();
```

## Colors

```cyrius
color_init();  # Must call once before using color functions

var red = COLOR_RED();
var custom = color_new(F64_HALF, F64_0, F64_1, F64_1);
var blended = color_lerp(COLOR_BLACK(), COLOR_WHITE(), F64_HALF);
var lum = color_luminance(custom);

# Write as f32 for GPU buffers (16 bytes)
var cbuf[16];
color_write_f32(red, &cbuf);
```

## Blend Modes

```cyrius
var bs = blend_state_new(BLEND_ALPHA_BLEND);
# Also: BLEND_OPAQUE, BLEND_PREMULTIPLIED_ALPHA, BLEND_ADDITIVE, BLEND_MULTIPLY
```

## Profiling

```cyrius
var prof = profiler_new();

profiler_begin_frame(prof);
# ... render/compute work ...
var frame_ms = profiler_end_frame(prof);

# Explicit scope timing (replaces RAII ProfileScope)
var start = profile_begin();
# ... work ...
var duration_ms = profile_end(start);

# Query stats
var avg = profiler_avg_frame_ms(prof);
var fps = profiler_fps(prof);
var worst = profiler_worst_frame_ms(prof);
```

## Error Handling

Mabda uses tagged unions (Ok/Err) from tagged.cyr:

```cyrius
var res = gpu_context_from_preinit(ptr);
if (is_err_result(res) == 1) {
    var err = payload(res);
    var code = gpu_err_code(err);
    var name = gpu_err_name(code);
    # Handle error...
    return 1;
}
var ctx = payload(res);  # success value
```

Recoverable errors (retry or reconfigure):

```cyrius
if (gpu_err_is_recoverable(code) == 1) {
    # Surface timeout/outdated — retry
}
```

## Workgroup Math

```cyrius
# 1D: ceil(total / workgroup_size); returns 0 on workgroup_size == 0
# (zero-guard added in 2.3.0 to avoid SIGFPE — audit MED-1)
var groups = workgroups_1d(element_count, 64);

# 2D: writes (ceil(w/wg_x), ceil(h/wg_y)) into dst; leaves dst zeroed
# when either wg_x or wg_y is 0.
var wg[16];
workgroups_2d(&wg, width, height, 8, 8);
var gx = load64(&wg);
var gy = load64(&wg + 8);

# Validate against device limits (optional — not called internally;
# consumers are expected to check before driving large dispatches).
var err = validate_dispatch(gx, gy, 1, max_per_dim);
```
