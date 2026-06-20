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

- [x] **AA.1a (2026-06-19)** — `native_gfx9_image_descriptor_typed(…, img_type,
  slice_count)` does the work (`W3 TYPE = img_type`, `W4 DEPTH[0:12] =
  slice_count-1`); the old `native_gfx9_image_descriptor` is now a thin wrapper
  delegating with `(2D, 1)` — **byte-identical**, so the 1 prod + 8 test callers
  + the oracle are untouched (no churn). `GFX9_SQ_RSRC_IMG_2D_ARRAY=13` /
  `_CUBE=11` consts. +9 asserts (native 1408→1417: TYPE 13/11 + DEPTH 7/5 + the
  slice-count-0 clamp + a byte-for-byte wrapper==_typed(2D,1) round-trip check).
  Suite 4327→4336; distlib idempotent.
- [x] **AA.1b (2026-06-19) — HW-VERIFIED on Cezanne.**
  `_backend_native_texture_create_2d_array_sampleable` (one BO of `layers·per_slice`
  contiguous slices + the T#/S# tail, array T# via the AA.1a `_typed` builder;
  `NATIVE_TEX_FIELD_SIZE` = per-slice bytes, `slice_count` = layers) +
  `_backend_native_texture_write_layer_level` (LINEAR: dest `+= layer·per_slice`;
  level>0 / tiled fail loud → AA.4). Slots wired in `backend_native_new`.
  `programs/native_array_store_e2e.cyr` + `make test-native-array-store-e2e`:
  4-layer 64×64 RGBA8 array created on the GPU (real BO + VA map), distinct color
  per layer written, **each reads back correct at `base + layer·per_slice`**.
  +17 CPU asserts (native 1417→1434: write_layer slice-math + both fillers' guard
  ladders). LINEAR/uncompressed only (compressed array = AA.4). Suite 4336→4353;
  smoke/lint/fmt/distlib green.

## Phase AA.2 — Native 2D-array sampling FS (DA=1 + slice VADDR) [M]

- [x] **AA.2a (2026-06-19)** — layer plumbing. The array slice is recorded in the
  descriptor tail at **+56** at BIND time (the draw writes scale@+48/+52 but
  leaves +56, so it survives — no pass-field/draw change needed).
  `gpu_render_pass_bind_texture_layer(ctx,pass,tex,sampler,layer)` (new slot
  `BIND_FOR_SAMPLE_LAYER`=320, `BACKEND_SIZE` 320→328) → native
  `_backend_native_texture_bind_for_sample_layer` writes `dcpu+56=layer`; the
  plain bind = this with layer 0 (a re-bind resets the layer). **Fix:**
  `native_tex_desc_cpu_addr` now returns 0 when `ADDR==0` — the refactor's
  unconditional +56 write would otherwise hit a fake-VA test tex's unmapped
  address (caught a real **segfault**; the grep-for-`failed` suite check missed
  it — now gate on exit code, see [[feedback_verify_test_suites_by_exit_code]]).
  +22 asserts (backend 508→521 dispatch/guards/size-pin; native 1434→1443
  +56-write + reset + guards). Suite 4353→4375; all suites exit 0; distlib idempotent.
  (One sampled layer per draw; layered *render* is a later arc.)
- [x] **AA.2b (2026-06-19) — HW-VERIFIED on Cezanne.**
  `native_gfx9_shader_textured_load_array_fs` — the 2D-array `image_load` FS:
  scale-load grows `dwordx2→dwordx4` (s14 = layer@+56), `v_mov v6, s14`,
  `image_load v[4:6] … DA` (`w0 0xF0001F00→0xF0005F00`, bit 14). All new dwords
  **llvm-mc-verified** (`dwordx4=0xC00A0300`, `v_mov v6,s14=0x7E0C020E`, `DA
  w0=0xF0005F00`). ABI unchanged (v0..v6 + s0..s15 still fit RSRC1 MIN|1 → draw
  override reused). `native_array_sample_e2e` + `make test-native-array-sample-e2e`:
  **`RT[x,y] == array[layer,x,y]` for layers 0 AND 2** (distinct per-layer alpha
  proves the slice coordinate selects). **`gpu_caps_native_texture_array` flipped
  to 1.** +34 asserts (native FS byte-oracle 1443→1477; caps-flip). Suite
  4375→4409; all suites exit 0; distlib idempotent. (POINT verified via the
  screen-pos oracle; the BILINEAR-array path rides the existing image_sample FS
  scale and is exercised when a consumer needs it.)

## Phase AA.3 — wgpu 2D-array create + upload + sample [M]

