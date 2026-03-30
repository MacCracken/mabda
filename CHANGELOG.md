# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **render_pipeline** — `RenderPipeline` struct with `RenderPipelineBuilder` for fluent pipeline construction, `DrawCommand` enum (Draw, DrawIndexed), `encode_draw()` for batched submissions, `draw()` for one-shot rendering
- **vertex** — `Vertex2D` (32B), `Vertex3D` (48B), `SkinnedVertex3D` (96B) vertex types with pre-computed `wgpu::VertexBufferLayout` descriptors; `VertexLayout` trait for generic pipeline construction
- **buffer** — `create_vertex_buffer()`, `create_index_buffer()` helpers; `GrowableBuffer` with 3/2 exponential regrowth for frame-varying data
- **surface** — `SurfaceState` for wgpu surface lifecycle (configure, resize, acquire); `PresentModePreference` enum (Vsync, NoVsync, Immediate, Mailbox); sRGB format preference on configure
- **sampler** — `SamplerPreset` enum (Nearest, Linear, Anisotropic, Comparison); `create_sampler()` and `create_sampler_custom()` helpers
- **feature gates** — `compute` feature gates compute module; `graphics` feature gates texture, render_target, vertex, sampler, render_pipeline, surface; core modules always available; default features changed to `["full"]`
- **deny.toml** — license, advisory, and source checks for dependencies
- **typed_buffer** — `UniformBuffer<T>` with 16-byte alignment enforcement; `StorageBuffer<T>` for typed storage buffer management; both use `bytemuck::Pod` for safe byte casting
- **depth** — `DepthTexture` struct with `Depth32Float` and `Depth24PlusStencil8` format constants, resize support, `depth_stencil_state()` helper for pipeline integration
- **blend** — `BlendPreset` enum (Opaque, AlphaBlend, PremultipliedAlpha, Additive, Multiply) with `blend_state()` converter
- **render_pass** — `RenderPassBuilder` for fluent render pass construction with multiple color attachments (MRT), MSAA resolve targets, depth/stencil attachments
- **render_target** — `RenderTargetBuilder` with MSAA support (multisampled + resolve textures), optional depth attachment; `render_view()` and `resolve_target()` helpers
- **texture** — `Texture::from_raw()` for any `TextureFormat` (not just RGBA8); default textures: `black_pixel()`, `transparent_pixel()`, `flat_normal()`; `CubemapTexture` for 6-face cubemaps with bind group helper; `copy_texture_to_texture()` utility; `mip_level_count()` calculation helper; `Texture::format()` accessor
- **compute** — multi-bind-group support: `with_layouts()` for multiple groups, `dispatch_multi()` / `encode_dispatch_multi()`, `encode_dispatch_indirect()` for GPU-driven dispatch; `PingPongBuffer` for iterative compute patterns (FDTD, blur, fluid)
- **bind_group** — `BindGroupLayoutBuilder` with preset methods: `uniform_buffer()`, `storage_buffer()`, `texture_2d()`, `texture_cube()`, `texture_depth_2d()`, `sampler()`, `comparison_sampler()`, auto-incrementing binding indices
- **instancing** — `InstanceData` (80B: model matrix + color tint) with vertex layout at locations 7–11; `InstanceBuffer` with dynamic growth
- **shader** — `ShaderCache` for hash-based shader module deduplication with `get_or_compile()`, `invalidate()`, and `clear()`
- **pipeline_cache** — `PipelineCache` for hash-based render/compute pipeline deduplication with `get_or_insert_render()` / `get_or_insert_compute()`, feature-gated per pipeline type
- **buffer** — `create_dispatch_indirect_buffer()`, `create_draw_indirect_buffer()`, `create_draw_indexed_indirect_buffer()` for GPU-driven dispatch and draw
- **debug** — `DebugScope` RAII guard for push/pop debug groups on command encoders; `push_debug_group()`, `pop_debug_group()`, `insert_debug_marker()` for encoders, render passes, and compute passes
- **resource** — `FrameResources` for tracking transient GPU buffers/textures with automatic end-of-frame cleanup
- **profiler** — frame history ring buffer with `worst_frame_ms()` / `best_frame_ms()` for stutter detection; `export_json()` and `export_history_csv()` for post-frame analysis; `ProfileScope` RAII timer that auto-records pass duration on drop; `with_alpha_and_history()` constructor
- **docs** — architecture overview (`docs/architecture/overview.md`), usage guide with examples (`docs/guides/usage.md`), consumer integration guide (`docs/guides/integration.md`)
- **context** — `GpuContextBuilder` for custom device features, limits, power preference, and device lost callback; existing `GpuContext::new()` / `new_for_surface()` remain as convenience wrappers
- **buffer** — `PendingReadback` + `read_buffer_async()` for non-blocking GPU readback; `read_buffer()` refactored to delegate to async path; `GrowableBuffer::generation()` counter for bind group invalidation detection
- **compute** — `validate_dispatch()` checks workgroup counts against `max_compute_workgroups_per_dimension`
- **texture** — `validate_dimensions()` checks against `max_texture_dimension_2d`

