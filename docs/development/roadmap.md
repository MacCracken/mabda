# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 29 domain modules, ~4,000 lines, 286 CPU assertions + 1 GPU
> integration test. `dist/mabda.cyr` bundle at 4,093 lines.

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It ships
now via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) can depend on a stable,
sovereign-owned surface immediately. The C shim is **transitional
scaffolding** — it exists exclusively to get mabda in front of
consumers while the native GPU backend is being built. When the
native backend lands, consumer code will not change by a single byte.

```
  v2.3.0  ───▶  wgpu-native via C shim          (shipping, audited)
  v2.4.0  ───▶  v1.0-parity (partial)           (compute E2E, LOW sweep)
  v2.4.1  ───▶  sakshi observability            (structured logging + spans — additive)
  v2.4.2  ───▶  GPU runtime validation          (FFI offset/enum sweep + compute E2E actually runs on GPU)
  v2.4.3  ───▶  render-pass FFI + render E2E    (closes v1.0 checklist)
  v2.5.0  ───▶  render graph                    (DAG pass orchestration — additive)
       │
       │        kernel GPU driver work (in parallel, AGNOS scope)
       ▼
  v3.0    ───▶  pure Cyrius GPU backend         (backend swap — API unchanged)
  v3.1    ───▶  multi-queue + mipmaps           (consumer catch-up, C shim retired)
  v3.2    ───▶  compressed textures + SPIR-V    (texture/shader breadth)
  v3.3    ───▶  image loading                   (gated on pure-Cyrius decoder)
  v3.x+   ───▶  WebGPU / WASM                   (blocked on Cyrius WASM backend)
```

Everything in this roadmap prioritizes **API stability** over
**backend correctness**. The public surface (context, buffer, compute,
texture, render_pipeline, etc.) is the load-bearing contract. The FFI
layer underneath is explicitly marked internal (`@internal` header on
line 1 of every FFI module) so no consumer accidentally couples to it.

---

## v2.0.0 — Cyrius Port (Shipped)

Complete port from Rust to Cyrius. All Rust modules ported. GPU FFI
operational via C launcher + struct-packing shims.

### Shipped
- All core modules: error, color, capabilities, context, profiler, resource
- All buffer/compute modules: buffer, compute, shader_cache, pipeline_cache, bind_group_cache
- All graphics modules: vertex, blend, sampler, depth, texture, bind_group, instancing
- All render modules: render_target, render_pipeline, render_pass, surface, debug
- FFI layer: wgpu_types, wgpu_descriptors, wgpu_ffi, C launcher
- 89 standalone tests + 4 GPU integration tests (including buffer round-trip)
- Buffer readback round-trip (write → copy → map → verify) — **v1.0 criterion met**
- Cyrius compiler fixes upstreamed: PIC codegen (3.4.12), mmap rename (3.4.12), `_cyrius_init` (3.4.14)
- GPU discovery via yukti 1.2.0
- Struct-packing shim pattern for wgpu functions with 6+ i64 args (avoids `fncall6` bug)

---

## v2.1.0 — Feature Catch-up (Shipped)

Rust-parity catch-up release. Every v2.1 item landed along with a
batch of wgpu v29 enum-value fixes that were latent in the v2.0 tree
(silently compiled against an older wgpu version — first noticed
when texture and render pipeline FFI actually dispatched into
wgpu-native).

- `typed_buffer.cyr` port
- Standalone `.tcyr` tests for pure-data modules (+191 assertions)
- GPU timestamp profiling (`gpu_timestamps.cyr`)
- Texture creation via FFI (slots 45–51)
- Render pipeline FFI (slots 52–53, `render_pipeline_create_simple`)
- Surface FFI (slots 54–57)
- CPU-only benchmark harness (`tests/mabda.bcyr`)

**Outcome:** 290 assertions, 10 GPU-backed phase-0 checks. Fixed a
latent cache-dangling-pointer bug that shipped in v2.0 (shared
`_hash_to_heap_key` helper in `cache_key.cyr`).

---

## v2.1.1 — Stdlib Inclusion (Shipped)

Goal: ship mabda as a Cyrius stdlib dep so every Cyrius user gets a
sovereign GPU surface they can build against.

