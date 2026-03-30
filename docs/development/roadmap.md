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

### GpuContext public fields vs accessor methods

`GpuContext` exposes `pub instance`, `pub adapter`, `pub device`, `pub queue` as raw wgpu types. This lets consumers bypass mabda entirely and call wgpu directly.

**Tradeoff:**
- Public fields: maximum flexibility, zero-cost access, consumers own their wgpu usage
- Accessor methods: mabda controls access, can add resource tracking, newtype wrappers, or multi-queue support later without breaking API

**Decision needed:** If any future backlog item (multi-queue, resource tracking, GPU memory stats) would require wrapping these types, the public fields must change to methods before v1. Evaluate during soorat migration — if soorat accesses these fields directly in many places, the cost of switching is high. If only a few call sites, switch now.

### Typed buffer alignment strategy

`UniformBuffer<T>` validates `size_of::<T>() % 16 == 0` at runtime. This catches misaligned types but only at creation time, not compile time. The alternative is `encase` (compile-time std140 derive) but adds a dependency.

**Decision needed:** If soorat migration reveals frequent alignment issues (vec3 padding, nested structs), evaluate adding `encase` as an optional dependency behind a feature gate.

### Vertex type extensibility

Fixed vertex types (`Vertex2D`, `Vertex3D`, `SkinnedVertex3D`) cover common cases. The `VertexLayout` trait exists for custom types, but consumers must hand-write `wgpu::VertexBufferLayout` with manual offset calculations.

**Decision needed:** If soorat needs additional vertex formats beyond the three provided, consider a derive macro or builder for vertex layout generation.

---

## v1.0 Remaining

- [ ] Migrate soorat onto mabda (local dependency, build, test, fix API friction)
- [ ] Resolve architectural decisions above based on migration findings
- [ ] 80%+ test coverage (162 tests exist, needs measurement)
- [ ] All public types documented with inline examples (`/// # Examples`)

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
