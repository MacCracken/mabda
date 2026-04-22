# v3 Native GPU Backend — Design Principles

**Status:** Draft (v3 branch, 2026-04-21)
**Related:** [ADR 006](../adr/006-native-cyrius-gpu-backend.md) (dual-backend decision), [ADR 005](../adr/005-public-api-surface-marking.md) (@public stability boundary), [ADR 004](../adr/004-c-launcher-ffi.md) (wgpu path, v3.x era)

## Framing

AGNOS's organizing principle, as stated during v3 kickoff:

> "Leave the past in the past and forge the future from now as the
> demark. Learn from [20+ years of GPU API evolution] and do it
> better — either from a different angle or by simplifying through
> language usage."

mabda's native backend is not "reimplement wgpu-native in Cyrius."
It is a fresh design that extracts lessons from every major GPU API
shipped since 2015 — Vulkan, D3D12, Metal, WebGPU, plus the engine
frameworks built on top (Frostbite FrameGraph, UE5 RDG, Unity SRP,
Granite, bgfx) — and commits to the simplest accurate expression of
the essential ideas in Cyrius idioms.

Four portable-knowledge bodies ground this work. They live in vidya
(`../vidya/content/`) and document what the industry learned. This
document records the **mabda-specific decisions** built on top:

- [render_graph_architecture](../../../vidya/content/render_graph_architecture/concept.toml) —
  DAG-based orchestration, barrier derivation, transient aliasing.
- [bindless_resources](../../../vidya/content/bindless_resources/concept.toml) —
  descriptors as GPU-memory tables, indexed by u32 handles.
- [explicit_gpu_synchronization](../../../vidya/content/explicit_gpu_synchronization/concept.toml) —
  graph-derived barriers, timeline semaphores, frame counters.
- [gpu_memory_pooling](../../../vidya/content/gpu_memory_pooling/concept.toml) —
  bump/ring/TLSF allocators, BAR upload, graph-informed aliasing.

Each vidya topic carries the portable lessons with source citations.
This document is opinionated about mabda's specific shape.

---

## The Seven Principles

### 1. Graph-first submission

The render graph is the **primary submission path**, not a helper
on top of a leaf API. Every consumer program — from `phase0.cyr`
smoke tests up to soorat's full compositor — submits work by building
a graph and calling `render_graph_run(g)`. Leaf ops (`draw`,
`dispatch`, `copy`) exist and are `@public`, but they're for simple
cases (a one-shot compute, a clear-and-blit debug frame). Graph is
not an optional abstraction.

**Why**: per
[vidya/render_graph_architecture](../../../vidya/content/render_graph_architecture/concept.toml),
every post-2015 engine converges on a graph layer because leaf APIs
can't schedule resource state with local-only information. Mabda
already has `src/render_graph.cyr` at v2.5.0 — we harden and extend
rather than design from scratch. The native backend assumes the
graph is there and optimizes aggressively against it.

**mabda opinions**:
- `render_graph_run(g)` is the canonical entry point. Documentation
  leads with it. Tests lead with it. The single-op shortcuts
  (`gpu_dispatch`, `gpu_draw`) delegate internally by building and
  running a one-node graph.
- Pass kinds are a tagged union (`lib/tagged.cyr`): `Compute{...}`,
  `Render{...}`, `Copy{...}`, `Present{...}`. Not virtual dispatch,
  not function-pointer tables with 65 slots.
- The graph's compile phase runs every frame and is cheap:
  single-digit microseconds at hundreds of nodes (Granite numbers,
  see vidya). No memoization in v3.0; revisit if a consumer profiles
  it as a bottleneck.
- Per-node debug scopes (`v2.5.x` follow-up in the existing roadmap)
  become table stakes on the native path — the backend's RenderDoc /
  PIX replacement emits them automatically.

### 2. Bindless-only, single resource table

No bind groups. No descriptor sets. No descriptor set layouts. No
descriptor update templates. One global resource table per frame,
indexed by typed u32 handles. Shaders read `resources[id]`.

