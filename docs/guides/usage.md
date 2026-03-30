# Usage Guide

## Getting Started

```toml
[dependencies]
mabda = { version = "0.1", features = ["full"] }
```

## GPU Context

```rust
use mabda::{GpuContext, GpuContextBuilder};

// Simple headless context
let ctx = pollster::block_on(GpuContext::new())?;

// Custom context with features
let ctx = pollster::block_on(
    GpuContextBuilder::new()
        .power_preference(wgpu::PowerPreference::LowPower)
        .features(wgpu::Features::TIMESTAMP_QUERY)
        .device_lost_callback(|reason, msg| {
            eprintln!("GPU device lost: {reason:?} — {msg}");
        })
        .build()
)?;
```

## Buffers

### Raw buffers

```rust
use mabda::buffer::*;

let storage = create_storage_buffer(&ctx.device, &data_bytes, "my_buf", false);
let uniform = create_uniform_buffer(&ctx.device, &uniform_bytes, "camera");
```

### Typed buffers (recommended)

```rust
use mabda::{UniformBuffer, StorageBuffer};

// Enforces 16-byte alignment for uniforms
let camera_buf = UniformBuffer::new(&ctx.device, &camera_data, "camera")?;
camera_buf.write(&ctx.queue, &updated_camera);

// Storage buffer with typed access
let particles = StorageBuffer::new(&ctx.device, &particle_data, "particles", false);
```

### Async readback

```rust
use mabda::{read_buffer_async, PendingReadback};

let pending = read_buffer_async(&ctx.device, &ctx.queue, &output_buf, size);
// ... do other GPU work ...
let data = pending.finish(&ctx.device)?;
```

## Compute Pipelines

```rust
use mabda::{ComputePipeline, workgroups_1d};

// Single bind group (simple)
let pipeline = ComputePipeline::new(&ctx.device, SHADER_SRC, "main", 2);
pipeline.dispatch(&ctx.device, &ctx.queue, &bind_group, workgroups_1d(count, 256), 1, 1);

// Multiple bind groups
let pipeline = ComputePipeline::with_layouts(&ctx.device, SHADER_SRC, "main", &[
    &storage_entries,
    &uniform_entries,
]);
pipeline.dispatch_multi(&ctx.device, &ctx.queue, &[&bg0, &bg1], 64, 1, 1);

// Indirect dispatch
pipeline.encode_dispatch_indirect(&mut encoder, &[&bg0], &indirect_buf, 0);
```

### PingPong buffers

```rust
use mabda::PingPongBuffer;

let mut pp = PingPongBuffer::new(&ctx.device, buffer_size, "fdtd_field");
for _ in 0..iterations {
    // source() reads, dest() writes
    pipeline.dispatch(&ctx.device, &ctx.queue, &create_bg(pp.source(), pp.dest()), x, 1, 1);
    pp.swap();
}
```

## Render Pipelines

```rust
use mabda::{RenderPipelineBuilder, DrawCommand, BlendPreset, blend_state};

let pipeline = RenderPipelineBuilder::new(&ctx.device, SHADER, "vs_main", "fs_main")
    .vertex_layout(Vertex3D::layout())
    .bind_group(uniform_entries)
    .bind_group(texture_entries)
    .color_target(surface_format, Some(blend_state(BlendPreset::AlphaBlend)))
    .depth_stencil(depth.depth_stencil_state())
    .cull_mode(Some(wgpu::Face::Back))
    .build()?;

pipeline.draw(
    &ctx.device, &ctx.queue, &color_view,
    &[&uniform_bg, &texture_bg],
    &[&vertex_buf],
    Some((&index_buf, wgpu::IndexFormat::Uint32)),
    DrawCommand::DrawIndexed { index_count: 36, instance_count: 1, first_index: 0, base_vertex: 0, first_instance: 0 },
    Some(Color::BLACK),
);
```

## Render Passes

```rust
use mabda::RenderPassBuilder;

let mut pass = RenderPassBuilder::new()
    .label("geometry")
    .color_attachment(&color_view, Some(Color::CORNFLOWER_BLUE))
    .depth_attachment(&depth.view)
    .begin(&mut encoder);
// ... draw calls on pass ...
```

### MSAA

```rust
use mabda::RenderTargetBuilder;

let target = RenderTargetBuilder::new(&ctx.device, 1920, 1080)
    .format(surface_format)
    .msaa(4)
    .depth(DepthTexture::DEFAULT_FORMAT)
    .build();

let mut pass = RenderPassBuilder::new()
    .color_attachment_msaa(target.render_view(), target.resolve_target().unwrap(), Some(Color::BLACK))
    .depth_attachment(target.depth_view().unwrap())
    .begin(&mut encoder);
```

## Textures

```rust
use mabda::{Texture, create_default_sampler};

// From image bytes (PNG/JPEG)
let tex = Texture::from_bytes(&ctx.device, &ctx.queue, include_bytes!("diffuse.png"), "diffuse")?;

// Any format
let hdr_tex = Texture::from_raw(
    &ctx.device, &ctx.queue, &hdr_data, 512, 512, 8,
    wgpu::TextureFormat::Rgba16Float, "hdr", sampler,
)?;

// Defaults
let white = Texture::white_pixel(&ctx.device, &ctx.queue)?;
let normal = Texture::flat_normal(&ctx.device, &ctx.queue)?;
```

## Bind Group Layout Builder

```rust
use mabda::BindGroupLayoutBuilder;

let layout = BindGroupLayoutBuilder::new()
    .uniform_buffer(wgpu::ShaderStages::VERTEX_FRAGMENT)   // binding 0
    .texture_2d(wgpu::ShaderStages::FRAGMENT)               // binding 1
    .sampler(wgpu::ShaderStages::FRAGMENT)                   // binding 2
    .build(&ctx.device, "material_layout");
```

## Profiling

```rust
use mabda::{FrameProfiler, ProfileScope};

let mut profiler = FrameProfiler::new();

// Each frame:
profiler.begin_frame();
{
    let _scope = ProfileScope::new(&mut profiler, "shadow_pass");
    // ... render shadow pass ...
}
let frame_ms = profiler.end_frame();

// Analysis
println!("FPS: {:.0}", profiler.fps);
println!("Worst: {:.1}ms", profiler.worst_frame_ms());
println!("{}", profiler.export_json());
```

## Debug Groups

```rust
use mabda::DebugScope;

{
    let _guard = DebugScope::new(&mut encoder, "post_process");
    // ... commands visible as "post_process" in RenderDoc/PIX ...
}
```

## Error Handling

```rust
use mabda::GpuError;

match result {
    Err(e) if e.is_recoverable() => {
        // Surface timeout/outdated — retry or resize
    }
    Err(GpuError::SurfaceLost) => {
        // Reconfigure surface
    }
    Err(e) => {
        // Fatal — log and exit
        tracing::error!("{e}");
    }
    Ok(_) => {}
}
```
