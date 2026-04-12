# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 27 library modules + 4 FFI modules + 1 cache helper, ~3,700 lines, 290 assertions. 58-slot FFI function table.

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It ships now via
a C shim over wgpu-native so real projects (soorat, rasa, ranga, bijli,
aethersafta, kiran) can depend on a stable, sovereign-owned surface
immediately. The C shim is **transitional scaffolding** — it exists
exclusively to get mabda in front of consumers while the native GPU
backend is being built. When the native backend lands, consumer code
will not change by a single byte.

```
  v2.1.1  ───▶  wgpu-native via C shim     (ship API now)
       │
       │        kernel GPU driver work (in parallel, AGNOS scope)
       ▼
  v3.0    ───▶  pure Cyrius GPU backend    (backend swap — API unchanged)
```

Everything in this roadmap prioritizes **API stability** over **backend
correctness**. The public surface (context, buffer, compute, texture,
render_pipeline, etc.) is the load-bearing contract. The FFI layer
underneath is explicitly marked internal so no consumer accidentally
couples to it.

---

## v2.0.0 — Cyrius Port (Shipped)

Complete port from Rust to Cyrius. All 24 Rust modules ported. GPU FFI
operational via C launcher + struct-packing shims.

### Shipped
- All core modules: error, color, capabilities, context, profiler, resource
- All buffer/compute modules: buffer, compute, shader_cache, pipeline_cache, bind_group_cache
- All graphics modules: vertex, blend, sampler, depth, texture, bind_group, instancing
- All render modules: render_target, render_pipeline, render_pass, surface, debug
- FFI layer: wgpu_types, wgpu_descriptors, wgpu_ffi, C launcher
- 89 standalone tests + 4 GPU integration tests (including buffer round-trip)
- Buffer readback round-trip (write → copy → map → verify) — **v1.0 criterion met**
- Cyrius compiler fixes upstreamed: PIC codegen (3.4.12), mmap rename (3.4.12), _cyrius_init (3.4.14)
- GPU discovery via yukti 1.2.0
- Struct-packing shim pattern for wgpu functions with 6+ i64 args (avoids `fncall6` bug)
- Flat project layout (src/, lib/, tests/, deps/ at repo root) matching vidya/cyrius convention
- Benchmarks reference: `docs/benchmarks-rust-v-cyrius.md`

---

## v2.1.0 — Feature Catch-up (Shipped)

Rust-parity catch-up release. Every v2.1 item landed along with a batch of
v29 enum-value fixes that were latent in the v2.0 tree (silently compiled
against an older wgpu version — first noticed when texture and render
pipeline FFI actually dispatched into wgpu-native).

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | `typed_buffer.cyr` port | ✅ Shipped | 352 LOC Rust → `src/typed_buffer.cyr` + `test_typed_buffer.tcyr` (26 assertions). API refactored to a capacity-based signature to stay under the 6-parameter Cyrius ceiling. |
| 2 | Standalone `.tcyr` tests for pure-data modules | ✅ Shipped | **+191 assertions** across 6 new test files: `test_typed_buffer` (26), `test_error` (31), `test_capabilities` (34), `test_state` (blend+sampler+depth, 50), `test_caches` (26), `test_surface` (24). Far exceeded the ~125-assertion target. |
| 3 | GPU timestamp profiling (`GpuTimestamps`) | ✅ Shipped | `src/gpu_timestamps.cyr` wraps `wgpuDeviceCreateQuerySet` + resolve buffer + read buffer. Device-side feature check via `wgpuDeviceHasFeature` (not adapter-side — devices must opt in). Error path verified on a stock device. |
| 4 | Texture creation via FFI | ✅ Shipped | Function table slots 45–51: `wgpuDeviceCreateTexture`, `wgpuTextureCreateView`, `wgpuDeviceCreateSampler`, struct-packed `wgpu_shim_queue_write_texture`, three release entries. `texture.cyr` rewritten with a real FFI path + `texture_from_rgba` convenience. |
| 5 | Render pipeline FFI | ✅ Shipped | Slots 52–53. `render_pipeline_create_simple` builds the full 168-byte descriptor (VertexState + PrimitiveState + MultisampleState + FragmentState + ColorTargetState), auto-layouts, creates a real pipeline on the GPU. Legacy `rpb_*` builder delegates to the simple path. |
| 6 | Surface FFI | ✅ Shipped | Slots 54–57. `surface.cyr` wraps configure/acquire/present/release. Headless (no windowing system) so the FFI compiles + links but isn't exercised end-to-end in `test_phase0`; `test_surface.tcyr` covers descriptor building and present-mode mapping. |
| 7 | `.bcyr` benchmark harness | ✅ Shipped | `tests/mabda.bcyr` covers the 7 CPU-only Rust benchmarks. Results seeded in `bench-history.csv`. Rust's picosecond numbers identified as LLVM optimising the bodies out — Cyrius numbers are apples-to-oranges on those benchmarks, meaningful on `profiler_frame_cycle` where both measure real work. |