- `dist/mabda.cyr` single-file bundle
- `[lib]` section in the manifest
- Toolchain pin bumped to Cyrius 3.4.19 (the release that activates
  `[deps.mabda]` as a first-class dep)
- Public API surface marked with `# @public` / `# @internal` on
  line 1 of every `src/*.cyr` file (see ADR 005)
- `examples/stdlib-consumer/` reference project
- `docs/stdlib-integration.md` consumer guide
- Vendored `lib/*.cyr` replaced by a symlink to `$HOME/.cyrius/lib/`
- `scripts/version-check.sh` — version consistency gate

---

## v2.1.2 — Rust-source Removal (Shipped)

Hygiene release. The frozen `rust-old/` tree was removed from the
working tree; the full Rust v1.0.0 source remains accessible via
`git checkout 1.0.0`. Preserved the Rust benchmark history CSV and
the Rust coverage snapshot inlined into
`docs/benchmarks-rust-v-cyrius.md`.

---

## v2.2.0 — Scaffolding Refresh (Shipped)

Project layout brought in line with the first-party AGNOS convention
(yukti / vidya / patra). Toolchain pin jumped from Cyrius 3.4.19 to
5.4.7. No library API changes — every consumer call site keeps
working without modification.

- `cyrius.toml` → `cyrius.cyml` (yukti format,
  `version = "${file:VERSION}"` templating)
- Flat layout: `src/lib.cyr` (renamed from `src/mabda.cyr`) declares
  the single include chain; 29 domain modules remain flat
- `tests/tcyr/mabda.tcyr` — consolidated CPU-only suite (273
  assertions)
- `tests/bcyr/mabda.bcyr` — benchmark harness in conventional subdir
- `programs/smoke.cyr` — link-check entry point for `cyrius build`
- `programs/phase0.cyr` — renamed GPU integration test
- CI rewritten to match yukti: lint / fmt / vet / dist-sync / test /
  bench gates
- Release workflow regenerates `dist/mabda.cyr` via `cyrius distlib`
- Makefile shrunk to a thin wrapper over the `cyrius` CLI
- `scripts/bundle.sh` retired (`cyrius distlib` replaces it)

---

## v2.3.0 — P(-1) Security Audit (Shipped)

Last audit-gated stdlib-candidate release before mabda is promoted
to first-party trusted status alongside yukti / patra / sakshi. Full
findings in
[`docs/audit/2026-04-19-audit.md`](../audit/2026-04-19-audit.md) —
0 CRITICAL / 2 HIGH / 6 MED / 6 LOW across 29 modules; every HIGH
and MED fixed.

- **HIGH-1** `surface_state_present` name collision → accessor was
  silently shadowed by the mutating present helper, breaking
  `_surface_state_configure`
- **HIGH-2** `rpb_label` 4-byte heap overflow (80-byte alloc, 8-byte
  store at +76)
- **MED-1** workgroup helpers: zero-divisor SIGFPE
- **MED-2** growable buffer: `cap * 2` overflow
- **MED-3** texture upload: dimension product overflow + missing
  zero-dim guard
- **MED-4** storage buffer write: `write_count × element_size`
  overflow
- **MED-5** storage buffer wrap: capacity / count / element_size
  inconsistency
- **MED-6** profiler: `clock_gettime` return-value unchecked
- 13 new audit-regression assertions (273 → 286)
- `CLAUDE.md` gains P(-1) + Security Hardening sections

### Scheduled (not blocking 2.3.0)
- **LOW-2** `read_buffer` size cap
- **LOW-3** wire `validate_dispatch` / `validate_dimensions` into
  internal dispatchers
- **LOW-4** bounded `strlen` in `wgpu_string_view`
- **LOW-5** resource cleanup on `compute_pipeline_new` failure paths
- **LOW-6** clamp color components in `texture_from_color`

---

## v2.4.0 — v1.0-Parity (Partial) (Shipped)

Pick off the v1.0 criteria the existing FFI surface can already
reach. Render-pipeline end-to-end deferred to v2.4.3 because the
FFI table has no render-pass execution slots yet (descriptor
builder exists; encoder dispatch does not — see `wgpu_ffi.cyr`,
slots end at 57/surface). Small release, no structural changes.

