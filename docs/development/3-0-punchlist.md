# Mabda v3.0 — Release Punch List

**Status:** Working document. Tick items off as they land.
**Date opened:** 2026-04-28
**Last refresh:** 2026-04-30 (post Step 7.1(c) — Phase D discovery HW-verified)
**Branch:** `v3`
**Roadmap reference:** [`roadmap.md` § v3.0](roadmap.md#v30--dual-backend-amd-native-added-alongside-c-path)

> Native compute dispatch verified end-to-end on AMD Cezanne (gfx90c)
> on 2026-04-28 — see
> [`docs/handoff/2026-04-28-session25-b4-verified.md`](../handoff/2026-04-28-session25-b4-verified.md).
> Backend abstraction layer + Phase B.4 follow-ups + multi-dispatch
> validation all landed. Native compute path is solid; **next big
> chunk is Phase C (textures + render)**, broken into bite-sized
> sub-steps below.

## Hard truths up front

Read these before sequencing.

- **WGSL → GFX9 ISA lowering is the single largest unknown.** Roadmap
  declares it in scope for v3.0; today the native path takes
  pre-compiled GFX9 bytes only. If lowering slips, v3.0's
  `examples/stdlib-consumer/` cannot run unmodified on `native` —
  which fails the exit criterion. Worth a sober design spike before
  committing to a v3.0 ship date.
- **Consumer cutover takes calendar time, not just code.** Even with
  all six consumers technically able to flip, "running native in
  production across a full release cycle" is what gates AMD wgpu
  retirement at v4.0. v3.0 ship just opens that window.
- **Tier 1 is multi-month.** Backend abstraction + Phase B.4
  follow-ups are done; Phase C native (texture + render) + Phase D
  native (surface) + WGSL lowering still ahead — bulk of remaining
  work. Don't try to swallow a phase whole; chunks below.
- **Linux + AMD only.** v3.0's native backend covers AMD on Linux.
  NVIDIA / Intel / macOS / Windows consumers continue on `wgpu`.
  Don't accept scope-creep that pretends otherwise.

## Tier 1 — Code completeness

### Backend abstraction layer ✅

Proposal: [`docs/proposals/v3-backend-interface.md`](../proposals/v3-backend-interface.md).

- [x] `src/backend.cyr` — 11-slot `Backend` struct + `BACKEND_KIND_*`
  constants + null-slot helpers + struct-layout assertions (Step 1)
- [x] `src/backend_wgpu.cyr` — fill all 11 slots with wgpu calls;
  existing wiring extracted into the slot wrappers (Step 2)
- [x] Refactor `src/buffer.cyr`, `src/compute.cyr`, `src/context.cyr`
  to dispatch through `ctx->backend->slot`; public API surface
  unchanged. `gpu_buffer_create / write / read / release`,
  `gpu_shader_module_create / release`, `gpu_compute_dispatch`,
  `gpu_device_wait_idle`, `gpu_context_release` all dispatch through
  the abstraction (Steps 3a–3g)
- [x] `MABDA_BACKEND_KIND` compile-time constant in `src/backend.cyr`
  (Step 4e)
- [x] `GpuContext` grew 32 → 40 → 48 → 96 bytes (Steps 3a, 4e, 4f.iv)
  — single greppable migration of every `alloc(N)` site
- [x] Smoke test: Cyrius-fn-to-Cyrius-fn `fncall` through a struct
  slot — `fncall1/2/3/5/6` all proven against real hardware (Steps 1, 3b–g)

### Native backend slot fills

#### Compute dispatch ✅

- [x] `Backend.compute_dispatch` on native — lifted into
  `native_compute_dispatch_cached` in `src/backend_native.cyr`
  (Steps 4a–4d, 4f.iv)
- [x] `gpu_context_new_native()` public bring-up entry — opens
  fd, allocates ctx_id + stub + cached IB + cached fence (Step 4e)
- [x] Multi-dispatch validated end-to-end on real hardware — same
  ctx, two back-to-back dispatches both pass (Step 5a)
- [x] `programs/native_compute_store.cyr` is the native-equivalent
  of `compute_e2e` (writes 0xDEADBEEF, verifies via CPU readback)

#### Phase C — texture path on native (broken into chunks)

The smallest first cut is a flat 2D RGBA8 BO with no tiling/no
mipmaps/no DCC. Format/tiling expansion comes later as consumers
need it.

- [x] **5.1** — `native_texture_create_2d_rgba8(fd, w, h, out)`
  primitive. Allocates GTT BO sized `w * h * 4`, va_maps at the
  texture VA range (e.g. `0xFFFF800100A00000`+), returns
  `(handle, va, addr, size)` packed in a 32-byte struct. CPU
  regression test: size formula, struct layout, VA-range
  isolation from existing IB / fence / shader / stub VAs.
- [x] **5.2** — `native_texture_release_2d_rgba8(fd, tex)`
  primitive. va_unmap + bo_release_gtt. Pair with 5.1.
- [x] **5.3** — Texture slot signatures designed in
  `docs/proposals/v3-backend-interface.md` revision: `texture_create`,
  `texture_write`, `texture_read`, `texture_release` slot signatures.
  Backend struct grows from 88 → 120 bytes (4 new slots).
- [x] **5.4** — `src/backend.cyr` updated with new slot offsets +
  layout asserts. CPU tests for new constants.
- [x] **5.5** — `_backend_wgpu_texture_*` slot wrappers around
  `wgpu_device_create_texture` / `wgpu_queue_write_texture` /
  copy-texture-to-buffer + map for read.
- [x] **5.6** — `_backend_native_texture_*` slot wrappers around
  the 5.1/5.2 primitives + a CPU-side memcpy for write/read
  (since GTT pages are CPU-accessible). No GPU-side pixel
  conversion yet.
- [x] **5.7** — Public `gpu_texture_create_2d_rgba8 / write / read
  / release` dispatchers in `src/texture.cyr`. Coexist with v2.x
  `texture_create_rgba8` which retires at v5.1.
- [x] **5.8** — Native texture round-trip end-to-end test:
  CPU-write pattern → GPU buffer → native texture (via copy or
  shader) → CPU-readback. Use `programs/native_compute_store.cyr`
  as the template; new program `programs/native_texture_e2e.cyr`.
- [x] **5.9** — Update `programs/phase0.cyr` Test 11 family to
  also exercise `gpu_texture_*` dispatchers under wgpu (mirrors
  the Step 3c `gpu_buffer_*` validation — proves the slot wires
  reach real wgpu without regressions).

#### Phase C — render pipeline + render pass on native (broken into chunks)

- [x] **6.1** — Design doc: GFX9 graphics pipeline state
  (vertex shader registers, fragment shader registers, rasterizer
  state, color/depth target setup). Pages from amdgpu kernel
  source + Mesa radv. Filed in
  `docs/proposals/v3-native-render-design.md`.
- [ ] **6.2** — Trivial GFX9 vertex+fragment shader pair
  (full-screen triangle, solid color output). **Real research
  chunk** — graphics shaders need GFX9 vertex/fragment ABI
  (SPI conventions, position/color exports), not the OpenCL
  compute kernel ABI that produced `native_gfx9_shader_store_deadbeef`.
  Path chosen: **hand-encode against GCN5 ISA spec** (path 1 from
  `docs/proposals/v3-shader-bytes-capture.md`).
  - [x] **6.2(a)** — spec-derived register addresses + bitfield
    minimums (12 R_SPI/CB constants, 6 RSRC1/RSRC2/format
    minimums, full-screen-triangle vertex count) in
    `src/backend_native.cyr` with 30 CPU value-asserts in
    `tests/tcyr/mabda_v3.tcyr` (Step 6.2(a), 2026-04-30). Capture
    protocol doc filed at
    `docs/proposals/v3-shader-bytes-capture.md`.
  - [x] **6.2(b) FS** — `native_gfx9_shader_solid_red` (92 bytes,
    4× v_mov + exp mrt0 done vm + s_endpgm + 16-NOP prefetch
    padding). Encoding spec-cited; 19 byte-asserts pin every
    dword. `GFX9_FS_SOLID_RED_SIZE = 92` exposed for Step 6.5's
    BO sizing. (Step 6.2(b) FS, 2026-04-30).
  - [x] **6.2(b) VS** — `native_gfx9_shader_fullscreen_triangle_vs`
    (116 bytes, standard radv `(vid&1)*4-1, (vid>>1)*4-1` pattern).
    VOP2 arithmetic encodings ground-truthed via `clang -target
    amdgcn--amdhsa -mcpu=gfx90c -O2` of an equivalent CL kernel
    (vid_to_pos.cl) + `llvm-objdump -d` — same Layer-1 protocol
    the compute shader used. Uses v0 (vertex_id) and v1 (clobbers
    instance_id) as scratch, keeps total VGPR count at 4 so RSRC1
    minimum is unchanged. 32 byte-asserts pin every dword.
    `GFX9_VS_FULLSCREEN_TRIANGLE_SIZE = 116` exposed for Step 6.5.
    (Step 6.2(b) VS, 2026-04-30).
- [x] **6.3** — `native_rt_create_2d_rgba8(fd, w, h, out)` +
  release primitive (Step 6.3 done 2026-04-28). NativeRenderTarget
  struct (32 bytes, same shape as NativeTexture). RT VA range at
  `0xFFFF800101000000`. Reuses Step 5.1's BO allocator pattern.
- [x] **6.4** — PM4 packets for graphics pipeline state setup
  (PA_SC_SCREEN_SCISSOR, PA_CL_VPORT_*, CB_TARGET_MASK, …). Build
  on top of existing PM4 helpers. Landed: `native_pm4_set_context_reg`
  + `_one` + `_pair` + `native_pm4_draw_index_auto` in
  `src/backend_native.cyr`. Synthetic-but-aligned addresses pending
  Step 6.5's radv-validated set.
- [ ] **6.5** — `native_pm4_build_render_clear_triangle(buf, …)` —
  PM4 stream that runs a vertex+fragment dispatch to clear a
  render target with a solid color. Mirrors
  `native_pm4_build_compute_store_deadbeef` shape.
  - [x] **6.5(a)** — clear-triangle PM4 register addresses + value
    constants. 38 new register addresses (pipeline-static ctx +
    pass-target + UConfig graphics) plus 12 simple value constants
    (target masks, blend / cull / clip defaults, primitive type,
    SPI hang-quirk minimum) in `src/backend_native.cyr`. Every
    address extracted from Mesa
    `src/amd/registers/gfx9.json` with a citation comment. 50 new
    CPU value-asserts in `tests/tcyr/mabda_v3.tcyr`. Surfaced and
    fixed a Step 6.2(a) bug along the way: the
    `SPI_SHADER_{POS,Z,COL}_FORMAT` triplet had been transcribed
    with scrambled values; corrected against authoritative Mesa
    output before HW test ran. (Step 6.5(a), 2026-04-30).
  - [x] **6.5(b)** — full PM4 stream composer
    (`native_pm4_build_render_clear_triangle`). Splits into
    pipeline_sh / pipeline_ctx / pass_target / draw_tail blocks
    per docs/proposals/v3-backend-interface.md v2.1. Helpers
    `native_int_to_f32_bits` + `native_f32_neg_bits` for viewport
    f32 conversion. Each block is its own fn for independent CPU
    testing. 75 new value-asserts pin every emitted dword. Bug
    catches during 6.5(b) implementation: PGM_HI mask was
    `& 0xFFFFFFFF` initially — corrected to `& 0xFF` to match
    compute precedent (line ~809) and avoid garbage in high reg
    bits; CB_COLOR_CONTROL design-doc value 0xCC was field-
    shorthand, real composed value is 0xCC0010 (MODE=CB_NORMAL +
    ROP3=COPY_SOURCE). Layer-2 byte-exact-vs-radv verification
    gated on Hyprland — Step 6.6 / 6.8c / 6.9b can build on this.
    (Step 6.5(b), 2026-04-30).
- [x] **6.6** — `native_render_dispatch_simple(ctx, …)` — GFX-ring
  analog of `native_compute_dispatch_cached`. Routes through the
  same `native_cs_submit_4chunk` Mesa-shape submit (per-context
  cached IB+fence + per-dispatch syncobj), differs in:
  `ip_type=AMDGPU_HW_IP_GFX` (was `_COMPUTE`); `ib_flags=0` (the PM4
  stream's ACQUIRE_MEM preamble handles cache invalidate, no kernel
  TC_WB hook needed); BO_HANDLES list = (fence/vs/fs/rt/ib) at
  priorities (1/4/4/3/10). Pairs with two CPU-testable helpers:
  `native_render_handles_write` (24-byte vs/fs/rt triple analog of
  `native_buf_pair_write`) and `native_render_bo_list_pack` (pulls
  the 5-entry residency build out of the inline shape so the priority
  story is unit-tested, not buried). 712 v3 (was 697 at 6.5(b) close)
  + 624 mabda CPU asserts green; smoke build + lint clean. The full
  syscall path is HW-gated — the e2e gate is 6.9(b)'s
  `programs/native_render_e2e.cyr`. (Step 6.6, 2026-04-30).
- [x] **6.7** — Backend interface render-pipeline / render-pass
  slots (proposal revision). Landed v2 expansion in
  `docs/proposals/v3-backend-interface.md`: 7 new slots
  (render_target_create + release, render_pipeline_create + release,
  render_pass_begin + draw + end), Backend struct grows 120 → 176
  bytes, append-after-kind preserved. All slots within fncall5
  ceiling. Code lands in Step 6.8.
- [ ] **6.8** — `_backend_wgpu_*` and `_backend_native_*` wrappers.
  - [x] **6.8 (a)** — pure layout extension in `src/backend.cyr`.
    7 new slot offset constants (RT create/release, pipeline
    create/release, pass begin/draw/end at +120..+168), bumped
    BACKEND_SIZE 120 → 176, added BACKEND_RENDER_SLOTS_BEGIN/END
    range constants. `backend_is_complete` deliberately defers v2
    range walk until 6.8b lands real wgpu wrappers (no half-state
    "complete with stubs" lie). Layout asserts in
    `tests/tcyr/mabda_v3.tcyr::test_backend_struct_layout` cover
    every new constant. 419 v3 + 624 mabda CPU tests green.
    (Step 6.8a, 2026-04-28).
  - [x] **6.8 (b)** — wgpu wrappers in `src/backend_wgpu.cyr`. 7
    new slot wrappers (RT create/release, pipeline create/release,
    pass begin/draw/end). `backend_wgpu_new()` installs all 21 slots.
    `backend_is_complete` extended to walk the v2 range. Pipeline
    descriptor builds inline using existing `_vertex_state_init` /
    `_primitive_state_init` / `_multisample_state_init` /
    `_color_target_state_new` / `_fragment_state_new` from
    `src/render_pipeline.cyr`. Pass struct (32 bytes) holds
    encoder + pass + rt back-reference. Pipeline struct (16 bytes)
    holds the WGPURenderPipeline handle. clear_color_ptr is a
    32-byte f64×4 RGBA block (mabda Color shape) — passed straight
    through to rpb_pass_color which calls color_write_f64. Tests:
    7 new layout asserts in test_backend_struct_layout (already in
    6.8a) + new `test_backend_wgpu_render_slot_identities` (7
    pointer-identity assertions) + extended
    `test_backend_is_complete_detects_missing_slot` to fill all
    three ranges + relaxed `test_backend_native_new_is_complete`
    to expect incomplete-with-v2-empty until 6.8c. Selective-include
    integration programs (compute_e2e, render_e2e) gained
    render_target / render_pipeline includes since backend_wgpu
    now references their constants. CPU sweep: 624 + 449 = 1073
    passing. All 7 GPU programs build clean. (Step 6.8b, 2026-04-28).
  - [x] **6.8 (c)** — native wrappers in `src/backend_native.cyr`.
    7 new slot wrappers (RT create/release, pipeline create/release,
    pass begin/draw/end). The 4-block PM4 split materializes as two
    structs: `NativeRenderPipeline` (320 B — header + pre-built 48 B
    `pipeline_sh_block` + pre-built 240 B `pipeline_ctx_block`) is
    packed once at `pipeline_create` via `native_render_pipeline_pack`;
    `NativePass` (32 B — ctx_ref + rt_ptr + clear_color_ptr) just
    stashes refs at `pass_begin` and defers PM4 emit to `pass_draw`.
    `pass_draw` composes the full IB (ACQUIRE_MEM preamble + 2
    UConfig + memcpy of pipeline_sh + memcpy of pipeline_ctx +
    `_pass_target` from RT extents + `_draw_tail` + NOP padding to
    256 dwords) and dispatches via Step 6.6's
    `native_render_dispatch_simple`. A byte-exact CPU test
    (`test_native_render_pipeline_pack_matches_composer`) asserts
    each cached block is dword-identical to a fresh composer
    invocation — so the memcpy fast path and the standalone composer
    can never silently diverge. Also extended `NativeRenderTarget`
    32 → 40 B to carry width/height (read by `pass_draw` to feed
    the pass_target composer; the RT is the source of truth for its
    own dimensions). v2-native limitations documented inline:
    single draw per pass, no state caching, `clear_color_ptr` ignored
    (FS shader hardcodes red), `color_fmt` ignored (RGBA8_UNORM only).
    `backend_native_new()` now wires all 21 slots — `backend_is_complete`
    returns 1 (was 0 with v2 range pending). 819 v3 (was 712 at 6.6
    close) + 624 mabda CPU asserts green; smoke + lint clean.
    (Step 6.8(c), 2026-04-30).
- [ ] **6.9** — Public dispatchers + `programs/native_render_e2e.cyr`
  (mirror of `programs/render_e2e.cyr`).
  - [x] **6.9 (a)** — public dispatchers landed. 7 ctx-aware
    `gpu_render_*` fns in `src/render_target.cyr`,
    `src/render_pipeline.cyr`, `src/render_pass.cyr` routing
    through `ctx->backend->render_*` slots. Coexist with v2.x
    `rtb_*` / `render_pipeline_create_simple` / `rpb_pass_*`
    helpers. 8 mock-backend CPU tests in
    `tests/tcyr/mabda_v3.tcyr` verify call threading + arg
    capture + null-ctx safety. Works on wgpu path immediately
    via 6.8b's slot wrappers; lights up on native path once
    6.8c lands. (Step 6.9a, 2026-04-28).
  - [x] **6.9 (b)** — `programs/native_render_e2e.cyr` mirroring
    `programs/render_e2e.cyr` on the native graphics ring. Drives
    the full 6.x chain end-to-end through the public 6.9(a)
    `gpu_render_*` API: native ctx → VS+FS shader BOs → RT create
    → pipeline create → pass begin/draw/end → CPU readback of
    pixel(0,0). RT BO is GTT-mapped linear, so readback is a
    direct `load8(rt_addr+0)` — no copy_texture_to_buffer dance.
    Builds clean (`make build/native_render_e2e` succeeds; full
    include chain links via `src/lib.cyr`); HW-gated (needs amdgpu
    + valid render-node fd; CI skip-if-no-DRM applies). The program
    documents three expected HW-time failure modes inline:
    post-draw cache flush missing (most likely — radv emits one,
    our 6.5(b) composer doesn't yet), TDR on GFX ring, pipeline
    state mis-encoding. Exit codes 0–11 map to specific failure
    classes for unattended runs. (Step 6.9(b), 2026-04-30).
- [x] **6.10 prep** — `EVENT_WRITE` / `CACHE_FLUSH_AND_INV` PM4
  packet builder. Stand-alone primitive (`native_pm4_event_write` +
  `native_pm4_event_write_cache_flush_and_inv` convenience wrapper)
  + four GFX9 event constants (`CACHE_FLUSH`, `CACHE_FLUSH_AND_INV`,
  `EVENT_INDEX_OTHER`, `EVENT_INDEX_TS`) cited from Mesa
  `gfx9.json`'s `VGT_EVENT_TYPE` enum. **Not yet wired into the
  render PM4 composer** — the splice into
  `native_pm4_build_render_clear_triangle` is gated on the first
  `make test-native-render-e2e` run on Cezanne. If pixel readback
  fails with the GTT 0x55 sentinel intact (Failure A documented in
  `programs/native_render_e2e.cyr`), the fix is one call append to
  the composer's draw block. If HW reveals a different failure
  class, the builder is still ready for Phase D surface present
  (which needs an end-of-frame flush) without rework. 5 CPU tests,
  17 asserts (binary form pinned + position-tracking composability +
  index packing). 836 v3 + 624 mabda CPU asserts green.
  (Step 6.10 prep, 2026-04-30).

#### Phase D — surface + present on native (broken into chunks)

- [ ] **7.1** — DRM/KMS device discovery: enumerate connectors,
  modes, encoders. `native_kms_init(fd)` returns a KmsState.
  Filed under `src/backend_native_kms.cyr` to keep it separate
  from the compute path. Three sub-bites:
  - [x] **7.1 (a)** — GetResources foundation. New module
    `src/backend_native_kms.cyr` (243 lines) wired into
    `src/lib.cyr` + `cyrius.cyml`. Implements
    `DRM_IOCTL_MODE_GETRESOURCES` (= `0xC04064A0`), the 64-byte
    `drm_mode_card_res` struct shape (mirrored from
    `uapi/drm/drm_mode.h`), and `native_kms_init(fd)` — the
    two-pass driver (count-only ioctl → array-fill ioctl) that
    returns a 96-byte `KmsState` holding fd, the 4 ID arrays
    (FB / CRTC / connector / encoder), and the framebuffer
    extent limits. `MODE_GETCONNECTOR` / `_GETENCODER` ioctl
    numbers + 13 short field accessors (`kms_state_fd` etc.)
    also exposed for sub-bites (b) + (c). Mode-set / present
    ioctls (`SETCRTC`, `PAGE_FLIP`) come in 7.2+. 5 CPU tests,
    51 asserts (ioctl numbers re-derived from first principles,
    struct/state field offsets, accessor round-trips, release
    safe-zero). 887 v3 (was 836 at 6.10-prep close) + 624 mabda
    CPU asserts green; smoke + lint + distlib clean. (Step
    7.1(a), 2026-04-30).
  - [x] **7.1 (b)** — DRM connector ioctl + struct shapes.
    Extends `src/backend_native_kms.cyr` with the
    `drm_mode_get_connector` struct (80 bytes; 16 fields pinned)
    + the `drm_mode_modeinfo` struct (68 bytes; 13 numeric fields
    + 32-byte name buffer at +36..+68) + the
    `DRM_MODE_CONNECTED/DISCONNECTED/UNKNOWN` connection-status
    enum + a 12-value `DRM_MODE_CONNECTOR_*` type enum (VGA / DVI
    / DP / HDMI / eDP / Virtual / DSI / USB) covering modern
    desktop + laptop hardware. Low-level
    `native_drm_mode_get_connector(fd, req)` ioctl helper.
    Higher-level "discover all connectors" driver deferred to
    7.1(c) where it composes naturally with encoder discovery
    + the topology summary. Phase D tests split into
    `tests/tcyr/mabda_v3_phase_d.tcyr` (new file; 100 asserts
    across 9 tests) so `mabda_v3.tcyr` stays under cyrlint's 128
    KiB cap. 4 new tests cover: connector field offsets (16
    asserts), full modeinfo field offsets (18 asserts including
    the name+name_len → size sanity check), connection-status
    constants (3), connector-type constants (12). 836 v3 + 100
    phase_d + 624 mabda CPU asserts green; smoke + lint +
    distlib clean. (Step 7.1(b), 2026-04-30).
  - [x] **7.1 (c)** — encoder ioctl + `native_kms_summary`
    topology printer + runnable diagnostic. `drm_mode_get_encoder`
    struct (20 B; 5 fields), 9-value `DRM_MODE_ENCODER_*` enum
    (NONE/DAC/TMDS/LVDS/TVDAC/Virtual/DSI/DP-MST/DPI),
    `native_drm_mode_get_encoder` ioctl helper. Three name-lookup
    helpers (`native_drm_connector_type_name`,
    `native_drm_encoder_type_name`,
    `native_drm_connection_status_name`) return cstr labels for
    use in any future diagnostic / summary output.
    `native_kms_summary(state)` walks the connector + encoder ID
    arrays from a populated KmsState and prints one line per
    resource to stdout (id / type-name / status / current-CRTC /
    possible-CRTCs). New runnable diagnostic
    `programs/native_kms_summary.cyr` (with `card0..card9` scan
    so node renumbering doesn't matter) opens a DRM master fd,
    initializes a KmsState, prints the framebuffer extent
    summary, then runs `native_kms_summary`. **Verified live on
    Cezanne 2026-04-30**: 4 connectors (1 HDMI-A connected to
    CRTC 87, 3 DP disconnected) + 8 encoders (4 TMDS + 4 DP-MST;
    poss_crtcs = 0xF on all). Phase D discovery is now HW-verified
    end-to-end. 6 new CPU tests, 31 asserts in mabda_v3_phase_d.tcyr
    (encoder field offsets, encoder-type enum, three name-lookup
    fns by first-byte spot-check, summary null-safe). 836 v3 +
    131 phase_d + 624 mabda CPU asserts green; smoke + lint +
    distlib clean. Closes Phase D discovery; foundation for
    7.2's mode-pick logic. (Step 7.1(c), 2026-04-30).
- [ ] **7.2** — Mode-set: pick a default mode (highest-rated CRTC
  for the first connected DP/HDMI), set it.
- [ ] **7.3** — Framebuffer creation: KMS-side wrapping of a
  GTT BO as a scanout surface.
- [ ] **7.4** — Page flip + vblank: `drmModePageFlip` analog
  through direct ioctl; vblank wait via `drmWaitVBlank` analog.
- [ ] **7.5** — Backend interface surface slots
  (`surface_configure / acquire / present`).
- [ ] **7.6** — `_backend_wgpu_surface_*` + `_backend_native_surface_*`
  wrappers.
- [ ] **7.7** — Public dispatchers + `programs/native_present_e2e.cyr`
  (open a window, render a clear, present, hold for 1s, exit).

#### WGSL → GFX9 ISA lowering (broken into chunks)

This is the v3.0 hard truth. Scoping carefully.

- [ ] **8.1** — Frontend choice: WGSL parser vs SPIR-V loader.
  Design doc + decision in `docs/proposals/v3-shader-lowering.md`.
  Prefer SPIR-V (well-specified, existing parsers) if mabda
  consumers can ship SPIR-V; WGSL is the customer-facing format
  but transpiling WGSL→SPIR-V is a separate problem with existing
  tools (Naga, Tint).
- [ ] **8.2** — IR design: tagged-union AST for the subset
  needed by Phase 1 (compute kernels with global stores, no
  textures, no atomics).
- [ ] **8.3** — GFX9 instruction encoder library:
  `src/gfx9_isa.cyr` with builders for SMEM / VMEM / SOPP / VOP1
  / VOP2 / VOP3 / VINTRP categories.
- [ ] **8.4** — Register allocator: linear-scan over SGPRs +
  VGPRs. Spill to scratch deferred (large-shader follow-up).
- [ ] **8.5** — End-to-end smoke: hand-built IR for "write
  0xDEADBEEF to s[0:1]" → runs through encoder → produces bytes
  that match the existing `native_gfx9_shader_store_deadbeef`
  byte-for-byte.
- [ ] **8.6** — Storage-buffer reads + writes (`global_load_*`,
  `global_store_*`).
- [ ] **8.7** — User-data binding mapping: WGSL/SPIR-V binding
  points → USER_DATA_0..3 SGPR slots.
- [ ] **8.8** — Workgroup size > 1 support (NUM_THREAD_X/Y/Z).
- [ ] **8.9** — `gpu_shader_module_create` accepts source bytes
  on native and runs them through the lowering. WGSL or SPIR-V
  per the 8.1 decision.
- [ ] **8.10** — Re-generate `native_gfx9_shader_store_deadbeef`
  from WGSL/SPIR-V source via the lowering. Replaces the hand-
  authored bytes; CI check that the lowered bytes match the
  hand-authored bytes byte-for-byte.

#### Phase B.4 follow-ups ✅

- [x] Tighten BO page perms — `_NATIVE_PERM_SHADER` (R|X),
  `_NATIVE_PERM_DATA` (R|W), `_NATIVE_PERM_IB` (R|X — X bit is
  load-bearing on Cezanne; surprise) (Step 4f.ii)
- [x] Memory-model design — `FrameResources` is now ctx-aware,
  `frame_resources_release_buffers` dispatches through
  `ctx->backend->buffer_release` (Step 4f.iii)
- [x] Error-model mapping — `_native_errno_to_gpu_err`,
  `_native_neg_rc_to_gpu_err`. Native fails through this helper
  for ENOMEM/EPERM/EACCES/ENOENT/EBUSY/ETIMEDOUT/ETIME (Step 4f.i)
- [x] BO ownership lifecycle — per-context cached IB + fence BOs
  (Step 4f.iv); 7 syscalls of churn per dispatch eliminated. Multi-
  dispatch validated (Step 5a). **Open follow-up:** BO list still
  hardcoded to 5 entries; multi-buffer dispatches need a
  parameterized residency list — file as Phase C / Tier 1
  follow-up under one of the 5.x or 6.x sub-steps.

## Tier 2 — Integration & regression

- [ ] **`programs/diagnostics/radv_capture/`** — headless Vulkan
  capture program. Phase 1: hardcoded for the Step 6.5
  clear-triangle, produces an `RADV_DEBUG=hang` IB dump for the
  Layer-2 byte-diff that gates "claim 6.5 done." Phase 2:
  parameterized for arbitrary draws (width/height/shader/vcount),
  becomes the golden-image diff harness. Phase 3: cross-consumer
  adoption — soorat / rasa / aethersafta / kiran land regression
  tests against it. Decision (2026-04-30): build this as a
  first-class deliverable rather than relying on
  `vkcube + native Hyprland` alone. Long-term payoff is UI testing
  across the AGNOS consumers, not just mabda's PM4 verification.
  See session 26 handoff for the suggested phasing.
- [ ] `examples/stdlib-consumer/` builds and runs under **both**
  backends on AMD. Phase-3 of the harness above lights this up
  as a real cross-backend pixel-identity check.
- [ ] Six-consumer regression sweep — soorat / rasa / ranga / bijli /
  aethersafta / kiran all build and run on `wgpu` (default
  unchanged); zero behavioural changes.
- [ ] At least one consumer (likely soorat or compute-heavy bijli)
  runs a CI matrix entry under `native` on AMD hardware.
- [x] CPU assertions still pass — 624 (mabda.tcyr) + 697
  (mabda_v3.tcyr) = **1321 passing as of session 26 close**, all
  backend-agnostic, no regressions throughout backend abstraction
  / texture / render PM4 composer work.
- [ ] All 13 GPU benches pass under `native` on AMD (today they
  only run under `wgpu`).

## Tier 3 — Performance evidence (the v3.0 "story")

The whole reason v3.0 is *dual* and not *swap* is the bench matrix —
without it, v3.0 has no measurement story.

- [ ] `bench-history.csv` schema — add `backend` column
  (forward-compatible; existing rows get `wgpu` retroactively)
- [ ] `make bench-gpu` runs the 13-bench suite under each backend
  on AMD
- [ ] Run all four matrix cells: 13 benches × {wgpu, native} ×
  {pre-5.6.x, post-5.6.x}
- [ ] `docs/benchmarks-rust-v-cyrius.md` refreshed to tell the
  four-quadrant story (cyrius codegen wins isolated from backend
  architecture wins)
- [ ] FFI overhead per-call number landed — the C-launcher cost the
  native path eliminates, with method
- [ ] Submit-to-completion timing for `Backend.compute_dispatch`
  integrated through `src/profiler.cyr` instead of the hand-rolled
  `mono_ns()` print in `native_compute_store`
- [ ] Honest perf framing — per the
  `feedback_honest_perf_framing` memory: report numbers as
  scale-labeled baselines, not "parity with X" until Phase D + 5.6.x
  give fair comparisons

## Tier 4 — Documentation

- [ ] `CLAUDE.md` updated for v3 architecture — currently still
  describes v2.5.0 single-backend; needs Backend layer + AMD-only
  native scope language
- [ ] `docs/stdlib-integration.md` — backend selector documented
  (consumer-facing)
- [ ] Consumer migration guide (1-pager) — how to flip an existing
  v2.x consumer to `BACKEND_KIND_AMD`
- [ ] `@public` surface audit per ADR 005 — confirm no `Backend`
  types leak through the boundary; ADR 005 audit script (if it
  exists) clean
- [ ] All 6 ADRs cross-referenced consistently — pass through ADR
  001-006 and check links / "Related" lines
- [ ] `CHANGELOG` `[3.0.0-dev]` running diary becomes a single dated
  `[3.0.0] — YYYY-MM-DD` section with the full delta vs 2.5.0
- [ ] `programs/native_compute_store.cyr` diagnostic scaffolding
  (`0xC0FFEE12` marker, IB hex dump, mono_ns timing) — decision: keep
  in-tree as a diagnostic program, move to a `programs/diagnostics/`
  subdir, or strip down to a minimal example. Pick one.

## Tier 5 — Release engineering (CLAUDE.md "Closeout Pass")

Ordered roughly the way they'll need to run.

- [ ] **P(-1) audit pass** — last one was
  [`docs/audit/2026-04-19-audit.md`](../audit/2026-04-19-audit.md)
  against 2.2.0. Required before any minor/major bump per CLAUDE.md.
  v3.0 audit will need to cover specifically: PM4 emitter input
  validation, every `syscall()` return check in
  `src/backend_native.cyr`, integer overflow on workgroup math,
  BO-perm tightening, label-string handling, `fncall*` discipline
  on the new abstraction layer.
- [ ] `VERSION` bump `2.5.0` → `3.0.0` + `cyrius.cyml` cross-check
  via `scripts/version-check.sh`
- [x] Toolchain pin decision — `cyrius = "5.7.28"` in `cyrius.cyml`
  as of 2026-04-28 (bumped during Step 4f.iii). Confirm at release
  time whether to bump again or pin here.
- [ ] `cyrius distlib` regenerate — `dist/mabda.cyr` will grow
  significantly (Backend layer + both backend implementations); CI
  gate must not drift
- [ ] Lint / fmt / vet clean across the **whole repo** (not just
  touched files) — `cyrius lint src/*.cyr programs/*.cyr`,
  `cyrius fmt --check`, `cyrius vet programs/smoke.cyr`
- [x] **Split `tests/tcyr/mabda.tcyr`** into multiple smaller files
  (Cleanup 1, 2026-04-28). `mabda.tcyr` for v2.x (624 assertions);
  `mabda_v3.tcyr` for v3 backend-abstraction tests (227 assertions).
  `Makefile` test target + CI workflows (`ci.yml`, `release.yml`)
  run both. Lint clean on both files.
- [ ] `.github/workflows/release.yml` tag filter + version-verify
  still work against the `v3.0.0` shape
- [ ] Soak window — run the new bundle in CI for N days (suggest
  ≥7) against the consumer integration tests without regression
- [ ] Pre-release: tag `v3.0.0-rc.1`; let consumers smoke-test
  before the final tag
- [ ] After ship: update `CHANGELOG`'s `[Unreleased]` section to
  empty; archive v3 punch list as
  `docs/development/archive/3-0-punchlist.md`

## Tier 6 — Forward tracking (file now, work later)

These keep the v4.0 / v5.0 commitments visible from the v3.0 ship.

- [ ] **ADR 007 placeholder** for NVIDIA native (v4.0). Status:
  `Deferred to v4.0`. Single page; documents the
  nouveau-vs-nvidia.ko design-spike question, SASS/PTX choice. The
  file's existence keeps the commitment visible from the v3.0 ship.
- [ ] (Optional) ADR 008 placeholder for Intel native (v5.0) — same
  shape. Could also wait until v5.0 design opens.
- [ ] Issue tracker entries for known v3.0-deferred items:
  parameterized BO residency list, multi-buffer dispatches,
  `Resource` tracker texture-release once Phase C lands. Anything
  that the audit flags but doesn't block ship.

## Recommended sequencing (refreshed 2026-04-28)

The smallest-bites-first order. Each step is verifiable end-to-end
before the next. **Steps 1–4 done; we're at the start of Step 5.**

1. ~~Backend abstraction smoke test~~ ✅
2. ~~`backend_wgpu.cyr` filling all slots + refactor public API~~ ✅
3. ~~Lift `native_compute_store` into `Backend.compute_dispatch`~~ ✅
4. ~~Phase B.4 follow-ups (BO perms, error mapping, IB ring,
   Resource tracker)~~ ✅
5. **Phase C native (texture + render pipeline)** — chunks 5.1–5.9
   (texture) then 6.1–6.9 (render pipeline). At least 18 sub-steps;
   budget weeks. Start with 5.1 (texture-create primitive) — small
   bite that establishes the BO allocation pattern.
6. **Phase D native (surface)** — chunks 7.1–7.7. Gated on Phase C
   render path landing.
7. **WGSL → GFX9 lowering** — chunks 8.1–8.10. Can be parallel to
   Phase C/D once the SPIR-V vs WGSL frontend choice (8.1) is made.
   May want to spike 8.1 alongside 5.x to surface design risk early.
8. **Bench harness + matrix + perf docs** — Tier 3, once all four
   programs pass under both backends.
9. **Tier 4 documentation** — runs alongside everything; finalize
   at end.
10. **Tier 5 release engineering** — P(-1) audit, version bump,
    distlib regen, soak, RC, ship.

## Status snapshot (refreshed 2026-04-30)

| Tier | Status | Last touched |
|------|--------|-------------|
| Tier 1 — Backend abstraction | ✅ done (Steps 1–3g, 4a–4e) | 2026-04-28 |
| Tier 1 — Phase B.4 follow-ups | ✅ done (Steps 4f.i–iv, 5a) | 2026-04-28 |
| Tier 1 — Phase C texture (5.1–5.9) | ✅ done | 2026-04-28 |
| Tier 1 — Phase C render (6.x) | **code-complete** — 6.1–6.9 all landed (6.9(b) builds clean, HW-gated to run); 6.5 Layer-2 verify + post-draw cache flush gated on Hyprland or headless capture program | 2026-04-30 |
| Tier 1 — Phase D surface (7.x) | partial — **7.1 discovery code-complete + HW-verified**; 7.2–7.7 (modeset → present) ahead | 2026-04-30 |
| Tier 1 — WGSL lowering (8.x) | ⬜ not started — chunks 8.1–8.10 queued | — |
| Tier 2 — Integration & regression | partial — CPU 624 mabda + 697 v3 = 1321 pass; GPU 32 untouched; consumer sweep pending | 2026-04-30 |
| Tier 3 — Performance evidence | ⬜ not started | — |
| Tier 4 — Documentation | partial — session 26 handoff filed; CHANGELOG `[3.0.0-dev]` updated; CLAUDE.md / migration guide / vidya field-notes pending | 2026-04-30 |
| Tier 5 — Release engineering | partial — toolchain pin 5.7.36 + test split + dist regenerate (clean post-distlib-fix) landed; rest pending | 2026-04-30 |
| Tier 6 — Forward tracking | ⬜ not started | — |

## Notes / decisions captured along the way

(Append-only log; date each entry. Decisions made during the
punch-list work that future-you should know about.)

- **2026-04-28** — Punch list opened. Phase B.4 verified on AMD
  Cezanne; native compute dispatch foundation in place. Backend
  interface proposal drafted (`docs/proposals/v3-backend-interface.md`).
- **2026-04-28** — Backend abstraction landed end-to-end (Steps 1–3g).
  Cyrius-fn-via-fnptr-via-struct-slot dispatch validated for `fncall1`,
  `fncall2`, `fncall3`, `fncall5`, `fncall6` against real wgpu hardware.
  The `feedback_fncall6_wgpu` memory's concern was about extern-C ABI
  mismatch; Cyrius-to-Cyrius works fine.
- **2026-04-28** — Native Backend slot wiring complete (Steps 4a–4d).
  `programs/native_compute_store.cyr` shrunk from 446 → 244 lines.
- **2026-04-28** — `MABDA_BACKEND_KIND` constant + `gpu_context_new_native`
  entry point landed (Step 4e). GpuContext now 48 → 96 bytes (Step 4f.iv).
- **2026-04-28** — Phase B.4 follow-ups all done (Steps 4f.i–iv).
  `programs/native_compute_store.cyr` final size: 244 lines.
- **2026-04-28** — Cyrius global init-order bug discovered during
  Step 4f.ii. Filed as
  `docs/development/issues/2026-04-28-cyrius-global-init-order.md`
  and saved as memory `feedback_cyrius_global_init_order.md`.
  CPU regression test pattern for computed constants pays for itself.
- **2026-04-28** — `cyrius lint` file-size threshold bug filed as
  `docs/development/issues/2026-04-28-cyrlint-multi-line-assert.md`.
  Workaround: split big test files (`mabda.tcyr` + `mabda_v3.tcyr`).
- **2026-04-28** — `cyim 1.1.4` regex commands fail mid-session. Filed
  as `docs/development/issues/2026-04-28-cyim-regex-pattern-error.md`.
  Workaround: use `cyim --batch` with NUL-separated pairs from Python
  heredocs; `cyim --write` for full-file overwrites.
- **2026-04-28** — Toolchain bumped to `cyrius 5.7.28` (Step 4f.iii).
- **2026-04-28** — Multi-dispatch validated (Step 5a). Cached IB +
  fence BOs survive back-to-back submits without state leakage. Two
  dispatches per `make test-native-compute-store` run, both pass at
  0 ms signal.
- **2026-04-28** — Step 6.4 (PM4 SET_CONTEXT_REG + DRAW_INDEX_AUTO
  builders) landed. Step 6.7 (Backend interface v2 expansion) landed
  doc-only in `docs/proposals/v3-backend-interface.md`: 7 new render
  slots, Backend struct 120 → 176 bytes, append-after-kind preserved.
  Vidya field-notes refreshed: 12 entries in `mabda-v3-gpu.cyml`
  (was 6), 24 entries in `language.cyml` (was 22), index updated.
- **2026-04-28** — Pit-stop research pass before Step 6.8 code
  landed. Two research agents verified v2 design against
  webgpu-native v29 headers + Mesa radv source. Caught two
  draft-time bugs: clear-color packed as u32 (must be 4×f64
  pointer per WGPUColor), and "encode pipeline state once at
  pipeline_create" (must split into pipeline_sh + pipeline_ctx +
  pass_target + draw_tail per radv ctx_cs/cs cut). Proposal
  bumped to v2.1; vidya entries
  `gfx9_pipeline_pm4_three_block_split` +
  `wgpucolor_is_4_doubles_not_a_packed_u32` capture the lessons
  for future native-PM4 work.
- **2026-04-28** — Step 6.8a landed (pure layout): 7 new slot
  offset constants in `src/backend.cyr`, BACKEND_SIZE 120 → 176,
  BACKEND_RENDER_SLOTS_BEGIN/END range. backend_is_complete
  deliberately defers v2 walk until 6.8b — no half-state "complete
  with stubs" lie. test_backend_struct_layout grew to assert every
  new offset; 419 v3 + 624 mabda tests green.
- **2026-04-28** — Step 6.8b landed (wgpu wrappers): 7 wgpu render
  wrappers in `src/backend_wgpu.cyr` reusing the existing descriptor
  helpers from render_pipeline.cyr / render_pass.cyr / render_target.cyr.
  `backend_wgpu_new()` installs all 21 slots; backend_is_complete
  walks all three ranges. Pipeline struct = 16 bytes (handle); Pass
  struct = 32 bytes (encoder + pass + rt). Native test relaxed to
  expect incomplete-v2-pending until 6.8c. compute_e2e + render_e2e
  gained render_pipeline / render_target includes since backend_wgpu
  now references their helpers. 1073 CPU tests pass; 7 GPU programs
  build clean.
- **2026-04-28** — Step 6.9a landed (public render dispatchers): 7
  ctx-aware `gpu_render_*` fns in render_target.cyr / render_pipeline.cyr
  / render_pass.cyr routing through ctx->backend slots. 8 new
  mock-backend dispatch tests (1108 CPU tests pass: 624 mabda + 484
  mabda_v3). Public render API now lives behind the abstraction —
  consumers can call gpu_render_pipeline_create / gpu_render_pass_begin
  / etc. and the backend (wgpu today, native at 6.8c) handles the
  rest. v2.x rtb_* / render_pipeline_create_simple / rpb_pass_* helpers
  stay in place for code that builds descriptors directly.
- **2026-04-28** — Step 6.2 attempted, surfaced as gated-on-decision.
  Research agent produced the spec-derived register state + offsets +
  shader-shape spec. Live IB capture path doesn't work on Mesa 26.0.5
  on this dev box (Arch base, no DRI3, no display) — RADV_DEBUG=hang
  only fires on hang, dumpibs is silent, RGP needs a swapchain. The
  existing compute shader bytes were derived via clang+AMDHSA, not
  from a live IB dump (correcting the framing in
  `feedback_pm4_verify_against_mesa_ib`). Three viable derivation
  paths for 6.2(b) documented in
  `docs/handoff/2026-04-28-session25c-punch-list-march.md`. 6.2(a) —
  the spec-derived register state + offset constants + capture-protocol
  doc — is unblocked and queued as the next bite. Session paused for
  user rest; handoff doc captures the decision-needed state.
- **2026-04-28** — Followup fix discovered (uncommitted at session
  pause): Step 6.9a's `render_target.cyr` addition to compute_e2e /
  render_e2e surfaces a transitive dependency on `depth.cyr`
  (rtb_build references depth_texture_*). Phase0 also gained the
  same dep via the v2 backend wrappers needing render_pass.cyr.
  3-file fix queued — handoff doc has the suggested commit message.
  **Lesson:** `cyrius build <prog>` ≠ `make build/<prog>` — only the
  Makefile path runs the GCC link that catches transitive deps.
