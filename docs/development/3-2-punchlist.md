# Mabda v3.2 — Release Punch List

**Status:** Planned — `v3.2` branch cut 2026-06-15. No implementation
started; this doc + the seven design proposals are the plan. Tick items as
they land.
**Date opened:** 2026-06-15
**Branch:** `v3.2` (cut from `main` after 3.1.1).
**Roadmap reference:** [`roadmap.md` § v3.2](roadmap.md#v32--texture--shader-breadth)
**Design proposals:**
[`v3.2-compressed-textures.md`](../proposals/v3.2-compressed-textures.md) ·
[`v3.2-transfer-copy.md`](../proposals/v3.2-transfer-copy.md) ·
[`v3.2-native-compressed-sampling.md`](../proposals/v3.2-native-compressed-sampling.md) ·
[`v3.2-spirv-ingestion-wgpu.md`](../proposals/v3.2-spirv-ingestion-wgpu.md) ·
[`v3.2-spirv-gfx9-native-lowering.md`](../proposals/v3.2-spirv-gfx9-native-lowering.md) ·
[`v3.2-f64-compute.md`](../proposals/v3.2-f64-compute.md) ·
[`v3.2-render-graph-multiqueue.md`](../proposals/v3.2-render-graph-multiqueue.md)

> **v3.2 — "Texture & Shader Breadth" + finish-what-we-started.** The arc
> delivers the roadmap's texture/shader breadth (compressed textures →
> SPIR-V → f64, the maintainer's fixed sequence; SPIR-V/f64 on **both
> backends**) **and absorbs every item that was previously going to be
> deferred** — native compressed *sampling*, the v3.1.2 TRANSFER/buffer-copy
> work, and render-graph multi-queue. **Nothing in this plan defers to
> v3.3 or v4.** The arc grows to fit (3.2.0 → 3.2.13). Where work is
> blocked *only* by hardware absent from the dev box, the code still ships
> in-arc and the **HW-verification gap is flagged for the maintainer**
> (see the Hardware-verification gaps section) — never silently dropped.

## Version map (grown to absorb everything; back-half minor numbers nominal/fluid, order + "all-in-3.2.x" invariant fixed)

| Minor   | Phase | Lands | `BACKEND_SIZE` |
|---------|-------|-------|----------------|
| 3.2.0   | **T**  | Compressed textures — formats/validation/upload + capability gating. wgpu full sample; native storage/readback (sampling → Phase TS, same arc). | 248→256 |
| 3.2.1   | **X**  | Real native `gpu_buffer_*` + TRANSFER→DMA ring + public `gpu_buffer_copy` / `gpu_queue_transfer_copy` (both backends). [absorbs v3.1.2 carryover] | 256→264 |
| 3.2.2   | **TS** (1) | Native compressed *sampling* foundation: GFX9 T#/S# builders, sampleable slots, textured FS, **uncompressed RGBA8 sampling MVP** on HW (TS.1–5). | 264→280 |
| 3.2.3   | **TS** (2) | Tile-swizzle + **BC compressed sampling** + bilinear/ETC2/ASTC (TS.6–8). Native compressed sampling **complete**; strikes Phase T's storage-only limit. | — |
| 3.2.4   | **S**  | SPIR-V ingestion (wgpu) + kind-tagged shader-module path (S.1–6). | (slot widens) |
| 3.2.5   | **N** (1) | Native SPIR-V→GFX9 compiler: encoder-lift + byte-oracle, SPIR-V parser (N.0–1). | 256 (no slot) |
| 3.2.6   | **N** (2) | MIR + uniformity lowering, instruction selection f32/i32 (N.2–3). | — |
| 3.2.7   | **N** (3) | Register allocation + waitcnt, encode + **downsample byte-oracle MVP** (N.4–5). | — |
| 3.2.8   | **N** (4) | Control flow + op breadth (N.7–8): uniform/divergent `if`, GLSL math, signed compares, VOP3-literal/inline/store/const-fold constants. Scalar SPIR-V **f32/i32 compiler complete**. | — |
| 3.2.9   | **N** (5) | Int div/mod (N.9): no HW integer divide → float-reciprocal macro. OpUDiv/UMod/SDiv/SRem/SMod, all HW-verified on Cezanne. | — |
| 3.2.10  | **N** (6) | Vectors (N.10): component-wise vec2/3/4 ops. | — |
| 3.2.11  | **N** (7) | Per-ext-set tracking (N.11): verify `OpExtInst` set is GLSL.std.450. | — |
| 3.2.12  | **F** (1) | f64 caps surface + native hand-authored f64 proof + wgpu f64 module (F.1–6, F.8). | — |
| 3.2.13  | **F** (2) | General native f64 via the Phase N emitter + attn11 acceptance (F.7, F.9–10). f64 **complete**. | — |
| 3.2.14  | **R** (1) | Render-graph multi-queue: node queue affinity, cross-queue edges→barriers, native render timeline dispatch (R.1–4). | — |
| 3.2.15  | **R** (2) | Per-node IB staging, scheduler, e2e (R.5–7). Render-graph multi-queue **complete**. | — |

