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

## v1.0 Remaining

- [ ] 80%+ test coverage (162 tests exist, needs measurement)
- [ ] All public types documented with inline examples (`/// # Examples`)
- [ ] At least one consumer (soorat) fully migrated to mabda pipelines

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
