# Mabda — Development Roadmap

> GPU foundation layer for AGNOS — device lifecycle, buffers, compute, textures, profiling, capability detection
>
> Consumers: soorat (renderer), rasa (image editor), ranga (image processing), bijli (EM simulation), aethersafta (desktop compositor), kiran (game engine via soorat)

### Post-Sprint Review Protocol

After completing each sprint, run a review/audit before starting the next:

1. **Cleanliness check**: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo audit`, `cargo deny check`, `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`
2. **Internal review**: Audit all new/changed code for gaps, optimizations, security, logging, errors, docs
3. **Fix findings**: Apply fixes from audit, re-run cleanliness check
4. **Additional tests/benchmarks**: From review findings
5. **Benchmarks**: Run full suite, verify no regressions
6. **Update roadmap & CHANGELOG**: Record sprint results, remove completed items

---

## Sprint 2: Core Hardening & Error Model

> Goal: Fix blocking correctness issues, structured errors, typed buffers, context customization

### 2.1 — Structured Error Types
- [ ] Replace `String` payloads with structured sub-errors preserving original wgpu error types via `#[source]`
- [ ] Distinguish recoverable (surface outdated, timeout) from fatal (device lost, adapter not found) errors
- [ ] Add `GpuError::is_recoverable()` method
- [ ] Surface errors as distinct variants (Lost, Outdated, Timeout) instead of `SurfaceTexture(String)`

### 2.2 — GpuContext Customization
- [ ] Accept optional `wgpu::DeviceDescriptor` for custom features/limits requests
- [ ] Configurable power preference (not hardcoded HighPerformance)
- [ ] Device lost callback / channel propagation to consumers
- [ ] Adapter feature query before device creation

### 2.3 — Typed Generic Buffers
- [ ] `UniformBuffer<T>` — typed wrapper with automatic std140 alignment (via `encase` or manual)
- [ ] `StorageBuffer<T>` — typed wrapper with std430 alignment
- [ ] `DynamicUniformBuffer<T>` — dynamic offset support for instanced uniforms
- [ ] Alignment validation at write time, not silent corruption
- [ ] Keep raw `create_*_buffer(&[u8])` for escape hatch

### 2.4 — Async Readback
- [ ] `read_buffer_async()` returning callback or channel-based result
- [ ] Non-blocking readback for compute consumers (bijli, rasa, ranga)
- [ ] Submission index tracking from `queue.submit()` for polling completion
- [ ] Keep `read_buffer()` sync variant for tests/screenshots

### 2.5 — Resource Dependency Safety
- [ ] Document that `GrowableBuffer::update()` invalidates dependent bind groups
- [ ] Consider bind group rebuild notification or generation tracking
- [ ] Validate dispatch workgroup counts against `max_compute_workgroups_per_dimension`
- [ ] Validate texture dimensions against `max_texture_dimension_2d` at creation

---

## Sprint 3: Advanced Textures & Render Targets

> Goal: Depth/stencil, format flexibility, MSAA, MRT, mipmaps, cubemaps

### 3.1 — Texture Format Flexibility
- [ ] Support configurable `TextureFormat` (not fixed RGBA8UnormSrgb)
- [ ] HDR texture support (Rgba16Float)
- [ ] Texture dimension validation per format and device limits
- [ ] Texture view with custom aspects (depth-only view for sampling)

### 3.2 — Depth & Stencil Textures
- [ ] `DepthTexture` struct (Depth32Float, Depth24PlusStencil8)
- [ ] Comparison sampler for shadow mapping (already have SamplerPreset::Comparison)
- [ ] Resize support matching render target dimensions
- [ ] Integration with render pipeline depth state

### 3.3 — Render Target Enhancements
- [ ] MSAA support — configurable `sample_count`, resolve texture management
- [ ] Depth attachment on render targets
- [ ] Multi-render-target (MRT) — multiple color attachments for deferred rendering
- [ ] Render pass builder — fluent API for color/depth attachments, load/store ops, clear values

### 3.4 — Mipmapping
- [ ] Mipmap generation on texture creation (compute or blit)
- [ ] Mip level count calculation helper
- [ ] Sampler mip filter configuration

### 3.5 — Cubemap & Array Textures
- [ ] `CubemapTexture` — 6-face creation, bind group helper
- [ ] `TextureArray` — layered 2D textures
- [ ] Generalized from soorat's `environment.rs`

### 3.6 — Texture Operations
- [ ] Texture-to-texture copy/blit (regions, format conversion)
- [ ] Write texture with offset (partial region updates)
- [ ] Default/fallback textures — white, black, flat normal, transparent

### 3.7 — Blend State Presets
- [ ] Premultiplied alpha, additive, multiply, alpha blend presets
- [ ] Integration with render pipeline builder `.color_target()` method

---

## Sprint 4: Compute & Pipeline Infrastructure

