# Consumer Integration Guide

## How to Depend on Mabda

### Cargo.toml

```toml
# Renderer (soorat, aethersafta)
[dependencies]
mabda = { version = "0.1", features = ["graphics"] }

# Compute-only (bijli, rasa)
[dependencies]
mabda = { version = "0.1", features = ["compute"] }

# Both (ranga, kiran)
[dependencies]
mabda = { version = "0.1", features = ["full"] }
```

### Feature Selection

Only pull what you need. `compute` and `graphics` are independent — enabling one does not pull in the other's types or compilation cost.

## Shared GpuContext

Create one `GpuContext` at application startup and pass `&GpuContext` to all subsystems. Do not create multiple contexts.

```rust
// Application entry
let ctx = pollster::block_on(GpuContext::new())?;

// Pass to subsystems
let renderer = MyRenderer::new(&ctx);
let compute = MyCompute::new(&ctx);
```

For windowed applications, create the context compatible with the surface:

```rust
let ctx = pollster::block_on(GpuContext::new_for_surface(&surface))?;
```

## Renderer Consumer (soorat pattern)

### What mabda provides

| Need | Mabda type |
|------|-----------|
| GPU device/queue | `GpuContext` |
| Vertex data | `Vertex2D`, `Vertex3D`, `SkinnedVertex3D` |
| Render pipeline | `RenderPipelineBuilder` → `RenderPipeline` |
| Render pass setup | `RenderPassBuilder` |
| Depth buffer | `DepthTexture` |
| Surface management | `SurfaceState` |
| Texture loading | `Texture`, `TextureCache` |
| Instanced rendering | `InstanceData`, `InstanceBuffer` |
| Blend modes | `BlendPreset`, `blend_state()` |
| Bind group setup | `BindGroupLayoutBuilder` |
| Sampler presets | `SamplerPreset`, `create_sampler()` |
| Profiling | `FrameProfiler`, `ProfileScope` |
| Debug markers | `DebugScope` |
| MSAA targets | `RenderTargetBuilder` |
| Shader caching | `ShaderCache` |
| Pipeline caching | `PipelineCache` |

### What stays in the renderer

- Shader source code (WGSL files)
- Material system (PBR uniforms, material parameters)
- Scene graph / object management
- Camera, lights, shadow map logic
- Post-processing effects (bloom, tone mapping, SSAO)
- Render graph / pass orchestration
- Application-specific draw logic

## Compute Consumer (bijli/rasa pattern)

### What mabda provides

| Need | Mabda type |
|------|-----------|
| GPU device/queue | `GpuContext` |
| Compute pipeline | `ComputePipeline` |
| Typed buffers | `UniformBuffer<T>`, `StorageBuffer<T>` |
| Raw buffers | `create_storage_buffer()`, `create_uniform_buffer()` |
| Iterative compute | `PingPongBuffer` |
| Async readback | `read_buffer_async()` → `PendingReadback` |
| Dispatch validation | `validate_dispatch()` |
| Workgroup math | `workgroups_1d()`, `workgroups_2d()` |
| Profiling | `FrameProfiler`, `GpuTimestamps` |

### What stays in the consumer

- Shader source code
- Simulation logic (FDTD grid, filter kernels)
- Data layout / parameter structures
- Result interpretation

## Error Handling

All mabda functions that can fail return `mabda::Result<T>`. Consumers should propagate or handle errors:

```rust
use mabda::{GpuError, Result};

fn my_render_frame(ctx: &GpuContext) -> Result<()> {
    let frame = surface_state.acquire(&surface)?;
    // If acquire returns SurfaceOutdated, caller can resize and retry
    // ...
    Ok(())
}
```

Use `is_recoverable()` to distinguish transient errors from fatal ones.

## Migration from Raw wgpu

If your crate currently uses wgpu directly:

1. Replace `wgpu::Device` / `wgpu::Queue` with `&GpuContext`
2. Replace manual buffer creation with `create_*_buffer()` or `UniformBuffer<T>`
3. Replace manual pipeline creation with `RenderPipelineBuilder` / `ComputePipeline`
4. Replace manual render pass setup with `RenderPassBuilder`
5. Replace manual vertex layouts with `Vertex2D::layout()` etc.
6. Add `mabda` to Cargo.toml, remove direct `wgpu` dependency

Mabda's `GpuContext` exposes raw wgpu types as public fields (`ctx.device`, `ctx.queue`) for cases where you need direct wgpu access.