**Why**: per
[vidya/bindless_resources](../../../vidya/content/bindless_resources/concept.toml),
the pool/set/layout/update-template trinity exists entirely for
backwards compatibility with pre-bindless hardware. D3D12 collapsed
it to one concept (the heap) in 2015; SM 6.6 collapsed it to zero
root-signature entries; Vulkan's `VK_EXT_descriptor_buffer` (2023)
is Khronos catching up. Ten years of converging API evolution —
all pointing at one primitive.

**mabda opinions**:
- **Hardware floor**: Vulkan 1.2 with `VK_EXT_descriptor_indexing`,
  or equivalent (`VK_EXT_descriptor_buffer` when universally
  available). DRM/KMS targets with AMDGPU, i915, nouveau, and
  recent vendor drivers qualify. Older hardware uses the wgpu path
  (ADR 004).
- `TextureHandle`, `BufferHandle`, `SamplerHandle` are tagged u32s:
  generation counter in the top bits, resource type tag in the
  middle, index in the bottom bits. Shader sees the raw u32; Cyrius
  sees the distinct types.
- Two tables only (hardware floor): resources (buffers + textures +
  storage images) and samplers. No further fragmentation.
- Deferred free by max-frames-in-flight + 1 ring. Generation counter
  makes use-after-free a detectable error, not a GPU hang.
- The native backend never ships the word "descriptor set." It is
  not in the `@public` API, it is not in the `@internal` FFI, it is
  not in the source tree. Legacy ceremony, dropped.

### 3. Barriers derived, timeline as escape hatch

The render graph emits every barrier. Hand-written barriers are not
in the `@public` API. For the ~5% of cases the graph can't express
(dynamic feedback loops, CPU-read-after-GPU-write with variable
cadence), exactly one escape hatch: a timeline primitive.

**Why**: per
[vidya/explicit_gpu_synchronization](../../../vidya/content/explicit_gpu_synchronization/concept.toml),
Frostbite/UE5 RDG/Granite all ship production with graph-derived
barriers covering essentially all application code. The escape
hatch needs to exist (for edge cases) but it shouldn't be the
primary interface — apps that write raw barriers have incomplete
declarations or a bug.

**mabda opinions**:
- Single-queue apps (phase0, compute_e2e, render_e2e, render_graph_e2e,
  all v2.x integration programs) use a **monotonic frame counter**,
  not timelines. `gpu_wait_frame(n)` + `gpu_current_frame()`. One
  primitive. No semaphore object, no value argument beyond the
  frame number.
- Multi-queue apps (rasa's compute/present overlap, bijli's large
  transfers) graduate to timelines. Exposed as
  `timeline_new()` / `timeline_wait(t, value)` /
  `timeline_signal(t, value)`. Tagged union again — don't build
  separate fence / binary semaphore / event types.
- Barrier emission is `@internal` — the backend's responsibility.
  The public surface declares `gpu_pass_reads(g, pass, resource)`
  and `gpu_pass_writes(g, pass, resource)`; the backend derives
  every transition. **No** `gpu_barrier(...)` in `@public`.
- Stage enum, not bitmask. `GPU_STAGE_COMPUTE`,
  `GPU_STAGE_VERTEX`, `GPU_STAGE_FRAGMENT`,
  `GPU_STAGE_COLOR_OUT`, `GPU_STAGE_DEPTH_OUT`,
  `GPU_STAGE_TRANSFER`, `GPU_STAGE_HOST`. Seven stages, not thirty.
- Access inferred from resource use, not independently specified.
  If a pass declares a read from a storage buffer, the backend
  knows it's a `SHADER_READ`; the app doesn't say so.

### 4. Three allocators, three memory tiers

- **Bump** (`gpu_transient_*`) — per-frame transient resources.
  Reset on frame boundary. Backs the render graph's transient
  resource set.
- **Ring** (`gpu_upload_*`) — CPU→GPU staging. Head advances on
  upload, tail advances when the GPU finishes reading (tied to
  frame counter).
- **TLSF block pool** (`gpu_persistent_*`) — long-lived buffers and
  textures. One DRM BO per 128 MB block; TLSF sub-allocates.

Three memory tiers:
- **DEVICE** — VRAM only, fastest GPU access. Default for persistent.
- **UPLOAD** — HOST_VISIBLE, CPU writes. Default for staging ring.
- **BAR** — DEVICE_LOCAL + HOST_VISIBLE when ReBAR is present.
  CPU writes directly to VRAM. Auto-selected over UPLOAD for
  upload-heavy workloads when available.

