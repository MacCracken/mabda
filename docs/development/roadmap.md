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

## Pre-v1.0 Architectural Decisions

Decisions that must be resolved during soorat migration, before tagging v1.0. Taking breaking changes after v1 is costly.

### GpuContext public fields vs accessor methods — RESOLVED

**Decision:** Keep public fields. See [ADR-001](../adr/001-gpucontext-public-fields.md).

Soorat accesses `ctx.device` and `ctx.queue` in many call sites with zero friction. Wrapping adds cost without current benefit. Multi-queue, if needed, is additive.

### Typed buffer alignment strategy — RESOLVED

**Decision:** Keep runtime validation, no `encase`. See [ADR-002](../adr/002-runtime-alignment-validation.md).

Soorat's 4 uniform types were already correctly aligned. Zero alignment errors during migration. Runtime check is sufficient.

### Vertex type extensibility — RESOLVED

**Decision:** Keep fixed types + manual `VertexLayout` trait, no derive macro. See [ADR-003](../adr/003-fixed-vertex-types.md).

Soorat uses all three provided types, defines zero custom ones. Derive macro is demand-gated.

---

## v1.0 Remaining
- [x] P1 - RenderPipelineBuilder depth-only pipeline support (`fragment: None`, `depth_only()` constructor)
- [x] All public types documented with inline examples (`/// # Examples`) — 35+ types covered
- [x] Resolve architectural decisions — ADR-001 (public fields), ADR-002 (runtime alignment), ADR-003 (fixed vertex types)
- [x] BindGroupCache added for soorat phase 4 readiness (bind group dedup/invalidation)
- [x] Test coverage — 278 tests, 75.4% line coverage (up from 162 tests / 22.47%); CI gate at 70%
- [x] Benchmark suite — 20 benchmarks (CPU + GPU), history tracked in CSV
- [x] Coverage gate — `./scripts/coverage-check.sh 70` added to work loop
- [ ] Migrate soorat onto mabda (phases 0-3 done, phase 4 pending: bind group reuse, shader dedup, pipeline caching)

---

## Test Coverage Roadmap

Current: **75.4%** (278 tests, 1034/1367 lines). CI gate: **70% minimum**.

### To 80% (~63 more lines needed)

| Module | Current | Gap | What to test |
|--------|---------|-----|--------------|
| profiler.rs | 59% (72/122) | 50 lines | GpuTimestamps::read_results full path (needs TIMESTAMP_QUERY device), export_history_csv edge cases |
| typed_buffer.rs | 51% (25/49) | 24 lines | StorageBuffer::new tracing paths, UniformBuffer zero-size rejection path |
| context.rs | 72% (51/71) | 20 lines | GpuContextBuilder with surface (needs window or mock surface) |
| render_target.rs | 71% (85/119) | 34 lines | RenderTargetBuilder MSAA read_pixels path, error overflow paths |
| pipeline_cache.rs | 72% (28/39) | 11 lines | get_or_insert_render/compute with real GPU pipelines |
| color.rs | 64% (35/55) | 20 lines | Remaining constant definitions (tarpaulin false negatives on `const` items) |

### To 90% (~150 more lines needed)

Requires infrastructure changes:

- [ ] **Surface testing harness** — create a headless surface via `wgpu::Instance::create_surface_from_core` or similar for testing `SurfaceState` (configure, resize, acquire). This covers surface.rs (41 uncovered lines).
- [ ] **TIMESTAMP_QUERY test device** — request a device with `Features::TIMESTAMP_QUERY` for testing `GpuTimestamps` full lifecycle (resolve + read_results). Falls back to skip on hardware without support.
- [ ] **Pipeline cache GPU tests** — build real render/compute pipelines, insert into cache, verify dedup and invalidation with actual GPU objects.
- [ ] **Render pipeline edge cases** — test with vertex buffers, multiple bind groups, MSAA multisample state, all topology variants.
- [ ] **Texture error paths** — test zero-size rejection, dimension overflow, format mismatch in from_raw.

---

## Backlog (demand-gated)

> Only promoted when a consumer needs it

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
- [ ] DynamicUniformBuffer<T> — dynamic offset support for instanced uniforms
- [ ] Shader preprocessing (#import, #ifdef) — evaluate naga_oil integration
- [ ] Pipeline specialization by key type — SpecializedPipeline<Key> trait
- [ ] GPU memory statistics — VRAM usage, allocation counts
- [ ] Staging belt integration (wgpu's StagingBelt for streaming uploads)
- [ ] TextureArray — layered 2D textures
- [ ] Mipmap generation (compute or blit)
