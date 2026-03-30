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
- [x] Test coverage push — 257 tests, 69.02% line coverage (up from 162 tests / 22.47%)
- [x] Benchmark suite — 20 benchmarks (CPU + GPU), history tracked in CSV
- [ ] Migrate soorat onto mabda (phases 0-3 done, phase 4 pending: bind group reuse, shader dedup, pipeline caching)
- [ ] Push test coverage toward 80%+ (remaining gap: surface lifecycle — requires real window)

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
