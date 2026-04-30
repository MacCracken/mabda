# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

Change categories in use: **Added**, **Changed**, **Deprecated**,
**Removed**, **Fixed**, **Security** (Keep a Changelog standard) plus
**Breaking** when a change is incompatible at the public-API level
(always accompanied by a migration note), **Unblocked** for
toolchain-side items that became viable mid-cycle, **Metrics** for
numeric deltas (module count, assertions, bundle size), and **Next**
for the immediate forward pointer.

## [Unreleased]

Nothing staged yet. File changes under a dated `## [X.Y.Z] — YYYY-MM-DD`
section when they ship.

## [3.0.0-dev]

Pre-release dev-track entries for the v3 native-backend work. No
release date; individual items are dated inline when they land.

### Added — 2026-04-30 (Session 26 — Steps 6.2 + 6.5 native render PM4 composer)

The native render path's CPU-testable structure is now end-to-end in
tree. Register addresses + minimums, VS+FS shader bytes, and the full
clear-triangle PM4 stream composer all land with by-construction CPU
asserts pinning every emitted byte. Layer-2 byte-exact verification
against radv-IB capture remains gated on Hyprland.

- **Step 6.2(a) — graphics-pipeline register addresses + minimums.**
  12 SH/context register addresses (VS+PS PGM_LO/HI/RSRC1/RSRC2 + SPI
  shader format outputs + CB_TARGET_MASK) and 6 spec-derived field
  minimums (RSRC1/RSRC2/format/vcount). 30 CPU value-asserts in
  `tests/tcyr/mabda_v3.tcyr`. Capture-protocol doc filed at
  `docs/proposals/v3-shader-bytes-capture.md` (298 lines).
- **Step 6.2(b) FS — `native_gfx9_shader_solid_red`.** 92-byte
  hand-encoded fragment shader emitting solid red via 4× `v_mov_b32`
  + `exp mrt0 done vm` + `s_endpgm` + 16-NOP prefetch padding. Each
  dword spec-cited from the GFX9/GCN5 ISA spec. 26 byte-asserts.
  `GFX9_FS_SOLID_RED_SIZE = 92` exposed for Step 6.5 BO sizing.
- **Step 6.2(b) VS — `native_gfx9_shader_fullscreen_triangle_vs`.**
  116-byte VS computing NDC fullscreen triangle from system VGPR v0
  via the standard radv pattern `(vid&1)*4-1, (vid>>1)*4-1`. VOP2
  arithmetic encodings cross-checked against
  `clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -nogpulib`
  disassembly (`llvm-objdump -d`) of an equivalent CL kernel — same
  Layer-1 protocol the compute shader used. 32 byte-asserts.
  `GFX9_VS_FULLSCREEN_TRIANGLE_SIZE = 116` exposed for Step 6.5.
- **Step 6.5(a) — clear-triangle PM4 register addresses + value
  constants.** 38 new register addresses (15 pipeline-static ctx +
  21 pass-target + 2 UConfig graphics) and 12 simple value constants
  (target masks, blend / cull / clip defaults, primitive type, SPI
  hang-quirk minimum, DB defaults). Every address extracted from
  authoritative Mesa
  `https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/amd/registers/gfx9.json`
  with citation comment. 50 CPU value-asserts. Plus 3 composed
  bit-pattern values (`CB_COLOR_CONTROL_NORMAL_COPY = 0xCC0010`,
  `CB_COLOR0_INFO_RGBA8_UNORM = 0x28`,
  `CB_COLOR0_ATTRIB_2D_LINEAR_RGBA8 = 0x10000000`).
- **Step 6.5(b) — PM4 stream composer for clear-triangle dispatch.**
  Four-block split per `docs/proposals/v3-backend-interface.md` v2.1:
  `native_pm4_build_render_pipeline_sh` (VS+PS shader VAs + RSRC,
  48 B), `_pipeline_ctx` (mode-static state, 240 B),
  `_pass_target` (RT bind + viewport + scissor, 152 B),
  `_draw_tail` (DRAW_INDEX_AUTO, 12 B). Top-level
  `native_pm4_build_render_clear_triangle(buf, vs_va, fs_va, rt_va,
  rt_width, rt_height)` composes the four blocks + ACQUIRE_MEM
  preamble + UConfig graphics + NOP padding to 256-dword alignment
  (1024 B total). Helpers `native_int_to_f32_bits` +
  `native_f32_neg_bits` for viewport f32 conversion (route through
  stdlib `f64_from` and color.cyr `f64_to_f32`). 75 CPU asserts
  exercise int↔f32 helpers + each block in isolation + the top-level
  composer.

### Fixed — 2026-04-30 (catches during Steps 6.2 + 6.5)

- **`SPI_SHADER_{POS,Z,COL}_FORMAT` addresses scrambled in 6.2(a).**
  Initial 6.2(a) shipped POS=0xA710, Z=0xA708, COL=0xA70C. Authoritative
  Mesa gfx9.json: POS=0xA70C, Z=0xA710, COL=0xA714. Caught during
  6.5(a) cross-check before any HW test ran. Lesson now codified in
  `docs/proposals/v3-shader-bytes-capture.md` § "Authoritative source
  for register addresses": every `R_*` constant must cite its
  gfx9.json `map.at` value.
- **PGM_HI mask in 6.5(b) `pipeline_sh` block.** Initial composer used
  `(va >> 40) & 0xFFFFFFFF`; correct mask matches compute pattern
  (line ~809) — `& 0xFF` (8 bits, bits 40-47 of VA). Cyrius's `>>`
  on i64 is logical (no sign-extend), so the bug was producing
  garbage in high bits of a 32-bit register that hardware would
  ignore but cluttered the IB.
- **CB_COLOR_CONTROL design-doc value 0xCC was field-shorthand.**
  `docs/proposals/v3-native-render-design.md` listed 0xCC for "NORMAL
  ROP". The bare 0xCC interpreted as the encoded register would set
  DEGAMMA_ENABLE bit 3 and put MODE=4 (CB_DECOMPRESS). Authoritative
  composition: MODE=CB_NORMAL (1) << 4 = 0x10, ROP3=0xCC << 16 =
  0xCC0000, union = 0xCC0010. Now exposed as
  `GFX9_CB_COLOR_CONTROL_NORMAL_COPY`.

### Unblocked — 2026-04-30 (toolchain + tooling)