### Changed

- **error** — `GpuError` restructured with `#[source]` chaining: `DeviceRequest` wraps `wgpu::RequestDeviceError`, `ReadbackMap` wraps `wgpu::BufferAsyncError`, `ImageDecode` wraps `image::ImageError`; surface errors split into `SurfaceTimeout` / `SurfaceOutdated` / `SurfaceLost`; added `is_recoverable()` method; added `WorkgroupLimitExceeded` and `TextureDimensionExceeded` variants

- **buffer** — all buffer creation functions now emit `tracing::debug!` with label and size
- **error paths** — all error paths now emit `tracing::warn!` (recoverable) or `tracing::error!` (failures) before returning errors
- **context** — adapter request and device creation failures now log at `error` level
- **texture** — validation errors (zero-size, overflow, size mismatch) now log at `warn`/`error` level
- **render_target** — readback overflow and map failures now log at `error` level
- **surface** — acquire failures log at `warn` (timeout, outdated) or `error` (lost)
- **capabilities** — `report()` uses `write!` instead of `format!` to avoid temporary allocation
- **profiler** — `begin_frame()`, `end_frame()`, `record_pass()`, `query_set()`, `max_passes()` marked `#[inline]`
- **compute** — `bind_group_layout()`, `raw()` marked `#[inline]`; `dispatch()` emits tracing

### Breaking

- **error** — removed `SurfaceTexture(String)` variant (replaced by `SurfaceTimeout`, `SurfaceOutdated`, `SurfaceLost`); removed `Readback(String)` variant (replaced by `ReadbackMap`, `ReadbackChannel`); `DeviceRequest` now wraps `wgpu::RequestDeviceError` instead of `String`

### Removed

- **Cargo.lock** — removed from tracking (library crate per Rust convention)

## [0.1.0] - 2026-03-29

### Added

- **context** — `GpuContext` with headless and surface-aware GPU initialization
- **compute** — `ComputePipeline` with WGSL shader compilation, bind group layout, dispatch helpers; `workgroups_1d()`, `workgroups_2d()` utilities
- **buffer** — `create_storage_buffer()`, `create_storage_buffer_empty()`, `create_uniform_buffer()`, `create_staging_buffer()`, `read_buffer()`, `read_buffer_typed()` helpers
- **texture** — `Texture` with PNG/JPEG loading, solid color, RGBA creation; `TextureCache` with lazy loading and bind group management; `create_default_sampler()`
- **render_target** — `RenderTarget` offscreen framebuffer with CPU readback and row alignment handling
- **profiler** — `FrameProfiler` with EMA smoothing and per-pass recording; `GpuTimestamps` for conditional GPU timestamp queries
- **capabilities** — `GpuCapabilities` with adapter limits, feature detection, serde support, WebGPU compatibility constants
- **color** — `Color` type with f32 RGBA, hex/u8 constructors, lerp, luminance, wgpu conversion
- **error** — `GpuError` with 10 typed variants, `#[non_exhaustive]`, Send+Sync