**Why**: per
[vidya/gpu_memory_pooling](../../../vidya/content/gpu_memory_pooling/concept.toml),
these three allocators and three tiers cover every documented
real-world win. VMA is ~20K lines; the essential subset is ~2K.
Mabda's native allocator aims for the ~2K figure.

**mabda opinions**:
- **No defragmentation in v3.0**. Shipping AAA titles size pools to
  avoid fragmentation; mabda will too. Revisit only if a consumer
  surfaces a real fragmentation bug in production.
- **Render-graph-informed transient aliasing is the main win** —
  `src/render_graph.cyr` already has `rg_aliasing(g, 1)` and
  `first_use` scaffolding from v2.5.0. The v3 native backend
  implements the alias analysis against the bump allocator's
  underlying block. Frostbite saw ~50% render-target memory
  reclaimed through this; it's the single largest memory
  optimization available.
- **BAR auto-detection**. At `gpu_context_init`, probe for
  resizable-BAR memory types. If present, upload paths default to
  BAR. If not, fall back to UPLOAD (staging-copy). Consumer code
  doesn't branch — allocator choice is internal.
- **No general-purpose allocator API**. Consumers don't call
  `gpu_alloc(size, tier)`. They create typed resources
  (`gpu_buffer_persistent(size)`, `gpu_buffer_upload(size)`,
  `gpu_buffer_transient(size, g)`) and the allocator is selected
  by the function name.

### 5. Express in Cyrius idioms, not C-API translations

**Why**: per
[feedback_agnos_forge_future](../../../.claude/projects/-home-macro-Repos-mabda/memory/feedback_agnos_forge_future.md),
mabda can simplify by leveraging what Cyrius gives: tagged unions,
first-class function pointers, manual alloc with fixed offsets,
direct syscall access, no template bloat, no vtables, no class
hierarchies. A Cyrius-idiomatic API typically needs 30-50% fewer
types than a C-shaped one.

**mabda opinions**:
- **Tagged unions** for every sum type. `PassKind`, `ResourceKind`,
  `BarrierKind` (internal), `GpuErr`. Not enum-plus-union pairs,
  not virtual dispatch.
- **Fixed-offset structs** documented in each module's header
  comment, matching the v2.x pattern. Readers can see the byte
  layout without indirection.
- **fnptr for the encode callback** on each pass. Not a type-erased
  `void*` callable — a direct function pointer with a known
  signature. Cyrius's fncall1/2/3/5/7/8 cover every arity we need.
- **u32 everywhere** for handles. Not 64-bit pointers masquerading
  as handles. 32 bits covers 16M resources per table, which is
  more than any real workload.
- **Descriptors as raw bytes**, matching `VK_EXT_descriptor_buffer`
  shape. Write the descriptor fields directly with `store64` /
  `store32`. No builder pattern, no struct-of-structs, no RAII
  helper. The descriptor format is a documented byte layout.
- **`alloc()` is explicit and visible**. No hidden allocations in
  hot paths. Per-frame transients come from the bump allocator,
  not `alloc()`.

### 6. Stdlib-tier ergonomics

mabda is a Cyrius stdlib library (promoted at cyrius 5.4.7+, per
`feedback_stdlib_promoted_agnos_libs`). The API reads like a
stdlib facility, not a third-party wrapper over wgpu-native.

**mabda opinions**:
- **Direct verbs** for ops. `dispatch`, `draw`, `submit`, `wait`.
  Not `gpu_command_encoder_begin_compute_pass`,
  `gpu_compute_pass_encoder_dispatch_workgroups`.
- **Short names**. `fb` for framebuffer, `tex` for texture, `buf`
  for buffer. Matches the 2-3 character abbreviations that
  Cyrius's stdlib (`vec`, `str`, `fmt`, `io`) has converged on.
- **One constructor per resource kind**, not N variants. Instead
  of `create_storage_buffer`, `create_uniform_buffer`,
  `create_vertex_buffer`, `create_index_buffer`, ship
  `gpu_buffer_new(usage, size, tier)` with `GPU_BUF_STORAGE`,
  `GPU_BUF_UNIFORM`, `GPU_BUF_VERTEX`, `GPU_BUF_INDEX` as usage
  flags. 7 public functions become 1.
