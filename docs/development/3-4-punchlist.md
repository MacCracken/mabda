# v3.4 Punchlist — Phase AA (AL-ARRAY): 2D array textures + cubemaps

**Goal:** turn the array/cubemap paths the v3.3 DDS/KTX2 loaders already
*parse-and-reject* into working **create + upload + sample + load**, on both
backends, native HW-verified on Cezanne — all **additive** (no existing
consumer call signature changes; the v3.0 load-bearing invariant).

**Branch:** `34x`. **Toolchain:** 6.2.26. **Version plan:** one **3.4.0** cut for
the whole arc (the 3.3.0 precedent), staged to 3.4.1 only if cube sampling
(AA.5b) proves HW-unreliable on Cezanne. Each bite adversarially reviewed
(Workflow) before merge, as in 3.2.x/3.3.x.

Planned 2026-06-19 from the `v34-al-array-plan` design workflow (4 research
dimensions + synthesis). `[M]` = mabda. Native field positions cited from Mesa
`gfx9.json` (the authoritative GFX9 source) — every byte re-verified at
implementation, not just planning.

---

## Key technical deltas (from the research)

- **Native T# (`native_gfx9_image_descriptor`, backend_native_shaders.cyr):** only
  a few `SQ_IMG_RSRC` fields change — `WORD3 TYPE[28:31]` 2D=9 → **2D_ARRAY=13 /
  CUBE=11**; `WORD4 DEPTH[0:12]` = **slice_count − 1** (cube = 6·ncube − 1);
  `WORD5 BASE_ARRAY[0:16]` stays 0 for a whole-array bind. Everything else
  (WIDTH/HEIGHT/format/DST_SEL/SW_MODE/LAST_LEVEL) is unchanged.
- **Native surface:** an N-slice array is **one BO** (slice 0 at base, slice k at
  `base + k·slice_pitch`), so residency/bind/USER_DATA are unchanged. LINEAR
  slice_pitch = per-slice surface size; **tiled slice stride is an addrlib gate
  (AA.4a)**.
- **Native sampling FS:** both existing FS hardcode `MIMG DA=0` + 2 VADDR. A
  2D-array needs **`DA=1` (w0 |= 1<<14) + a 3rd VADDR = slice index**; the layer
  reaches the FS via the per-draw descriptor tail (extend `scale@+48/+52` with
  **`layer@+56`**, s-loaded). **Cube needs a brand-new FS** (direction → face/u/v,
  `DA=1` float s/t/r) — the biggest Cezanne risk.
- **NativeTexture struct:** stash `slice_count` (layers / 6 faces) in the unused
  **`+60` pad** → struct stays 64 B (`NATIVE_TEX_STRUCT_SIZE` unchanged).
- **wgpu:** pure descriptor/view parameterization — `depthOrArrayLayers` =
  layers, view `dimension` = **2DArray=3 / Cube=4** (webgpu.h values, NOT the JS
  ordering), `arrayLayerCount`/`baseArrayLayer`; per-layer write via
  `writeTexture origin.z`; new WGSL `texture_2d_array` / `texture_cube` FS. Zero
  new FFI.
- **Containers:** KTX2 is **mip-outer, layer-inner** (one level byteLength covers
  all layers·faces; per-image stride = blen / (layers·faces)); DDS is
  **surface-major, mip-inner**. Both flip their existing rejects to an
  outer-layer/face loop. Face order **+X,−X,+Y,−Y,+Z,−Z**.
- **API:** new public dispatchers `gpu_texture_create_2d_array` /
  `gpu_texture_create_cube` / `gpu_texture_write_layer_level` + caps
  `gpu_caps_native_texture_array` / `_cube`; **3 new Backend slots** (fncall
  arity is fixed — a layers/layer arg can't fold into the fncall5
  create_2d_fmt_mipped / write_level slots), **BACKEND_SIZE 296 → 320**.

---

## Phase AA.0 — API + slot scaffold (CPU, both backends) [M]

- [x] **AA.0a (2026-06-19)** — `backend.cyr`: 3 fixed-arity slots
  `TEXTURE_CREATE_2D_ARRAY=296` / `TEXTURE_CREATE_CUBE=304` /
  `TEXTURE_WRITE_LAYER_LEVEL=312`; `BACKEND_SIZE 296 → 320` +
  `BACKEND_ARRAYCUBE_SLOTS_BEGIN/END` range. `NativeTexture +60` pad now holds
  `slice_count` (`NATIVE_TEX_FIELD_SLICE_COUNT=60`, struct stays 64 B) +
  `native_tex_slice_count` / `_set` accessors (memset-0 = single layer,
  back-compat). +16 asserts (backend 464→473 slot/size pins; native 1401→1408
  field-offset + accessor round-trip + t_va-no-overlap). Suite 4276→4292;
  smoke/lint/fmt/distlib green.
