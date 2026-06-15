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
| 3.2.8   | **N** (4) | First novel kernel / SAXPY, control flow (N.6–7). | — |
| 3.2.9   | **N** (5) | Op breadth + vectors + generic dispatcher (N.8). Native SPIR-V **f32 compiler complete**. | — |
| 3.2.10  | **F** (1) | f64 caps surface + native hand-authored f64 proof + wgpu f64 module (F.1–6, F.8). | — |
| 3.2.11  | **F** (2) | General native f64 via the Phase N emitter + attn11 acceptance (F.7, F.9–10). f64 **complete**. | — |
| 3.2.12  | **R** (1) | Render-graph multi-queue: node queue affinity, cross-queue edges→barriers, native render timeline dispatch (R.1–4). | — |
| 3.2.13  | **R** (2) | Per-node IB staging, scheduler, e2e (R.5–7). Render-graph multi-queue **complete**. | — |

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

- [ ] **X.1** — `NativeBuf` struct + real `_backend_native_buffer_create`/
  `_release` (page-align, VA sub-alloc, GEM/VA map). Retires the stubs.
- [ ] **X.2** — real native `buffer_write`/`_read` (memcpy + `off+n<=size`
  guard). Native `gpu_buffer_*` now real on both backends.
- [ ] **X.3** — DMA-ring flip (`_native_queue_kind_to_ring(TRANSFER)`→
  `AMDGPU_HW_IP_DMA`) + compute-on-DMA-queue mis-route guard +
  `GPU_ERR_TRANSFER=19`.
- [ ] **X.4** — `native_transfer_copy_timeline` driver (SDMA into cached
  IB; BO_HANDLES fence/ib/src/dst; consume pending-wait → in-CS
  TIMELINE_WAIT; signal timeline).
- [ ] **X.5** — `BACKEND_SLOT_BUFFER_COPY` (`BACKEND_SIZE`→264) + public
  `gpu_buffer_copy(ctx,src,dst,size)` + `gpu_queue_transfer_copy(...)`;
  extend completeness walk.
- [ ] **X.6** — wgpu fill (`copy_buffer_to_buffer` via the existing shim +
  submit); install both backends; document COPY_SRC/DST.
- [ ] **X.7** — HW e2e (`native_transfer_copy_e2e.cyr`: compute-produce →
  barrier → transfer-copy-consume; wgpu serialized verify).
- [ ] **X.8** — **Cut 3.2.1.**

---

## Phase TS — Native compressed (and general) texture sampling → 3.2.2–3.2.3