- **Result types for fallible ops**, value returns for
  infallible. Tagged union `Result` from `lib/tagged.cyr` is
  already in use. Fallible ops return `Result<Handle, GpuErr>`;
  ops that can't fail (handle accessors, layout queries) return
  the value directly.

### 7. Observability is first-class

`sakshi` integration exists today (v2.4.1). On the native
backend it's not optional — graph structure, barrier emission,
allocator stats, timeline values, ReBAR detection, BO cache
hit-rate all emit sakshi events at info/debug levels.

**mabda opinions**:
- `mabda_observability_enable()` (v2.4.1 API) stays. Off by
  default; one call turns on structured events.
- Every graph compile emits one `info` event
  (`render_graph.compile`) with pass count, barrier count, and
  compile wall-time. Every frame emits the same with execution
  wall-time.
- Allocator events: BO create/destroy, BAR detection,
  fragmentation high-water mark. `debug` level by default;
  consumers surface them in ad-hoc debugging.
- **No GPU debug markers in `@public`**. The backend emits them
  per-node automatically under graph execution (v2.5.x
  follow-up). Manual markers are fine internally but not an API
  for consumers.

---

## API Sketch (pre-commit, illustrative)

The shape the seven principles lead to. Not final, not exhaustive —
enough to show the API is small.

```cyrius
# Context (setup — one call)
fn gpu_context_new() { ... }        # Returns Result<ctx, GpuErr>
fn gpu_context_release(ctx) { ... }

# Resources (one constructor per kind, tier-aware)
fn gpu_buffer_new(ctx, usage, size, tier) { ... }      # → BufferHandle
fn gpu_buffer_transient(ctx, usage, size, g) { ... }   # → BufferHandle (graph-managed)
fn gpu_texture_new(ctx, format, w, h, usage, tier) { ... }
fn gpu_sampler_new(ctx, filter, address) { ... }

# Render graph (the primary API)
fn render_graph_new() { ... }
fn rg_pass_compute(g, label, shader, wg_x, wg_y, wg_z) { ... }   # → PassHandle
fn rg_pass_render(g, label, color_attachments, depth) { ... }
fn rg_pass_reads(g, pass, resource) { ... }
fn rg_pass_writes(g, pass, resource) { ... }
fn rg_pass_encode(g, pass, fnptr) { ... }
fn render_graph_run(ctx, g) { ... }   # Compiles + executes + submits

# Sync (escape hatch only)
fn gpu_current_frame(ctx) { ... }         # u64 monotonic
fn gpu_wait_frame(ctx, n) { ... }         # Block until GPU finished frame n

# Multi-queue (opt-in; timeline primitive)
fn timeline_new(ctx) { ... }              # Only when multi-queue matters
fn timeline_wait(t, value) { ... }
fn timeline_signal(t, value) { ... }

# Leaf ops (simple cases; no graph needed)
fn gpu_dispatch(ctx, shader, wg_x, wg_y, wg_z, resources) { ... }
fn gpu_draw(ctx, pipeline, vertex_count, resources, target) { ... }
fn gpu_copy(ctx, src, dst, size) { ... }
```

Approximate public-surface size: **~25 functions**. Compare to the
wgpu path's ~65 FFI entries + the ~40 public helpers in today's
`buffer.cyr` / `compute.cyr` / `texture.cyr` / `render_*.cyr`.
Collapsing usage flags into one constructor per resource kind is
the single biggest reduction.

---

## Benchmarks that prove each principle

Each principle gets at least one benchmark that would regress if
the principle is violated. These land in `tests/bcyr/mabda.bcyr`
(CPU-side) and `programs/benchmarks.cyr` (GPU-side) on the v3
branch.