**Outcome:** `make test-all` green, 290 assertions, 10 GPU-backed phase-0
checks including full render-pipeline creation. Fixes a latent
cache-dangling-pointer bug that shipped in v2.0.

### Byproduct fixes (worth calling out)
- **v29 enum drift in `wgpu_types.cyr`, `sampler.cyr`, `depth.cyr`,
  `render_pipeline.cyr`.** Texture formats (RGBA8Unorm 18 → 0x16, etc),
  sampler filter/address modes (off by one), present modes (wrong order),
  primitive topology (TriangleList 3 → 4), cull mode (None 0 → 1),
  shader sType (0x07 → 0x02). All re-derived from the webgpu.h header.
- **Sampler descriptor default `maxAnisotropy=1`** — wgpu v29 rejects
  samplers with the field left at 0.
- **Cache modules' dangling stack-key bug** — all four cache modules
  (shader, pipeline, bind_group, texture) previously stored pointers to
  stack-allocated key buffers into `hashmap`. Fixed via shared
  `src/cache_key.cyr::_hash_to_heap_key`. The new `test_caches.tcyr`
  surfaced the regression.
- **Cyrius 6-param ceiling for fns that hit wgpu fncall.** Documented in
  `CLAUDE.md` and `feedback_cyrius_param_ceiling.md`. Same family of bugs
  as the v2.0 `fncall6` direct-call crash.

---

## v2.1.1 — Stdlib Inclusion Release

**Goal:** ship mabda as a Cyrius stdlib dep **now**, so every Cyrius user
gets a sovereign GPU surface they can build against. The C shim is a known
transitional detail, not a blocker. Cyrius 3.4.17 has already staged
`[deps.mabda]` pointing at tag `2.1.1`; when that tag ships,
Cyrius 3.4.18 activates the dep.

### What "stdlib inclusion" actually means here

- Consumers write code against mabda's public API (`gpu_context_*`,
  `buffer_new`, `compute_dispatch`, …) and that code is portable across
  **every** future backend — wgpu-native today, pure Cyrius tomorrow.
- The `dist/mabda.cyr` bundle compiles standalone. FFI function pointers
  are globals populated at runtime by the consumer's launcher (same pattern
  as today's `wgpu_main.c`).
- The C shim + wgpu-native dependency lives at the consumer's edge, not
  inside the language toolchain. Cyrius itself stays dependency-free.
- When v3.0 swaps the backend, the public API does not change. Consumers
  recompile against a new mabda tag and the C launcher requirement
  disappears. **Users don't even notice.**

### Inclusion checklist

Focused on shipping. Anything that's about "FFI ABI stability" is
deliberately omitted — the FFI is internal scaffolding and will be
replaced, so freezing its shape would be work spent on something we're
throwing away.