`BACKEND_SIZE` / slot offsets are append-after-KIND in **landing order**;
the absolute `+N` in each proposal is nominal and finalized per the order
above (T's slot, then X's, then TS's two). Each bite ends green:
`cyrius build` (0 warn) · per-file `cyrius lint` / `cyrius fmt --check`
(128 KiB cap) · `cyrius vet` · `cyrius distlib` diff-clean ·
`./scripts/version-check.sh`. Every new code path adds a CPU assertion; HW
behaviour lives in `programs/*_e2e.cyr` (Cezanne / a wgpu-native box).
Bites adding FFI-boundary bytes — and **every Phase N compiler bite** —
re-run the P(-1) security-audit checklist against the diff.

---

## Phase T — Compressed textures (storage + upload) → 3.2.0

Proposal: [`v3.2-compressed-textures.md`](../proposals/v3.2-compressed-textures.md).
Formats + validation + upload + capability gating. wgpu: full
create/upload/bind/sample. native: storage + readback this minor;
**sampling lands in Phase TS (3.2.2–3.2.3), same arc** — not deferred.

- [ ] **T.1** — Format table + ids + accessors (`MABDA_TEXFMT_*` + block
  geometry, `src/texture_format.cyr`). Pure Cyrius.
- [ ] **T.2** — `WGPUTextureFormat` v29 mapping (BC1=0x32 … ASTC8x8=0x58),
  each pinned vs `webgpu.h`; `mabda_texfmt_to_wgpu`.
- [ ] **T.3** — Block size math (`ceil(w/bw)*ceil(h/bh)*blockBytes`) +
  block-aware dimension/overflow guards.
- [ ] **T.4** — Capability gating (`GpuCapabilities` +120 compression
  bitset; `GPU_ERR_FORMAT_UNSUPPORTED`; wgpu from `adapter_has_feature`).
- [ ] **T.5** — Create slot + dispatcher (`BACKEND_SLOT_TEXTURE_CREATE_2D_FMT`,
  `BACKEND_SIZE`→256; `gpu_texture_create_2d(ctx,w,h,fmt)`).
- [ ] **T.6** — wgpu create + block-upload + sample wiring; install +
  extend `backend_is_complete`.
- [ ] **T.7** — native create + storage path (block-sized linear BO,
  memcpy write/read); sample attempt fails loud until Phase TS lands it.
- [ ] **T.8** — e2e: `compressed_texture_e2e.cyr` (wgpu sample-verify),
  `native_compressed_store_e2e.cyr` (Cezanne round-trip).
- [ ] **T.9** — **Cut 3.2.0** (VERSION/CHANGELOG/roadmap/CLAUDE/dist/gate;
  P(-1) audit over texture-size/writeTexture/enum deltas).

---

## Phase X — TRANSFER→DMA ring + public buffer-copy (both backends) → 3.2.1

Proposal: [`v3.2-transfer-copy.md`](../proposals/v3.2-transfer-copy.md).
Absorbs the v3.1.2 carryover. Also lands **real native `gpu_buffer_*`**
(create/write/read/release are stubs today). SDMA `COPY_LINEAR` already
HW-proven (`native_sdma_copy_e2e.cyr`).

- [x] **X.1** — `NativeBuf` struct + real `_backend_native_buffer_create`/
  `_release` (page-align, VA sub-alloc, GEM/VA map). Retires the stubs.
- [x] **X.2** — real native `buffer_write`/`_read` (memcpy + `off+n<=size`
  guard). Native `gpu_buffer_*` now real on both backends.
- [x] **X.3** — DMA-ring flip (`_native_queue_kind_to_ring(TRANSFER)`→
  `AMDGPU_HW_IP_DMA`) + compute-on-DMA-queue mis-route guard +
  `GPU_ERR_TRANSFER=20` (19 is `FORMAT_UNSUPPORTED`).
- [x] **X.4** — `native_transfer_copy_timeline` driver (SDMA into cached
  IB; BO_HANDLES fence/ib/src/dst; consume pending-wait → in-CS
  TIMELINE_WAIT; signal timeline).
- [x] **X.5** — `BACKEND_SLOT_BUFFER_COPY` (`BACKEND_SIZE`→264) + public
  `gpu_buffer_copy(ctx,src,dst,size)` + `gpu_queue_transfer_copy(...)`;
  completeness walk (8th range, activated at X.6).
- [x] **X.6** — wgpu fill (`copy_buffer_to_buffer` + submit + poll);
  install both backends; activate completeness walk. **+ audit fixes
  (2026-06-15 review):** 4-byte-multiple size guard in both dispatchers
  (HIGH-1), native SDMA >4 MiB reject via `_NATIVE_SDMA_COPY_MAX_BYTES`
  (HIGH-2), `_backend_wgpu_buffer_create` OR's in COPY_SRC|COPY_DST so
  any mabda buffer is a copy operand, matching native (MED-3).
- [x] **(interrupt)** — toolchain pin `6.2.10`→`6.2.11` (latest); was the
  X.9 fold-in, done early.
- [x] **X.7** — HW e2e. `native_transfer_copy_e2e.cyr` (HW-verified on
  Cezanne): leg A public `gpu_buffer_copy` 4096B round-trip + alignment-guard
  reject; leg B compute-produce → barrier → public `gpu_queue_transfer_copy`
  consume (consumed == [0xDEADBEEF, pattern...]). `wgpu_transfer_copy_e2e.cyr`
  serialized verify (compile-checked; HW-gated). Makefile targets
  `test-native-transfer-copy-e2e` / `test-wgpu-transfer-copy-e2e`. **Finding:**
  producer/consumer can't OVERLAP through the single per-context cached IB
  (the consumer's packet clobbers the producer's) — the e2e serializes with
  `wait_idle`, matching `native_multiqueue_e2e`. True overlap needs per-IB
  staging → **reinforces R.5 (3.2.13)**. No regression in the other native
  HW programs (compute_store / multiqueue / sdma_copy / queue_compute all
  still exit 0).
- [x] **X.8** — Native SDMA **chunking**. `native_sdma_build_copy_chained`
  loops `COPY_LINEAR` packets (each ≤4 MiB, per-chunk src/dst VA offsets) into
  the cached IB; `native_transfer_copy_timeline` no longer rejects at 4 MiB —
  it chains, and only rejects (`GPU_ERR_BUFFER`) a chain that overflows the IB
  (~146 packets / ~584 MiB per submission; >that = multi-submission, future).
  CPU test for the chunk/VA-offset/overflow math; HW e2e **leg C** (6 MiB =
  2 packets, byte-exact across the boundary, verified on Cezanne). Closes the
  >4 MiB native gap for all realistic buffers.
- [x] **X.9** — **Cut 3.2.1.** VERSION 3.2.0→3.2.1; CHANGELOG `[3.2.1]`;
  CLAUDE.md / README version + state synced (2530 asserts); audit doc
  `docs/audit/2026-06-15-buffer-copy-audit.md`; `version-check.sh` OK; dist
  regenerated (embeds 3.2.1) + idempotent. Phase X **complete**.

---

## Phase TS — Native compressed (and general) texture sampling → 3.2.2–3.2.3

Proposal: [`v3.2-native-compressed-sampling.md`](../proposals/v3.2-native-compressed-sampling.md).
**Re-introduces the GFX9 T#/S#/tiling/`image_sample` path the v3.1 mipmap
pivot deleted** — so native AMD can *sample* textures, not just
store/readback. Two sub-features: uncompressed (linear + T#/S#) and
compressed (tiled + swizzle + T#/S#); the tiling swizzle is the
high-risk half, sequenced last. **This is the work I wrongly punted to
"v4-scale"; it is in the arc.**

> **Sequencing (2026-06-15, maintainer):** "3 then 1, you are on HW" — do
> the lower-risk S#/format-table pieces first, then the T#; descriptor bytes
> sourced from gfx9.json (via the GitLab API, see memory) + HW-verified at
> TS.5 on the Cezanne. Builders land as pure CPU-pinned byte-writers; the
> `gpu_sampler_create` dispatcher + wgpu filler + the two slots fold into TS.3.

- [x] **TS.1** — **done.** format→IMG-format table (`native_gfx9_texfmt_to_img_format`
  + `native_img_data_format`/`_num_format`) + the 256-bit T# builder
  (`native_gfx9_image_descriptor`): all 8 dwords + every field (BASE_ADDRESS
  hi/lo, DATA/NUM_FORMAT, WIDTH/HEIGHT, identity DST_SEL, LAST_LEVEL, SW_MODE,
  TYPE=2D, PITCH, MAX_MIP) pinned vs gfx9.json `SQ_IMG_RSRC_WORD0..6` +
  `SQ_SEL_XYZW01`/`SQ_RSRC_IMG_TYPE`. Bit positions authoritative; value
  semantics (linear PITCH alignment, MIN_LOD) HW-cross-checked at TS.5.
- [x] **TS.2** — GFX9 S# sampler-descriptor builder
  (`native_gfx9_sampler_descriptor`, pinned vs gfx9.json) + `NATIVE_SAMP_*`
  intents + **`gpu_sampler_create`** (backend-agnostic packed intent handle;
  the WGPUSampler / S# materialize at bind time, TS.5 — so no per-sampler
  slot, keeping the 2-slot plan). wgpu `WGPUSampler` build folds into the
  TS.5 wgpu `bind_for_sample`.
- [~] **TS.3** — struct + slot scaffolding **done**: `NativeTexture` 48→64
  (sw_mode@48, t_va@52 + accessors); NativePass 32→40 (tex_desc_va@32); slots
  `CREATE_2D_SAMPLEABLE`@264 + `BIND_FOR_SAMPLE`@272, `BACKEND_SIZE` 264→280,
  `BACKEND_SAMPLE_SLOTS_*` range; public `gpu_texture_create_2d_sampleable` +
  `gpu_render_pass_bind_texture` dispatchers (mock-tested). **Remaining (→ TS.5):**
  the `backend_is_complete` walk over 264..280 activates once both backends
  fill the slots ("activate when both fill").
- [~] **TS.4** — textured FS **done**: `native_gfx9_shader_textured_load_fs`
  — the `image_load` bring-up oracle (each fragment loads the texel at its
  own screen position → RT[x,y]=tex[x,y]; zero interpolation, reuses the
  existing fullscreen VS). Assembled + round-tripped via `llvm-mc -mcpu=gfx90c`,
  byte-pinned + added to `scripts/disasm-shaders.sh`. SPI wiring constants
  (`GFX9_SPI_PS_INPUT_TEXTURED=0x302` = PERSP_CENTER|POS_X|POS_Y;
  `R_SPI_SHADER_USER_DATA_PS_0/1`). **Deferred to TS.7** (with the scaled
  compressed path): the UV-export VS + normalized `image_sample` FS — not
  needed for the RT-sized RGBA8 MVP, which the screen-pos oracle covers.
- [x] **TS.5** — **Uncompressed RGBA8 sampling MVP on Cezanne** (delivered in
  sub-bites). **TS.5a done:** native `_backend_native_texture_create_2d_sampleable`
  (linear surface BO + a descriptor BO carrying the T# at +0 and a default
  point/clamp S# at +32; descriptor BO bump-leaked per the create-once model)
  + `_backend_native_texture_bind_for_sample` (stashes the descriptor VA on
  the pass); both native slots installed + CPU-tested. **TS.5b done** (minimal
  wgpu + walk): `_backend_wgpu_texture_create_2d_sampleable` (real — the
  fmt-create already makes a TEXTURE_BINDING texture + view) +
  `_backend_wgpu_texture_bind_for_sample` (stashes tex/sampler on the wgpu
  pass, grown 32→40); both wgpu slots installed; **`backend_is_complete` walk
  over 264..280 ACTIVATED** (9th range, both backends complete). Sequencing
  chosen 2026-06-15: "3, 2, 1". **(2) wgpu sample path — DONE + HW-verified.**
  `wgpu_texture_bind_group_descriptor` (bind_group.cyr) +
  `wgpu_sampler_descriptor_from_intent` (sampler.cyr) pinned vs webgpu.h v29;
  added `wgpu_render_pipeline_get_bind_group_layout` FFI (fp65 + C-shim
  fn_table[65], FN_COUNT 65→66); `_backend_wgpu_render_pass_draw` gained a
  textured branch (build sampler+bind-group from the pipeline's auto-BGL +
  `set_bind_group`); `programs/wgpu_texture_sample_e2e.cyr` **passes on the
  Cezanne** (RT == sampled color via the public API). Adversarial review
  (`review-wgpu-sample-ts5`, 4 confirmed) → fixed: per-draw sampler/bind-group
  /BGL **handle leak** (release after set_bind_group; HIGH) + null-guards on
  the create chain (LOW). No regression (render_e2e still green).
  **(1) native HW sampling — DONE + HW-verified on Cezanne.** Single-BO
  sampleable create (surface + descriptor tail); `render_pass_draw` emits the
  textured override (`SPI_PS_INPUT_ENA=0x302`, `RSRC1_PS` VGPR bump, PS
  USER_DATA = descriptor VA) after the pipeline blocks (last-write) + adds the
  surface BO to a 6-entry residency list; `_NATIVE_PM4_SCRATCH_BYTES` 1024→2048
  (the override pushes the stream to ~261 dwords). `programs/native_texture_sample_e2e.cyr`
  **passes pixel-exact (RT[x,y]==tex[x,y])** — the only bug was the scratch
  overflow; the gfx9.json descriptors + llvm-mc shader + SPI/RSRC were correct
  first-try (no TDR iteration). Adversarial review (`review-native-sample-ts5`,
  6 confirmed) → fixed: sampleable release size mismatch (recompute bo_size
  from t_va; MED), release now clears t_va (use-after-bind; MED), per-draw
  bound_tex clear (LOW), reject compressed-sampleable until TS.7 (LOW), stale
  docstrings (NIT). No regression (native_render_e2e green).

  **TS.5 COMPLETE** — native + wgpu RGBA8 sampling both HW-verified. The
  T#/S#/image_load/descriptor/sampleable infra works on both backends.
- [x] **Cut 3.2.2** (2026-06-15) — TS.1–TS.5 content-complete (per the version
  map). VERSION 3.2.1→3.2.2; CHANGELOG `[3.2.2]`; audit
  `docs/audit/2026-06-15-ts-sampling-audit.md`; CLAUDE.md/README synced (2702
  asserts, struct/BACKEND_SIZE deltas); version-check OK; dist regen (embeds
  3.2.2) + idempotent; closeout green (suite 2702/0, 9 benches, 7 HW programs
  exit 0 on Cezanne incl. native+wgpu texture-sample). TS.6–8 → 3.2.3.
- [~] **TS.6** — Tile-swizzle for SW_64KB_S compressed surfaces.
  **Approach (maintainer, 2026-06-15): "try SDMA HW tiling first"** — the GFX9
  swizzle is config-dependent addrlib (no pattern table to transcribe), so
  instead of a pure-Cyrius swizzle port, use an **SDMA `COPY_TILED_SUB_WINDOW`**
  (sub-op 5) linear↔tiled copy and let the GPU apply the swizzle (write = L2T,
  read = T2L). **Done this bite:** `native_sdma_build_copy_tiled` — the 14-dword
  packet builder (transcribed from Mesa `ac_emit_sdma_copy_tiled_sub_window`,
  SDMA_4_0; `info_dword = element_size | swizzle_mode<<3 | dim<<9 | epitch<<16`),
  CPU-pinned. **SDMA-tiling mechanism PROVEN on Cezanne:**
  `programs/native_sdma_tiled_roundtrip.cyr` — a linear→tiled→linear round-trip
  (`ADDR_SW_64KB_S`=9, `RADEON_RESOURCE_2D`=1, 8-byte/BC1 element, 256×256)
  comes back byte-identical, so the `COPY_TILED_SUB_WINDOW` packet is accepted
  and the HW applies + reverses the swizzle. (Round-trip = self-consistency;
  ABSOLUTE TA-match is TS.7's sampling test.) **Remaining → TS.7:** the
  addrlib-exact tiled geometry (`epitch` + tiled BO size that the TA agrees
  with) — confirmed at TS.7 by sampling the SDMA-tiled BC surface vs a CPU
  decode.
- [x] **TS.7** — **BC1/BC7 compressed sampling on Cezanne** (delivered in
  sub-bites). **TS.7a done:** `native_gfx9_shader_textured_sample_fs` — the
  `image_sample` (BC-decoding) FS (llvm-mc + byte-pinned + disasm round-trip).
  **TS.7b done:** `native_tiled_geometry` — SW_64KB_S 2D block dims + aligned
  pitch (epitch) + 64 KiB-aligned BO size, sourced from addrlib
  (`Block256_2d`<<4 / `ComputeThinBlockDimension`; BC1 128×64, RGBA8 128×128,
  BC7 64×64 blocks), CPU-pinned. **TS.7c-1 done:** switched the sample FS + the
  default S# to **unnormalized** coords (S# `FORCE_UNNORMALIZED`); the FS now
  feeds the fragment position straight to `image_sample` (no rcp / UV divide),
  giving it the *same 2-user-SGPR ABI as the image_load FS* — so the TS.5 draw
  override is reused unchanged (no per-format draw path). FS is 112 B;
  `GFX9_PS_PGM_RSRC2_SAMPLE` removed. **TS.7c-2 done:** tiled
  `create_sampleable` foundation — `native_gfx9_image_descriptor` gained an
  explicit `epitch` param; `GFX9_SW_MODE_64KB_S`(9); `native_tex_tiled_params`
  (compressed fmt → block geometry); the create compressed branch builds a
  geometry-sized SW_64KB_S surface + BC T# (epitch = awb-1) + unnorm S#. A
  native backend caps gate (`NATIVE_TEXCOMP_SUPPORTED`, =0) keeps compressed
  `create` returning 0 (pre-ioctl) until write + HW verify land — no dead-end
  resource; mirrors the wgpu backend caps guard. write/read reject a tiled tex
  (`GPU_ERR_TEXTURE`) until TS.7c-3. **Remaining:**
  - **TS.7c-3 done:** wired write(L2T)/read(T2L) via `native_tex_build_tiled_copy_packet`
    + a transient staging BO + `_native_dma_submit_oneshot` on the DMA ring. epitch
    is derived from `native_tex_tiled_params` (awb-1) — the SAME source as the T# —
    so the SDMA layout and the T# can't drift. HW-verified by
    `native_tiled_texture_roundtrip` (BC1 256×256 = 64 blocks, awb=128 → epitch=127,
    a NON-block_w-aligned surface the TS.6 256-block probe couldn't catch).
  - **TS.7c-4 done:** `native_compressed_sample_e2e.cyr` samples SW_64KB_S tiled
    surfaces in the image_sample FS and matches a CPU decode **pixel-exact on
    Cezanne** for **BC1 (RGB565 endpoints + within-block checker), BC4 (1-channel
    → R,0,0,1), and BC5 (2-channel → R,G,0,1)** — spanning the channel-mapping
    spectrum. The tiled round-trip also HW-verifies **BC7 (16-byte block)** tiling
    alongside BC1 (8-byte). Flipped `NATIVE_TEXCOMP_SUPPORTED` → `MABDA_TEXCOMP_BC`.
    **BC compressed sampling is live on native AMD.**
    - **Review fixes (2026-06-16):**
      - **BC6H rejected at create** — it is HDR unsigned-float (T# NUM_FORMAT=FLOAT)
        but the native sample path only has an RGBA8_UNORM render target, which
        clamps/quantizes HDR values → a silently-wrong decode. Gated off until an
        HDR-capable RT path exists (the BC family bit otherwise admits it).
      - **Per-component DST_SEL in the T#** — identity X/Y/Z/W broadcasts a 1-/2-
        channel format's present channel(s) across RGBA (HW-confirmed: BC4 sampled
        (R,R,R,R)). BC4 → X,0,0,1, BC5 → X,Y,0,1; 4-channel formats stay identity.
        Caught by adding the BC4/BC5 sample probes — a real wrong-color bug fixed
        before it shipped.
    - *Sample coverage:* **BC1/BC3/BC4/BC5/BC7 are ALL pixel-exact-verified on HW**
      (TS.8). BC7 uses a hand-encoded mode-6 block (single-partition RGBA, 7-bit
      endpoints + P-bit, all-index-0 → endpoint0 exact). Every sampleable BC
      format is now verified — no family-bet residual. BC6H is gated off (HDR
      float vs RGBA8 RT).
    - *Residual:* the native caps ADVISORY (`gpu_caps_supports_format`) still
      reports BC unsupported because native never builds a caps struct at all —
      wiring native caps is TS.8's "strike Phase T's storage-only limitation."
- [~] **TS.8** — Bilinear **DONE** (TS.8a scale plumbing + TS.8b observable e2e,
  HW-verified, see below); ETC2/ASTC **RESOLVED — HW-blocked on AMD** (vulkaninfo:
  ETC2/ASTC=false, BC=true; cap correctly NOT flipped, see below); native caps
  advertisement **done**; **strike Phase T's storage-only limitation** (BC done;
  full from_context caps builder remaining); BC6H needs an HDR RT path. Remaining:
  from_context caps builder, BC6H, then cut the TS minors. Details:
  - **Bilinear TS.8a done (scale plumbing @ scale=1.0, regression-only)** — both
    textured FS builders (image_load + image_sample) now `s_load_dwordx2` a per-draw
    scale (f32) from descriptor+48/+52 and `v_mul_f32` the fragment position before
    sampling (llvm-mc-verified, byte-pinned). The draw writes the scale into the
    descriptor tail unconditionally (TS.8a: literal 1.0); create defaults the tail
    to 1.0. No ABI/draw-override change (s[12:13] fits MIN|1). HW-verified: BC1/3/4/
    5/7 + RGBA8 sample e2es stay **pixel-exact** (scale 1.0 → coord==fragment).
    `native_tex_desc_cpu_addr` helper added. **TS.8b done (wiring):** (1) the
    float shim `int_ratio_to_f32(num,den)` (inline SSE2, CPU-tested) since Cyrius
    has no native float arithmetic — filed a toolchain proposal
    (`cyrius/docs/development/proposals/2026-06-16-native-float-arithmetic.md`) to
    replace it; (2) the draw now computes the REAL scale = `int_ratio_to_f32(tex_dim,
    rt_dim)` (equal dims → exactly 1.0, so the existing e2es stay pixel-exact —
    HW-re-verified); (3) `bind_for_sample` rebuilds the S# from the bound sampler
    (BILINEAR/POINT + CLAMP/WRAP, unnorm kept) — POINT/CLAMP rebuilds byte-identical
    to the create default (regression-safe, CPU-asserted). **TS.8b done
    (observable):** `native_bilinear_sample_e2e` samples a 32×32 gradient texture
    (R=x*8) over a 64×64 RT (scale 0.5 → fractional coords) with POINT then
    BILINEAR. Convention-independent discriminator on RT row 32 — **POINT = 0
    intermediates** (every R a multiple of 8, exact texels), **BILINEAR = 62
    intermediates** (blends adjacent columns), edges clamp for both. **HW-verified
    on Cezanne — native bilinear filtering is live.** EXACT interior weights + edge
    fidelity for arbitrary unnormalized scaling are a documented precision
    limitation; the edge-faithful normalized-UV path (UV-export VS + rcp FS) is the
    follow-on — a convention refinement, not missing functionality.
    **Bilinear/scaled sampling (TS.8) is complete.**
  - **Native caps advertisement (done)** — added `gpu_caps_native_texture_compression()`
    (native sibling of wgpu's `gpu_caps_detect_texture_compression(adapter)`), so a
    consumer populates a native context's caps the same way:
    `gpu_caps_set_texture_compression(caps, gpu_caps_native_texture_compression())`
    then `gpu_caps_supports_format` — closing the storage-only gap (review
    2026-06-16) to wgpu parity. The per-format truth (incl. the BC6H HDR exclusion
    the family bitset can't express) lives in `native_texfmt_sampleable(fmt)`, the
    single source of truth the create gate also uses. A full backend-abstracted
    `from_context` caps builder (texture limits etc.) is the remaining storage-only
    cleanup.
  - **BC6H sample** once an HDR RT path lands (BC1/BC3/BC4/BC5/BC7 are all
    HW-sample-verified; BC6H is the only BC format still gated, on HDR grounds).
  - **ETC2/ASTC — CONFIRMED HW-blocked on AMD; cap stays OFF (final).** A TS.8 HW
    probe sampled an ETC2_RGB8 block via the proven image_sample path and got
    uniform black, byte-order-insensitive (TA not decoding). Disambiguated against
    the same AMD HW through Vulkan: **`vulkaninfo` reports `textureCompressionBC =
    true`, `textureCompressionETC2 = false`, `textureCompressionASTC_LDR = false`**.
    So AMD (Cezanne/gfx90c, and AMD desktop/APU generally) does NOT decode
    ETC2/ASTC — the gfx9.json IMG_DATA_FORMAT enum has the values (24/25/46) but
    the silicon/driver doesn't implement the decode. This was the right call to
    flag, not a mabda bug. `NATIVE_TEXCOMP_SUPPORTED = MABDA_TEXCOMP_BC` is correct
    and final for the AMD backend. The non-working probe was removed; the generic
    per-format DST_SEL groundwork (ETC2_RGB 3-channel → W=SEL_1) + format-table
    rows + CPU tests stay (correct descriptor-builder behavior, ready for any
    future arch that does decode ETC2 — e.g. a v4 NVIDIA/Intel native backend).

---

## Phase S — SPIR-V shader ingestion (wgpu) → 3.2.4

Proposal: [`v3.2-spirv-ingestion-wgpu.md`](../proposals/v3.2-spirv-ingestion-wgpu.md).
Adds an explicit shader **source-kind tag** (WGSL/SPIR-V/GFX9-ISA) to the
byte-polymorphic boundary. Native SPIR-V is Phase N (fail-loud here).

- [x] **S.1** — done. `WGPU_STYPE_SHADER_SOURCE_SPIRV=0x1` (wgpu_types) +
  `wgpu_shader_source_spirv(words_ptr, word_count)` (32 B; codeSize in **words**
  @+16, code ptr @+24 — webgpu.h v29-pinned) + `_spirv_validate(ptr, byte_len)`
  (magic/align/bound + 16 Mi-word cap; 6 distinct codes incl. byte-swapped-magic
  detection) — all in `wgpu_descriptors.cyr`, CPU-tested (backend.tcyr).
- [x] **S.2** — done. `ShaderSourceKind` enum (WGSL/SPIRV/GFX9, backend.cyr);
  shader-create slot widened `(ctx,bytes,n,kind)` (fncall3→4, no offset change);
  wgpu filler branches WGSL/SPIRV (validate + `wgpu_shader_source_spirv`, codeSize
  n/4); native stub takes kind (SPIRV→0 fail-loud, Phase N). `gpu_shader_module_create`
  forwards the bound backend's default (WGSL/GFX9 — no call-site churn) +
  `gpu_shader_module_create_spirv` forwards SPIRV. Dispatch tests assert all three
  kinds route. (Also fixed a pre-existing awb-1 comment fmt drift v3.2 had inherited.)
- [x] **S.3** — done. `_shader_hash_n(ptr, byte_len, kind)` — length-explicit
  FNV-1a (SPIR-V has embedded NULs) with the kind folded into the seed; WGSL
  (kind 0) is the identity fold so legacy keys are byte-stable, while WGSL/SPIRV/
  GFX9 namespaces never collide. `_shader_hash` delegates to it. SPIR-V peers
  `shader_cache_get_spirv` / `_set_spirv` / `_get_or_compile_spirv` (validates +
  builds the SPIRV source). CPU-tested (caches.tcyr: kind separation, embedded-NUL,
  round-trip).
- [x] **S.4** — done. `deps/wgpu_main.c` instance descriptor now requests
  `WGPUInstanceFeatureName_ShaderSourceSPIRV` (requiredFeatureCount=1 +
  requiredFeatures); without it every SPIR-V createShaderModule nulls. Verified
  with `cc -fsyntax-only -I deps/wgpu-native/include` (const-correct; the only
  warning is the pre-existing `nextInChain` cast). **Consumer migration note**
  (→ goes in the 3.2.4 CHANGELOG Breaking/Migration at S.6): consumers that copy
  the launcher must carry this 4-line edit to use SPIR-V shaders; WGSL is
  unaffected. mabda can't detect a missing-feature instance before the create —
  the fail-loud 0 module is the contract.
- [x] **S.5** — done. `programs/spirv_e2e.cyr` (+ `make test-spirv-e2e`): a
  fullscreen-triangle+red SPIR-V module (glslang `-e vs_main`/`-e fs_main` +
  spirv-link, spirv-val-clean, 339 words embedded) created via
  `gpu_shader_module_create_spirv`, rendered through `render_pipeline_create_simple`,
  read back, and **byte-identical to the WGSL twin** (cross-source identity).
  **HW-verified on the wgpu-native box** — exercises S.2 (slot+entry) + S.4
  (instance feature) end-to-end. (Fullscreen triangle → whole RT red, so the
  identity holds regardless of the WGSL-vs-Vulkan Y convention.)
- [x] **S.6** — **Cut 3.2.4** (2026-06-16). VERSION/cyrius.cyml/CHANGELOG/README/
  CLAUDE.md → 3.2.4; toolchain pinned `6.2.14`; Phase S audit filed
  (`docs/audit/2026-06-16-phaseS-audit.md`, 0 confirmed / 1 LOW dismissed); dist
  regenerated idempotent (`b894948`); `version-check.sh` consistent. Full gate
  green: smoke build OK, **2908/0** across 12 tcyr suites, 0 lint warnings, 0 fmt
  drift (file-first `cyrius fmt <f> --check`).

---

## Phase N — Native SPIR-V → GFX9 ISA compiler → 3.2.5–3.2.9

Proposal: [`v3.2-spirv-gfx9-native-lowering.md`](../proposals/v3.2-spirv-gfx9-native-lowering.md).
**The dominant, highest-risk effort** — an in-tree, compute-only,
GFX9/Cezanne-only SPIR-V→GFX9 compiler behind the existing shader slot (no
new slot). Own AGNOS package. A compiler emits bytes no human checked for
inputs no human saw — the entire mitigation is *manufactured oracles*. The
hand-authored shaders stay as oracle + fallback.

- [x] **N.0** *(3.2.5, 2026-06-16)* — Oracle harness + encoder lift
  (`src/gfx9_encode.cyr`). Operand-parameterized encoders for every
  compute-format the compiler emits — VOP1, VOP2, VOP3a, SOP2, SOPP, SMEM,
  FLAT(global) — plus VOP src operand helpers (`gfx9_vgpr`/`gfx9_sgpr`/
  `gfx9_inline_int`). **Oracle GREEN:** `tests/tcyr/compiler.tcyr` (118 asserts)
  re-encodes every compute-format hand-authored dword in
  backend_native_shaders.cyr byte-for-byte (deadbeef + the full downsample
  SOP2/VOP1/VOP2/VOP3a stream + the textured-FS SMEM/v_mul/v_cvt), and a
  capstone rebuilds store_deadbeef wholly through the encoders + memcmps the
  HW-verified builder's bytes. `scripts/disasm-shaders.sh` gains a per-form
  llvm-mc round-trip section: each format decodes to its expected gfx90c
  mnemonic. The hand-authored shader builders are UNTOUCHED (still the ground
  truth). EXP (graphics export) + MIMG (image ops) encoders are NOT lifted —
  the proposal scopes the compiler compute-only; they land with a future
  graphics/texture compiler phase (flagged here, not silently dropped). Pure
  CPU. Wired into `[lib].modules` + `src/lib.cyr` (before shaders, after amdgpu).
  **Follow-on refactor (same bite):** all six hand-authored builders in
  `backend_native_shaders.cyr` re-expressed to emit through the encoders
  (`gfx9_emit32` + `gfx9_enc_*`) + a shared `gfx9_emit_prefetch_pad` for the
  16×`s_nop` tail — the proposal's "round-trip the fixed shaders through the new
  encoders" check; **byte-identical** (native.tcyr golden/checksum tests
  unchanged-green). EXP/MIMG dwords stay raw (compute-only scope).
- [x] **N.1** *(3.2.5, 2026-06-16)* — SPIR-V parser (`src/spirv_parse.cyr`).
  **N.1a:** word/header accessors + `(opcode, wordcount)` decode + the
  whole-stream VALIDATOR (the untrusted-input rejection gate — bad-magic /
  short / unaligned / byte-swapped / id-bound via `_spirv_validate`, plus
  zero-wordcount and truncated-instruction) + instruction count + GLCompute
  entry-point / `LocalSize` probes (24 asserts incl. 5 rejection cases).
  **N.1b:** the type / constant / decoration LOOKUP TABLES the SSA model N.2
  lowers from — caller-provided buffers of `id_bound` fixed-size records indexed
  directly by `<id>` (no hashmap, no alloc → tests stay stack-based). Type table
  (void/bool/int/float/vector/array/runtime-array/struct/pointer/function +
  per-op `wc` guards), scalar constant table, and Binding/DescriptorSet
  decoration table (27 asserts on a typed-module fixture). Pure CPU.
  **3.2.5 content (N.0 + N.1) is COMPLETE** — ready to cut on the maintainer's
  word. Next phase: N.2 (MIR + uniformity lowering) at 3.2.6.
- [~] **N.2** *(3.2.6)* — MIR + uniformity lowering. Design via a 3-lens panel +
  synthesis (workflow). **N.2a done (2026-06-16):** `src/mir.cyr` — the SSA IR
  data model (a MirMod header over four caller-provided, `<id>`-indexed buffers:
  values / instructions / access side-table / blocks; operands stored as SPIR-V
  `<id>`s for 1:1 SSA), builders + accessors, type distillation (`_mir_lower_type`:
  i32/u32/f32/vec2-4, non-32-bit → UNSUPPORTED), and the **GFX9 uniformity pass**
  (`_mir_meet` + seed + single forward sweep — UNIFORM→SGPR/SALU vs
  DIVERGENT→VGPR/VALU). 87 asserts incl. the `_mir_meet` truth table + a
  SAXPY-shape hand-built MIR proving every value's class + immediate-operand skip
  + the cap/id-OOR error paths. Pure CPU, no SPIR-V walk (testable in isolation).
  **Adversarial review (workflow): 5 confirmed findings, all fixed pre-merge** —
  synth-id headroom/OOB (split `cap_ids` from `id_bound`), `mir_set_*` + 
  `mir_add_ptr` bounds/sentinel guards, and `_mir_lower_type` unbounded recursion
  + vector-of-vector corruption (require scalar vector component); regression
  asserts added.
  **N.2b-1 done (2026-06-16):** `src/spirv_lower.cyr` — the SPIR-V-module→MIR walk
  for the NON-MEMORY subset + the BuiltIn-resolution gap-closing pass the N.1b
  parser does not provide (`_spirv_resolve_builtins`). `spirv_lower_module`
  validates the entry point, seeds globals (constants + StorageBuffer/Uniform/
  PushConstant vars → BUFVAR, Input+BuiltIn vars → BUILTIN), walks the entry
  function body dispatching ALU (IADD/ISUB/IMUL/SHL/SHR/AND/OR/XOR/FADD/FSUB/FMUL),
  conversions, OpCompositeExtract, the builtin OpLoad alias, and OpReturn, then
  runs the uniformity sweep. Control flow → `LOWER_ERR_CONTROL_FLOW`, unmapped →
  `MIR_ERR_UNSUPPORTED_OP`, no entry → `LOWER_ERR_NO_ENTRY` (all fail loud). 33
  asserts: a GlobalInvocationId-arithmetic kernel lowered end-to-end (builtin
  resolve + load-alias + extract + ALU + uniformity), the StorageBuffer seed
  path, and the 3 fail-loud negatives.
  **Adversarial review (workflow): 9 confirmed findings, all fixed pre-merge** —
  the untrusted-id OOB class (a controlled-offset OOB *write* via the
  `OpDecorate BuiltIn` target + wild OOB *reads* from operand / result-type /
  load-source / seed-var ids: `spirv_validate_stream` never bounds in-instruction
  ids, and the lowering indexed tables with them) and a silent empty-body
  success; all now bound-checked / fail-loud, with 5 crafted-module regression
  asserts.
  **N.2b-2 done (2026-06-16):** the buffer memory path. `_spirv_lower_access_chain`
  (MVP shape pointer→struct{runtimearray<scalar>}, 2 indices) records a
  `(binding, byte-offset)` access in the ptr side-table — a constant array index
  folds to `const_off` (overflow-guarded `civ > 0x7FFFFFFF/stride`), a dynamic
  index emits a synth `IMUL(index, stride)`; `OpLoad`/`OpStore` of the pointer
  emit `MIR_OP_LOAD`/`STORE`. 40 asserts: the **full SAXPY** (`y[gid.x] =
  a*x[gid.x] + y[gid.x]`) lowered end-to-end — 9-instr stream, the 2 ptr records
  + bindings, and every value's uniformity (GID-indexed loads + arithmetic all
  divergent) — plus access-chain variants (const-index fold, offset overflow,
  bad member). **Adversarial review (workflow): 2 confirmed findings, both fixed
  pre-merge** — vec3/non-scalar element stride miscomputation (now requires a
  scalar element, fail-loud) and a constant array index not validated as an
  integer (now requires i32/u32, fail-loud); regression asserts added. The
  binding-range and dynamic-offset-range checks remain flagged N.6/audit gates.
  **N.2 is COMPLETE.**
  **Next:** N.3 (instruction selection, `src/gfx9_isel.cyr`) for 3.2.6.
- [~] **N.3** *(3.2.6, 2026-06-16)* — Instruction selection (`src/gfx9_isel.cyr`).
  `gfx9_isel` walks the MIR and selects one abstract GFX9 op (`GISEL_*`) per
  instruction over VIRTUAL registers (= MIR SSA ids; N.4 assigns physical regs).
  The load-bearing SALU-vs-VALU choice falls out of the N.2 uniformity: integer
  ops pick `S_*` (uniform) / `V_*` (divergent), float ops are always VALU;
  load/store carry the ptr-table index + binding, RET → `S_ENDPGM`. No
  class-coercion copies (the straight-line uniformity guarantees compatible
  operand classes; constant-bus cases are N.8). 41 asserts: the SALU/VALU
  op-selection table, the **SAXPY** and **gid** kernels selected end-to-end
  (proving uniform `1<<2` → `S_LSHL_B32` vs divergent `i*4` → `V_MUL_LO_U32`,
  the f32 ALU → VALU, load/store binding flow), + cap/unsupported negatives.
  **Adversarial review (workflow): 2 confirmed findings, both fixed pre-merge** —
  the integer path silently picked SALU for a result-less op (vals[0] sentinel)
  and for an UNKNOWN-uniformity result (sweep not run); both now fail loud,
  regression-tested. **Next:** N.4 (register allocation + `s_waitcnt`) for 3.2.7.
- [x] **N.4** *(3.2.7)* — Register allocation + `s_waitcnt`. **N.4a done
  (2026-06-17):** `src/gfx9_regalloc.cyr` — linear-scan allocation of the N.3
  virtual regs (MIR SSA ids) to physical VGPR/SGPR. Two independent files keyed
  by N.2 uniformity (UNIFORM→SGPR, DIVERGENT→VGPR); per-register "free-at" reuse
  (a freed VGPR is reclaimed by a later divergent value — the reuse the
  downsample byte-match needs); ABI-reserved registers skipped via caller
  `sgpr_base`/`vgpr_base` (USER_DATA+TGID, v0=LID); NO SPILL → fail loud over the
  file cap; VGPR/SGPR high-water marks for RSRC1 (N.6). Only SSA results are
  allocated (constants inline, builtins/buffers ABI/binding). 18 asserts: the
  **gid** kernel allocated exactly (incl. `%14` reclaiming `%12`'s freed `v1`,
  the uniform `1<<2` → `s8`), the **SAXPY** (7 divergent values reuse into v1-v4,
  high-water 5, no SGPR use), and the VGPR-overflow / cap-too-large fail-louds.
  **Adversarial review (workflow): 1 confirmed HIGH, fixed** — a float/CVT op
  (always VALU) with a uniform result was misfiled into an SGPR; the file now
  follows the selected op class (`gisel_writes_vgpr`), not uniformity (a uniform
  value in a VGPR feeding a later SALU op is the N.8 operand-coercion case).
  **N.4b done (2026-06-17):** `src/gfx9_waitcnt.cyr` — splices `s_waitcnt` so no
  instruction reads a still-outstanding memory result (async loads = garbage if
  read early; the compiler's top-tier correctness risk). Use-before-wait is
  **impossible by construction**: `vmcnt(0)` (0x0F70) before the first consumer of
  an outstanding load, `vmcnt(0) lgkmcnt(0)` (0x0070) before `s_endpgm` if any
  memory op ran — the hand-authored downsample pattern (for the N.5 byte-match).
  MVP = conservative (a use waits for ALL outstanding loads; per-load count is
  N.8). New `GISEL_S_WAITCNT`; runs on the isel list independently of regalloc.
  17 asserts incl. a `_wc_check_invariant` walker that proves no output instr
  reads an un-waited load. **Adversarial review (workflow): clean (1 candidate,
  0 confirmed).** **N.4 COMPLETE.**
- [~] **N.5** *(3.2.7)* — **MVP: encode + ABI wiring + the downsample bring-up
  oracle.** Scoped 2026-06-17 (user, [[project_n5_saxpy_first_proven_equiv]]):
  the downsample needs 5 isel/ABI features the gid/SAXPY-built pipeline lacks
  (VOP3 fusion, 64-bit carry, SGPR→VGPR moves, builtin→ABI-SGPR,
  binding→USER_DATA), so the arc grows to absorb them — **SAXPY is the first
  oracle** (intermediate milestone, green gates throughout) and the downsample
  stays the **named** MVP exit (N.5g) as **proven-equivalent + Cezanne
  pixel-match** (not literal byte-match). Sub-bites:
  - [x] **N.5a (2026-06-17)** — `src/gfx9_compile.cyr` encode driver:
    `gfx9_emit_program` walks the GISEL list + regalloc map → ISA dwords via the
    `gfx9_encode` encoders, resolving operands to phys reg (file-map from the
    defining op's class) / inline const / 32-bit literal. Covers SOP2 / VOP1
    (CVT) / VOP2 (incl. rev-shift) / SOPP; fail-loud on FLAT/VOP3/EXTRACT/builtin/
    buffer (N.5b+), the ≤1-literal rule, and a SALU-VGPR operand. Added the
    missing SOP2/VOP2 opcodes (llvm-mc gfx900-verified). 14 asserts: a 9-instr
    program byte-matches llvm-mc ground-truth + the fail-louds + the subtraction
    operand-file matrix. **Adversarial review (workflow): 1 confirmed, fixed** —
    non-commutative `v_sub` was on the commutative path and negated
    `vgpr - const`; dedicated `_emit_vop2_sub` picks `v_sub`/`v_subrev` (new
    opcodes) to always compute `a - b`, regression-tested 3 operand-file cases.
  - [~] **N.5b** — FLAT load/store + VOP3 `v_mul_lo_u32` encode + `gfx9_abi.cyr`
    (canonical ABI: builtin→v0/s6/s7, binding→USER_DATA SGPR pairs) +
    `gfx9_rsrc1`/`gfx9_rsrc2`.
    - [x] **N.5b-1 (2026-06-17)** — `src/gfx9_abi.cyr`: `gfx9_rsrc1`/`gfx9_rsrc2`
      (downsample `0x2C0083`/`0x18C` + deadbeef `0x44` oracles) + `gfx9_abi_assign`
      (binding k→`s[2k]`, WGID→`s[user_sgpr..]`, LID→`v0..`, sgpr/vgpr bases;
      GID→-1 needs-expansion). 12 asserts. Pin bumped 6.2.15→6.2.16. Adversarial
      review (workflow).
    - [x] **N.5b-2 (2026-06-17)** — `gfx9_compile.cyr`: FLAT load/store (SADDR:
      offset VGPR + binding USER_DATA base SGPR via the ABI) + VOP3
      `v_mul_lo_u32` (0x285) + `GFX9_FLAT_GLOBAL_LOAD_DWORD` (0x14). `gfx9_emit_program`
      gained the `abi` param. 10 asserts: load→mul→store byte-matches llvm-mc.
      **N.5b-1 review fix folded in:** `gfx9_rsrc1` now fails loud on a VGPRS/SGPRS
      field overflow (was a silent `& 0xF` wrap; LOW, defense-in-depth).
      **N.5b-2 review (workflow): 3 confirmed FLAT-path gaps fixed** — the offset/
      value operands are now checked (fail loud on const-offset `off_id`=0, on a
      uniform/non-VGPR operand, and on an out-of-range binding; `gfx9_abi_assign`
      caps bindings at the 16-SGPR USER_DATA limit). All latent (unwired) but go
      live in N.5c. `GISEL_EXTRACT` builtin resolution → N.5c.
  - [x] **N.5c (2026-06-17)** — top-level `gfx9_compile(ctx, spirv, n, isa, desc)`
    chaining validate→tables→lower(+uniformity)→isel→abi→regalloc→waitcnt→emit→pad,
    writing the ISA + `[isa_len,rsrc1,rsrc2,user_sgpr,num_bindings]` descriptor +
    `_emit_extract` (LID→`v_mov` v[comp]; WGID/GID fail loud). **First end-to-end
    oracle:** `_spv_build_saxpy_lid` (SAXPY re-indexed by LocalInvocationId) compiles
    to a coherent ISA + descriptor (12 asserts). **Adversarial review (workflow,
    read-only Explore agents): 1 confirmed CRITICAL, fixed** — the SPIR-V validate
    gate used `< 0` but the gate returns positive codes, so malformed input bypassed
    it into OOB table reads; now `!= 0` + a bad-magic regression test. GID expansion
    → N.5c-2.
  - [~] **N.5c-2** — GlobalInvocationId expansion + `EXTRACT(WorkgroupId)`→`s[tgid]`.
    - [x] **N.5c-2a (2026-06-17)** — the encode/regalloc foundation: EXTRACT
      file-class fix (`gisel_result_is_vgpr` — EXTRACT follows uniformity, used by
      both regalloc + the encode file-map), SOP1 `s_mov` encoder, and
      `EXTRACT(WorkgroupId)`→`s_mov` from the TGID SGPR. 9 asserts (WGID extract →
      SGPR `s_mov`, LID extract → VGPR `v_mov`). **Adversarial review (workflow):
      1 confirmed, fixed** — `_gisel_one` now fails loud on an UNKNOWN-uniformity
      EXTRACT (would have mis-filed to SGPR), mirroring the N.3 ALU guard.
    - [x] **N.5c-2b (2026-06-17)** — `_spirv_expand_gid` lowers
      `EXTRACT(OpLoad(GID).comp)` to `wgid.comp*local_size_comp + lid.comp` (synth
      WGID+LID builtins, `local_size` via `spirv_find_local_size` threaded down,
      `IMUL`+`IADD`; `comp` bounded ≤2). Uniformity → uniform `s_mul` + divergent
      `v_add`; file-class → wgid SGPR / lid VGPR. **Real GID-SAXPY compiles
      end-to-end** + a lowering-shape test; pipeline fixtures re-indexed to LID,
      `_spv_build_saxpy_gid` for the GID path. **Adversarial review (workflow): 2
      confirmed (same root), fixed** — `mir_alloc_synth_id` now fails loud (`-1`)
      at the `cap_ids` ceiling (was silent OOB), callers + `_mir_set_val` (`id<=0`)
      guard it; regression-tested. **N.5c-2 + N.5c COMPLETE** — a divergent-index
      compute kernel goes SPIR-V → GFX9 ISA + RSRC.
  - [~] **N.5d** — dispatch seam.
    - [x] **N.5d-1 (2026-06-17)** — parameterized `native_pm4_build_compute_downsample`
      to take `rsrc1`/`rsrc2` (was hardcoded `0x2C0083`/`0x18C`), so a compiler-derived
      RSRC flows into the dispatch packet; CPU-tested both the hand-authored + an
      alternate RSRC flow through.
    - [x] **N.5d-2 (2026-06-17) — HW-VERIFIED on Cezanne.** A compiled SPIR-V
      kernel `out[lid.x] = lid.x*3+7` runs correctly on the GPU (all 8 lanes
      7..28). `native_pm4_build_compute_generic` (generic 1-binding compute
      composer: compiler RSRC + binding VA + NUM_THREAD) + `programs/native_spirv_
      compute_e2e.cyr` (compile → BO → dispatch → readback) +
      `make test-native-spirv-compute-e2e` + a CPU structural test. **The compiler
      is HW-verified end-to-end — MVP reached.** (`_backend_native_shader_module_create`
      slot wiring + the multi-binding/TGID generic dispatcher remain for N.6.)
  - [ ] **N.5e** — VOP3 ops `v_add3_u32`/`v_bfe_u32`/`v_or3_b32` (new MIR/GISEL
    ops + isel rules + 3-src encode). *Not on the MVP critical path — N.5g shows
    the box-filter compiles with the base 2-input ALU set; these are throughput
    fusions for a later optimization pass.*
  - [ ] **N.5f** — 64-bit carry SALU (`s_add_u32`/`s_addc_u32` split) + SGPR→VGPR
    `v_mov` materialization for FLAT addresses. *Not on the MVP critical path —
    the SADDR FLAT form + the power-of-2 offset math reach the downsample without
    64-bit address splitting; needed when buffers exceed 32-bit VA offsets.*
  - [x] **N.5g (2026-06-17) — HW-VERIFIED on Cezanne. MVP exit reached.** A 2×2
    box-filter downsample compiled in-tree from SPIR-V pixel-matches an
    independent CPU box-filter on the GPU. `dst[i] = (src[a]+src[b]+src[c]+
    src[d]) >> 2` over each 2×2 source block (`dst` 4×4, `src` 8×8, LocalSize 16);
    2-binding dispatch (`src`→`s0:s1`, `dst`→`s2:s3`); all 16 texels correct, with
    a `0xBAADF00D` dst sentinel guarding against a spurious pass. Power-of-2 dims
    keep index math in shifts/masks (no div/mod). **No new compiler code** — reuses
    the N.6 multi-binding composer + cached submit; the compiler drove a real
    image kernel (5 `OpAccessChain`, 2×2 offset arithmetic, accumulate, shift)
    through N.2–N.5 unchanged. `programs/native_spirv_downsample_e2e.cyr` +
    `make test-native-spirv-downsample-e2e`. *(Single-channel; the RGBA
    channel-pack variant is mechanical channel repetition over the same op set —
    an optional N.5g-2 follow-up, proves nothing new about the compiler.)*
- [~] **N.6** *(3.2.8)* — First novel kernel + generic dispatcher.
  - [x] **N.6 core (2026-06-17) — HW-verified on Cezanne.** A novel 2-binding
    SAXPY-shape kernel (`y[lid.x]=3*x[lid.x]+y[lid.x]`, the compiler never saw it
    as bytes) compiles + dispatches reading/writing two real buffers (binding k →
    `s[2k:2k+1]`), `y[i]==3*i+100` all lanes. `native_pm4_build_compute_dispatch`
    (N-binding composer) + `native_compute_dispatch_cached_n` (variable BO list) +
    `programs/native_spirv_saxpy_e2e.cyr` + a CPU structural test.
  - [x] **N.6r (2026-06-17) — HW-verified on Cezanne. The compiler is reachable
    through the public API.** `gpu_shader_module_create_spirv` → the native slot
    compiles SPIR-V (`gfx9_compile`) into a GTT BO at a context VA + records RSRC +
    bindings + LocalSize in a **magic-tagged** shader-module struct;
    `gpu_compute_dispatch` detects the tag and dispatches via the generic N-binding
    composer (the legacy 16-byte deadbeef pair is discriminated by the absence of the
    magic — `native_compute_store` + the queue programs untouched).
    `programs/native_spirv_public_api_e2e.cyr` (2-binding SAXPY, all 8 lanes) +
    `make test-native-spirv-public-api-e2e` + 22 CPU asserts (struct layout / kind
    gate / dispatch binding guards / tag-checked release). Adversarially reviewed.
    *MVP scope:* 1-D grid (public `y`/`z` validated, compiled path uses `x`); SPIR-V
    `id_bound` ≤ 128; synchronous submit.
  - [x] **N.6 — 2-D / TGID compiled dispatch (2026-06-17) — HW-verified on Cezanne.**
    `gfx9_abi_assign` derives TGID_X/Y/Z + LID component enables from the max
    component each builtin is extracted with (MIR EXTRACT scan); `gfx9_rsrc2` gained
    `tgid_z` + `tidig_comp_cnt`; `native_pm4_build_compute_dispatch` +
    `_native_dispatch_compiled` carry full `(ntx,nty,ntz, gx,gy,gz)` + `local_y/z`.
    Fixed the bug where `gid.y` aliased `gid.x` (HW never preloaded `wgid.y`/`lid.y`).
    `programs/native_spirv_2d_dispatch_e2e.cyr` (4×4 global, 16/16 correct) +
    `test_gfx9_abi_assign_2d` + the 2-D rsrc2 oracles.
  - [x] **N.6 — timeline-queue compiled dispatch (2026-06-17) — HW-verified.**
    `native_compute_dispatch_cached_n_timeline` (N-binding analog of the deadbeef
    timeline submit) + the compiled dispatch routes to it on a current logical queue
    (signals the queue's persistent timeline; consumes a pending cross-ring barrier
    as an in-CS wait, Q.4 contract). `programs/native_spirv_queue_dispatch_e2e.cyr`
    (`out[i]=3i+7` on the COMPUTE queue, `gpu_queue_wait_idle`, 8/8 correct).
  - [x] **N.6 — id_bound ceiling 128 → 256 (2026-06-17).** `NATIVE_SHADER_CAP_IDS` +
    `CMP_ISEL_CAP` raised to the ARCHITECTURAL max: `RA_MAX_REGS=256` sizes the
    regalloc free-at scratch + the `gfx9_compile`/`gfx9_waitcnt` per-id file-maps, so
    >256 is rejected by those stages and needs growing their internal scratch (N.8).
  - [x] **N.6 — fuller CPU oracle (2026-06-17).** The 2-D path is CPU-cross-checked
    (`test_gfx9_abi_assign` 1-D + `test_gfx9_abi_assign_2d` + the 2-D/3-D `gfx9_rsrc2`
    oracles + the compiled-dispatch binding/grid guards); the six `native_spirv_*_e2e`
    programs are the end-to-end differential oracle on real silicon. **N.6 COMPLETE.**
  - [ ] **N.8 op-breadth carry: VOP3 literal materialization** — a VALU multiply by a
    constant > 64 (no VOP3 inline form) → `CMP_ERR_VOP3_LITERAL`. Needs a `v_mov` of
    the 32-bit literal into a register before the VOP3. Found via the N.6 2-D kernel
    (`gid.x * 100`); worked around there with an inline (`*16`) multiplier.
  - [x] **N-HARDEN.1 (2026-06-17) — security, found during N.5g via adversarial
    review.** **Unchecked OOB write in the SPIR-V table builders on untrusted
    `id_bound`, now gated.** `spirv_build_type_table` / `_const_table` /
    `_decoration_table` (`src/spirv_parse.cyr`) wrote `out + id*stride` for every
    result `<id>` (and an up-front `memset(out, 0, id_bound*REC)`) with **no
    capacity parameter and no bounds check**; the validate gate only caps
    `id_bound` at a loose `0x40000000` (`wgpu_descriptors.cyr`), unrelated to the
    caller's fixed buffer → a crafted oversized `id_bound` silently corrupted
    memory, surfacing much later as a spurious `MIR_ERR_ID_OOR` (it bit dev as a
    confusing `-25`). **Fix:** each builder takes `cap` (table capacity in
    records), rejects `id_bound > cap` with `-SPIRV_ERR_ID_CAP` before touching
    `out`, and guards each per-id write (`id == 0 || id >= cap`); `gfx9_compile`
    threads the capacity from a new `CC_CAP_IDS` ctx field and maps any rejection
    to `CMP_ERR_TABLE`. +8 asserts (per-builder reject + untouched canary +
    `gfx9_compile` over-capacity integration reject). Landed **before** the N.6
    remainder exposes this path through `gpu_shader_module_create`.
- [x] **N.7** *(3.2.8)* **(2026-06-17) — COMPLETE, HW-verified on Cezanne.** Control
  flow: uniform `if` (`s_cbranch_scc0`) + divergent `if` (EXEC mask). Loops / `OpPhi` /
  nested ifs fail loud (N.8+).
  - [x] **N.7a (2026-06-17) — control-flow encoders + byte-oracle.** SOPP branches
    (`s_branch`, `s_cbranch_scc0/scc1`, `s_cbranch_execz/execnz`), the new
    `gfx9_enc_sopc` + u32 `s_cmp_*` (→ SCC), and the EXEC-mask ops
    (`s_and_saveexec_b64` / `s_andn2_b64` / `s_or_b64` / `s_mov_b64`) — all
    llvm-mc-verified, +14 asserts (`test_gfx9_enc_control_flow`).
  - [x] **N.7b — uniform `if` pipeline (2026-06-17) — COMPLETE, HW-verified on Cezanne.**
    - [x] **N.7b-1 (2026-06-17) — control-flow front-end.** `OpLabel`/`OpBranch`/
      `OpBranchConditional`/`OpSelectionMerge` + integer comparisons lower to new MIR
      ops (`LABEL`/`BRANCH`/`COND_BRANCH`/`ICMP_*`), modelled as flat-list instructions
      (no block array → no `mir_mod_init` change). Entry label skipped (straight-line
      kernels unchanged); `ICMP` uniformity = meet of operands (`wgid.x==c` → uniform).
      Loops + phi still fail loud. `test_spirv_lower_uniform_if` (+17 asserts).
    - [x] **N.7b-2a (2026-06-17) — control-flow isel.** `LABEL`/`BRANCH`/`COND_BRANCH`/
      `ICMP_*` → `GISEL_LABEL`/`S_BRANCH`/`S_CBRANCH`/`S_CMP` (SCC; SOPC op in flags<<8;
      divergent compare fails loud → N.7c). Regalloc + s_waitcnt traverse the CF list
      unchanged (new ops carry no SSA result). `test_gfx9_isel_uniform_if` (+15).
    - [x] **N.7b-2b (2026-06-17) — emit + HW. The uniform `if` is COMPLETE.** Two-pass
      branch layout in `gfx9_emit_program` (LABEL records its byte offset / 0 bytes;
      branches patched `simm16 = (target-(branch+4))/4`); `_emit_sopc` for `S_CMP`.
      `test_gfx9_compile_uniform_if` (empty-if → `s_cbranch_scc0 +1` / `s_branch +0`)
      + `programs/native_spirv_uniform_if_e2e.cyr` HW-verified on Cezanne
      (`if (wgid.x==0){out[gid.x]=gid.x+7}`, grid 2 → wg1 gated out, out[8..15] sentinel).
      Straight-line kernels byte-identical (no LABEL/branch).
  - [x] **N.7c — divergent `if`** (2026-06-17) — COMPLETE, HW-verified on Cezanne.
    - [x] **N.7c-1 (2026-06-17) — VOPC `v_cmp` encoders.** `gfx9_enc_vopc` + u32
      `v_cmp_*` (lt/eq/le/gt/ne/ge) → VCC, llvm-mc-verified (`test_gfx9_enc_vopc`, +7).
    - [x] **N.7c-2a (2026-06-17) — divergent compare selection.** `_gisel_vcmp` +
      `GISEL_V_CMP` (divergent ICMP → `v_cmp`→VCC; uniform stays `s_cmp`→SCC); constant
      second operand → src0 + flipped predicate. `_emit_vopc` in the back-end. Divergent
      COND_BRANCH fails loud (EXEC mask is N.7c-2b). +12 asserts.
    - [x] **N.7c-2b (2026-06-17) — EXEC-masking branch + HW. The divergent `if` is
      COMPLETE.** Divergent COND_BRANCH → `s_and_saveexec_b64` + `s_cbranch_execz`;
      merge LABEL precedes its `s_or_b64` EXEC restore. Saved EXEC parks in `s[100:101]`
      (regalloc cap→100 + rsrc1 SGPR bump, only when divergent CF — non-divergent kernels
      byte-identical). Nested ifs fail loud (N.8). `native_spirv_divergent_if_e2e.cyr`
      HW-verified on Cezanne (`if (gid.x<4){out[gid.x]=gid.x+7}`, one wave, lanes 4-7
      masked, out[4..7] sentinel). +13 asserts.
- [~] **N.8** *(3.2.8)* — Scalar SPIR-V f32 compiler **complete** (FAbs + const-fold
  close it). **REVISED VERSION PLAN (maintainer, 2026-06-17):** the remaining big-ticket
  op breadth is split across its own minors instead of one "N.8":
  **3.2.8** = scalar f32 complete (N.8b-7 FAbs ✓ + N.8b-8 const-fold) →
  **3.2.9** = int div/mod (no HW divide → reciprocal/Newton) →
  **3.2.10** = vectors (component-wise vec2/3/4) →
  **3.2.11** = per-ext-set tracking (verify `OpExtInst` set is GLSL.std.450) →
  then resume the previously-roadmapped 3.2.x arc (Phase F f64, Phase R render-graph
  multi-queue, …) which all shift down accordingly.
  - [x] **N.8a (2026-06-17) — VOP3-literal materialization.** A non-inline constant
    operand of a VOP3a op (`gid.x*100`) is materialized into the first-free scratch VGPR
    via `v_mov_b32`, then used; rsrc1 VGPR count bumped only when materializing
    (non-materializing kernels byte-identical). `native_spirv_mul_literal_e2e.cyr`
    HW-verified on Cezanne. +7 asserts. Two-literal VOP3 still fails loud (const-fold).
  - [x] **N.8b-1 (2026-06-17) — GLSL.std.450 math.** `OpExtInst` front end (assumes the
    GLSL.std.450 set): FMin/FMax → `v_min_f32`/`v_max_f32` (VOP2), Sqrt → `v_sqrt_f32`
    (VOP1); unknown numbers fail loud. `native_spirv_glsl_max_e2e.cyr` HW-verified on
    Cezanne (exact f32). +17 asserts (lower parse + emit byte-exact).
  - [x] **N.8b-2 (2026-06-17) — GLSL Floor + ternary Fma.** `Floor` → `v_floor_f32`
    (VOP1); `Fma` → `v_fma_f32` (VOP3 3-src via the new `_emit_vop3_3src`; the ext-inst
    handler gained ternary arity). `native_spirv_fma_e2e.cyr` HW-verified on Cezanne
    (gid²+gid, exact f32). +15 asserts. A non-inline FLOAT const in a VOP3 fails loud.
  - [x] **N.8b-3 (2026-06-17) — float inline constants.** `gfx9_inline_float`
    (±0.5/1/2/4 → 240..247) + `_cmp_resolve` routes f32 consts through it.
    `native_spirv_fma_const_e2e.cyr` (`fma(gid,2.0,1.0)`=2·gid+1) HW-verified on Cezanne.
    `0.25` is NOT inline → downsample byte/RSRC oracle unchanged (verified). +14 asserts.
    VOP2 still needs the const in src0 (operand placement → N.8b-4).
  - [x] **N.8b-4 (2026-06-17) — signed int compares + GLSL FClamp.** SLT/SGT/SLE/SGE →
    i32 SOPC/VOPC (EQ/NE stay sign-agnostic); FClamp → `v_med3_f32`. HW-verified on
    Cezanne (`native_spirv_signed_if_e2e.cyr`, `native_spirv_fclamp_e2e.cyr`). +33 asserts.
  - [x] **N.8b-5 (2026-06-17) — store-constant materialization.** `out[i]=<const>` —
    the constant is materialized into the scratch VGPR (`v_mov_b32`) before the FLAT store
    (SSA stores byte-identical). HW-verified on Cezanne (`native_spirv_store_const_e2e.cyr`,
    `out[gid.x]=0xCAFE`). +7 asserts.
  - [x] **N.8b-6 (2026-06-17) — VOP2 const-operand (ALREADY IMPLEMENTED; verified +
    covered).** `_emit_vop2`'s commutative path already swaps a non-VGPR operand into src0;
    with N.8b-3 inline floats, `x*2.0`/`x+1.0`/`x|5` all compile. HW-verified
    (`native_spirv_vop2_const_e2e.cyr`, `float(gid.x)*2.0`) + byte-exact CPU test (+5).
    No new src code — the N.8b-3 "VOP2 placement pending" note was wrong.
  - [x] **N.8b-7 (2026-06-17) — GLSL FAbs.** `abs(x)` → `v_and_b32 x, 0x7FFFFFFF` (clear
    the sign bit; correct for finite/±inf/nan/±0). HW-verified on Cezanne
    (`native_spirv_fabs_e2e.cyr`, `abs(float(gid.x)-4.0)`). +7 asserts. Also rebased the
    GISEL error codes 40/41 → 100/101 (op enum reached 41).
  - [x] **N.8b-8 (2026-06-17) — const-fold two-literal ops. SCALAR f32 COMPILER
    COMPLETE.** `_spirv_both_const` + `_spirv_const_fold` (int i64-masked, float via the
    f32/f64 builtins) fold a two-const binary op into one constant at lowering time.
    HW-verified on Cezanne (`native_spirv_const_fold_e2e.cyr`, `gid.x+6*7`). +14 asserts;
    the gid-kernel fixtures' `1<<2` now folds (tests updated) + uniform-SALU coverage moved
    to `test_gfx9_regalloc_uniform_salu`. **→ ready to cut 3.2.8.**
  - [x] **N.9 *(3.2.9)* — int div/mod.** GFX9 has no integer divide → LLVM's float-
    reciprocal **vector** udiv macro (19 loop-free instrs: cvt→rcp_iflag→×0x4F7FFFFE→cvt
    →Newton(sub/mul_lo/mul_hi/add)→estimate(mul_hi/mul_lo/sub)→2 correction cmp+cndmask).
    Design-verified over 5M+ pairs + ULP-perturbation (the magic constant biases the
    reciprocal down so v_rcp's ≤1-ULP error can't overshoot; 2 +1 corrections repair the
    underestimate). b=0 → 0xFFFFFFFF (natural saturate; pinned in the HW test).
    - [x] **N.9a (2026-06-17) — encoders + oracle.** 5 opcode constants (cvt_f32_u32 0x06,
      cvt_u32_f32 0x07, rcp_iflag_f32 0x23, mul_hi_u32 0x286, cndmask_b32 0x00), all
      llvm-mc-verified, reusing existing encoders. +7 byte-oracle asserts.
    - [x] **N.9b-1 (2026-06-18) — primitives plumbed.** 6 primitive ops (VMOV/CVT_F32_U/
      CVT_U_F32/RCP_IFLAG/MUL_HI/CNDMASK) MIR→isel→emit; vector-only → uniformity-forced
      DIVERGENT. Dedicated `_emit_cndmask` (VCC implicit, a=false/b=true). +10 byte asserts.
    - [x] **N.9b-2 (2026-06-18) — OpUDiv macro + HW.** `_spirv_lower_udiv` expands OpUDiv to
      the float-reciprocal vector macro (VMOV N/D → force VGPR; 19-op reciprocal; synth ids;
      contiguous cmp/cndmask). b=0 → deterministic 0xFFFFFFFF guard (the natural saturate
      is a-derived garbage — confirmed on HW, per design risk #1). `native_spirv_udiv_e2e.cyr`
      value-exact on Cezanne over the edge matrix. Adversarial review + HW-stress.
    - [x] **N.9c (2026-06-18) — u32 OpUMod (137).** Factored `_udiv_core` (shared by udiv);
      umod applies the remainder's correction-2 + a b=0→N guard. udiv re-verified byte-
      identical. `native_spirv_umod_e2e.cyr` value-exact on Cezanne + HW-stress.
    - [x] **N.9d-1 (2026-06-18) — signed OpSDiv (135) + OpSRem (138).** `_signed_prep`
      (ASHR-31 sign masks + branchless abs) → `_udiv_core` on magnitudes → sign-apply
      (`signA⊕signB` for div, `signA` for rem). New `MIR_OP_ASHR` → `v_ashrrev_i32` 0x11 /
      `s_ashr_i32` 0x20. HW-verified on Cezanne (8 signed cases each) + HW-stress.
    - [x] **N.9d-2 (2026-06-18) — OpSMod (139), floored.** `_spirv_lower_smod` = SRem + the
      floored fixup: two `ICMP_NE`-gated `CNDMASK`s add the divisor iff (remainder nonzero
      AND signs differ), so the result follows the divisor's sign (GLSL `mod`). No new
      encoders/isel — reuses VMOV/XOR/ASHR/ICMP_NE/CNDMASK/IADD/ISUB (37 MIR instrs).
      `native_spirv_smod_e2e.cyr` value-exact on Cezanne (8-lane signed matrix) + the
      `test_spirv_lower_smod` structural unit test. **→ 3.2.9 (int div/mod) COMPLETE.**
  - [x] **N.10 *(3.2.10)* — vectors.** Component-wise vec2/3/4 ops (load/store/arith).
    Scalarize-on-lower: a vecN is a `MIR_VK_VECTOR` value packing its N component scalar
    `<id>`s (16 bits each) into the payload; count rides in the packed type. Every vector op
    emits N scalar ops, so the scalar back end (isel/regalloc/waitcnt/emit) is reused
    unchanged. Design-validated via the N.10 design workflow.
    - [x] **N.10a (2026-06-18) — infra + Construct + Extract.** `MIR_VK_VECTOR` +
      `mir_set_vector` / `mir_is_vector` / `mir_vec_comp`; `OpCompositeConstruct` (80) packs the
      operand ids (no instructions); `OpCompositeExtract` (81) from a vec → one `VMOV` from the
      packed component (the GID vec3 special-case stays ahead of it). CPU-only (+24 asserts).
      Adversarial review (7 findings fixed): `mir_emit` centrally rejects a `MIR_VK_VECTOR`
      operand (unimplemented vector ops fail loud, not silent-wrong); Extract bounds the
      component for all vec3 builtins; Construct requires scalar CONST/SSA operands. **Deferred
      (fail loud):** vector/nested operands to Construct + non-scalar-per-component arity —
      land with broader Construct breadth later in N.10.
    - [x] **N.10b (2026-06-18) — component-wise binary arith + first vector HW.** vec
      `OpFAdd/FMul/...` → N scalar ops (`_spirv_lower_vec_binop`) + scalar broadcast +
      count/base guards. **Plus a prerequisite enabler:** a constant `OpAccessChain` index now
      VMOV-materializes its byte offset into a VGPR (the FLAT emitter requires a VGPR offset;
      prior kernels only ever used the dynamic gid offset, so constant-index access returned
      `CMP_ERR_FLAT_CONST_OFFSET`). HW: `native_spirv_vector_add_e2e.cyr` vec3 add+mul
      value-exact on Cezanne (+18 CPU asserts). The immediate-offset FLAT form (avoid the
      extra VMOV) is a later optimization.
    - [x] **N.10c (2026-06-18) — vector load/store.** `OpAccessChain` accepts a vecN element
      (std430 stride computed from the element type — vec2→8, vec3/vec4→16; no `ArrayStride`
      lookup, an MVP layout assumption); `_spirv_lower_vec_load` / `_spirv_lower_vec_store`
      scalarize a vecN load/store into N consecutive 4-byte accesses (`element_base + j*4`).
      HW: `native_spirv_vector_load_store_e2e.cyr` (`out[i]=a[i]+b[i]` over `array<vec4>`,
      gid-indexed) value-exact on Cezanne (+46 CPU asserts). Adversarial review: a non-std430
      `ArrayStride` now **fails loud** (`_spirv_check_array_strides`) instead of silently
      mis-addressing; vec2/vec3/dynamic-index coverage added; the dynamic-index overflow is
      documented as SPIR-V UB (OOB index, runtime bounds-checking out of scope). Multi-member
      struct stays deferred (single-member, member-0 MVP).
    - [x] **N.10d (2026-06-18) — `OpConstantComposite` + splat.** Vector constants: the seed
      pass reuses the Construct packer (constituents are seeded scalar consts) → a
      `MIR_VK_VECTOR`; splat is Construct/ConstantComposite with a repeated operand (no new
      code). HW: `native_spirv_vector_const_e2e.cyr` (`out[i]=a[i]+vec4(10,20,30,40)`)
      value-exact on Cezanne — the non-inline constants exercise the VOP2-literal path (+16
      CPU asserts). Adversarial review: `_spirv_lower_vec_binop` now const-folds a component
      with two const operands (a vec const+const would otherwise emit an unencodable
      `v_add(const,const)`). **→ Phase N.10 (vectors) complete.**
    - [x] **N.10e (2026-06-18) — cut 3.2.10.** VERSION/README/CHANGELOG/CLAUDE.md bumped;
      dist re-stamped; closeout green (suite 3800, bench, vector HW arc on Cezanne);
      release-readiness audit. **→ 3.2.10 (vectors) shipped.**
  - [x] **N.11 *(3.2.11)* (2026-06-18) — per-ext-set tracking.** `_spirv_find_glsl_ext_set`
    resolves the `OpExtInstImport "GLSL.std.450"` id (packed-string match); `_spirv_check_ext_sets`
    (in `spirv_lower_module`) requires every `OpExtInst` to reference it, else `LOWER_ERR_EXT_SET`
    — drops the MVP "only GLSL.std.450 imported" assumption. No HW change (existing GLSL kernels
    still run on Cezanne); +5 CPU asserts. Loops / `OpPhi` / nested-if stay fail-loud (v3.3+).

---

## Phase F — f64 (double-precision) compute → 3.2.10–3.2.11

Proposal: [`v3.2-f64-compute.md`](../proposals/v3.2-f64-compute.md).
Consumer: **attn11**. f64 hard-gates on SPIR-V on **both** backends (no
`ShaderF64` feature; WGSL has no f64). Zero new Backend slots. Correctness,
not speed, is the deliverable. **No escape hatch — general native f64 is a
committed 3.2.x minor (3.2.11), gated on Phase N which is also in-arc.**

- [x] **F.1** *(3.2.12)* (2026-06-18) — `GpuCapabilities` `shader_f64` field at **+128**,
  struct **128→136** (T.4's texture-compression bitset already took +120, so the proposal's
  "+120→128" was stale) + `gpu_caps_shader_f64` accessor / `gpu_caps_set_shader_f64` setter +
  `gpu_caps_new` sized up. +3 core asserts. (F.2–F.10 still annotated 3.2.10/3.2.11 — reflow to
  3.2.12/3.2.13 lands with F.10.)
- [x] **F.2** *(3.2.12)* (2026-06-18) — per-backend f64 detection. native
  `gpu_caps_native_shader_f64()` → `MABDA_NATIVE_F64` (the f64 ANSWER, 0 until F.7) + the wgpu
  NECESSARY-gate `gpu_wgpu_spirv_passthrough_supported(device)` → `has_feature(SpirvShaderPassthrough
  0x00030017)`. Kept F.1's struct accessor `gpu_caps_shader_f64(c)` (the proposal's
  `gpu_caps_shader_f64(ctx)` would clash). **Adversarial review:** the wgpu helper is deliberately
  NOT named `*_shader_f64` — passthrough-available ≠ Vulkan `shaderFloat64` (review #1), and the
  reference launcher doesn't request passthrough yet so it returns 0 today (review #3); the honest
  wgpu f64 detect (compose the gate + a real-module probe + the launcher request) is **F.8**, so
  F.2 populates NO wgpu f64 field. +5 core asserts.
- [x] **F.3** *(3.2.12)* (2026-06-18) — f64 shader gate. `spirv_uses_f64` (`spirv_parse.cyr`)
  scans `OpCapability Float64` (bounded, zero-wc-safe); the native shader-create rejects an f64
  module up front while `MABDA_NATIVE_F64 == 0` (pre-F.7). **Adapted** the proposal's dispatch
  guard to **create-time** — both backends already reject f64-on-unsupported at create (native
  gate/compiler; wgpu f64 non-functional until F.8 wires passthrough + the launcher request), so
  a dispatch guard + a flag on the opaque shmod would never fire; the F.2 cap check stays the
  consumer's primary hard-fail. **Adversarial review:** closed an OOB operand read in
  `spirv_uses_f64` + the sibling scanners (`off + wc > total` bound, for untrusted/odd-length
  bytes). +5 asserts; non-f64 native kernel (`smod_e2e`) unaffected on Cezanne.
- [x] **F.4–F.6** *(3.2.12)* (2026-06-18) — **first native f64 on Cezanne.** `native_gfx9_shader_fma_f64`
  (`V_FMA_F64` VOP3 0x1CC + `GLOBAL_LOAD/STORE_DWORDX2`, RSRC1 0x2C0042 = 10 VGPRs; llvm-mc-pinned
  + `test_gfx9_enc_f64` oracle) + `native_pm4_build_compute_fma_f64` composer + `native_f64_fma_e2e.cyr`:
  `out=a*b+c` **bit-exact vs a fused-FMA reference** (8 lanes). Fused-rounding proven — the GPU's
  single-rounding `v_fma_f64` matches `math.fma` (not the unfused `mul`+`add`, which differs by 1 ULP
  on lanes like `0.1*0.1+0.1`); Cyrius has no fused builtin so the reference is embedded + math.fma-
  validated. +4 asserts; no regression (smod/vector_const still pass on Cezanne). Decoupled from Phase N.
- [x] **F.8a** *(3.2.12)* (2026-06-18) — wgpu **raw-SPIR-V passthrough escape hatch** (FFI
  plumbing): a new C shim `wgpu_shim_create_shader_module_spirv(device, words, word_count)` wraps
  `wgpuDeviceCreateShaderModuleSpirV` (naga-bypass) → `fn_table[66]` (`FN_COUNT` 66→67) → the
  Cyrius wrapper `wgpu_device_create_shader_module_spirv_passthrough` (`src/wgpu_ffi.cyr`,
  `_fp(66)`); the launcher requests `SpirvShaderPassthrough` gated by `wgpuAdapterHasFeature`.
  **Launcher regression: `spirv_e2e` all-PASS + exit 0 on Cezanne.** NOTE (F.8b): this is **not**
  the f64 route — f64 uses `ShaderF64` + the standard naga path; passthrough is a general escape
  hatch for SPIR-V naga can't carry, kept + exercised by the e2e where the adapter supports it
  (unsupported on RADV/Cezanne).
- [x] **F.8b** *(3.2.12)* (2026-06-18) — wgpu f64 via **`WGPUNativeFeature_ShaderF64`**, HW-verified
  on Cezanne. **Corrected the F.2/proposal premise** ("passthrough is the only f64 route" — wrong):
  wgpu-native has a real `ShaderF64` (0x0003001D) and naga's SPIR-V frontend accepts f64 once it is
  enabled, so f64 rides the ordinary `gpu_shader_module_create_spirv` path. Launcher requests
  `ShaderF64` (gated); `gpu_wgpu_shader_f64_supported(device)` is the authoritative bit;
  `gpu_caps_wgpu_shader_f64` = that bit gated by `MABDA_WGPU_COMPUTE` (0 today — wgpu compute
  dispatch is a v3.0 stub → honest 0, no silent f32 fallback, M.6b). `_wgpu_f64_probe_spirv`
  (55 spirv-as words). `programs/f64_compute_e2e.cyr` (HW: ShaderF64 on, f64 module creates via
  naga, passthrough skipped-unsupported, caps 0) + `backend.tcyr` +2 tests. Folds in the former F.8c.
  See [[project_wgpu_f64_via_shaderf64_not_passthrough]].
- **F.7** *(3.2.13)* — General native f64: route consumer f64 SPIR-V through the SPIR-V→GFX9
  compiler (`V_*_F64`). Central new concept: **64-bit register pairs** (even-aligned) as `MIR_T_F64`
  threaded MIR→isel→regalloc→encode. Milestone-first, then breadth; flip `MABDA_NATIVE_F64`→1 ONLY
  when f64 runs end-to-end for every op attn11 uses — partial back end stays 0 (review F.2 #2). The
  9-module understand map (2026-06-18) confirmed the ABI/RSRC1 `ceil(vgpr_hw/4)-1` formula already
  handles pairs; the load-bearing change is regalloc pair allocation (F.7c).
  - [x] **F.7a** *(2026-06-18)* — front-end f64 type: `MIR_T_F64` + `_mir_lower_type` accepts
    `OpTypeFloat 64` (scalar + vec); `compiler_lower.tcyr` f64 assertions flipped from UNSUPPORTED.
  - [x] **F.7b** *(2026-06-18)* — `V_*_F64` GFX9 encoders, llvm-mc gfx900-verified + byte-oracle'd
    (`compiler_encode.tcyr` `test_gfx9_enc_f64_alu`, +11): VOP3 V_ADD_F64=0x280 / V_MUL_F64=0x281 /
    V_MIN_F64=0x282 / V_MAX_F64=0x283 + VOP1 CVT_F64_F32=0x10 / CVT_F32_F64=0x0F / CVT_{F64_I32,
    I32_F64,F64_U32,U32_F64} (V_FMA_F64=0x1CC + DWORDX2 from F.4). No V_SUB_F64/V_ABS_F64 (src-mods);
    FDiv/FSqrt macros deferred to breadth. GISEL_V_*_F64 enum + GISEL→GFX9 dispatch land with isel (F.7d).
  - [x] **F.7c** *(2026-06-18)* — regalloc **even-aligned 64-bit pairs**: `_ra_claim_pair` (lowest
    even reg, both halves free; odd base rounds up; step 2) + driver f64 width detection
    (`mir_val_type` base == `MIR_T_F64` → claim pair, `hw += 2`). `compiler_backend.tcyr` +2 tests
    (direct `_ra_claim_pair` align/reuse/half-busy/exhaustion + a divergent f64 chain → v[2:3], v[4:5],
    hw 6). ABI RSRC1 `ceil(hw/4)-1` already pair-correct (F.4 oracle).
  - [x] **F.7d** *(2026-06-18)* — isel f64 routing: type-aware `_gisel_float_op(mop, tbase)` →
    `GISEL_V_{ADD,MUL,FMA,MIN,MAX}_F64` (f64 SUB/SQRT/FLOOR/CLAMP/ABS fail loud, breadth); f64
    load/store → `GISEL_GLOBAL_{LOAD,STORE}_DWORDX2`; compile dispatch reuses the VOP3 emitters
    (regalloc's even low reg = the pair) + parameterized FLAT opcode; `gfx9_waitcnt` tracks DWORDX2.
    Tests: `_gisel_float_op` f32/f64 + an f64 isel check (DWORDX2 routing integration-tested in F.7e).
  - [x] **F.7e** *(2026-06-18)* — *(HW)* **first COMPILED f64 on silicon.** f64 FMA SPIR-V →
    `gfx9_compile` → dispatched on Cezanne, all 8 lanes bit-exact vs the fused-FMA reference;
    RSRC1 `0x2C0042` matches the F.4 oracle (regalloc produced the same budget). Fixed the f64
    access-chain stride (esize 8) in `_spirv_lower_access_chain` + `_spirv_std430_stride_ok`.
    `native_spirv_f64_fma_e2e.cyr` + `_spv_build_f64_fma` + a `gfx9_compile` CPU test (4 bindings,
    8 user-SGPRs, v_fma_f64 in the ISA). Proves F.7a–d compose end-to-end.
  - [x] **F.7f.1** *(2026-06-18)* — FADD + FMUL + GLSL FMin + FMax (already wired by F.7b/d) HW-verified:
    `out=max(min(a*b+c,b),a)` compiled + dispatched on Cezanne, all 8 lanes bit-exact vs an in-process
    f64 reference (f64_add/mul + f64_lt/gt min/max). `native_spirv_f64_arith_e2e.cyr` + `_spv_build_f64_arith`
    + a `gfx9_compile` CPU test (v_add/mul/min/max_f64 in the ISA). Lane 6 shows UNFUSED (...5C2) vs the
    F.7e FUSED (...5C3) — separate OpFMul+OpFAdd round twice. (Min/max NaN/±0 conformance: breadth.)
  - [x] **F.7f.2** *(2026-06-18)* — **f32↔f64 CVT** (OpFConvert): `MIR_OP_FCONVERT` + lowering +
    result-type-dispatched isel (f64→V_CVT_F64_F32 widen, f32→V_CVT_F32_F64 narrow) through `_emit_vop1`
    + the F.7b encoders. HW-verified: `out=double(float(a))` bit-exact vs `f32_to_f64(f64_to_f32(a))`.
    `native_spirv_f64_cvt_e2e.cyr` + `_spv_build_f64_cvt` + a `gfx9_compile` CPU test.
  - [x] **F.7f.3** *(2026-06-18)* — f64 **FSub + FAbs** via VOP3 source modifiers: FSub = V_ADD_F64 +
    NEG src1 (hi 0x40000000); FAbs = V_MAX_F64(|x|,|x|) + ABS src0+src1 (lo 0x300), llvm-mc-verified,
    reusing the F.7b opcodes. `GISEL_V_SUB_F64`/`GISEL_V_ABS_F64` + `_emit_fsub_f64`/`_emit_fabs_f64`.
    HW-verified `out=abs(a-b)` bit-exact vs `f64_abs(f64_sub(a,b))`. `native_spirv_f64_subabs_e2e.cyr`.
  - [x] **F.7f.4** *(2026-06-18)* — **`array<vecN-f64>` load/store HW-verified** (closes the F.7e
    tracked gap): `out=a+b` over `array<dvec2>` bit-exact on Cezanne (x+y), confirming the f64
    access-chain stride (std430 dvec2=16) + the i*8 per-component offset. No src change (N.10 + F.7a-e
    wiring). `native_spirv_f64_vec_e2e.cyr`. Found the buffer-sizing trap (vec kernels need ~2× cap_ids
    buffers or the memsets corrupt — [[reference_compiler_test_buffer_sizing]]).
  - **F.7f.5 finding** *(2026-06-18)* — **GFX9 `v_sqrt_f64` is an ~f32-precision APPROXIMATION**, not
    correctly-rounded (HW-proven: every lane ~2^26 ULP off, even sqrt(4)). The single-op attempt was
    reverted to fail-loud. f64 **FSqrt** AND **FDiv** therefore need the LLVM-style Newton-refinement
    macro (`v_rsq_f64`/`v_rcp_f64` seed + `v_fma_f64` iterations + `v_div_fixup_f64`) — the hardest f64
    ops, same class as the N.9 int udiv. See [[project_gfx9_f64_transcendentals_approximate]].
  - **F.7f libm arc** *(maintainer decision 2026-06-18, after the attn11 scope)* — attn11's f64 GPU
    path is M18/M19 (deferred, no consumer yet), but its blocking f64 op set is {sqrt, exp, ln, tanh,
    div} — all transcendentals. Decision: **build the full GFX9 f64 transcendental libm now** rather
    than stop at the algebraic MVP. Dependency order: **FDiv → FSqrt → exp → ln → tanh → pow** (div's
    reciprocal pattern seeds sqrt; tanh is built on exp; pow = exp(e·ln(b))). Each op is the LLVM
    gfx900 expansion, byte-oracle'd then HW-verified value-exact vs a CPU f64 reference.
  - **F.7f.6 (FDiv macro)** — correctly-rounded f64 div, the LLVM 11-instr reciprocal Newton macro
    (`v_div_scale_f64` VOP3B + `v_rcp_f64` seed + `v_fma_f64` iterations + `v_div_fmas_f64` + the
    `v_div_fixup_f64` rounding fixup, with VCC threaded scale→fmas).
    - [x] **F.7f.6a** *(2026-06-18)* — encoder foundation: `V_RCP_F64`/`V_DIV_SCALE_F64`/
      `V_DIV_FMAS_F64`/`V_DIV_FIXUP_F64` opcodes + the `gfx9_enc_vop3b_lo` VOP3B form (SDST in
      lo[14:8]) + the `test_gfx9_enc_f64_div` byte-oracle (llvm-mc gfx900-matched). No lowering yet.
    - [x] **F.7f.6b** *(2026-06-18)* — **HW-verified on Cezanne**: OpFDiv → the 11-op reciprocal-Newton
      MIR macro (`_spirv_lower_fdiv`) → 6 new MIR/GISEL ops (RCP/DIV_SCALE/DIV_FMAS/DIV_FIXUP +
      the NEG-src0 FMA_N0/FMA_N0_ONE) → 3 new emit helpers (div_scale VOP3B→VCC, the two NEG-src0
      FMAs); regalloc claims the f64 pairs op-agnostically, waitcnt passes them through, VCC threads
      2nd-DIV_SCALE→DIV_FMAS by emission contiguity. `out=a/b` **BIT-EXACT vs CPU f64_div** on all 8
      lanes incl. the repeating-fraction cases (1/3, 22/7, 355/113, 1e10/3, 1/1e6) —
      `native_spirv_f64_div_e2e.cyr`. f32 FDiv fails loud (see the native-f32-div note below).
      Adversarially reviewed (Workflow, 4 dims / 11 agents; 5 findings refuted): the one confirmed
      finding — the new f64 div macro primitives weren't in `_mir_instr_unif`'s forced-DIVERGENT
      guard (latent: masked today by regalloc forcing VGPR for V_* ops, but a foot-gun) — was fixed
      (guard extended to ops 46–51, pinned by `test_mir_unif_f64_div_ops_forced_divergent`).
    - [ ] **native-f32-div** *(deferred, no consumer)* — `_spirv_lower_fdiv` rejects f32 (returns
      UNSUPPORTED_TYPE; pinned by `test_spirv_lower_fdiv_f32_rejects`). The f32 parallel macro
      (v_div_scale_f32 / v_rcp_f32 / v_div_fmas_f32 / v_div_fixup_f32) is straightforward but
      unneeded: attn11's f32 portable path runs on wgpu (naga lowers f32 div). Add when a native
      f32-div consumer appears.
  - **F.7f.7 (FSqrt macro)** — correctly-rounded f64 sqrt, the ~22-instr LLVM gfx900 expansion:
    a `v_rsq_f64` seed + the Newton-Goldschmidt refinement (mul + 7×fma with NEG-src0/inline-0.5),
    wrapped by subnormal scaling (`v_cmp_gt_f64` gate → cndmask the 2^256 scale exponent →
    `v_ldexp_f64` up, then `v_ldexp_f64` down by 2^-128) and a special-case passthrough
    (`v_cmp_class_f64` mask 0x260 = +0/-0/+inf → cndmask the input through, so sqrt(0)=0 / sqrt(inf)=inf
    instead of the Newton 0·inf=NaN). +normal computes; negative/NaN take the Newton path → NaN.
    - [x] **F.7f.7a** *(2026-06-19)* — encoder foundation: `V_RSQ_F64` (VOP1 0x26), `V_LDEXP_F64`
      (VOP3 0x284, f64,i32), `V_CMP_GT_F64`/`V_CMP_CLASS_F64` (VOPC 0x64/0x12) opcodes — all reuse the
      existing vop1/vop3a/vopc encoders — + the `test_gfx9_enc_f64_sqrt` byte-oracle (llvm-mc-matched).
    - [x] **F.7f.7b** *(2026-06-19)* — **HW-verified on Cezanne**: OpExtInst Sqrt (f64) → the
      Newton-Goldschmidt macro (`_spirv_lower_fsqrt_f64`) → 5 new MIR/GISEL ops (RSQ / CMP_CLASS /
      CNDMASK / FMA_N0_HALF / MUL_HALF) + 4 emit helpers (the inline-0.5 FMA/MUL, the VCC-writing
      cmp_class, the f64-pair cndmask = 2× v_cndmask_b32). `out=sqrt(a)` correct on all 8 lanes —
      bit-exact vs offline `math.sqrt` for the irrational sqrts (2/3/1e-5) + exact squares (4/1e10/
      0.25), `sqrt(0)=0` via the +0 cmp_class passthrough, `sqrt(-1)=NaN` via the Newton path —
      `native_spirv_f64_sqrt_e2e.cyr`. The new ops are in the forced-DIVERGENT guard (51→56).
    - [x] **F.7f.7c (subnormal scaling)** *(2026-06-19)* — **HW-verified on Cezanne, FSqrt now
      full-range correctly-rounded**. Wraps the 7b core with the LLVM scale-up/rescale: `MIR_OP_F64_CONST`
      (the F.7f.9 primitive — 2× v_mov_b32 of the 2^-767 threshold) → `CMP_GT` subnormal gate →
      cndmask 2^256 scale-up → `LDEXP` → [core] → cndmask 2^-128 rescale → `LDEXP`, with `vcc_sub`
      from CMP_GT driving BOTH cndmasks across the core (only VMOV/LDEXP/FMA/MUL between — none write
      VCC) before CMP_CLASS overwrites it. 3 new ops (F64_CONST/LDEXP/CMP_GT 57–59, forced-DIVERGENT).
      `out=sqrt(a)` bit-exact vs `math.sqrt` on all 8 lanes **incl. 3 genuine subnormals** (5e-324 /
      2.5e-310 / 1.5e-308) — `native_spirv_f64_sqrt_e2e.cyr` (the macro grew 208→272 bytes).
      Adversarially reviewed (Workflow, 3 dims / 13 agents; 9 refuted): one confirmed **CRITICAL** —
      `MIR_OP_F64_CONST` passed the 64-bit threshold as a single imm in `a`, but MIR packs operands
      into 32-bit halves so it truncated to 0.0 → the subnormal gate was DEAD (the lanes passed only
      because the GFX9 core handles subnormals unscaled, masking it). Fixed: carry the f64 const as
      two 32-bit halves (a=lo, b=hi); re-verified — the subnormals now exercise ACTIVE scaling and
      still pass bit-exact. Pinned by a threshold-VALUE assert + [[reference_mir_operand_32bit_packing]].
  - **F.7f transcendentals — ARITHMETIC-ONLY** *(maintainer decision 2026-06-19)* — mabda does NOT
    provide f64 exp/ln/tanh/pow as `OpExtInst` intrinsics. **GLSL.std.450 Exp/Log/Exp2/Log2/Pow/Sin
    are 16/32-bit-only** — `spirv-val` rejects them on f64 (only Sqrt/FAbs/Floor/FMin/FMax + core
    OpFDiv are f64-valid). A conformant Vulkan consumer can't invoke an f64 exp intrinsic. So consumers
    (attn11) **hand-roll** exp/ln/tanh/pow as polynomials using mabda's f64 arithmetic; mabda compiles
    the arithmetic. A fully-built degree-13 exp macro (≤1 ULP) was **backed out** (uninvocable). The
    deliverable is the CONFORMANT TOOLKIT so consumers can hand-roll. See
    [[project_f64_transcendentals_arithmetic_only]]. (Former F.7f.10–13 exp/ln/tanh/pow intrinsics: dropped.)
    - [x] **F.7f.10 (i32↔f64 OpConvert)** *(2026-06-19)* — **HW-verified on Cezanne**: OpConvertFToS
      (f64→i32) → `CVT_I32_F64` (0x03), OpConvertSToF (i32→f64) → `CVT_F64_I32` (0x04). `out=(double)(int)a`
      bit-exact vs SPIR-V round-TOWARD-ZERO — confirming GFX9 `v_cvt_i32_f64` TRUNCATES (a design agent
      wrongly claimed round-to-nearest). `native_spirv_f64_cvt_e2e.cyr`. The conformant salvage of the
      exp work; the cvt ops + F64_CONST primitive are kept (FSqrt + consumers use them).
  - [x] **F.7f.8 (f64 compares + OpSelect)** *(2026-06-19)* — **HW-verified on Cezanne**: the public
    bool-producing path for attn11's argmax/masking. An ordered `OpFOrd*` (LT/GT/LE/GE/EQ/LG, VOPC
    0x61–0x66) → `v_cmp_*_f64`→VCC→cndmask → a **0/-1 MASK** (a normal VGPR value — robust, not VCC-
    fragile); `OpSelect` → `v_bfi_b32(mask, x, y)` per 32-bit lane (1 for i32/f32, 2 for an f64 pair),
    no VCC at select time → order-independent. New: `MIR_OP_F64_CMP` (reuses GISEL_V_CMP/_emit_vopc) +
    `MIR_OP_SELECT` + the bfi encoder/emit. `out=(a>b)?a:b` (max) bit-exact on all 8 lanes incl. the
    equal (→false) + signed-zero cases — `native_spirv_f64_select_e2e.cyr` + byte-oracle + compile test.
    Adversarially reviewed (Workflow, 2 dims / 11 agents): **6 confirmed**, all one root cause —
    OpSelect didn't ENFORCE its cond is a 0/-1 mask, so a non-OpFOrd* cond (an integer-compare bool →
    VCC/SCC with no mask reg → an unallocated VGPR; or a bool CONSTANT → an inline code) would silently
    corrupt the bfi blend. Fixed: the f64-compare mask is now typed `MIR_T_BOOL` (the marker), and
    `_spirv_lower_select` FAILS LOUD unless cond is an SSA `MIR_T_BOOL` (pinned by
    `test_spirv_lower_select_rejects_nonmask`). The non-mask cases are the tracked follow-ups.
    - [ ] **F.7f.8 follow-ups** *(deferred — not attn11-blocking)* — unordered `OpFUnord*` compares
      (the 0x69.. VOPC set, true on NaN); f32 float compares (`v_cmp_*_f32`); vector OpSelect (per-
      component); and OpSelect on a non-OpFOrd* condition (integer compares lower to VCC/SCC, not a
      0/-1 mask — needs a unified bool representation). All fail loud today, none blocking attn11's
      f64 hand-rolled kernels.
  - [x] **F.7f.9 (SPIR-V f64 OpConstant)** *(2026-06-19)* — **HW-verified on Cezanne**: f64 literals
    flow end-to-end. The const table (`spirv_build_const_table`) + `mir_set_const` now keep the full
    64-bit value (word-count wc>=5 discriminator; the MIR payload is already 64-bit); `_spirv_f64_arg`
    pre-materializes an f64-const OPERAND into a VGPR pair via `MIR_OP_F64_CONST` (VOP3 has no 64-bit
    literal), applied at the scalar binop (FADD/FSUB/FMUL), OpExtInst x0/x1/x2 (FMA/min/max/sqrt/abs/
    floor), and OpFDiv (a/b); the 32-bit const-fold is skipped for f64 (`_spirv_both_const` returns 0).
    `out=0.5a²+1.5a+2.5` via two FMAs with f64 const coeffs **bit-exact** vs the fused reference —
    `native_spirv_f64_const_e2e.cyr` + the `_spirv_f64_arg` materialization unit test. The hand-rolled-
    polynomial enabler. Adversarially reviewed (Workflow, 2 dims / 10 agents): **7 confirmed** — and
    the vec-binop/store/vec-store gaps were SILENT-WRONG (not fail-loud as first assumed: the emit
    truncated an f64 literal to its low dword → 0.0 for 0.5/1.0/1.5/…). Fixed: (a) a fail-loud BACKSTOP
    in `_cmp_resolve` (an f64-const operand reaching the emit unmaterialized now errors, never
    truncates); (b) `_spirv_f64_arg` applied at the vec binop, OpStore, vec store, and OpFDiv operand
    sites; (c) type-gated the wc≥5 hi-word pack (only f64; a malformed f64 wc<5 fails loud) + reverted
    the redundant untyped pack in `spirv_build_const_table` (it serves only i32 access-chain indices).
    Pinned by `test_spirv_f64_arg` + a store-const compile test.
  - [x] **F.7f.Ldexp** *(2026-06-19)* — **HW-verified on Cezanne**: GLSL.std.450 Ldexp IS f64-valid
    (general-float, like Sqrt — spirv-val confirms). Wired `OpExtInst Ldexp` (f64) → `MIR_OP_F64_LDEXP`
    (the encoder + MIR op + emit already existed from F.7f.7a/c). `out=ldexp(a,3)=a·8` bit-exact on all
    8 lanes — `native_spirv_f64_ldexp_e2e.cyr` + a compile test. Consumers get 2^k for hand-rolled exp.
    (f32 Ldexp unwired — no native consumer; wgpu covers f32.)
  - [x] **F.7-flip** *(2026-06-19)* — **`MABDA_NATIVE_F64 = 1`**. The conformant op set attn11's
    hand-rolled f64 kernels need is HW-verified on Cezanne: {add,sub,mul,div,sqrt,fma,cmp,select,
    cvt(i32↔f64 + f32↔f64),const,ldexp}. The `_backend_native_shader_module_create` f64 gate now opens
    and `gpu_caps_native_shader_f64()` honestly returns 1. (Tests at `core.tcyr` updated to the post-
    flip state.) Consumers hand-roll exp/ln/tanh/pow from the toolkit (GLSL transcendentals are f32-only).
- [x] **F.9** *(2026-06-19)* — **attn11 consumer smoke via the PUBLIC API, HW-verified on Cezanne**:
  `out[i] = (x[i] - mean[i]) / sqrt(var[i] + 1e-5)` (a real layernorm-normalize step — FSub + FAdd +
  f64 OpConstant + OpExtInst Sqrt + OpFDiv) compiled + dispatched entirely through
  `gpu_shader_module_create_spirv` → `gpu_compute_dispatch` (not internal gfx9_compile), **bit-exact**
  on all 8 lanes vs the step-by-step in-process f64 reference — `native_spirv_f64_layernorm_e2e.cyr`.
  Proves the f64 toolkit works end-to-end through the public surface as a consumer uses it.
- [ ] **F.10** *(3.2.13)* — Docs (throughput caveats) + **roadmap reflow
  correcting the `SHADER_F64` framing**; cut.

---

## Phase R — Render-graph multi-queue scheduling → 3.2.12–3.2.13

Proposal: [`v3.2-render-graph-multiqueue.md`](../proposals/v3.2-render-graph-multiqueue.md).
Absorbs the v3.1.2 carryover (was "Phase Q.7"). The v2.5 render graph is
single-submit; this adds per-node queue affinity + cross-queue fence edges
+ per-queue submit batching, composing the v3.1.1 queue primitives (no new
Backend slots). Multi-queue is opt-in; omitting affinity reproduces v2.5
ordering exactly. wgpu serialized-equivalent.

- [ ] **R.1** *(3.2.12)* — `rg_node_queue` affinity (Node 72→80,
  append-only) + `rg_execute_mq` entry; default reproduces v2.5.
- [ ] **R.2** *(3.2.12)* — Cross-queue edge classification (same-queue =
  order; cross-queue = fence edge); build-time cycle check covers it.
- [ ] **R.3** *(3.2.12)* — Per-queue submit batching (group a queue's
  nodes; `RenderGraph` 40→48 schedule ptr).
- [ ] **R.4** *(3.2.12)* — `native_render_dispatch_timeline` (GFX-ring
  timeline variant of the render submit; the missing piece for render
  nodes to join cross-ring overlap).
- [ ] **R.5** *(3.2.13)* — Per-node IB staging (fixes the shared-cached-IB
  clobber under genuine overlap; explicit, never a silent serialization).
- [ ] **R.6** *(3.2.13)* — Scheduler (per-queue toposort + cross-queue
  barrier insertion + batch submit); pure-CPU graph-math tests.
- [ ] **R.7** *(3.2.13)* — HW e2e (`native_render_graph_mq_e2e.cyr`:
  compute→cross-queue-edge→render, distinct rings, no CPU stall; wgpu
  parity vs `render_graph_e2e.cyr`); cut.

---

## Hardware-verification gaps (flagged for the maintainer — code ships in-arc, NOT deferred)

These are the only items blocked by hardware absent from the Cezanne dev
box. **The code paths are authored and ship within 3.2.x**; only the
HW *verification* (and the corresponding capability bit) waits on silicon.
Surfaced here for your call — not silently dropped:

1. **ASTC / ETC2 native HW decode (Phase TS.8).** BC1/BC7 decode on Cezanne
   and are HW-verified (TS.7). ASTC + ETC2 are mobile families that may not
   decode on this desktop-class APU. The TS.8 code (IMG format rows,
   descriptors, FS path) ships; the native ASTC/ETC2 cap bits stay 0 until
   verified on ASTC/ETC2-capable AMD silicon. **Decision for you:** accept
   "code shipped, native cap bit dormant until HW available" vs. source
   ASTC/ETC2-capable hardware.
2. **Full-rate f64 (`V_MFMA_F64`, Phase F).** Correctness f64
   (`V_ADD/MUL/FMA_F64`) works on Cezanne (~1:16–1:32 rate) and is the
   deliverable. The full-rate matrix path (`V_MFMA_F64`) needs CDNA/Instinct
   silicon — none on the dev box. The correctness path ships in-arc; the
   MFMA full-rate path is documented and stays HW-unverifiable here.
   **Decision for you:** accept documented-but-unverified MFMA vs. source
   CDNA hardware (or leave MFMA as an explicit, tracked v4/native-expansion
   verification item — your call, not mine).

## Tier 2 — Integration & regression (per cut)

- [ ] Six-consumer regression sweep — soorat, rasa, ranga, bijli,
  aethersafta, kiran build + run against each new bundle. kiran/aethersafta
  validate compressed textures (T + TS); attn11 is the f64 acceptance
  consumer (F.9).
- [ ] Bench baseline per cut; f64 perf documented with the throughput
  caveats (never presented as f32-comparable).