Proposal: [`v3.2-native-compressed-sampling.md`](../proposals/v3.2-native-compressed-sampling.md).
**Re-introduces the GFX9 T#/S#/tiling/`image_sample` path the v3.1 mipmap
pivot deleted** — so native AMD can *sample* textures, not just
store/readback. Two sub-features: uncompressed (linear + T#/S#) and
compressed (tiled + swizzle + T#/S#); the tiling swizzle is the
high-risk half, sequenced last. **This is the work I wrongly punted to
"v4-scale"; it is in the arc.**

- [ ] **TS.1** — GFX9 T# image-descriptor builder (256-bit) +
  format→IMG-format table; every dword cited vs Mesa gfx9.json / radv.
- [ ] **TS.2** — GFX9 S# sampler-descriptor builder (128-bit) +
  `gpu_sampler_create` + wgpu `WGPUSampler` filler (point/clamp first).
- [ ] **TS.3** — Struct + slot growth (`NativeTexture` 48→64; slots
  `CREATE_2D_SAMPLEABLE` + `BIND_FOR_SAMPLE`; `BACKEND_SIZE`→280; extend walk).
- [ ] **TS.4** — Textured FS + UV-export VS in hand-authored GFX9 ISA
  (`image_load` oracle then `image_sample`; SPI input wiring); llvm-mc
  byte-pinned.
- [ ] **TS.5** — **Uncompressed RGBA8 sampling MVP on Cezanne**
  (`native_texture_sample_e2e.cyr`) — proves T#/S#/`image_sample` with zero
  tiling risk.
- [ ] **TS.6** — Tile-swizzle transform (SW_64KB_S) — pure-Cyrius per-block
  remap; round-trip-identity + addrlib-reference CPU tests.
- [ ] **TS.7** — **BC1/BC7 compressed sampling on Cezanne**
  (`native_compressed_sample_e2e.cyr`, CPU-decode verify); flip native BC
  cap bit to 1.
- [ ] **TS.8** — Bilinear; ETC2/ASTC path authored (cap bit flipped IFF
  HW decode verifies — see HW gaps); **strike Phase T's storage-only
  limitation**; cut the TS minors.

---

## Phase S — SPIR-V shader ingestion (wgpu) → 3.2.4

Proposal: [`v3.2-spirv-ingestion-wgpu.md`](../proposals/v3.2-spirv-ingestion-wgpu.md).
Adds an explicit shader **source-kind tag** (WGSL/SPIR-V/GFX9-ISA) to the
byte-polymorphic boundary. Native SPIR-V is Phase N (fail-loud here).

- [ ] **S.1** — Constants + `WGPUShaderSourceSPIRV` builder (codeSize in
  **words**) + structural validation (magic/align/bound).
- [ ] **S.2** — Widen shader-create slot `(…,kind)` (fncall3→4); wgpu
  branches on kind; `gpu_shader_module_create_spirv`.
- [ ] **S.3** — Source-kind-aware shader cache (`_shader_hash_n`, kind in
  the seed; WGSL/SPIR-V non-collision).
- [ ] **S.4** — Reference launcher gate (`ShaderSourceSPIRV` instance
  feature in `deps/wgpu_main.c`); consumer migration note.
- [ ] **S.5** — wgpu HW e2e (`spirv_e2e.cyr`, render path — wgpu compute
  is still a v3.0 stub; cross-source identity vs WGSL).
- [ ] **S.6** — **Cut 3.2.4.**

---

## Phase N — Native SPIR-V → GFX9 ISA compiler → 3.2.5–3.2.9

Proposal: [`v3.2-spirv-gfx9-native-lowering.md`](../proposals/v3.2-spirv-gfx9-native-lowering.md).
**The dominant, highest-risk effort** — an in-tree, compute-only,
GFX9/Cezanne-only SPIR-V→GFX9 compiler behind the existing shader slot (no
new slot). Own AGNOS package. A compiler emits bytes no human checked for
inputs no human saw — the entire mitigation is *manufactured oracles*. The
hand-authored shaders stay as oracle + fallback.

- [ ] **N.0** *(3.2.5)* — Oracle harness + encoder lift (`src/gfx9_encode.cyr`);
  every existing hand-authored dword re-encodes byte-identically.
- [ ] **N.1** *(3.2.5)* — SPIR-V parser (`src/spirv_parse.cyr`); untrusted-input
  rejection tests.
- [ ] **N.2** *(3.2.6)* — MIR + uniformity lowering (`src/mir.cyr`,
  `src/spirv_lower.cyr`).
- [ ] **N.3** *(3.2.6)* — Instruction selection, straight-line f32/i32/u32
  (`src/gfx9_isel.cyr`).
- [ ] **N.4** *(3.2.7)* — Register allocation + `s_waitcnt` (no spill,
  fail-loud over cap; ABI-reservation first).
- [ ] **N.5** *(3.2.7)* — **MVP: encode + ABI wiring + recompile the
  downsample shader from SPIR-V and byte-match it** on Cezanne. MVP exit.
- [ ] **N.6** *(3.2.8)* — First novel kernel (SAXPY); differential CPU oracle;
  generic dispatcher.
- [ ] **N.7** *(3.2.8)* — Control flow (uniform `s_cbranch`; divergent
  EXEC-mask); divergent-vs-uniform test matrix. May split into two bites.
- [ ] **N.8** *(3.2.9)* — Op breadth + vectors + dispatcher polish. Native
  SPIR-V f32 compiler **complete**.

---

## Phase F — f64 (double-precision) compute → 3.2.10–3.2.11

Proposal: [`v3.2-f64-compute.md`](../proposals/v3.2-f64-compute.md).
Consumer: **attn11**. f64 hard-gates on SPIR-V on **both** backends (no
`ShaderF64` feature; WGSL has no f64). Zero new Backend slots. Correctness,
not speed, is the deliverable. **No escape hatch — general native f64 is a
committed 3.2.x minor (3.2.11), gated on Phase N which is also in-arc.**

- [ ] **F.1** *(3.2.10)* — `GpuCapabilities` f64 field (+120, struct→128) +
  accessors.
- [ ] **F.2** *(3.2.10)* — `gpu_caps_shader_f64(ctx)` (wgpu: probe-module +
  `SpirvShaderPassthrough`; native: `MABDA_NATIVE_F64`).
- [ ] **F.3** *(3.2.10)* — Shader-module f64 flag + fail-loud dispatch guard.
- [ ] **F.4** *(3.2.10)* — `native_gfx9_shader_fma_f64` hand-authored
  reference kernel (`V_FMA_F64`, doubled-VGPR RSRC1); llvm-mc byte-pinned.
- [ ] **F.5** *(3.2.10)* — `native_pm4_build_compute_fma_f64` composer.
- [ ] **F.6** *(3.2.10)* — `native_f64_fma_e2e.cyr` (Cezanne): a*b+c
  **bit-exact vs CPU f64**. f64-on-HW proof, decoupled from Phase N.
- [ ] **F.8** *(3.2.10)* — wgpu f64 module via SPIR-V passthrough + probe
  (`f64_compute_e2e.cyr`; dispatch gated on real wgpu compute → M.6b posture).
- [ ] **F.7** *(3.2.11)* — General native f64: route consumer f64 SPIR-V
  through the Phase N emitter (`V_*_F64`); per-op f64 conformance.
- [ ] **F.9** *(3.2.11)* — attn11 consumer smoke (one real f64 kernel,
  bit-faithful vs the CPU f64 oracle). Acceptance proof.
- [ ] **F.10** *(3.2.11)* — Docs (throughput caveats) + **roadmap reflow
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