| # | Item | Rationale |
|---|------|-----------|
| 1 | **`dist/mabda.cyr` single-file bundle** | All 27 modules concatenated in `src/mabda.cyr` include order. Matches yukti/patra/sigil/sakshi bundle convention — this is how `cyrius deps` symlinks resolve. |
| 2 | **`scripts/bundle.sh`** — reproducible bundler | Produces `dist/mabda.cyr` from `src/mabda.cyr` by expanding `include` directives in place. Byte-reproducible so CI can diff-check against the committed copy. Idempotent. |
| 3 | **`[lib]` section in `cyrius.toml`** | `entry = "src/mabda.cyr"`, `modules = [...]` in dependency order. Mirrors yukti's `[lib]` block. |
| 4 | **`cyrius-version = "3.4.18"`** | Bump the minimum Cyrius to the release that activates `[deps.mabda]` as a first-class dep. |
| 5 | **`cyrius audit` clean on the public-API path** | `fmt --check`, `lint`, pure-data tests all green. `lib/` is a symlink, never vendored. |
| 6 | **`cat dist/mabda.cyr \| cc3 > /dev/null` compiles** | The bundled form must pass cc3 with zero errors. `undefined function` warnings are OK for the FFI fn-table slots (those are externals populated by the launcher) — document the expected warning list so consumers know it's benign. |
| 7 | **Public API surface marked and documented** | Every non-FFI `.cyr` in `src/` gets a `# @public` or `# @internal` comment at the top of the file. Consumer docs list only the `@public` surface. The FFI modules (`wgpu_types`, `wgpu_descriptors`, `wgpu_ffi`, `tagged_obj`, anything touching `fncall_N`) are all `@internal`. This is the contract that survives the backend swap. |
| 8 | **Consumer example under `examples/stdlib-consumer/`** | Minimal "hello GPU" that declares `[deps.mabda]` in its own `cyrius.toml`, runs `cyrius deps`, and compiles against the symlinked bundle. Proves the integration works without in-tree hacks. Also serves as the v3.0 regression test — the same example must still compile after the backend swap. |
| 9 | **`docs/stdlib-integration.md`** — consumer guide | C launcher template, GPU pre-init snippet (`instance → adapter → device → queue`), how to build `wgpu_main.c`, how to call `mabda_main(fn_table, preinit)`. Clearly labeled "transitional — requirements go away in v3.0." |
| 10 | **Vendored `lib/*.cyr` cleanup** | Delete the 45 regular `.cyr` files in `lib/` and replace with a single symlink to `$HOME/.cyrius/lib/`. Mabda tracks the Cyrius toolchain, never vendors. |
| 11 | **Sakshi dep refresh** | Verify `sakshi = "0.9.0"` still resolves via `cyrius deps` after the flat layout. |
| 12 | **Release notes drafted** | `CHANGELOG.md` v2.1.1 entry: the bundle, stdlib-inclusion contract, `cyrius-version` bump, and a prominent "backend is transitional — v3.0 replaces it with native Cyrius, public API unchanged" callout. |
| 13 | **`scripts/version-check.sh`** | Fails `make test-all` if `VERSION`, `cyrius.toml`, `CHANGELOG.md`, and `README.md` disagree on the version number. Prevents future drift. |

**v2.1.1 exit criteria:**
- `dist/mabda.cyr` byte-identical when regenerated from `src/`
- `cat dist/mabda.cyr | cc3 > /dev/null` exits 0
- Every `src/*.cyr` has a `# @public` or `# @internal` header comment
- `cyrius audit` reports 0 warnings, all tests pass
- The consumer example project builds end-to-end without any mabda-tree files
- Cyrius 3.4.18 lands immediately after — flips `[deps.mabda]` from
  "staged" to "active" and adds `lib/mabda.cyr` symlink

---

## Future — Native GPU Backend (direction, unscoped)

The wgpu-native + C shim path is transitional. The long-term direction is
a pure Cyrius GPU backend (DRM/KMS today, AGNOS kernel driver eventually)
so Cyrius projects depend on zero C artifacts for GPU work. **Scope is
deliberately left open** — this will be tackled when there's appetite for
it, not front-loaded as a commitment.

**The one invariant that matters:** the public mabda API (`# @public` files
from v2.1.1) does not change when the backend swaps. Consumers bump their
`[deps.mabda]` tag and their C launcher requirement disappears. The v2.1.1
`examples/stdlib-consumer/` project is the regression test — if it still
compiles after the swap, the contract held.

Everything else — which vendor first, what the shader lowering looks like,
whether to take a compute-only waypoint — is a call for a later version
when the work actually starts. Not spec'd here.

---

## Backlog (demand-gated, orthogonal to the backend swap)

| Feature | Status | Notes |
|---------|--------|-------|
| Render graph (DAG pass orchestration) | Not started | After render pipeline FFI works (v2.1.0 item #5) |
| Multi-queue coordination | Not started | Additive to GpuContext |
| Compressed textures (BC/ETC2/ASTC) | Not started | Needs texture FFI (v2.1.0 item #4) |
| SPIR-V shader support | Not started | Currently WGSL only |
| Mipmap generation | Not started | Needs texture FFI |
| Image loading (PNG/JPEG) | Not started | Needs C image library (stb_image) |
| WebGPU/WASM target | Not started | Needs Cyrius WASM backend |

---

## v1.0.0 Criteria (met in Rust, porting to Cyrius)

- [x] All 25 modules ported
- [x] GPU context creation working
- [x] Buffer create/write/release working
- [ ] Compute dispatch end-to-end test
- [ ] Render pipeline end-to-end test
- [ ] Consumer integration (soorat port to Cyrius)

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (load64 at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — **transitional**, replaced in v3.0 |
| 005 (planned, v2.1.1) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that survives the backend swap |
| 006 (planned, v3.0) | Pure Cyrius GPU backend via DRM/KMS — supersedes ADR 004 |
