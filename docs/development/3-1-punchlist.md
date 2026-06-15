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
- **Native mipmaps need NO WGSL frontend AND no descriptor table.** The
  2×2 box-filter downsample shader is hand-authored GFX9 ISA, the same
  method as the deadbeef / triangle shaders; WGSL→GFX9 lowering stays
  deferred to v3.x. **PIVOT (M.3, 2026-06-15):** native textures are
  LINEAR, so the shader uses flat `global_load`/`global_store` against
  computed VAs (the deadbeef user-SGPR ABI) — the image descriptor table
  (T#/V#) the proposal first assumed is **gone** (deleted ~200 lines of
  ABI-risky infra; residual risk is just the per-channel RGBA8 averaging,
  1–2 HW iterations). See `v3.1-mipmap-generation.md` § PIVOT.
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

- [x] **M.1** — Mip-chain BO + VA layout (native). DONE 2026-06-15 across
  three bites (a/b/c below). Mip-layout math + `NativeTexture` mip
  metadata + per-context VA sub-allocator all landed, CPU-tested, and
  HW-verified on Cezanne (texture/compute/render e2e all pass).
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
  - [x] **M.1(c)** — texture VA sub-allocator (2026-06-15). Texture VA
    region moved up to `0xFFFF800180000000` with a 2 GiB span (the old
    6 MiB gap below the RT base couldn't hold a mip chain); 64 KiB-aligned
    per allocation. Per-context bump cursor at `GpuContext+120`
    (`GPU_CONTEXT_SIZE` 120 → 128, zero-init by the ctx memset, lazy-init
    in the allocator). `native_ctx_alloc_texture_va` + accessors;
    `native_texture_create_2d_rgba8` now takes the VA from the allocator
    (the slot calls it). 9 CPU asserts (allocator math, alignment,
    exhaustion, RT non-overlap, ctx size). HW-verified: the new VA base is
    valid GPUVM on Cezanne (texture e2e byte-exact); compute + render
    unaffected by the ctx growth. Build / lint / fmt / distlib /
    version-check clean; 2032 CPU asserts green.
- [x] **M.2** — mipmap slot layout (2026-06-15).
  `BACKEND_SLOT_TEXTURE_CREATE_2D_RGBA8_MIPPED` (+208) +
  `BACKEND_SLOT_TEXTURE_GENERATE_MIPMAPS` (+216); `BACKEND_SIZE` 208→224;
  `BACKEND_MIPMAP_SLOTS_BEGIN/END` (208/224). Per the 6.8a / 7.5 pattern
  the `backend_is_complete` walk over the mipmap range is **deferred to
  M.6** (when the fillers land) so existing backends stay "complete"
  rather than falsely incomplete. 7 layout asserts; build / lint / fmt /
  vet / distlib / version-check clean; 2039 CPU asserts green. Pure CPU —
  no HW behavior yet.
- [x] **M.3** — Per-level GPU access (both backends; 2026-06-15).
  - [x] **M.3(native)** + **PIVOT** (2026-06-15). Native textures are
    LINEAR GTT BOs, and the deadbeef shader proves flat
    `global_load`/`global_store` against a user-SGPR VA works on Cezanne —
    so per-level access is just `native_texture_level_va(tex, level)`
    (chain base + `native_mip_level_offset`). **This DELETES the image
    descriptor-table (T#/V#) design** from the proposal (~200 lines,
    ABI-risky, 2–4 HW iterations) — the downsample shader (M.4) addresses
    memory arithmetically instead. 4 CPU asserts; proposal updated with
    the pivot rationale. Build/lint/fmt/distlib clean; 2043 asserts green.
  - [x] **M.3(wgpu)** — descriptor machinery (2026-06-15).
    `wgpu_texture_view_descriptor_level(label, fmt, dim, base_mip)` for
    per-level views; `wgpu_bgl_entry_texture` (sampled read) +
    `wgpu_bgl_entry_storage_texture` (write) BGL-entry builders, sub-struct
    offsets + enum values (`SampleType_Float=2`, `StorageTextureAccess_WriteOnly=2`)
    cross-checked against `deps/wgpu-native/include/webgpu/webgpu.h` v29.
    11 CPU byte-layout asserts. (Building the per-level view list +
    bind groups + dispatch loop is the wgpu half of M.6.)
- [x] **M.4** — Downsample shader (both backends; 2026-06-15).
  - [x] **M.4(wgpu)** — `mabda_wgsl_downsample_2x2(dst)` (2026-06-15):
    mabda-internal WGSL 2×2 box-filter (sampled level n via textureLoad →
    storage level n+1 via textureStore; @workgroup_size(8,8) + in-shader
    bounds check). Assembled from <120-char pieces into a caller-owned
    buffer (Cyrius has no literal concat + the 120-char line lint). 5 CPU
    smoke asserts. WGSL validated on wgpu HW in M.7 (no standalone naga
    here), like the native shader is HW-validated then.
  - [x] **M.4(native)** — `native_gfx9_shader_downsample_2x2`
    (2026-06-15). 79 GFX9 instructions / 316 B + 64 B NOP prefetch pad =
    380 B (`GFX9_DOWNSAMPLE_2X2_SIZE`). Single-thread-per-workgroup:
    SALU address math (inputs are uniform user SGPRs s0:1=src VA,
    s2:3=dst VA, s4=dst_w, s6/s7=tgid x/y), two `global_load_dwordx2`
    (a,b / c,d adjacent), per-channel RGBA8 average (`v_and`/`v_bfe_u32`/
    `v_add3_u32`/`>>2`), `global_store_dword`. Authored to mabda's minimal
    ABI (clang's full-ABI output isn't transcribable — it calls runtime
    fns for group-id + loads args from a kernarg segment); the averaging
    mirrors clang -O2. **Every dword assembled + round-trip-verified via
    `llvm-mc` (`scripts/disasm-shaders.sh` now covers it — decodes
    line-for-line).** CPU byte-pin: size + full-buffer checksum + spot
    checks. Build/lint/fmt/distlib clean; 2053 asserts green. HW dispatch
    validation lands with M.5/M.6 (byte-pinned-before-HW, like deadbeef).
    NOTE for M.5: RSRC1 must reserve VGPRs v0..v14 (16) + SGPRs s0..s19
    (24); RSRC2 must enable TGID_X|TGID_Y; USER_SGPR=6.
- [x] **M.5** — `native_pm4_build_compute_downsample(buf, shader_va,
  stub_va, src_va, dst_va, dst_w, dst_h)` (2026-06-15). 64-dword PM4
  stream mirroring the deadbeef composer's proven scaffold, with the
  downsample ABI: RSRC1 `0x2C0083` (16 VGPR / 24 SGPR), RSRC2 `0x18C`
  (USER_SGPR=6 + TGID_X|Y so the workgroup id loads into s6/s7),
  USER_DATA_0/1=src VA, _2/3=dst VA, _4=dst_w, NUM_THREAD 1/1/1,
  DISPATCH (dst_w, dst_h, 1). Structural CPU test (size + scan for the
  wired RSRC/VA/dim/marker values). Build/lint/fmt/distlib clean; 2084
  asserts green. **The inter-level cache-flush barrier
  (`native_pm4_event_write_cache_flush_and_inv`) is wired in the M.6
  `generate` loop** (it sits between dispatches, not inside this
  single-level composer). HW dispatch validation: M.7.
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

- [x] **Test suite reorganized by functionality (2026-06-15).** Replaced
  the version-named `mabda.tcyr` / `mabda_v3.tcyr` / `mabda_v3_phase_d.tcyr`
  trio with **11 domain-named suites** under `tests/tcyr/` (core, buffer,
  compute, texture, graphics, render, backend, caches, surface, native,
  kms), mirroring `src/`. Each is standalone (own `main` + `assert_summary`)
  and self-contained (needed mocks inlined). 2076 asserts preserved exactly
  (verified file-by-file). Makefile/CI/release now glob `tests/tcyr/*.tcyr`.
  Also fixed a latent gap: the old `mabda_v3.tcyr` had grown to 148 KiB,
  past the 128 KiB lint/fmt cap (its tail was silently unchecked); every
  new file is well under (native, the largest, is 113 KiB).
- [ ] `CLAUDE.md` updated for v3.1 surface (new slots, queue/mip API,
  `BACKEND_SIZE` growth, assert count). *(test-layout section done.)*
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