| # | Item | Status |
|---|------|--------|
| 1 | Compute dispatch end-to-end test in `programs/` | Planned — write → dispatch → read-back assertion, mirroring the buffer round-trip that already ships |
| 2 | Pick up scheduled LOW audit items (LOW-2 → LOW-6) | Planned — none individually block, but 2.4.0 is a good time to sweep |
| 3 | `make build-gpu-programs` CI gate | Planned — compile-but-don't-link every `programs/*.cyr`, fail on any cc5 warning. Closes the missing-include class of bug surfaced by `docs/issues/2026-04-19-phase0-build-broken.md` Issue 2 |

Exit criteria: `programs/phase0.cyr` + `programs/compute_e2e.cyr`
together exercise every v1.0 criterion the current FFI can reach.
Render-pipeline E2E waits for v2.4.3.

---

## v2.4.1 — Sakshi Observability (Shipped)

Earn the sakshi include that's already in `src/lib.cyr`. Wire the
structured-logging / span / packed-error primitives into mabda's
existing plumbing. Additive only — no public API changes, default
behaviour stays silent (consumers opt in via `sakshi_set_level`).

### Planned scope
- **`src/error.cyr`** — every `Err(gpu_err(...))` construction path
  emits `sakshi_error(...)` with the GpuErr code mapped to a short
  category string. Existing tagged-union return shape unchanged.
- **`src/profiler.cyr`** — `profiler_frame_begin` /
  `profiler_frame_end` wrap their timing logic in
  `sakshi_span_enter("frame", 5)` / `sakshi_span_exit()` so trace
  consumers get per-frame spans for free.
- **`src/context.cyr`** — `sakshi_info` on context creation,
  `sakshi_warn` on the adapter-not-found / device-create-failed
  error paths.
- **~5 new CPU assertions** — level gating, emission fires, span
  depth behaves across frame begin/end.

### Out of scope for 2.4.1
- Replacing any part of the profiler with sakshi — both coexist.
- Wiring sakshi into `resource.cyr` leak tracing (backlog — adds
  noise without a consumer asking for it).
- UDP / file / ring-buffer sakshi outputs — consumer-configured,
  mabda just emits events.

### Exit criteria
- `dist/mabda.cyr` still clean; `examples/stdlib-consumer/` still
  compiles unchanged.
- CPU suite passes with sakshi level set to `SK_TRACE` end-to-end
  (captures events without crashing).
- At least one soorat / rasa consumer opts in and reports back that
  spans show up in their sakshi output.

---

## v2.4.2 — GPU Runtime Validation (Shipped)

Scope re-carve. The original v2.4.2 plan (render-pass FFI + render
E2E) moves to v2.4.3. v2.4.1 shipped with latent FFI bugs that
CPU-only tests couldn't catch — `compute_e2e` and `phase0` were
compile-clean and link-clean but had never run against a real
wgpu-native + Vulkan driver. First-ever run on a RADV / Mesa 26.0
box surfaced a cascade of offset / enum / ABI issues. v2.4.2 fixes
all of them and adds CPU regression assertions that would have
caught the originals — the provable foundation v2.4.3 can build on.

### Shipped
- Toolchain pin `5.4.10 → 5.5.11`; `_cyrius_init` fix and
  `fncall7`/`fncall8` confirmed at the current release.
- **`deps/wgpu_main.c`** — `wgpuCreateInstance` now passes
  `InstanceExtras { backends = Vulkan }`. Headless-safe on Mesa.
- **`Makefile`** — `strstr` localized (was interposing libc and
  breaking Mesa's driver-string probing).
- **`src/wgpu_descriptors.cyr`** — `wgpu_bgl_entry_buffer` offsets
  corrected for v29's `WGPUBufferBindingLayout` (8-byte
  `nextInChain` first; `type@+40`, `hasDynOffset@+44`, `minSize@+48`).
- **`src/wgpu_types.cyr`** — `WGPUBufferBindingType` renumbered for
  v29 (inserted `BindingNotUsed = 0`); `WGPULoadOp` values
  un-swapped (`LOAD=1`, `CLEAR=2`).
- **`src/compute.cyr`** — `compute_dispatch` signature reduced from
  7 params to 5 via a `dims_xyz` pointer. The 7-param class
  reliably crashes when internally fncalling into wgpu-native
  (re-verified at 5.5.11 — see `feedback_cyrius_param_ceiling`).
  **Breaking** — migration in the CHANGELOG.
