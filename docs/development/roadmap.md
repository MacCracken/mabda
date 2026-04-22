# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 30 domain modules, ~4,500 lines, 387 CPU assertions + 4 GPU
> integration programs + 13 GPU benchmarks. `dist/mabda.cyr`
> bundle at ~4,900 lines.

This document is **forward-looking**. For detail on every shipped
release, see [`CHANGELOG.md`](../../CHANGELOG.md) — that is the
source of truth for completed work. Shipped-section details were
pruned 2026-04-21 when the v2.x line closed so this file stays
useful for planning instead of bloating with history.

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It ships
now via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) can depend on a stable,
sovereign-owned surface immediately. In v3.0 a **pure Cyrius DRM/KMS
backend is added alongside** the C launcher path — both coexist,
selectable per consumer, so the same bench suite exercises both. This
gives us a clean A/B across two axes: **(C hooks vs native Cyrius) ×
(pre-5.6.x vs post-5.6.x compiler optimizations)**. The public API
does not change when the native backend lands; consumer code stays
byte-identical.

```
  v2.0.0 → v2.3.0  ─▶  Cyrius port + Rust-v1 parity + P(-1) audit      (shipped)
  v2.4.0 → v2.4.5  ─▶  v1.0 parity completion + FFI validation +
                        benchmark parity + cache hot-path unblock      (shipped)
  v2.5.0           ─▶  render graph (DAG pass orchestration)           (shipped)
       │
       │                   kernel GPU driver work (parallel, AGNOS scope)
       ▼
  v2.5.x           ─▶  render graph follow-ups (out-of-order toposort,
                        aliasing pass) — driven by consumer demand
  v3.0             ─▶  dual-backend — native Cyrius DRM/KMS added
                        alongside wgpu+C; API unchanged; A/B bench matrix
  v3.1             ─▶  multi-queue + mipmaps (consumer catch-up)
  v3.2             ─▶  compressed textures + SPIR-V (texture/shader breadth)
  v3.3             ─▶  image loading (gated on pure-Cyrius decoder)
  v3.x+            ─▶  WebGPU / WASM (blocked on Cyrius WASM backend)
  v4.0             ─▶  C launcher + wgpu-native retired;
                        native Cyrius backend is the only backend
```

Across the v3.x line the C launcher stays in-tree as a permanent
second backend. It retires in **v4.0** — only after every consumer
has been running native in production across a full release cycle.
Retirement is consumer-driven, not calendar-driven.

Everything in this roadmap prioritizes **API stability** over
**backend correctness**. The public surface (context, buffer, compute,
texture, render_pipeline, render_pass, render_graph, etc.) is the
load-bearing contract. The FFI layer underneath is explicitly marked
internal (`@internal` header on line 1 of every FFI module) so no
consumer accidentally couples to it.

---

## Shipped — one-line history

Source of truth: [`CHANGELOG.md`](../../CHANGELOG.md). Use that for
detail; this table is a jump list.

