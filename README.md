# Mabda

**Mabda** (Arabic: مبدأ — origin, principle, starting point) is the GPU foundation layer for the [AGNOS](https://github.com/MacCracken) ecosystem. It owns the `wgpu` dependency and provides shared GPU infrastructure that all AGNOS GPU consumers build upon.

## Features

- **Device lifecycle** — adapter selection, device/queue creation, headless + surface-aware init
- **Compute pipelines** — shader compilation, bind group layout, 1D/2D dispatch helpers
- **Buffer management** — storage, uniform, staging buffers with typed readback
- **Texture loading** — PNG/JPEG, solid color, RGBA, caching with bind group management
- **Render targets** — offscreen framebuffers with CPU readback (row alignment handled)
- **GPU profiling** — CPU frame timing (EMA), GPU timestamp queries
- **Capability detection** — adapter limits, feature queries, WebGPU compatibility constants

## Cargo Features

| Feature | Description |
|---------|-------------|
| `graphics` | Render targets, texture loading, surface helpers |
| `compute` | Compute pipeline, storage buffers, dispatch utilities |
| `full` | Enables both `graphics` and `compute` |

## Consumers

| Crate | Role |
|-------|------|
| **soorat** | Rendering engine (sprites, PBR, shadows, post-fx) |
| **rasa** | Image editor (GPU compute filters) |
| **ranga** | Image processing library (GPU pixel ops) |
| **bijli** | Electromagnetic simulation (FDTD compute) |
| **aethersafta** | Desktop compositor (GPU compositing) |
| **kiran** | Game engine (via soorat) |

## Quick Start

```toml
[dependencies]
mabda = { version = "0.1", features = ["full"] }
```

```rust
use mabda::{GpuContext, GpuCapabilities};

// Create a headless GPU context (compute-only)
let ctx = pollster::block_on(GpuContext::new()).expect("GPU init failed");

// Query capabilities
let caps = GpuCapabilities::from_context(&ctx);
println!("{}", caps.report());
```

## Dependencies

- **wgpu** — GPU abstraction (Vulkan, Metal, DX12, GL, WebGPU)
- **bytemuck** — safe byte casting for GPU buffers
- **pollster** — minimal async runtime for GPU init
- **serde** — serialization for capabilities and color types
- **thiserror** — error type derivation
- **tracing** — structured logging
- **image** *(optional)* — PNG/JPEG texture loading

## License

GPL-3.0
