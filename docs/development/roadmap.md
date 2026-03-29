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
6. **Update roadmap**: Record sprint results and findings

---

## P(-1): Scaffold Hardening

### Research Findings

#### Current State (v0.1.0)

**Present:**
- `GpuContext` — device/queue/adapter ownership, headless + surface-aware init
- `ComputePipeline` — shader compilation, single bind group layout, 1D/2D dispatch
- Buffer helpers — storage (init + empty), uniform, staging, sync readback
- `Texture` — PNG/JPEG loading, solid color, caching, bind group helpers (fixed RGBA8UnormSrgb)
- `RenderTarget` — offscreen framebuffer, CPU readback with row alignment handling
- `FrameProfiler` — CPU timing with EMA, per-pass recording
- `GpuTimestamps` — conditional GPU timestamp queries
- `GpuCapabilities` — adapter limits, feature detection, serde support
- `Color` — f32 RGBA, hex/u8 constructors, lerp, luminance
- `GpuError` — 10 typed variants, Send+Sync

**Missing (identified via soorat audit):**
- No render pipeline abstraction (compute has one, graphics does not)
- No vertex/index buffer helpers or vertex attribute layouts
- No surface lifecycle management (resize, reconfigure, frame acquisition)
- No render graph / multi-pass orchestration
- No instancing support
- No depth/stencil texture support
- No sampler variety (only ClampToEdge + Nearest)
- No mipmapping, array textures, or cubemaps
- No multi-bind-group layout builders
- No shader module caching
- No debug labels/groups in command encoding
- No indirect dispatch/draw support

#### Soorat Code Available for Extraction

| Soorat File | Capability | Extraction Notes |
|---|---|---|
| `vertex.rs` | Vertex2D, Vertex3D, SkinnedVertex3D, buffer layouts | Generic vertex types + layout descriptors |
| `render_graph.rs` | Multi-pass orchestration, topological sort, PassType enum | Make PassType extensible, remove renderer-specific variants |
| `instancing.rs` | InstanceData, InstanceBuffer with dynamic growth | Generic per-instance buffer management |
| `window.rs` | Surface + window integration, resize handling | Surface lifecycle management |
| `environment.rs` | Cubemap textures, bind group management | Generalize naming (IblBindGroup → CubemapBindGroup) |

#### Patterns to Adopt from Soorat

- Persistent buffer regrowth pattern (SpriteBuffers/InstanceBuffer)
- Multi-bind-group organization (group 0: uniforms, 1: textures, 2+: special)
- Full-screen triangle rendering (no vertex buffer, shader-generated geometry)
- Command encoder + pass pattern with proper scoping

### Hardening Results

#### Completed Fixes
- Added `#[inline]` to 7 hot-path methods (profiler: `begin_frame`, `end_frame`, `record_pass`, `query_set`, `max_passes`; compute: `bind_group_layout`, `raw`)
- Added `tracing::debug!` to 5 operations (buffer readback, compute dispatch, texture RGBA creation, capabilities query, render target readback)
- Replaced `format!` with `write!` in `GpuCapabilities::report()` to avoid temporary allocation
- Removed `Cargo.lock` from tracking (library crate)
- Created `deny.toml` for license/advisory/source checks
- Created `.gitignore` (was missing entirely; `target/` was committed)

#### Tests Added (43 → 53)
- `capabilities_report_format` — validates report output content
- `capabilities_uniform_storage_fits` — validates limit checking methods
- `workgroups_1d_single` / `workgroups_2d_single` — zero and single-element edge cases
- `workgroups_1d_large` — validates large dispatch calculations
- `color_lerp_midpoints` — quarter-point interpolation accuracy
- `color_from_hex_components` — per-channel hex parsing
- `profiler_total_pass_time_empty` — zero-pass total
- `profiler_custom_alpha` — EMA alpha clamping
- `profiler_multiple_resets` — repeated reset cycles

#### Benchmarks Added (5 → 7)
- `color_luminance`: ~288 ps
- `capabilities_report`: ~325 ns (write! impl)

#### Baseline Benchmarks
| Benchmark | Time |
|---|---|
| `color_lerp` | ~261 ps |
| `color_from_hex` | ~258 ps |
| `color_luminance` | ~288 ps |
| `workgroups_1d` | ~258 ps |
| `workgroups_2d` | ~263 ps |
| `profiler_frame_cycle` | ~66 ns |
| `capabilities_report` | ~325 ns |

