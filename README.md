# Mabda

**Mabda** (Arabic: مبدأ — origin, principle, starting point) is the GPU foundation layer for the [AGNOS](https://github.com/MacCracken) ecosystem. It wraps the wgpu-native C API and provides shared GPU infrastructure that all AGNOS GPU consumers build upon.

Written in [Cyrius](https://github.com/MacCracken/cyrius), the AGNOS systems language.

## Features

- **Device lifecycle** — GpuContext creation with adapter/device/queue management
- **Buffer management** — storage, uniform, vertex, index, staging, indirect buffers; synchronous readback; GrowableBuffer with generation tracking
- **Compute pipelines** — shader compilation, bind group layout, dispatch, PingPongBuffer
- **Render pipelines** — builder pattern for vertex/fragment shaders, blend, depth, topology
- **Textures** — RGBA creation, TextureCache, mip levels, dimension validation
- **Render targets** — offscreen framebuffers with optional MSAA and depth
- **Profiling** — FrameProfiler with EMA smoothing, frame history, explicit scope timing
- **Caching** — ShaderCache, PipelineCache, BindGroupCache for GPU resource deduplication
- **Capabilities** — GPU feature/limit detection, WebGPU compatibility constants

## Quick Start

```cyrius
include "lib/mabda.cyr"

fn main() {
    alloc_init();
    color_init();

    # Create GPU context (via C launcher pre-init)
    var res = gpu_context_from_preinit(preinit_ptr);
    var ctx = payload(res);
    var device = gpu_ctx_device(ctx);
    var queue = gpu_ctx_queue(ctx);

    # Create a storage buffer
    var usage = WGPU_BUFFER_USAGE_STORAGE | WGPU_BUFFER_USAGE_COPY_DST;
    var desc = wgpu_buffer_descriptor("my-buf", usage, 1024, 0);
    var buf = wgpu_device_create_buffer(device, desc);

    # Write data
    var data[64];
    store64(&data, 42);
    wgpu_queue_write_buffer(queue, buf, 0, &data, 64);

    wgpu_buffer_release(buf);
    gpu_context_release(ctx);
    return 0;
}
```

## Modules

| Layer | Modules |
|-------|---------|
| **Core** | error, color, capabilities, context, profiler, resource, debug |
| **Buffers** | buffer, compute (workgroup math, dispatch, PingPongBuffer) |
| **Graphics** | vertex, blend, sampler, depth, texture, bind_group, instancing |
| **Render** | render_target, render_pipeline, render_pass, surface |
| **Caching** | shader_cache, pipeline_cache, bind_group_cache |
| **FFI** | wgpu_types, wgpu_descriptors, wgpu_ffi |

## Consumers

| Project | Use Case |
|---------|----------|
| **soorat** | Rendering engine (sprites, PBR, shadows, post-effects) |
| **rasa** | Image editor (GPU compute filters) |
| **ranga** | Image processing (GPU pixel ops) |
| **bijli** | EM simulation (FDTD compute) |
| **aethersafta** | Desktop compositor (GPU compositing) |
| **kiran** | Game engine (via soorat) |

## Architecture

Mabda owns the wgpu-native FFI boundary. Consumers depend on mabda, not on wgpu directly.

```
Consumer (soorat, bijli, ...)
    ↓
  mabda (GPU abstraction)
    ↓
  wgpu-native C API (via function table + C shim)
    ↓
  Vulkan / Metal / DX12
```

## Build

Requires [Cyrius](https://github.com/MacCracken/cyrius) 3.4.14+ and gcc.

```sh
# Fetch wgpu-native (one-time)
cd cyr/deps && sh fetch-wgpu.sh && cd ../..

# Run standalone tests (no GPU needed)
cd cyr
cyrius test tests/test_color.tcyr
cyrius test tests/test_profiler.tcyr
cyrius test tests/test_vertex.tcyr

# Run GPU tests (requires Vulkan)
make test-phase0
```

## Project Structure

```
mabda/
├── cyr/                  # Cyrius port (active)
│   ├── src/              # 25 source modules (3,274 lines)
│   ├── tests/            # Test suites (.tcyr)
│   ├── deps/             # wgpu-native binaries + C shim
│   ├── cyrius.toml       # Build config
│   └── Makefile          # Hybrid C/Cyrius build
├── rust-old/             # Original Rust implementation (reference)
├── docs/                 # Architecture, guides, ADRs
├── VERSION               # 2.0.0
└── CHANGELOG.md
```

## License

GPL-3.0-only
