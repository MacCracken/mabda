# Mabda v3.1 — Release Punch List

**Status:** Active — minor opened, `v3.1` branch cut 2026-06-15. Tick items as they land.
**Date opened:** 2026-06-15
**Branch:** `v3.1` (cut from `main` after 3.0.4).
**Roadmap reference:** [`roadmap.md` § v3.1](roadmap.md#v31--mipmaps--multi-queue-consumer-catch-up)
**Design proposals:**
[`v3.1-mipmap-generation.md`](../proposals/v3.1-mipmap-generation.md) ·
[`v3.1-multiqueue.md`](../proposals/v3.1-multiqueue.md)

> The v3.1.x arc delivers the two "consumer catch-up" features backlogged
> against v1.0/v2.x, now that the v3.0 backend abstraction exists. They
> are **independent** and ship in **sequence**, smallest-risk first:
>
> - **3.1.0 — Mipmaps** (on-device mip-chain generation, both backends)
> - **3.1.1+ — Multi-queue** (logical ordering abstraction, both backends)

## Hard truths up front

Read these before sequencing.

- **wgpu cannot do real multi-queue.** WebGPU = one queue per device; the
  v29 FFI exposes no way to make more. Multi-queue is therefore a
  **logical ordering abstraction** — wgpu serializes all logical queues
  onto its single queue (correct, no overlap); native AMD gets real
  GFX/COMPUTE/DMA rings + timeline syncobjs (real overlap). Same public
  API (ADR 005); backend-specific performance (the v3.0 story). See
  [`v3.1-multiqueue.md`](../proposals/v3.1-multiqueue.md).
- **Mipmaps do NOT need multi-queue, and ship first.** Per-level
  read-after-write is handled on a single queue by a cache
  flush/invalidate between dispatches — `native_pm4_event_write_cache_flush_and_inv`
  already exists (Step 6.10). Mipmaps are self-contained and lower-risk,
  so they are 3.1.0.
- **Native mipmaps need NO WGSL frontend.** The 2×2 box-filter downsample
  shader is hand-authored GFX9 ISA (~100–200 bytes), the same method as
  the deadbeef / triangle shaders. WGSL→GFX9 lowering stays deferred to
  v3.x. The genuinely new native work is the **image descriptor table
  (V#)** for image_load/store — ~200 lines, ABI-risky, budget 2–4 HW
  iterations on Cezanne.
- **The render-graph multi-queue refactor is the arc's biggest risk.**
  The v2.5 graph is single-encoder/single-submit; per-node queue affinity
  + cross-queue fence edges is a 2–3 week design spike with no prototype.
  3.1.1's minimum-viable scope is **direct-dispatch** multi-queue; the
  render-graph integration is a separate bite (3.1.2) and **may graduate
  multi-queue to its own minor** if it balloons.
- **Linux + AMD only for the native wins.** Same scope boundary as v3.0.
- **No stub slots (ADR 006).** A backend slot exists only when *both*
  backends implement it for real. wgpu's serialized multi-queue and
  WGSL-shader mipmaps are *complete* implementations, not stubs.

## Tier 1 — Code completeness

### Phase M — Mipmap generation (→ 3.1.0)

Proposal: [`v3.1-mipmap-generation.md`](../proposals/v3.1-mipmap-generation.md).

- [~] **M.1** — Mip-chain BO + VA layout (native). Generalize the single
  hardcoded `_NATIVE_TEXTURE_VA_BASE` into a small VA sub-allocator;
  contiguous mip levels with per-level offsets
  (`offset_M = Σ align(w_k·h_k·4)`, ≈1.333×). Grow `NativeTexture` (or a
  ctx-side mip-metadata table) to carry `mip_count` + offsets. CPU tests:
  offset math, VA-range isolation, size formula.
  - [x] **M.1(a)** — mip-layout math (2026-06-15). Shared `mip_level_dim`
    (`src/texture.cyr`) + native contiguous `native_mip_level_offset` /
    `native_mip_chain_size` with 256-byte per-level alignment (`V#` base
    requirement) + `_native_align_up` (`src/backend_native.cyr`). 23 CPU
    asserts (mip count/dim in `mabda.tcyr`; offset/size/align in
    `mabda_v3.tcyr`). Also backfilled the previously-untested
    `mip_level_count`. Build + lint + fmt clean; 2016 CPU asserts green.
  - [x] **M.1(b)** — `NativeTexture` mip metadata (2026-06-15). Struct
    grew 32 → 48 B, appending `width` (+32), `height` (+36), `mip_count`
    (+40); offsets 0..24 unchanged. `native_texture_create_2d_rgba8`
    populates them (`mip_count = 1` for single-level). Per-level VA is
    recomputed on demand as `va + native_mip_level_offset(w, h, level)` —
    no need to store the offset array. Updated the struct-layout + round-
    trip CPU tests; HW-verified (native_texture_e2e 64×32 byte-exact on
    Cezanne). Build + lint + fmt clean; 2023 CPU asserts green.
  - [ ] **M.1(c)** — texture VA sub-allocator. Bump cursor over a widened
    texture VA region (the current 6 MiB gap between the texture base and
    the RT base can't hold a large mip chain), per-context cursor state.
    Pulls in a small VA-map layout adjustment — its own bite. Next.
- [ ] **M.2** — `BACKEND_SLOT_TEXTURE_CREATE_2D_RGBA8_MIPPED` (+208) and
  `BACKEND_SLOT_TEXTURE_GENERATE_MIPMAPS` (+216); `BACKEND_SIZE` 208→224;
  `BACKEND_MIPMAP_SLOTS_BEGIN/END` + `backend_is_complete` walk. Layout
  asserts in `tests/tcyr/mabda_v3.tcyr`.
- [ ] **M.3** — Per-level GPU access. wgpu: N `WGPUTextureView`s
  (`baseMipLevel=i`) in a ctx-local handle-keyed table; wire the
  scaffolded storage-texture bind-group-layout entry path. native:
  `native_descriptor_table_*` (16-byte V# entries; `SET_SH_REG` of the
  table VA to `COMPUTE_USER_DATA_*`). CPU tests: view table, V# layout.
- [ ] **M.4** — Downsample shader.
  - [ ] **M.4(wgpu)** — mabda-internal WGSL 2×2 box-filter compute
    shader (sampled level n → storage level n+1).
  - [ ] **M.4(native)** — `native_gfx9_shader_downsample_2x2`
    (hand-authored GFX9 ISA; clang+objdump ground-truth; byte-pinned CPU
    asserts per `feedback_verify_gfx9_shader_bytes_with_llvm_mc`).
- [ ] **M.5** — `native_pm4_build_compute_downsample(buf, src_va, dst_va,
  w, h)` + inter-level barrier wiring
  (`native_pm4_event_write_cache_flush_and_inv` between levels). CPU
  byte-exact composer test.
- [ ] **M.6** — Slot fillers (`_backend_wgpu_texture_*_mipped` /
  `_backend_native_*`) + public `gpu_texture_create_2d_rgba8_mipped` /
  `gpu_texture_generate_mipmaps` dispatchers in `src/texture.cyr` with
  the 3.0.4 input-validation guards + a `mip_count` bound. Mock-backend
  CPU tests for slot threading + null-safety.
- [ ] **M.7** — `programs/native_mipmap_e2e.cyr` + `programs/mipmap_e2e.cyr`
  (wgpu): generate a chain from a known level-0 pattern, CPU-read each
  level, verify 2×2 averaging numerically. HW-gated on AMD (renderD128).

### Phase Q — Multi-queue coordination (→ 3.1.1+)

Proposal: [`v3.1-multiqueue.md`](../proposals/v3.1-multiqueue.md).

- [ ] **Q.1** — Queue abstraction: `QUEUE_GRAPHICS/COMPUTE/TRANSFER`
  constants; Queue struct (kind + ring/handle + persistent timeline
  syncobj + current point); ctx-side queue table (NOT in the +0..+24 dual
  region). 3 slots `QUEUE_GET/BARRIER/WAIT_IDLE`; `BACKEND_SIZE` 224→248;
  range markers + `backend_is_complete`. Layout asserts.
- [ ] **Q.2** — wgpu impl: logical queues alias the single `WGPUQueue`;
  `queue_barrier` pins submit ordering; `queue_wait_idle` = device poll.
  Documented no-overlap. Mock + behavioral CPU tests.
- [ ] **Q.3** — native impl: per-queue persistent timeline syncobj
  (replaces ephemeral per-dispatch on the queue-targeted path);
  GRAPHICS→GFX, COMPUTE→COMPUTE; queue-targeted dispatch via ctx-stash
  ("current queue"). CPU tests: timeline-point accounting.
- [ ] **Q.4** — `gpu_queue_barrier`: native cross-ring timeline
  WAIT-in/SIGNAL-out chunks; wgpu ordering pin. CPU tests: barrier-chunk
  layout.
- [ ] **Q.5** — TRANSFER ring: SDMA PM4 copy path in
  `src/backend_native_pm4.cyr` (or documented COMPUTE-ring fallback if
  SDMA bring-up is deferred).
- [ ] **Q.6** — `programs/native_multiqueue_e2e.cyr`: compute writes a
  buffer, `queue_barrier`, graphics consumes it — distinct rings,
  timeline-ordered, CPU-verified. wgpu serialized-equivalent.
- [ ] **Q.7** — **Render-graph multi-queue scheduling** — design spike
  first (`docs/proposals/v3.1-render-graph-multiqueue.md`), then per-node
  queue affinity + cross-queue fence edges + per-queue submit batching.
  **Scoped separately (3.1.2 / own minor); NOT a 3.1.1 blocker.**

## Tier 2 — Integration & regression

- [ ] Full CPU suite green across all three `.tcyr` files; every new code
  path adds an assertion (house rule). Baseline carried from 3.0.4
  (1993 asserts).
- [ ] `programs/native_*_e2e` pass on Cezanne (renderD128; mipmaps and
  GRAPHICS+COMPUTE multi-queue are render-node-only — no DRM master
  needed, unlike Phase D present).
- [ ] Consumer smoke: the consumers that pull each feature build against
  the new bundle — mipmaps: soorat / kiran; multi-queue: rasa / bijli.
  (Light sweep; the user noted only a couple of consumers are live today.)
- [ ] `dist/mabda.cyr` regenerates diff-clean (idempotent) at each cut.

## Tier 3 — Performance evidence

The multi-queue feature's whole point is overlap; it needs a number.

- [ ] Mipmap generation timing (compute downsample chain) on native vs a
  CPU-side reference, both backends, into `src/profiler.cyr`.
- [ ] Multi-queue overlap measurement on native: serialized
  (single-queue) vs GRAPHICS+COMPUTE overlapped wall-clock for a
  compute+graphics workload — the headline 3.1.1 number. wgpu shows the
  serialized baseline (expected: no speedup — documents the constraint
  honestly per `feedback_honest_perf_framing`).
- [ ] `bench-history.csv` gains mipmap + multi-queue cells where
  applicable.

## Tier 4 — Documentation

- [ ] `CLAUDE.md` updated for v3.1 surface (new slots, queue/mip API,
  `BACKEND_SIZE` growth, assert count).
- [ ] `roadmap.md` v3.1 section reflects the shipped shape (mipmaps 3.1.0,
  multi-queue 3.1.1+, render-graph-mq possibly 3.1.2/own minor).
- [ ] `CHANGELOG.md` `[3.1.0]` and `[3.1.1]` sections (Added/Changed/…).
- [ ] Audit index: re-run the P(-1) audit checklist against the
  FFI/buffer/texture/CS-submit deltas; file
  `docs/audit/YYYY-MM-DD-audit.md` if HIGH/MED surfaces.
- [ ] Per-feature consumer guides if a consumer adopts (mipmap usage;
  queue/barrier usage).

## Open decisions (resolved as bites land)

- **Does multi-queue's render-graph refactor (Q.7) justify its own
  minor?** Decide after the Q.7 design spike. The arc is structured so
  3.1.1 ships direct-dispatch multi-queue regardless.
- **TRANSFER ring now or later?** (Q.5) Ship GRAPHICS+COMPUTE overlap
  first; SDMA may be a follow-up bite if bring-up is heavy.
- **Mip metadata: grow `NativeTexture` vs ctx-side table?** (M.1) Lean
  ctx-side table to keep the texture struct opaque/stable; confirm at
  implementation.
