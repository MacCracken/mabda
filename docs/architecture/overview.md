# Architecture Overview

## What Mabda Is

Mabda is a GPU foundation library. It owns the `wgpu` dependency and provides shared GPU infrastructure so that every AGNOS consumer builds on one consistent base instead of duplicating device management, buffer creation, and pipeline setup.

## Module Map

```
                    ┌─────────────────────────────────────┐
                    │           GpuContext                 │
                    │   (device, queue, adapter, instance) │
                    └──────────────┬──────────────────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         │                         │                         │
    ┌────▼────┐              ┌─────▼─────┐            ┌──────▼──────┐
    │ Compute │              │   Core    │            │  Graphics   │
    │         │              │           │            │             │
    │ compute │              │ buffer    │            │ texture     │
    │         │              │ typed_buf │            │ depth       │
    └─────────┘              │ error     │            │ render_tgt  │
                             │ color     │            │ render_pipe │
    ┌─────────┐              │ caps      │            │ render_pass │
    │ Caching │              │ debug     │            │ vertex      │
    │         │              │ resource  │            │ sampler     │
    │ shader  │              │ profiler  │            │ surface     │
    │ pipe_$  │              └───────────┘            │ blend       │
    └─────────┘                                      │ bind_group  │
                                                     │ instancing  │
                                                     └─────────────┘
```

## Feature Gates

| Feature | Modules |
|---------|---------|
| *(always)* | context, error, buffer, typed_buffer, capabilities, color, profiler, debug, resource, shader, pipeline_cache |
| `compute` | compute (ComputePipeline, PingPongBuffer, validate_dispatch, workgroups) |
| `graphics` | texture, depth, render_target, render_pipeline, render_pass, vertex, sampler, surface, blend, bind_group, instancing |
| `full` | compute + graphics (default) |

## Data Flow

### Compute consumer (e.g., bijli)

```
GpuContext::new()
  → ComputePipeline::with_layouts(device, shader, bind_groups)
  → create_storage_buffer(device, data) or StorageBuffer<T>::new()
  → pipeline.dispatch(device, queue, bind_group, x, y, z)
  → read_buffer(device, queue, output_buf, size)
```

### Graphics consumer (e.g., soorat)

```
GpuContext::new_for_surface(surface)
  → SurfaceState::configure(surface, adapter, device, w, h, Vsync)
  → RenderPipelineBuilder::new(device, shader, "vs", "fs")
      .vertex_layout(Vertex3D::layout())
      .color_target(format, Some(blend_state(AlphaBlend)))
      .depth_stencil(depth.depth_stencil_state())
      .build()
  → RenderPassBuilder::new()
      .color_attachment(&color_view, Some(Color::BLACK))
      .depth_attachment(&depth.view)
      .begin(&mut encoder)
  → surface_state.acquire(surface) → present
```

## Consumers

| Crate | Uses | Feature |
|-------|------|---------|
| soorat | render pipeline, vertex, texture, depth, surface, instancing, blend | `graphics` |
| rasa | compute pipeline, typed buffers, async readback | `compute` |
| ranga | compute pipeline, texture, typed buffers | `full` |
| bijli | compute pipeline, PingPongBuffer, async readback | `compute` |
| aethersafta | render pipeline, surface, blend, render pass | `graphics` |
| kiran | via soorat | `full` |

## Dependencies

```
mabda
  └── wgpu 29         (GPU abstraction)
  └── bytemuck 1       (safe byte casting)
  └── pollster 0.4     (async runtime for GPU init)
  └── serde 1          (serialization)
  └── thiserror 2      (error derivation)
  └── tracing 0.1      (structured logging)
  └── image 0.25       (optional, PNG/JPEG loading)
```