- **`programs/phase0.cyr`, `programs/compute_e2e.cyr`** — added
  missing `include "lib/sakshi.cyr"` (regression from v2.4.1's
  observability wiring, caught by 5.5.11's stricter `cyrius check`).
- **18 new CPU regression assertions** under a new
  `v2.4.2 — GPU runtime validation regressions` section. 309 → 327.
- **`make test-phase0`** 10/10 pass; **`make test-compute-e2e`** 7/7
  pass on a RADV / Mesa 26.0 / kernel 6.18 host.

### v1.0 tick
- **Compute dispatch end-to-end** — now runs on real hardware.
  Last open v1.0 item is render-pipeline E2E (v2.4.3).

---

## v2.4.3 — Render-pass FFI + Render E2E

Closes the v1.0 checklist. Adds the wgpu render-pass execution
surface. Mechanical FFI work — no public mabda API changes;
`render_pass.cyr` builder gains an actual dispatcher to hand its
descriptors to. Full plan already in
`docs/proposals/2026-04-19-render-pass-ffi.md` — the foundation it
builds on is now proven by v2.4.2's runtime-validated FFI.

### Planned scope
- **`wgpu_ffi.cyr` slots 58-63 (approx)** — add:
  - `wgpuCommandEncoderBeginRenderPass` (likely struct-packed via
    a C shim — descriptor has 6+ fields including a colorAttachments
    array pointer; fits the `feedback_fncall6_wgpu` pattern)
  - `wgpuRenderPassEncoderSetPipeline`
  - `wgpuRenderPassEncoderSetBindGroup`
  - `wgpuRenderPassEncoderDraw`
  - `wgpuRenderPassEncoderEnd`
  - `wgpuRenderPassEncoderRelease`
  - `wgpuCommandEncoderCopyTextureToBuffer` (struct-packed shim —
    src/dst are `WGPUTexelCopyTextureInfo` / `WGPUTexelCopyBufferInfo`
    structs)
- **`render_pass.cyr` dispatcher** — `rpb_pass_begin(encoder, b)`
  builder method that emits the descriptor and calls the new FFI.
- **`programs/render_e2e.cyr`** — create offscreen RGBA8 render
  target with RENDER_ATTACHMENT | COPY_SRC usage; clear to a known
  colour via render pass; copy texture → buffer; map + verify
  pixels.
- **`deps/wgpu_main.c`** — 2-3 new struct-packing shims, fn-table
  population.
- **~10 new CPU assertions** + 1 new GPU integration program.

### Exit criteria
- `make test-render-e2e` passes on a wgpu-native-capable host.
- v1.0 criteria checklist below is fully ticked.
- `make build-gpu-programs` CI gate continues to compile-clean
  every `programs/*.cyr`.

### Why this isn't bundled into 2.5.0
The render graph in 2.5.0 *uses* render-pass dispatch as a
primitive — it cannot land cleanly without the FFI slots existing
first. Splitting them gives 2.5.0 a clean "build orchestration on
top of stable primitives" narrative instead of mixing FFI plumbing
with DAG design.

---

## v2.5.0 — Render Graph (After 2.4.3)

First feature release post-parity. Adds DAG-style pass orchestration
on top of the existing `render_pass` + `render_pipeline` +
`compute` primitives. Additive only — no public API of the
underlying modules changes, so consumers can opt in at their own
pace. Designed to survive the 3.0 backend swap unchanged (graph
execution is pure Cyrius; nodes dispatch through the public mabda
API, not through FFI directly).

### Planned scope
- `src/render_graph.cyr` — new module. Node types: **compute node**
  (shader + bind group + workgroup dims), **render node** (pipeline
  + target + draw list), **copy node** (buffer↔buffer, buffer↔texture,
  texture↔texture), **transient resource** (buffer / texture the
  graph owns for the duration of a frame).
- Topological sort + linear execution. No automatic barrier insertion
  in v2.5 — wgpu-native handles the synchronization we need; the
  graph only owns the **ordering**.
- Resource aliasing pass (optional, off by default) — transient
  buffers/textures with disjoint lifetimes share allocation.
- `rasa` / `soorat` are the reference consumers. One simple render
  graph end-to-end program in `programs/` (clear → compute → blit →
  present).

### Out of scope for 2.5.0
- Cross-queue coordination (moves to 3.1 with multi-queue).
- Barrier tracking / automatic layout transitions (wgpu handles; the
  native backend in 3.0 will revisit).
- Conditional / branching passes. Linear DAG only.

### Exit criteria
- Render graph module with ≥20 new CPU assertions in
  `tests/tcyr/mabda.tcyr`.
- One GPU integration program exercising compute → render → copy
  in a single graph submission.
- Documentation: `docs/guides/render-graph.md` — authoring guide with
  a three-node example.

---

## v3.0 — Native GPU Backend (Backend Swap)

The wgpu-native + C shim path is transitional. **v3.0 is the swap.**
wgpu-native and the `deps/wgpu_main.c` launcher are retired in
favour of a pure Cyrius GPU backend (DRM/KMS on Linux first, AGNOS
kernel driver eventually). Cyrius projects depend on zero C
artifacts for GPU work.

**The one invariant that matters:** the public mabda API (`# @public`
files from v2.1.1) does not change when the backend swaps. Consumers
bump their `[deps.mabda]` tag and their C launcher requirement
disappears. The `examples/stdlib-consumer/` project is the regression
test — if it still compiles after the swap, the contract held. The
v2.5 render graph must also replay unchanged.

### Scope (high level, refined closer to the work)
- DRM/KMS backend in pure Cyrius — no libdrm, no libwayland.
- WGSL → hardware ISA lowering path (vendor-first decision deferred).
- Backend selector: `wgpu-native` (legacy, kept for one release as
  migration safety) vs. `native` (default).
- ADR 006 supersedes ADR 004 (C launcher FFI). Filed when scope
  concretizes.

### Exit criteria
- `dist/mabda.cyr` consumers rebuild with `[backends]` defaulting to
  `native`; `examples/stdlib-consumer/` and `programs/phase0.cyr`
  pass without the C launcher.
- soorat, rasa, ranga, bijli, aethersafta, kiran all green on the
  native backend in their own CI.
- `deps/wgpu_main.c` and `deps/wgpu-native/` removed from the tree
  one release after the swap (v3.1 cleanup).

---

## v3.1 — Multi-queue + Mipmaps (Consumer Catch-up)

Both items are orthogonal to the backend swap and were backlog'd
against v1.0 / v2.x — 3.1 is their natural home once the native
backend is stable.

- **Multi-queue coordination** — additive to `GpuContext`. Separate
  compute / graphics / transfer queues with explicit submit ordering.
  Consumers needing it: rasa (compute + present overlap), bijli
  (large transfer workloads).
- **Mipmap generation** — on-device chain generation via compute
  shader. Builds on the existing texture FFI surface. Consumers:
  soorat (texture-heavy UI), kiran (asset pipeline).
- **`deps/wgpu_main.c` + `deps/wgpu-native/` removal** — the final
  cleanup from the v3.0 swap, one release later as per v3.0 exit
  criteria.

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

Empty. All previously-listed backlog items are now slotted into
2.5.0 → 3.x above. New items land here first and graduate to a
version once there's consumer demand plus a clear scope.

---

## v1.0.0 Criteria (Rust v1.0 feature parity in Cyrius)

- [x] All 29 modules ported
- [x] GPU context creation working
- [x] Buffer create / write / release working
- [x] Buffer readback round-trip (write → copy → map → verify) —
      `programs/phase0.cyr`
- [x] Texture create / view / upload / release — `programs/phase0.cyr`
- [x] Render pipeline create / release — `programs/phase0.cyr`
- [x] Compute dispatch end-to-end test (v2.4.0 wrote the program;
      v2.4.2 got it running on real hardware — `programs/compute_e2e.cyr`)
- [ ] Render pipeline end-to-end draw + readback (v2.4.3 — needs
      render-pass FFI expansion)
- [ ] Consumer integration: soorat port to Cyrius (consumer-side, not
      mabda-side — tracked in soorat's repo)

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (`load64` at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — **transitional**, replaced in v3.0 |
| [005](../adr/005-public-api-surface-marking.md) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that survives the backend swap |
| 006 (planned, v3.0) | Pure Cyrius GPU backend via DRM/KMS — supersedes ADR 004 |
