# Mabda v3.3 — Release Punch List

**Status:** Planned — `33x` branch cut 2026-06-19 (toolchain bumped to
Cyrius 6.2.23). No implementation started; this doc + the two design
proposals are the plan. Tick items as they land.
**Date opened:** 2026-06-19
**Branch:** `33x` (cut from `main` after the 3.2.14 cleanup).
**Roadmap reference:** [`roadmap.md` § v3.3](roadmap.md#v33--asset-loading)
**Design proposals:**
[`v3.3-asset-loading.md`](../proposals/v3.3-asset-loading.md) ·
[`v3.3-chitra-png-decoder-package.md`](../proposals/v3.3-chitra-png-decoder-package.md)

> **v3.3 — "Asset Loading."** mabda can create + sample textures but cannot
> yet *load* one from a file/byte-buffer. v3.3 closes that with three loaders
> behind the established decode-in-package / upload-in-mabda split:
> **KTX2 + DDS** are parsed in mabda and their GPU-ready blocks placed per
> mip level (no external decoder — leverages the 3.2.x BC/compressed stack);
> **PNG** is CPU-decoded to RGBA8 by a **new pure-Cyrius sibling package,
> `chitra`**, which mabda deps. The original roadmap framing ("blocked, no
> pure-Cyrius decoder exists") was **wrong** — the stdlib `sankoch` already
> ships zlib/DEFLATE inflate, and `kii` already has a fuzz-hardened PNG
> decoder. `chitra` is therefore a **fork + adaptation** of kii's proven PNG
> code (byte-buffer I/O + a canonical-RGBA8 output pass), not a greenfield
> build. JPEG and KTX2 supercompression stay decoder-gated (tracked, §Gates).

## Version map (smallest-risk-first; back-half minor numbers nominal/fluid, order fixed)

**[M]** = mabda repo · **[C]** = the new `chitra` package repo.

| Minor   | Phase | Repo | Lands | `BACKEND_SIZE` |
|---------|-------|------|-------|----------------|
| 3.3.0   | **AL.0** | M | Format-mapping tables (`asset_format.cyr`: VkFormat + DXGI_FORMAT → `MABDA_TEXFMT_*`) + caps-on-ctx access. Pure CPU, no slots. | 280 |
| 3.3.1   | **AL.1** | M | Per-mip-level upload — `gpu_texture_write_level` + `BACKEND_SLOT_TEXTURE_WRITE_LEVEL`. | 280→288 |
| 3.3.2   | **AL.2** | M | Generalized mipped create — `gpu_texture_create_2d_fmt_mipped` + `BACKEND_SLOT_TEXTURE_CREATE_2D_FMT_MIPPED` (any fmt, N levels). | 288→296 |
| 3.3.3   | **AL.3** | M | DDS loader — `gpu_texture_load_dds` (FourCC + DX10/DXGI), per-level walk, caps gate, array/cube parsed-and-rejected-loud. | 296 |
| 3.3.4   | **AL.4** | M | KTX2-uncompressed loader — `gpu_texture_load_ktx2`; `supercompressionScheme != 0` fails loud (decoder gate); arrays/faces rejected-loud. | 296 |
| 3.3.5   | **AL.P0** | C | **chitra 0.1.0** — fork kii's PNG framing/unfilter/security into a standalone `sankoch`-backed package + **new** byte-buffer API + **new** canonical-RGBA8 (incl. palette/gray expansion + tRNS) pass. | — |
| 3.3.6   | **AL.5** | M | PNG integration — `[deps.chitra] tag="0.1.0"`, `asset_load.cyr`: `gpu_texture_load_png` → chitra decode → RGBA8 upload. | 296 |
| 3.3.7   | **AL.6** | M | Magic-byte sniffer `gpu_texture_load` + HW-gated e2e (png/ktx2/dds on Cezanne) + closeout + cut. | 296 |

**Schedule note:** AL.0–AL.4 [M] and AL.P0 [C] are **independent** and should
run in parallel; they converge only at AL.5. The one hard cross-repo edge:
**chitra 0.1.0 must be cut + tagged before AL.5 can wire the dep.**

## Verified facts (load-bearing for the bites)

- `BACKEND_SIZE = 280`; highest slot `TEXTURE_BIND_FOR_SAMPLE = 272`. New
  slots: `TEXTURE_WRITE_LEVEL = 280` (→288), `TEXTURE_CREATE_2D_FMT_MIPPED =
  288` (→296). **Two** new slots — both carry the FFI-descriptor audit review.
- `GPU_ERR` max = `GPU_ERR_TRANSFER = 20`. New: `GPU_ERR_CONTAINER_PARSE = 21`,
  `GPU_ERR_IMAGE_DECODE = 22` (verify "10 free-for-reuse" claim at impl; prefer
  fresh codes).
- Inflate is **free**: stdlib `sankoch` ships `zlib_decompress(src, src_len,
  dst, dst_cap)` + crc32/adler32 (chitra deps it via `[deps].stdlib`, NOT a git
  dep). kii uses the same for IDAT.
- kii is **v1.0.1**, a **terminal image→ASCII viewer**; its `png.cyr` is
  **path-based**, emits **native channels / palette indices** (no RGBA8
  normalization, no tRNS). chitra is a **one-time fork** + real new code.

---

## Phase AL.0 — Format tables + caps access (3.3.0) [M]

- [ ] **AL.0a** — `src/asset_format.cyr` scaffold (pure, sibling of
  `texture_format.cyr`); add to `src/lib.cyr` after `texture_format.cyr` **and**
  to `cyrius.cyml [lib].modules` (the distlib-manifest gotcha). Smoke links.
- [ ] **AL.0b** — `mabda_texfmt_from_vk_format(vk)` — **pin every VkFormat
  number against the canonical Vulkan `VkFormat` enum** at impl (cite in the
  header, gfx9.json-style discipline). N:1 collapse is real (BC1_RGB 131 /
  BC1_RGBA 133 / their sRGB peers → one `MABDA_TEXFMT_BC1`); see the **sRGB
  policy** decision below. Unmapped → 0. Test each + unknown.
- [ ] **AL.0c** — `mabda_texfmt_from_dxgi_format(dxgi)` — pin against the D3D
  `DXGI_FORMAT` enum. Unmapped → 0. Test each + unknown.
- [ ] **AL.0d** — **sRGB policy** (the #1 silent-wrong risk): mabda has no
  `_SRGB` format ids. Decide per impl: (a) map sRGB sources to the UNORM id +
  record an `is_srgb` flag the loader surfaces, or (b) add `MABDA_TEXFMT_*_SRGB`
  ids. Do NOT silently load sRGB data as UNORM. Test the chosen policy.
- [ ] **AL.0e** — caps-on-ctx: stash the `GpuCapabilities` on `GpuContext` at
  create (or add `gpu_ctx_supports_format(ctx, fmt)` resolving caps internally),
  so the loaders can run the ETC2/ASTC/BC6H gate. **Prerequisite for AL.3/AL.4.**
  Test the accessor + a format-supported query.

## Phase AL.1 — Per-mip-level upload (3.3.1) [M]

- [ ] **AL.1a** — `backend.cyr`: `BACKEND_SLOT_TEXTURE_WRITE_LEVEL = 280`,
  `BACKEND_SIZE = 288`, completeness-range update. Slot-completeness test.
- [ ] **AL.1b** — `texture.cyr`: `gpu_texture_write_level(ctx, tex, level, src,
  n)` — validate `level < mip_count`; expected `n = mabda_texfmt_data_size(fmt,
  mip_dim(w,level), mip_dim(h,level))`; reject mismatch; route through slot.
  Tests: level-OOB / size-mismatch reject; routing sentinel.
- [ ] **AL.1c** — native filler — per-level VA offset (tiled → `_native_texture_
  tiled_copy`; linear → memcpy at `va_base + mip_offset`). Mock-VA test for
  levels 0/1/N.
- [ ] **AL.1d** — wgpu filler — `wgpu_queue_write_texture` with `mipLevel` +
  block-correct `bytesPerRow`. **Handle the 256-byte `bytesPerRow` alignment**
  (X2) — staging re-pack for non-aligned widths. Descriptor-shape test.
- [ ] **AL.1e** *(if e2e needs per-level verify)* — `gpu_texture_read_level`
  peer (else AL.6 can't read back individual levels; decide scope — may add a
  third slot, BACKEND_SIZE→304).

## Phase AL.2 — Generalized mipped create (3.3.2) [M]

- [ ] **AL.2a** — `backend.cyr`: `BACKEND_SLOT_TEXTURE_CREATE_2D_FMT_MIPPED =
  288`, `BACKEND_SIZE = 296` (a fixed-arity fnptr slot can't grow an arg — it's
  a **new** slot, not an extension of `CREATE_2D_FMT`). Completeness test.
- [ ] **AL.2b** — `texture.cyr`: `gpu_texture_create_2d_fmt_mipped(ctx, w, h,
  fmt, mip_count)` — validate mip_count (0 → full chain; > full chain → reject);
  validate fmt. Tests: validation + full-chain computation.
- [ ] **AL.2c** — native: compressed mip-chain BO sizing (per-level block-
  rounded + 256-aligned). Test total BO size (e.g. BC1 256×256 chain).
- [ ] **AL.2d** — wgpu: `mipLevelCount` on the compressed-fmt texture
  descriptor. Descriptor-shape test.

## Phase AL.3 — DDS loader (3.3.3) [M]

- [ ] **AL.3a** — `error.cyr`: `GPU_ERR_CONTAINER_PARSE = 21` (+ string). Round-trip test.
- [ ] **AL.3b** — `src/asset_load.cyr` scaffold + lib.cyr + manifest. Smoke link.
- [ ] **AL.3c** — `_dds_parse_header` — magic `0x20534444`, 124-B header, 32-B
  pixelformat; **bounds-check every read vs `len`**. Tests: hand-built blob →
  w/h/mips; truncated → reject; bad magic → reject.
- [ ] **AL.3d** — FourCC path: DXT1→BC1, DXT5→BC3; **DXT3/BC2 → fail-loud** (no
  mabda id). Test each + DXT3 reject.
- [ ] **AL.3e** — DX10 path: 20-B DXT10 header iff FourCC=='DX10' → DXGI map;
  read `arraySize` + cubemap misc bit → **reject array>1 / cube** (`FORMAT_
  UNSUPPORTED`). Tests: DX10 BC7 ok; array=2 reject; cube reject.
- [ ] **AL.3f** — legacy `dwCaps2 & DDSCAPS2_CUBEMAP` → reject. Test.
- [ ] **AL.3g** — per-level walk via `mabda_texfmt_data_size`; **caps gate
  before create** (AL.0e); `create_2d_fmt_mipped` + `write_level` loop. Tests:
  offset math; ASTC-on-BC-only-caps → `FORMAT_UNSUPPORTED`.
- [ ] **AL.3h** — `gpu_texture_load_dds` + `_result` (0/Err wrappers). Full
  mock-backend parse→upload-routing test.

## Phase AL.4 — KTX2-uncompressed loader (3.3.4) [M]

- [ ] **AL.4a** — `_ktx2_parse_header` — 12-B identifier + header fields, bounds-
  checked. Tests: blob → fields; truncated/bad-magic reject.
- [ ] **AL.4b** — `supercompressionScheme != 0` → **fail-loud** `FORMAT_
  UNSUPPORTED` with detail (the BasisLZ/Zstd/ZLIB decoder gate). Test scheme=2.
- [ ] **AL.4c** — `faceCount != 1` / `layerCount > 1` → reject (v3.4). Tests.
- [ ] **AL.4d** — level-index parse (levelCount × 24-B); scheme==0 ⇒ compressed
  len == uncompressed len. Tests: parse + mismatch reject.
- [ ] **AL.4e** — `mabda_texfmt_from_vk_format` + caps gate + `create_2d_fmt_
  mipped` + `write_level` loop. Tests: vkFormat→fmt; upload routing; unmapped reject.
- [ ] **AL.4f** — `gpu_texture_load_ktx2` + `_result`. Full mock parse→upload test.

## Phase AL.P0 — chitra 0.1.0 (3.3.5) [C — new repo]

- [ ] **AL.P0a** — **Create the `chitra` git repo + remote** (nothing to tag
  against otherwise). Scaffold: `cyrius.cyml` (name=chitra, pin 6.2.23,
  `[deps].stdlib = [..., "sankoch", "thread"]`), VERSION 0.1.0, `src/lib.cyr`,
  `programs/smoke.cyr`, CI + scripts (copy mabda's). Smoke links.
- [ ] **AL.P0b** — **Fork** kii's `png.cyr` framing/chunk-walk/unfilter +
  security hardening (decompression-bomb caps, lying-IHDR guards, CRC checks).
  One-time fork (no live kii dep; kii-bugfix backport is manual/out-of-scope).
  Split per the 128 KiB lint cap if needed. Port kii's asserts.
- [ ] **AL.P0c** — **New byte-buffer I/O**: replace kii's path-based reads with
  an in-memory `(src, len)` cursor. `ChitraImage` struct (w/h/pixels/channels)
  + accessors + `chitra_image_free`. Tests.
- [ ] **AL.P0d** — **New canonical-RGBA8 pass** (the real new work, NOT in kii):
  gray(0)→RGBA, RGB(2)→RGBA, palette(3)→RGBA via PLTE, gray+α(4)/RGBA(6)
  passthrough; **tRNS** alpha synthesis for 0/2/3 (X1); depth-8 first (1/2/4/16
  staged). Output `w*h*4`. Tests: one embedded PNG **byte array** per color type
  → pixel-exact (fixtures embedded as byte arrays, NOT paths — X5).
- [ ] **AL.P0e** — `chitra_png_decode(src,len) → Ok(ChitraImage*)|Err` +
  `_rgba8(src,len,wp,hp) → ptr|0`; `ChitraErr` (GpuErr-compatible 16-B). Carry
  kii-fuzz adversarial corpus (truncated/lying/CRC-fail/bomb → reject, no OOB).
- [ ] **AL.P0f** — `dist/chitra.cyr` distlib + version-check + CHANGELOG + **cut
  + tag 0.1.0** (the cross-repo gate for AL.5).

## Phase AL.5 — PNG integration (3.3.6) [M]

- [ ] **AL.5a** — `cyrius.cyml`: `[deps.chitra] git=… tag="0.1.0"
  modules=["dist/chitra.cyr"]`; `cyrius deps` resolves `lib/chitra.cyr`. Smoke.
- [ ] **AL.5b** — `src/lib.cyr`: `include "lib/chitra.cyr"`. Include-order link.
- [ ] **AL.5c** — `asset_load.cyr`: `gpu_texture_load_png(ctx, bytes, len)` →
  `chitra_png_decode` → `create_2d(RGBA8)` + `write` (w*h*4) → `chitra_image_
  free`; decode-fail → `GPU_ERR_IMAGE_DECODE = 22`. Mock-backend byte-count test.
- [ ] **AL.5d** — `gpu_texture_load_png_mipped` → mipped RGBA8 create + write
  level 0 + `generate_mipmaps`; **surface wgpu's `GPU_ERR_NOT_IMPLEMENTED`
  honestly** (no silent single-level fallback). Tests.
- [ ] **AL.5e** — `_result` variants. Ok/Err discrimination test.

## Phase AL.6 — Sniffer + e2e + closeout (3.3.7) [M]

- [ ] **AL.6a** — `gpu_texture_load(ctx, bytes, len)` magic-byte dispatch
  (`89 50 4E 47`→png, `0xAB KTX`→ktx2, `DDS `→dds; else `CONTAINER_PARSE`). Tests.
- [ ] **AL.6b** — HW-gated programs (Cezanne): `native_load_png_e2e` (PNG →
  native RGBA8 → sample → pixel-verify), `load_ktx2_e2e` (BC7), `load_dds_e2e`.
  Flagged HW-gated, shipped.
- [ ] **AL.6c** — Closeout: `make test` (new `tests/tcyr/asset_load.tcyr`
  asserts; count via `scripts/count-test-assertions.sh` — the texture.tcyr NUL
  trap), distlib diff-clean (mabda + the chitra dep), version-check, CHANGELOG,
  roadmap, audit index. Cut 3.3.7.

---

## Gates & risks (nothing silently dropped)

1. **KTX2 supercompression (BasisLZ / Zstandard / ZLIB) — GATED.** Needs a
   pure-Cyrius Zstd/Basis decoder that does not exist; `supercompressionScheme
   != 0` fails loud (AL.4b). *Possible cheap follow-up:* the ZLIB scheme could
   ride `sankoch` — a candidate bite if a consumer needs it. Tracked, not dropped.
2. **JPEG — DEFERRED + TRACKED** (not in v3.3). chitra v0.2+ / a later arc. The
   "never silently defer" pointer lives in `v3.3-chitra-png-decoder-package.md`.
3. **Array layers / cubemaps — parsed-and-rejected-loud in v3.3; real support is
   v3.4 (Phase AL-ARRAY).** The parsers read `layerCount`/`faceCount`/`arraySize`
   so they reject precisely rather than mis-upload. Tracked in the asset-loading
   proposal + the roadmap.
4. **ETC2 / ASTC sampling on native AMD — HW-BLOCKED on Cezanne** (gfx90c, final).
   The AL.0e caps gate must run **before** create so an ETC2/ASTC container fails
   loud (`FORMAT_UNSUPPORTED`), not a black sample. wgpu handles all three.
5. **BC6H needs an HDR render target** (carried from 3.2.3) — flag in the table.
6. **sRGB color-space collapse** (AL.0d) — the highest silent-wrong risk; mabda
   has no `_SRGB` ids. Decide the policy in AL.0; never load sRGB as UNORM silently.
7. **wgpu 256-byte `bytesPerRow` alignment** (AL.1d) — tightly-packed decoded
   data needs a staging re-pack for non-aligned widths.
8. **chitra cross-repo cut-before-wire edge** — chitra 0.1.0 must be tagged
   before AL.5; chitra is a one-time fork of kii (provenance drift is accepted,
   no live dep).
9. **AGNOS-target clone-mutex block** — `sankoch`'s `thread` dep (`mutex_lock`
   → `CLONE_VM`) does not link on `CYRIUS_TARGET_AGNOS` today (kii hit this).
   chitra inherits it → **PNG decode is host-only until the toolchain fix lands**.
   KTX2/DDS (no sankoch) are unaffected. Tracked.
10. **Format numbers must be pinned** (AL.0b/c) — the source designs disagreed on
    VkFormat ids; pin against the canonical Vulkan/DXGI enums, cite in the header.
    A wrong number is a silent-wrong load.

## Tier 2 — Integration & regression (per cut)

- [ ] Consumer check — soorat / rasa / ranga / bijli / aethersafta / kiran build
  against the new bundle; the natural first adopter is a texture-heavy consumer
  (soorat / kiran asset pipeline).
- [ ] `kii` could later re-base its PNG path onto `chitra` (dedupe) — a separate,
  optional follow-up, not a v3.3 gate.
