# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

#### Caching
- **bind_group_cache** — `BindGroupCache` for deduplicating bind group creation by caller-provided key; supports `get_or_insert()`, `invalidate()`, `invalidate_where()` for bulk invalidation on resource reallocation

#### Render Pipeline
- **render_pipeline** — `RenderPipelineBuilder::depth_only()` constructor for depth-only pipelines (shadow maps, depth pre-pass); fragment shader is now optional via `Option<&str>` fragment entry

#### Documentation
- **all modules** — `/// # Examples` doc blocks on all 35+ public types (was 8/35)
- **adr** — ADR-001 (public fields), ADR-002 (runtime alignment), ADR-003 (fixed vertex types) — all resolved from soorat migration findings

#### Testing
- **all modules** — GPU integration tests using headless wgpu backend; 278 tests (was 162), 75.4% line coverage (was 22.47%)

#### Infrastructure
- **ci** — GitHub Actions CI pipeline: check/lint, security audit, cargo-deny, multi-platform tests (Linux/macOS/Windows), feature-gate tests, MSRV (1.89), coverage (codecov.io), documentation, benchmarks
- **release** — GitHub Actions release pipeline: tag-triggered, version consistency verification (VERSION/Cargo.toml/tag), crates.io publish, GitHub Release with auto-generated notes
- **Makefile** — local `make check` (fmt + clippy + test + audit), plus bench, coverage, doc targets
- **codecov.yml** — coverage thresholds (5% project, 70% patch)
- **scripts/coverage-check.sh** — CI coverage gate script, fails if line coverage drops below threshold (default: 70%)
- **rust-toolchain.toml** — pins stable channel with rustfmt + clippy
- **CONTRIBUTING.md** — development workflow, prerequisites, code style, testing, benchmarks
- **SECURITY.md** — vulnerability reporting via GitHub Security Advisories, response SLAs
- **CODE_OF_CONDUCT.md** — Contributor Covenant v2.1

### Changed

#### Audit Hardening
- **buffer** — `read_buffer_typed`, `GrowableBuffer::new/update` use checked/saturating arithmetic to prevent integer overflow on 32-bit targets
- **typed_buffer** — `StorageBuffer::empty` uses saturating arithmetic for size calculation
- **instancing** — `InstanceBuffer::with_capacity/update` use saturating arithmetic for buffer size and growth factor
- **render_target** — `read_pixels` uses `u64` arithmetic for pixel buffer size to prevent overflow on large textures
- **profiler** — `GpuTimestamps::new` uses `saturating_mul` for query count; `read_results` logs warning on readback failure instead of silently returning empty; `ProfileScope::drop` uses `mem::take` instead of `clone` to avoid allocation
- **compute** — `ComputePipeline::with_layouts` now emits `tracing::debug!` on pipeline creation; bind group layout labels use `write!` instead of `format!`
- **render_pipeline** — bind group layout labels use `write!` instead of `format!` to avoid temporary allocations in loop
- **context** — `adapter_info()`, `limits()`, `features()` marked `#[inline]`
- **color** — `from_rgba8()`, `from_hex()`, `to_wgpu()` marked `#[inline]`
- **blend** — `blend_state()` marked `#[inline]`
- **depth** — `depth_stencil_state()` marked `#[inline]`
- **profiler** — `FrameProfiler::new()`, `with_alpha()`, `with_alpha_and_history()` marked `#[must_use]`
- **vertex** — `VertexLayout::layout()` trait method marked `#[must_use]`

#### Core
- **context** — `GpuContextBuilder` for custom device features, limits, power preference, and device lost callback
- **error** — structured error variants with `#[source]` chaining (`DeviceRequest`, `ReadbackMap`, `ImageDecode`); surface errors split into `SurfaceTimeout` / `SurfaceOutdated` / `SurfaceLost`; `is_recoverable()` method; `WorkgroupLimitExceeded` and `TextureDimensionExceeded` validation variants
- **typed_buffer** — `UniformBuffer<T>` with 16-byte alignment enforcement; `StorageBuffer<T>` with capacity-checked writes
- **buffer** — `PendingReadback` + `read_buffer_async()` for non-blocking GPU readback; `GrowableBuffer` with generation counter for bind group invalidation; `create_dispatch_indirect_buffer()`, `create_draw_indirect_buffer()`, `create_draw_indexed_indirect_buffer()`
- **debug** — `DebugScope` RAII guard for push/pop debug groups; marker helpers for encoders, render passes, and compute passes
- **resource** — `FrameResources` for tracking transient GPU buffers/textures with end-of-frame cleanup
- **shader** — `ShaderCache` for hash-based shader module deduplication
- **pipeline_cache** — `PipelineCache` for hash-based render/compute pipeline deduplication, feature-gated per pipeline type