#### External Research Findings
- **Thin wrappers over wgpu, not heavy abstractions** — bevy_render, rend3, iced_wgpu all wrap wgpu types with newtype + Deref, keeping the full wgpu API accessible. Mabda's current approach (public wgpu fields on GpuContext) aligns.
- **Render graph belongs in consumers, not foundation** — bevy puts its render graph in bevy_render (a higher layer). Mabda should provide primitives; soorat owns orchestration. Moved render graph to backlog.
- **Pipeline caching via descriptor hashing** — universal pattern (bevy, screen-13, rend3). `PipelineCache` with `get_or_create()` keyed by descriptor hash. Added to Sprint 3.
- **Bind group layout interning** — hash-based dedup of layouts shared across pipelines. Added to Sprint 2.5.
- **Size-class buffer pooling** — highest-impact resource management feature. Pool buffers in power-of-two size classes, recycle on drop. Added to Sprint 3.
- **Shader module cache keyed by source hash** — already planned (Sprint 3.3). Research confirms hash-based dedup + optional hot-reload behind feature gate.
- **Profiling: scope-based API** — `wgpu-profiler` pattern (begin_scope/end_scope or RAII guard). Consider for profiler enhancement.
- **Feature-gated profiling** — profiling calls should compile to no-ops when disabled.

#### Known Issues (deferred to sprints)
- Feature gates (`graphics`, `compute`) declared in Cargo.toml but not enforced in lib.rs — Sprint 1
- `debug_assert` for Color NaN/Inf (silent in release) — acceptable for GPU hot path
- No integration tests requiring GPU hardware — requires CI with GPU or mock adapter
- Texture format fixed to RGBA8UnormSrgb — Sprint 2.2

---

## Sprint 1: Core Pipeline Infrastructure ✓

> Goal: Render pipeline parity with compute pipeline, vertex/index support, surface lifecycle

### 1.1 — Render Pipeline Abstraction ✓
- [x] `RenderPipeline` struct wrapping `wgpu::RenderPipeline` + bind group layout(s)
- [x] `RenderPipelineBuilder` with vertex/fragment shaders, vertex layouts, blend state, depth config
- [x] `encode_draw()` for batching into existing encoder
- [x] `draw()` one-shot convenience method
- [x] Multi-bind-group layout support (Vec<BindGroupLayout>)
- [x] `DrawCommand` enum (Draw, DrawIndexed) with `#[non_exhaustive]`

### 1.2 — Vertex & Index Buffers ✓
- [x] `create_vertex_buffer()` — generic over `bytemuck::Pod`
- [x] `create_index_buffer()` — generic (u16/u32)
- [x] `VertexLayout` trait for `wgpu::VertexBufferLayout` generation
- [x] `Vertex2D` (32B), `Vertex3D` (48B), `SkinnedVertex3D` (96B) — extracted from soorat
- [x] `GrowableBuffer` — persistent buffer with 3/2 exponential regrowth

### 1.3 — Surface Lifecycle ✓
- [x] `SurfaceState` managing `wgpu::SurfaceConfiguration`
- [x] `configure()` with sRGB format preference and validation
- [x] `resize()` with zero-size skip
- [x] `acquire()` with error handling for all surface states
- [x] `PresentModePreference` enum (Vsync/NoVsync/Immediate/Mailbox)

### 1.4 — Sampler Expansion ✓
- [x] `SamplerPreset` enum: Nearest, Linear, Anisotropic, Comparison
- [x] `create_sampler()` and `create_sampler_custom()` helpers
- [x] `create_default_sampler()` retained for backward compat

### 1.5 — Feature Gate Enforcement ✓
- [x] `compute` feature gates compute module
- [x] `graphics` feature gates texture, render_target, vertex, sampler, render_pipeline, surface
- [x] Core modules (buffer, context, error, capabilities, color, profiler) always available
- [x] Default features changed to `["full"]`

### Sprint 1 Post-Review Findings
- Hardened tracing: added `debug!` to all 6 buffer creation functions, `warn!`/`error!` to all error paths
- Tracing levels: `debug` for normal operations, `warn` for recoverable errors (zero-size, timeout, outdated), `error` for failures (lost surface, adapter not found, overflow, readback failure)
- Tests: 53 → 90 (37 new across vertex, sampler, surface, render_pipeline, buffer)
- Added `VertexLayout` trait doc comment, re-exported `create_sampler_custom`

---

## Sprint 2: Advanced Textures & Bind Groups

> Goal: Depth/stencil, mipmaps, cubemaps, flexible bind group management

### 2.1 — Depth & Stencil Textures
- [ ] `DepthTexture` struct (Depth32Float, Depth24PlusStencil8)
- [ ] Comparison sampler for shadow mapping consumers
- [ ] Resize support matching render target dimensions
- [ ] Integration with render pipeline depth state

### 2.2 — Texture Format Flexibility
- [ ] Support configurable `TextureFormat` (not fixed RGBA8UnormSrgb)
- [ ] HDR texture support (Rgba16Float)
- [ ] Format conversion utilities
- [ ] Texture dimension validation per format