- [x] **AA.3a (2026-06-19)** — wgpu view-dim consts (`2D_ARRAY=3`/`CUBE=4`/
  `CUBE_ARRAY=5`, **pinned against `deps/wgpu-native/include/webgpu/webgpu.h`**) +
  `wgpu_texture_descriptor_layered` (depthOrArrayLayers@+44 = layers; tex
  dimension stays 2D) + `wgpu_texture_view_descriptor_array` (dimension@+28 =
  2DArray, arrayLayerCount@+44 = layers, baseArrayLayer@+40 = 0). +11 asserts
  (backend 521→532, descriptor byte-offsets). **Confirmed the wgpu path still
  links/runs on 6.2.28** (`make test-phase0` green — context/buffers/textures/
  samplers), so AA.3b is HW-verifiable here. Suite 4409→4420; distlib idempotent.
- [x] **AA.3b (2026-06-19) — HW-VERIFIED on Cezanne (wgpu via Vulkan).**
  `_backend_wgpu_texture_create_2d_array` (layered descriptor `depthOrArrayLayers`
  + 2DArray view; 32-B wrapper handle@0/view@8/w@16/h@20/LAYERS@24/fmt@28,
  caps-gated) + `_backend_wgpu_texture_write_layer_level` (writeTexture origin.z =
  layer via `wgpu_texel_copy_texture_info_layer`; mipLevel+origin.z, so
  mipped-array-ready). Slots wired. `programs/wgpu_array_sample_e2e.cyr` +
  `make test-wgpu-array-sample-e2e`: 4-layer array created+uploaded, a
  `texture_2d_array` WGSL FS samples it, **RT == layer color for layers 0 AND 2**
  (distinct R proves the `array_index` selects). All suites exit 0; distlib
  idempotent. (Found+fixed: `str_builder_add` takes a `Str`, not (ptr,len) — the
  WGSL layer digit must go via `str_builder_add_cstr` on a NUL-terminated buf.)
- [ ] **AA.3c** — wgpu DRAW-TIME layer selection (`gpu_render_pass_bind_texture_layer`
  on wgpu, currently native-only → wgpu returns NOT_IMPLEMENTED). Needs a layer
  uniform the `texture_2d_array` FS reads + the wgpu `bind_for_sample_layer` filler
  building the bind group with it (AA.3b's e2e hardcodes the layer in WGSL, which
  isolates the array-view+sample path but isn't consumer-selectable). The native
  path already supports it (the +56 descriptor tail). **Tracked asymmetry, not
  dropped** — flag for this arc's remainder or a follow-on.

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

- [x] **AA.5a (2026-06-19) — HW-VERIFIED on Cezanne.** Refactored the AA.1 array
  create into a shared `_native_create_layered_sampleable(…, slices, img_type)`;
  `_backend_native_texture_create_cube(ctx, size, fmt, mip_count)` = it with
  `slices=6, img_type=CUBE` (TYPE=11, DEPTH=5; square; single-level — mipped cube
  fails loud). Slot wired; `write_layer_level` reused for per-face upload.
  `native_cube_store_e2e` + `make test-native-cube-store-e2e`: 6-face 64×64 cube
  created on the GPU, 6 distinct faces written, **each reads back at
  `base + face·per_face`** (the +X,−X,+Y,−Y,+Z,−Z ordering). +5 CPU asserts
  (cube-create guards, native 1477→1482). Suite 4420→4425; all suites exit 0;
  distlib idempotent.
- [x] **AA.5b (2026-06-19) — THE marquee HW risk: PASSED on Cezanne.**
  `native_gfx9_shader_cube_sample_fs` (`image_sample DA=1` cube,
  `w0=0xF0804F00`; 3 inline-float `v_mov`s → v[4:6]). **Key HW finding:** GFX9
  cube `image_sample` VADDR is **(s, t, FACEID)** — the 3rd component is the face
  index (0..5), NOT a raw direction (a raw (0,0,1) sampled face 1; the consumer
  derives faceid from a direction via the `v_cube*` ALU). The cube S# needs
  **unnorm=0** (`_native_create_layered_sampleable` gained an `unnorm` param;
  cube=0, array=1). `native_cube_sample_e2e` + `make test-native-cube-sample-e2e`:
  **`RT == face[faceid]` for faceid 4 (+Z) and 0 (+X)**, proving the CUBE T# face
  addressing. +cube FS byte-oracle (llvm-mc-verified), saved to memory
  [[reference_gfx9_cube_image_sample_faceid]]. Suite 4425→4457; all exit 0.
  **`caps_native_texture_cube` stays 0** pending the consumer-facing ergonomics
  (a cube-aware bind that auto-selects unnorm=0 — the generic bind forces unnorm=1,
  so the e2e binds with sampler 0; the `v_cube*` direction FS is consumer-side).
  Tracked — the marquee *reliability* risk is retired.
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