- [x] **AA.0b (2026-06-19)** — `texture.cyr`: `gpu_texture_create_2d_array` /
  `gpu_texture_create_cube` (faces=6 implicit) / `gpu_texture_write_layer_level`
  dispatchers — the two creates fncall5, write_layer_level **fncall6**
  (Cyrius-to-Cyrius, like `gpu_compute_dispatch` — the fncall6-into-extern-C wgpu
  hazard doesn't apply to a Cyrius filler). `MABDA_MAX_TEXTURE_ARRAY_LAYERS = 256`
  (texture_format.cyr) bounds layers before any multiply. **Null-slot guards**
  (slot == 0 → 0 for create, `GPU_ERR_NOT_IMPLEMENTED` for write) so the unfilled
  AA.0 slots don't fncall on null. `gpu_caps_native_texture_array` / `_cube`
  (backend_native.cyr, default 0; flip at AA.1/AA.2/AA.5/AA.7b). +35 asserts
  (backend 473→508: dispatch + guards + null-slot + caps). Suite 4292→4327;
  smoke/lint/fmt/vet/distlib green.

## Phase AA.1 — Native 2D-array create + per-slice upload (storage) [M]

- [ ] **AA.1a** — `native_gfx9_image_descriptor` gains `(img_type, slice_count)`:
  `W3 TYPE = img_type` (`GFX9_SQ_RSRC_IMG_2D_ARRAY=13` / `_CUBE=11`),
  `W4 |= ((slice_count-1) & 0x1FFF)`. **Existing 2D callers pass
  `(2D, 1)` byte-identical** (the round-trip invariant). CPU oracle for the new
  TYPE/DEPTH bit fields vs gfx9.json.
- [ ] **AA.1b** — native 2D-array RGBA8 **LINEAR** storage +
  `write_layer_level`: `_backend_native_texture_create_2d_array_sampleable`
  (slice_count · per-slice surface contiguous + the T#/S# tail, array T# via
  AA.1a); native write adds `layer·slice_pitch`. **HW-verify on Cezanne:** write
  distinct-color slices, SDMA-read each back, CPU-verify per-slice bytes (no
  sampling yet).

## Phase AA.2 — Native 2D-array sampling FS (DA=1 + slice VADDR) [M]

- [ ] **AA.2a** — per-draw `layer@+56` descriptor tail: write in
  `_backend_native_render_pass_draw` beside the scale writes; FS s-loads it.
  Fixes one layer per draw (sufficient for the AA.6 loader path; **layered
  *render* is out of scope, flagged**). Tail-offset + draw-write tests.
- [ ] **AA.2b** — 2D-array `image_sample` FS (`DA=1` w0 |= 1<<14, 3rd VADDR v6 =
  slice) + bind (S#-from-sampler reused; dimension baked in the T#). **HW-verify
  on Cezanne:** `RT[x,y] == array_tex[layer,x,y]` pixel-exact, POINT then
  BILINEAR.

## Phase AA.3 — wgpu 2D-array create + upload + sample [M]

- [ ] **AA.3a** — wgpu view-dim consts (`2D_ARRAY=3` / `CUBE=4` / `CUBE_ARRAY=5`,
  webgpu.h values) + `wgpu_texture_descriptor_layered` (depthOrArrayLayers) +
  layered view builder (dimension@+28, arrayLayerCount@+44, baseArrayLayer@+40).
  Descriptor byte-offset asserts.
- [ ] **AA.3b** — wgpu `create_2d_array` (2DArray view) + `write_layer_level`
  (writeTexture origin.z) + a `texture_2d_array` WGSL FS; grow the 32-B wgpu tex
  wrapper with layer count + view dim. **HW-verify on this box** (wgpu-native +
  Vulkan/RADV present).

## Phase AA.4 — Native 2D-array compressed (tiled) [M]

- [ ] **AA.4a** — **GATE: tiled-array slice-stride** (gfx9.json/addrlib): does
  `SW_64KB_S` interleave slices within a 64 KiB block or pad to a slice-pitch
  boundary; is `ARRAY_PITCH (W5[13:16]) = 0` correct for a non-mipped tiled
  array; can one `COPY_TILED` with `copy_d=N-1` move all slices or is a per-slice
  loop required? Pure investigation + CPU oracle; **no HW until resolved**.
- [ ] **AA.4b** — native 2D-array compressed (BC) tiled create + per-slice upload
  + sample (reuse the AA.2b array FS). HW-verify BC1/3/4/5/7 array layers vs CPU
  decode on Cezanne. **BC-only** (ETC2/ASTC HW-blocked on AMD — unchanged).

## Phase AA.5 — Cube native + wgpu (the Cezanne HW-risk phase) [M]

- [ ] **AA.5a** — native cube T# + 6-slice storage + per-face upload (cube = a
  6-slice 2D-array; `TYPE=11`, `DEPTH=5`). HW-verify storage+readback —
  distinct per-face colors via SDMA, confirming the **+X,−X,+Y,−Y,+Z,−Z**
  face→layer ordering (no sampling yet).
- [ ] **AA.5b** — **THE load-bearing Cezanne HW risk:** native cube
  direction→(face,u,v) `image_sample` FS (`DA=1`, float s/t/r) — brand-new ISA,
  no FS to reuse. HW-prove POINT single-mip first, then BILINEAR + seamless
  edges. **If Cezanne TA cube filtering is unreliable → the cube SAMPLING FS is
  gated (cube storage from AA.5a still ships, `caps_cube` stays 0)** — flagged,
  not silently dropped.
- [ ] **AA.5c** — wgpu cube create (depthOrArrayLayers=6, Cube view) + 6-face
  upload + a `texture_cube` WGSL FS; validate width==height. **HW-verify on this
  box** (wgpu-native present).

## Phase AA.6 — DDS/KTX2 array + cube loader integration [M]

- [ ] **AA.6a** — **overflow-safe per-layer offset math** (re-apply the 3.3.0
  KTX2 byteOffset CRITICAL): `per_image = blen / total_images` with
  `total_images != 0` guard + `blen % total_images != 0` reject; `sub_off = boff
  + image_idx·per_image` with the `sub_off > len - per_image` no-overflow check
  re-applied **per layer** (both divisor and multiplier are now untrusted header
  fields). Regression asserts for div-by-zero + overflow.
- [ ] **AA.6b** — KTX2 array + cube load: remove the faceCount/layerCount
  rejects; surface `layer_count` + `face_count`; **mip-outer, layer-inner**
  loop; `blen == per_image · layers · faces`; route to create_2d_array /
  create_cube + write_layer_level. Existing reject asserts **flip** to
  successful-parse + correct-count asserts.
- [ ] **AA.6c** — DDS array (DX10 arraySize) + cube (legacy dwCaps2 + DX10
  miscFlag) load: remove the rejects; `face_count` (legacy: require all 6 face
  bits; DX10: arraySize·(cube?6:1)); **surface-major, mip-inner** outer loop;
  reuse AA.6a math. Reject asserts flip.

## Phase AA.7 — HW e2e + arc cut [M]

- [ ] **AA.7a** — `native_array_cube_load_e2e` on Cezanne: load a real KTX2/DDS
  array + cube through the public API, create+upload+sample, CPU-verify
  per-layer/per-face distinct colors — **the data-ordering HW verify** (DDS
  +X..−Z and KTX2 layer→face→z must map 1:1 to the backend layer index the
  sampler expects). Closes the spec-vs-HW ordering open Q.
- [ ] **AA.7b** — closeout: flip `caps_native_texture_array` / `_cube` to 1 (cube
  gated on AA.5b), distlib idempotent, version-check, re-run the per-layer-stride
  audit checklist against the diff, audit doc, CHANGELOG + roadmap + CLAUDE.md.
  **Cut 3.4.0.**

---

## Gates & risks (nothing silently dropped)

1. **Cube sampling = the single biggest HW risk** (AA.5b, Cezanne gfx90c). New
   direction→(face,u,v) ISA, no FS to reuse; seamless cube-edge filtering
   unverified. POINT first, then BILINEAR. Unreliable → cube sampling gated,
   cube storage (AA.5a) still ships, `caps_cube=0`.
2. **KTX2/DDS per-layer offset overflow-safety** (AA.6a) carries the 3.3.0
   CRITICAL forward — divisor AND multiplier now untrusted. Lands WITH AA.6b/6c
   + regression asserts that would catch a malicious header.
3. **Data-ordering** (AA.7a) needs HW verification — DDS +X..−Z and KTX2
   layer→face→z must map 1:1 to the backend layer index. Per-face distinct-color
   HW test; not settleable on CPU.
4. **Tiled-array slice stride** (AA.4a) is an addrlib/gfx9.json gate before any
   tiled-compressed-array HW.
5. **Layered *render* (rendering INTO array layers / cube faces) is OUT OF
   SCOPE** for this arc — the `layer@+56` tail fixes one *sampled* layer per
   draw. Flagged for a later arc, not silently implied.
6. **Cube-array** (KTX2 layerCount≥1 ∧ faceCount==6; DDS arraySize·6): keep
   `total_images = layers·faces` general in the parser, but **gate cube-array
   CREATE behind a reject** until a HW test exists, so the parser never mis-sizes
   a kind the backend can't sample.
7. **ETC2/ASTC array/cube stay HW-blocked on AMD** — native compressed-array
   (AA.4) is BC-only, matching the existing 2D cap.
8. **wgpu HW-verify runs on this box.** `deps/wgpu-native/lib/libwgpu_native.so`
   + the `wgpu_main.c` launcher + Vulkan/RADV (RENOIR = Cezanne) are all present,
   so the wgpu path runs here via `make test-phase0` / `test-gpu` — AA.3b/AA.5c
   get real HW verification (same silicon as native, through Vulkan vs DRM).
   (The `.so` predates the 6.2.26 bump; a `test-phase0` smoke at AA.3 confirms it
   still links — only if that drifts do the wgpu bites fall back to CPU-asserted.)
9. **`MABDA_MAX_TEXTURE_ARRAY_LAYERS` gate** (AA.0b, 256) bounds layers·faces
   before it scales any offset (GFX9 DEPTH[0:12] max 4095 slices).