| Release | Theme |
|---------|-------|
| [2.5.0](../../CHANGELOG.md#250--2026-04-21) | Render graph — DAG pass orchestration, single encoder + single submit, 44 new regression assertions |
| [2.4.5](../../CHANGELOG.md#245--2026-04-21) | Cache hot-path unblock — u64-keyed hashmap migration (cyrius v5.5.20); `bind_group_cache_hit` reaches Rust parity |
| [2.4.4](../../CHANGELOG.md#244--2026-04-21) | Benchmark parity — 13 GPU benches ported, `depth_texture_new` / `rtb_build` latent stubs fixed |
| [2.4.3](../../CHANGELOG.md#243--2026-04-20) | Render-pass FFI + render E2E (closes v1.0 checklist) |
| [2.4.2](../../CHANGELOG.md#242--2026-04-20) | GPU runtime validation — FFI offset / enum sweep; compute E2E actually runs on GPU |
| [2.4.1](../../CHANGELOG.md#241--2026-04-19) | Sakshi observability — opt-in structured logging and spans |
| [2.4.0](../../CHANGELOG.md#240--2026-04-19) | v1.0 parity (partial) — compute E2E program, LOW audit sweep |
| [2.3.0](../../CHANGELOG.md#230--2026-04-19) | P(-1) security audit — 0 CRITICAL / 2 HIGH / 6 MED / 6 LOW across 29 modules |
| [2.2.0](../../CHANGELOG.md#220--2026-04-19) | Scaffolding refresh — flat layout, `cyrius.cyml`, `cyrius distlib` |
| [2.1.2](../../CHANGELOG.md#212--2026-04-12) | Rust-source removal hygiene release |
| [2.1.1](../../CHANGELOG.md#211--2026-04-12) | Stdlib inclusion — `dist/mabda.cyr` single-file bundle |
| [2.1.0](../../CHANGELOG.md#210--2026-04-12) | Feature catch-up — `typed_buffer`, GPU timestamps, texture / render pipeline FFI |
| [2.0.0](../../CHANGELOG.md#200--2026-04-11) | Cyrius port — complete Rust → Cyrius port of every module |
| [1.0.0](../../CHANGELOG.md#100--2026-04-09) | Last Rust release, frozen as the Cyrius-port reference |

---

## v2.5.x — Render graph follow-ups (not yet scheduled)

Everything below is in the out-of-scope list in `docs/guides/render-graph.md`.
None of it is blocking a consumer today; each item ships when a specific
consumer request arrives. Listed so they don't get lost.

- **Full out-of-order toposort.** Today's Kahn implementation only
  counts edges in insertion-order direction; that validates users
  who build graphs in a correct linear order. Programmatic consumers
  that build graphs with cross-inserted dependencies need multi-
  version read/write tracking.
- **Resource aliasing pass.** The `rg_aliasing(g, 1)` flag and the
  `first_use` field on `TransientResource` are already scaffolded.
  Ship the alias analysis that walks transients and reuses backing
  storage between disjoint lifetimes — relevant for consumers with
  dozens of transient buffers per frame.
- **Per-node debug scopes.** Wrap each node's encoding in
  `debug_push(node.label)` / `debug_pop` so GPU profilers
  (RenderDoc, PIX, RGP) show the graph structure.
- **Graph visualizer.** Export a DOT file or similar from the
  toposorted node/edge list. Consumer-requested; not core.

---

## v3.0 — Dual Backend (Native Cyrius Added Alongside C Path)

v3.0 **adds** a pure Cyrius GPU backend (DRM/KMS on Linux first, AGNOS
kernel driver eventually) **alongside** the existing wgpu-native + C
launcher path. Both coexist; neither is retired. Consumers pick per
build, the bench suite runs both, and the resulting matrix is the
evidence base for future cutover decisions.

**Why dual rather than swap.** The C-hooked path is our measurement
baseline. Keeping it in-tree lets us quantify: FFI overhead per call,
native-backend regressions, pre-vs-post-5.6.x O-pass attribution (C
path isolates the cyrius codegen changes from backend architecture
changes), and consumer-by-consumer cutover timing. Retiring the C
path before that data existed would destroy the comparison.

**The one invariant that matters:** the public mabda API (`# @public`
files from v2.1.1) does not change. Consumers who don't opt in to the
native backend see zero behavioural change; consumers who do flip a
single build flag. The `examples/stdlib-consumer/` project is the
regression test — if it still compiles against both backends, the
contract held. The v2.5 render graph must replay unchanged on both.

### Scope (high level, refined closer to the work)
- **Backend abstraction layer** — extract the wgpu-handle dispatch
  points behind an internal `Backend` interface (context/buffer/
  compute/texture/render-pipeline/render-pass entry points). The
  public API stays untouched; the indirection lives under `@internal`.
- **DRM/KMS backend in pure Cyrius** — no libdrm, no libwayland.
- **WGSL → hardware ISA lowering path** — vendor-first decision
  deferred; spike during v3.0 design.
- **Backend selector** — `wgpu` (C launcher, default, unchanged for
  existing consumers) vs. `native` (new, opt-in). Probably a
  `cyrius.cyml` flag or a build-time constant; finalized in ADR 006.
- **Dual-backend bench harness** — `make bench-gpu` runs the
  13-bench suite under each backend, emits CSV columns for both,
  `bench-history.csv` grows a `backend` column.
- **ADR 006** — pure Cyrius GPU backend via DRM/KMS. **Supplements**
  ADR 004 rather than superseding it; both backends are now
  architecturally load-bearing. Filed when scope concretizes.

### Exit criteria
- `dist/mabda.cyr` ships with both backends compiled in; default is
  still `wgpu` for API-stability reasons.
- `examples/stdlib-consumer/`, `programs/phase0.cyr`,
  `programs/compute_e2e.cyr`, `programs/render_e2e.cyr`,
  `programs/render_graph_e2e.cyr` all pass under both backends.
- All 387 CPU assertions + 13 GPU benches pass under both backends
  (CPU assertions are backend-agnostic and should be untouched).
- soorat / rasa / ranga / bijli / aethersafta / kiran continue to
  build and run under the `wgpu` default; at least one consumer runs
  a CI matrix entry under `native` to prove the path.
- Bench matrix published: 13 benches × {wgpu, native} × {pre-5.6.x,
  post-5.6.x} — four columns per bench in `bench-history.csv`, with
  `docs/benchmarks-rust-v-cyrius.md` refreshed to tell the story.

---

## v3.1 — Multi-queue + Mipmaps (Consumer Catch-up)

Both items were backlog'd against v1.0 / v2.x — 3.1 is their natural
home once the v3.0 backend abstraction is in place (both backends
implement the new surface).

- **Multi-queue coordination** — additive to `GpuContext`. Separate
  compute / graphics / transfer queues with explicit submit ordering.
  Consumers needing it: rasa (compute + present overlap), bijli
  (large transfer workloads). Implemented on both backends.
- **Mipmap generation** — on-device chain generation via compute
  shader. Builds on the existing texture surface. Consumers:
  soorat (texture-heavy UI), kiran (asset pipeline). Implemented on
  both backends.

---

## v3.2 — Texture & Shader Breadth

- **Compressed textures (BC / ETC2 / ASTC)** — format enums,
  validation, upload path. Builds on the texture FFI (now native).
  Driven by kiran and aethersafta's asset sizes.
- **SPIR-V shader support** — alternative path to WGSL. Shader
  cache keyed on source kind. Reuses `shader_cache.cyr`.

---

## v3.3 — Asset Loading

- **Image loading (PNG / JPEG)** — adopted only once a pure-Cyrius
  decoder exists in the AGNOS ecosystem. **Dependency-gated**: if
  no decoder ships by the 3.3 window, this moves to 3.4+. No C
  image library will be vendored to force the issue.

---

## v3.x+ — Web Target (Blocked)

- **WebGPU / WASM target** — blocked on the Cyrius WASM backend
  landing in the compiler. No version commitment until that
  prerequisite exists. Tracked here so it doesn't get forgotten.

---

## Backlog (not yet scheduled)

Empty. New items land here first and graduate to a version once
there's consumer demand plus a clear scope.

---

## v1.0.0 Criteria (Rust v1.0 feature parity in Cyrius)

All mabda-side criteria complete as of v2.4.3. The last open item is
consumer-side.

- [x] All core modules ported (30 as of v2.5.0 — v1.0 had 25).
- [x] GPU context creation working.
- [x] Buffer create / write / release working.
- [x] Buffer readback round-trip (write → copy → map → verify) —
      `programs/phase0.cyr`.
- [x] Texture create / view / upload / release — `programs/phase0.cyr`.
- [x] Render pipeline create / release — `programs/phase0.cyr`.
- [x] Compute dispatch end-to-end test — `programs/compute_e2e.cyr`
      (v2.4.0 wrote; v2.4.2 got it running on real hardware).
- [x] Render pipeline end-to-end draw + readback —
      `programs/render_e2e.cyr` (v2.4.3).
- [ ] Consumer integration: soorat port to Cyrius (consumer-side,
      tracked in soorat's repo).

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (`load64` at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — permanent, coexists with native backend from v3.0 onward |
| [005](../adr/005-public-api-surface-marking.md) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that holds across backends |
| [006](../adr/006-native-cyrius-gpu-backend.md) | Native Cyrius GPU backend via DRM/KMS — adds a second backend alongside ADR 004 during v3.x; wgpu path retires at v4.0 (Proposed, v3.0 design) |