| Principle | Benchmark | Target |
|-----------|-----------|--------|
| Graph-first | `render_graph_compile_100_nodes` | < 10 µs (Granite parity) |
| Graph-first | `render_graph_execute_deferred_scene` | < native wgpu by 10%+ post-5.6.x |
| Bindless-only | `draw_calls_per_frame` (indirect multi-draw) | 100k+ at 60 FPS |
| Bindless-only | `descriptor_update_throughput` | 2× set-based (Granite parity) |
| Barriers derived | `barrier_count_vs_naive` | 40% fewer than hand-placed (Frostbite parity) |
| Allocators | `transient_aliasing_memory_win` | 50% reclaim on render-graph-e2e frame (Frostbite parity) |
| Allocators | `upload_throughput_bar_vs_staging` | 1.3× on ReBAR hardware |
| Cyrius idioms | `dist/mabda_native.cyr LOC` | < 6000 lines (vs 20K+ VMA alone in C++) |
| Stdlib ergo | `public_fn_count` | ≤ 35 (vs ~100 on wgpu path) |
| Observability | `sakshi_event_overhead_when_enabled` | < 5% of frame time |

Each benchmark has a reference row in
`docs/benchmarks-rust-v-cyrius.md` once the native path runs.

---

## Out of scope for v3.0

- **Defragmentation.** Re-evaluate if a consumer surfaces a real
  fragmentation bug.
- **Mesh shaders / amplification shaders.** Follow-up once the
  core is stable. Newer hardware only; not a portability target.
- **Ray tracing.** AS-build sync is still unsettled in the API
  ecosystem; circle back when it is.
- **Mobile / tiler-specific optimizations.** Mali/Adreno have
  subtle bindless semantics — AGNOS target hardware is desktop
  discrete + Apple Silicon tier initially.
- **Multi-GPU.** One device per context. Multi-device is its own
  follow-up.
- **Unified memory fast-path for iGPUs.** iGPUs can skip staging;
  mabda's v3.0 allocator treats them as small DEVICE + BAR.
  Revisit if consumer perf demands it.
- **Pipeline caching across boots.** v2.4.5 ships in-process
  shader_cache/pipeline_cache. Filesystem-persistent caching is
  a v3.x follow-up.

---

## Stability contract

ADR 005's `@public` surface from v2.1.1 onward must still compile
under the native backend. That's the contract we're validating.
The contract does NOT say "the @public surface never grows" — it's
allowed to gain new functions, new resource kinds, new
capabilities. It says the existing v2.1.1 names keep working with
the same semantics.

Where principles here suggest API simplifications (e.g., collapsing
seven `create_*_buffer` functions into one `gpu_buffer_new`), the
v2.1.1 signatures stay as thin wrappers over the new form during
v3.x, and retire in v4.0 alongside the wgpu path. The simplification
is additive during v3.x and subtractive at v4.0.

---

## Open questions deferred to v3.0 implementation

- **WGSL vs native ISA.** WGSL → SPIR-V → vendor native (via Mesa's
  nir stack or direct vendor toolchains) vs WGSL → vendor native
  direct? Probably SPIR-V first (existing ecosystem) with a
  vendor-direct path as a v3.x exploration.
- **Selector mechanism.** `cyrius.cyml` flag? Compile-time constant
  in `src/lib.cyr`? Both are implementable. Lean towards the
  `cyrius.cyml` flag for consumer ergonomics.
- **DRM/KMS surface-present path.** How does surface acquire/present
  integrate with AGNOS's compositor (aethersafta)? Coordinate with
  aethersafta team once the render-graph portion of the native
  backend is running.
- **Error model.** `GpuErr` codes exist (`src/error.cyr`); the
  native backend needs a mapping from syscall errno to GpuErr.
  Low-risk design question; resolve during implementation.

---

## Next steps

1. User review of this document. Revisions welcomed — principles
   are firmer than API sketch.
2. On sign-off, ADR 006 updates to cite these principles (currently
   ADR 006 describes the dual-backend decision; principles belong
   here).
3. First concrete code: the `Backend` abstraction layer (revised
   from the rolled-back experiment). This time sized for the
   compile-time-selection shape, not runtime dispatch.
4. First implementation target: native bump allocator + transient
   resource aliasing on top of `render_graph.cyr`. This is a pure
   CPU-side change that validates principles 1, 4, and 5 before
   any DRM/KMS ioctl exists.
5. DRM/KMS spike: compute-only path. `programs/native_compute_spike.cyr`
   dispatches a trivial shader and reads back results with no
   wgpu-native in the link. Validates principles 2 and 3.
