# v3 Native GPU Backend — Design Principles

**Status:** Draft (v3 branch, 2026-04-23 — Phase A shipped; Phase B.0–B.3.c complete; **B.3.d NOT actually passing** — Session 8's "pass" was a 10s TDR false positive, discovered Session 9; CS-submission blocker sits between ioctl and CP execution)
**Related:** [ADR 006](../adr/006-native-cyrius-gpu-backend.md) (dual-backend), [ADR 005](../adr/005-public-api-surface-marking.md) (@public boundary), [ADR 004](../adr/004-c-launcher-ffi.md) (wgpu path, v3.x era), [GFX9 store blocker](../issues/2026-04-21-gfx9-store-blocker.md)

## Framing

v3 is not "reimplement wgpu-native in Cyrius." It extracts lessons from 20+ years of GPU API evolution and commits to the simplest accurate expression of the essential ideas in Cyrius idioms. Four vidya topics ground this:

- [render_graph_architecture](../../../vidya/content/render_graph_architecture/concept.toml)
- [bindless_resources](../../../vidya/content/bindless_resources/concept.toml)
- [explicit_gpu_synchronization](../../../vidya/content/explicit_gpu_synchronization/concept.toml)
- [gpu_memory_pooling](../../../vidya/content/gpu_memory_pooling/concept.toml)

This document records **mabda-specific decisions** on top. Each decision cites the vidya concept it rests on.

---

## The three non-negotiables

These determine the API shape. If any flips, the API flips.

### 1. Graph-first submission

`render_graph_run(g)` is the canonical entry point. Single-op shortcuts (`gpu_dispatch`, `gpu_draw`, `gpu_copy`) exist as convenience — internally they build a one-node graph. **Leaf command buffers are not `@public`.** The backend assumes the graph is there and optimizes aggressively against it: barriers derived from declared reads/writes, transient memory aliased across disjoint lifetimes.

Mabda already has `src/render_graph.cyr` at v2.5.0 — we harden, we don't restart.

### 2. Bindless-only, single resource table

No bind groups. No descriptor sets. No layouts. No update templates. One typed u32 handle per resource; shaders index into per-frame `resources[]` and `samplers[]` tables.

**Hardware floor**: Vulkan 1.2 `VK_EXT_descriptor_indexing` or better. AGNOS picks the target. Older hardware stays on the wgpu path (ADR 004).

Consumer-visible resource primitives: `TextureHandle`, `BufferHandle`, `SamplerHandle` — tagged u32s with generation counter + type tag + index. Nothing else.

### 3. Graph-derived sync; frame counter default

No hand-placed barriers in `@public`. For the ~5% the graph can't cover:

- **Single-queue** (all v2.x integration programs, soorat, rasa, bijli today): `gpu_current_frame(ctx) → u64` and `gpu_wait_frame(ctx, n)`. One primitive.
- **Multi-queue** (future rasa compute/present overlap, bijli large transfers): opt into `timeline_new/wait/signal`.

Stage **enum** — seven stages: `COMPUTE`, `VERTEX`, `FRAGMENT`, `COLOR_OUT`, `DEPTH_OUT`, `TRANSFER`, `HOST`. Not a 30-bit mask.

---

## Allocator model (structural, not a principle)

Three allocators, three memory tiers. Graph-informed transient aliasing is the biggest single win available (Frostbite reclaimed ~50% of render-target memory).

**Allocators:**
- `bump` — per-frame transients; reset on frame boundary; backs graph transients.
- `ring` — CPU→GPU upload; head advances on write, tail on GPU-finished (frame counter).
- `tlsf_block` — persistent; one DRM BO per 128 MB, TLSF sub-allocates.

**Tiers:**
- `DEVICE` — VRAM only. Default for persistent.
- `UPLOAD` — HOST_VISIBLE. Default staging.
- `BAR` — DEVICE_LOCAL + HOST_VISIBLE when ReBAR detected at `gpu_context_new`. Auto-selected over UPLOAD.

**Excluded in v3.0:** defragmentation (shipping AAA sizes pools to avoid it; revisit only if a consumer surfaces a real bug), general-purpose `gpu_alloc(size, tier)` API (consumers create typed resources; allocator selected by function name).

---

## Expression constraints

- Tagged unions (`lib/tagged.cyr`) for every sum type: `PassKind`, `ResourceKind`, `GpuErr`.
- Fixed-offset structs with byte layout in module header comments (v2.x pattern continues).
- `fnptr` for pass encode callbacks — not type-erased callables.
- u32 handles everywhere; descriptors as raw bytes matching `VK_EXT_descriptor_buffer` shape.
- Stdlib-style names: direct verbs (`dispatch`, `draw`, `submit`), short types (`buf`, `tex`, `fb`).
- `mabda_observability_enable()` from v2.4.1 stays. All backend events route through sakshi.

---

## API delta from v2.1.1 @public

What collapses, what stays, what's new.

**Collapses** — v2.1.1 constructor variants fold into one per resource kind:

| v2.1.1 | v3.0 |
|--------|------|
| `create_storage_buffer`, `create_storage_buffer_empty`, `create_uniform_buffer`, `create_vertex_buffer`, `create_index_buffer`, `create_dispatch_indirect_buffer`, `create_staging_buffer` (7 fns) | `gpu_buffer_new(ctx, usage, size, tier)` |
| Texture-create variants in `src/texture.cyr` | `gpu_texture_new(ctx, format, w, h, usage, tier)` |
| Render-target-create variants | `gpu_render_target_new(ctx, format, w, h, msaa, depth)` |

v2.1.1 names stay as thin shims across v3.x. **Retire at v5.1** alongside the full wgpu path removal (per ADR 005 dual checkpoint, ADR 006 per-chipset retirement schedule). The shims are cheap; preserving them through the v4.0/v5.0 vendor-by-vendor cutover means consumers don't have to re-fight the renaming question while they're flipping backends.

**Stays unchanged:**
- `render_graph_new`, `rg_*` family (v2.5.0) — hardens internally.
- `gpu_context_from_preinit`, `gpu_context_release`.
- `GpuErr` codes, `Result` helpers, `mabda_observability_enable`.

**New in v3.0:**
- `gpu_current_frame(ctx)`, `gpu_wait_frame(ctx, n)`.
- `gpu_buffer_transient(ctx, usage, size, g)` — graph-managed lifetime.
- `timeline_*` (deferred; only when multi-queue enters).

**Public-function-count target:** ≤ 35 under native backend (vs ~100 effective on the wgpu path once all helpers are counted).

---

## Backend selection

**Decision:** build-time per consumer.

- Consumers set a cyrius build flag. Exact mechanism resolved in Phase A when we touch `cyrius.cyml` — candidates: a `[mabda]` table entry, a `#define`-style compile-time constant in `src/lib.cyr`, or a separate `dist/mabda_native.cyr` that includes the native path only.
- Default is `wgpu` for v3.0 (byte-compatibility for every existing consumer).
- `dist/mabda.cyr` ships both backends compiled in. Consumers shipping under `native` can later pull `dist/mabda_native.cyr` (native-only bundle) for a smaller binary.

Not a runtime switch. The mabda process has one backend per run.

---

## Phase plan

Five phases. Each has a concrete scope and an unambiguous exit criterion. No phase starts before the previous phase's exit criterion is met.

### Phase A — Transient aliasing planner

**Scope:**
- `src/render_graph.cyr` extended — transient aliasing planner. Pure function: given the graph's existing pass array + first_use/last_use per resource, computes per-resource byte offsets so disjoint-lifetime transients share the same backing slot. Activates the `rg_aliasing(g, 1)` flag + `first_use` scaffolding from v2.5.0.
- `tests/tcyr/mabda.tcyr` grows ~30 assertions (planner correctness on synthetic graphs: single-resource base case, disjoint-lifetime share, overlapping separate, persistent-excluded, N-resource known-reclaim).
- `tests/bcyr/mabda.bcyr` grows `render_graph_compile_e2e` + `transient_aliasing_reclaim` benches.
- `fuzz/render_graph_aliasing.fcyr` — random lifetime sets, verify invariants (no co-live offset overlap, all offsets in range, block size ≥ max concurrent).

**No new allocator module.** The cyrius stdlib already ships `lib/alloc.cyr::arena_*` (24-byte header, 8-byte-aligned bump). Mabda reuses it for any CPU-side scratch. GPU-memory allocators (bump/ring/TLSF block, aligned to bufferImageGranularity) appear in Phase B where they have a DRM BO to sub-allocate from.

**Exit criterion:** 417+ CPU assertions pass. `fuzz/render_graph_aliasing.fcyr` exits 0. `transient_aliasing_reclaim` bench reports ≥ 30% memory reclaim on `render_graph_e2e`'s frame shape. `render_graph_compile_e2e` bench < 100 µs.

**Why first:** validates principle 1 (graph-first) by proving graph-level analysis produces real memory wins. Pure CPU, pure math, zero DRM ioctl risk, entirely reversible.

### Phase B — DRM compute spike (subdivided)

Phase B is an order of magnitude larger than Phase A. Phase A was pure CPU algorithm (interval coloring). Phase B requires direct DRM hardware interaction — vendor-specific ring buffer format, command-submission ioctls, shader ISA encoding, fence primitives. Mesa's radv/anv reference implementations are 50K+ lines per vendor. Not an afternoon spike.

**Subdivided into B.0–B.4**, each with its own mini-exit.

#### B.0 — Research + target-vendor selection

External research on the minimum AMDGPU (likely starting target — most-open kernel driver, Mesa radv as reference) ioctl-level compute dispatch path. Findings land in a new vidya topic for reuse across AGNOS projects. Decision point: direct-ioctl vs libdrm stepping stone — sovereignty principle prefers direct; if B.0 surfaces months of kernel RE, we negotiate the interim *before* committing code.

**Exit:** vidya topic written, target vendor chosen, estimated B.3 scope documented.

#### B.1 — Device enumeration + capability query

- `src/backend_native.cyr` (new, `@internal`) — skeleton only.
- Open `/dev/dri/renderD128`, issue `DRM_IOCTL_VERSION`, parse driver name + version.
- Tests: ioctl wrapper round-trip.
- Fuzz: `fuzz/drm_device_enum.fcyr` — device-path edge cases.

**Exit:** device-open test returns the target driver string on dev hardware.

#### B.2 — GEM BO create/map/round-trip

- Vendor-specific `GEM_CREATE` + `GEM_MMAP` ioctls.
- CPU write → CPU read-back through the BO. No GPU execution.
- Tests: BO lifecycle round-trip assertions.
- Fuzz: random size + domain combinations.

**Exit:** byte-identical round-trip through a GEM BO.

#### B.3 — Pre-compiled compute shader dispatch

- The hard part. Scope gets concrete after B.0 research.
- Pre-compiled shader binary (hand-assembled ISA or offline-compiled SPIR-V→ISA via a separate tool). **WGSL compilation is out-of-scope for v3.0** — separate mountain, post-v3.x.
- Ring buffer format, doorbell path, submission ioctl.
- `programs/native_compute_spike.cyr` dispatches a trivial hardcoded shader.
- Tests: PM4 (or equivalent) packet-encoding assertions.
- Fuzz: `fuzz/native_descriptor_encoding.fcyr` — random descriptor writes, verify byte-layout invariants.

**Exit:** compute dispatch completes without kernel error.

#### B.4 — Fence + readback + wgpu parity

- Wait for submission via sync-obj or legacy fence ioctl.
- Read result buffer; diff against wgpu backend's output on the same shader.

**Exit (Phase B stated):** byte-identical compute output vs wgpu backend. All fuzz harnesses green.

**Why this subdivision:** each sub-phase has a concrete, testable mini-exit. If B.0 research says direct-ioctl B.3 is months of kernel RE, we negotiate a libdrm stepping stone *with clear-eyed awareness of the sovereignty tradeoff* rather than hitting the wall mid-implementation.

#### Phase B status (2026-04-23)

- **B.0** ✅ direct-ioctl path picked; AMDGPU as first target.
- **B.1** ✅ `native_device_enum` returns `amdgpu` on gfx90c.
- **B.2** ✅ `native_gem_roundtrip` — byte-identical CPU→GPU→CPU.
- **B.3.a** ✅ ctx/BO-list/VA-map infrastructure lands in `backend_native.cyr`.
- **B.3.b** ✅ PM4 builder (pure math). Two encoding bugs found + fixed in Session 7 (see issue doc): ACQUIRE_MEM count field off-by-one; DISPATCH_DIRECT missing shader_type=2. Test suite gained byte-exact header assertions against Mesa `AMD_DEBUG=ib`.
- **B.3.c** ✅ `native_gfx9_shader_endpgm` (single-instruction no-op).
- **B.3.d** ⚠️ **Not actually passing (retracted Session 9, 2026-04-23).** Session 8's `dispatch completed (sync-obj signaled)` was a 10-second AMDGPU TDR recovery, not real CP execution. Session 9 proof: adding a CP-side WRITE_DATA packet to the spike's IB never lands in the target BO — post-submit readback shows the pre-submit memset value unchanged. Empty/NOP-only IB takes exactly ~10s (TDR default). Mesa's `cl_probe` on the same hardware runs in 77 ms with correct readback, confirming GPU health. The CS ioctl returns 0 but submissions never reach the CP. Session 7's PM4 encoder fixes remain valid — they sit downstream of this blocker. Full handoff: `docs/handoff/2026-04-23-session9-tdr-false-positive.md`.
- **B.4** ⛔ **Blocked on B.3.d.** All code is written and gated (real store shader in `src/backend_native.cyr`; Mesa-aligned PM4 in `programs/native_compute_store.cyr`; byte-exact shader test green in `tests/tcyr/mabda.tcyr`; 621 CPU assertions passing). Iterations that tried RSRC2 = 0x04 vs 0x08, canonical-high vs low output VA, and a hardcoded-VA variant that bypasses USER_DATA all fail the same way — because none of them ever executed. B.4 cannot be meaningfully retested until Session 9's CS-submission blocker is fixed. Second-kernel (add-kernel) discussion before declaring Phase B done remains valid and worth having post-unblock.

### Phase C — DRM render path

**Scope:**
- Native backend gains render pipeline + render pass + render target support.
- `programs/native_render_spike.cyr` — trivial clear + draw, offscreen target, readback via staging ring.
- `programs/render_graph_e2e.cyr` runs under native backend.
- Tests: barrier derivation correctness assertions (synthetic graphs with known correct barrier emission).
- Bench: `native_vs_wgpu_render`, `barrier_derivation_e2e` in `tests/bcyr/mabda.bcyr`.
- Fuzz: `fuzz/render_graph_barrier_derivation.fcyr` — random pass graphs, verify derived barriers produce identical pixel output to a hand-placed reference.

**Exit criterion:** `render_graph_e2e` native output matches wgpu output on the same pipeline. All four v2.x integration programs (phase0, compute_e2e, render_e2e, render_graph_e2e) pass under both backends. Fuzz harnesses green.

**Why third:** render needs surface abstractions compute doesn't. Headless first; surface/present integration (aethersafta) is v3.1.

### Phase D — Consumer canary

**Scope:**
- One consumer (soorat or rasa) adds a `backend = native` CI matrix entry.
- Consumer code does not change. `@public` compatibility is the whole test.

**Exit criterion:** canary CI green on the chosen consumer's repo.

**Why fourth:** the stability contract is only meaningful if validated against real consumer code.

### Phase E — v3.0 ship

**Scope:**
- Dual-backend bench matrix published: (wgpu × native) × (pre-5.6.x × post-5.6.x, if 5.6.x landed) in `bench-history.csv`.
- `docs/benchmarks-rust-v-cyrius.md` refreshed.
- ADR 006 moves from Proposed to Accepted, citing these principles.
- CHANGELOG entry for 3.0.0.

**Exit criterion:** `VERSION` = 3.0.0, release tagged.

**v3.1+ (post-ship):** aethersafta surface/present integration, multi-queue timelines on consumer demand, filesystem-persistent pipeline cache.

---

## Work-loop convention (all phases)

Every phase lands code + tests + bench + fuzz together. No merge to `v3` without all four.

- **Tests** — `tests/tcyr/mabda.tcyr` for CPU assertions. Every new function gets assertions covering the happy path and at least one failure mode.
- **Benchmarks** — `tests/bcyr/mabda.bcyr` for CPU-side, `programs/benchmarks.cyr` for GPU-side. New code path = new bench row. Results append to `bench-history.csv`.
- **Fuzz** — `fuzz/<module>.fcyr`. Standalone programs that exit 0 on pass / nonzero (distinct code per check) on fail. `make fuzz` runs all harnesses; CI fails on any nonzero exit.

Convention matches cyrius stdlib fuzz (`../cyrius/fuzz/*.fcyr`). Mabda's `Makefile` gains a `fuzz` target in Phase A.

## Validation benchmarks

Calibrated to mabda's existing dev environment. No aspirational hardware claims.

| Benchmark | Target | Gate |
|-----------|--------|------|
| `rg_plan_aliasing_stats_5` | baseline, no target (allocator-floor noise) | Phase A |
| `rg_plan_aliasing_stats_30` | < 50 µs (synthetic, O(n²) projection) | Phase A |
| `transient_aliasing_reclaim` | ≥ 30% memory reclaim on 6-node staggered chain | Phase A exit |
| `native_vs_wgpu_compute` | Native ≥ 90% of wgpu on `compute_e2e` | Phase B exit |
| `native_vs_wgpu_render` | Native ≥ 90% of wgpu on `render_e2e` | Phase C exit |
| `public_fn_count_native` | ≤ 35 `@public` functions under native | Phase E exit |
| `dist_mabda_native_loc` | ≤ 6000 lines for `dist/mabda_native.cyr` | Phase E exit |

**Scale caveat:** all Phase A benches are on synthetic graphs of ≤30 transients. mabda has no realistic-scale consumer graph today — `render_graph_e2e` is 3 nodes / 0 transients; the Phase A benches are *above* that scale but below any production shader/renderer. Consumer-visible wins (real draw-call rate, real-frame render-graph compile, ReBAR upload) land in Phase D canary benchmarks on soorat or rasa's actual graphs. No published comparison (Granite, Frostbite, UE5 RDG) applies at our current scale because those are quoted at hundreds of resources.

**Planner complexity:** O(n²) in transient count. Acceptable at tens; will need interval-tree retrofit (O(n log n)) if Phase D consumer workloads cross ~100 transients. Captured as a candidate entry in `docs/proposals/cyrius-5.6x-optimization-requests.md` if the rewrite becomes necessary.

---

## Out of scope for v3.0

Defragmentation. Mesh shaders. Ray tracing. Mobile/tiler-specific optimizations. Multi-GPU. iGPU unified-memory fast path. Filesystem-persistent pipeline caching. All live as v3.x backlog or later.

---

## Open questions (resolve during implementation, not blocking)

- **WGSL → ISA path.** WGSL → SPIR-V → vendor (Mesa NIR) vs direct vendor. Lean SPIR-V first for ecosystem reasons.
- **DRM ioctl vs syscall wrapper.** Probably `syscall(16, SYS_IOCTL, fd, cmd, arg)` via `lib/syscalls.cyr`. Confirm in Phase B.
- **aethersafta surface path.** Defer to Phase C completion; aethersafta team coordinates v3.1.

---

## Sign-off gate

On user approval:

1. ADR 006 updates to cite these principles (Status stays Proposed until Phase E).
2. **Phase A kicks off.** First concrete code: `src/allocator.cyr` bump allocator + assertions in `tests/tcyr/mabda.tcyr`. Everything else follows.

If any non-negotiable flips (graph-first, bindless-only, graph-derived sync), this doc revises before any Phase A code lands. If the allocator model or expression constraints flip, the code adjusts but the phase plan holds.