- **Cyrius distlib 64-KB-per-module truncation fixed upstream in
  v5.7.36.** `src/backend_native.cyr` grew past 64 KB during the
  6.2 work. Cyrius distlib's read buffer (`alloc(65536) /
  file_read_all(..., 65535)` in `cbt/commands.cyr:894`) silently
  truncated, producing a bundle missing every fn defined past
  byte 65535. Fixed upstream — buffer raised 64 KB → 256 KB. Mabda
  pin moved 5.5.20 → 5.7.35 → 5.7.36 across the session.
- **Cyrius lint/fmt now per-file (5.7.x CLI shape change).** Bare
  `cyrius lint` / `cyrius fmt --check` (repo-wide) is gone — both
  require an explicit file argument. Project-wide form for downstream
  consumers is now a shell loop:
  `for f in src/*.cyr; do cyrius lint "$f" || exit 1; done`. CLAUDE.md
  P(-1) Scaffold Hardening section still describes the bare form;
  flag for Tier 4 doc update.
- **clang + LLVM AMDGPU installed on the dev box.** `clang 22.1.3` +
  `llvm-objdump` + `llvm-mc`. Unblocks Layer-1 cross-check protocol
  (hand-encoded shader bytes vs `clang -target amdgcn--amdhsa
  -mcpu=gfx90c -O2 -nogpulib` disassembly). Used for the 6.2(b) VS
  arithmetic VOP2 encodings.

### Methodology note — 2026-04-30 (ground-truth-everything)

The session's bug catches (scrambled SPI addresses, PGM_HI mask,
CB_COLOR_CONTROL value, six test arithmetic errors) all share a
shape: speculation-from-design-doc that didn't survive a check
against the authoritative source. The protocol now reflected in
`docs/proposals/v3-shader-bytes-capture.md`:

- Layer 1 (shader ISA bytes) — clang `amdgcn--amdhsa` + `llvm-objdump`
  is the cross-check tool. Reference CL kernel preserved in the proposal
  doc for re-verification.
- Layer 2 (PM4 packet stream) — `RADV_DEBUG=hang` IB dump from a vkcube
  clear-triangle is the cross-check. **Currently gated on Hyprland**
  (the dev box is Arch base, no DRI3, vkcube needs a presentation
  surface). Once unblocked, run on the existing
  `native_pm4_build_render_clear_triangle` output to claim Step 6.5
  done.
- Register addresses — Mesa
  `https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/amd/registers/gfx9.json`
  is the source of truth. Lookup script captured inline in the
  proposal doc. Every `R_*` constant in `src/backend_native.cyr`
  carries a `# Mesa 0x028XYZ` citation comment.

### Added — 2026-04-30 (Step 6.6 — render-ring dispatch)

`native_render_dispatch_simple` lands as the GFX-ring analog of
`native_compute_dispatch_cached`. Routes through the same Mesa-shape
4-chunk submit (BO_HANDLES + SYNCOBJ_OUT + FENCE + IB) on the same
per-context cached IB+fence, with a per-dispatch syncobj. Three
deltas vs the compute path:

- `ip_type = AMDGPU_HW_IP_GFX` (was `_COMPUTE`) selects the graphics
  ring.
- `ib_flags = 0` (compute uses `0x08` /
  `AMDGPU_IB_FLAG_TC_WB_NOT_INVALIDATE`). The render PM4 stream's
  ACQUIRE_MEM preamble (block 0 of
  `native_pm4_build_render_clear_triangle`) does its own cache
  invalidate; we don't need the kernel-side TC writeback hook that
  the compute flag toggles.
- BO_HANDLES list shape: `(fence / vs / fs / rt / ib)` at residency
  priorities `(1 / 4 / 4 / 3 / 10)`. Compute is
  `(fence / stub / out / shader / ib)` at `(1 / 3 / 3 / 4 / 10)` —
  shader prio (4) goes to both VS and FS, target prio (3) goes to
  the RT, the compute stub drops out.

Two CPU-testable helpers split out of the inline shape:
`native_render_handles_write` (24-byte vs/fs/rt triple, analog of
`native_buf_pair_write`) and `native_render_bo_list_pack` (5-entry
40-byte residency list builder). 15 new CPU asserts in
`tests/tcyr/mabda_v3.tcyr` pin both layouts and guard against an
accidental "same shape as compute" refactor.

The full submit + syscall path is HW-gated — the e2e gate is
6.9(b)'s `programs/native_render_e2e.cyr`. 712 v3 (was 697) +
624 mabda CPU asserts green; smoke + lint clean.

### Added — 2026-04-30 (Step 6.8(c) — native render slot wrappers)

The 4-block PM4 split from `docs/proposals/v3-backend-interface.md`
v2.1 lands in code: two new structs (`NativeRenderPipeline`,
`NativePass`) plus seven slot wrappers wire the public render API
through to the native graphics ring.

- **`NativeRenderPipeline`** (320 B): header (vs+fs handle/va, 32 B)
  + pre-built `pipeline_sh_block` (48 B, exact output of
  `native_pm4_build_render_pipeline_sh`) + pre-built
  `pipeline_ctx_block` (240 B, exact output of
  `native_pm4_build_render_pipeline_ctx`). Packed once at
  `pipeline_create` time via `native_render_pipeline_pack`. Per the
  4-block split's pipeline-static lifetime, these blocks are encoded
  once and memcpy'd into every IB that uses this pipeline.
- **`NativePass`** (32 B): ctx_ref / rt_ptr / clear_color_ptr +
  reserved. Layout matches `_backend_wgpu_render_pass_begin`'s
  pass struct shape for parity. v2-native defers all PM4 emit to
  `pass_draw` rather than splitting begin/draw — single-draw-per-
  pass means the lifetime split has no caching value yet.
- **Seven slot wrappers** in `src/backend_native.cyr`:
  - `_backend_native_render_target_create_2d_rgba8` /
    `_backend_native_render_target_release` — wrap Step 6.3's
    `native_rt_create_2d_rgba8` / `_release` primitives.
  - `_backend_native_render_pipeline_create` /
    `_backend_native_render_pipeline_release` — alloc + pack /
    zero. `color_fmt` accepted for slot ABI parity but ignored
    (only RGBA8_UNORM supported on native v2).
  - `_backend_native_render_pass_begin` — alloc 32 B, stash refs.
    No PM4 emit; defers to `pass_draw`.
  - `_backend_native_render_pass_draw` — composes the full IB
    (ACQUIRE_MEM preamble + 2 UConfig + memcpy `pipeline_sh_block`
    + memcpy `pipeline_ctx_block` + `_pass_target` from RT extents
    + `_draw_tail` + NOP padding to 256 dwords) and dispatches via
    Step 6.6's `native_render_dispatch_simple`. `vc` / `ic` accepted
    for ABI parity; the FS+VS pair is fixed-shape (3-vertex
    fullscreen triangle), so the actual draw_tail uses
    `GFX9_FULLSCREEN_TRI_VCOUNT` regardless.
  - `_backend_native_render_pass_end` — zero pass struct.
- **`NativeRenderTarget` extended 32 → 40 B**: added width / height
  fields at +32 / +36 (u32 each). Required by `pass_draw` to feed
  RT dimensions into `native_pm4_build_render_pass_target`. The RT
  is the source of truth for its own dimensions; consumers
  shouldn't reconstruct from `size = w*h*4`.
- **`backend_native_new()`** now wires all 21 slots —
  `backend_is_complete()` returns 1 (was 0 with v2 render range
  pending since 6.8(b) close).

**Load-bearing CPU test:**
`test_native_render_pipeline_pack_matches_composer` asserts that
the cached `pipeline_sh_block` and `pipeline_ctx_block` inside a
freshly-packed `NativeRenderPipeline` are dword-identical to what
the standalone composers emit into a parallel scratch buffer. Guards
against the "memcpy fast path silently diverges from the composer"
class of bug — if `native_pm4_build_render_pipeline_sh` ever
changes its emit order or count, this test fails immediately rather
than producing a corrupt IB at HW dispatch.

**v2-native limitations** (documented inline; lifted in v3.x):
single draw per pass, no pipeline state caching across draws,
`clear_color_ptr` ignored (FS shader hardcodes red — Step 6.2(b)),
`color_fmt` ignored (only RGBA8_UNORM).

819 v3 (was 712) + 624 mabda CPU asserts green; smoke + lint clean.

### Added — 2026-04-30 (Step 6.9(b) — native render e2e program)

`programs/native_render_e2e.cyr` (243 lines) lands as the
native-path mirror of `programs/render_e2e.cyr`. Drives the full
6.x chain end-to-end through the public 6.9(a) `gpu_render_*` API
in one program:

```
gpu_context_new_native()
  → vs/fs shader BOs (4 KiB GTT each, va_map'd at canonical-high)
  → vs_mod/fs_mod (handle, va) pairs
  → gpu_render_target_create_2d_rgba8(ctx, 256, 256)
  → gpu_render_pipeline_create(ctx, &vs_mod, &fs_mod, 0)
  → gpu_render_pass_begin(ctx, rt, &clear)
  → gpu_render_pass_draw(ctx, pass, pipe, 3, 1)   # GFX-ring submit
  → gpu_render_pass_end(ctx, pass)
  → load8(rt_addr + 0..3)  # native RT is GTT-mapped → CPU-direct
  → expect (0xFF, 0x00, 0x00, 0xFF)
```

Native RT is GTT-mapped linear — readback is a direct `load8` of
the mmap'd bytes. No `copy_texture_to_buffer` round-trip needed
(unlike the wgpu path). Twelve named exit codes (0–11) map to
specific failure classes for unattended runs.

Wired into the Makefile as `build/native_render_e2e` /
`test-native-render-e2e` (mirrors the `native_compute_store` /
`native_texture_e2e` pattern; no C launcher since the native path
doesn't link wgpu-native).

**Documented HW-time failure modes** (most likely first, with the
reasoning so future-you can read it cold):

- **A. Post-draw cache flush missing.** The 6.5(b) PM4 composer
  emits ACQUIRE_MEM (cache *invalidate*) at the start of the stream
  — for shader-fetch correctness — but no end-of-pass
  CACHE_FLUSH_AND_INV. radv emits one before reading the RT. If
  pixel(0,0) reads back as 0x00000000 (uninitialized GTT) while
  syncobj signaled normally, this is the suspect. Fix lands in a
  6.x follow-up (extend the composer with a CACHE_FLUSH_AND_INV at
  draw_tail end).
- **B. TDR on the GFX ring.** ~10000ms elapsed + non-zero rc =
  ring hang. `dmesg | grep amdgpu` confirms; Layer-2 byte-diff
  vs radv localizes the bad packet.
- **C. Pipeline state register mis-encoding.** Non-red pixel
  (e.g. black) means FS ran but RT bind / blend / target-mask is
  wrong. Suspect 6.5(a)'s pipeline_ctx register addresses.

Build-clean (`cyrius build programs/native_render_e2e.cyr` succeeds;
full include chain links). HW-gated to run — needs amdgpu + valid
render-node fd. CI runners without DRM skip; developer gate is
local `make test-native-render-e2e` on a Cezanne / equivalent box.

**Phase C render is now code-complete** — every 6.x sub-bullet has
either landed or is a HW-time follow-up (post-draw flush, Layer-2
verify). The structural critical path from "shader bytes" to
"pixel verified" is in tree.

### Added — 2026-04-30 (Step 6.10 prep — CACHE_FLUSH_AND_INV builder)

Stand-alone PM4 primitive for the post-draw cache flush radv emits
before any CB→CPU readback. Lands as a separate prep step (not yet
wired into the composer) so the moment HW data confirms the
hypothesis from `programs/native_render_e2e.cyr`'s "Failure A"
note, the fix is one call-site addition rather than a fresh
research session.

- **`native_pm4_event_write(buf, pos, event_type, event_index)`** —
  general-purpose 2-dword EVENT_WRITE builder. Body packs
  `event_type` in bits[5:0] and `event_index` in bits[11:8] (matches
  Mesa's `EVENT_TYPE(t) | EVENT_INDEX(i)` macro convention). Used
  for `event_index = 0` "other" events; TS-style events with a
  64-bit writeback address need RELEASE_MEM (deferred).
- **`native_pm4_event_write_cache_flush_and_inv(buf, pos)`** —
  convenience wrapper that emits the canonical
  `EVENT_TYPE = CACHE_FLUSH_AND_INV (0x16)`,
  `EVENT_INDEX = OTHER (0)` packet. The CP processes this event
  before writing the user-fence completion seqno, so by the time
  the syncobj signals the RT writes are CPU-visible.
- Four GFX9 event-type / event-index constants exposed and pinned:
  `GFX9_EVENT_TYPE_CACHE_FLUSH = 0x04`,
  `GFX9_EVENT_TYPE_CACHE_FLUSH_AND_INV = 0x16`,
  `GFX9_EVENT_INDEX_OTHER = 0`, `GFX9_EVENT_INDEX_TS = 5`. All
  cited from Mesa `gfx9.json`'s `VGT_EVENT_TYPE` enum (the same
  authoritative source 6.5(a)'s register addresses use).

**Not yet wired into the render PM4 composer.** The first
`make test-native-render-e2e` on Cezanne is the gate. Two outcomes
the prep is designed for:

- If pixel readback returns the GTT 0x55 sentinel (Failure A in
  the e2e program's docstring), append
  `native_pm4_event_write_cache_flush_and_inv(buf, pos)` after
  the draw_tail in `native_pm4_build_render_clear_triangle`. One
  call-site change; the builder + tests are already done.
- If HW reveals a different failure class, the builder is still
  needed for Phase D surface present (end-of-frame flush before
  the page-flip ioctl) — not throwaway work even in the
  non-flush-bug branch.

**5 CPU tests, 17 asserts:** GFX9 constants pinned, EVENT_WRITE
binary form (header `0xC0004600` + body `0x00000016` for
CACHE_FLUSH_AND_INV), wrapper byte-exact equivalence, position-
tracking composability with `native_pm4_nop`, event_index packing
(verified with a non-zero index = 5 case so future TS-event paths
can build on this).

### Added — 2026-04-30 (Step 7.1(a) — Phase D DRM/KMS foundation)

First Phase D bite lands: a new module `src/backend_native_kms.cyr`
(243 lines) implementing the DRM/KMS GetResources ioctl. Filed
separately from `backend_native.cyr` because the surface path is
structurally distinct from the compute / render path — `card0`
master node + `DRM_IOCTL_MODE_*` family, vs `renderD128` render
node + `DRM_IOCTL_AMDGPU_*` family.

- **`DRM_IOCTL_MODE_GETRESOURCES`** (= `0xC04064A0`) — derived from
  `_IOC(RW, 0x64, 0xA0, 64)`. Returns the kernel's count of FBs /
  CRTCs / connectors / encoders + ID arrays for each.
- **`drm_mode_card_res`** struct shape (64 bytes, every field
  pinned by CPU asserts): four `*_ptr` fields, four `count_*`
  fields (IN: capacity, OUT: actual), four extent-limit fields.
- **`native_drm_mode_get_resources(fd, req)`** — low-level
  ioctl wrapper. Caller controls `req` zeroing (count-only pass
  vs array-fill pass).
- **`native_kms_init(fd)`** — the two-pass driver. Pass 1 with
  null pointers + zero counts to discover sizes; pass 2 with
  heap-allocated arrays of the discovered sizes. Returns a
  96-byte `KmsState` (or 0 on failure / no DRM master).
- **`KmsState`** struct (96 bytes): fd + 4 counts + 4 ID array
  pointers + 4 extent-limit fields + 24 reserved bytes for
  per-connector / per-encoder summary tables landing in 7.1(b/c).
- **13 short field accessors** (`kms_state_fd`, `kms_state_count_*`,
  `kms_state_*_ids`, `kms_state_min_width` etc.) keep call sites
  readable without spreading the field-offset constants through
  consumers.
- **`MODE_GETCONNECTOR`** (= `0xC05064A7`) and **`MODE_GETENCODER`**
  (= `0xC01464A6`) ioctl-number constants exposed for sub-bites
  (b) + (c). Helper fns land in those steps.
- **`native_kms_release(state)`** — safe-zero teardown. Bump-
  allocator pattern means heap allocs aren't freed (consistent
  with `native_compute_dispatch_cached`); zeroing the struct
  surfaces stale-pointer use as null on subsequent loads.

`backend_native_kms.cyr` is wired into the include chain
(`src/lib.cyr`) and `[lib].modules` (`cyrius.cyml`) so
`cyrius distlib` bundles it for downstream consumers and the
smoke build links it.

5 CPU tests, 51 asserts in `tests/tcyr/mabda_v3.tcyr`: ioctl
numbers re-derived from first principles to catch transcription
drift, drm_mode_card_res field offsets pinned, KmsState layout
+ accessor round-trips, release safe-zero (null + populated).
The `native_kms_init` driver itself is HW-gated — needs a DRM
master fd (`/dev/dri/card0`), which CI runners typically lack.

### Fixed — 2026-04-30 (caught during Step 7.1(a))

- **`cyrius lint` 128 KiB read-buffer cap** surfaced when
  `tests/tcyr/mabda_v3.tcyr` crossed 131,072 bytes during the
  7.1(a) test additions. `cyrlint.cyr:523` allocs
  `var buf = alloc(131072)` and `file_read_all(path, buf, 131072)`
  truncates anything larger — the linter then misreports
  "unclosed braces at end of file" near the cutoff while the
  file is structurally fine. Identical class of bug to the
  `cyrius distlib` 64K truncation fixed in 5.7.36 (raised to
  256K). cyrlint never got the same treatment; as of cyrius
  5.7.42 the cap is still 128 KiB. Worked around by tightening
  the new test bodies' assertion messages so the file lands
  at 130,932 bytes (140 bytes headroom). Saved as a memory note;
  real fix is to bump cyrlint's buffer upstream and re-pin —
  noted as a tooling follow-up.

### Added — 2026-04-30 (Step 7.1(b) — DRM connector primitives)

Extends `src/backend_native_kms.cyr` with the data structures the
per-connector enumeration driver (landing in 7.1(c)) will consume:

- **`drm_mode_get_connector`** struct shape (80 bytes; 16 fields
  pinned by CPU asserts). Same two-call protocol as
  drm_mode_card_res — pass 1 with zero counts to discover sizes,
  pass 2 with caller-allocated arrays for encoder IDs, modes,
  property IDs, and property values. Caller MUST set `connector_id`
  at +48 before the ioctl (kernel keys lookup off it).
- **`drm_mode_modeinfo`** struct shape (68 bytes; 13 numeric fields
  for clock + h/v timings + flags + type + a 32-byte fixed-width
  name buffer at +36..+68). Each modes_ptr entry in a populated
  drm_mode_get_connector is one of these.
- **Connection-status enum**: `DRM_MODE_CONNECTED = 1`,
  `DRM_MODE_DISCONNECTED = 2`, `DRM_MODE_UNKNOWNCONNECTION = 3`.
- **Connector-type enum**: 12 values covering modern desktop +
  laptop hardware — `Unknown` (0), `VGA` (1), `DVII` (2), `DVID`
  (3), `DVIA` (4), `DisplayPort` (10), `HDMIA` (11), `HDMIB` (12),
  `eDP` (14), `VIRTUAL` (15), `DSI` (16), `USB` (20). Obscure
  pre-2010 types (TV / Composite / 9PinDIN) deferred until a
  consumer asks.
- **`native_drm_mode_get_connector(fd, req)`** — low-level ioctl
  wrapper. Caller manages the two-call zeroing pattern.

The higher-level "discover every connector" driver is deferred to
7.1(c), where it composes naturally with encoder discovery + the
topology summary fn.

### Changed — 2026-04-30 (Phase D test split)

`tests/tcyr/mabda_v3.tcyr` was approaching cyrlint's 128 KiB
read-buffer cap (the bug surfaced during 7.1(a) close — see
session 26 notes). Split out Phase D tests into a new file
**`tests/tcyr/mabda_v3_phase_d.tcyr`** (9.5 KB, 100 asserts across
9 tests). The boundary is clean: every test in the new file
exercises `src/backend_native_kms.cyr` exclusively. `make test`
runs it after `mabda_v3.tcyr`. As 7.x grows (per-connector
enumeration in 7.1(c), modeset in 7.2, framebuffer in 7.3,
page-flip in 7.4, etc.) new Phase D tests land in this file and
keep `mabda_v3.tcyr` under the cap.

`mabda_v3.tcyr` shrank 134 KB → 126 KB (8 KB headroom). Total
asserts unchanged: 836 v3 + 100 phase_d = 936, was 887 at 7.1(a)
close (+49 from 7.1(b)'s 4 new tests).

### Added — 2026-04-30 (Step 7.1(c) — Phase D discovery HW-verified)

Closes Phase D discovery: encoder ioctl + struct, three name-lookup
helpers, the `native_kms_summary` topology printer, and a runnable
`programs/native_kms_summary.cyr` diagnostic that **was verified
live on Cezanne** to print the real DRM/KMS topology of the
machine.

- **`drm_mode_get_encoder`** struct shape (20 bytes; 5 fields:
  encoder_id / encoder_type / crtc_id / possible_crtcs /
  possible_clones).
- **`DRM_MODE_ENCODER_*`** enum (9 values: NONE / DAC / TMDS /
  LVDS / TVDAC / Virtual / DSI / DPMST / DPI). DAC + TMDS + LVDS
  cover legacy / desktop / laptop; Virtual / DSI / DPMST / DPI
  cover modern ARM + DP-MST setups.
- **`native_drm_mode_get_encoder(fd, req)`** — low-level ioctl
  wrapper. Caller sets encoder_id at +0 before calling.
- **Three name-lookup helpers** returning cstr labels:
  `native_drm_connector_type_name(t)` → "DP" / "HDMI-A" / "eDP" / "?",
  `native_drm_encoder_type_name(t)` → "TMDS" / "DP-MST" / "DAC" / "?",
  `native_drm_connection_status_name(c)` → "connected" / "disconnected" / "unknown" / "?".
  Inline if/elif chains rather than data tables — keeps the
  module pure-Cyrius (no string-table runtime), and the fallback
  "?" surfaces any new kernel value visibly.
- **`native_kms_summary(state)`** — walks the connector + encoder
  ID arrays from a populated KmsState and prints one line per
  resource to stdout. Output shape:
  ```
    conn 110 HDMI-A-1 connected enc 109
    enc 109 TMDS crtc 87 poss 0x0000000F
  ```
  Returns 0 on full success, negative errno on the first ioctl
  that fails; partial output prints up to the failure point —
  useful when debugging which connector is misbehaving.
- **`programs/native_kms_summary.cyr`** — runnable diagnostic
  with a `card0..card9` scan so DRM node renumbering doesn't
  matter (Cezanne lands at `card1` on this box; other systems
  may use `card0`). Opens the first available card-node, calls
  `native_kms_init` for the GetResources discovery, then
  `native_kms_summary` for per-resource enumeration. 4 named
  exit codes (0–3) for unattended runs.

**HW verification (Cezanne, 2026-04-30):** ran cleanly on the dev
box. Discovered 4 connectors (1 HDMI-A-1 connected to CRTC 87 +
3 DP disconnected) and 8 encoders (4 TMDS + 4 DP-MST; all with
possible_crtcs = 0x0000000F = all 4 CRTCs usable). Framebuffer
extent: up to 16384×16384. Real-world output matches the documented
shape exactly. Phase D discovery is now end-to-end HW-validated —
no longer "structurally tested but unrun."

6 new CPU tests, 31 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
encoder field offsets (6), encoder-type enum (9), connector + encoder
+ connection name-lookup spot-checks via first-byte comparison (5
+ 5 + 4), summary null-safe (2). The HW path itself is exercised
via `make test-native-kms-summary`.

### Added — 2026-04-30 (Step 7.2(a) — per-connector mode enum + preferred picker)

Foundation for Phase D mode-set: walk a connector's `modes_ptr`
array, pick the EDID-preferred mode, expose dimensions + refresh.
HW-verified on Cezanne — the diagnostic now reports
`2560x1440@59Hz preferred` for the active monitor.

- **`native_kms_get_connector_modes(fd, conn_id, count_out)`** —
  two-call wrapper (count → fill) returning a heap
  `drm_mode_modeinfo[count]` array. Pass-1 reads only count_modes;
  pass-2 alloc'd to that count and refills. Returns 0 on failure
  or when count_modes==0 (typical for disconnected connectors).
  Connection-status filtering is the caller's responsibility.
- **`DRM_MODE_TYPE_*`** enum: `BUILTIN = 0x01` (deprecated),
  `PREFERRED = 0x08` (load-bearing — EDID's preferred mode hint),
  `USERDEF = 0x20`, `DRIVER = 0x40` (typical modern flag). A
  modern monitor's PREFERRED-flagged mode is typed
  `DRIVER | PREFERRED = 0x48`.
- **`native_kms_pick_preferred_mode(modes_ptr, count)`** — returns
  index of the first mode with the PREFERRED bit set, falls back
  to 0 when no mode is flagged. Matches radv's
  `radv_get_preferred_mode` shape.
- **Five mode accessors**: `native_kms_mode_hdisplay/_vdisplay/
  _vrefresh/_type/_refresh_hz`. The last is the load-bearing one —
  computes refresh as `(clock × 1000) / (htotal × vtotal)` because
  the kernel-reported `vrefresh` field is unreliable on modern
  kernels (libdrm derives it the same way; the raw `_vrefresh`
  accessor is exposed only for debugging the kernel's value).
- **Diagnostic extended.** `programs/native_kms_summary.cyr` now
  walks each CONNECTED connector, fetches its modes array, picks
  the preferred index, and prints
  `conn 110: 2560x1440@59Hz preferred (42 mode total)`. The
  hardware run on Cezanne verified the full path end-to-end.

7 new CPU tests, 22 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
DRM_MODE_TYPE_* constants, null-safety on the modes wrapper +
picker, picks-first-preferred (synthetic 3-mode array with two
preferred), falls-back-to-0 (no preferred), all 5 mode accessors,
refresh_hz computed against canonical 1080p60 timing
(clock=148500, htotal=2200, vtotal=1125 → exactly 60 Hz).

### Added — 2026-04-30 (Step 7.3 — KMS framebuffer primitives)

The kernel-side handle that turns a GEM BO into something a
DRM/KMS CRTC can scan out. Without this, 7.2(b)'s SETCRTC has
nothing to display.

- **`DRM_IOCTL_MODE_ADDFB2`** (= `0xC06464B8`) + **`RMFB`**
  (= `0xC00464AF`) ioctl numbers, derived from first principles
  in CPU tests so transcription drift surfaces immediately.
- **`drm_mode_fb_cmd2`** struct shape (100 bytes): fb_id (u32 OUT)
  + width/height/format/flags + 4-plane arrays for handles
  (u32×4), pitches (u32×4), offsets (u32×4), modifiers (u64×4).
  AddFB2 supports up to 4 planes for YUV; single-plane RGB scanout
  uses planes[0] only.
- **Four `DRM_FORMAT_*` fourcc constants**: `XRGB8888` (0x34325258),
  `ARGB8888` (0x34325241), `XBGR8888` (0x34324258),
  `ABGR8888` (0x34324241). The wgpu ↔ DRM byte-order mapping is
  documented inline:
  - wgpu `RGBA8_UNORM` (memory: R G B A) ↔ DRM `ABGR8888`
  - wgpu `BGRA8_UNORM` (memory: B G R A) ↔ DRM `ARGB8888`
  v3 Phase D scans out from RGBA8 BOs (matches the render path's
  RT format), so `ABGR8888` is the working format. `XRGB`/`XBGR`
  are exposed for opaque scanout where alpha is don't-care.
- **`DRM_FORMAT_MOD_LINEAR`** (0) — explicit linear-tiling
  modifier for clarity at call sites. Tiled scanout (DCC, swizzle)
  is post-v3.0 perf work.
- **Low-level wrappers**: `native_drm_mode_add_fb2(fd, req)` and
  `native_drm_mode_rm_fb(fd, fb_id_ptr)`.
- **High-level helpers**:
  `native_kms_add_fb_xrgb8888(fd, bo_handle, w, h, pitch)` returns
  the FB ID on success (always > 0), 0 on failure.
  `native_kms_rm_fb(fd, fb_id)` releases. Both null-safe.

The actual ADDFB2 ioctl is not exercised on HW yet — properly
exercising it needs the BO + master-fd + render-fd cross-namespace
plumbing (KMS lives on master / `card0`, GEM allocator typically
on render / `renderD128`) that 7.2(b)'s end-to-end modeset path
composes naturally. Structural primitives are fully CPU-tested
so when 7.2(b) lands the helpers are HW-ready.

5 new CPU tests, 34 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
ioctl numbers re-derived, drm_mode_fb_cmd2 field offsets +
4-plane sub-array consistency (handles[4] ends where pitches
starts, etc.), fourcc constants derived from ASCII byte values,
null-safety on both high-level fns.

### Metrics — 2026-04-30 (post Step 7.3)

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: 836 assertions (unchanged).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: **187 assertions** (was 153
  at 7.2(a) close; +34 from 7.3).
- `src/backend_native_kms.cyr`: ~760 lines (was ~620 at 7.2(a);
  +140 lines for ADDFB2 / RMFB primitives + fourcc + helpers).
- `dist/mabda.cyr`: regenerated (10213 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Next — 2026-04-30 (post Step 7.3)

**Phase D structural primitives complete.** Next bite is end-to-end:

1. **7.2(b) — `DRM_IOCTL_MODE_SETCRTC` ioctl + driver +
   end-to-end modeset.** All four prerequisites are now in tree:
   discovery (7.1), mode-pick (7.2(a)), encoder/CRTC binding
   (7.1(c)), framebuffer (7.3). The driver shape:
   `native_kms_modeset(state, conn_id)` walks discovery → fetches
   modes → picks preferred → walks to encoder → picks CRTC from
   `possible_crtcs` mask → allocates a BO sized to the mode →
   `native_kms_add_fb_xrgb8888` → SETCRTC. The first run on
   Cezanne would visibly flip the desktop's modeset — biggest
   "real thing happening" gate yet on Phase D. Cross-fd plumbing
   (BO on render-fd, FB on master-fd, dmabuf import) is the
   non-obvious piece worth designing carefully before coding.
2. **7.4–7.7 — page-flip, present, release, public API.** v3.0
   completion path for native surface present, after 7.2(b)
   proves modeset works.

**Phase C render HW-gated items still pending:**

- Run `make test-native-render-e2e` on Cezanne. Failure A → 6.10
  composer splice, Failure B → Layer-2 IB diff, Failure C → audit
  6.5(a) registers.
- Layer-2 byte-diff vs radv (Hyprland or headless capture program).

**WGSL → GFX9 ISA lowering (8.x)** remains the v3.0 ship-blocker.
Worth a sober design spike before the next major bite.

Full handoff (Step 6.5 close-out + 6.6 sequencing):
[`docs/handoff/2026-04-30-session26-render-pm4-composer.md`](docs/handoff/2026-04-30-session26-render-pm4-composer.md).

### Verified — 2026-04-27/28 (B.4 store shader live-verified on Cezanne)

- `programs/native_compute_store.cyr` lands `0xDEADBEEF` in `out[0]`
  from a pure-Cyrius compute dispatch on AMD Cezanne (gfx90c, kernel
  6.18.24-1-lts, MEC fw `0x1e2`). Submit-to-syncobj signal is **0 ms**
  (not the 10 s TDR shape). The post-dispatch `0xC0FFEE12` `WRITE_DATA`
  marker confirms the CP returned cleanly from `DISPATCH_DIRECT`. The
  cl_probe canary stays green afterward. **All three B.4 calibration
  gates from the Session 24 handoff are now met**, reversing the
  Session 9 retraction.
- The Session 25b investigation found six independent bugs that had to
  be fixed for the Cyrius native compute path to match the working
  C spike byte for byte. Full diff in
  `docs/handoff/2026-04-28-session25-b4-verified.md`. Summary:
  - `src/backend_native.cyr::native_pm4_nop` — count_minus_1 off-by-one;
    `pad_dwords` was passed directly to `native_pm4_pkt3_header` (which
    subtracts 1 internally), so the on-wire header claimed one extra
    body dword. CP read past the IB end and either hung or processed
    garbage. Fix: pass `pad_dwords - 1`. Same convention bug class as
    the `feedback_pm4_count_minus_1_naming` memory.
  - `programs/native_compute_store.cyr` — `PGM_LO/HI` encoding. GFX9
    reconstructs `addr = (PGM_HI << 40) | (PGM_LO << 8)`, so PGM_LO
    must carry bits `[39:8]` (use `(va >> 8) & 0xFFFFFFFF`), not bits
    `[31:0]` (`va & 0xFFFFFFFF`). The old encoding produced
    `addr = 0x0000800000000000` for `shader_va = 0xFFFF800100000000` —
    wrong VA, wave fails launch, queue TDRs. Same class as the
    `feedback_cyrius_signed_div_high_va` memory; converted remaining
    `va / 2^N` patterns to `>> N` shifts.
  - `programs/native_compute_store.cyr` — stub VA moved from
    `0x200000` (user-low) to `0xFFFF800100004000` (canonical-high).
    Session 25b's post-dispatch marker test showed user-low writes
    from the compute queue silently fail. USER_DATA_2/3 now encodes
    the canonical-high stub VA as a 64-bit pointer split low/high.
  - `programs/native_compute_store.cyr` — `TA_CS_BC_BASE_ADDR = 0`.
    The Session 16 hypothesis that `BC_BASE` had to point at
    `shader_va` (or Mesa's magic `0x01004400 / 0x80`) is falsified
    by Session 25b's clean A/B: the only difference between the
    failing 25 run and the passing 25b run is `BC_BASE = magic` vs
    `BC_BASE = 0`.
  - `src/backend_native.cyr` — added `native_cs_submit_4chunk` with
    explicit `AMDGPU_CHUNK_ID_FENCE` (offset 32) plus
    `IB flags = 0x08` (`AMDGPU_IB_FLAG_TC_WB_NOT_INVALIDATE`).
    Replaces the 3-chunk path that submitted cleanly but never
    queued the IB to MEC (Session 9 false-positive shape).
  - `programs/native_compute_store.cyr` — inline BO list with
    `operation = list_handle = 0xFFFFFFFF` sentinels (Mesa rusticl
    inline-residency path); replaces `BO_LIST_OP_CREATE`. All BOs
    given full `R | W | X` page perms during bring-up — flagged for
    re-tightening once a regression test catches the narrowing
    failure mode.
- Falsified hypotheses (cleared from the live tracking list):
  - `BC_BASE = shader_va` is load-bearing (Session 16).
  - `BC_BASE = 0x01004400 / 0x80` (Mesa magic VA) is load-bearing.
  - Compute queues accept both VA halves (user-low + canonical-high).
- What B.4 closing does **not** include: a `Backend.compute_dispatch`
  abstraction, multi-dispatch submits, profiler integration, or the
  other three integration shapes (`phase0`, `render_e2e`,
  `render_graph_e2e`). These are still required by the v3.0 exit
  criteria. Backend abstraction (ADR 006) is the natural next step.

### Fixed — 2026-04-22/23 (B.3.d PM4 encoder bugs)

- `src/backend_native.cyr::native_pm4_acquire_mem_full_invalidate` —
  count argument `7 → 6`. The PKT3 header's word-count formula is
  `(count - 1) & 0x3FFF`, and ACQUIRE_MEM on GFX9 has 6 data dwords
  (coher_cntl, size_lo, size_hi, base_lo, base_hi, poll_interval).
  Passing `count=7` made the header claim 7 payload dwords; the CP
  consumed the following `SET_SH_REG` header as stray data, then
  mis-parsed every subsequent packet in the IB. Resulting
  `gfx_v9_0_bad_op_irq` + MODE2 reset looked like different failures
  across Sessions 1–6 but was one-and-the-same desync. Mesa's
  `AMD_DEBUG=ib` dump shows `0xC0055802`; we now match byte-exact.
- `src/backend_native.cyr::native_pm4_dispatch_direct` — predicate
  argument `0 → 2`. The low byte of IT_DISPATCH_DIRECT's PKT3 header
  is `shader_type` (0 = graphics, 2 = compute), not a predicate bit.
  Header now emits `0xC0031502`, matching Mesa.
- `src/backend_native.cyr` — added `native_pm4_set_uconfig_reg_pair`
  helper for paired register writes (TA_CS_BC_BASE_ADDR + _HI).
- `programs/native_compute_spike.cyr` — rewrote PM4 emission to
  mirror Mesa rusticl's exact preamble order (PGM_HI →
  STATIC_THREAD_MGMT × 2 → UCONFIG preamble → PGM_LO → RSRC1/2 →
  TMPRING_SIZE → USER_DATA_2/3 → USER_DATA_0 → ACQUIRE_MEM →
  RESOURCE_LIMITS → NUM_THREAD → DISPATCH_DIRECT). Register values
  aligned: `RESOURCE_LIMITS = 0x140` (was `0` — zero waves = silent
  stall), `TMPRING_SIZE = 0x100` (was unset), `STATIC_THREAD_MGMT_SE1
  /SE2/SE3 = 0` (Cezanne has 1 SE; writing 0xFFFFFFFF to absent-SE
  mask registers is meaningless and diverges from Mesa).

### Retracted — 2026-04-23 (Session 9): Phase B.3.d was a TDR false positive

- **B.3.d is NOT closed.** The "dispatch completed (sync-obj signaled),
  RC=0, dmesg silent" signature does not prove the CP ran our IB — it
  only proves the kernel's Timeout Detection & Recovery (TDR) path
  signalled our fence. Session 9 investigation added a CP-side
  WRITE_DATA packet to the spike's PM4 stream targeting the stub BO's
  VA. Post-submit CPU readback of the stub BO's first word returned
  `0x00000000` (unchanged from creation's memset), not `0xCAFEBABE`
  (the value WRITE_DATA would have written if the IB actually ran).
- **Timing proof:** empty/NOP-only IB runs take exactly ~10 seconds —
  AMDGPU's default TDR timeout — and return "success." Mesa's
  `cl_probe` on the same hardware runs in ~77 milliseconds with
  correct readback, confirming the GPU is healthy and Mesa's
  submission path works.
- **Location of actual blocker:** somewhere between "AMDGPU_CS ioctl
  returns 0" and "CP executes our PM4." Candidates ranked (see Session
  9 handoff): ring index / ip_instance, BO_HANDLES chunk vs BO_LIST
  ioctl, CS/IB flags, context priority, accumulated TDR state.
- **What stays valid:** Session 7 PM4 encoder fixes (ACQUIRE_MEM
  count, DISPATCH_DIRECT shader_type) are real bugs that *would*
  wedge the CP if submissions ever reached it. The store shader
  bytes and byte-exact tests are correct. Only the "verified live"
  claim is retracted.
- **Full Session 9 handoff:**
  `docs/handoff/2026-04-23-session9-tdr-false-positive.md`.

### Added — testing

- `tests/tcyr/mabda.tcyr::test_native_pm4_acquire_mem_layout` — byte-
  exact header + payload assertion against the Mesa IB dump.
- Updated `test_native_pm4_dispatch_direct_layout` and the PM4
  composability test for the new `shader_type` byte.
- Test count 602 → 610 (8 new assertions). All green under 5.6.13.

### Methodology note

The actually-valuable lesson: any direct-PM4 code should diff header
bytes byte-exactly against `AMD_DEBUG=ib` output *before* being run
live. Six sessions of theorizing about VMID/scratch/VA aperture were
all downstream of a one-bit count-field bug that a five-minute diff
would have caught. Captured as `feedback_pm4_verify_against_mesa_ib`
in auto-memory and as a vidya field note.

### Prepared — 2026-04-23 (B.4 store shader — built, not live-verified)

- `src/backend_native.cyr::native_gfx9_shader_store_deadbeef` —
  Session 6 diagnostic stub replaced with the real 6-instruction
  store kernel (v_mov_b32 v0,s0; v_mov_b32 v1,s1; v_mov_b32 v2,
  literal; global_store_dword v[0:1],v2,off glc slc; s_waitcnt
  vmcnt(0) lgkmcnt(0); s_endpgm) + 16-dword NOP prefetch padding.
  Bytes are byte-exact output of `clang -target amdgcn--amdhsa
  -mcpu=gfx90c`. Function grew from 84 → 96 bytes.
- `programs/native_compute_store.cyr` — full rewrite. PM4 preamble
  is byte-identical to `programs/native_compute_spike.cyr` (the
  Session 8 verified-working baseline) except for three store-
  specific deltas: USER_DATA_0/1 carry the real output VA (spike
  used Mesa's scratch-V# stub there); USER_DATA_2/3 kept at the
  spike values (shader ignores s2/s3); BO list has 4 entries (shader
  + stub + output + IB). All VAs canonical-high.
- `tests/tcyr/mabda.tcyr::test_native_gfx9_shader_store_deadbeef_writes_bytes` —
  new byte-exact assertion covering every dword of the store shader
  + the NOP padding. Test count 610 → 621.
- `build/native_compute_store` built under released 5.6.13. **Session
  9 update: cannot be meaningfully verified until the CS-submission
  blocker documented in the Session 9 handoff is fixed.** All iteration
  attempts on the store program (tried both USER_SGPR=2 and =4;
  canonical-high and low output VA; hardcoded-VA shader variant
  bypassing USER_DATA) failed the same way because the CP never
  actually executes our IB.

### Changed — toolchain

- `cyrius.cyml` pin `5.5.20 → 5.6.13`. The active toolchain on the
  dev box was already running 5.6.x; the manifest was lagging. Pin
  now matches the released 5.6 line (5.6.14 is in-dev, not shipped).
- `dist/mabda.cyr` regenerated — picked up +274/-6 lines of latent
  drift from v3 Phase A work (`src/render_graph.cyr` transient
  aliasing planner, commit `211a47b`) that had never been re-
  bundled. Not related to the pin bump; caught as a side-effect of
  the bump-prompted regen.

## [2.5.0] — 2026-04-21

**First feature release post-v1.0-parity. Adds a DAG-style render
graph on top of the now-stable compute + render-pass + copy
primitives.** Consumers describe a frame as nodes + transient
resources; the graph topo-sorts and executes every node into a
single command encoder with one queue submit. Additive only — no
existing public API changed. Designed to survive the v3.0 backend
swap unchanged.

### Added
- **`src/render_graph.cyr`** — new module. Public API:
  - `rg_new()` / `rg_release(g)` — graph lifetime.
  - `rg_label(g, cstr)` / `rg_aliasing(g, on)` — attributes.
  - `rg_add_compute(g, pipeline, bg, dims_xyz, label)` → node_id.
  - `rg_add_render(g, pass_builder, pipeline, draw_verts, label)` → node_id.
    `pipeline = 0` + `draw_verts = 0` ⇒ clear-only pass.
  - `rg_add_copy_buf_buf(g, src, src_off, dst, dst_off, size)` → node_id.
  - `rg_add_copy_tex_buf(g, args72)` → node_id. `args72` is a pointer
    to a 72-byte WgpuCopyTexToBufArgs — same layout as the v2.4.3
    render-pass FFI shim.
  - `rg_add_transient_buffer(g, size, usage, label)` → res_id.
  - `rg_add_transient_texture(g, w, h, format, usage, label)` → res_id.
  - `rg_node_reads(g, node_id, res_id)` / `rg_node_writes(...)` — drive
    the Kahn toposort and (future) aliasing analysis.
  - `rg_build(g, device)` — validate dependency graph + allocate
    transient GPU resources. Returns 0 on success, 1 on cycle.
  - `rg_execute(g, device, queue)` — one encoder, one submit. Returns
    0 on success, 1 on unbuilt-graph / null device or queue / encoder
    failure.
- **`programs/render_graph_e2e.cyr`** — 3-node integration test
  (compute doubler → render clear-to-red → copy_texture_to_buffer).
  Verifies compute output matches `[2, 4, 6, ... 16]` and readback
  pixel(0,0) = `(0xFF, 0x00, 0x00, 0xFF)` exact. All 5 assertions
  pass first try on RADV / Mesa 26.0.
- **`make test-render-graph-e2e`** Makefile target. Added to the
  aggregate `test-gpu` gate.
- **`docs/guides/render-graph.md`** — authoring guide with the three-
  node example, node-kind table, reads/writes semantics, execution
  contract, when-NOT-to-use section, and out-of-scope list.
- **44 new CPU regression assertions** (343 → 387) in
  `tests/tcyr/mabda.tcyr`. Cover graph construction, transient
  recording, reads/writes guards, build idempotence, linear-chain
  and diamond topological sort, null-handle short-circuits, and the
  aliasing flag round-trip.

### Scope

- **Linear DAG only.** Cycles return error from `rg_build`. Out-of-order
  insertion: toposort respects writer→reader edges only in insertion
  direction, which effectively validates the user supplied a correct
  linear ordering. Full multi-version read/write tracking (programmatic
  consumers that build graphs out of execution order) is v2.5.1+ work.
- **No automatic barrier insertion.** wgpu-native handles layout
  transitions and memory barriers. v3.0's native backend revisits this.
- **Aliasing pass scaffolded but OFF by default.** `rg_aliasing(g, 1)`
  flips a flag the current build path does not yet consume — every
  transient gets its own allocation. Alias-pass implementation lands
  when a consumer asks for memory-tight frames.
- **Single-queue only.** Cross-queue coordination moves to v3.1 with
  multi-queue support.

### Metrics
- **Modules**: 30 (was 29 — +`render_graph.cyr`).
- **Source lines**: ~4,500 (+~350 render_graph).
- **Tests**: 387 assertions (was 343 — +44 render graph).
- **Programs**: 5 (was 4 — +`render_graph_e2e.cyr`).
- **FFI slots**: 65 (unchanged; render_graph dispatches through
  existing slots — render pass FFI from v2.4.3, compute FFI from
  v2.0, copy from v2.0).
- **Dist bundle**: `dist/mabda.cyr` regenerated.
- **GPU integration**: phase0 10/10, compute_e2e 7/7, render_e2e 8/8,
  render_graph_e2e 5/5, bench-gpu 13/13 all pass.

### Next
- v2.5.1+ — full out-of-order toposort, aliasing pass, per-pass
  debug labels in the command encoder (nested `debug_push` wrap
  around each node).
- v3.0 — pure Cyrius GPU backend. The render graph's public surface
  does not change; only the dispatch primitives it calls get
  replaced.

---

## [2.4.5] — 2026-04-21

**Cache hot-path unblock via cyrius v5.5.20's u64-keyed hashmap.**
mabda's cache modules now call `map_u64_*` directly, retiring the
per-lookup `_hash_to_heap_key` allocation documented in the v2.4.4
benchmark report. `bind_group_cache_hit` drops **13× (210 ns → 16 ns)
and reaches Rust v1 parity**; `shader_cache_hit` drops 2.8× (553 ns →
195 ns).

### Changed
- **Toolchain pin** `cyrius = "5.5.11" → "5.5.20"` in `cyrius.cyml`.
  Picks up the `map_u64_*` API in `lib/hashmap.cyr` (see cyrius
  v5.5.20 CHANGELOG — SplitMix64-hashed, 16 B slot layout, zero alloc
  on get/has/set-of-existing-key).
- **`src/shader_cache.cyr`** — `shader_cache_new` / `_get` / `_set` /
  `_get_or_compile` now back on `map_u64_*`. The FNV-1a hash
  (`_shader_hash`) output goes straight into the map as the u64 key;
  no more decimal-string conversion.
- **`src/pipeline_cache.cyr`**, **`src/bind_group_cache.cyr`**,
  **`src/texture.cyr`** (texture_cache helpers) — same migration.
  Callers pass raw u64 hash keys.
- **`programs/benchmarks.cyr`** — `shader_cache_hit` and
  `bind_group_cache_hit` iteration caps relaxed (10 × 1000 → 100 ×
  10 000); the arena-exhaustion risk that forced the cap in v2.4.4
  is gone with the new zero-alloc hit path.

### Removed
- **`src/cache_key.cyr`** deleted. The `_hash_to_heap_key` helper
  (decimal-string conversion for cstr-keyed hashmap keys) is no
  longer needed. `src/lib.cyr` and `cyrius.cyml`'s `[lib] modules`
  list updated to drop the include. Three `programs/*.cyr` that
  selectively included `src/cache_key.cyr` also dropped the line.

### Unblocked
- Cache-hit cost is no longer a gating item for v2.5.0 render graph.
  Graph-node lookups (compute + render + copy + transient) all land
  in the cache modules touched here; the new 16-ns floor means cache
  overhead stays well below the render-pass ~5 µs setup cost.

### Metrics
- **Benchmarks**: 20 (unchanged). New v2.4.5 rows in
  `bench-history.csv` at commit `6899eac`.
- **Tests**: 343 assertions (unchanged).
- **Source lines**: ~4,150 (−26 net — cache_key.cyr deletion
  outweighed the cache modules getting slightly leaner).
- **Dist bundle**: `dist/mabda.cyr` regenerated.
- **GPU integration**: phase0 10/10, compute_e2e 7/7, render_e2e 8/8,
  bench-gpu 13/13 all pass on RADV / Mesa 26.0.

### Next
- v2.5.0 — render graph (DAG pass orchestration). Foundation now
  has a zero-alloc cache-lookup floor, so graph-node dedup is cheap.

---

## [2.4.4] — 2026-04-21

**Benchmark parity with Rust v1.0 — full 20-benchmark suite.** Ports
the 13 GPU-backed Rust benchmarks that v2.1's CPU harness deferred.
Exercising them on real hardware surfaced two more latent FFI stubs
that shipped through v2.4.3 (both carried TODO comments — neither
ever ran against wgpu-native). v2.4.4 fixes those, lands the full
benchmark harness, and records a side-by-side comparison to Rust v1
in `docs/benchmarks-rust-v-cyrius.md`.

See the doc for per-benchmark numbers; headline: **Cyrius is faster
than Rust v1 on 7 of 13 GPU benchmarks**, within 2× on 4 more, and
notably slower only on the two cache-hit benches (which route through
a per-lookup heap alloc — tracked for a u64-keyed-hashmap fix in the
cyrius stdlib for v2.5+).

### Added
- **`programs/benchmarks.cyr`** — 13 GPU-backed benchmarks matching
  the Rust v1 set: `create_storage_buffer_4k`,
  `create_uniform_buffer_64`, `uniform_buffer_write`,
  `shader_cache_hit`, `shader_cache_miss`, `bind_group_cache_hit`,
  `texture_1x1_solid`, `texture_256x256_rgba`, `depth_texture_1080p`,
  `render_target_1080p`, `render_target_msaa4_1080p`,
  `render_pipeline_build`, `compute_dispatch_1024`. Uses
  `lib/bench.cyr` for timing, caps iteration counts per-bench so the
  alloc arena isn't exhausted by cache benchmarks. Prints both
  human-readable lines and `CSV:name,ns` rows that pipe straight into
  `bench-history.csv`.
- **`make bench-gpu`** — Makefile target linking `programs/benchmarks.cyr`
  with `deps/wgpu_main.c` (same pattern as `test-phase0` / `test-compute-e2e` /
  `test-render-e2e`). Output to stdout; pipe through `grep '^CSV:'`
  for machine-readable rows.
- **`_csv_row` helper** in `tests/bcyr/mabda.bcyr` — CPU harness now
  also emits `CSV:` lines so CPU + GPU rows share one capture path.
- **Expanded `docs/benchmarks-rust-v-cyrius.md`** with the full
  Cyrius-vs-Rust comparison table for all 20 benchmarks, plus notes
  on each outlier (sub-ns Rust optimisation-out, `capabilities_report`
  workload mismatch, `texture_256x256_rgba` Rust-side wait artifact,
  cache-hit alloc pattern, `profiler_frame_cycle` vec_push cost).
- **Fresh `bench-history.csv` entries** for all 20 benchmarks at
  commit `6899eac`, timestamp `2026-04-21T04:25:00Z`.

### Fixed
- **`src/depth.cyr` — `depth_texture_new` was a latent stub.** The
  function called `wgpu_device_create_buffer` (wrong API — should be
  `create_texture`) with a hand-rolled 80-byte descriptor whose field
  offsets were off by 4 (`size` at +40 vs the correct +36). The
  returned `DepthTexture` struct stored a zero texture handle and was
  never actually usable on the GPU — the bug slept behind a `TODO`
  comment through every release up to v2.4.3. Rewritten to use the
  shared `wgpu_texture_descriptor` builder (which has correct v29
  offsets) and `wgpu_device_create_texture` (slot 45), plus a default
  2D view for use as a render-pass depth attachment.
- **`src/render_target.cyr` — `rtb_build` was a stub** that stored
  width/height/format metadata but never created any GPU textures
  (another `TODO`). Rewritten to create the main render-target
  texture (with `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC`
  usage), an optional N-sample MSAA texture when `sample_count > 1`
  (patching the descriptor's `sampleCount @ +56` in place because
  `wgpu_texture_descriptor` hard-codes 1), and an optional depth
  attachment via `depth_texture_new`.
- Both functions had ADR 005 `@public` markers; consumers depending on
  `rtb_build` or `depth_texture_new` would have been relying on a
  function that silently returned a half-populated struct. Neither
  fix changes the public API signature.

### Metrics
- **Benchmarks**: 20 total (7 CPU + 13 GPU) — was 7 (CPU only).
- **Tests**: 343 assertions (unchanged from v2.4.3).
- **GPU integration**: phase0 10/10, compute_e2e 7/7, render_e2e 8/8,
  bench-gpu 13/13 all pass on RADV / Mesa 26.0.
- **Dist bundle**: `dist/mabda.cyr` regenerated.

### Next
- v2.5.0 — render graph (DAG pass orchestration). Foundation now
  covers the full v1.0 surface area on real hardware.

---

## [2.4.3] — 2026-04-20

**Render-pass FFI + render E2E — v1.0 checklist closed.** Adds the
wgpu render-pass execution surface v2.4.0 deferred and v2.4.2's
FFI validation made safe to build on. A 6-step offscreen render
pass (create RGBA8 target → build pass with CLEAR color → open pass
via the new FFI → end pass → copy texture → map + verify pixel)
runs clean on RADV / Mesa 26.0 / kernel 6.18, with pixel(0,0)
matching the clear color byte-exact.

### Added
- **7 new wgpu FFI slots (58-64)** in `deps/wgpu_main.c` and
  `src/wgpu_ffi.cyr`:
  - 58: `wgpu_shim_command_encoder_begin_render_pass` (struct-packed
    shim — descriptor contains struct-by-value fields, fits the
    `feedback_fncall6_wgpu` pattern)
  - 59: `wgpuRenderPassEncoderSetPipeline` (direct, 2 args)
  - 60: `wgpuRenderPassEncoderSetBindGroup` (direct, 5 args)
  - 61: `wgpuRenderPassEncoderDraw` (direct, 5 args)
  - 62: `wgpuRenderPassEncoderEnd` (direct, 1 arg)
  - 63: `wgpuRenderPassEncoderRelease` (direct, 1 arg)
  - 64: `wgpu_shim_command_encoder_copy_texture_to_buffer`
    (struct-packed shim — both src/dst are nested v29 structs)
- **2 new struct-packing C shims** with field-by-field unpack in C:
  - `WgpuBeginPassArgs` (40 bytes): packed render-pass descriptor
    without the struct-by-value overhead.
  - `WgpuCopyTexToBufArgs` (72 bytes): flat src/dst/copy-size
    layout, C unpacks into `WGPUTexelCopyTextureInfo` /
    `WGPUTexelCopyBufferInfo` / `WGPUExtent3D`.
- **`rpb_pass_begin(encoder, builder)`** in `src/render_pass.cyr` —
  dispatcher method that allocates a `WgpuBeginPassArgs` from the
  builder and calls slot 58. Short-circuits on null encoder or
  empty color-attachment list (wgpu validates those and wouldn't
  appreciate the round-trip).
- **`texture_create_render_target_rgba8(device, w, h, label)`** in
  `src/texture.cyr` — RGBA8_UNORM target with
  `RENDER_ATTACHMENT | COPY_SRC | COPY_DST` usage so it can be
  drawn into, read back, and initialised. Same validation envelope
  as `texture_create_rgba8` (rejects invalid dims).
- **`programs/render_e2e.cyr`** — the end-to-end integration test
  itself. 256×256 render target, clear to `(1.0, 0.0, 0.0, 1.0)`,
  copy back, verify pixel(0,0) is `0xFF, 0x00, 0x00, 0xFF` exact
  (RGBA8_UNORM round-trips integer-valued f64s losslessly).
- **16 new CPU regression assertions** (327 → 343) under a new
  `v2.4.3 — render-pass FFI regressions` section in
  `tests/tcyr/mabda.tcyr`:
  - `test_audit_color_attachment_size_72` — guards the
    `COLOR_ATTACHMENT_SIZE = 72` constant against drift.
  - `test_audit_rpb_pass_color_offsets` (8 assertions) — every
    field `rpb_pass_color` writes lands at its v29 offset, so the
    packed array can be passed to wgpu without repacking.
  - `test_audit_rpb_pass_begin_null_encoder` — null encoder
    short-circuits before calling the shim.
  - `test_audit_rpb_pass_begin_empty_short_circuits` — empty color
    attachment list short-circuits.
  - `test_audit_render_target_rgba8_rejects_invalid` (3 assertions)
    — same input-validation envelope as `texture_create_rgba8`.

### Fixed
- **`src/render_pass.cyr` — color attachment layout.** The
  ColorAttachment struct was documented as 56 bytes (with
  `COLOR_ATTACHMENT_SIZE = 64` — internally inconsistent) and laid
  out against a pre-v29 `WGPURenderPassColorAttachment` that
  didn't have `nextInChain`. v29's struct is 72 bytes with
  `nextInChain @ +0 / view @ +8 / depthSlice @ +16 + pad /
  resolveTarget @ +24 / loadOp @ +32 / storeOp @ +36 /
  clearValue @ +40`. `rpb_pass_color` / `rpb_pass_color_msaa` now
  write to the correct offsets and `COLOR_ATTACHMENT_SIZE = 72`,
  so the packed array flows straight to wgpu-native.
  Latent bug — render E2E had never run before this release.

### Metrics
- **Modules**: 29 (unchanged)
- **FFI slots**: 65 (was 58 — +7 render pass)
- **Source lines**: ~4,170 (+~70 across render_pass, texture, ffi,
  wgpu_main.c, render_e2e program)
- **Tests**: 343 assertions (was 327 — +16 v2.4.3 regressions)
- **GPU integration**: `make test-phase0` 10/10,
  `make test-compute-e2e` 7/7,
  **`make test-render-e2e` 8/8** — all pass on RADV / Mesa 26.0.
- **Dist bundle**: `dist/mabda.cyr` regenerated
- **v1.0 checklist**: ✅ closed. Every v1.0 criterion mabda can
  cover (non-consumer-side) is now runtime-validated.

### Next
- v2.5.0 — render graph (DAG pass orchestration). Builds on the
  now-stable render_pass + render_pipeline + compute primitives.
  No public API churn expected.

---

## [2.4.2] — 2026-04-20

**GPU runtime validation release.** mabda v2.4.1 shipped with latent
FFI bugs that CPU-only tests couldn't catch — `compute_e2e` and
`phase0` were compile-clean and link-clean but had never executed
against a real wgpu-native + Vulkan driver. Running them for the
first time against a RADV / Mesa 26.0 / kernel 6.18 host exposed a
cascade of latent offset / enum / ABI issues. This release closes all
of them, adds CPU regression assertions that would have caught the
originals, and earns the v1.0 compute-dispatch tick.

v2.4.2 is a **scope re-carve**: the roadmap's original v2.4.2
(render-pass FFI + render E2E) is pushed to v2.4.3. Landing render-pass
FFI on top of broken FFI infrastructure would have amplified the same
offset/enum classes across a wider surface. This release is the
provable foundation v2.4.3 can build on.

### Changed (toolchain)
- **Toolchain pin** `cyrius = "5.4.10" → "5.5.11"` in `cyrius.cyml`.
  Picks up `fncall7` / `fncall8` (scalar-only, not AAPCS64-compatible
  past arg 6 on aarch64) plus the stdlib's updated `lib/fnptr.cyr`
  header documenting the struct-by-value ABI handshake. The
  "fncall6 + wgpu" crash class is now understood as a struct-by-value
  passing mismatch, not a cyrius bug — rationale in
  `docs/archive/issues/2026-04-19-fncall6-wgpu-crash-resolution.md`.

### Fixed (FFI runtime validation)
- **`deps/wgpu_main.c` — Vulkan-only backend.** `wgpuCreateInstance(NULL)`
  used the default `InstanceBackend_All`, which tries GLES; Mesa's EGL
  init path crashes on hosts without a live DISPLAY / Wayland socket.
  `preinit_gpu` now passes a `WGPUInstanceDescriptor` with a chained
  `WGPUInstanceExtras { backends = WGPUInstanceBackend_Vulkan }`.
  Deterministic and headless-safe.
- **`Makefile` — localize `strstr`.** Cyrius stdlib exports `strstr`
  as a GLOBAL symbol. When linked with `wgpu_main.o`, cyrius's
  implementation was interposing libc's `strstr`, and Mesa's Vulkan
  init path calls `strstr` during driver-string probing. The incompatible
  implementation crashed the adapter-enumeration path. Added `strstr` to
  `LOCALIZE_SYMS` so `objcopy -L` hides it from the linker.
- **`src/wgpu_descriptors.cyr` — `wgpu_bgl_entry_buffer` offsets.**
  `WGPUBufferBindingLayout` has an 8-byte `nextInChain` pointer first;
  `type` / `hasDynamicOffset` / `minBindingSize` belong at +40 / +44 /
  +48 of the outer `WGPUBindGroupLayoutEntry`, not +32 / +36 / +40.
  Pre-fix, the buffer-type value was written into the `nextInChain`
  pointer slot, producing a non-null garbage pointer that wgpu_core
  rejected as an invalid chained struct. Header comment now lists the
  full 120-byte layout including v29's `bindingArraySize @ +24`.
- **`src/wgpu_types.cyr` — `WGPUBufferBindingType` renumbered.** v29
  inserted `BindingNotUsed = 0`, shifting every subsequent value up.
  Pre-fix mabda had `UNIFORM = 1` / `STORAGE = 2`; v29 expects
  `UNIFORM = 2` / `STORAGE = 3`. The runtime silently treated every
  storage binding as a uniform binding, which is what surfaced as
  "Storage class Uniform doesn't match the shader" on real dispatch.
- **`src/wgpu_types.cyr` — `WGPULoadOp` swap.** `LOAD` and `CLEAR`
  were swapped (`CLEAR = 1`, `LOAD = 2`). v29 has `LOAD = 1`,
  `CLEAR = 2`. Silent data-corruption bug in render passes —
  CLEAR-configured attachments would have loaded instead, and vice
  versa. Latent because no render E2E runtime test exists yet; the
  enum audit caught it before v2.4.3's render pass ever ran.
- **`src/compute.cyr` — `compute_dispatch` signature reduced to 5
  parameters.** The previous `(device, queue, cp, bg, x, y, z)`
  7-parameter form was documented in `feedback_cyrius_param_ceiling`
  as a crash class — Cyrius functions with 7+ parameters that
  internally `fncall*` into wgpu-native segfault on the wgpu call.
  At 5.5.11 the crash is still present (re-verified). Refactored
  to `(device, queue, cp, bg, dims_xyz)` where `dims_xyz` is a
  pointer to 12 bytes holding three packed u32 workgroup counts.
  **Breaking** — all callers and the `test_audit_compute_dispatch_*`
  assertions updated.
- **`programs/phase0.cyr`** and **`programs/compute_e2e.cyr`** — added
  missing `include "lib/sakshi.cyr"`. Both programs use selective
  includes; when v2.4.1 wired sakshi into `src/error.cyr` /
  `src/context.cyr` / `src/profiler.cyr`, the programs silently
  compiled with undefined `sakshi_*` references until 5.5.11's
  stricter `cyrius check` escalated them to errors.
- **`Makefile` `build-gpu-programs`** — the CI gate now ignores
  warnings whose path begins with `lib/` (stdlib-originated, tracked
  upstream) so a stdlib-side warning cannot break mabda's gate.

### Added
- **18 new CPU regression assertions** in `tests/tcyr/mabda.tcyr`
  (309 → 327), all under a new `v2.4.2 — GPU runtime validation
  regressions` section:
  - `test_audit_buffer_binding_type_values` (5) — asserts every v29
    value of `WGPUBufferBindingType` end-to-end.
  - `test_audit_load_op_values` (6) — asserts `WGPULoadOp` /
    `WGPUStoreOp` values match v29.
  - `test_audit_bgl_entry_buffer_offsets` (7) — asserts
    `wgpu_bgl_entry_buffer` writes go to the correct offsets
    (`type@+40`, `hasDynOffset@+44`, `minSize@+48`).
  - Updated `test_audit_compute_dispatch_*` to use the new
    `dims_xyz` pointer API.

### Breaking
- `compute_dispatch(device, queue, cp, bg, x, y, z)` →
  `compute_dispatch(device, queue, cp, bg, dims_xyz)`.
  **Migration:**
  ```cyr
  var dims[12];
  store32(&dims, x);
  store32(&dims + 4, y);
  store32(&dims + 8, z);
  compute_dispatch(device, queue, cp, bg, &dims);
  ```
  Consumers using the `ping_pong_*` family of compute helpers are
  unaffected — those wrap `compute_dispatch` internally and their
  public signatures haven't changed.

### Unblocked (toolchain-side)
- `_cyrius_init` GLOBAL emission in `object;` mode — fixed in
  cyrius 5.4.9, confirmed at 5.5.11.
- `fncall6 + wgpu-native` crash — reclassified: SysV / AAPCS64
  struct-by-value ABI mismatch, not a cyrius bug.
- 7-parameter Cyrius function + wgpu fncall crash — re-verified at
  5.5.11 (still real). `feedback_cyrius_param_ceiling` stays valid.

### Notes
- Stdlib at 5.5.11 emits `warning:lib/syscalls_x86_64_linux.cyr:358:
  syscall arity mismatch` on any build including `lib/syscalls.cyr`.
  Filtered out in `build-gpu-programs`; to report upstream.
- v1.0 checklist: **compute dispatch end-to-end** now ticked.
  Render pipeline end-to-end (the last open item) moves to v2.4.3.

### Metrics
- **Modules**: 29 (unchanged)
- **Source lines**: ~4,100 (+~30 across descriptor offsets,
  compute_dispatch refactor, new comments)
- **Tests**: 327 assertions (was 309 — +18 v2.4.2 regressions)
- **GPU integration**: `make test-phase0` 10/10 pass,
  `make test-compute-e2e` 7/7 pass on a RADV / Mesa 26.0 / kernel 6.18 host
- **Dist bundle**: `dist/mabda.cyr` regenerated

### Next
- v2.4.3 — render-pass FFI expansion + render E2E (closes v1.0). Full
  plan already in `docs/archive/proposals/2026-04-19-render-pass-ffi.md` — the
  foundation it builds on is now proven.
- v2.5.0 — render graph

---

## [2.4.1] — 2026-04-19

Sakshi observability wiring. Mabda's existing error / profiler /
context plumbing now emits structured sakshi events when the
consumer opts in. No public API changes; default behaviour stays
silent. Earns the sakshi include that's been part of the mabda
include chain since v2.1.1.

### Added
- **`mabda_observability_enable()` / `mabda_observability_disable()`
  / `mabda_observability_is_enabled()`** in `src/error.cyr` — opt-in
  gating for the new emission paths. Independent of sakshi's own
  level / output configuration so consumers can keep mabda silent
  even with sakshi otherwise active.
- **`_sk_emit_err(code)` + `_sk_info_cstr(msg)`** — internal helpers
  that route mabda events to sakshi. Recoverable GpuErr codes
  emit `sakshi_warn`; non-recoverable emit `sakshi_error`. Both
  use `gpu_err_name(code)` so the event message matches the
  human-readable code name.
- **`profiler_begin_frame` / `profiler_end_frame` sakshi spans** —
  wrapped with `sakshi_span_enter("frame", 5) / sakshi_span_exit()`
  when observability is enabled. Trace consumers get per-frame
  timing for free; profiler's existing CPU timing math is untouched.
- **`gpu_context_from_preinit` success path** emits
  `sakshi_info("mabda: gpu context created")`.
- **`gpu_context_release`** emits
  `sakshi_info("mabda: gpu context released")`.
- **6 new CPU assertions** in `tests/tcyr/mabda.tcyr` (303 → 309)
  covering: default-disabled state, enable/disable flag flips,
  disabled-no-emission contract, enabled-emits-on-err contract,
  frame span depth invariant.

### Notes
- Failure paths in `gpu_context_from_preinit` route through
  `gpu_err_result(...)`, which already calls `_sk_emit_err`. No
  duplicate emission.
- Tests use `sakshi_output_buffer()` + `sakshi_ring_*` to verify
  emission counts without polluting test stderr.

### Metrics
- **Modules**: 29 (unchanged)
- **Source lines**: ~4,100 (+~50 across error/profiler/context)
- **Tests**: 309 assertions (was 303 — +6 observability)
- **Dist bundle**: `dist/mabda.cyr` regenerated

### Next
- v2.4.2 — render-pass FFI expansion + render E2E (closes v1.0)
- v2.5.0 — render graph

---

## [2.4.0] — 2026-04-19

v1.0-parity (partial) closeout. Picks off the v1.0 criteria the
existing FFI surface can already reach: compute dispatch end-to-end
plus the scheduled LOW audit sweep. Render-pipeline E2E deferred to
v2.4.2 (needs render-pass FFI expansion — see roadmap and
`docs/archive/issues/2026-04-19-phase0-build-broken.md`).

### Added
- **`programs/compute_e2e.cyr`** — compute dispatch end-to-end
  GPU integration test: write → bind → dispatch → copy → map →
  verify. WGSL shader doubles every u32 element; readback asserts
  every element matches `2 * input`. Mirrors the existing
  buffer round-trip in `programs/phase0.cyr`.
- **`make build-gpu-programs`** CI gate — `cyrius check` every
  `programs/*.cyr` and fail on any warning. Closes the missing-include
  class of bug surfaced as Issue 2 in
  `docs/archive/issues/2026-04-19-phase0-build-broken.md`. Runnable on CI
  without `wgpu-native`.
- **`make test-compute-e2e`** + **`make test-render-e2e`** + **`make
  test-gpu`** Makefile targets. Pattern rule for `build/%.o`
  generalises the phase0 build to any `programs/*.cyr`.
- **`docs/archive/issues/2026-04-19-phase0-build-broken.md`** — internal
  issue doc tracking the cyrius `_cyrius_init`-LOCAL regression
  (fixed upstream in cyrius v5.4.9), the `lib/str.cyr` missing-include
  bug in `programs/phase0.cyr` (fixed mabda-side), and the queued
  `cyrius build --strict` enhancement.
- **17 new audit-regression assertions** in `tests/tcyr/mabda.tcyr`
  (286 → 303), one or more per LOW fix below.

### Fixed (LOW)
- **LOW-2 `read_buffer` size cap** (`src/buffer.cyr`). New
  `read_buffer_capped(device, queue, buffer, size, max_bytes)`
  rejects `size <= 0`, `size > max_bytes`, and `size >
  wgpu_buffer_get_size(buffer)` before allocating staging or host
  memory. The existing `read_buffer(...)` now delegates to it with a
  256 MB default cap (matches WebGPU `maxStorageBufferBindingSize`
  default). Regression: `test_audit_read_buffer_zero_size_rejected`,
  `test_audit_read_buffer_exceeds_cap_rejected`.
- **LOW-3** `validate_dispatch` / `validate_dimensions` wired into
  the internal dispatchers (`src/compute.cyr`, `src/texture.cyr`).
  `compute_dispatch` short-circuits on `<= 0` or `> 65535`
  workgroup counts; `texture_create_rgba8` short-circuits on `<= 0`
  or `> 8192` dimensions. Both match the WebGPU spec minimum.
  Regressions: `test_audit_compute_dispatch_zero_dim_rejected`,
  `test_audit_compute_dispatch_exceeds_max_rejected`,
  `test_audit_texture_create_exceeds_max_rejected`.
- **LOW-4 bounded `_wgpu_strnlen`** (`src/wgpu_descriptors.cyr`).
  `wgpu_string_view` now bounds its strlen at 4 KB
  (`WGPU_LABEL_MAX_BYTES`) so a corrupt or non-null-terminated label
  cannot walk off mapped memory. Regressions:
  `test_audit_strnlen_short_string`, `test_audit_strnlen_caps_at_max`.
- **LOW-5 `compute_pipeline_new` failure-path cleanup**
  (`src/compute.cyr`). Each early-return between BGL / pipeline
  layout / shader / pipeline creation now releases the wgpu handles
  it has accumulated so far, plus an upfront `storage_count <= 0`
  guard. Regression:
  `test_audit_compute_pipeline_zero_storage_rejected`.
- **LOW-6 `_clamp_unit` in `texture_from_color`**
  (`src/texture.cyr`). f64 RGBA components outside `[0.0, 1.0]` are
  clamped before the u8 conversion so wrapping arithmetic
  (`1.5 → 382 → 126`) cannot produce a garbage pixel. Regressions:
  `test_audit_clamp_unit_in_range`, `test_audit_clamp_unit_above_one`,
  `test_audit_clamp_unit_below_zero`.

### Fixed (other)
- **`programs/phase0.cyr`** — added missing `include "lib/str.cyr"`.
  Phase0 used `str_builder_*` / `str_cstr` for the WGSL shader source
  (added when the literal was split across lines) but the include
  block hadn't been updated. Linker failed with 8 undefined-references
  on a clean build. Mabda-side fix; root-cause Issue 2 in
  `docs/archive/issues/2026-04-19-phase0-build-broken.md`.

### Changed
- **Toolchain pin** `cyrius = "5.4.7" → "5.4.10"` in `cyrius.cyml`.
  Picks up the v5.4.9 fix for `_cyrius_init` GLOBAL emission in
  `object;` mode (Issue 1 in the `phase0-build-broken` doc), plus
  the v5.4.10 `lib/thread.cyr` post-clone child-path fix.
- **Makefile** — `build/phase0.o` rule generalised to a `build/%.o`
  pattern rule covering all `programs/*.cyr`. New per-program link
  rules + phony test targets follow the same template.

### Metrics
- **Modules**: 29 (unchanged)
- **Source lines**: ~4,050 (+50 across LOW fixes)
- **Tests**: 303 assertions (was 286 — +17 LOW-sweep regressions)
- **Programs**: 3 (was 2 — added `compute_e2e.cyr`)
- **Dist bundle**: `dist/mabda.cyr` regenerated

### Next
- v2.4.1 — sakshi observability (additive)
- v2.4.2 — render-pass FFI expansion + render E2E (closes v1.0)
- v2.5.0 — render graph

---

## [2.3.0] — 2026-04-19

P(-1) scaffold-hardening release. Last audit-gated milestone before
mabda is promoted to first-party trusted stdlib status alongside
yukti / patra / sakshi. Full findings in
[`docs/audit/2026-04-19-audit.md`](docs/audit/2026-04-19-audit.md) —
2 HIGH + 6 MED + 6 LOW across 29 modules; every HIGH and MED fixed.

### Added
- **`docs/audit/2026-04-19-audit.md`** — full security audit report
  (scope, methodology, findings with severity / file / lines / class,
  CVE sweep, remediation plan, non-findings).
- **`CLAUDE.md` P(-1) + Security Hardening sections** — the 10-point
  release checklist mabda now enforces before every minor bump.
- **13 new audit-regression assertions** in `tests/tcyr/mabda.tcyr`
  (273 → 286), one per HIGH / MED fix.
- **`storage_buffer_wrap_raw`** — unchecked byte-oriented wrapper
  for callers that previously relied on `storage_buffer_wrap`'s
  byte-oriented convenience path. The public `storage_buffer_wrap`
  now enforces `capacity ≥ count × element_size` and overflow-safety.

### Fixed (HIGH)
- **HIGH-1 `surface_state_present` name collision** (`src/surface.cyr`).
  The mutating present helper shadowed the present-mode accessor,
  so `_surface_state_configure` was (accidentally) calling the
  present function and configuring the surface with `present_mode = 0`.
  Mutating helper renamed to `surface_state_submit_present`; accessor
  unchanged. Regression: `test_audit_surface_present_accessor`.
- **HIGH-2 `rpb_label` 4-byte heap overflow** (`src/render_pipeline.cyr`).
  Builder allocation bumped `alloc(80)` → `alloc(88)` so the label
  slot at `+76` (8 bytes) fits. Regression: `test_audit_rpb_label_fits`.

### Fixed (MEDIUM)
- **MED-1** `workgroups_1d` / `workgroups_2d` return 0 on zero
  workgroup size instead of SIGFPE-ing (`src/buffer.cyr`).
- **MED-2** `growable_buffer_update` detects signed-i64 overflow on
  `cap * 2` and falls back to `size` (`src/buffer.cyr`).
- **MED-3** `texture_upload_rgba8` short-circuits on zero / negative /
  past-i32 dimensions before handing to wgpu-native (`src/texture.cyr`).
- **MED-4** `storage_buffer_write` rejects
  `write_count × element_size` that would overflow i64
  (`src/typed_buffer.cyr`).
- **MED-5** `storage_buffer_wrap` validates
  `capacity ≥ count × element_size` at wrap time and clamps
  `element_size` to 1 on inconsistency; unchecked variant preserved
  as `storage_buffer_wrap_raw` for internal byte-oriented use
  (`src/typed_buffer.cyr`).
- **MED-6** `_time_now_ns` zeroes its timespec before the
  `clock_gettime` syscall so a failure returns 0 instead of stack
  garbage (`src/profiler.cyr`).

### Fixed (LOW)
- **LOW-1** `GpuCapabilities` struct header comment corrected from
  "128 bytes" to "120 bytes" to match the actual `alloc(120)`
  (`src/capabilities.cyr`).

### Scheduled (LOW, not blocking 2.3.0)
- **LOW-2** `read_buffer` size cap
- **LOW-3** wire `validate_dispatch` / `validate_dimensions` into
  internal dispatchers
- **LOW-4** bounded `strlen` in `wgpu_string_view`
- **LOW-5** resource cleanup on `compute_pipeline_new` failure paths
- **LOW-6** clamp color components in `texture_from_color`

### Metrics
- **Modules**: 29 (unchanged)
- **Source lines**: ~4,000 (unchanged)
- **Tests**: 286 assertions (was 273 — +13 audit regressions)
- **Dist bundle**: `dist/mabda.cyr` regenerated

### Promotion note
Mabda 2.3.0 is the last stdlib-candidate release requiring an
external audit gate. Starting with 2.4.0, mabda is treated as a
first-party trusted dependency: the Security Hardening checklist in
`CLAUDE.md` is the internal gate, and the audit artefact moves to a
rolling review rather than a release-blocking pass.

## [2.2.0] — 2026-04-19

Project scaffolding brought in line with the first-party AGNOS convention
(yukti / vidya / patra). Toolchain pin jumps from Cyrius 3.4.19 to 5.4.7.
No library API changes — every call site in soorat, rasa, ranga, bijli,
and aethersafta keeps working without modification.

### Added
- **`cyrius.cyml`** replaces `cyrius.toml`. Version is pulled from
  `VERSION` via `${file:VERSION}` so a single file is the source of
  truth. `[deps] stdlib = [...]` declares the stdlib modules mabda
  needs; `cyrius deps` resolves them against the toolchain.
- **`tests/tcyr/mabda.tcyr`** — single consolidated CPU-only suite
  covering error, color, capabilities, profiler, typed_buffer, vertex,
  state (blend/sampler/depth), caches, surface. 273 assertions.
- **`tests/bcyr/mabda.bcyr`** — moved into its conventional subdirectory;
  run via `cyrius bench tests/bcyr/mabda.bcyr`.
- **`programs/smoke.cyr`** — link-check program that includes
  `src/lib.cyr` and exits 0. Gives CI an entry point for
  `cyrius build` without inventing a fake CLI.
- **`programs/phase0.cyr`** — GPU integration test (renamed from
  `tests/test_phase0.tcyr`). Still compiled via the Makefile's C-launcher
  path because it links against wgpu-native.
- Flat layout: `src/lib.cyr` (renamed from `src/mabda.cyr`) declares the
  full include chain; domain modules remain flat (zero transitive
  includes) so `cyrius distlib` can concatenate them cleanly.

### Changed
- **Toolchain pin**: `cyrius = "5.4.7"` in `cyrius.cyml` (was `3.4.19`).
- **CI** (`.github/workflows/ci.yml`) reworked to match yukti:
  lint, fmt-check, vet, dist-in-sync check (`cyrius distlib` diff-clean
  against `dist/mabda.cyr`), link-check build, `cyrius test`, `cyrius
  bench`, security scan, docs/version-consistency gate.
- **Release** (`.github/workflows/release.yml`) rewritten around
  `cyrius distlib` — regenerates `dist/mabda.cyr` and attaches it to
  the GitHub Release alongside the source tarball.
- **Makefile** shrunk to a thin wrapper over the `cyrius` CLI; the GPU
  integration path (`make test-phase0`) retained for local dev.
- **`scripts/bundle.sh`** removed — `cyrius distlib` handles bundling.
- **`scripts/version-check.sh`** targets `cyrius.cyml` and accepts the
  `${file:VERSION}` templated form.
- **`scripts/version-bump.sh`** now only touches `VERSION` (the manifest
  reads from it).

### Removed
- `cyrius.toml` — replaced by `cyrius.cyml`.
- `src/tagged_obj.cyr` — internal object-mode tagged-union scaffolding
  that hasn't been referenced since the FFI rework; the `tagged` stdlib
  covers every remaining caller.
- Ten per-module test files (`tests/test_*.tcyr`) — folded into
  `tests/tcyr/mabda.tcyr`. dynlib's tests are dropped from the mabda
  suite since dynlib is a stdlib concern.

### Not breaking
- `dist/mabda.cyr` is regenerated but the exported API surface
  (`gpu_context_from_preinit`, `wgpu_*`, `color_*`, `storage_buffer_*`,
  `render_pipeline_create_simple`, …) is byte-identical at the function
  signature level. Consumers pinning `[deps.mabda] tag = "2.2.0"` only
  need to bump the tag.

## [2.1.2] — 2026-04-12

Rust source removal release. The frozen `rust-old/` tree is gone from the
working tree; the full Rust v1.0.0 source remains accessible via
`git checkout 1.0.0`. This is a hygiene release — no library code changes,
no API changes, no test changes.

### Removed
- **`rust-old/`** — 9,261 LOC of frozen Rust source + ~5.4 GB of build
  artifacts under `target/`. The Rust source was purely reference material
  after the v2.0.0 port shipped; every one of the 25 Rust modules had a
  Cyrius counterpart. Archaeology is preserved via `git checkout 1.0.0`.

### Preserved before removal
- **`docs/rust-v1-bench-history.csv`** — `git mv` of the original Rust
  benchmark CSV (68 lines, 4 real runs across commits `4a802cd`,
  `ba81a3e`, `19d8b66`, `f113c93` on 2026-03-30). Cited as the reference
  dataset in `docs/benchmarks-rust-v-cyrius.md`.
- **Rust v1.0 line coverage snapshot** — 1,034 / 1,367 lines (75.6%),
  extracted from `rust-old/target/tarpaulin/mabda-coverage.json` and
  inlined into `docs/benchmarks-rust-v-cyrius.md` as a per-module table
  before the target/ tree was dropped. Only line-coverage data point
  available for the v1.0 reference implementation.

### Dropped without preservation
- `rust-old/target/debug/` (4.9 GB) — debug build artifacts
- `rust-old/target/release/` (503 MB) — release build artifacts
- `rust-old/target/criterion/` (5.7 MB) — detailed Criterion stats
  (point estimates already captured in `bench-history.csv`)
- `rust-old/target/doc/` (5.9 MB) — `cargo doc` HTML output, regeneratable
- `rust-old/benchmarks.md` — content already in `docs/benchmarks-rust-v-cyrius.md`
- `rust-old/Cargo.{toml,lock}`, `codecov.yml`, `deny.toml`,
  `rust-toolchain.toml`, `Makefile`, `scripts/*.sh` — Rust-specific
  tooling, no Cyrius equivalent

### Changed
- **`README.md`** — rewrote the stale Project Structure section (still
  showed the pre-flatten `cyr/` subdirectory from v1.x) with the current
  flat layout including `dist/`, `examples/`, and `scripts/`. Build
  instructions updated to use `cyrius audit` and `make test-all`.
  Added pointers to the `@public`/`@internal` marker system,
  ADR-005, the stdlib integration guide, and `git tag 1.0.0` for Rust
  archaeology. Minimum Cyrius version bumped `3.4.14` → `3.4.19` to
  match `cyrius.toml`.
- **`CLAUDE.md`** — project structure diagram updated to include `dist/`,
  `examples/`, and the version-check/bundle Make targets. `rust-old/`
  entry removed; replaced with a note about `git checkout 1.0.0`.
- **`.gitignore`** — dropped the `rust-old/target/` and
  `rust-old/Cargo.lock` lines.
- **`docs/benchmarks-rust-v-cyrius.md`** — title now reads "Rust v1.0 vs.
  Cyrius v2.1" (was "vs. Cyrius v2.0"). `rust-old/` path references
  rewritten to cite the preserved CSV and `git tag 1.0.0`. New "Rust v1.0
  line coverage" section with the 24-module table.
- **Test docstrings** — the six test files that cited "Ported from
  rust-old/src/..." now say "Ported from the Rust v1.0.0 ... — see git
  tag 1.0.0" instead. Affects `test_error.tcyr`, `test_capabilities.tcyr`,
  `test_state.tcyr`, `test_caches.tcyr`, `test_surface.tcyr`,
  `mabda.bcyr`.

### Stats
- **-9,261 LOC** of Rust source removed from the working tree
- **-5.4 GB** of build artifacts reclaimed on disk (already gitignored,
  but no longer sitting on the filesystem)
- `cyrius audit` — still 14/14 pass, 290 assertions green
- `dist/mabda.cyr` — unchanged (byte-identical regen from `src/`)

### How to reach the deleted files
```sh
git checkout 1.0.0          # the entire Rust v1.0.0 tree
git log --all -- rust-old/  # every commit that touched rust-old
```

## [2.1.1] — 2026-04-12

Stdlib inclusion release. Mabda is now consumable as a Cyrius stdlib dep
via `[deps.mabda]` in downstream `cyrius.toml` files. Cyrius 3.4.19 has
already staged the dep entry; when 3.4.19 ships it becomes active and
`cyrius deps` will resolve it automatically.

### The transitional backend callout

**Mabda's wgpu-native C launcher is transitional scaffolding, not the
long-term design.** The public API (`@public` files in `src/`) is the
stability boundary. When the pure-Cyrius GPU backend lands in v3.0,
the launcher, the `deps/wgpu-native/` binaries, the libC dependency,
and the FFI layer all go away — and every consumer that only touches
the `@public` API recompiles without edits. The `examples/stdlib-consumer/`
project is the regression test for that contract.

### Added
- **`dist/mabda.cyr`** — single-file bundled distribution (~141 KB,
  29 modules concatenated in `src/mabda.cyr` include order). Strips
  per-module `include` lines; consumer supplies stdlib via their own
  `cyrius.toml`.
- **`scripts/bundle.sh`** — reproducible bundler. Byte-identical output
  given an unmodified `src/` tree. Idempotent. Intentionally minimal
  (no banner, no per-module separators) because a larger-format bundle
  tripped cc3's token buffer limit during development.
- **`[lib]` section in `cyrius.toml`** — declares the module graph for
  `cyrius deps` consumers. Lists all 29 modules in dependency order.
- **`src/*.cyr` public/internal markers** — every file gets a line-1
  comment: `# @public — stable API surface` (26 files) or `# @internal —
  FFI / toolchain scaffolding, replaced in v3.0` (5 files: `wgpu_types`,
  `wgpu_descriptors`, `wgpu_ffi`, `tagged_obj`, `cache_key`). Consumer
  docs instruct "do not reference `@internal`."
- **`examples/stdlib-consumer/`** — minimal "hello GPU" example
  (`cyrius.toml` + `src/main.cyr` + `README.md`) that consumes mabda via
  the stdlib-dep path. Proves the stdlib-inclusion contract end-to-end
  and serves as the v3.0 regression test.
- **`docs/stdlib-integration.md`** — consumer guide. Covers declaring
  the dep, writing consumer code against the `@public` API, building
  the (transitional) C launcher, and what specifically disappears in
  v3.0. Clearly labels every transitional section.
- **`docs/adr/005-public-api-surface-marking.md`** — ADR capturing the
  `@public`/`@internal` marker decision, the v2.1.1 inventory, and the
  v3.0 migration checklist.
- **`scripts/version-check.sh`** — fails `make test-all` if `VERSION`,
  `cyrius.toml`, `CHANGELOG.md`, or `README.md` disagree on the version
  number. Prevents future drift.

### Changed
- **`cyrius-version` bumped `3.4.12` → `3.4.19`.** 3.4.19 is the release
  that activates `[deps.mabda]` as a first-class Cyrius stdlib dep.
- **Line-length and naming-convention lint warnings eliminated.** 16
  warnings in v2.1.0 (line length in `blend`, `color`, `compute`,
  `wgpu_ffi`; PascalCase `GpuOk`/`GpuErr`/`GpuErrMsg` in `error`).
  Renamed to `gpu_ok`/`gpu_err_result`/`gpu_err_result_msg` across all
  src files and tests. Lint now clean.
- **Format pass across `gpu_timestamps`, `profiler`, `render_pipeline`,
  `surface`, `texture`.** `cyrius fmt` now reports clean on all `src/`
  files.

### Stats
- `cyrius audit` — 14/14 pass (compile, 11 test suites, lint, fmt)
- `dist/mabda.cyr` — 4,025 lines, 141,912 bytes, compiles cleanly as a
  single bundle with zero errors (~29 expected `undefined function`
  warnings for the FFI slot externals, documented as benign in
  `docs/stdlib-integration.md`)
- 11 test binaries, still 290 assertions (no test churn in v2.1.1)
- 26 `@public` files + 5 `@internal` files in `src/`
- Version sync enforced by `scripts/version-check.sh`

## [2.1.0] — 2026-04-12

v2.1.0 is the Rust-parity catch-up release. All seven v2.1 roadmap items
landed along with a batch of v29 API-value fixes surfaced by the first
real GPU-backed uses.

### Added
- **`src/typed_buffer.cyr`** — `UniformBuffer` / `StorageBuffer` wrappers with
  runtime alignment validation (16-byte multiple for uniform buffers) and
  capacity-tracking metadata. API: `uniform_buffer_new`, `uniform_buffer_write`,
  `storage_buffer_create`, `storage_buffer_wrap`, `storage_buffer_new`,
  `storage_buffer_empty`, `storage_buffer_write`, accessors, release. Ports
  `rust-old/src/typed_buffer.rs` (352 LOC, 14 tests).
- **`src/gpu_timestamps.cyr`** — GPU timestamp profiling via wgpu's query set +
  resolve buffer + read buffer triple. API: `gpu_timestamps_supported` (device
  feature check), `gpu_timestamps_new`, `gpu_timestamps_resolve`,
  `gpu_timestamps_map`/`unmap`, `gpu_timestamps_release`.
- **Texture FFI** — `wgpuDeviceCreateTexture`, `wgpuTextureCreateView`,
  `wgpuDeviceCreateSampler`, `wgpuQueueWriteTexture` (struct-packed shim),
  `wgpuTextureRelease`, `wgpuTextureViewRelease`, `wgpuSamplerRelease` wired
  through slots 45–51 of the function table. `texture.cyr` rewrite exposes
  `texture_create_rgba8`, `texture_view_create_rgba8`, `texture_upload_rgba8`,
  `texture_from_rgba` convenience wrapper, and `texture_release`.
- **Render pipeline FFI** — `wgpuDeviceCreateRenderPipeline` +
  `wgpuRenderPipelineRelease` at slots 52–53. New `render_pipeline_create_simple`
  entry builds the full 168-byte `WGPURenderPipelineDescriptor` (vertex state,
  primitive state, multisample state, fragment state with a single color
  target) and auto-layouts. The legacy `rpb_*` builder API is retained and
  delegates to the simple path for backward compatibility.
- **Surface FFI** — `wgpuSurfaceConfigure`, `wgpuSurfaceGetCurrentTexture`,
  `wgpuSurfacePresent`, `wgpuSurfaceRelease` at slots 54–57. `surface.cyr`
  rewrite wraps configure/acquire/present/release. Since mabda is headless,
  consumers still provide the `WGPUSurface` handle from their windowing
  library; mabda owns the lifecycle after that.
- **`src/cache_key.cyr`** — shared `_hash_to_heap_key` helper used by
  `shader_cache`, `pipeline_cache`, `bind_group_cache`, and `texture` cache.
  Fixes a latent bug where each cache module previously stored a pointer to
  a stack-allocated key buffer that dangled as soon as the setter returned
  (hashmap.cyr::map_set stores pointers without copying). Second-insert
  test case catches the regression.
- **`tests/mabda.bcyr`** — first Cyrius benchmark harness. Batch-timed via
  `lib/bench.cyr` over 100 rounds × 10 000 iterations, covers the 7 CPU-only
  Rust benchmarks: `color_lerp`, `color_from_hex`, `color_luminance`,
  `workgroups_1d`, `workgroups_2d`, `profiler_frame_cycle`, `capabilities_report`.
  Results seeded into `bench-history.csv` (same schema as `rust-old/`).
  Comparison updated in `docs/benchmarks-rust-v-cyrius.md` — Rust's picosecond
  numbers were identified as LLVM having optimised the bodies out.
- **Pure-data test batch** — `test_typed_buffer` (26), `test_error` (31),
  `test_capabilities` (34), `test_state` (blend+sampler+depth, 50),
  `test_caches` (26), `test_surface` (24). **+191 assertions recoverable**
  over v2.0's standalone total.
- **FFI function table grew 40 → 58 slots.** New entries: query set (4),
  device feature check (1), texture (7), render pipeline (2), surface (4).

### Fixed
- **v29 enum value drift** — several constants in `wgpu_types.cyr`, `sampler.cyr`,
  `depth.cyr`, and `render_pipeline.cyr` (pre-existing stub) were set to values
  from an older wgpu version. Re-verified against the v29 header:
  - `WGPUTextureFormat::RGBA8Unorm` 18 → 0x16 (22)
  - `WGPUTextureFormat::BGRA8Unorm` 23 → 0x1B (27)
  - `WGPUTextureFormat::Depth32Float` 39 → 0x30 (48)
  - `WGPUTextureFormat::Depth24PlusStencil8` 41 → 0x2F (47)
  - `WGPUSType::ShaderSourceWGSL` 0x07 → 0x02
  - `WGPUAddressMode::ClampToEdge` 2 → 1
  - `WGPUFilterMode::{Nearest,Linear}` 0/1 → 1/2
  - `WGPUMipmapFilterMode::{Nearest,Linear}` 0/1 → 1/2
  - `WGPUPresentMode::Fifo/FifoRelaxed/Immediate/Mailbox` 2/3/0/1 → 1/2/3/4
  - `WGPUPrimitiveTopology::TriangleList` 3 → 4
  - `WGPUCullMode::None` 0 → 1
  These silently compiled against v29 but would have crashed the first time
  any real FFI call hit them. All caught when the texture + render pipeline
  FFI landed.
- **`WGPUSamplerDescriptor` default init missing `maxAnisotropy=1`** — wgpu v29
  rejects samplers with `maxAnisotropy < 1`. `_sampler_desc_init` now writes
  the default along with `lodMaxClamp=32.0f` to match `WGPU_SAMPLER_DESCRIPTOR_INIT`.
- **Cache dangling-pointer bug** — `shader_cache_set`, `pipeline_cache_set`,
  `bind_group_cache_set`, and `texture_cache_set` passed a `var ibuf[24]`
  stack buffer to `hashmap.cyr::map_set`, which stores key pointers without
  copying. Cross-call the stack slot would alias, causing subsequent lookups
  to miss. Now all four use the shared `_hash_to_heap_key` helper.

### Cyrius language feedback
- **7-parameter functions that fncall into wgpu crash.** Discovered via
  `storage_buffer_new(device, queue, data, count, element_size, label, read_only)`
  — the exact same logic in a helper with ≤4 params worked. Worked around by
  folding parameters into a capacity-based API. Rule is now documented in
  `CLAUDE.md`: any Cyrius function that makes a wgpu `fncall*` must cap at
  6 parameters. Saved as `feedback_cyrius_param_ceiling.md`.

### Stats
- 11 test binaries, **290 assertions** (was 93 at v2.0 ship)
- 27 library modules + 4 FFI modules + 1 cache helper
- 58-slot FFI function table (was 40)
- 5 new struct-packed shims in `wgpu_main.c`
- Device-side full GPU path proven: context → buffer → texture → sampler →
  shader module → render pipeline → release, on a real GPU with no panics

## [2.0.0] — 2026-04-11

### Added — Pre-release Cleanup
- **Buffer readback round-trip test** — `test_phase0.tcyr` now exercises the full
  write → copy → map → verify path on a real GPU device. Closes the v1.0
  completion criterion. 93 tests total passing (89 standalone + 4 GPU).
- **Struct-packing shim pattern** for wgpu entry points with 6+ i64 arguments.
  `wgpu_command_encoder_copy_buffer_to_buffer` and `wgpu_buffer_map_sync` now
  allocate arg structs in Cyrius and call C shims via `fncall2`, routing
  around an `fncall6` + wgpu-native ABI interaction that segfaulted reliably.
  Pattern documented in `docs/architecture/overview.md`.
- **`docs/benchmarks-rust-v-cyrius.md`** — Rust v1.0 vs Cyrius v2.0 reference
  (source size −63%, 20 benchmark numbers from commit `f113c93`, binary
  size comparison, test parity audit).

### Changed — Pre-release Cleanup
- **Flat project layout** — `cyr/{src,lib,tests,deps,Makefile,cyrius.toml}`
  moved to repo root. Matches vidya/cyrius convention. `make test-all` now
  runs from repo root. `lib/` remains a symlink to the upstream Cyrius stdlib
  (overridden in CI to `$HOME/.cyrius/lib`), so mabda never vendors stdlib —
  it always tracks the installed toolchain.
- **Makefile `test-all`** now runs all five test suites (added `test-profiler`
  and `test-vertex` which were previously orphaned in the Makefile).
- **CI workflows** updated for the flat layout. Removed all `working-directory: cyr`
  entries and `cyr/cyrius.toml` / `cyr/src/` path references.

### Fixed — Pre-release Cleanup
- **`vec_get` undefined warning** — `fmt.cyr` and `str.cyr` (from vendored
  cyrius stdlib) reference `vec_get` without declaring it. Tests that use
  those modules now include `lib/vec.cyr` explicitly.
- **Removed crashing `test_syslib`** — `syslib.cyr` and `test_syslib.tcyr`
  deleted. `dynlib.cyr` (already upstreamed to Cyrius 3.4.11) is the
  supported path for dynamic library loading.
- **`wgpu_queue_submit_one`** — replaces the old array-based `wgpu_queue_submit`
  for the single-command-buffer case. C shim allocates the 1-element array
  itself, avoiding one more Cyrius-side alloc in the hot path.

### Added — Cyrius Language Port

Complete port of mabda from Rust to Cyrius. 25 modules, 3,274 lines of Cyrius source,
701 lines of tests. GPU FFI via wgpu-native C API linked through a C shim.

#### Core Modules
- **error.cyr** — 18 GPU error codes with Result type via tagged unions, `gpu_err_is_recoverable()`
- **color.cyr** — Color struct (f64 internally), f64↔f32 conversion, hex/rgba8 parsing, lerp, luminance, 7 preset colors
- **context.cyr** — GpuContext lifecycle (instance/adapter/device/queue handles), `gpu_context_from_preinit()`
- **capabilities.cyr** — GpuCapabilities struct (13 fields), validation helpers, WebGPU compatibility constants
- **profiler.cyr** — FrameProfiler with EMA smoothing, frame history ring buffer, explicit `profile_begin()`/`profile_end()`
- **resource.cyr** — FrameResources for transient GPU buffer/texture tracking

#### Buffer & Compute
- **buffer.cyr** — 7 buffer creation helpers, synchronous readback, GrowableBuffer with generation counter, workgroup math (`workgroups_1d`, `workgroups_2d`, `validate_dispatch`)
- **compute.cyr** — ComputePipeline creation with bind group layouts, dispatch, PingPongBuffer for iterative compute
- **shader_cache.cyr** — FNV-1a hash-based shader module deduplication
- **pipeline_cache.cyr** — hash-based render/compute pipeline deduplication
- **bind_group_cache.cyr** — hash-based bind group caching with clear

#### Graphics
- **vertex.cyr** — Vertex2D (32B), Vertex3D (48B) with f32 layout, attribute descriptor builders
- **blend.cyr** — 5 blend presets (Opaque, AlphaBlend, PremultipliedAlpha, Additive, Multiply)
- **sampler.cyr** — 4 sampler presets (Nearest, Linear, Anisotropic, Comparison) with WGPUSamplerDescriptor builders
- **depth.cyr** — DepthTexture struct, format constants, depth stencil state builder
- **texture.cyr** — Texture struct (handle/view/sampler), TextureCache, mip level count, dimension validation
- **bind_group.cyr** — BindGroupLayoutBuilder with fluent API (uniform, storage, texture, sampler entries)
- **render_target.cyr** — RenderTarget struct with MSAA support, RenderTargetBuilder
- **render_pipeline.cyr** — RenderPipeline + RenderPipelineBuilder (vertex layout, color target, depth, cull, topology), DrawCommand enum
- **render_pass.cyr** — RenderPassBuilder with color/depth/MSAA attachments
- **surface.cyr** — SurfaceState for window surface lifecycle, PresentModePreference
- **instancing.cyr** — InstanceData (80B: 4x4 matrix + RGBA), attribute layout, InstanceBuffer
- **debug.cyr** — GPU debug group push/pop/marker stubs

#### FFI Layer
- **wgpu_types.cyr** — wgpu-native v29 C API enum constants (BufferUsage, MapMode, TextureFormat, ShaderStage, etc.)
- **wgpu_descriptors.cyr** — C struct builders for all wgpu descriptor types, verified via offsetof() test program (386 lines)
- **wgpu_ffi.cyr** — Function table-based FFI — C launcher populates 40 wgpu function pointers, Cyrius calls via fncall0-6
- **wgpu_main.c** — C launcher: GPU pre-init, simplified shim wrappers for by-value struct callbacks, function table export
- **tagged_obj.cyr** — Runtime-initialized tagged unions for object mode compatibility

#### Stdlib Contributions (upstreamed to Cyrius)
- **dynlib.cyr** — Pure Cyrius ELF .so loader via mmap (Cyrius 3.4.11, Module #40)
- **syslib.cyr** — System dlopen/dlsym wrapper via libc (pending stdlib merge)

#### Infrastructure
- **cyrius.toml** — Cyrius build configuration
- **Makefile** — Hybrid C/Cyrius build: `test-color`, `test-profiler`, `test-vertex`, `test-dynlib`, `test-phase0`
- **deps/fetch-wgpu.sh** — Downloads wgpu-native v29 pre-built binaries
- **deps/print_offsets.c** — C program to verify wgpu struct field offsets
- **deps/wgpu_shim.c** — C shim for by-value struct callback wrapping

#### Testing
- 89 standalone test assertions (color 48, profiler 15, vertex/blend 19, dynlib 7)
- 3 GPU integration tests (context create, buffer create+release, buffer write)
- All tests passing on Cyrius 3.4.14

### Changed
- **Project structure** — Rust source moved to `rust-old/`, Cyrius port in `cyr/`
- **Starship prompt** — Added `𝕮` icon for Cyrius language detection via `cyrius.toml`

### Breaking
- **Language** — Crate is now a Cyrius library, not a Rust crate. Consumers must port to Cyrius.

### Cyrius Compiler Contributions
- **PIC codegen** (Cyrius 3.4.12) — `object;` mode emits `LEA [rip+disp32]` with R_X86_64_PC32 for data/string/fnptr refs, eliminating DT_TEXTREL
- **Symbol clash fix** (Cyrius 3.4.12) — `mmap`/`munmap`/`mprotect` renamed to `cyr_*` in stdlib to avoid libc conflicts
- **`_cyrius_init` export** (Cyrius 3.4.14) — Top-level code wrapped as callable function in object mode with proper prologue/epilogue
- **GPU discovery** (Yukti 1.2.0) — `gpu.cyr` module for sysfs-based GPU enumeration

## [1.0.0] — 2026-04-09

Rust v1.0.0 release. Full GPU foundation library with 25 modules, 278 tests,
20 benchmarks. See `rust-old/` for complete Rust source.

### Added
- All Rust modules: context, error, capabilities, color, buffer, typed_buffer,
  compute, texture, render_target, render_pipeline, render_pass, depth, vertex,
  sampler, surface, blend, bind_group, instancing, profiler, shader, pipeline_cache,
  bind_group_cache, debug, resource
- CI/CD pipeline, coverage tracking, security audit
- ADR-001 (public fields), ADR-002 (runtime alignment), ADR-003 (fixed vertex types)

## [0.1.0] — 2026-03-29

### Added
- Initial Rust implementation: context, compute, buffer, texture, render_target,
  profiler, capabilities, color, error

---

[Unreleased]: https://github.com/MacCracken/mabda/compare/2.5.0...HEAD
[2.5.0]: https://github.com/MacCracken/mabda/compare/2.4.5...2.5.0
[2.4.5]: https://github.com/MacCracken/mabda/compare/2.4.4...2.4.5
[2.4.4]: https://github.com/MacCracken/mabda/compare/2.4.3...2.4.4
[2.4.3]: https://github.com/MacCracken/mabda/compare/2.4.2...2.4.3
[2.4.2]: https://github.com/MacCracken/mabda/compare/2.4.1...2.4.2
[2.4.1]: https://github.com/MacCracken/mabda/compare/2.4.0...2.4.1
[2.4.0]: https://github.com/MacCracken/mabda/compare/2.3.0...2.4.0
[2.3.0]: https://github.com/MacCracken/mabda/compare/2.2.0...2.3.0
[2.2.0]: https://github.com/MacCracken/mabda/compare/2.1.2...2.2.0
[2.1.2]: https://github.com/MacCracken/mabda/compare/2.1.1...2.1.2
[2.1.1]: https://github.com/MacCracken/mabda/compare/2.1.0...2.1.1
[2.1.0]: https://github.com/MacCracken/mabda/compare/2.0.0...2.1.0
[2.0.0]: https://github.com/MacCracken/mabda/compare/1.0.0...2.0.0
[1.0.0]: https://github.com/MacCracken/mabda/compare/0.1.0...1.0.0
[0.1.0]: https://github.com/MacCracken/mabda/releases/tag/0.1.0
