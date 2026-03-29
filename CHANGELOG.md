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

### Changed

- **buffer** — all buffer creation functions now emit `tracing::debug!` with label and size
- **error paths** — all error paths now emit `tracing::warn!` (recoverable) or `tracing::error!` (failures) before returning errors
- **context** — adapter request and device creation failures now log at `error` level
- **texture** — validation errors (zero-size, overflow, size mismatch) now log at `warn`/`error` level
- **render_target** — readback overflow and map failures now log at `error` level
- **surface** — acquire failures log at `warn` (timeout, outdated) or `error` (lost)
- **capabilities** — `report()` uses `write!` instead of `format!` to avoid temporary allocation
- **profiler** — `begin_frame()`, `end_frame()`, `record_pass()`, `query_set()`, `max_passes()` marked `#[inline]`
- **compute** — `bind_group_layout()`, `raw()` marked `#[inline]`; `dispatch()` emits tracing

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