> Goal: Compute completeness, pipeline caching, instancing, bind group builder

### 4.1 — Compute Pipeline Enhancements
- [ ] Multi-bind-group layout support (currently single group only)
- [ ] Indirect dispatch — `encode_dispatch_indirect()` with indirect buffer
- [ ] Timestamp query integration in compute passes
- [ ] `PingPongBuffer` — double-buffer swap for iterative compute (FDTD, blur, fluid)

### 4.2 — Bind Group Layout Builder
- [ ] Fluent builder for `BindGroupLayoutDescriptor`
- [ ] Multi-group layout support (up to MAX_BIND_GROUPS)
- [ ] Preset entry helpers: uniform buffer, storage buffer, texture, sampler, comparison sampler
- [ ] Dynamic offset support

### 4.3 — Pipeline Cache & Specialization
- [ ] `PipelineCache` — hash-based dedup of render/compute pipeline descriptors
- [ ] `get_or_create()` API keyed by descriptor hash
- [ ] Bind group layout interning — share layouts across pipelines
- [ ] Pipeline specialization by key type — `SpecializedPipeline<Key>` trait pattern

### 4.4 — Shader Management
- [ ] `ShaderCache` — deduplicate `wgpu::ShaderModule` creation by source hash
- [ ] Shader preprocessing — `#import`, `#ifdef`, shader defines (evaluate naga_oil integration)
- [ ] Invalidation on source change (dev workflow, feature-gated)

### 4.5 — Instancing
- [ ] `InstanceBuffer` with dynamic growth (from soorat's instancing.rs)
- [ ] Generic `InstanceData` trait for per-instance attributes
- [ ] Vertex step mode configuration
- [ ] Integration with render pipeline draw calls

### 4.6 — Indirect Dispatch & Draw
- [ ] `create_indirect_buffer()` for `DispatchIndirect` / `DrawIndirect` / `DrawIndexedIndirect`
- [ ] Helpers for compute-driven draw call generation
- [ ] Multi-draw indirect support (capability-gated)

---

## Sprint 5: Debugging, Profiling & Polish

> Goal: Production-grade debugging, resource management, documentation

### 5.1 — Debug Labels & Groups
- [ ] `debug_group()` / `debug_marker()` wrappers on command encoder
- [ ] Auto-label all created resources (buffers, textures, pipelines)
- [ ] Configurable via feature flag (`debug-labels`)

### 5.2 — Resource Lifecycle
- [ ] Buffer/texture pool for high-frequency allocation scenarios
- [ ] Frame-scoped transient resources (allocate at frame start, reclaim at frame end)
- [ ] Staging belt integration (wgpu's `StagingBelt` for streaming uploads)
- [ ] Resource cleanup patterns / drop semantics documentation

### 5.3 — Profiler Enhancements
- [ ] Scope-based GPU profiling — `begin_scope()` / `end_scope()` with nesting
- [ ] Automatic pass timing integration
- [ ] Per-frame history ring buffer for stutter detection
- [ ] Profiler report export (JSON/CSV)
- [ ] GPU memory statistics — VRAM usage, allocation counts, bytes uploaded per frame

### 5.4 — Documentation & Examples
- [ ] Architecture overview (`docs/architecture/overview.md`)
- [ ] Usage guide with code examples for each module
- [ ] Consumer integration guide (how soorat/rasa/bijli depend on mabda)
- [ ] Inline doc examples (`cargo test --doc`)

---

## Backlog (demand-gated)

> Only promoted to a sprint when a consumer needs it

- [ ] Render graph (DAG-based pass orchestration) — likely belongs in soorat, not foundation
- [ ] Multi-queue coordination (async compute + graphics)
- [ ] Compressed texture format support (BC/DXT, ETC2, ASTC)
- [ ] SPIR-V shader support (alongside WGSL)
- [ ] Compute barrier/atomics helpers
- [ ] Texture streaming / virtual textures
- [ ] Backend selection at compile time (feature-gated)
- [ ] WebGPU/WASM target support
- [ ] Scissor/viewport helpers for per-region rendering
- [ ] Command encoder abstraction / multi-pass batching
- [ ] Vertex format extensibility (derive macro for custom layouts)

---

## v1.0 Criteria

- [ ] All Sprint 2–5 items complete
- [ ] 80%+ test coverage
- [ ] Benchmark suite covering all hot paths
- [ ] Zero `unwrap()` / `panic!()` in library code
- [ ] All public types documented with examples
- [ ] At least one consumer (soorat) fully migrated to mabda pipelines
- [ ] `cargo clippy --all-features -- -D warnings` clean
- [ ] `cargo deny check` clean
- [ ] CHANGELOG complete with benchmark numbers for performance items
- [ ] Structured errors with source chaining throughout
- [ ] No silent data corruption (typed buffers with alignment enforcement)
- [ ] Device lost handling for all consumers