### 2.3 — Mipmapping
- [ ] Mipmap generation on texture creation (compute or blit)
- [ ] Mip level count calculation
- [ ] Sampler mip filter configuration

### 2.4 — Cubemap & Array Textures
- [ ] `CubemapTexture` — 6-face creation, bind group helper
- [ ] `TextureArray` — layered 2D textures
- [ ] Generalized from soorat's `environment.rs`

### 2.5 — Bind Group Layout Builder
- [ ] Fluent builder for `BindGroupLayoutDescriptor`
- [ ] Multi-group layout support (up to MAX_BIND_GROUPS)
- [ ] Preset entry helpers: uniform buffer, storage buffer, texture, sampler, comparison sampler
- [ ] Dynamic offset support

---

## Sprint 3: Caching, Pooling & Performance

> Goal: Pipeline caching, buffer pooling, instancing, shader caching, indirect dispatch

### 3.1 — Pipeline Cache
- [ ] `PipelineCache` — hash-based dedup of `RenderPipeline` and `ComputePipeline` descriptors
- [ ] `get_or_create()` API keyed by descriptor hash
- [ ] Bind group layout interning — share layouts across pipelines
- [ ] Optional async pipeline compilation (non-blocking creation)

### 3.2 — Buffer Pool
- [ ] Size-class buffer pooling (power-of-two sizes)
- [ ] Generational handles for safe buffer reuse
- [ ] Automatic recycling on drop
- [ ] Staging belt for upload batching

### 3.3 — Instancing
- [ ] `InstanceBuffer` with dynamic growth (from soorat's instancing.rs)
- [ ] Generic `InstanceData` trait for per-instance attributes
- [ ] Vertex step mode configuration
- [ ] Integration with render pipeline draw calls

### 3.4 — Shader Module Cache
- [ ] `ShaderCache` — deduplicate `wgpu::ShaderModule` creation
- [ ] Key by source hash or label
- [ ] Invalidation on source change (dev workflow)

### 3.5 — Indirect Dispatch & Draw
- [ ] `create_indirect_buffer()` for `DispatchIndirect` / `DrawIndirect` / `DrawIndexedIndirect`
- [ ] Helpers for compute-driven draw call generation
- [ ] Multi-draw indirect support (capability-gated)

### 3.6 — Compute Pipeline Enhancements
- [ ] Multi-bind-group layout support (currently single group only)
- [ ] Timestamp query integration in compute passes
- [ ] Push constant support (when available)
- [ ] Workgroup shared memory size declaration

---

## Sprint 4: Debugging, Validation & Polish

> Goal: Production-grade debugging, resource management, documentation

### 4.1 — Debug Labels & Groups
- [ ] `debug_group()` / `debug_marker()` wrappers on command encoder
- [ ] Auto-label all created resources (buffers, textures, pipelines)
- [ ] Configurable via feature flag (`debug-labels`)

### 4.2 — Resource Lifecycle
- [ ] Lost device detection and recovery path
- [ ] Resource cleanup patterns / drop semantics documentation
- [ ] Buffer/texture pool for high-frequency allocation scenarios

### 4.3 — Profiler Enhancements
- [ ] GPU timestamp integration into render/compute pass recording
- [ ] Per-pass GPU timing in render graph
- [ ] Profiler report export (JSON/CSV)

### 4.4 — Documentation & Examples
- [ ] Architecture overview (`docs/architecture/overview.md`)
- [ ] Usage guide with code examples for each module
- [ ] Consumer integration guide (how soorat/rasa/bijli depend on mabda)
- [ ] Inline doc examples (`cargo test --doc`)

---

## Backlog (demand-gated)

> Only promoted to a sprint when a consumer needs it

- [ ] Render graph (DAG-based pass orchestration) — likely belongs in soorat, not foundation
- [ ] Multi-queue coordination (async compute + graphics)
- [ ] Memory budget tracking / allocation strategy
- [ ] Specialization constants
- [ ] SPIR-V shader support (alongside WGSL)
- [ ] Compute barrier/atomics helpers
- [ ] Buffer layout helpers (std140/std430 alignment)
- [ ] Texture streaming / virtual textures
- [ ] Async GPU readback (non-blocking)
- [ ] Backend selection at compile time (feature-gated)
- [ ] WebGPU/WASM target support

---

## v1.0 Criteria

- [ ] All Sprint 1–4 items complete
- [ ] 80%+ test coverage
- [ ] Benchmark suite covering all hot paths
- [ ] Zero `unwrap()` / `panic!()` in library code
- [ ] All public types documented with examples
- [ ] At least one consumer (soorat) fully migrated to mabda pipelines
- [ ] `cargo clippy --all-features -- -D warnings` clean
- [ ] `cargo deny check` clean
- [ ] CHANGELOG complete with benchmark numbers for performance items