#### Compute
- **compute** — multi-bind-group support (`with_layouts()`, `dispatch_multi()`, `encode_dispatch_multi()`); `encode_dispatch_indirect()` for GPU-driven dispatch; `validate_dispatch()` for workgroup limit checking; `PingPongBuffer` for iterative compute patterns

#### Graphics
- **render_pipeline** — `RenderPipeline` with `RenderPipelineBuilder` (vertex/fragment shaders, vertex layouts, blend state, depth/stencil, topology, cull mode); `DrawCommand` enum
- **render_pass** — `RenderPassBuilder` for fluent render pass construction with MRT, MSAA resolve targets, depth/stencil attachments
- **render_target** — `RenderTargetBuilder` with MSAA support and optional depth attachment; `render_view()`, `resolve_target()`, `depth_view()` helpers
- **vertex** — `Vertex2D` (32B), `Vertex3D` (48B), `SkinnedVertex3D` (96B) with `VertexLayout` trait
- **surface** — `SurfaceState` for wgpu surface lifecycle; `PresentModePreference` enum
- **sampler** — `SamplerPreset` enum (Nearest, Linear, Anisotropic, Comparison); `create_sampler()`, `create_sampler_custom()`
- **depth** — `DepthTexture` with format constants, resize, `depth_stencil_state()` helper
- **blend** — `BlendPreset` enum (Opaque, AlphaBlend, PremultipliedAlpha, Additive, Multiply)
- **bind_group** — `BindGroupLayoutBuilder` with preset entry methods and auto-incrementing binding indices
- **instancing** — `InstanceData` (80B: model matrix + color) with `InstanceBuffer` dynamic growth
- **texture** — `Texture::from_raw()` for any format; `CubemapTexture`; default textures (`black_pixel`, `transparent_pixel`, `flat_normal`); `copy_texture_to_texture()`; `mip_level_count()`; `validate_dimensions()`
- **buffer** — `create_vertex_buffer()`, `create_index_buffer()` helpers

#### Profiling
- **profiler** — frame history ring buffer with `worst_frame_ms()` / `best_frame_ms()`; `export_json()` and `export_history_csv()`; `ProfileScope` RAII timer

#### Infrastructure
- **feature gates** — `compute` and `graphics` features enforced on modules; default `["full"]`
- **deny.toml** — license, advisory, and source checks
- **docs** — architecture overview, usage guide, consumer integration guide

### Changed

- **error** — `GpuError` restructured: `DeviceRequest` wraps `wgpu::RequestDeviceError` via `#[source]`, `ReadbackMap` wraps `wgpu::BufferAsyncError`, `ImageDecode` wraps `image::ImageError`
- **buffer** — all creation functions emit `tracing::debug!`; error paths emit `tracing::warn!` or `tracing::error!`
- **capabilities** — `report()` uses `write!` over `format!` to avoid temporary allocation
- **profiler** — `begin_frame()`, `end_frame()`, `record_pass()`, `query_set()`, `max_passes()` marked `#[inline]`
- **compute** — `bind_group_layout()` now takes index parameter (multi-group support); `raw()` marked `#[inline]`

### Breaking

- **error** — removed `SurfaceTexture(String)` (replaced by `SurfaceTimeout`, `SurfaceOutdated`, `SurfaceLost`); removed `Readback(String)` (replaced by `ReadbackMap`, `ReadbackChannel`); `DeviceRequest` now wraps `wgpu::RequestDeviceError` instead of `String`
- **compute** — `bind_group_layout()` signature changed from `&self -> &BindGroupLayout` to `&self, usize -> Option<&BindGroupLayout>`

### Removed

- **Cargo.lock** — removed from tracking (library crate per Rust convention)

## [0.1.0] - 2026-03-29

### Added

- **context** — `GpuContext` with headless and surface-aware GPU initialization
- **compute** — `ComputePipeline` with WGSL shader compilation, single bind group layout, dispatch helpers; `workgroups_1d()`, `workgroups_2d()`
- **buffer** — `create_storage_buffer()`, `create_storage_buffer_empty()`, `create_uniform_buffer()`, `create_staging_buffer()`, `read_buffer()`, `read_buffer_typed()`
- **texture** — `Texture` with PNG/JPEG loading, solid color, RGBA creation; `TextureCache`; `create_default_sampler()`
- **render_target** — `RenderTarget` offscreen framebuffer with CPU readback
- **profiler** — `FrameProfiler` with EMA smoothing; `GpuTimestamps` for GPU timestamp queries
- **capabilities** — `GpuCapabilities` with adapter limits, feature detection, serde, WebGPU constants
- **color** — `Color` type with f32 RGBA, hex/u8 constructors, lerp, luminance
- **error** — `GpuError` with 10 typed variants, `#[non_exhaustive]`, Send+Sync
