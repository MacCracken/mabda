# Mabda — Claude Code Instructions

## Project Identity

**Mabda** (مبدأ — Arabic: origin, principle, starting point) — GPU
foundation layer for AGNOS. Owns device lifecycle, buffers, compute
dispatch, textures, render pipelines, profiling, and capability
detection.

- **Type**: Cyrius library (include-chain) + dist bundle + dual-backend
  (wgpu C-launcher path + native AMD DRM-ioctl path)
- **License**: GPL-3.0-only
- **Language**: Cyrius 6.2.29+ (`cyrius.cyml: cyrius = "6.2.29"`)
- **Version**: 3.4.0 in tree. 3.0.0 GA shipped 2026-06-02. 3.0.x tracked
  the toolchain + AGNOS deps; 3.0.4 → P(-1) security-hardening patch.
  **3.1.0** → on-device mipmap generation (native AMD HW-verified; wgpu
  `generate` deferred). **3.1.1** → multi-queue coordination (logical
  queue abstraction + native GFX/COMPUTE timeline queues + cross-ring
  barrier, HW-verified; wgpu aliases the single device queue). **3.2.0** →
  opens the v3.2.x "texture & shader breadth" arc with block-compressed
  textures (BC/ETC2/ASTC): wgpu create+upload+sample, native AMD
  storage+readback (native compressed *sampling* is Phase TS, this arc).
  **3.2.1** → buffer-to-buffer copy (Phase X): public `gpu_buffer_copy` /
  `gpu_queue_transfer_copy` on both backends (native SDMA on a real DMA
  ring with >4 MiB packet chaining; wgpu `copy_buffer_to_buffer`), real
  native `gpu_buffer_*`, TRANSFER→DMA ring + `GPU_ERR_TRANSFER`, all
  HW-verified on Cezanne.
  **3.2.2** → native RGBA8 texture *sampling* (Phase TS.1–5): GFX9 T#/S#
  descriptor builders (gfx9.json-pinned) + sampleable textures +
  `gpu_sampler_create` + `gpu_render_pass_bind_texture` + a textured `image_load`
  FS, so native AMD samples a texture in an FS — `RT[x,y]==tex[x,y]` HW-verified;
  wgpu gains the real bind+sample render path.
  **3.2.3** → native compressed + filtered sampling (Phase TS.6–8), completing
  the arc's native-sampling story: **BC (BC1/BC3/BC4/BC5/BC7) sampling** via SDMA
  SW_64KB_S tiling (COPY_TILED L2T/T2L) + tiled `create_sampleable` + the
  `image_sample` FS, pixel-exact vs a CPU decode; **bilinear / scaled** filtering
  (per-draw `tex_dim/rt_dim` scale in the FS + S#-from-bound-sampler), POINT-vs-
  BILINEAR HW-verified; native caps advertisement (`gpu_caps_native_texture_compression`).
  **ETC2/ASTC are HW-blocked on AMD** (`vulkaninfo`: BC=true, ETC2/ASTC=false) —
  cap stays BC-only; **BC6H** needs an HDR RT (gated). Toolchain 6.2.11→6.2.12.
  **3.2.4** → SPIR-V shader ingestion on wgpu (Phase S): a source-KIND tag on
  the byte-polymorphic shader boundary (`ShaderSourceKind`), the widened
  shader-create slot `(…,kind)`, `gpu_shader_module_create_spirv` + the
  `WGPUShaderSourceSPIRV` builder + `_spirv_validate` + a kind-folded shader
  cache, and the launcher `ShaderSourceSPIRV` instance feature. HW-verified
  byte-identical to WGSL (`spirv_e2e` cross-source identity). Native SPIR-V→GFX9
  is Phase N (fail-loud). Toolchain 6.2.12→6.2.14.
  **3.2.5** → opens the **native SPIR-V→GFX9 compute compiler** (Phases N.0+N.1,
  pure CPU, no public-API change). **N.0:** `gfx9_encode.cyr` —
  operand-parameterized GFX9 encoders (VOP1/VOP2/VOP3a/SOP2/SOPP/SMEM/FLAT) +
  emit helpers, proven byte-identical against ~90 hand-authored dwords
  (the `compiler_encode.tcyr` oracle + per-form llvm-mc round-trip); the six
  `native_gfx9_shader_*` builders were then re-expressed to emit through the
  encoders (zero byte change — the proposal's round-trip check). **N.1:**
  `spirv_parse.cyr` — the SPIR-V front end: `spirv_validate_stream` (the
  untrusted-input rejection gate) + entry-point/LocalSize probes + the
  type/constant/decoration lookup tables N.2 lowers from. EXP/MIMG encoders +
  native SPIR-V lowering (N.2+) are later in the arc. Toolchain 6.2.14→6.2.15.
  **3.2.6** → the **native SPIR-V→GFX9 compute compiler, HW-verified end-to-end on
  Cezanne** (Phases N.2–N.5). A SPIR-V compute kernel lowers all the way to GFX9
  ISA + a dispatch descriptor and runs correctly on the AMD GPU. **N.2** `mir.cyr`
  + `spirv_lower.cyr` — SSA MIR (40-B value records carrying GFX9 **uniformity**)
  + the SPIR-V→MIR lowering (ALU/cvt/extract + the `OpAccessChain` buffer path +
  builtin resolve + a forward uniformity sweep: UNIFORM→SGPR/SALU vs
  DIVERGENT→VGPR/VALU + the `GlobalInvocationId = wgid*local_size + lid`
  expansion). **N.3** `gfx9_isel.cyr` — MIR→virtual-reg GFX9 selection (the
  SALU-vs-VALU choice falls out of uniformity). **N.4** `gfx9_regalloc.cyr`
  (linear-scan VGPR/SGPR, no-spill fail-loud, ABI-reserved bases) +
  `gfx9_waitcnt.cyr` (`s_waitcnt` so no instruction reads an outstanding load —
  use-before-wait impossible by construction). **N.5** `gfx9_abi.cyr` (canonical
  compute ABI: binding k→USER_DATA `s[2k:2k+1]`, WGID→TGID SGPRs, LID→`v0..` +
  `gfx9_rsrc1`/`rsrc2`) + `gfx9_compile.cyr` (the encode driver + the top-level
  `gfx9_compile` chaining the whole pipeline → ISA + descriptor) + the generic
  compute dispatch composer + `native_spirv_compute_e2e` (a compiled
  `out[lid.x]=lid.x*3+7` runs on Cezanne, all 8 lanes correct). Toolchain
  6.2.15→6.2.18 (native f32 `f32_from`/`f32_to` builtins — the hand-rolled f32 asm
  shims in `color.cyr` were retired; `src/` now has zero inline asm). Every stage
  adversarially reviewed (~20 findings fixed pre-merge). The MVP — a compiled
  kernel on real silicon — is reached.
  **3.2.7** → the **native SPIR-V→GFX9 compiler reaches the public API** (Phases
  N.5g + N.6 + N.6r) plus a security hardening (N-HARDEN.1), all HW-verified on
  Cezanne. **N.5g** (the named MVP exit): a 2×2 box-filter downsample compiled
  in-tree **pixel-matches** a CPU box-filter (`native_spirv_downsample_e2e`).
  **N.6** core: a novel 2-binding SAXPY-shape kernel + the generic N-binding PM4
  composer (`native_pm4_build_compute_dispatch`) + the variable-BO cached submit
  (`native_compute_dispatch_cached_n`). **N.6r**: a consumer compiles + dispatches
  SPIR-V entirely through the public API — `gpu_shader_module_create_spirv` →
  the native slot runs `gfx9_compile`, stages the ISA in a BO, and records RSRC +
  bindings + LocalSize in a **magic-tagged** shmod; `gpu_compute_dispatch` detects
  the tag and dispatches through the generic composer (the legacy 16-byte deadbeef
  pair is discriminated by the absent magic — untouched). **N-HARDEN.1** (security,
  found via the N.5g adversarial review): the SPIR-V table builders no longer trust
  the untrusted header `id_bound` — each takes a `cap`, rejects `id_bound > cap`
  before any write (the OOB the compiler-on-the-public-API path would have exposed).
  Toolchain 6.2.18→6.2.19. 1-D compiled dispatch (multi-dim grid fails loud) +
  `id_bound`≤128 are the MVP limits; 2-D/TGID is the N.6 remainder.
  **3.2.8** → the **native SPIR-V→GFX9 scalar f32/i32 compiler**, control flow +
  full op breadth, HW-verified on Cezanne. **Phase N.7** — structured control flow:
  uniform `if` (`s_cmp`/`s_cbranch_scc0`, two-pass branch layout) and divergent `if`
  (the EXEC mask: `v_cmp`→VCC + `s_and_saveexec_b64` + `s_cbranch_execz` + `s_or_b64`
  restore, saved-EXEC in `s[100:101]`). **Phase N.8** — op breadth: VOP3-literal
  materialization (`gid*100`), GLSL.std.450 math (min/max/sqrt/floor via `OpExtInst`,
  the ternary Fma, FClamp via `v_med3_f32`, FAbs via sign-bit clear), float inline
  constants (±0.5/1/2/4), signed int compares (SLT/SGT/SLE/SGE → i32 SOPC/VOPC),
  store-constant materialization, VOP2 const operands, and compile-time const-fold of
  two-constant ops. 18 `native_spirv_*_e2e` HW programs on Cezanne; every bite
  adversarially reviewed (Workflow). Toolchain 6.2.19→6.2.21.
  **3.2.9** → the **native SPIR-V→GFX9 integer division & remainder** family (**Phase
  N.9**), the full int div/mod set HW-verified on Cezanne. GFX9 has no integer-divide
  instruction, so `OpUDiv` (134) expands to LLVM's loop-free float-reciprocal **vector**
  macro (cvt→`rcp_iflag`→×`0x4F7FFFFE`→cvt→Newton→estimate→2× cmp/cndmask correction;
  the magic constant biases the reciprocal down so `v_rcp`'s ≤1-ULP error can't
  overshoot) with a `b=0 → 0xFFFFFFFF` guard. `OpUMod` (137) reuses the factored
  `_udiv_core` and selects the corrected remainder (`b=0 → N`). The signed ops wrap the
  unsigned core in a sign-magnitude shell — `_signed_prep` (ASHR-31 sign masks +
  branchless abs `(x^sign)-sign`) → core on `|a|/|b|` → sign re-apply: `OpSDiv` (135)
  `(q^rs)-rs`, `OpSRem` (138) `(r^signA)-signA` (dividend sign, C `%`), `OpSMod` (139) a
  floored fixup adding the divisor when the remainder is nonzero and signs differ
  (divisor sign, GLSL `mod`). One new primitive — arithmetic-shift-right
  (`v_ashrrev_i32` 0x11 / `s_ashr_i32` 0x20). Five `native_spirv_{udiv,umod,sdiv,srem,
  smod}_e2e` HW programs, value-exact vs independent CPU references (8-lane edge matrices
  + 32-lane HW-stress sweeps); every bite adversarially reviewed (Workflow). Toolchain
  6.2.21.
  **3.2.10** → **native SPIR-V→GFX9 vector support** (vec2/3/4, **Phase N.10**), HW-verified
  on Cezanne. GFX9 has no vector registers → **scalarize-on-lower**: a vecN is a
  `MIR_VK_VECTOR` value packing its N component scalar `<id>`s (16 bits each; count in the
  packed type), and every vector op emits N scalar ops, so the whole scalar back end is reused
  unchanged. **N.10a** `OpCompositeConstruct` (packs operand ids, no instrs) / `OpCompositeExtract`
  (one `VMOV` from the packed component; GID vec3 special-case kept ahead). **N.10b**
  component-wise binary arith (`_spirv_lower_vec_binop` → N scalar ops + scalar broadcast +
  per-component const-fold) + the **constant-index FLAT enabler** (a constant `OpAccessChain`
  index VMOV-materializes its byte offset into a VGPR — the FLAT emitter needs a VGPR offset;
  prior kernels only used the dynamic gid offset). **N.10c** `array<vecN>` `OpLoad`/`OpStore`
  (N consecutive 4-byte accesses at `base+j*4`) with the **std430 array stride** computed from
  the element type (vec2→8, vec3/vec4→16) + a fail-loud `_spirv_check_array_strides` gate
  rejecting a non-std430 `ArrayStride`. **N.10d** `OpConstantComposite` (seed pass reuses the
  Construct packer) + splat. Central enforcement: `mir_emit` rejects a `MIR_VK_VECTOR` operand
  (vectors must be scalarized first). Four `native_spirv_vector_{add,load_store,const}_e2e` +
  `…_load_store` HW programs value-exact on Cezanne; every bite adversarially reviewed
  (Workflow). Toolchain 6.2.21.
  **3.2.11** → **per-ext-set tracking** (**Phase N.11**), closing the v3.2.x compiler-breadth
  arc. `spirv_lower_module` resolves the `OpExtInstImport "GLSL.std.450"` id
  (`_spirv_find_glsl_ext_set`, packed-string match, nul-byte-only terminator check) and
  `_spirv_check_ext_sets` requires every `OpExtInst` to reference it (else `LOWER_ERR_EXT_SET`) —
  dropping the MVP "only GLSL.std.450 imported" assumption (a foreign ext set, or no GLSL import,
  fails loud rather than mis-decoding its numbers as GLSL math). Validation-only (no new HW
  capability); existing GLSL kernels still run on Cezanne. Toolchain 6.2.21. **N.7–N.11 complete**.
  **3.2.12** → general native **f64** (double-precision) compute (**Phase F**), HW-verified bit-exact
  on Cezanne. The SPIR-V→GFX9 compiler emits `V_*_F64` for the conformant **arithmetic** toolkit —
  64-bit register pairs (even-aligned `v[2:3]`) threaded as `MIR_T_F64` through MIR→isel→regalloc→
  encode — covering add/sub/mul/**div** (correctly-rounded reciprocal-Newton macro)/**sqrt**
  (correctly-rounded, full-range incl. subnormals)/fma, i32↔f64 + f32↔f64 convert, OpConstant,
  Ldexp, ordered compares, and OpSelect (via `v_bfi_b32` on a 0/-1 mask). `MABDA_NATIVE_F64` flipped
  to **1**; `gpu_caps_native_shader_f64()` returns 1. **Transcendentals are arithmetic-only**:
  GLSL.std.450 Exp/Log/Pow are f32-only (spirv-val rejects f64), so consumers (attn11) hand-roll
  exp/ln/tanh/pow as polynomials over the toolkit (a complete degree-13 exp macro was backed out as
  uninvocable). The F.9 layernorm smoke runs through the public API (`gpu_shader_module_create_spirv`
  → `gpu_compute_dispatch`). Every bite adversarially reviewed (Workflow); five reviews caught real
  silent-wrong bugs (forced-DIVERGENT, 64-bit MIR-operand truncation, vec/store const, OpSelect mask)
  — each fixed + pinned. Toolchain 6.2.21→6.2.22.
  **3.2.13 + 3.2.14** → **render-graph multi-queue scheduling (Phase R)**, HW-verified on Cezanne,
  closing the v3.2.x arc. The v2.5 render graph was single-submit (one wgpu encoder, one queue
  submit); Phase R makes it multi-queue aware on native and serialized-equivalent on wgpu. **3.2.13
  (R.1–R.4):** per-node queue affinity (`rg_node_queue`; `Node` 72→80 — `queue_kind`/`batch_idx`),
  the pure-CPU scheduler `rg_schedule` (per-queue Batches + cross-queue writer→reader `FenceEdge`
  classification; `RenderGraph` 40→48), and `native_render_dispatch_timeline` (the GFX-ring timeline
  analog of the compute path). **3.2.14 (R.5–R.7):** per-node IB staging (the cached IB BO →
  `_NATIVE_IB_SLOTS`=16 slices + a `GpuContext +160` cursor, ctx 160→168, so concurrent in-flight
  submits don't clobber a shared IB), the native MQ executor `_rg_execute_native_mq` (global-sort
  walk + cross-queue `gpu_queue_barrier` insertion + per-ring timeline dispatch + `wait_idle`;
  `_backend_native_render_pass_draw` made queue-aware; byte-polymorphic node payloads), and the HW
  e2e `native_render_graph_mq_e2e.cyr` (compute on the COMPUTE ring → render on the GFX ring, cross-
  queue ordered, distinct rings, both results CPU-verified, 5/5 stable). The executor submits in
  global sort_idx order (the toposort guarantee), not batch-by-batch min-sort. wgpu stays on the v2.5
  single-encoder path. multi-dim grid + `id_bound`≤128 still to be raised.
  The v3.1.2-deferred TRANSFER/buffer-copy + render-graph multi-queue were
  pulled INTO the v3.2.x arc (Phases X / R) per maintainer decision — with the
  N.10/N.11 (vectors + per-ext-set) insert shifting Phase F/R down, the
  arc spans 3.2.0→3.2.14 (F shipped as a single 3.2.12 cut, R is 3.2.13–3.2.14),
  nothing deferred to v3.3/v4 (see the punchlist).
  **3.3.0** → opens v3.3 with **asset loading (Phase AL)**, shipped as a single cut
  (AL.0–AL.6). A backend-agnostic loader layer (`src/asset_load.cyr` + `asset_format.cyr`):
  parse a container header (bounds-checked against untrusted input), map its format to a
  `MABDA_TEXFMT_*` id, caps-gate it, create + upload each level. **DDS** (FourCC DXT1/3/5 +
  DX10/DXGI) and **KTX2** (uncompressed subset) parsed in-tree; **PNG** CPU-decoded by the new
  pure-Cyrius sibling package **chitra** (`[deps.chitra] tag="0.1.0"`, + `thread`/`sankoch`
  transitive stdlib deps) then uploaded RGBA8; a magic-byte sniffer (`gpu_texture_load`) routes
  all three. New public API: `gpu_texture_load{,_dds,_ktx2,_png,_png_mipped}` (+ `_result`),
  `gpu_texture_create_2d_fmt_mipped`, `gpu_texture_write_level`, `gpu_ctx_supports_format`; two new
  Backend slots (`BACKEND_SIZE` 280→296); `GPU_ERR_CONTAINER_PARSE = 21`; format-map unmapped
  sentinel is **-1** (0 is the valid RGBA8 id). **Native PNG path HW-verified on Cezanne**
  (`native_load_png_e2e`: file→decode→upload→sample→pixel-exact). Pre-cut adversarial security
  review of the untrusted-input parsers found+fixed **1 CRITICAL** (KTX2 `byteOffset`
  signed-overflow OOB → `boff > len - blen`) + regression test
  (`docs/audit/2026-06-19-asset-loading-audit.md`). Arrays/cubemaps parsed-and-rejected-loud
  (v3.4); KTX2 supercompression fails loud; native compressed-mip *sampling* awaits the tiled
  path (linear storage/upload ships). Toolchain 6.2.23.
  **3.4.0** → **array textures + cubemaps (Phase AA / AL-ARRAY)**, HW-verified end-to-end on
  Cezanne, closing the parse-and-reject gap the v3.3 loaders left. **2D array textures** + **cubemaps**
  now create + upload-per-layer/face + sample + load-from-`.dds`/`.ktx2` on BOTH backends, additively
  (no consumer call-signature change). Native arrays sample via `image_load DA=1` + a slice VADDR;
  native cubes via `image_sample DA=1` with an **`(s,t,FACEID)`** VADDR (the GFX9 finding: a face
  *index*, not a raw direction — the consumer derives it via `v_cube*`); a cube-aware bind
  auto-selects `unnorm=0`. wgpu uses 2DArray/Cube views + `texture_2d_array`/`texture_cube` WGSL
  (naga projects a direction). The DDS/KTX2 loaders flip their array/cube rejects to per-image
  `write_layer_level` (overflow-safe per-image split carrying the 3.3.0 audit forward; cube-array /
  array>256 / mipped-multi-image / partial-cube rejected loud). New API:
  `gpu_texture_create_2d_array` / `_cube` / `gpu_texture_write_layer_level` /
  `gpu_render_pass_bind_texture_layer` + `gpu_caps_native_texture_array`/`_cube` (both → 1); 4 new
  Backend slots (`BACKEND_SIZE` 296→328); `MABDA_MAX_TEXTURE_ARRAY_LAYERS=256`; the NativeTexture
  `slice_count` (+60, is-cube in bit 31). HW-verified: native+wgpu array sample, native+wgpu cube
  sample, and the data-ordering capstone (`native_array_cube_load_e2e` — real KTX2 array+cube files,
  file image k → slice k). Adversarially reviewed pre-cut (0 critical). **Deferred to 3.4.1:** AA.4
  native compressed/tiled arrays, AA.3c wgpu draw-time layer, and the `F64_HALF`/`F64_TWO`↔`math`
  collision (the attn11 block → namespace `MABDA_F64_*`). Toolchain 6.2.23→6.2.28.
  v3.0 ships dual backend (wgpu +
  native AMD); native is `Backend`-slot-abstracted alongside wgpu, AMD
  only in v3.0; NVIDIA/Intel native scoped to v4.0/v5.0.
- **GPU FFI**: dual-path
  - **wgpu** — wgpu-native v29 C API via `deps/wgpu_main.c` launcher
    (65-slot fn table, 7 struct-packing shims)
  - **native AMD** — direct DRM ioctls via `syscall(SYS_IOCTL)`. No
    libdrm. Phase B = compute (PM4 + AMDGPU CS), Phase C = textures
    + render (clear-triangle composer, GFX-ring dispatch), Phase D =
    KMS surface (modeset, page-flip, present)
- **`samvada` dep** — sister AGNOS package (sibling of sakshi/patra/
  sigil) carrying the dbus client for logind master delegation.
  Wired via `[deps.samvada] tag = "0.4.1"`. Consumer programs link
  `samvada/deps/samvada_main.c` to populate the C-shim fn-table.
  v4.0 retires both `wgpu_main.c` and `samvada_main.c` together.

## Goal

One Cyrius library that answers "set up a GPU device, move bytes on
and off it, and draw / compute / present with them" for every AGNOS
downstream. v3.0 ships dual backend (wgpu and native AMD); v4.0
retires the wgpu path for AMD. Backend abstraction is the
load-bearing v3.0 architectural choice — the public API surface
doesn't change between paths.

## Current State (post 3.4.0, 2026-06-19)

- **Source**: 51 domain modules under `src/*.cyr`, ~22,600 lines
  (`queue.cyr` added at v3.1.1 for the logical queue abstraction; the
  former 137-KiB `backend_native.cyr` was split into four files at
  rc.2 — `_amdgpu.cyr` / `_shaders.cyr` / `_pm4.cyr` / `.cyr` —
  to land under cyrius lint/fmt's 128 KiB cap; the SPIR-V→GFX9 compiler
  added 9 no-deps-tier modules before `_shaders.cyr` — `gfx9_encode` +
  `spirv_parse` (3.2.5) then `mir` / `spirv_lower` / `gfx9_isel` /
  `gfx9_regalloc` / `gfx9_waitcnt` / `gfx9_abi` / `gfx9_compile` (3.2.6);
  3.2.12 Phase F extended `gfx9_encode` / `mir` / `spirv_lower` / `gfx9_isel` /
  `gfx9_compile` in place with the f64 `V_*_F64` path — no new module; 3.2.13–3.2.14
  Phase R extended `render_graph.cyr` (scheduler + native MQ executor), `queue.cyr`
  ordering, `context.cyr` (IB-slot cursor, ctx 160→168), and `backend_native.cyr`
  (`native_render_dispatch_timeline` + IB staging + queue-aware render draw) in place
  — no new module; 3.3.0 Phase AL added two modules — `asset_format.cyr` (format-id
  mapping) + `asset_load.cyr` (DDS/KTX2 parsers + the PNG/sniffer load API) — and
  the `chitra` dep at `lib/chitra.cyr`).
- **Tests**: **4494 CPU-only assertions** across **17 functionality-named
  domain files** under `tests/tcyr/` (3.3.0 added `asset_load.tcyr`, the 17th;
  3.4.0 Phase AA grew backend/native/asset_load in place — no new file;
  reorganized 2026-06-15 from the old
  version-named mabda/mabda_v3/mabda_v3_phase_d trio — see the v3.1 test
  reorg). Each file is a standalone suite (own `main()` + `assert_summary`)
  mirroring the `src/` domains; `make test` globs them all.
  **Counting gotcha:** `texture.tcyr`'s summary line has a leading NUL byte, so
  `make test | grep` (or `awk`) treats it as binary and **silently drops
  texture's 207** — the naive total reads **4287**, not the true **4494**. Use
  `./scripts/count-test-assertions.sh` (it strips NULs + runs per-file) for an
  accurate count; the trap has fooled humans and review agents alike.
  Domain breakdown:
  - `core` 160 (error/color/capabilities/profiler/resource/debug/obs +
    `int_ratio_to_f32` / `f64_to_f32` f32 conversions — native `f32_from`/`f32_to`
    builtins since the 6.2.18 fold-in; the former x86 SSE2 shims were retired +
    F.1–F.3/F.7-flip `shader_f64` caps: native = 1 post-flip)
  - `buffer` 53 · `compute` 26 · `texture` 207 (incl. v3.2 compressed
    format table / block math / caps gating / create dispatch) · `graphics` 61
  - `render` 196 (incl. Phase R: node queue affinity + `rg_schedule` batches/
    fence-edges + the native MQ executor capture-mock) · `backend` 432 (abstraction + wgpu FFI + dispatch mocks
    + v3.2 block-aware texture write/read + X buffer-copy + TS sampleable
    slots / sampler / bind-group + wgpu-sampler descriptor builders + S SPIRV
    descriptor/validate + kind-routed shader-create slot)
  - `caches` 42 · `surface` 73 · `kms` 335 · `queue` 102 (v3.1.1
    multi-queue: layout + wgpu/native fillers + barrier + dispatch; `caches`
    adds S kind-folded `_shader_hash_n` + SPIR-V cache peers)
  - `native` 1376 (amdgpu/PM4/GFX9 ISA/native textures+render + v3.1.1
    timeline syncobjs / SDMA copy / queue fillers + v3.2 fmt create +
    X real buffers / transfer-copy / SDMA chaining + TS T#/S#/IMG-format
    builders / TS.6 COPY_TILED + TS.7 BC tiled create/write-read/params +
    per-format DST_SEL + TS.8 scaled FS / bind S# rebuild / desc-cpu-addr /
    sampleable + caps `native_texfmt_sampleable` + N.5d the generic +
    rsrc-parameterized compute composers + N.6r the magic-tagged compiled
    shmod struct / kind gate / compiled-dispatch binding guards / tag-checked
    release / LIFO texture-VA reclaim + R.4 `native_render_dispatch_timeline`
    guard ladder + R.5 IB-slot staging cursor/isolation)
  - `compiler` 1016, **split across 4 files** (2026-06-18) sharing the `compiler_common.cyr`
    `_spv_build_*` fixtures: `compiler_encode` 259 (N.0 oracle + N.1 parse + F.7 f64 encoders)
    · `compiler_lower` 394 (N.2 MIR + lowering + F.7 f64 lower/select) · `compiler_backend` 255
    (N.3 isel + N.4 regalloc/waitcnt + N.5 emit + F.7 f64 pairs/forced-divergent) ·
    `compiler_compile` 108 (ABI + `gfx9_compile` e2e + capacity/hardening rejects + F.7 f64 e2e).
    (Phase N SPIR-V→GFX9: N.0 `gfx9_encode` oracle + N.1
    `spirv_parse` rejection gate; N.2 MIR + uniformity sweep + SPIR-V lowering
    (ALU/access-chain/GID-expansion); N.3 isel SALU/VALU; N.4 regalloc reuse +
    `s_waitcnt` use-before-wait invariant; N.5 encode (byte-matched to llvm-mc) +
    ABI/RSRC (downsample `0x2C0083`/`0x18C` oracles) + the top-level `gfx9_compile`
    driver compiling the gid/SAXPY fixtures end-to-end + N-HARDEN.1 table-builder
    cap-gate rejection / over-capacity reject; **3.2.8** adds N.7 control flow
    (uniform + divergent `if` encode/isel/emit) + N.8 op breadth (signed compares,
    GLSL min/max/sqrt/floor/fma/clamp/abs, VOP3-literal/store/float-inline/const-fold
    constants); **3.2.9** adds N.9 int div/mod (the float-reciprocal udiv primitives +
    oracle, the `_signed_prep` ASHR primitive, and the `test_spirv_lower_smod` 37-instr
    floored-modulo structure); **3.2.10** adds N.10 vectors (`MIR_VK_VECTOR` construct/extract,
    component-wise binop + fold, vec load/store + std430 stride + ArrayStride gate, constant
    composite, the mir_emit vector-operand guard); **3.2.11** adds N.11 per-ext-set tracking
    (the GLSL.std.450 import resolver + the OpExtInst set-validation gate); **3.2.12** adds
    Phase F f64 (the `V_*_F64` encoder oracles, MIR_T_F64 64-bit-pair regalloc, the FDiv
    reciprocal-Newton + FSqrt + i32↔f64 cvt + f64-const-materialization lowering, the
    forced-DIVERGENT guard, and the OpSelect-mask-enforcement reject). Every stage's
    adversarial-review regressions live here too)
  Splitting also kept every file under the 128 KiB lint/fmt cap (the old
  `mabda_v3.tcyr` had grown to 148 KiB, past the cap).
  Plus GPU integration programs: `phase0`, `compute_e2e`,
  `render_e2e`, `render_graph_e2e`, `spirv_e2e` (3.2.4 — SPIR-V vs WGSL
  cross-source identity) for wgpu; `native_compute_store`,
  `native_texture_e2e`, `native_render_e2e`, `native_mipmap_e2e`
  (v3.1 — mip-chain generate verified vs a CPU box-filter on Cezanne),
  and the v3.1.1 multi-queue set `native_queue_compute_e2e` /
  `native_queue_barrier_e2e` / `native_sdma_copy_e2e` /
  `native_multiqueue_e2e` (3-ring compute→barrier→graphics + SDMA
  consume, all HW-verified on Cezanne) for native; the v3.2 TS sampling set
  `native_texture_sample_e2e` (TS.5 RGBA8), `native_sdma_tiled_roundtrip` +
  `native_tiled_texture_roundtrip` (TS.6/7 tiling), `native_compressed_sample_e2e`
  (TS.7 BC1/3/4/5/7 vs CPU decode), `native_bilinear_sample_e2e` (TS.8 POINT-vs-
  BILINEAR), all HW-verified on Cezanne; the v3.2.12 Phase F **f64** set
  `native_f64_fma_e2e` (F.4–F.6 hand-authored), `native_spirv_f64_{fma,arith,cvt,subabs,
  vec,div,sqrt,i32_cvt,const,ldexp,select}_e2e` (the compiled toolkit), and
  `native_spirv_f64_layernorm_e2e` (F.9 public-API smoke) — all bit-exact on Cezanne —
  plus the wgpu `f64_compute_e2e` (F.8b ShaderF64); the v3.2.14 Phase R
  `native_render_graph_mq_e2e` (R.7 — a compute→render cross-queue graph on distinct
  COMPUTE/GFX rings, in-CS-timeline-ordered, both legs verified, HW-verified on Cezanne);
  plus `native_kms_summary`, `native_kms_modeset_smoke`, `native_present_e2e`
  for Phase D.
- **Benchmarks**: `tests/bcyr/mabda.bcyr` — 9 CPU benches. GPU
  benches via `make bench-gpu` (13 benches, Rust v1 parity set on
  the wgpu path). Reference Rust numbers in
  `docs/benchmarks-rust-v-cyrius.md`. Native-on-AMD bench cell
  is Tier 3 work pending a consumer flip.
- **Dist bundle**: `dist/mabda.cyr` — ~22,300 lines.
  `cyrius distlib` regenerates it.
- **Integration**: consumed by soorat, rasa, ranga, bijli, aethersafta,
  kiran (via soorat). Six-consumer regression sweep is Tier 2 ship
  work.

## Why v3.0 ships dual

- **Backend abstraction layer** — `Backend` struct (208 bytes,
  25 fnptr slots) + `backend_wgpu_new` / `backend_native_new`
  fillers. Public API (`gpu_buffer_*` / `gpu_compute_dispatch` /
  `gpu_texture_*` / `gpu_render_*` / `gpu_surface_*`) routes through
  `ctx->backend->slot` via fncall1/2/3/5 — same call-site shape on
  both backends, the slot impls differ.
- **WGSL → GFX9 lowering deferred to v3.x.** The earlier "v3.0
  ship-blocker" framing was wrong. `gpu_shader_module_create` is
  byte-polymorphic at the backend boundary — wgpu reads bytes as
  WGSL UTF-8, native reads as pre-compiled GFX9 ISA. Consumers
  ship two-form bundles in v3.0; in-mabda WGSL frontend is a v3.x
  pure-Cyrius project. See
  `docs/proposals/v3-wgsl-frontend-choice.md`.
- **Logind master gate solved by samvada.** `gpu_surface_configure_native_logind`
  routes through `samvada_session_take_device` for the master fd;
  `gpu_surface_configure_native_kiosk` is the alternate path where
  the caller manages master themselves.

## Consumers

| Project      | Usage                                       |
|--------------|---------------------------------------------|
| soorat       | Renderer — textures, pipelines, render pass |
| rasa         | Image editor — compute shaders, textures    |
| ranga        | Image processing — buffers, compute         |
| bijli        | EM simulation — compute, storage buffers    |
| aethersafta  | Desktop compositor — surfaces, present      |
| kiran        | Game engine (via soorat)                    |

## Dependencies

- **Cyrius stdlib** — `string`, `fmt`, `alloc`, `vec`, `str`, `io`,
  `args`, `hashmap`, `syscalls`, `tagged`, `fnptr`, `mmap`, `dynlib`,
  `sakshi` (ships with Cyrius >= 5.4.7), plus `thread` + `sankoch`
  (added v3.3.0 — chitra's transitive deps: mutex + zlib/DEFLATE inflate)
- **`chitra` (AGNOS dep)** — pure-Cyrius PNG decoder (sibling package).
  Pinned via `[deps.chitra] tag = "0.1.0"`; bundle at `lib/chitra.cyr`.
  `asset_load.cyr`'s `gpu_texture_load_png` decodes through it to RGBA8.
  Its bundle excludes its own stdlib deps, so mabda provides `thread` +
  `sankoch` (above). **Consumers of mabda's dist must likewise add
  `[deps.chitra]` + `thread`/`sankoch`** (the samvada-style pattern).
- **`samvada` (AGNOS dep)** — Cyrius dbus client for logind master
  delegation. Pinned via `[deps.samvada] tag = "0.4.1"` in
  `cyrius.cyml`. mabda doesn't link libsystemd directly — consumer
  programs link `samvada/deps/samvada_main.c` which calls into
  `samvada_main(table)` to populate the static fn-table that
  mabda's `_native_logind` slot reads from.
- **wgpu-native v29** — external C library, downloaded by consumers
  alongside their `deps/wgpu_main.c` launcher. Not a Cyrius dep.
- **libsystemd** — needed by samvada's C shim (`samvada_main.c`).
  Consumer-provided link, not a mabda direct dep. Drops at v4.0
  alongside wgpu-native.

All Cyrius deps are pinned in `cyrius.cyml`. `cyrius deps` resolves
them against the installed toolchain.

### Dependency wiring (HARD RULE)

`lib/` is a **real directory** populated by `cyrius deps` — it contains
per-module copies of the stdlib files declared in `[deps].stdlib`, plus
symlinks into `~/.cyrius/deps/<pkg>/<ver>/dist/` for bundled deps
(`mabda.cyr`, `patra.cyr`, `sakshi.cyr`, `sigil.cyr`, etc.). It is
gitignored (`/lib/` in `.gitignore`) — a build artifact, not source.

**NEVER** replace `lib/` with a symlink to a cyrius checkout (e.g.
`ln -s /home/macro/Repos/cyrius/lib lib`) or to `~/.cyrius/lib`. The
repo previously shipped exactly that symlink and it caused a recurring
corruption bug: any agent working in this repo (formatting, linting,
refactoring, dead-code cleanup) that wrote to `lib/<anything>.cyr`
actually wrote through the symlink into the **cyrius** repo. `mabda`
has no visibility into who else `include`s those files, so a dead-code
pass against `lib/dynlib.cyr` would silently delete fns that
`cyrius/lib/fdlopen.cyr` depends on. CI then broke in the cyrius repo,
not mabda's — making the root cause invisible.

Legitimate setup — both CI and local dev — is:

```
rm -rf lib && mkdir lib && cyrius deps
```

Never edit `lib/*.cyr` by hand. If the stdlib needs a fix, fix it in
the `cyrius` repo, cut a release, bump `cyrius = "x.y.z"` in
`cyrius.cyml`, re-run `cyrius deps`.

## Quick Start

```bash
cyrius deps                                          # resolve stdlib + samvada into lib/
cyrius build programs/smoke.cyr build/mabda_smoke    # link-check
make test                                            # 4276 CPU assertions across 17 domain files
cyrius bench tests/bcyr/mabda.bcyr                   # 9 CPU benchmarks
cyrius distlib                                       # → dist/mabda.cyr
make test-gpu                                        # wgpu integration programs (needs wgpu-native)
make test-native-compute-store                       # native compute (needs amdgpu)
make test-native-render-e2e                          # native render (HW-gated; cache-flush in tree)
make test-native-kms-summary                         # KMS topology probe (works in any session)
make test-native-kms-modeset                         # native modeset (HW + DRM master)
make test-native-present-e2e                         # 120-frame animated present (HW + master)
make bench-gpu                                       # 13 GPU benchmarks (wgpu only today)
```

**Before tripping toolchain wires**: read
[`docs/development/2026-04-30-toolchain-issues.md`](docs/development/2026-04-30-toolchain-issues.md) —
consolidated cheat-sheet of cyrius lint/fmt 128 KiB cap, fncall6
ABI bug, `var X;` rejection, global init order, logical right shift,
bump allocator exhaustion in tests. Cross-references the deeper
`docs/development/issues/` filings + memory notes.

## Architecture (flat — matches yukti / vidya)

```
mabda/
├── src/                 49 GPU library modules — flat, zero transitive includes
│   ├── lib.cyr                      — single include chain (stdlib + domain modules + samvada)
│   ├── error.cyr                    — GpuErr codes + Result helpers
│   ├── color.cyr                    — f64-backed RGBA colour type
│   ├── capabilities.cyr             — WebGPU limit detection
│   ├── profiler.cyr                 — CPU-side frame timing, EMA, history
│   ├── resource.cyr                 — RAII-ish buffer/texture lifetime tracker
│   ├── wgpu_types.cyr               — @internal: usage/format/shader-stage enums
│   ├── texture_format.cyr           — public MABDA_TEXFMT_* (BC/ETC2/ASTC) +
│   │                                  block geometry/size math + caps gating (v3.2 T)
│   ├── wgpu_descriptors.cyr         — @internal: packed descriptor builders
│   ├── wgpu_ffi.cyr                 — @internal: 65-entry wgpu fn-pointer table
│   ├── backend.cyr                  — @internal: Backend struct (208 B, 25 slots)
│   │                                  + BACKEND_KIND_* + null-slot helpers
│   ├── backend_wgpu.cyr             — @internal: wgpu fillers for all 25 slots
│   ├── context.cyr                  — GpuContext (120 B; dual-interpretation
│   │                                  +0..+24 + backend ptr + native cache + surface stash
│   │                                  + PM4 scratch slot)
│   ├── backend_native_amdgpu.cyr    — @internal: DRM/AMDGPU/GEM/syncobj/CS-submit
│   │                                  ioctl wrappers (foundational layer, no PM4 deps)
│   ├── gfx9_encode.cyr              — @internal: operand-parameterized GFX9 instruction
│   │                                  encoders + emit helpers (v3.2.5 N.0; builders emit through these)
│   ├── spirv_parse.cyr              — @internal: SPIR-V parser (v3.2.5 N.1) — validate gate +
│   │                                  type/constant/decoration lookup tables
│   ├── mir.cyr                      — @internal: SSA MIR model + GFX9 uniformity sweep (N.2)
│   ├── spirv_lower.cyr              — @internal: SPIR-V → MIR lowering + GID expansion (N.2)
│   ├── gfx9_isel.cyr                — @internal: MIR → virtual-reg GFX9 selection (N.3)
│   ├── gfx9_regalloc.cyr            — @internal: linear-scan VGPR/SGPR, no-spill (N.4)
│   ├── gfx9_waitcnt.cyr             — @internal: s_waitcnt insertion (N.4)
│   ├── gfx9_abi.cyr                 — @internal: canonical compute ABI + RSRC1/RSRC2 (N.5)
│   ├── gfx9_compile.cyr             — @internal: encode driver + top-level gfx9_compile (N.5)
│   ├── backend_native_shaders.cyr   — @internal: GFX9 ISA shader builders + GFX9
│   │                                  graphics register addresses + value minimums
│   ├── backend_native_pm4.cyr       — @internal: PM4 packet primitives + compute +
│   │                                  render PM4 stream composers (pure byte builders)
│   ├── backend_native.cyr           — @internal: native AMD slot fillers + dispatch
│   │                                  drivers + native_texture/_rt/_render_pipeline +
│   │                                  ctx accessors + backend_native_new()
│   ├── backend_native_kms.cyr       — @internal: KMS surface ioctls (modeset/page-flip/PRIME)
│   ├── buffer.cyr                   — public gpu_buffer_* dispatch through ctx->backend
│   ├── typed_buffer.cyr             — uniform/storage buffer metadata
│   ├── gpu_timestamps.cyr           — TIMESTAMP_QUERY feature wiring
│   ├── compute.cyr                  — public gpu_compute_dispatch + ping-pong helpers
│   ├── shader_cache.cyr             — WGSL source → module cache (u64-keyed, v2.4.5)
│   ├── pipeline_cache.cyr           — u64 hash → pipeline cache
│   ├── bind_group_cache.cyr         — u64 hash → bind group cache
│   ├── vertex.cyr                   — Vertex2D / 3D layouts
│   ├── blend.cyr                    — BLEND_* constants
│   ├── sampler.cyr                  — sampler descriptor builder
│   ├── depth.cyr                    — depth-stencil state + depth-texture helpers
│   ├── bind_group.cyr               — BGL builder
│   ├── texture.cyr                  — public gpu_texture_* dispatch through ctx->backend
│   ├── render_target.cyr            — public gpu_render_target_* + RenderTargetBuilder
│   ├── render_pipeline.cyr          — public gpu_render_pipeline_* + builder + create_simple
│   ├── render_pass.cyr              — public gpu_render_pass_* + RenderPassBuilder
│   ├── render_graph.cyr             — DAG pass orchestration (v2.5.0)
│   ├── surface.cyr                  — v2 wgpu surface_state_* lifecycle
│   ├── surface_v3.cyr               — public gpu_surface_* dispatchers (Step 7.7):
│   │                                  configure_wgpu / _native_kiosk / _native_logind +
│   │                                  acquire / present / release
│   ├── queue.cyr                    — public gpu_queue_* (v3.1.1): logical
│   │                                  queue abstraction (kind→ring) + ctx
│   │                                  queue table + barrier/wait_idle dispatch
│   ├── instancing.cyr               — instance buffer + identity helpers
│   └── debug.cyr                    — push/pop debug markers
├── tests/
│   ├── tcyr/                        — 16 functionality-named domain suites
│   │   │                              (4276 asserts total; `make test` globs
│   │   │                              `tests/tcyr/*.tcyr`). Each standalone
│   │   │                              (own main + assert_summary), self-
│   │   │                              contained (needed mocks inlined).
│   │   ├── core.tcyr  buffer.tcyr  compute.tcyr  texture.tcyr
│   │   ├── graphics.tcyr  render.tcyr  backend.tcyr  caches.tcyr
│   │   ├── surface.tcyr  native.tcyr  kms.tcyr  queue.tcyr
│   │   ├── compiler_encode.tcyr   (Phase N: N.0 gfx9_encode oracle + N.1 parse)
│   │   ├── compiler_lower.tcyr    (N.2 MIR + SPIR-V→MIR lowering + breadth)
│   │   ├── compiler_backend.tcyr  (N.3 isel + N.4 regalloc/waitcnt + N.5 emit)
│   │   ├── compiler_compile.tcyr  (ABI + gfx9_compile e2e + capacity rejects)
│   │   └── compiler_common.cyr    (@shared, not a .tcyr — `_spv_build_*` fixtures
│   │                               + include chain for the four compiler_*.tcyr)
│   └── bcyr/mabda.bcyr              — CPU benchmark harness (9 benches)
├── programs/
│   ├── smoke.cyr                    — link-check for the full include chain
│   ├── phase0.cyr                   — wgpu GPU smoke (buffer/texture/pipeline)
│   ├── compute_e2e.cyr              — wgpu compute dispatch round-trip
│   ├── render_e2e.cyr               — wgpu render pass clear + pixel verify
│   ├── render_graph_e2e.cyr         — wgpu 3-node DAG (compute → render → copy)
│   ├── benchmarks.cyr               — 13 GPU benches, Rust-v1 parity (wgpu)
│   ├── native_compute_store.cyr     — native compute, write 0xDEADBEEF + readback
│   ├── native_texture_e2e.cyr       — native texture round-trip
│   ├── native_render_e2e.cyr        — native render: clear-triangle + pixel verify
│   ├── native_mipmap_e2e.cyr        — native mip-chain generate vs CPU box-filter
│   ├── native_spirv_compute_e2e.cyr — N.5d: compile SPIR-V → GFX9 → dispatch on GPU (HW-verified)
│   ├── native_spirv_saxpy_e2e.cyr    — N.6: novel 2-binding SAXPY compiled + dispatched (HW)
│   ├── native_spirv_downsample_e2e.cyr — N.5g: 2×2 box-filter downsample, pixel-match (HW)
│   ├── native_spirv_public_api_e2e.cyr — N.6r: SPIR-V via gpu_shader_module_create_spirv + gpu_compute_dispatch (HW)
│   ├── native_spirv_2d_dispatch_e2e.cyr — N.6: 2-D/TGID compiled grid (HW)
│   ├── native_spirv_queue_dispatch_e2e.cyr — N.6: compiled kernel on a COMPUTE queue (HW)
│   ├── native_spirv_uniform_if_e2e.cyr — 3.2.8 N.7b: uniform `if` (s_cbranch_scc0), wg gated (HW)
│   ├── native_spirv_divergent_if_e2e.cyr — 3.2.8 N.7c: divergent `if` (EXEC mask), per-lane (HW)
│   ├── native_spirv_mul_literal_e2e.cyr — 3.2.8 N.8a: gid*100, VOP3-literal materialize (HW)
│   ├── native_spirv_glsl_max_e2e.cyr   — 3.2.8 N.8b-1: GLSL FMax via OpExtInst (HW)
│   ├── native_spirv_fma_e2e.cyr / native_spirv_fma_const_e2e.cyr — N.8b-2/3: Fma (3-src) + float inline const (HW)
│   ├── native_spirv_fclamp_e2e.cyr / native_spirv_signed_if_e2e.cyr — N.8b-4: FClamp (v_med3) + signed compare (HW)
│   ├── native_spirv_store_const_e2e.cyr / native_spirv_vop2_const_e2e.cyr — N.8b-5/6: store-const + VOP2 const (HW)
│   ├── native_spirv_fabs_e2e.cyr / native_spirv_const_fold_e2e.cyr — N.8b-7/8: FAbs + const-fold (HW)
│   ├── native_spirv_udiv_e2e.cyr / native_spirv_umod_e2e.cyr — 3.2.9 N.9b-2/c: u32 div/rem (HW)
│   ├── native_spirv_sdiv_e2e.cyr / native_spirv_srem_e2e.cyr — 3.2.9 N.9d-1: signed div/rem (HW)
│   ├── native_spirv_smod_e2e.cyr — 3.2.9 N.9d-2: floored modulo (divisor sign, HW)
│   ├── native_spirv_vector_add_e2e.cyr — 3.2.10 N.10b: vec3 construct + add/mul + extract (HW)
│   ├── native_spirv_vector_load_store_e2e.cyr — 3.2.10 N.10c: array<vec4> load/op/store (HW)
│   ├── native_spirv_vector_const_e2e.cyr — 3.2.10 N.10d: OpConstantComposite vec4 + add (HW)
│   ├── native_compressed_store_e2e.cyr — v3.2: native BC1/BC7 store round-trip
│   ├── compressed_texture_e2e.cyr   — v3.2: wgpu BC1 create+upload+copy-back (verify)
│   ├── native_queue_compute_e2e.cyr — v3.1.1: compute on a COMPUTE queue + timeline
│   ├── native_queue_barrier_e2e.cyr — v3.1.1: cross-ring barrier (COMPUTE→GFX in-CS wait)
│   ├── native_sdma_copy_e2e.cyr     — v3.1.1: SDMA COPY_LINEAR on the DMA ring
│   ├── native_multiqueue_e2e.cyr    — v3.1.1: 3-ring compute→barrier→graphics + SDMA consume
│   ├── native_render_graph_mq_e2e.cyr — v3.2.14 R.7: compute→render cross-queue graph, distinct rings (HW)
│   ├── native_kms_summary.cyr       — KMS topology probe (Phase D, no-master)
│   ├── native_kms_modeset_smoke.cyr — native modeset visual smoke (red screen)
│   └── native_present_e2e.cyr       — 7.7 e2e: 120-frame animated gradient
├── dist/mabda.cyr                   — bundle for [deps.mabda] consumers
├── deps/
│   ├── wgpu_main.c                  — C launcher: wgpu fn table + struct-packing shims
│   └── wgpu-native/                 — external C binaries (gitignored)
├── lib/                             — populated by `cyrius deps` (gitignored):
│   ├── string.cyr / fmt.cyr / ...   — Cyrius stdlib copies
│   └── samvada.cyr                  — symlink into ~/.cyrius/deps/samvada/0.4.1/dist/
├── cyrius.cyml                      — package manifest + [lib] + [deps] + [deps.samvada]
├── Makefile                         — wrapper over `cyrius` CLI + GPU paths + native programs
└── VERSION                          — source of truth, templated into manifest
```

## FFI Architecture

mabda has two GPU paths and one auxiliary dbus path; each uses the
fn-table-via-C-shim pattern.

### wgpu path (`deps/wgpu_main.c`)

1. C `main()` calls `_cyrius_init()` then `alloc_init()`
2. C pre-initializes GPU (instance/adapter/device/queue — Vulkan-only
   via `WGPUInstanceExtras { backends = Vulkan }`; see v2.4.2 CHANGELOG
   for why the default `All` was a problem on headless boxes)
3. C builds the function-pointer table (65 wgpu functions +
   7 struct-packing shims)
4. C calls `mabda_main(fn_table_ptr, preinit_ptr)` which the consumer defines
5. Cyrius calls wgpu via `fncall1`/`fncall2`/`fncall5` and struct-packed
   shims — **never** `fncall6` with a struct-by-value arg

### Native AMD path (no C shim)

Compute / render / surface ioctls go directly through
`syscall(SYS_IOCTL)` — no C library, no libdrm. Pure Cyrius.
Mappings:

- **`/dev/dri/renderD128`** for compute + render allocator (BO
  create / mmap / GEM close / VA map). AMDGPU-specific ioctls
  (GEM_CREATE / CTX / BO_LIST / GEM_VA / CS) routed through fd
  with no master required.
- **`/dev/dri/cardN`** for KMS surface (MODE_GETRESOURCES /
  MODE_GETCONNECTOR / MODE_GETENCODER / ADDFB2 / SETCRTC /
  PAGE_FLIP). Requires DRM master — see `samvada` path for the
  in-session master story.
- **PRIME bridge** between the two fds for the surface FB story
  (see `phase_d_prime_cross_fd_handle_bridge` vidya entry).

### samvada path (`samvada/deps/samvada_main.c`)

Consumer programs that use `gpu_surface_configure_native_logind`
link `samvada/deps/samvada_main.c` alongside their wgpu launcher.
Same fn-table pattern:

1. C `main()` builds the samvada 9-slot table (sd_bus_*) and calls
   `samvada_main(table)` to populate samvada's static reference.
2. Cyrius calls `samvada_session_take_device(major, minor)` etc.
   via fncall through the table.
3. mabda's `_backend_native_surface_configure_logind` slot reads
   the master fd back from samvada and stashes on
   `gpu_ctx_native_card_fd` for the slot dispatch.

### CPU testing (no GPU, no master, no dbus)

`cyrius test` runs all the `tests/tcyr/*.tcyr` files against
`src/lib.cyr` — no wgpu-native, no amdgpu hardware, no libsystemd
needed. Backend-abstraction routing exercised via mock-fnptr
sentinels; native ioctls / wgpu calls / sd_bus calls all surface
as null-safety + struct-shape tests at the Cyrius layer. HW gates
live in the `programs/native_*.cyr` programs.

## Key Constraints

- **Tests are the way** — 4276 CPU assertions across 17 domain test
  files + a dozen GPU/HW programs. Every new code path adds an
  assertion. Stack-local `var ctx[112]` for test-scoped buffers
  (heap-allocated tests exhaust the bump allocator — see
  `bump_allocator_exhaustion_in_tests` vidya entry).
- **Own the stack** — every external dep is either an AGNOS package
  (samvada, sakshi, patra, sigil) or a consumer-provided C library
  (wgpu-native, libsystemd-via-samvada). wgpu-native and libsystemd
  both retire at v4.0; the trajectory is pure-Cyrius all the way
  down.
- **No magic** — every operation measurable, auditable, traceable.
- **Manual memory** — `alloc / store64 / load64`. Every struct has a
  header comment block with field offsets.
- **Tagged unions for errors** — `Ok(value)` / `Err(gpu_err(...))` via
  `lib/tagged.cyr`.
- **f64 internally, f32 at the GPU boundary** — use `f64_to_f32` only
  when writing to a GPU buffer (a thin wrapper over the native `f32_from`
  builtin since 6.2.18; `src/` carries zero inline asm).
- **Prefix private helpers with `_`** — public API uses descriptive names.
- **Struct-pack wgpu args with 6+ parameters.** Cyrius `fncall6` +
  wgpu-native segfaults. Wrap via a C shim in `deps/wgpu_main.c`
  that takes `(handle, struct_ptr)` and call via `fncall2` — see
  `wgpu_command_encoder_copy_buffer_to_buffer` and `wgpu_buffer_map_sync`
  for the canonical pattern.
- **6-parameter ceiling for Cyrius fns that fncall into wgpu.** Pure
  Cyrius functions can take 12+ args without issue, but the moment one
  internally `fncall*`s into wgpu-native, any signature with 7+ params
  reliably segfaults. Fold into a struct pointer or split. See
  `fncall6_ceiling_into_extern_c` vidya entry.
- **`var X = expr;` initialization required.** Cyrius rejects bare
  `var X;` declarations — every var needs an initializer. Use
  `var X = 0;` for "to-be-set-later" pattern.
- **`gpu_shader_module_create` is byte-polymorphic.** wgpu reads as
  WGSL, native reads as pre-compiled GFX9 ISA. v3.0 ships consumer
  two-form bundles; in-mabda WGSL → GFX9 lowering is v3.x scope.
- **Phase D ioctl ordering matters.** Discovery → mode-pick → encoder
  → CRTC → AddFB2 → SETCRTC → PAGE_FLIP. Skipping discovery and
  hardcoding IDs breaks across reboots. See
  `phase_d_kms_sequencing` vidya entry.

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, audit history — know what was intended
1. Cleanliness: `cyrius build programs/smoke.cyr` (0 warnings),
   per-file `cyrius lint src/*.cyr` (0 warnings; the bare repo-wide
   form was removed in 5.7.x — see
   `feedback_cyrius_lint_fmt_per_file` memory),
   `cyrius vet programs/smoke.cyr` clean
2. Test sweep: 4276+ assertions pass across all 17 domain test files,
   `cyrius distlib` diff-clean
3. Benchmark baseline: `cyrius bench tests/bcyr/mabda.bcyr`, save CSV
4. Internal deep review — gaps, optimizations, correctness, docs
5. External research — wgpu-native / WebGPU / GPU-driver CVE sweep
   since last pass
6. Security audit (see below) — file findings in
   `docs/audit/YYYY-MM-DD-audit.md`
7. Additional tests from findings — each HIGH/MED fix lands with an
   assertion that would have caught the original bug
8. Post-review benchmarks — prove the wins (if any)
9. Documentation audit — CLAUDE.md, roadmap, CHANGELOG, audit index
10. Repeat if heavy

### Work Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check — `make test` (globs all `tests/tcyr/*.tcyr`)
3. Test + benchmark additions for new code
4. Internal review — performance, memory, correctness
5. If any FFI / buffer / texture math changed: re-run the audit
   checklist against the diff
6. Documentation — update CHANGELOG, roadmap, docs
7. Version check — `./scripts/version-check.sh` passes
8. Return to step 1

### Security Hardening (before release)

1. **Input validation** — every function accepting consumer-supplied
   data (buffer sizes, texture dimensions, workgroup counts,
   descriptor fields, label strings) validates bounds, types, ranges
   before use
2. **Buffer safety** — every `var buf[N]` and `alloc(N)` verified:
   N in bytes, max offset < N, no adjacent-allocation overflow. The
   struct header comment's byte count must match the actual `alloc`
3. **Integer overflow** — any `a * b` / `a + b` / `a << n` on sizes
   or dimensions gets an overflow guard before use, especially in
   texture / buffer / workgroup math
4. **Divide-by-zero** — any `/` or `%` verifies the divisor is
   non-zero before the operation (workgroup helpers were the
   regression case in 2.3.0)
5. **Syscall return handling** — every `syscall()` return value is
   checked; error paths either recover or deterministically zero
   any output buffer the caller will read
6. **Pointer validation** — no raw deref of consumer-supplied
   pointers; label strings use `wgpu_string_view_len` with an
   explicit length when length is known
7. **FFI descriptor offset review** — every edit to
   `wgpu_descriptors.cyr` cross-referenced against the v29
   `webgpu.h` layout; field offsets noted in the module header
   comment block
8. **`fncall6` avoidance** — any wgpu-native call taking 6+ i64
   arguments goes through a struct-packing shim in
   `deps/wgpu_main.c`; direct `fncall6` reliably crashes against
   wgpu-native (see `feedback_fncall6_wgpu` memory)
9. **Known CVE check** — review against current wgpu-native /
   WebGPU / GPU-driver CVEs since the prior audit
10. **File findings** — `docs/audit/YYYY-MM-DD-audit.md` with
    severity, file, line, class, mitigation

Severity levels: **CRITICAL** (exploitable immediately) / **HIGH**
(moderate effort) / **MEDIUM** (specific conditions) / **LOW**
(defense-in-depth).

### Task Sizing

- **Low/Medium effort**: batch freely
- **Large effort**: small bites — break into sub-tasks, verify each
- **If unsure**: treat as large

### Closeout Pass (before every minor/major bump)

1. Full CPU suite — `make test` runs all three files
   (all 17 `tests/tcyr/*.tcyr` domain suites); 4276+
   asserts pass.
2. Bench baseline — `cyrius bench tests/bcyr/mabda.bcyr`
3. GPU integration (wgpu) — `make test-phase0` passes on a box with
   wgpu-native
4. GPU integration (native) — `make test-native-compute-store`
   passes on a box with amdgpu (HW-gated; requires AMD render node).
   `make test-native-render-e2e` and Phase D programs run from a
   tty / kiosk session OR with samvada+logind wired through a
   consumer.
5. `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean
6. Version consistency — `./scripts/version-check.sh` passes
7. Consumer check — soorat, rasa, ranga, bijli, aethersafta still build
   against the new bundle (Tier 2 ship work)
8. Audit index up to date — `docs/audit/` has the current
   `YYYY-MM-DD-audit.md` referenced from CHANGELOG

## CI / Release

- **Toolchain pin**: `cyrius = "6.2.29"` in `cyrius.cyml`. CI + release
  both read from the manifest — no hardcoded versions in YAML.
- **Tag filter**: release workflow triggers on `v[0-9]+.[0-9]+.[0-9]+`
  and `[0-9]+.[0-9]+.[0-9]+`. Version-verify step asserts
  `VERSION == git tag`.
- **Lint/fmt/vet gates**: CI fails on any `cyrius lint` warning,
  `cyrius fmt --check` drift, or `cyrius vet` finding.
- **Dist gate**: CI runs `cyrius distlib` and fails if
  `dist/mabda.cyr` drifts from the committed copy.
- **Smoke build**: `cyrius build programs/smoke.cyr` — proves the
  full include chain links.
- **Test/bench**: `make test` (globs all `tests/tcyr/*.tcyr` domain suites
  files) + `cyrius bench tests/bcyr/mabda.bcyr`.
- **GPU integration is local only** — CI runners don't have
  wgpu-native or amdgpu hardware; `make test-phase0` /
  `make test-native-*` are developer gates.

## CHANGELOG Format

```markdown
## [X.Y.Z] — YYYY-MM-DD
### Added — new features
### Changed — changes to existing features
### Fixed — bug fixes
### Breaking — breaking changes with migration guide
```

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies (wgpu-native is the exception,
  consumer-provided)
- Do not skip `cyrius test` before claiming changes work
- Do not commit `build/`, `deps/wgpu-native/`, or `deps/*.o`
- Do not call wgpu-native functions with 6+ i64 args via `fncall6` —
  always go through a struct-packing shim in `deps/wgpu_main.c`
- Do not add Cyrius stdlib includes in individual `src/*.cyr` files —
  `src/lib.cyr` owns the whole include chain
- Do not hardcode Cyrius toolchain versions in CI YAML — read
  `cyrius.cyml`
- Do not shell out to `cc5` directly for library code — go through
  `cyrius <subcommand>`. The one exception is `programs/phase0.cyr`,
  which the Makefile compiles with `printf 'object;\n' | cc5` because
  it needs to be linked against the C launcher.
