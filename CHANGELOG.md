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

### Security — Phase N-HARDEN.1 (unchecked OOB write in the SPIR-V table builders)
- **The SPIR-V→GFX9 table builders no longer trust the header `id_bound`.**
  `spirv_build_type_table` / `_const_table` / `_decoration_table`
  (`src/spirv_parse.cyr`) index a caller-provided buffer by every result `<id>`
  using the (untrusted) `id_bound`, and previously took no buffer-capacity
  parameter — so a crafted oversized `id_bound` (the validate gate only caps it at
  a loose `0x40000000`) ran the up-front `memset` and every per-id write past the
  buffer, silently corrupting memory and surfacing much later as a spurious
  `MIR_ERR_ID_OOR`. Each builder now takes a `cap` (table capacity in records),
  rejects `id_bound > cap` with `-SPIRV_ERR_ID_CAP` **before touching `out`**, and
  guards each per-id write (`id == 0 || id >= cap`). `gfx9_compile` threads the
  capacity from a new `CC_CAP_IDS` context field and surfaces any builder
  rejection as `CMP_ERR_TABLE`. Found via the N.5g adversarial review; it had
  already bitten development as a confusing `-25` when a caller sized its tables
  for `cap_ids` instead of `id_bound`. +8 asserts in `tests/tcyr/compiler.tcyr`
  (per-builder rejection + untouched-canary + the `gfx9_compile` integration
  reject). Closes the gap **before** N.6's `gpu_shader_module_create` SPIR-V path
  exposes the compiler to consumers.

### Added — Phase N.6 (multi-binding dispatch + a novel kernel on the GPU)
- **A compiled kernel the compiler never saw as hand-authored bytes runs on
  Cezanne with two real storage buffers.** `programs/native_spirv_saxpy_e2e.cyr`:
  a SAXPY-shape `y[lid.x] = 3*x[lid.x] + y[lid.x]` (uint, LocalSize 8) →
  `gfx9_compile` → dispatch reading `x` (binding 0 → `s0:s1`) and read-modify-
  writing `y` (binding 1 → `s2:s3`) → `y[i]` becomes `3*i + 100` for all 8 lanes.
  `make test-native-spirv-saxpy-e2e`.
- `src/backend_native_pm4.cyr` — `native_pm4_build_compute_dispatch`: a generic
  N-binding compute composer that lays each binding VA into USER_DATA per the
  compiler's ABI (binding k → `s[2k:2k+1]`) and uses the compiler's RSRC1/RSRC2
  verbatim (no scratch — a compiled MVP kernel does not spill).
- `src/backend_native.cyr` — `native_compute_dispatch_cached_n`: the cached submit
  with a variable BO list (fence + stub + each binding handle + shader + IB) so
  every storage buffer is resident.
- `tests/tcyr/native.tcyr` +4 asserts: a structural CPU test that N binding VAs
  land in the right USER_DATA slots with the compiler's RSRC. (Bounded by the
  16-SGPR USER_DATA limit ⇒ ≤8 bindings.)

### Added — Phase N.5g (a compiled image kernel — the MVP-exit downsample)
- **A 2×2 box-filter downsample, compiled in-tree from SPIR-V, pixel-matches a
  CPU box-filter on Cezanne.** `programs/native_spirv_downsample_e2e.cyr`: a
  single-channel `dst[i] = (src[a]+src[b]+src[c]+src[d]) >> 2` over each 2×2
  source block (`dst` 4×4, `src` 8×8, LocalSize 16) → `gfx9_compile` → 2-binding
  dispatch (`src` binding 0 → `s0:s1`, `dst` binding 1 → `s2:s3`) → every one of
  the 16 destination texels equals the average of its 2×2 source block. The
  reference is an **independent** CPU box-filter over the same seeded source, and
  `dst` is pre-seeded with a `0xBAADF00D` sentinel, so a no-op or wrong kernel
  fails rather than passing spuriously. Power-of-2 dims keep all index math in
  shifts/masks (`x=i&3`, `y=i>>2`, `aoff=(y<<4)+(x<<1)`) — no div/mod, which the
  compiler does not lower. The compiler handled a real image kernel (5
  `OpAccessChain` loads/store, the 2×2 offset arithmetic, accumulate, shift)
  through the full N.2–N.5 pipeline with no new compiler code — it reuses the N.6
  multi-binding composer + cached submit. `make test-native-spirv-downsample-e2e`.

## [3.2.6] — 2026-06-17

**Native SPIR-V → GFX9 compute compiler — HW-verified end-to-end on Cezanne
(Phases N.2–N.5).** Building on the N.0/N.1 foundation (3.2.5), the in-tree
compiler now lowers a SPIR-V compute kernel all the way to GFX9 ISA + a dispatch
descriptor and **runs it correctly on the AMD GPU**: SPIR-V parse → SSA MIR + GFX9
uniformity → instruction selection (SALU/VALU) → linear-scan register allocation →
`s_waitcnt` insertion → ISA encode → ABI/RSRC wiring → on-GPU dispatch. A compiled
`out[gl_LocalInvocationID.x] = lid.x*3 + 7` produced the correct per-lane results
on Cezanne (`native_spirv_compute_e2e`). All-CPU compiler, pure-Cyrius, no public
API change; toolchain pin → 6.2.18 (native f32 builtins — the hand-rolled f32 asm
shims retired). Every stage adversarially reviewed; ~20 findings fixed pre-merge.

### Added — Phase N.2a (MIR data model + GFX9 uniformity pass)
- `src/mir.cyr` — the SSA IR the SPIR-V→GFX9 compiler lowers through (stage N.2
  of `docs/proposals/v3.2-spirv-gfx9-native-lowering.md`; design via a 3-lens
  panel + synthesis). A `MirMod` header (80 B) wires four caller-provided,
  `<id>`-indexed buffers — values (40 B records: kind / distilled type /
  **uniformity** / def / payload), instructions (32 B, operands stored as SPIR-V
  `<id>`s so the SSA graph maps 1:1), an `OpAccessChain` access side-table, and
  blocks (one for the MVP, pre-shaped for N.7 control flow). Builders + accessors,
  `_mir_lower_type` (i32/u32/f32 + vec2-4; non-32-bit → `MIR_T_UNSUPPORTED`), and
  the **uniformity pass** (`_mir_meet` + a seed + single forward sweep:
  UNIFORM→SGPR/SALU vs DIVERGENT→VGPR/VALU — constants/WorkgroupId/buffer-base
  uniform, LocalInvocationId/GlobalInvocationId/divergent-offset-loads divergent).
  Pure data structures; the only dependency is the read-only N.1b type-table
  accessors, so it is testable in isolation with hand-built MIR.
- `tests/tcyr/compiler.tcyr` +87 asserts: type-distillation matrix, the
  `_mir_meet` truth table, builder/accessor round-trips, a SAXPY-shape hand-built
  MIR proving every value's GFX9 uniformity + the immediate-operand skip, block /
  synth-id helpers, the cap-exceeded / id-out-of-range error paths, plus the
  adversarial-review regression set (below).

### Fixed — N.2a adversarial review (5 confirmed findings, all fixed pre-merge)
- **Synth-id headroom / value-table OOB:** `mir_alloc_synth_id` hands out ids ≥
  `id_bound`, but the value-id ceiling and the vals[] capacity were both exactly
  `id_bound` — so synth ids were unusable (rejected by `mir_emit`/`mir_add_ptr`)
  and, via the unguarded `mir_set_*` builders, caused a 40-byte out-of-bounds
  write. Split the ceiling from `id_bound`: `mir_mod_init` gains a `cap_ids`
  param (vals sized `cap_ids * 40`); all value writes bound-check `cap_ids`.
- **`_mir_set_val` / `mir_set_*` missing bounds check:** added the id-0 sentinel
  + `cap_ids` guard the sibling builders already had (was an OOB-write gap on an
  untrusted/synth id).
- **`mir_add_ptr` accepted `result_id` 0:** clobbered the id-0 "no value"
  sentinel; now rejected.
- **`_mir_lower_type` unbounded recursion:** a crafted self-/mutually-referential
  vector component `<id>` recursed forever (untrusted-input stack-overflow DoS).
- **`_mir_lower_type` vector-of-vector corruption:** a nested-vector component
  produced a bogus non-`UNSUPPORTED` type (wrong base, lane count 6). Both fixed
  by requiring the vector component kind ∈ {INT,FLOAT,BOOL} before lowering.
- Regression asserts for all five added to `compiler.tcyr` (`test_mir_review_n2a`
  + a synth-id-as-result case).

### Added — Phase N.2b-1 (SPIR-V → MIR lowering, non-memory subset)
- `src/spirv_lower.cyr` — walks a validated SPIR-V compute module (via the N.1b
  parser + tables) and builds MIR (the N.2a model). `spirv_lower_module`
  validates the entry point, runs the **builtin-resolution gap-closing pass**
  (`_spirv_resolve_builtins` — the N.1b parser records Binding/DescriptorSet but
  NOT `OpDecorate BuiltIn`), seeds globals (constants → CONST; StorageBuffer/
  Uniform/PushConstant vars → BUFVAR with binding; Input+BuiltIn vars → BUILTIN),
  walks the entry-function body dispatching the i32/u32/f32 ALU set + conversions
  + `OpCompositeExtract` + the builtin `OpLoad` alias + `OpReturn`, then runs the
  N.2a uniformity sweep. Control flow → `LOWER_ERR_CONTROL_FLOW`, unmapped op →
  `MIR_ERR_UNSUPPORTED_OP`, missing GLCompute entry → `LOWER_ERR_NO_ENTRY` (all
  fail loud). The buffer memory path (`OpAccessChain` + buffer `OpLoad`/`OpStore`)
  is N.2b-2 and currently fails loud. Returns 0 or a negative error.
- `tests/tcyr/compiler.tcyr` +38 asserts: a GlobalInvocationId-arithmetic kernel
  lowered end-to-end (builtin resolve → load-alias → extract → ALU → uniformity,
  asserting the instruction stream + every value's class), the StorageBuffer seed
  path (on the N.1b typed-module fixture), the three fail-loud negatives
  (no-entry, control-flow, unmapped op), and the review-regression set (below).

### Fixed — N.2b-1 adversarial review (9 confirmed findings, all fixed pre-merge)
- **Untrusted-id out-of-bounds class (the big one).** `spirv_validate_stream`
  checks structure but NOT that in-instruction `<id>`s are `< id_bound`, and the
  lowering used them as raw table indices. A structurally-valid (validator-
  passing) crafted module could drive: (a) a **controlled-offset OOB write** —
  an `OpDecorate BuiltIn` target id into `out_bi[]`; (b) wild OOB **reads** — an
  ALU/extract/convert operand id (via `mir_emit` → `mir_run_uniformity`'s
  `vals[]` read), an `OpLoad` source id (`mir_val_kind`), a result-type/pointee
  id (`_mir_lower_type` → type table), and the seed-path variable id (`deco`/
  `out_bi` reads). Fixes: `mir_emit` now bounds non-immediate operands against
  `cap_ids`; `_mir_lower_type` takes `id_bound` and bounds its type ids (incl.
  the vector-component recursion); `_spirv_resolve_builtins` bounds the decorate
  target before the store; `_spirv_lower_load` bounds the source; the seed bounds
  the variable id + pointer-type id. All reject with a fail-loud error code.
- **Silent empty-body success.** An entry `<id>` with no matching `OpFunction`
  (or a body with no `OpFunctionEnd`) lowered to an empty MIR and returned OK;
  `_spirv_lower_body` now fails loud (`LOWER_ERR_NO_ENTRY`).
- Regression asserts for all of these added (`test_spirv_lower_n2b1_review`,
  5 crafted-module cases proving fail-loud + no OOB).

### Added — Phase N.2b-2 (SPIR-V → MIR lowering, buffer memory path) — completes N.2
- `spirv_lower.cyr` gains the memory path. `_spirv_lower_access_chain` (MVP shape
  pointer → struct{ runtimearray<scalar> }, exactly two indices) records a
  `(binding, byte-offset)` access in the ptr side-table: a **constant** array
  index folds to a `const_off` (overflow-guarded — `civ > 0x7FFFFFFF / stride`
  → `MIR_ERR_OVERFLOW`); a **dynamic** index emits a synth `IMUL(index, stride)`
  whose result is the offset. `_spirv_lower_load` (PTR src) and
  `_spirv_lower_store` emit `MIR_OP_LOAD` / `MIR_OP_STORE` referencing the ptr
  record (`a` = off_id so the uniformity sweep classifies the load; `b` = ptr
  index). Deeper nesting / non-zero struct member / non-pointer result-type all
  fail loud (`MIR_ERR_BAD_ACCESS_CHAIN`). The dynamic-index range-vs-array-length
  check is flagged as an N.6 / audit gate (the runtime length is not static here).
  The `consts` table threads back through the lowering (the access chain needs
  the member/array-index constant values).
- `tests/tcyr/compiler.tcyr` +42 asserts: the **full SAXPY** (`y[gid.x] =
  a*x[gid.x] + y[gid.x]`, two StorageBuffer bindings) lowered end-to-end — the
  9-instruction stream, the synth offset `IMUL`s, the two ptr records + bindings,
  and every value's uniformity (GID-indexed loads + the arithmetic all divergent)
  — plus access-chain variants (constant-index fold, offset overflow, bad member,
  and the review-regression cases below).

### Fixed — N.2b-2 adversarial review (2 confirmed findings, both fixed pre-merge)
- **vec3 (non-scalar) element stride miscomputation:** `stride = mir_type_count
  (elem) * 4` gives 12 for a `vec3` runtime-array element, but the buffer array
  stride is 16 — and nothing enforced the documented scalar-element MVP. The
  access chain now requires a scalar element (`MIR_ERR_BAD_ACCESS_CHAIN`
  otherwise); vector/aggregate elements + the `ArrayStride` decoration are
  deferred. (Latent: the wrong stride would become a GPU wrong-write once N.3
  ships.)
- **Constant array index not validated as an integer:** the const-fold path
  checked "is it a constant?" but not "is it an integer?", so a float constant's
  bits could be reinterpreted as the index. It now requires an i32/u32 const
  index (`MIR_ERR_BAD_ACCESS_CHAIN` otherwise) before the overflow-guarded fold.
- Regression asserts added (vec-element → bad-access-chain, float-index →
  bad-access-chain, integer huge-index → overflow).

### Added — Phase N.3 (GFX9 instruction selection)
- `src/gfx9_isel.cyr` — selects one abstract GFX9 op (`GISEL_*`) per MIR
  instruction over VIRTUAL registers (= MIR SSA `<id>`s; N.4 assigns physical
  regs, N.5 encodes). The load-bearing **SALU-vs-VALU** choice falls out of the
  N.2 uniformity: integer ops select the `S_*` form when the result is uniform
  and the `V_*` form when divergent; float ops are always VALU (GFX9 has no
  scalar f32 ALU). Load/store carry the ptr-table index + the access binding;
  `OpReturn` → `S_ENDPGM`. No class-coercion copies — the straight-line
  uniformity guarantees compatible operand classes (a uniform result has
  all-uniform operands); constant-bus / multi-SGPR cases are N.8. Output is a
  caller-provided buffer of 48-byte selected-instruction records; `gfx9_isel`
  returns the count or a negative error.
- `tests/tcyr/compiler.tcyr` +43 asserts: the SALU/VALU op-selection table, the
  **SAXPY** and **gid** kernels selected end-to-end (uniform `1<<2` →
  `S_LSHL_B32` vs divergent `i*4` → `V_MUL_LO_U32`, the f32 ALU → VALU, the
  load/store ptr-index + binding flow), plus cap-exceeded / unsupported-op /
  review-regression negatives.

### Fixed — N.3 adversarial review (2 confirmed findings, both fixed pre-merge)
- The integer-selection path read the result's uniformity without guarding two
  sentinel cases, silently picking the SALU form instead of failing loud: (a) a
  **result-less** integer ALU op (`dst` 0) read the `vals[0]` sentinel (= UNIFORM)
  → SALU; (b) an **UNKNOWN-uniformity** result (the N.2 sweep not run / a value it
  missed) → `div=0` → SALU for a possibly-divergent value. `_gisel_one` now
  rejects both with `GISEL_ERR_UNSUPPORTED_OP` (defensive hardening — N.4/N.5 are
  not wired yet, so no live miscompile, but it converts a silent mis-selection
  into a loud error). Regression asserts added.

### Added — Phase N.4a (register allocation)
- `src/gfx9_regalloc.cyr` — linear-scan allocation of the N.3 virtual registers
  (MIR SSA `<id>`s) to physical VGPR/SGPR numbers. Two independent files keyed by
  the N.2 uniformity (UNIFORM → SGPR, DIVERGENT → VGPR — the scalar-vs-vector
  decision is already made). Reuse via a per-register "free-at" step: a freed
  VGPR is reclaimed by a later divergent value (the reuse that lets a kernel's
  many SSA values fit a small register budget). NO SPILL — fail loud over the
  file cap (`RA_ERR_VGPR_OVERFLOW` / `_SGPR_OVERFLOW`). ABI-fixed registers are
  reserved by the caller via `sgpr_base`/`vgpr_base` (USER_DATA + TGID SGPRs,
  v0 = LocalInvocationId). Emits the VGPR/SGPR high-water marks N.6 feeds into
  RSRC1. Only SSA results are allocated (constants inline, builtins/buffers
  ABI/binding-resolved). Returns 0 or a negative error.
- `tests/tcyr/compiler.tcyr` +24 asserts: the **gid** kernel allocated exactly
  (incl. `%14` reclaiming `%12`'s freed `v1` and the uniform `1<<2` → `s8`), the
  **SAXPY** (7 divergent values reuse into v1-v4, VGPR high-water 5, no SGPR
  use), the VGPR-overflow / cap-too-large fail-louds, and the review-regression
  case below.

### Fixed — N.4a adversarial review (1 confirmed HIGH, fixed pre-merge)
- **VALU-op result misfiled into an SGPR.** isel always selects a VALU (`V_*`)
  op for float / `CVT` (GFX9 has no scalar f32 ALU), but regalloc chose the
  register file from the value's *uniformity* — so a float/CVT op with a
  **uniform** result (e.g. `CVT_F32` of `WorkgroupId`) had its VGPR-writing
  instruction assigned an SGPR (invalid encoding / silent corruption). The file
  now follows the selected op's destination class (`gisel_writes_vgpr`): `S_*`
  → SGPR, everything else with a result → VGPR. Integer ops already agreed (isel
  picks `S_*`/`V_*` by the same uniformity). Regression: a uniform `CVT_F32`
  result lands in a VGPR. (A uniform value held in a VGPR later feeding a SALU op
  is the operand-class-coercion case — flagged N.8.)

### Added — Phase N.4b (s_waitcnt insertion)
- `src/gfx9_waitcnt.cyr` — splices `s_waitcnt` into the N.3 selected-instruction
  list so no instruction reads the result of a still-outstanding memory op. GFX9
  loads are asynchronous (the dst register is invalid until the hardware `vmcnt`
  counter drains); reading early reads garbage — the compiler's top-tier
  correctness risk. The pass makes a use-before-wait **impossible by
  construction**: `vmcnt(0)` (`0x0F70`) before the first instruction that
  consumes an outstanding load result, and `vmcnt(0) lgkmcnt(0)` (`0x0070`)
  before `s_endpgm` whenever any memory op ran (so the wave does not retire
  before its stores land) — exactly the hand-authored downsample pattern, for the
  N.5 byte-match oracle. MVP policy is deliberately **conservative**: a use of any
  outstanding load waits for *all* of them (over-waiting is safe; the danger is
  under-waiting; the per-load count is N.8). Outstanding loads tracked by result
  `<id>`; runs on the isel list independently of regalloc (waits carry no
  register, the reg-map is id-keyed). New `GISEL_S_WAITCNT` op (simm in `a`,
  flagged immediate). Returns the new count or a negative error.
- `tests/tcyr/compiler.tcyr` +17 asserts: the **SAXPY** kernel (two loads, one
  `vmcnt(0)` before the f32 multiply covering both loads, the drain before
  `s_endpgm`, the exact simms), a no-memory kernel (zero waits, none before
  `s_endpgm`), the cap-exceeded / cap_ids-too-large fail-louds, and a
  `_wc_check_invariant` walker that **proves** no output instruction reads an
  un-waited load. Adversarial review (workflow): clean — 1 candidate, 0 confirmed
  (no under-wait / bounds gap survived verification).

### Added — Phase N.5a (GFX9 encode driver)
- `src/gfx9_compile.cyr` — the encode driver: walks the N.4 selected-instruction
  (GISEL) list + the regalloc map and emits a GFX9 ISA dword stream through the
  `gfx9_encode.cyr` encoders. `gfx9_emit_program(m, isel, n_isel, alloc, buf,
  pos)` returns the end byte position (the caller appends the prefetch pad).
  Resolves each operand to a physical register (regalloc map + a per-value
  file-map keyed by the *defining op's* class via `gisel_writes_vgpr` — not
  uniformity, the N.4a lesson), an inline constant, or a trailing 32-bit literal
  dword. Covers the single-dword formats the current isel emits — SOP2, VOP1
  (CVT), VOP2 (incl. the rev-operand shifts: value → `vsrc1`, amount → `src0`),
  SOPP (`s_waitcnt`/`s_endpgm`). Fail-loud (`CMP_ERR_*`) on FLAT / VOP3 / EXTRACT
  / builtin / buffer operands (N.5b+), the ≤1-literal-per-instruction rule, and a
  VGPR operand handed to a SALU op.
- `src/gfx9_encode.cyr` — added the SOP2 (`S_SUB_I32`/`S_LSHR_B32`/`S_AND_B32`/
  `S_OR_B32`/`S_XOR_B32`) and VOP2 (`V_ADD_F32`/`V_SUB_F32`/`V_XOR_B32`/
  `V_SUB_U32`) opcodes the full GISEL ALU set needs, each **llvm-mc gfx900
  verified**.
- `tests/tcyr/compiler.tcyr` +14 asserts: a self-consistent 9-instruction program
  (SOP2 inline+const, VOP1 CVT bootstrapping a VGPR, VOP2 commutative + literal,
  VOP2 rev-shift, SOPP) whose emitted bytes **byte-match llvm-mc gfx900
  ground-truth**, the unsupported-op / builtin-operand / cap_ids fail-louds, and
  the subtraction operand-file matrix below. Adversarially reviewed (workflow).

### Fixed — N.5a adversarial review (1 confirmed, fixed pre-merge)
- **Non-commutative subtraction was routed through the commutative VOP2 path and
  silently negated.** GFX9 `v_sub` computes `src0 - vsrc1` and `vsrc1` must be a
  VGPR, so a `divergent_var - constant` (VGPR minuend, non-VGPR subtrahend — the
  ubiquitous `idx - 1` pattern) had its operands swapped, computing `const - var`.
  Added a dedicated `_emit_vop2_sub` that emits `v_sub` when the subtrahend is the
  VGPR and `v_subrev` (new opcodes `V_SUBREV_U32`=0x36 / `V_SUBREV_F32`=0x03,
  llvm-mc-verified) when the minuend is the VGPR — both yielding `a - b`. (`ISUB`
  was untested by the original oracle, so the path was unexercised; the fix lands
  with a 3-case operand-file matrix for `ISUB` + `FSUB`.)

### Added — Phase N.5b-1 (compute ABI + RSRC wiring)
- `src/gfx9_abi.cyr` — the canonical compute ABI the compiler always emits, plus
  the `COMPUTE_PGM_RSRC1`/`RSRC2` dword computation. `gfx9_rsrc1(vgpr_hw,
  sgpr_hw)` derives the VGPRS/SGPRS fields from the regalloc high-water marks
  (`ceil(v/4)-1`; `ceil((s+2)/8)-1` with a +2 VCC reserve, floored at SGPRS=1);
  `gfx9_rsrc2(user_sgpr, tgid_x, tgid_y, trap)` packs the dispatch enables.
  `gfx9_abi_assign` scans the MIR for buffer bindings + builtins and fills a
  descriptor: binding k → USER_DATA SGPR pair `s[2k:2k+1]`, `WorkgroupId` →
  `s[user_sgpr..]` (TGID), `LocalInvocationId` → `v0..`, and the `sgpr_base`/
  `vgpr_base` the allocator reserves above the ABI region. `GlobalInvocationId`
  resolves to `-1` (it is `wgid*size+lid`, materialized by a later lowering step,
  not a single register).
- `tests/tcyr/compiler.tcyr` +12 asserts: `rsrc1(15,20)==0x2C0083` and
  `rsrc2(6,1,1,0)==0x18C` (the HW-verified downsample values), `rsrc1(4,4)` =
  `RSRC1_MIN`, `rsrc2(2,0,0,1)==0x44` (deadbeef), and the 2-binding +
  GlobalInvocationId ABI layout (user_sgpr, bases, binding/builtin resolution).
  Adversarially reviewed (workflow).

### Fixed — N.5b-1 adversarial review (1 confirmed LOW, fixed pre-merge)
- **`gfx9_rsrc1` SGPRS 4-bit field silently wrapped past 120 SGPRs.** `(sgprs &
  0xF) << 6` masked an overflow to a too-small allocation (`sgpr_hw=128` →
  SGPRS=0 = 8 SGPRs) instead of failing. Not currently reachable (no driver feeds
  large counts yet; GFX9 HW caps near 104 SGPRs) — defense-in-depth before the
  N.6 driver. `gfx9_rsrc1` now returns `-ABI_ERR_SGPR_OVERFLOW` /
  `-ABI_ERR_VGPR_OVERFLOW` on a field overflow; regression-tested (`rsrc1(4,120)`
  ok, `rsrc1(4,128)` / `rsrc1(300,20)` fail loud). (Noted for follow-up:
  `gfx9_regalloc` validates the SGPR cap against `RA_MAX_REGS`=256, the VGPR file
  size, not the ~104 GFX9 SGPR limit.)

### Added — Phase N.5b-2 (FLAT load/store + VOP3 mul encode)
- `src/gfx9_compile.cyr` — `gfx9_emit_program` now takes the `abi` descriptor and
  encodes `GISEL_GLOBAL_LOAD`/`_STORE` (FLAT **SADDR form**: the binding's
  USER_DATA base SGPR pair as `saddr` + the per-lane byte-offset VGPR as `vaddr` —
  no SGPR→VGPR move or 64-bit add for the divergent-offset case) and
  `GISEL_V_MUL_LO_U32` (VOP3 two-source, the `idx*stride` offset multiply; VOP3
  has no literal form, so an out-of-inline-range constant fails loud).
- `src/gfx9_encode.cyr` — `GFX9_FLAT_GLOBAL_LOAD_DWORD`=0x14 +
  `GFX9_VOP3_V_MUL_LO_U32`=0x285 (llvm-mc-verified).
- `tests/tcyr/compiler.tcyr` +10 asserts: a load→mul→store program byte-matching
  llvm-mc gfx900 (binding 0→`s0`, binding 1→`s2` via the ABI). Adversarially
  reviewed (workflow). (`GISEL_EXTRACT` builtin resolution stays N.5c.)

### Fixed — N.5b-2 adversarial review (3 confirmed FLAT-path gaps, fixed pre-N.5c)
- The FLAT emitters read the offset/value operands via raw `gfx9_reg` without
  validating them, so three not-yet-handled shapes silently emitted wrong bytes
  (all latent today — the pipeline is unwired — but they go live the moment N.5c
  wires it, so hardened now). The emitters now route those operands through a
  checked resolver + a binding range-check and **fail loud**:
  - a **const-offset access chain** (`off_id`=0) used to encode `vaddr=v255` and
    drop the byte offset → `CMP_ERR_FLAT_CONST_OFFSET` (the FLAT immediate-offset
    form is a later bite);
  - a **uniform / non-VGPR offset or store value** used to emit an SGPR index into
    the VGPR `vaddr`/`data` field → `CMP_ERR_FLAT_NONVGPR` (SGPR→VGPR
    materialization is a later bite);
  - a **binding past the loaded USER_DATA region** → `CMP_ERR_BINDING_RANGE`, and
    `gfx9_abi_assign` now rejects >8 bindings (`ABI_ERR_TOO_MANY_BINDINGS`, the
    16-SGPR USER_DATA cap). +4 regression asserts.

### Added — Phase N.5c (top-level driver + EXTRACT — the first SPIR-V→GFX9 oracle)
- `src/gfx9_compile.cyr` — **`gfx9_compile(ctx, spirv, n, isa, desc)`**, the
  top-level driver that chains the whole compiler over a caller-provided scratch
  context: `spirv_validate_stream` → type/const/decoration tables →
  `spirv_lower_module` (incl. the uniformity sweep) → `gfx9_isel` →
  `gfx9_abi_assign` → `gfx9_regalloc` (ABI-reserved `sgpr_base`/`vgpr_base`;
  GFX9 file caps 104/256) → `gfx9_waitcnt` → `gfx9_emit_program` →
  `gfx9_emit_prefetch_pad`. Writes the GFX9 ISA byte stream + a
  `[isa_len, rsrc1, rsrc2, user_sgpr, num_bindings]` descriptor; any stage's
  negative error short-circuits.
- `_emit_extract` — `GISEL_EXTRACT` of `LocalInvocationId.comp` → `v_mov(result,
  v[comp])` (the HW-loaded per-lane id). `WorkgroupId`/`GlobalInvocationId` fail
  loud (`CMP_ERR_UNSUPPORTED_BUILTIN`; the `wgid*size+lid` expansion is N.5c-2).
- `tests/tcyr/compiler.tcyr` +12 asserts: the **`EXTRACT(LID)→v_mov v?,v0`**
  byte-match (+ GID fail-loud), and **the first end-to-end oracle** —
  `_spv_build_saxpy_lid` (the SAXPY fixture re-indexed by LocalInvocationId)
  compiled through `gfx9_compile` to a coherent ISA stream (first instr the LID
  `v_mov`, terminating `s_endpgm` + the 64-byte prefetch pad) + the right
  descriptor (2 bindings, `user_sgpr=4`, `rsrc2=0x48` LID-only-no-TGID, RSRC1
  FLOAT_MODE flags).

### Fixed — N.5c adversarial review (1 confirmed CRITICAL, fixed pre-merge)
- **The SPIR-V validation gate was inverted — malformed input bypassed it.**
  `gfx9_compile` checked `spirv_validate_stream(...) < 0`, but that gate returns a
  *positive* error code (0 = OK, 1–11 = errors), so `< 0` never fired: a bad-magic
  / truncated / overrun module sailed past validation into the table builders
  (which assume a validated stream) → out-of-bounds reads on untrusted input. Now
  rejects any non-zero with `CMP_ERR_VALIDATE`; regression test added (a bad-magic
  module is rejected, not crashed). (Caught by a read-only Explore-agent review —
  the no-file-write rule held: zero scratch files left behind.)

### Added — Phase N.5c-2a (EXTRACT file-class fix + SOP1 `s_mov` + `EXTRACT(WGID)`)
- `src/gfx9_isel.cyr` — `gisel_result_is_vgpr(m, out, i)`: the authoritative
  register-file of an instruction's result. For `GISEL_EXTRACT` it follows the
  result's **uniformity** (`WorkgroupId` → SGPR, `Local`/`GlobalInvocationId` →
  VGPR) — a builtin extract just reads that builtin's register; for every other op
  it stays `gisel_writes_vgpr(op)`. Replaces the bare `gisel_writes_vgpr` call in
  **both** `gfx9_regalloc` (the file decision) and `gfx9_compile`'s encode
  file-map so the two never disagree. (`gisel_writes_vgpr` alone can't decide
  EXTRACT — it lacks the operand.)
- `src/gfx9_encode.cyr` — `gfx9_enc_sop1` + `GFX9_SOP1_S_MOV_B32` (llvm-mc-verified,
  `s_mov_b32 s8,s6 = 0xBE880006`).
- `src/gfx9_compile.cyr` — `_emit_extract` resolves `WorkgroupId.comp` →
  `s_mov(result, s[user_sgpr+comp])` (the TGID SGPR); `LocalInvocationId.comp`
  stays `v_mov(result, v[comp])`. `GlobalInvocationId` still fails loud (the
  lowering expands it — N.5c-2b).
- `tests/tcyr/compiler.tcyr` +9 asserts: a uniform WGID extract is allocated an
  SGPR and emits `s_mov s3, s2`; a divergent LID extract is allocated a VGPR and
  emits `v_mov v1, v0` (the file-class fix proven through regalloc + encode); plus
  the review-regression below. All prior divergent extracts unchanged.

### Fixed — N.5c-2a adversarial review (1 confirmed, fixed pre-merge)
- **`EXTRACT` didn't fail loud on an `UNKNOWN`-uniformity result.** Its register
  file is now decided from the result's uniformity, so an unclassified result
  (the N.2 sweep not run / a value it missed) would be silently filed to an SGPR —
  a LID extract that should be a VGPR would then get a `v_mov` with an SGPR dst.
  `_gisel_one` now rejects a result-less or `MIR_UNKNOWN` EXTRACT with
  `GISEL_ERR_UNSUPPORTED_OP`, mirroring the integer-ALU guard (N.3). Not reachable
  in the normal `gfx9_compile` flow (uniformity always runs before isel) —
  defensive hardening; regression test added.

### Added — Phase N.5c-2b (GlobalInvocationId expansion — the real GID SAXPY compiles)
- `src/spirv_lower.cyr` — `OpCompositeExtract` of a `GlobalInvocationId` load now
  **expands** to `gid.comp = WorkgroupId.comp · local_size_comp +
  LocalInvocationId.comp` (it is not a hardware register). `_spirv_expand_gid`
  synthesizes the two primitive builtins + their extracts + an `IMUL` (by the
  `LocalSize` dim, via `spirv_find_local_size`, default 1) + an `IADD` keeping the
  original result `<id>`. The uniformity sweep then classifies the wgid-extract +
  mul **UNIFORM** (→ `s_mul`) and the lid-extract + add **DIVERGENT** (→ `v_add`),
  and the N.5c-2a file-class rule places wgid in an SGPR, lid in a VGPR — the
  natural compute index lowering. `local_size` is threaded
  `spirv_lower_module → _spirv_lower_body → _spirv_lower_one_instr`; the `comp`
  is bounded ≤ 2 (vec3) before the `lsz` read (untrusted-input guard).
- `tests/tcyr/compiler.tcyr` — the **real GlobalInvocationId-indexed SAXPY now
  compiles end-to-end** (`test_gfx9_compile_saxpy_gid`: `rsrc2` gains `TGID_X`,
  first two instrs the wgid `s_mov` + lid `v_mov`) and a direct lowering-shape
  test (`test_spirv_lower_gid_expansion`: instrs 0-3 = extract/extract/imul(×64)/
  iadd with the right builtins + uniformity). The two base fixtures
  (`_spv_build_saxpy`, `_spv_build_gid_kernel`) were re-indexed to
  **LocalInvocationId** so the pipeline-shape tests stay single-extract;
  `_spv_build_saxpy_gid` re-indexes by GID for the expansion tests. **Closes
  N.5c-2 — a divergent-index compute kernel compiles SPIR-V → GFX9 end-to-end.**

### Fixed — N.5c-2b adversarial review (2 confirmed, same root, fixed pre-merge)
- **Synth-id overflow could silently OOB-write `vals[]`.** `mir_alloc_synth_id`
  had no `cap_ids` ceiling check, and `_spirv_expand_gid` didn't check the
  `mir_set_builtin` returns — so with tight `cap_ids` headroom (a kernel whose
  synth ids — 5 per GID extract + the access-chain offsets — exceed `cap_ids`) an
  out-of-range id could be written past the value table (defended only
  coincidentally by downstream `mir_emit` operand bounds). `mir_alloc_synth_id`
  now returns `-1` at the ceiling; every caller (`_spirv_expand_gid`, the
  access-chain offset) checks it; `_mir_set_val` rejects `id <= 0` (was `== 0`).
  Regression test: a `cap_ids` too tight for the GID expansion fails loud
  (`MIR_ERR_ID_OOR`), not a corrupt write.

### Added — Phase N.5d-2 (a compiled SPIR-V kernel runs on the GPU — MVP reached)
- **The in-tree SPIR-V→GFX9 compiler is HW-verified end-to-end on Cezanne.**
  `programs/native_spirv_compute_e2e.cyr` hand-authors the SPIR-V for
  `out[gl_LocalInvocationID.x] = lid.x*3 + 7` (LocalSize 8), runs it through
  `gfx9_compile` to GFX9 ISA + an RSRC descriptor, publishes the ISA to a GTT BO,
  dispatches one 8-thread workgroup, and reads back **all 8 lanes correct**
  (7,10,13,16,19,22,25,28) from the GPU — per-lane arithmetic, so it cannot be a
  constant store. `make test-native-spirv-compute-e2e`.
- `src/backend_native_pm4.cyr` — `native_pm4_build_compute_generic`: a generic
  single-binding compute dispatch composer (the proven Cezanne scaffold with
  parameterized RSRC1/RSRC2, binding-0 USER_DATA VA, and NUM_THREAD_X) that
  dispatches a compiled shader rather than a hand-authored one.
- `tests/tcyr/native.tcyr` +4 asserts: a structural CPU test that the compiler
  RSRC, binding VA, and NUM_THREAD wire into the generic composer's PM4. (Scope:
  1 binding, LID index, no TGID — the multi-binding/TGID generic dispatcher is
  N.6.)

### Changed — Phase N.5d-1 (dispatch seam: parameterized compute RSRC)
- `src/backend_native_pm4.cyr` — `native_pm4_build_compute_downsample` now takes
  `rsrc1`/`rsrc2` as parameters instead of hardcoding `0x2C0083`/`0x18C`, so a
  **compiler-derived** `gfx9_rsrc1`/`gfx9_rsrc2` can flow into the compute dispatch
  packet (the N.5d seam). The internal mipmap caller passes the hand-authored
  shader's values; `tests/tcyr/native.tcyr` proves an alternate (compiler-shaped)
  RSRC flows through where the hardcode used to be. (The `_store_deadbeef` composer
  stays fixed — it dispatches the constant test shader, not a compiled one.)
- **Toolchain pin → `cyrius = "6.2.18"`** (tracking upstream; was 6.2.15).
  6.2.18 lands the native-float `f32_from` / `f32_to` builtins (the f32 conversion
  gap — `cvtsd2ss`/`cvtss2sd` on x86, `fcvt` on aarch64).
- `cyrius.cyml` `[lib].modules` + `src/lib.cyr` include the compiler modules
  `mir.cyr` / `spirv_lower.cyr` / `gfx9_isel.cyr` / `gfx9_regalloc.cyr` /
  `gfx9_waitcnt.cyr` / `gfx9_abi.cyr` / `gfx9_compile.cyr` (after
  `spirv_parse.cyr`).

### Removed — f32 inline-asm shims (the 6.2.19 toolchain fold-in)
- `src/color.cyr` — retired the three hand-rolled **x86-64 SSE2 inline-asm**
  float shims now that the toolchain provides native-float builtins:
  `f64_to_f32` → `f32_from`, `f32_to_f64` → `f32_to`, and `int_ratio_to_f32` →
  `f32_from(f64_div(f64_from(n), f64_from(d)))`. The descriptive names are kept as
  the GPU-boundary idiom (thin builtin wrappers; ~30 call sites in `color.cyr` /
  `vertex.cyr` / `backend_native.cyr` unchanged). Byte-identical f32 output
  (the pinned `int_ratio` / conversion vectors in `core.tcyr` pass unchanged) and
  **`src/` now contains zero inline asm** — fully portable Cyrius, with the
  aarch64 NEON path carried by the builtins.

### Notes
- Cyrius reserves `mod` (modulo) as a keyword — the MIR module handle parameter
  is `m`, not `mod`.

## [3.2.5] — 2026-06-16

**Native SPIR-V → GFX9 compiler — foundation (Phases N.0 + N.1).** The first two
stages of the in-tree compute compiler
(`docs/proposals/v3.2-spirv-gfx9-native-lowering.md`), both pure CPU with no
public-API change. **N.0** lifts the hand-authored GFX9 ISA into
operand-parameterized encoders proven byte-identical against ~90 known-good
dwords, then re-expresses the six HW-verified shader builders through them (zero
byte change, the proposal's round-trip regression check). **N.1** adds the SPIR-V
front end: `spirv_validate_stream`, the untrusted-input rejection gate, plus the
type / constant / decoration lookup tables the lowering (N.2) will consume.
Toolchain **6.2.14 → 6.2.15**. CPU suite 2908 → **3077** assertions.

### Added — Phase N.0 (native SPIR-V→GFX9 compiler: encoder lift + byte oracle)
- `src/gfx9_encode.cyr` — operand-parameterized GFX9 instruction encoders, the
  output stage of the in-tree SPIR-V→GFX9 compute compiler
  (`docs/proposals/v3.2-spirv-gfx9-native-lowering.md`). One pure encoder per
  compute-format the compiler emits — VOP1, VOP2, VOP3a, SOP2, SOPP, SMEM,
  FLAT(global) — plus VOP src-operand helpers (`gfx9_vgpr` / `gfx9_sgpr` /
  `gfx9_inline_int`) and per-format opcode constants. Pure byte math, no deps.
- `tests/tcyr/compiler.tcyr` — the N.0 regression ORACLE (118 asserts): every
  encoder reproduces, byte-for-byte, the hand-authored dwords in
  `backend_native_shaders.cyr` (each already llvm-mc-round-tripped on gfx90c) —
  the `store_deadbeef` shader, the full `downsample_2x2` SOP2/VOP1/VOP2/VOP3a
  stream, and the textured-FS SMEM/`v_mul_f32`/`v_cvt` dwords — plus a capstone
  that rebuilds `store_deadbeef` wholly through the encoders and `memeq`s the
  HW-verified builder's output. The safety net before any *generated* code exists.
- `scripts/disasm-shaders.sh` — a per-form llvm-mc round-trip section: one
  representative encoder output per GFX9 format, each decoded to confirm it is a
  valid gfx90c encoding of the expected mnemonic (independent of the Cyrius
  oracle).
- `gfx9_encode.cyr` emit helpers: `gfx9_emit32` (store one dword + advance — the
  stream primitive the N.5 encode stage will drive) and `gfx9_emit_prefetch_pad`
  (the AMDGPU-mandated 16 × `s_nop 0` tail, lifted from the loop that was
  duplicated in all six shader builders).

### Changed — N.0 follow-on refactor (shader builders now emit through the encoders)
- All six hand-authored builders in `backend_native_shaders.cyr`
  (`store_deadbeef`, `downsample_2x2`, `solid_red`, `fullscreen_triangle_vs`,
  the two textured FSes) re-expressed from raw `store32(0xLITERAL)` dwords to
  operand-level `gfx9_emit32(buf, p, gfx9_enc_*(...))` calls — self-documenting,
  no magic dwords, and the proposal's "each fixed shader round-trips to its
  known bytes through the new encoders" regression check. **Byte-identical:**
  guarded by the pre-existing per-dword golden / checksum tests in `native.tcyr`
  (e.g. the downsample's `179445143226` dword checksum) — the full suite is the
  proof the produced bytes did not move. The duplicated `s_nop` pad loop is now
  one `gfx9_emit_prefetch_pad` call.
- `cyrius.cyml` `[lib].modules` + `src/lib.cyr`: `src/gfx9_encode.cyr` moved to
  **before** `backend_native_shaders.cyr` (it is now the builders' dependency;
  same "no deps" tier, after `_amdgpu`).

### Unblocked
- Toolchain pin **6.2.14 → 6.2.15** (`cyrius.cyml` + CLAUDE.md): upstream 6.2.15
  repairs the macOS benchmark-timing path and a stdlib fix. `cycc` on the dev box
  is already 6.2.15; the full suite + smoke + dist are green on it.

### Added — Phase N.1 (SPIR-V parser: validate gate + lookup tables)
- `src/spirv_parse.cyr` — the compiler's front end (stage N.1 of
  `docs/proposals/v3.2-spirv-gfx9-native-lowering.md`). Word/header accessors,
  `(opcode, wordcount)` instruction decode, and `spirv_validate_stream` — the
  UNTRUSTED-INPUT REJECTION GATE: header via the Phase S `_spirv_validate`
  (bad-magic / short / unaligned / byte-swapped / id-bound / over-cap) plus a
  whole-stream instruction walk that rejects a zero word count (would not
  advance) and any instruction that runs past the end. Plus probes:
  `spirv_count_instructions`, `spirv_find_entry_point` (GLCompute), and
  `spirv_find_local_size`. Pure byte walking, no syscalls/alloc; included after
  `wgpu_descriptors.cyr` so it reuses `_spirv_validate`.
- `spirv_parse.cyr` N.1b — the type / constant / decoration LOOKUP TABLES the
  SSA model N.2 lowers from: caller-provided buffers of `id_bound` fixed-size
  records indexed directly by `<id>` (no hashmap, no alloc → callers/tests stay
  stack-based). `spirv_build_type_table` (void/bool/int/float/vector/array/
  runtime-array/struct/pointer/function, with per-op `wc` guards so operand
  reads stay inside each instruction), `spirv_build_const_table` (scalar
  OpConstant), `spirv_build_decoration_table` (Binding / DescriptorSet, -1 =
  absent) + their accessors.
- `tests/tcyr/compiler.tcyr` +51 parser asserts: a minimal GLCompute fixture for
  N.1a (every accessor/probe + **5 rejection cases** — bad magic, truncated
  header, zero id-bound, zero word count, instruction overrun) and a typed-module
  fixture for N.1b (type/const/decoration table contents + absent-id defaults).

### Notes
- EXP (graphics color/pos export) and MIMG (image load/sample) dwords stay raw
  literals in the builders — the proposal scopes the compiler compute-only, so
  those encoders land with a future graphics/texture phase. Flagged inline in
  each affected builder; the existing byte-pinned tests still cover them.

### Security
- The SPIR-V parser is a new UNTRUSTED-INPUT boundary (a consumer's toolchain
  produced the bytes). `spirv_validate_stream` is the rejection gate — header via
  `_spirv_validate` (bad-magic / short / unaligned / byte-swapped / id-bound /
  over-cap) + a per-instruction word-count walk rejecting zero-wc (would not
  advance) and stream overrun — and every table builder bounds-guards each
  operand read by the instruction's declared word count. 5 malformed-input
  rejection tests. Audit: `docs/audit/2026-06-16-phaseN01-audit.md` — 0 findings.

### Metrics
- CPU suite **2908 → 3077** assertions (N.0 encoder oracle +118, N.1 parser +51);
  **13** domain test files (new `compiler.tcyr`). Bundle 16094 → **16814** lines
  (gfx9_encode + emit helpers + spirv_parse; builder bodies trade magic dwords
  for encoder calls).

### Next
- **3.2.5 content (N.0 + N.1) is complete** — ready to cut. Then N.2 (MIR +
  uniformity lowering, `src/mir.cyr` + `src/spirv_lower.cyr`) opens 3.2.6.

## [3.2.4] — 2026-06-16

**SPIR-V shader ingestion on wgpu (Phase S).** The byte-polymorphic shader
boundary gains an explicit source-KIND tag, so a consumer can hand mabda a
pre-assembled **SPIR-V** binary (`u32` word stream, magic `0x07230203`) and the
wgpu backend creates a shader module from it — a peer frontend to WGSL, which
stays the default. HW-verified on a wgpu-native box: a SPIR-V module renders
**byte-identical** to the equivalent WGSL (`spirv_e2e` cross-source identity).
Native SPIR-V→GFX9 lowering is Phase N (fail-loud here). Toolchain
**6.2.12 → 6.2.14**. CPU suite 2882 → **2908** assertions.

### Added

- **`gpu_shader_module_create_spirv(ctx, words_ptr, byte_len)`** — create a
  shader module from a SPIR-V binary (wgpu). `byte_len = word_count * 4`.
- **`ShaderSourceKind`** enum (`SHADER_SRC_WGSL`/`SPIRV`/`GFX9`) — the explicit
  tag the shader-create slot now carries (the bytes' meaning is no longer purely
  the bound backend, since wgpu accepts both WGSL and SPIR-V).
- `wgpu_shader_source_spirv` (the `WGPUShaderSourceSPIRV` descriptor, codeSize in
  **words**) + `WGPU_STYPE_SHADER_SOURCE_SPIRV` + `_spirv_validate`
  (magic/align/bound + a 16 Mi-word cap; rejects byte-swapped binaries) — all
  pinned vs webgpu.h v29.
- Source-kind-aware shader cache: `_shader_hash_n` (length-explicit — SPIR-V has
  embedded NULs — with the kind folded into the seed; WGSL is the identity fold,
  so legacy keys are byte-stable) + `shader_cache_{get,set,get_or_compile}_spirv`.
- `deps/wgpu_main.c` requests the `ShaderSourceSPIRV` instance feature (the
  reference launcher; see Migration).
- `programs/spirv_e2e.cyr` + `make test-spirv-e2e` — the HW cross-source-identity
  e2e (a glslang+spirv-link, spirv-val-clean fullscreen-triangle module).

### Changed

- **Toolchain pin 6.2.12 → 6.2.14.**
- The shader-create backend slot widened `(ctx, bytes, n, kind)` (fncall3→4) —
  **no slot-offset / `BACKEND_SIZE` change**, the completeness walk is untouched.
  `gpu_shader_module_create` keeps its public signature and forwards the bound
  backend's default kind (WGSL on wgpu, GFX9 on native) — no consumer call-site
  churn.

### Fixed

- **Validate-before-hash** in `shader_cache_get_or_compile_spirv` — never key the
  cache on a malformed SPIR-V blob (defense-in-depth; Phase S review).

### Breaking / Migration

- **Consumers that copy `deps/wgpu_main.c` must carry the 4-line instance-feature
  edit** (`requiredFeatureCount=1` + `requiredFeatures = {ShaderSourceSPIRV}`) to
  use SPIR-V shaders. WGSL is unaffected. mabda cannot detect a feature-less
  instance before the create call, so a SPIR-V `createShaderModule` against an
  un-updated launcher returns `0` (the fail-loud null) — `gpu_shader_module_create_spirv`
  surfaces that as a clean 0, not a crash.

### Security

- One adversarial review workflow over the Phase S diff (FFI descriptor offsets +
  `_spirv_validate` completeness + the slot/kind/cache integration) — **0 CONFIRMED
  findings** (one LOW defense-in-depth note on the dead-in-tree cache path,
  dismissed; the validate-before-hash reorder lands anyway). SPIR-V binaries are
  structurally validated before crossing the FFI; the embedded test binary is
  `spirv-val`-clean. `docs/audit/2026-06-16-phaseS-audit.md`.

### Metrics

- CPU assertions 2882 → **2908**. Toolchain 6.2.12 → 6.2.14. `Backend` unchanged
  (280 B — the slot widened in signature only, no new offset).

### Next

- **Phase N (3.2.5–3.2.9)** — native SPIR-V→GFX9 ISA compiler (encoder-lift +
  byte-oracle → MIR → regalloc → kernels). Then Phase F (f64), Phase R
  (render-graph multi-queue). Plus the TS follow-ons (from_context caps, BC6H).

## [3.2.3] — 2026-06-16

**Native compressed (BC) + filtered texture sampling (Phase TS.6–8).** Completes
the v3.2.x arc's native-sampling story (`docs/development/3-2-punchlist.md`
Phase TS): the **native AMD** backend now samples **block-compressed** textures
(BC1/BC3/BC4/BC5/BC7) and does **bilinear / scaled** filtering — not just the
RGBA8/linear point sampling of 3.2.2. The whole path is HW-verified on Cezanne:
SDMA SW_64KB_S tiling (TS.6) → BC sampling pixel-exact vs a CPU decode (TS.7) →
bilinear blends vs point (TS.8). **ETC2/ASTC are resolved HW-blocked on AMD**
(`vulkaninfo`: `textureCompressionBC=true`, `ETC2`/`ASTC=false`) — the cap stays
BC-only. Toolchain **6.2.11 → 6.2.12**. CPU suite 2702 → **2882** assertions.

### Added

- **Native BC compressed sampling (BC1/BC3/BC4/BC5/BC7).** A compressed
  `gpu_texture_create_2d_sampleable` builds a SW_64KB_S-tiled surface + a BC T#;
  `gpu_texture_write`/`read` route through an SDMA `COPY_TILED_SUB_WINDOW`
  (L2T/T2L) bridge; the `image_sample` FS does the TA block-decode. HW-verified
  pixel-exact vs a CPU decode by `native_compressed_sample_e2e` (BC1 RGB565
  endpoints + checker, BC4→(R,0,0,1), BC5→(R,G,0,1), BC3 RGB+alpha, BC7 mode-6).
- **Native bilinear / scaled sampling (TS.8).** The textured FS multiplies the
  fragment position by a per-draw scale (`tex_dim/rt_dim`, f32, from the
  descriptor tail), and `gpu_render_pass_bind_texture` rebuilds the S# from the
  bound `gpu_sampler_create` (POINT/BILINEAR × CLAMP/WRAP). HW-verified by
  `native_bilinear_sample_e2e` (POINT = exact texels, BILINEAR = blends).
- **Native capability advertisement** — `gpu_caps_native_texture_compression()`
  (sibling of the wgpu adapter detector) + `native_texfmt_sampleable(fmt)` (the
  per-format source of truth, BC6H-aware), so a consumer populates a native
  context's caps the same way as wgpu (strikes the "storage-only" limitation).
- Byte-builders + helpers: `native_sdma_build_copy_tiled` (TS.6 COPY_TILED),
  `native_tex_tiled_params` / `native_tex_build_tiled_copy_packet` (awb-1 epitch),
  `native_tex_desc_cpu_addr`, `int_ratio_to_f32` (the float shim — see below).
- e2e programs `native_tiled_texture_roundtrip` (BC1+BC7 L2T→T2L identity),
  `native_compressed_sample_e2e`, `native_bilinear_sample_e2e` + `make` targets.

### Changed

- **Toolchain pin 6.2.11 → 6.2.12.**
- Both textured FS builders (`image_load`, `image_sample`) gained the per-draw
  scale `s_load_dwordx2` + `v_mul_f32` (108/112 → **124** B each; llvm-mc-verified
  + byte-pinned). No PS register/ABI change (`s[12:13]` fits `RSRC1_PS MIN|1`).
- `native_gfx9_image_descriptor` gained an explicit `epitch` param + per-format
  `DST_SEL` (BC4→X001, BC5→XY01, ETC2_RGB→XYZ1; else identity).
  `native_gfx9_sampler_descriptor` gained `unnorm` (FORCE_UNNORMALIZED).
- `NATIVE_TEXCOMP_SUPPORTED = MABDA_TEXCOMP_BC` — native BC create+sample on by
  default; the create slot gates per-format via `native_texfmt_sampleable`.

### Fixed

- Adversarial-review fixes (3 review workflows; `docs/audit/`): **BC6H gated off**
  (HDR float decoded into an RGBA8_UNORM RT = silently-wrong; HIGH); **per-format
  DST_SEL** (identity swizzle broadcast a 1-channel BC4 to (R,R,R,R); HIGH, caught
  by adding the BC4/BC5 sample probes); **staging-VA leak** (per-copy bump-VA
  exhaustion → a fixed re-used staging VA; MED); **partial-create GEM handle leak**
  (root-fixed in `_native_bo_create_domain`; LOW, codebase-wide); **clock-before-
  submit UAF window** + **errno-preserving error class** on the SDMA copy (LOW).
- Latent **probe-buffer overflow** in the sample e2es (`var X[N]` is N bytes; the
  `[8]`/`[5]` arrays held 1 i64 but stored 8/5 — passed only by stack luck).

### Resolved / HW gaps

- **ETC2/ASTC — HW-blocked on AMD** (`vulkaninfo` confirms no decode; the GFX9
  IMG_DATA_FORMAT enum has them but the silicon/driver doesn't implement it). Cap
  stays BC-only; the generic per-format groundwork (ETC2_RGB DST_SEL) is kept for
  a future arch. See `project_etc2_astc_cezanne_black` memory.
- **BC6H** sampling needs an HDR (float) render-target path — gated off until then.
- **Unnormalized bilinear** edge fidelity (half-texel/clamp convention) is a
  documented precision limitation; the edge-faithful normalized-UV path
  (UV-export VS + rcp FS) is the follow-on, not missing functionality.

### Unblocked

- Filed a Cyrius toolchain proposal for **native float arithmetic**
  (`cyrius/docs/development/proposals/2026-06-16-native-float-arithmetic.md`):
  mabda computes the blit scale via the inline-SSE2 `int_ratio_to_f32` **shim**
  because Cyrius has no float ops; the proposal replaces all three asm float
  shims with first-class floats (also unblocks non-x86-64 targets).

### Security

- Three adversarial review workflows over the diffs (TS.7c-2/-3/-4) — **0
  CRITICAL**, **2 HIGH→fixed**, several MED/LOW→fixed before the cut. Each fix
  landed with a test or is HW-covered. `docs/audit/2026-06-16-ts678-audit.md`.

### Metrics

- CPU assertions 2702 → **2882**. Both textured FS 108/112 → 124 B. Toolchain
  6.2.11 → 6.2.12. `Backend` unchanged (280 B — caps advertisement is non-slot).

### Next

- **from_context caps builder** + **BC6H** (HDR RT) finish Phase TS; then Phase S
  (SPIR-V), Phase N (native SPIR-V→GFX9), Phase F (f64), Phase R (render-graph MQ).

## [3.2.2] — 2026-06-15

**Native texture sampling (Phase TS.1–5).** Third feature of the v3.2.x arc
(`docs/development/3-2-punchlist.md` Phase TS;
`docs/proposals/v3.2-native-compressed-sampling.md`). Re-introduces the GFX9
T#/S#/`image_sample` infrastructure the v3.1 mipmap pivot deleted, so the
**native AMD** backend can now **sample** a texture in a fragment shader — not
just store/readback. A consumer creates a sampleable texture, a sampler, binds
them into a render pass, and the bound pipeline's FS samples it — the same
public API on both backends. **HW-verified on Cezanne**: `native_texture_sample_e2e`
samples a known RGBA8 texture pixel-exact (`RT[x,y]==tex[x,y]` via screen-pos
`image_load`); `wgpu_texture_sample_e2e` renders the sampled color through the
real wgpu bind-group path (which Phase T had not actually wired). This is the
work the maintainer pulled out of "v4-scale" into the arc. **Native compressed
sampling (BC/ETC2/ASTC) is Phase TS.6–8 (3.2.3)** — native sampleable is
RGBA8/linear this minor (compressed needs the tiled `SW_MODE` + swizzle). CPU
suite 2530 → **2702** assertions.

### Added

- **`gpu_texture_create_2d_sampleable(ctx, w, h, fmt)`** — a 2D texture that can
  be sampled (vs `gpu_texture_create_2d`'s storage BO). Native: a single BO
  carrying the surface + a GFX9 T#/S# descriptor in the tail; wgpu: a
  `TEXTURE_BINDING` view.
- **`gpu_sampler_create(ctx, filter, addr)`** — a backend-agnostic sampler
  intent (point/bilinear × clamp/wrap); materialized as a GFX9 S# / `WGPUSampler`
  at bind time.
- **`gpu_render_pass_bind_texture(ctx, pass, tex, sampler)`** — binds a
  sampleable texture so the bound pipeline's FS samples it.
- GFX9 descriptor builders (`native_gfx9_image_descriptor` T#,
  `native_gfx9_sampler_descriptor` S#, `native_gfx9_texfmt_to_img_format`) —
  every dword/field pinned vs the authoritative Mesa `gfx9.json`.
- `native_gfx9_shader_textured_load_fs` — the textured `image_load` FS
  (llvm-mc-assembled + byte-pinned + in `scripts/disasm-shaders.sh`).
- wgpu `wgpu_render_pipeline_get_bind_group_layout` FFI + the bind-group /
  sampler-from-intent descriptor builders.
- e2e programs `native_texture_sample_e2e` / `wgpu_texture_sample_e2e` +
  `make test-native-texture-sample-e2e` / `test-wgpu-texture-sample-e2e`.

### Changed

- `Backend` 264 → 280 B (two sample slots: `create_2d_sampleable` @264,
  `bind_for_sample` @272); `backend_is_complete` walks the 9th range.
- `NativeTexture` 48 → 64 B (`sw_mode`@48, `t_va`@52); `NativePass` 32 → 40 B
  (`bound_tex`@32); `_NATIVE_PM4_SCRATCH_BYTES` 1024 → 2048 (the textured draw
  override appends ~15 dwords past the 256-dword minimum pad).
- Toolchain pin unchanged (6.2.11).

### Fixed

- Adversarial-review fixes (`docs/audit/2026-06-15-ts-sampling-audit.md`, two
  workflows): wgpu per-draw sampler/bind-group/BGL **handle leak** (HIGH);
  native sampleable **release size mismatch** + uncleared `t_va` (MED, both
  leaked / use-after-bind); per-draw `bound_tex` clear + reject compressed-
  sampleable on native (LOW). Each landed with a test or is HW-covered.

### Limitations / Deferred (within the arc)

- **Native compressed sampling** (BC/ETC2/ASTC) — TS.6 (tile-swizzle) + TS.7
  (BC sampling) + TS.8 (bilinear/ETC2/ASTC), all **3.2.3, this arc**. Native
  sampleable is RGBA8/linear at 3.2.2; `gpu_caps_supports_format(BC)` stays 0
  on native until TS.7 flips it.
- wgpu samples compressed already (Phase T); the new slots now drive a real
  wgpu bind+sample render path.

### Security

- Two adversarial reviews of the diff — **0 CRITICAL / 2 HIGH (fixed)**, all
  findings resolved before the cut. `docs/audit/2026-06-15-ts-sampling-audit.md`.

### Metrics

- CPU assertions 2530 → **2702**. `Backend` 264 → 280 B; `NativeTexture`
  48 → 64 B; `NativePass` 32 → 40 B; PM4 scratch 1024 → 2048 B.

### Next

- **Phase TS.6–8 (3.2.3)** — tile-swizzle + BC/ETC2/ASTC native sampling +
  bilinear; then Phase S/N (SPIR-V), Phase F (f64), Phase R (render-graph MQ).

## [3.2.1] — 2026-06-15

**Buffer-to-buffer copy + real native buffers (Phase X).** Second feature of
the v3.2.x arc (`docs/development/3-2-punchlist.md` Phase X). A consumer
copies bytes between GPU buffers through one public API on both backends —
**native AMD** runs an SDMA `COPY_LINEAR` on a real DMA (TRANSFER) ring,
**wgpu** a `copy_buffer_to_buffer` on its single device queue — and the
native `gpu_buffer_*` family is now real (linear GTT BOs) rather than
diagnostic stubs. HW-verified on Cezanne (`native_transfer_copy_e2e`): a
public-API round-trip, a compute-produce → barrier → `gpu_queue_transfer_copy`
consume, and a 6 MiB chunked copy byte-exact across the 4 MiB packet
boundary. Absorbs the v3.1.2-carryover TRANSFER/buffer-copy items. CPU suite
2434 → **2530** assertions. Toolchain pin 6.2.10 → **6.2.11**.

### Added

- **`gpu_buffer_copy(ctx, src, dst, bytes)`** — synchronous buffer→buffer
  copy on an implicit TRANSFER queue; **`gpu_queue_transfer_copy(ctx, queue,
  src, dst, bytes)`** is the explicit-queue variant. Both route through the
  new `BACKEND_SLOT_BUFFER_COPY` and enforce a 4-byte-multiple size.
- **Real native `gpu_buffer_*`** — `NativeBuf` (page-aligned GTT BO + VA
  sub-allocation); `create` / `write` / `read` / `release` are bounds-checked
  memcpy over the coherent mmap (the v3.0 diagnostic stubs are retired).
- **`native_transfer_copy_timeline`** — SDMA driver on the DMA ring: chained
  `COPY_LINEAR` packets, 4-entry BO residency, in-CS `TIMELINE_WAIT` from a
  cross-ring barrier, timeline-signalling submit.
- **`native_sdma_build_copy_chained`** — splits a copy across N ≤4 MiB
  packets (per-chunk VA offsets) so one submission moves buffers larger than
  a single packet's 22-bit count field allows.
- **`GPU_ERR_TRANSFER`** (20) — a compute/render dispatch routed onto a
  DMA/TRANSFER queue, or a copy onto a non-DMA queue, is rejected with it.
- e2e programs `native_transfer_copy_e2e` (3 legs, HW-verified) +
  `wgpu_transfer_copy_e2e` (serialized verify); `make test-native-transfer-copy-e2e`
  / `test-wgpu-transfer-copy-e2e`.

### Changed

- The logical TRANSFER queue now maps to a real **`AMDGPU_HW_IP_DMA`** ring
  (was a COMPUTE-ring fallback); compute dispatch onto a DMA-ring queue is
  rejected (`GPU_ERR_TRANSFER`) before any PM4.
- `Backend` 256 → 264 B (buffer_copy slot); `backend_is_complete` walks it
  (8th range, activated once both backends fill it).
- `_backend_wgpu_buffer_create` OR's in `COPY_SRC|COPY_DST` (MAP-aware) so
  every mabda-created wgpu buffer is a valid copy operand — matching native's
  usage-agnostic linear BOs.
- **Toolchain pin 6.2.10 → 6.2.11.**

### Fixed

- P(-1)/work-loop audit (`docs/audit/2026-06-15-buffer-copy-audit.md`,
  adversarial workflow — 7 confirmed / 4 dismissed): **(HIGH)** the public
  copy now rejects non-4-byte-multiple sizes uniformly, closing a
  succeed-on-native / fault-on-wgpu split; **(HIGH)** SDMA copies >4 MiB are
  chained (were silently corrupting via the 22-bit count field); **(MED)**
  the COPY-usage asymmetry above. Each landed with a regression assertion.

### Limitations / Deferred (within the arc)

- **Producer/consumer overlap** through the single per-context cached IB is
  not supported (the consumer's packet would clobber the producer's); the
  transfer-copy e2e serializes with `wait_idle` like `native_multiqueue_e2e`.
  True overlap needs per-submission IB staging → **R.5 (3.2.13)**.
- A single SDMA submission tops out at ~146 packets (~584 MiB); larger copies
  (multi-submission) are future work, well beyond realistic buffer sizes.

### Security

- Adversarial review of the diff — **0 CRITICAL / 2 HIGH / 1 MEDIUM**, all
  fixed before the cut. `docs/audit/2026-06-15-buffer-copy-audit.md`.

### Metrics

- CPU assertions 2434 → **2530**. `Backend` 256 → 264 B. New error code
  `GPU_ERR_TRANSFER` (20).

### Next

- **Phase TS (3.2.2–3.2.3)** — native compressed-texture *sampling* (GFX9
  T#/tiling/sampler), then Phase S/N SPIR-V, Phase F f64, Phase R
  render-graph multi-queue.

## [3.2.0] — 2026-06-15

**Compressed textures (BC / ETC2 / ASTC).** First feature of the v3.2.x
"texture & shader breadth" arc (`docs/development/3-2-punchlist.md`
Phase T; `docs/proposals/v3.2-compressed-textures.md`). A consumer creates
a texture in a block-compressed format and uploads the opaque blocks; the
upload threads the block-derived layout (`bytesPerRow` in block-rows). Same
public API both backends — **wgpu** creates + uploads + samples (HW BC
decode), **native AMD** stores + reads back byte-identical (sampling lands
in **Phase TS, this arc** — not deferred). HW-verified on Cezanne:
`native_compressed_store_e2e` (BC1+BC7 round-trip) and the wgpu
`compressed_texture_e2e` (BC1 byte-exact copy-back). CPU suite 2265 →
**2434** assertions. Toolchain pin 6.2.8 → **6.2.10**.

### Added

- **`gpu_texture_create_2d(ctx, w, h, fmt)`** — format-parameterized 2D
  texture create; `gpu_texture_create_2d_rgba8` is the RGBA8 fast path.
- **`src/texture_format.cyr`** — `MABDA_TEXFMT_*` ids (RGBA8 + BC1/3/4/5/6H/7,
  ETC2 RGB8/RGBA8, ASTC 4x4/6x6/8x8); block geometry accessors;
  `mabda_texfmt_to_wgpu` (v29 `WGPUTextureFormat`, header-pinned);
  `blocks_for_dim`/`data_size` block math with an explicit overflow/dim
  bound; `MABDA_TEXCOMP_*` families + `gpu_caps_supports_format`.
- **Capability gating** — `GpuCapabilities` texture-compression bitset +
  `gpu_caps_texture_compression`; `gpu_caps_detect_texture_compression`
  (wgpu, from `wgpuAdapterHasFeature`); `GPU_ERR_FORMAT_UNSUPPORTED`.
- Backend create slot filled on both backends (block-aware wgpu upload;
  block-sized page-aligned native linear BO). e2e programs +
  `make test-compressed-texture-e2e` / `test-native-compressed-store`.

### Changed

- `Backend` 248 → 256 B (texfmt create slot); `backend_is_complete` walks it.
- `GpuCapabilities` 120 → 128 B; `NativeTexture` stashes fmt at +44.
- The reference launcher (`deps/wgpu_main.c`) now enables the adapter-
  supported compressed device features at device creation (without it,
  `wgpuDeviceCreateTexture` aborts on a compressed format).
- **Toolchain pin 6.2.8 → 6.2.10.**

### Fixed

- **The entire wgpu program suite was un-buildable** — the Makefile called
  `cc5`, which cyrius 6.1 renamed `cycc`; fixed. Their hand-maintained
  selective include lists had also rotted (the native backend's
  `compute`/`queue`/`texture_format` deps outgrew them since v3.1) — all
  switched to `src/lib.cyr`. `phase0`/`render_e2e`/`compute_e2e` etc. build
  + run + pass on Cezanne again.
- P(-1) audit fixes (`docs/audit/2026-06-15-audit.md`): wgpu compressed
  read fails loud (`NOT_IMPLEMENTED`) instead of mis-validating; wgpu
  create gained the redundant dim cap + an adapter-family pre-check
  (graceful 0 instead of a process abort on an unsupported family).

### Limitations / Deferred (within the arc)

- **Native compressed *sampling*** is Phase TS (3.2.2–3.2.3, this arc) —
  it re-introduces the GFX9 T#/tiling/sampler path. Native is storage +
  readback this minor.
- **wgpu compressed readback via `gpu_texture_read`** is fail-loud
  (`NOT_IMPLEMENTED`) — needs 256-padded block-row handling; consumers use
  a padded copy-back meanwhile. Native compressed read works.
- **ETC2/ASTC native decode** is HW-verification-gated (BC verified on
  Cezanne; ETC2/ASTC need capable AMD silicon) — see the punchlist
  Hardware-verification gaps.

### Security

- P(-1) adversarial audit of the diff — **0 HIGH / 0 MEDIUM**, 5 LOW all
  fixed. `docs/audit/2026-06-15-audit.md`.

### Next

- **Phase X (3.2.1)** — TRANSFER→DMA ring + public buffer-copy; then
  Phase TS native sampling, Phase S/N SPIR-V, Phase F f64.

## [3.1.1] — 2026-06-15

**Multi-queue coordination (native AMD).** Second feature of the v3.1.x
arc (`docs/development/3-1-punchlist.md` Phase Q;
`docs/proposals/v3.1-multiqueue.md`). A logical queue abstraction
(`GRAPHICS` / `COMPUTE` / `TRANSFER`) lets a consumer dispatch onto
distinct hardware rings and order cross-ring work with a barrier. **Native
AMD is HW-verified on Cezanne**: compute on the COMPUTE ring → barrier →
graphics on the GFX ring (in-CS timeline wait) → an SDMA copy on the DMA
ring that consumes compute's output — all three rings timeline-ordered
(`programs/native_multiqueue_e2e.cyr`). Same public API on both backends;
wgpu aliases all logical queues onto its single device queue (no real
overlap — see Limitations). No change to existing public API. CPU suite
2102 → **2265** assertions. Toolchain pin 6.2.6 → **6.2.8**.

### Added

- **Logical queue API** (`src/queue.cyr`): `gpu_queue_get(ctx, kind)`,
  `gpu_queue_barrier(ctx, src, dst)`, `gpu_queue_wait_idle(ctx, queue)`,
  and `gpu_ctx_set_current_queue` to target dispatches. `QUEUE_KIND_*`
  constants + a 48-byte `Queue` (kind / ring / persistent timeline
  syncobj / point / pending cross-ring wait) + a ctx-side queue table.
- **Native timeline syncobjs**: `native_syncobj_timeline_wait` /
  `_query` ioctls + a `native_cs_submit_timeline` driver
  (`BO_HANDLES + [TIMELINE_WAIT] + TIMELINE_SIGNAL + FENCE + IB`). Each
  native logical queue carries one persistent timeline syncobj; every
  queue-targeted submit signals the next point on GPU completion.
  ABI ground-truthed against `drm.h`/`amdgpu_drm.h` (kernel 7.0).
- **Cross-ring barrier**: `gpu_queue_barrier` records a pending wait on
  the consumer queue; its next submit carries it as an in-CS
  `SYNCOBJ_TIMELINE_WAIT` (with `WAIT_FOR_SUBMIT`), so the dependency
  resolves GPU-side with no CPU stall — genuine GFX/COMPUTE overlap.
- **Queue-targeted native dispatch**: `_backend_native_compute_dispatch`
  routes through the current-queue stash to that queue's ring + timeline
  (`programs/native_queue_compute_e2e.cyr`,
  `programs/native_queue_barrier_e2e.cyr`).
- **SDMA copy foundation** (TRANSFER): `native_sdma_build_copy_linear`
  (SDMA v4 `COPY_LINEAR`) + `AMDGPU_HW_IP_DMA`. The DMA-ring submit is
  HW-verified (`programs/native_sdma_copy_e2e.cyr`: 4 KiB copied
  byte-identical, 0 ms, no TDR).
- wgpu queue fillers: logical queues alias the single `WGPUQueue`;
  `queue_barrier` is a submit-order no-op, `queue_wait_idle` a device
  poll.

### Changed

- `Backend` struct 224 → 248 B (three queue slots, append-after-KIND);
  `backend_is_complete` walks the new range (both backends fill it).
- `GpuContext` 128 → 160 B (queue table at +128..+152, outside the
  +0..+24 dual-interpretation region).
- **Toolchain pin 6.2.6 → 6.2.8** (`cyrius.cyml`). Full gate re-run green
  on 6.2.8.

### Limitations / Deferred

- **wgpu multi-queue has no real overlap.** WebGPU exposes one device
  queue; all three logical queues alias it and run in submit order. The
  barrier degrades to an ordering pin. Consumers needing genuine
  concurrency use the native backend.
- **TRANSFER queue uses the COMPUTE-ring fallback in 3.1.1.** The SDMA
  copy path is HW-proven, but the `TRANSFER → AMDGPU_HW_IP_DMA` flip + a
  public buffer-copy op (with wgpu parity) land together in **3.1.2** —
  a DMA-ring queue with no public copy op would only be a footgun today.
- **Render-graph multi-queue scheduling** (per-node queue affinity +
  cross-queue fence edges) is scoped to **3.1.2** (Phase Q.7), not a
  3.1.1 blocker.

### Next

- 3.1.2: TRANSFER → SDMA ring + public buffer-copy API (wgpu parity) +
  render-graph multi-queue scheduling. Then six-consumer regression sweep
  (Tier 2 ship work) before the v3.1.x line settles.

## [3.1.0] — 2026-06-15

**On-device mipmap generation (native AMD).** First feature of the v3.1.x
arc (`docs/development/3-1-punchlist.md` Phase M;
`docs/proposals/v3.1-mipmap-generation.md`). A consumer creates a texture
with N mip levels, writes level 0, and asks mabda to fill levels 1..N-1
by GPU 2×2 box-filter downsample. **Native AMD is fully implemented and
HW-verified on Cezanne** (gfx90c); the public API is the same on both
backends but wgpu `generate` is deferred (see Limitations). No change to
existing public API. CPU suite 1991 → **2102** assertions, reorganized
into 11 functionality-named domain files. Verified green on 6.2.6.

### Added

- **`gpu_texture_create_2d_rgba8_mipped(ctx, w, h, mip_count)`** — create
  a 2D RGBA8 texture with a mip chain (`mip_count == 0` = full chain).
- **`gpu_texture_generate_mipmaps(ctx, tex)`** — GPU-fill levels 1..N-1
  from level 0. Native: one compute dispatch per level. wgpu: see below.
- Native mipmap stack: contiguous mip-chain BO + a per-context texture VA
  sub-allocator; a hand-authored GFX9 ISA 2×2 downsample compute shader
  (`native_gfx9_shader_downsample_2x2`, llvm-mc round-trip-verified) using
  flat `global_load`/`global_store` (native textures are linear — no image
  descriptor table needed); a PM4 compute-downsample composer
  (`native_pm4_build_compute_downsample`, RSRC1/RSRC2 for the shader's 16
  VGPR / 24 SGPR + TGID enables). `programs/native_mipmap_e2e.cyr` proves
  generated levels are byte-exact vs a CPU box-filter on real hardware.
- wgpu: `wgpu_texture_descriptor_mipped`, per-level view descriptor +
  sampled/storage bind-group-layout entry builders (v29-offset-verified).

### Changed

- `Backend` struct 208 → 224 B (two mipmap slots, append-after-KIND);
  `backend_is_complete` walks the new range.
- `GpuContext` 120 → 128 B (per-context texture VA cursor at +120).
- `NativeTexture` 32 → 48 B (width/height/mip_count); offsets 0..24
  unchanged. Native texture VA region moved to `0xFFFF800180000000`
  (2 GiB) so a mip chain fits.
- **Tests reorganized by functionality** — the version-named
  `mabda.tcyr` / `mabda_v3.tcyr` / `mabda_v3_phase_d.tcyr` trio is
  replaced by 11 domain-named suites under `tests/tcyr/` (core, buffer,
  compute, texture, graphics, render, backend, caches, surface, native,
  kms); Makefile/CI/release glob `tests/tcyr/*.tcyr`. Every file now sits
  under the 128 KiB lint/fmt cap (the old `mabda_v3.tcyr` had reached
  148 KiB, past the cap — its tail was silently unchecked).

### Fixed

- **Native `GEM_VA` map requires a 4 KiB-page-aligned size.** Sub-page
  BO/VA-map sizes failed (`rc=99`); the mip-chain BO and the downsample
  shader BO are now page-aligned. (Single-level textures were unaffected —
  all current sizes are page-aligned.)

### Limitations / Deferred

- **wgpu `gpu_texture_generate_mipmaps` returns `GPU_ERR_NOT_IMPLEMENTED`.**
  It needs a real wgpu compute pipeline, but the wgpu compute path itself
  (`_backend_wgpu_compute_dispatch`) has been a stub since v3.0. wgpu
  mipmap *generation* is therefore blocked on a wgpu-compute project
  (v3.x). wgpu `create_mipped` works, so a wgpu consumer can create a
  mipped texture and upload all levels manually meanwhile. The native
  backend is the working generation path.
- Multi-queue (the arc's second feature) remains v3.1.1+ — see the
  punchlist Phase Q.

## [3.0.4] — 2026-06-15

**P(-1) security-hardening patch.** First full-surface security audit
since 3.0.0 GA (all 38 modules, not just a delta) and since the cyrius
6.0 → 6.2 toolchain jump. The 2026-04-30 audit's findings held: all 5
prior ship-blockers and 9/11 deferred items are verified fixed in tree.
This patch lands the new findings — 4 HIGH, 5 MEDIUM, 7 LOW — each
HIGH/MED with a regression assertion. CPU suite grew 1957 → **1991**
assertions; no public API change; verified green on 6.2.6 (lint / fmt /
vet / dist all clean). Full report:
[`docs/audit/2026-06-14-audit.md`](docs/audit/2026-06-14-audit.md).

The dominant theme: the v3 backend-abstraction dispatchers were added as
thin `ctx->backend->slot` pass-throughs and dropped the input validation
their v2 siblings still enforce. Fixed uniformly.

### Fixed

- **[HIGH-1] `gpu_texture_create_2d_rgba8` now validates dimensions.**
  The v3 dispatcher forwarded consumer width/height with no positivity /
  upper-bound / overflow check (its v2 sibling `texture_create_rgba8`
  enforces `MABDA_MAX_TEXTURE_DIM_2D`). Unvalidated `width*height*4` could
  reach the native GEM_CREATE ioctl + mmap. Guard added at the dispatcher
  and at the native slot (`_backend_native_texture_create_2d_rgba8`).
- **[HIGH-2] logind-acquired DRM master fd no longer leaks on release.**
  On the `gpu_surface_configure_native_logind` success path mabda owns
  the samvada-delegated master fd, but `gpu_surface_release` only zeroed
  the ctx stash — never `samvada_session_release_device` + `sys_close`.
  Every successful configure→release cycle leaked an fd and held DRM
  master for the process lifetime. Fixed via per-surface provenance
  (`NATIVE_SURFACE_MASTER_OWNED` / `_MINOR`, in the struct's free tail):
  the release slot now returns master to logind and closes the fd; kiosk
  (consumer-owned) fds are untouched. (Distinct from the 2026-04-30
  HIGH-2, which fixed only the configure *failure* paths.)
- **[HIGH-3]/[MED-4] WGSL shader source no longer truncated at 4 KiB.**
  `wgpu_shader_source_wgsl` derived its StringView length from the 4 KiB
  `WGPU_LABEL_MAX_BYTES` label cap, silently truncating any shader ≥ 4 KiB
  (wgpu-native reads exactly `StringView.length` bytes → compile failure
  or wrong shader). The v3 slot also discarded the explicit length `n` it
  was handed. Added `wgpu_shader_source_wgsl_len` (used by the slot with
  the real `n`) and raised the cstr-path scan to a 1 MiB shader-source cap.
- **[HIGH-4] cache keys 0 / `0xFFFFFFFFFFFFFFFF` no longer corrupt the
  cache.** `pipeline_cache` and `bind_group_cache` passed caller-supplied
  u64 hash keys straight to `map_u64_*`, whose reserved EMPTY (0) / TOMB
  (MAX) sentinels silently dropped those two keys (and key=0 corrupted the
  occupied-count). Keys now route through `_cache_safe_key`, which remaps
  the two reserved values out of band.
- **[MED-1]/[MED-2] native texture/render-target size overflow guards.**
  `native_texture_size_2d_rgba8` / `native_rt_size_2d_rgba8` computed
  `width*height*4` with no upper-dimension cap before GEM_CREATE + mmap.
  The dimension cap (subsumes the prior deferred MED-2 render-path twin
  [LOW-5] — the 14-bit CB_COLOR0_ATTRIB2 extent can no longer wrap).
- **[MED-3] `gpu_compute_dispatch` now validates workgroup counts.**
  The v3 compute dispatcher dropped the `<=0` + `MABDA_MAX_DISPATCH_DIM`
  (65535) guards its v2 sibling enforces; restored at the dispatcher and
  the native slot so out-of-range counts can't reach PM4 + CS submit.
- **[MED-5] `gpu_timestamps_new` null-checks buffer creation.** The two
  `wgpu_device_create_buffer` returns were stored unchecked; on VRAM
  exhaustion the wrapper held null handles later deref'd in wgpu-native.
  Now releases acquired resources and returns `Err`.
- **[MED-6] test no longer shadows production `present_mode_to_wgpu`.**
  `mabda.tcyr` re-defined it (and the `WGPU_PRESENT_MODE_*` vars) verbatim;
  under "last definition wins" the test validated its own stale copy, and
  the duplicate-fn build warning tripped the zero-warning gate. Removed —
  the test now exercises the real `src/surface.cyr` symbol.
- **[LOW-1]/[LOW-2] struct-header comment corrections.**
  `NativeRenderPipeline` ("320 bytes" → real 644) and `RenderTarget`
  ("48 bytes" → real 64) header comments now match the load-bearing size
  constants. Runtime was already safe; the comments were the editing hazard.
- **[LOW-3] `surface_state_new` / `_resize` reject non-positive dims.**
  The v2 surface path used `== 0` guards; a negative dim truncated to a
  huge u32 extent at the wgpu boundary. Now `<= 0`, matching the v3 path.
- **[LOW-4] `clock_gettime` return checked in native dispatch.** Both
  native compute/render dispatch sites discarded the syscall return and
  read an uninitialized `t_now`, risking a garbage syncobj deadline on
  failure. Now `memset` + return-check at both sites.
- **[LOW-6] `gpu_timestamps_resolve`/`_map` guard negative counts.** A
  negative `query_count` survived the upper-bound clamp and fed `count*8`
  as a huge unsigned size into wgpu; clamped below at 0.

### Security

- Full-surface audit + amdgpu/DRM + wgpu/WebGPU CVE sweep
  (2026-04-30 → 2026-06-14). No CRITICAL findings; no remotely-reachable
  issue under the trusted-caller model. The two in-window amdgpu CVEs
  whose bug-classes touch mabda's hand-laid ioctls were code-verified as
  **non-reachable**: CVE-2026-23468 (BO-list entry count) — mabda
  hardcodes `bo_number = 5` (the fixed BO set), not a consumer-controlled
  count; CVE-2026-46220 (SDMA4 `BUG_ON(addr & 0x3)`) — mabda submits only
  to GFX/COMPUTE (never SDMA) and passes a fixed, DWORD-aligned
  `fence_offset = 32`.
- **Defense-in-depth: `native_cs_submit_4chunk` now rejects a
  non-DWORD-aligned `fence_offset`** with `-EINVAL` before the CS ioctl.
  Guards a future regression (a new caller or an SDMA submit path) from
  reaching the kernel BUG_ON the CVE-2026-46220 class describes; a no-op
  for today's fixed offset. Regression test added.

## [3.0.3] — 2026-06-14

**Toolchain + dep tracking patch.** Documents the cyrius pin already at
`6.2.6` in tree and advances the `samvada` dep `0.2.2` → `0.4.1`. No
public API change — the samvada surface mabda consumes (`samvada_main`,
`samvada_session_take_device`, `samvada_session_release_device`) is
unchanged across the jump. Verified green on 6.2.6: `cyrius deps`
resolves clean (samvada 0.4.1, 29 deps locked), smoke build links,
per-file `cyrius lint` 0 warnings, `cyrius vet` clean, full `.tcyr`
suite **1957/1957** (624 + 951 + 382), CPU benches run clean, and
`dist/mabda.cyr` regenerated via `cyrius distlib` (v3.0.3).

### Changed

- **cyrius pin `6.2.1` → `6.2.6`.** The `6.2.6` pin landed in tree with
  the "language bump to 6.2" commit; 3.0.3 documents it and re-burns the
  bundle on it. samvada 0.4.1 pins the same `6.2.6` toolchain.
- **samvada dep `0.2.2` → `0.4.1`** (`[deps.samvada] tag`). samvada's
  0.2.2 → 0.3.0 → 0.4.0 → 0.4.1 line is a chain of certified
  no-public-API-change toolchain-tracking releases; every exported
  symbol's signature and error-code contract is unchanged from 0.2.2.
  `cyrius deps` re-resolves `lib/samvada.cyr` to the 0.4.1 bundle
  (`samvada_version()` → `(0,4,1)`); `cyrius.lock` updated.

### Fixed

- **`make fmt-check` aligned to `cyrfmt --check` exit-code semantics.**
  cyrius 6.x's `cyrfmt --check` reports drift via exit code only and no
  longer echoes the formatted file to stdout, so the Makefile's old
  diff-against-stdout gate false-failed every file on 6.2.x. CI already
  carried this fix (`.github/workflows/ci.yml`); the local gate now
  matches it.

## [3.0.2] — 2026-06-12

### Changed

- **cyrius pin `6.0.43` → `6.2.1`** (ecosystem-wide stdlib pin sweep onto the
  current toolchain). No source changes — mabda's `[deps]` carries no carved-out
  modules and its deps (sakshi, samvada) are unaffected. Verified green on 6.2.1:
  `cyrius deps` resolves cleanly, full `.tcyr` suite 382/382, bench 3/3,
  `dist/mabda.cyr` regenerated via `cyrius distlib`.

## [3.0.1] — 2026-06-02

**Toolchain refresh + 6 h confirmation soak.** First 3.0.x patch:
advances the cyrius pin to the latest release and re-burns the bundle
on it. No public API change.

### Changed

- **Toolchain pin `6.0.27 → 6.0.43`.** GA shipped on the soaked 6.0.27;
  3.0.1 tracks the latest cyrius. Full gate sweep clean on 6.0.43 —
  1957 CPU asserts, lint / fmt / vet / dist all green, no fmt drift.

### Fixed

- **`scripts/soak.sh` hands its logdir back to `$SUDO_USER` at exit.**
  The runner needs sudo for dmesg capture, which left the committed
  soak artifacts root-owned — and root-owned tracked files break a
  later `git checkout` / merge on the untracked working-tree copies
  (the 3.0.0 GA-merge papercut). It now `chown`s `$LOGDIR` back to the
  invoking user. See
  [`docs/issues/2026-06-01-soak-stale-binary.md`](docs/issues/2026-06-01-soak-stale-binary.md).

### Soak

- **6 h confirmation soak on 6.0.43 cleared** (`--workload=all`,
  `docs/handoff/soak-20260603T012856Z/`): full 6 h end-to-end with a
  clean `exit=0`, RSS flat at 13 444 KB (0 % drift), dmesg Δ = 0, 0
  FAIL. Iters: compute 12.3 M, render 9.6 M, wgpu 583 K. Validated
  both soak.sh fixes from the GA cut — the monitor ran to completion
  (no infant-mortality hang) and handed the logdir back to the
  invoking user.

## [3.0.0] — 2026-06-02

**General availability — dual-backend GPU foundation.** mabda 3.0.0
ships the wgpu (C-launcher) and native AMD (DRM-ioctl) backends behind
one unchanged public API, routed through the `Backend` slot table —
the load-bearing architectural choice of the v3.0 cycle. The 24-hour
GA soak gate is cleared: the rc.3 bundle, rebuilt on cyrius **6.0.27**,
ran `--workload=all` (compute + wgpu + render in parallel) for
**26 h 13 m** — RSS flat at 13 092 KB (0 % drift), dmesg Δ = 0, all
three workloads green throughout, including sustained desktop video
contention and a live VT-switch / DRM-master handoff mid-run (the
render-node paths need no master and ran uninterrupted). Full result
tree in `docs/handoff/soak-20260601T222652Z/`.

**Metrics**: 38 src/ modules / ~14,500 LoC / **1957 CPU asserts**
(624 + 951 + 382 across three test files) / `dist/mabda.cyr` 12,372
lines / 8 GPU integration programs / cyrius pin **6.0.27** /
samvada `0.2.2`.

### Added

- **24-hour GA soak gate cleared.** `scripts/soak.sh --workload=all
  --stop=24h` on the 6.0.27-built bundle. Final documented state at
  26 h 13 m (`docs/handoff/soak-20260601T222652Z/`):
  `iters_compute = 53.9 M` (native AMD PM4 dispatch + 0xDEADBEEF
  readback, every iteration verified), `iters_render = 41.6 M`
  (native GFX-ring clear-triangle + pixel verify),
  `iters_wgpu = 2.49 M` (`render_graph_e2e` 3-node DAG through
  wgpu-native), `rss_kb = 13 092` (0 % drift vs t=1s),
  `dmesg_delta = 0`. The run exceeded the 24 h gate; the 3-day
  observation window is 3.0.x territory.

### Changed

- **Toolchain pin `5.11.64 → 6.0.27`.** The 6.0.0 line renamed the
  test-runner assert intrinsics; the three `.tcyr` suites were swept
  to the new form and the GA bundle was soaked on 6.0.27.
  `dist/mabda.cyr` regenerated on 6.0.27 (cosmetic module-separator
  normalization).

### Fixed

- **`scripts/soak.sh` stale-binary guard.** The runner only built a
  workload binary when it was *missing*, so a pre-existing-but-stale
  `build/` artifact (older than its `src/*.cyr` deps, or built on a
  superseded toolchain) silently soaked the wrong bundle. Now invokes
  `make` unconditionally and lets mtime deps resolve staleness. See
  [`docs/issues/2026-06-01-soak-stale-binary.md`](docs/issues/2026-06-01-soak-stale-binary.md).
- **`scripts/soak.sh` monitor-death robustness.** `nohup sudo soak.sh`
  shielded only the outer `sudo`; a SIGHUP on session teardown (or
  SIGPIPE on the checkpoint `tee`) could kill the monitor process
  mid-run while the orphaned workload loops ran on silently. Added
  `trap '' HUP PIPE` plus a `MONITOR_PID` liveness check so workload
  loops self-exit if the monitor dies. (Surfaced ~15 min before the
  24 h finish on the GA soak; workloads were unaffected — they ran
  clean past 24 h.)

### Notes

- **Carried to 3.0.x:** 6-consumer regression sweep (soorat / rasa /
  ranga / bijli / aethersafta / kiran-via-soorat), the master-gated
  `present` workload soak, and the 24 h → 72 h observation window.
  None gate GA. Next planned patch: **3.0.1** pinned to the latest
  cyrius release with a 6 h confirmation soak.
- rc.1 – rc.4 detail is retained in the sections below; this entry is
  the GA consolidation.

## [3.0.0-rc.3] — 2026-05-19

**6-hour soak gate cleared.** Two GFX9-ISA root causes that were
masking as PM4 issues for the entire rc.2 → rc.3 window are fixed in
tree (EXP opcode byte + SPI_SHADER_COL_FORMAT field values; see
[`docs/issues/2026-05-13-native-render-cezanne-tdr.md`](docs/issues/2026-05-13-native-render-cezanne-tdr.md)).
17 supporting register-correctness improvements landed alongside.
The rc.3 6 h soak (`--workload=all` = compute + wgpu + render in
parallel, per `docs/development/3-0-rc-3-punchlist.md`) ran clean
end-to-end: **33 / 33 checkpoints PASS, 21.6 M total iterations, RSS
12 760 KB flat, dmesg Δ = 0**. Toolchain pin advanced
`5.7.48 → 5.11.64` (samvada already on 0.2.2). Official `3.0.0` is
next: rc.4 takes this bundle into the 24 h GA gate.

**Metrics**: 38 src/ modules / ~14,500 LoC unchanged / **1957 CPU
asserts** across 3 test files (was 1871 at rc.2; +86 from the
mabda_v3 assert rebalance landed alongside the TDR fix) /
`dist/mabda.cyr` ~11,800 lines / 8 GPU integration programs.

### Fixed — 2026-05-13 (native render Cezanne TDR root-cause)

- **EXP instruction encoding (TDR cause).** The hand-encoded GFX9
  `exp` instructions in
  `native_gfx9_shader_fullscreen_triangle_vs` and
  `native_gfx9_shader_solid_red` used top byte `0xF8` (claiming the
  GFX9 EXP opcode); the correct encoding has bits 31-26 = `0x31`,
  i.e. top byte `0xC4`. `llvm-mc --disassemble --arch=amdgcn
  --mcpu=gfx90c` flags `0xF8` as `invalid instruction encoding`;
  `0xC4` decodes as `exp pos0 …` and `exp mrt0 … vm` cleanly.
  Symptom: VS waves ran every preceding instruction, then the
  malformed `exp` silently failed to publish vertex position, VGT
  waited forever for output, 2-second GFX-ring TDR. FS hit the
  same wall one stage later. Fix is in
  `src/backend_native_shaders.cyr`; regression coverage is the
  `scripts/disasm-shaders.sh` byte-check now wired into the
  pre-soak verification recipe per the
  [[feedback_verify_gfx9_shader_bytes_with_llvm_mc]] memory.
- **`SPI_SHADER_COL_FORMAT` field values (zero-pixel cause).** Past
  the EXP fix the GFX ring stopped hanging at dispatch=0 ms, but
  the RT came back `(0x00, 0x00, 0x00, 0x00)` instead of the FS's
  exported `(0xFF, 0x00, 0x00, 0xFF)` UNORM8 conversion. Root
  cause: mabda's three SPI_SHADER_COL_FORMAT constants were all
  off by a few slots, and `FP32_ABGR = 0xE` happens to be reserved
  in the GFX9 SPI 4-bit field — SX silently routes reserved values
  to `SPI_SHADER_ZERO`, so the FS export never reached the CB.
  Corrected to the GFX9 PAL spec table values
  (`FP16_ABGR = 0x4`, `UNORM16_ABGR = 0x5`, `FP32_ABGR = 0x9`) in
  `src/backend_native_shaders.cyr`.
- **17 supporting register-correctness improvements** landed in the
  same session before the EXP/COL_FORMAT discoveries surfaced.
  Highlights: `VGT_SHADER_STAGES_EN = 0x00010000` (was 0 = VS_OFF
  — a real bug, just not the TDR cause), `SPI_SHADER_PGM_RSRC3_VS
  = 0x003FFFFE` (CU enable mask), `SPI_SHADER_LATE_ALLOC_VS = 24`,
  plus the broader sweep documented in the TDR issue file. None
  caused the hang on their own; collectively they push mabda's PM4
  stream closer to the radv byte-exact target the
  `programs/native_pm4_dump` capture harness verifies against.

### Changed — 2026-05-13

- **`tests/tcyr/mabda_v3.tcyr` assert rebalance** — 124 lines removed,
  293 added in the "fixing asserts" pass; counts settle at 951 (was
  858 at rc.2). The v3 suite now exercises the corrected SPI/EXP
  values directly so the byte-encoding regressions caught above
  show up as structural test failures, not as silent runtime
  zero-pixel returns.
- **`scripts/soak.sh` hardening** for the rc.3 / rc.4 gates: the
  workload bundle keys (`all` / `native` / `both`), the dynamic
  per-workload CSV columns (`iters_<name>`), and the
  exponential-then-15-min checkpoint cadence all date from this
  pass. `--workload=all` is the new rc.3/rc.4 primary; the
  pre-rc.3 `both` alias is retained for back-compat.
- **`programs/benchmarks.cyr`** picked up the wgpu-side benches
  that complete the Rust-v1 parity grid on the wgpu path.

### Unblocked — 2026-05-19

- **rc.3 6 h soak.** Full result tree in
  `docs/handoff/soak-20260519T072831Z/` (`soak.log`, `soak.csv`,
  per-workload `iterations/`, dmesg baseline + final). Run:
  `scripts/soak.sh --workload=all --stop=6h`. Final state from
  the CSV last row (`t=21600s`):
  - `iters_compute = 12 030 710` (native AMD compute, PM4 dispatch
    + 0xDEADBEEF readback, every iteration verified)
  - `iters_wgpu = 602 971` (`render_graph_e2e` — 3-node DAG
    compute → render → copy through wgpu-native)
  - `iters_render = 9 278 402` (native AMD GFX-ring clear-triangle
    + pixel verify — the path the TDR bugs above were blocking)
  - `rss_kb = 12 760` (identical to t=1s — no bump-allocator
    monotonic growth across 6 hours, validating the rc.2
    MED-7+LOW-5 PM4-scratch leak fix in long-form)
  - `dmesg_delta = 0` (no new `amdgpu`/`drm` lines vs baseline; no
    `dmesg.diff` artifact written — that's FAIL-only)
- **rc.4 24 h GA gate** is the next soak. Same `--workload=all`,
  `--stop=24h`, same logdir shape. Reference recipe:
  `docs/development/3-0-rc-4-punchlist.md`.

### Next

- **rc.4 24 h GA gate** — `scripts/soak.sh --workload=all --stop=24h`.
  Clean = earliest cut for 3.0.0 GA; the full 3-day window is
  3.0.x territory per the rc.4 punchlist.
- **6-consumer regression sweep** against the rc.3 bundle (soorat /
  rasa / ranga / bijli / aethersafta / kiran-via-soorat) — still
  owed from the rc.3 punchlist exit list. Parallelizable to a
  sub-agent; file any regressions at
  `docs/issues/2026-05-MM-<project>-rc3-regression.md`.
- **`present` workload soak** (`programs/native_present_e2e`,
  120-frame animated gradient) is still gated on a tty / kiosk
  session or a samvada-wired consumer holding DRM master — see
  the [[project_phase_d_master_logind_blocker]] memory. Not a
  3.0.0 blocker (KMS Phase D is feature-complete; the soak is
  observation), but useful coverage before the 24 h cut if the
  session shape allows.
- VERSION 3.0.0-rc.3 → 3.0.0-rc.4 → 3.0.0 once the 24 h-clean gate
  passes.

## [3.0.0-rc.2] — 2026-05-01

**Audit-track closeout cut.** Every deferred item from the
[2026-04-30 audit](docs/audit/2026-04-30-audit.md) (10 findings: 4
MED, 5 LOW, plus the bundled MED-7+LOW-5 PM4-scratch leak) is fixed
in tree, plus the two toolchain-side scaffold items the rc.2
punchlist gated on (`backend_native.cyr` 4-way split + CI fmt-
truncation guard). Official `3.0.0` is the next cut, after the
6-consumer regression sweep + 3-day soak (`rc.3` if anything
regresses).

**Metrics**: 38 src/ modules (was 35; backend_native.cyr split into
`_amdgpu.cyr` / `_shaders.cyr` / `_pm4.cyr` / `.cyr` to land under
cyrius lint/fmt's 128 KiB cap) / ~14,500 LoC unchanged / **1871 CPU
asserts** across 3 test files (was 1828 at rc.1; +43 from audit
fixes) / `dist/mabda.cyr` ~11,800 lines / 8 GPU integration programs
(rc.1's 7 + new `native_pm4_dump` for the radv_capture Phase 2
byte-diff harness — CI-safe, no GPU needed).

### Fixed — 2026-05-01 (audit-track closeout)

- **MED-2 — caller-supplied dimension overflow guards** in the
  scanout path. New `_kms_validate_fb_dims` helper in
  `src/backend_native_kms.cyr` caps `(width, height)` at 16384 each
  (WebGPU `MAX_TEXTURE_DIMENSION_2D`, also the AMD single-pipe scanout
  limit), computes the 256-byte-aligned pitch, and rejects total BO
  bytes above 2 GiB. Wired into `native_kms_alloc_fb` and
  `native_kms_modeset_first_connected` — EDID-fed modes that exceed
  the caps now fail fast (`-1` from alloc_fb, `-7` from modeset)
  before any BO allocation. 18 new asserts in
  `tests/tcyr/mabda_v3_phase_d.tcyr` covering 1080p valid math, 16384²
  at the cap, just-over rejects, non-positive, null out-ptrs, plus
  the boundary-rejection asserts the audit asked for (16385 / 65537 /
  negative / overflow).
- **MED-4 — two-pass DRM discovery TOCTOU clamping.** After the
  fill-pass ioctl in `native_kms_init` and `native_kms_get_connector_modes`,
  clamp `actual = min(actual, capacity)` against the pre-fill cap so a
  hot-add race or future kernel quirk can't cause downstream code to
  read past the heap arrays sized off pass-1 counts. Defense-in-depth;
  no behavioural change on a sane kernel.
- **MED-5 — `native_drm_set_master` no longer collapses `-EINVAL` to
  success.** Previously the wrapper treated `-EINVAL` as
  "already master" — but `-EINVAL` is also returned for genuine
  argument errors, and disambiguation requires a separate
  `DRM_IOCTL_AUTH_MAGIC` probe. New behaviour: pass `-EINVAL`
  through unchanged so callers can decide based on subsequent
  ioctl results. Comment block in `src/backend_native_kms.cyr`
  documents the contract.
- **MED-7 + LOW-5 — bump-allocator leaks on per-dispatch PM4
  scratches.** Both were leaking through every compute / render
  dispatch:
  - **MED-7**: `_backend_native_compute_dispatch` leaked 256 B per
    dispatch (`alloc(256)` for the PM4 scratch);
    `_backend_native_render_pass_draw` leaked 1024 B per draw
    (`alloc(1024)` for the same purpose). At 60 fps render = ~61
    KiB/sec, ~15 GiB over a 3-day soak.
  - **LOW-5**: both `native_compute_dispatch_cached` and
    `native_render_dispatch_simple` leaked an additional 8 B per
    dispatch (`alloc(8)` for the syncobj-handle out-pointer).
  Fix: extend `GpuContext` from 112 → 120 bytes with a new
  `+112: pm4_scratch` slot, allocate the 1024-byte scratch once at
  `gpu_context_new_native`, share between compute (uses first 256 B)
  and render (uses full 1024 B) slots. Replace `alloc(8)` syncobj
  scratches with stack-local `var syncobj_buf[8]`. Lifetime contract
  documented in `src/context.cyr`: scratch is reset on each dispatch
  entry, never reclaimed; safe under Cyrius's single-threaded
  execution model. 5 new asserts in Phase D pinning the new
  GpuContext size + scratch ptr stability.
- **LOW-1 — `fd > 0` guard on `native_drm_set_master` /
  `native_drm_drop_master`** matching every other ioctl wrapper in
  `src/backend_native_kms.cyr`. Returns `-1` on invalid fd instead of
  issuing a syscall and depending on the kernel's `-EBADF`. 4
  asserts.
- **LOW-2 — `_kms_summary_print_u32` 16-byte stack buffer too small
  for a max-i64 input.** `fmt_int_buf` is i64-typed (max 19 decimal
  digits + sign + null terminator = 21 bytes); the original 16-byte
  buffer would have overflowed for max-magnitude inputs. KMS IDs are
  u32 in practice, but defense-in-depth — buffer bumped to 24 bytes.
  1 assert pinning the underlying property (`fmt_int_buf` width for
  max signed i64 = 19).
- **LOW-3 — diagnostic `GPU_ERR_NOT_IMPLEMENTED` for the native
  `gpu_buffer_*` / `gpu_shader_module_*` slot stubs.** New error
  code `GPU_ERR_NOT_IMPLEMENTED = 18` in `src/error.cyr`; native
  `_buffer_write` / `_buffer_read` slot stubs return it instead of
  `GPU_ERR_OTHER` so callers can distinguish "this backend doesn't
  support `gpu_buffer_*` yet" from generic failures. v3.0 native
  consumers should keep using `native_buf_pair_*` directly; the
  public `gpu_buffer_*` API native impl is v3.x scope. 8 asserts.
- **LOW-4 — `vc` (vertex_count) honoured in native render draw.**
  `_backend_native_render_pass_draw` previously ignored the `vc`
  parameter and always emitted the fullscreen-triangle 3-vertex count
  in DRAW_INDEX_AUTO. New behaviour: thread `vc` through
  `native_pm4_build_render_draw_tail` (signature changed from
  `(buf, pos)` to `(buf, pos, vertex_count)` — the standalone
  `native_pm4_build_render_clear_triangle` composer continues to pass
  `GFX9_FULLSCREEN_TRI_VCOUNT` so its byte-exact PM4 stream stays
  unchanged). Also reject `ic != 1` with `GPU_ERR_NOT_IMPLEMENTED`
  (instance rendering is v3.x scope; needs additional VGT_NUM_INSTANCES
  wiring). 7 new asserts: vc=6/36/0 produce distinct DRAW_INDEX_AUTO
  dword[1] values + ic=0/2 reject paths.

### Changed — 2026-05-01 (toolchain + scaffold)

- **`src/backend_native.cyr` split into 4 files** (rc.2 punchlist
  toolchain item). The 142 KiB / 3171-line monolith was over the
  cyrius lint/fmt 128 KiB read-buffer cap, producing silent
  truncation warnings and false-positive fmt drift on every CI
  run. Split along section boundaries:
  - `src/backend_native_amdgpu.cyr` (~32 KiB, 783 lines) — DRM/GEM/
    AMDGPU/syncobj/CS-submit ioctl wrappers (foundational layer).
  - `src/backend_native_shaders.cyr` (~28 KiB, 575 lines) — GFX9
    ISA shader libraries + GFX9 graphics-register addresses + value
    minimums.
  - `src/backend_native_pm4.cyr` (~34 KiB, 731 lines) — PM4 packet
    primitives + compute + render PM4 stream composers (pure byte
    builders).
  - `src/backend_native.cyr` (~52 KiB, 1158 lines) — Backend slot
    fillers + dispatch drivers + texture/RT/pipeline + ctx accessors
    + `backend_native_new()`. The integration layer.
  Include order in `src/lib.cyr` (load-bearing for forward
  references): `amdgpu → shaders → pm4 → backend_native →
  backend_native_kms`.
- **CI fmt-check now defends against future toolchain truncation.**
  `.github/workflows/ci.yml` adds a line-count guard before the
  diff: `cyrius fmt --check` output line count must equal the file's
  line count, or fail with `FAIL fmt truncation: $f — file the
  toolchain bug` instead of the old false-positive "needs fmt." The
  previous `>128 KiB skip` block is gone (no file remains over the
  cap); files growing past the cap now hard-fail with `FAIL: $f
  exceeds cyrius fmt 128 KiB cap — split required` to prevent silent
  regressions. (rc.2 punchlist toolchain item.)
- **`samvada` bumped 0.2.0 → 0.2.2** (`[deps.samvada]` in
  `cyrius.cyml`). Brings in upstream documentation + CI-quality-bar
  alignment + `cyrius fmt` drift cleanup; no API changes.
- **`GpuContext` size bumped 112 → 120** to accommodate the new
  `+112: pm4_scratch` slot (MED-7 closeout). `GPU_CONTEXT_SIZE`
  constant updated accordingly; the existing
  `test_gpu_context_size_extended_to_112` test renamed to `_to_120`.
  `+0..+24` dual interpretation, `+32` backend, `+40..+88` native
  cache, `+96/+104` surface-stash all unchanged.
- **`CLAUDE.md` architecture diagram updated** for the 38-module
  layout + the new GpuContext field offset.

### Added — 2026-05-01 (radv_capture Phase 2)

- **`programs/native_pm4_dump.cyr` + extractor + `make compare`.**
  rc.1 shipped Phase 1 of the radv_capture diagnostic harness
  (proves a libvulkan dispatch runs + RADV emits a `--dump=ibs`
  trace). Phase 2 adds the byte-diff reduction tooling the audit
  punchlist gated on:
  - `programs/native_pm4_dump.cyr` — runs
    `native_pm4_build_compute_store_deadbeef` against fixed canonical
    VAs and writes the dword stream to stdout. CI-safe — no GPU
    access, no DRM fd, no BO allocation. Pair Makefile target:
    `make dump-native-pm4`.
  - `programs/diagnostics/radv_capture/extract_dispatch.sh` — awk
    script that normalizes a PM4 dump (mabda format OR RADV
    `--dump=ibs` format) into one-packet-per-line decoded output,
    filtering down to the compute dispatch tail (SET_SH_REGs to
    `COMPUTE_*` registers, ACQUIRE_MEM, DISPATCH_DIRECT). Format-
    agnostic — same script handles both inputs.
  - `make compare` in
    `programs/diagnostics/radv_capture/Makefile` — runs both
    extractors over the RADV + mabda dumps and shows the focused
    diff.
  - README rewrite documenting the workflow + the
    "known-equivalents" table for diff lines that are not byte-clean
    but represent semantically-equivalent shapes (e.g., RADV's
    `EVENT_WRITE CACHE_FLUSH_AND_INV` vs mabda's
    `WRITE_DATA(WR_CONFIRM=1)` post-dispatch CP marker).
  This unblocks the radv-IB byte-exact verification gate that's
  been "Layer-2 work pending Hyprland" since v3 Step 6.5 — the
  capture itself still needs a HW box, but the comparison tooling
  now exists.

### Next

- **Toolchain pin bump** (in-flight 2026-05-12): `cyrius 5.7.48 →
  5.11.28` in `cyrius.cyml`, samvada dep already on 0.2.2. Closeout
  documented in the rc.3 punchlist.
- 6-consumer regression sweep against the rc.2 bundle (soorat / rasa /
  ranga / bijli / aethersafta / kiran-via-soorat). Parallelizable to
  a sub-agent. Any file regression filed at
  `docs/issues/2026-MM-DD-<project>-rc2-regression.md`.
- **Soak strategy split into rc.3 + rc.4** (2026-05-12 plan):
  - **rc.3** — ≤6h soak proof on the dev box. The dirty-fast gate:
    if anything regresses inside 6h, we cut rc.3 and iterate before
    sinking the 24h+ runs. Punchlist:
    [`docs/development/3-0-rc-3-punchlist.md`](docs/development/3-0-rc-3-punchlist.md).
  - **rc.4** — extended soak from 24h → up to 3 days. **1-day
    clean = earliest cut for 3.0.0 GA.** The 3-day window is the
    focus of the first few 3.0.x patches (anything that surfaces
    between 24h and 72h becomes a 3.0.1+ tracked item, not a GA
    blocker). Punchlist:
    [`docs/development/3-0-rc-4-punchlist.md`](docs/development/3-0-rc-4-punchlist.md).
- VERSION 3.0.0-rc.2 → 3.0.0-rc.3 → 3.0.0-rc.4 → 3.0.0 + tag once
  the rc.4 24h-clean gate passes.

## [3.0.0-rc.1] — 2026-04-30

**Release-candidate cut of the v3 native-backend work.** Dual backend
(wgpu + native AMD) ships against the same public API surface; the
`Backend` 25-slot fnptr table routes `gpu_buffer_*` /
`gpu_compute_dispatch` / `gpu_texture_*` / `gpu_render_*` /
`gpu_surface_*` to the appropriate impl. Native path is direct
AMDGPU DRM ioctls (no libdrm), with `samvada` as the sister AGNOS
package providing logind master delegation via libsystemd C-shim.

The 5 ship-blockers from the
[2026-04-30 audit](docs/audit/2026-04-30-audit.md) are fixed in
tree (HIGH-1, HIGH-2, MED-1, MED-3, MED-6 + LOW-6 bundled). Audit
disposition: 10 deferred items file as the rc.2 punchlist
(`docs/development/3-0-rc-2-punchlist.md`) — official `3.0.0` ships
once those land clean.

**Metrics**: 35 src/ modules / ~14,500 LoC / 1828 CPU asserts across
3 test files (was 1819 pre-audit; +9 from MED-3 odd-dim rejection
asserts) / `dist/mabda.cyr` ~11,500 lines / 7 GPU integration
programs (`phase0`, `compute_e2e`, `render_e2e`, `render_graph_e2e`
on wgpu; `native_compute_store`, `native_texture_e2e`,
`native_render_e2e` on native; plus `native_kms_summary`,
`native_kms_modeset_smoke`, `native_present_e2e` for Phase D).

The remaining v3.0.0-dev entries below are dev-track items that
shipped over the v3 cycle; they remain dated and per-step for
historical traceability.

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

### Added — 2026-04-30 (Step 7.2(b) — SETCRTC primitive)

The ioctl that actually flips a display: binds `(CRTC,
framebuffer, mode, connector list)` in one atomic kernel call.
Disabling a CRTC is the same ioctl with fb_id=0 + mode_valid=0
+ count_connectors=0 — the kernel reads that combination as
"stop scanning out from this CRTC."

- **`DRM_IOCTL_MODE_SETCRTC`** (= `0xC06864A2`) ioctl number,
  re-derived from first principles in CPU tests.
- **`drm_mode_crtc`** struct shape (104 bytes): connector ptr +
  count, CRTC ID, FB ID, scan-out (x, y), gamma_size,
  mode_valid flag, embedded `drm_mode_modeinfo` at +36. The
  embedded modeinfo end-aligns to the struct boundary exactly
  (36 + 68 = 104) — pinned in tests so any struct drift between
  modeinfo and SETCRTC layouts surfaces immediately.
- **Low-level** `native_drm_mode_set_crtc(fd, req)` wrapper —
  caller fills the 104-byte request directly.
- **High-level** `native_kms_set_crtc(fd, crtc_id, fb_id,
  conn_ids_ptr, conn_count, mode_ptr)` builds the request
  inline. Memcpy's the 68-byte modeinfo from the caller's
  pointer — typical input is the result of
  `native_kms_get_connector_modes` + `native_kms_pick_preferred_mode`
  followed by an offset into the modes array.
- **Convenience** `native_kms_disable_crtc(fd, crtc_id)` issues
  the same ioctl with all "active" fields zeroed. Useful for
  clean teardown after present, and as a CRTC-accessibility
  probe.

Documented inline:

- **Permission**: SETCRTC requires DRM master. On a running
  desktop the compositor (Hyprland / GNOME / KDE) holds master,
  so this ioctl returns EACCES from a non-master client. Need
  a vt-switch to a tty (which drops the previous master) or
  explicit `DRM_IOCTL_SET_MASTER` after the compositor releases.
- **Cross-fd**: `fb_id` only resolves on the fd where it was
  created. BO + AddFB2 + SETCRTC must all be reachable through
  one master fd. The PRIME bridge (next bite) is what lets a
  render-fd BO become a master-fd FB.

No HW exercise yet — saved for the end-to-end modeset bite
(7.2(c)) where we have BO → FB → SETCRTC composed end-to-end.

4 new CPU tests, 21 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
ioctl number derivation, drm_mode_crtc field offsets including
the embedded-modeinfo end-aligns-to-size sanity check,
null-safety on both helpers covering all 6 invalid-arg cases for
`set_crtc` plus 2 for `disable_crtc`.

### Added — 2026-04-30 (Step 7.2(c) — PRIME cross-fd BO bridge + AddFB2 HW-verified)

The DRM PRIME ioctl pair that bridges a GEM BO from the render
node's handle namespace into the master fd's, so a BO allocated
via the render fd can be wrapped by AddFB2 on the master fd.
**The end-to-end chain validated live on Cezanne in this bite.**

- **`DRM_IOCTL_PRIME_HANDLE_TO_FD`** (= `0xC00C642D`) — converts
  a render-fd handle to a kernel-level dmabuf fd.
- **`DRM_IOCTL_PRIME_FD_TO_HANDLE`** (= `0xC00C642E`) — converts a
  dmabuf fd to a master-fd handle (different from the render-fd
  handle; the two refcount the same underlying memory but each
  is closed independently).
- **`drm_prime_handle`** struct shape (12 B; handle / flags / fd).
  Same struct used for both directions — only which fields are
  IN vs OUT swap.
- **`DRM_CLOEXEC` (0x80000) + `DRM_RDWR` (0x2)** flag constants
  for HANDLE_TO_FD. CLOEXEC is mandatory in practice (otherwise
  fork+exec leaks the handle); RDWR is needed for KMS scanout
  BOs which the kernel writes to.
- **Low-level wrappers**: `native_drm_prime_handle_to_fd`,
  `native_drm_prime_fd_to_handle`.
- **High-level**: `native_kms_import_bo(card_fd, render_fd,
  render_handle)` — runs both ioctls + closes the transient
  dmabuf fd, returns the new card-fd handle (0 on failure).
  Caller is responsible for closing both handles when done
  (each fd has its own).

**Live HW verification on Cezanne (2026-04-30):**
`programs/native_kms_summary.cyr` was extended with an FB smoke
section that allocates a 256×256 GTT BO on `/dev/dri/renderD128`,
imports it onto the master `card1` fd via the PRIME bridge,
calls `native_kms_add_fb_xrgb8888`, prints the FB ID, then
cleans up. Real output:

```
fb smoke (render -> PRIME -> master AddFB2):
  render bo=1 card_handle=1 fb_id=145 PASS
```

This single live run validates the entire 7.3 + 7.2(c)
structural chain — AddFB2 was previously only CPU-tested.

4 new CPU tests, 13 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
ioctl numbers re-derived, drm_prime_handle field offsets, flag
constants, null-safety on the high-level bridge across all 3
invalid-arg cases.

### Added — 2026-04-30 (Step 7.2(d) — end-to-end modeset driver)

Composes everything from 7.1 + 7.2(a/b/c) + 7.3 into one call.
Plus a runnable smoke program that fills the resulting framebuffer
with solid red and sleeps 3 seconds — when run from a tty, the
screen visibly flips. Verified live on Cezanne up through the
SETCRTC permission gate.

- **`NativeKmsScanout`** struct (40 B): conn_id / crtc_id / fb_id
  / card_handle / render_handle / width / mapped_addr (u64) /
  height / bo_size. Every resource the caller needs to track for
  clean teardown lives in this one struct.
- **`native_kms_modeset_first_connected(card_fd, render_fd, state,
  out)`** — the load-bearing driver. Walks state's connector IDs,
  finds the first `DRM_MODE_CONNECTED` one, fetches its encoder,
  picks a CRTC from `possible_crtcs` (lowest set bit → index into
  `state.crtc_ids`), fetches the connector's modes array, picks
  the `DRM_MODE_TYPE_PREFERRED` one, allocates a render-fd BO
  sized to the mode at 256-byte pitch alignment, PRIME-imports to
  the card-fd handle namespace via `native_kms_import_bo`, AddFB2
  via `native_kms_add_fb_xrgb8888`, then SETCRTC via
  `native_kms_set_crtc`. Returns 0 on success; on failure returns
  one of 11 named negative rcs that map to specific failure
  steps — diagnostic value for unattended runs and for filing
  bug reports against specific kernels.
- **`native_kms_release_scanout(card_fd, render_fd, scanout)`** —
  teardown. Issues `disable_crtc` + `rm_fb` + 2 × `gem_close` +
  `bo_release_gtt` + zeroes the scanout struct. Idempotent on
  zero — every conditional skips its release call when the
  field is 0. Safe to call after a partial modeset failure.
- **`native_kms_lowest_set_bit(mask)`** — pure-Cyrius helper that
  returns 0..31 for the lowest set bit, -1 if no bit is set.
  Used by the modeset driver to pick a CRTC from
  `possible_crtcs`; exposed because it's tiny and callers
  building their own modeset paths will want it.
- **`programs/native_kms_modeset_smoke.cyr`** — runnable smoke.
  Opens `cardN` master fd + `renderD128`, runs discovery, calls
  the modeset driver, fills the BO with solid red (XRGB8888 LE
  pixel = `0x00FF0000`), sleeps 3 seconds via
  `clock_nanosleep(2)` (CLOCK_MONOTONIC, relative), tears down.
  Documented exit codes 0–4 + sub-rc decoding for code 4 (all 11
  modeset failure step codes).

**Verified live on Cezanne (2026-04-30, from a desktop session):**

```
mabda native modeset smoke (v3 Step 7.2(d))
opened card_fd=3
opened render_fd=4
discovered 4 connectors, 4 crtcs
FAIL: modeset rc=-11 (EACCES likely — not master; run from a tty)
```

This is the **expected** result from inside a Hyprland session —
the driver walks the entire pipeline (discovery → mode-pick →
encoder fetch → CRTC pick → BO alloc → PRIME bridge → AddFB2)
and only fails at the SETCRTC permission boundary. Running from
a tty (Ctrl-Alt-F2 drops Hyprland's master and systemd-logind
hands master to whoever's on the new vt) is the documented path
to the actual visible flip.

4 new CPU tests, 22 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
`lowest_set_bit` edge cases (bit 0, bit 1, mixed, high bit, zero
sentinel), NativeKmsScanout field offsets (10 fields), modeset
driver null-safety across all 4 invalid-arg cases, release
idempotent on zero scanout.

### Added — 2026-04-30 (Step 7.2(d.1) + 7.4 — master ioctls + page-flip)

Two complementary additions: explicit DRM master acquisition (so
the modeset smoke gives clean diagnostics on which step is
gated by perms) and the page-flip primitive that v3.0 surface
present is built on.

**Step 7.2(d.1) — DRM master acquisition:**

- **`DRM_IOCTL_SET_MASTER`** (`0x641E`) and **`DROP_MASTER`**
  (`0x641F`) — no-payload ioctls (dir=0, size=0).
- **`native_drm_set_master(fd)`** treats `-EINVAL` (already
  master) as success, otherwise returns the kernel errno.
- **`native_drm_drop_master(fd)`** for clean release after the
  smoke completes.
- The modeset smoke now calls SET_MASTER explicitly and prints a
  distinct diagnostic (rc + errno + actionable hint) instead of
  guessing at SETCRTC's `-EACCES`. On the dev box this surfaces
  `SET_MASTER: rc=-13 (errno=13)` immediately, with hints
  pointing at the logind / no-compositor / vkms workarounds.

**Step 7.4 — page-flip + event read:**

- **`DRM_IOCTL_MODE_PAGE_FLIP`** (`0xC01864B0`) ioctl number,
  `drm_mode_crtc_page_flip` struct (24 B; crtc_id / fb_id /
  flags / reserved / user_data).
- **`DRM_MODE_PAGE_FLIP_EVENT`** (0x01) + **`_ASYNC`** (0x02)
  flag constants. EVENT mode queues a vblank event the caller
  reads back; ASYNC mode flips immediately (tearing).
- **Low-level** `native_drm_mode_page_flip(fd, req)` +
  **high-level** `native_kms_page_flip(fd, crtc_id, fb_id,
  flags, user_data)` — null-safe on every ptr arg.
- **Event-read path**: `drm_event` header (8 B; type + length),
  `drm_event_vblank` payload (32 B total; user_data, tv_sec,
  tv_usec, sequence, crtc_id). `DRM_EVENT_VBLANK` (0x01) +
  `_FLIP_COMPLETE` (0x02) type constants. **`native_drm_read_event(fd,
  buf, n)`** wraps `read(2)` (SYS_READ=0). Five inline accessors
  (`drm_event_type`, `_length`, `drm_event_vblank_user_data`,
  `_sequence`, `_crtc_id`) keep call sites readable.

Together these unlock vsync-paced double-buffered present:
flip with EVENT flag set, fd becomes readable, draining one
`drm_event_vblank` per flip lets the present loop know when
the buffer-swap actually hit screen.

### Documented — 2026-04-30 (Phase D logind master blocker)

`programs/native_kms_modeset_smoke.cyr` was run with `sudo`
from the dev session and from a tty. Both returned
`SET_MASTER: rc=-13 (EACCES)` followed by `modeset rc=-11`.
Root cause: modern systemd-logind retains DRM master in the
running compositor's session even after vt-switch + sudo.
Confirmed pre-existing-but-undocumented constraint; saved as
project memory `project_phase_d_master_logind_blocker.md` with
three workarounds for HW testing (vkms, no-compositor session,
stop display-manager). Treated as **deferred to v3.x logind
integration design** (Step 7.7); doesn't block Phase D
primitive development. The mabda smoke walks the entire
pipeline correctly through `AddFB2` — only `SETCRTC` is
gated.

7 new CPU tests, 32 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
master ioctl numbers, page-flip ioctl + struct + flags +
null-safety, event-struct shapes + round-trip accessors, read
null-safety.

### Added — 2026-04-30 (Step 7.4(b) — present primitive + reusable FB alloc)

`NativeKmsFb` struct (32 B) bundles a framebuffer with its
backing BO + both fd handles. `native_kms_alloc_fb(card_fd,
render_fd, w, h, out)` extracts the BO + PRIME + AddFB2
sequence from `native_kms_modeset_first_connected` into a
reusable helper — same step-code error convention as the
modeset driver. `native_kms_release_fb` is the matching
teardown.

The load-bearing primitive: **`native_kms_present(card_fd,
scanout, new_fb_id, sequence_out)`** issues `page_flip` with
the EVENT flag, blocks on `read_event` for the matching
`drm_event_vblank`, validates the type is `FLIP_COMPLETE`,
updates `scanout.fb_id` to the new value, and writes the
kernel's vblank sequence to `*sequence_out` (if non-null).
Step codes -1 invalid args, -2 short read, -3 wrong event
type, plus propagated kernel errnos.

Designed for the simplest single-flip + blocking-read shape.
A real present loop would either (a) use a non-blocking read
with poll/epoll, or (b) batch multiple events; v3.0 keeps
this simple and consumers needing more sophisticated handling
can call `native_kms_page_flip` + `native_drm_read_event`
directly.

The double-buffered render pattern becomes:

```
alloc_fb(w, h, &fb_b)
fill fb_b.mapped_addr with frame N
present(scanout, fb_b.fb_id, &seq)
fill scanout.fb_id (now back-buffer) with frame N+1
present(scanout, scanout_old_fb_a.fb_id, &seq)
... repeat
```

No HW exercise yet — same logind master gate as 7.2(d) / 7.4.

4 new CPU tests, 19 asserts in
`tests/tcyr/mabda_v3_phase_d.tcyr`: FB struct field offsets,
alloc + release null-safety, present null-safety covering
fd / scanout / new_fb / no-crtc.

### Added — 2026-04-30 (Step 7.5 — Backend interface surface slot layout)

Code-only extension to `src/backend.cyr`, mirroring the 5.4 /
6.8(a) pattern: declare slot offsets + bump struct size +
expose range markers, defer the implementation wrappers to the
following step.

- **4 new slot offsets**: `BACKEND_SLOT_SURFACE_CONFIGURE`
  (176), `_ACQUIRE` (184), `_PRESENT` (192), `_RELEASE` (200).
- **`BACKEND_SIZE`** bumped 176 → 208.
- **`BACKEND_SURFACE_SLOTS_BEGIN/END`** = 176 / 208 range markers.
- **Slot signatures documented inline** in the source:
  - `surface_configure(ctx, w, h) → surface_ptr` — sets up the
    surface (wgpu: `wgpu_surface_configure`; native: opens
    card+render fds + runs full modeset + allocs 2 FBs).
  - `surface_acquire(ctx, surface) → fb_ptr` — returns the
    back-buffer the consumer renders into (wgpu: texture view;
    native: the `NativeKmsFb` not currently scanning).
  - `surface_present(ctx, surface) → 0|err` — submits the
    back-buffer for display + swaps front/back.
  - `surface_release(ctx, surface) → 0` — backend-owned
    cleanup. Consumer-owned window/fd lifecycle stays the
    consumer's call (master-fd release in particular needs
    7.7's design to settle).
- **`backend_is_complete` defers** the v3 surface range walk
  until 7.6 lands the real wrappers — same pattern as 6.8(a).
  Adding the range walk now would falsely report all backends
  incomplete.

10 new layout asserts in `tests/tcyr/mabda_v3.tcyr` cover every
new constant (4 slot offsets + 2 range markers + total size +
3 sanity arithmetic checks).

The proposal-doc revision (`docs/proposals/v3-backend-interface.md`)
is *intentionally* deferred — the load-bearing architectural
question (TTY-only model vs logind-aware compositor delegation)
is part of the 7.7 public-API design and writing the proposal
now would lock in choices that 7.7 will need to revisit.

### Added — 2026-04-30 (Step 7.6 — surface slot stubs, both backends)

Both `backend_wgpu_new` and `backend_native_new` now install all
25 slots — the v3 surface range (4 slots × 8 = 32 bytes) is
filled with stub fns that return 0 or `GPU_ERR_OTHER`.
`backend_is_complete` extended to walk the v3 range; both
backends pass.

- **`_backend_wgpu_surface_{configure,acquire,present,release}`**
  — stubs in `src/backend_wgpu.cyr`. Documented inline that
  wgpu consumers needing real surface support should use the
  v2 `src/surface.cyr` `surface_state_*` API directly until 7.7
  lands the consumer-side window-handle protocol.
- **`_backend_native_surface_{configure,acquire,present,release}`**
  — stubs in `src/backend_native.cyr`. Documented inline that
  the master-fd-vs-render-fd story (and the logind delegation
  question) settles in 7.7.

The stubs are intentional — both backends have an unresolved
consumer-side protocol question that 7.7 resolves alongside
the public API design. Wiring stubs now keeps the abstraction
layer "structurally complete" (every slot non-null) without
locking in either consumer protocol.

10 new layout asserts in `tests/tcyr/mabda_v3.tcyr`:
- `test_backend_is_complete_detects_missing_slot` extended to
  walk v3 (the "v0+v1+v2 alone is no longer complete" pattern,
  matching the 6.8(b) → 6.8(c) staging).
- `test_backend_wgpu_new_is_complete` + `_native_new_is_complete`
  each check all 4 v3 slots are filled.

### Added — 2026-04-30 (Step 7.7 — Phase D public API + e2e program)

Closes Phase D code-completion. The full chain from window
handle (or DRM master fd) to scanned-out frame goes through one
public API surface, with both backends fully wired.

**GpuContext extension (96 → 112 bytes)** — added two
consumer-stash fields per the v3-surface-api-design proposal:
`wgpu_surface_handle` at +96 (set by
`gpu_surface_configure_wgpu`) and `native_card_fd` at +104
(set by `gpu_surface_configure_native_kiosk`). Accessor pairs
`gpu_ctx_*_handle` / `gpu_ctx_set_*_handle` /
`gpu_ctx_native_card_fd` / `gpu_ctx_set_native_card_fd` exposed
in `src/context.cyr`. Single greppable migration: every
`alloc(96)` site bumped to `alloc(112)` (3 src + 38 test
sites).

**`src/surface_v3.cyr`** — new module. Six public dispatchers:

```
gpu_surface_configure_wgpu(ctx, wgpu_surface, w, h)        → surface_ptr
gpu_surface_configure_native_kiosk(ctx, card_fd, w, h)     → surface_ptr
gpu_surface_configure_native_logind(ctx, w, h)             → 0  (v3.0 stub)

gpu_surface_acquire(ctx, surface)  → fb_ptr
gpu_surface_present(ctx, surface)  → 0|err
gpu_surface_release(ctx, surface)  → 0
```

The three configure entries stash backend-specific resources
(`WGPUSurface` handle / DRM master `card_fd`) on `GpuContext`
via the new accessors, then dispatch through the slot table —
slot signature `(ctx, w, h)` stays stable from 7.5. The three
per-frame ops route directly through the slot table.

**Wgpu wrappers (real impls, replacing 7.6 stubs).** Thin
delegates over the existing v2 `src/surface.cyr`
`surface_state_*` lifecycle. Configure reads `wgpu_surface`
from ctx, calls `surface_state_new` with default RGBA8_UNORM
+ vsync. Acquire unwraps the `Result` returned by
`surface_state_acquire`. Present + release passthrough.

**Native wrappers (real impls).** Introduce a 120-byte
`NativeSurface` struct (`card_fd` + `render_fd` + `state` ptr
+ inline 40-byte scanout + inline 32-byte fb_b + `front_is_b`
flag + dimensions + pitch). Configure runs the full pipeline:
`native_kms_init` → `native_kms_modeset_first_connected` →
`native_kms_alloc_fb` for the second buffer. Acquire returns
the back-buffer pointer (toggle on `front_is_b`). Present
calls `native_kms_present` then toggles. Release tears down
all three (fb_b + scanout + state).

**`programs/native_present_e2e.cyr`** (~190 lines). Runs the
public API end-to-end: opens card_fd, takes DRM master,
configures the surface, then a 120-frame loop fills the
back-buffer with a vertically-scrolling blue-red gradient
(0x00RRGGBB pixels into the BO's `mapped_addr`, pitch-aware).
Verified live on the dev box up through `SET_MASTER` — returns
EACCES because Hyprland holds master, same gate as
`native_kms_modeset_smoke`. Pipeline is structurally correct;
visible flip needs a tty + stopped compositor, OR the
v3.x `samvada` package landing.

**Bump-allocator note.** Stack-local `var ctx[112]` for the
new tests (heap-allocated tests at this point in the file
exhaust the bump allocator). Pattern carries forward — any
new tests in `mabda_v3_phase_d.tcyr` that need a ctx should
use stack-local arrays.

8 new CPU tests, 38 asserts in `tests/tcyr/mabda_v3_phase_d.tcyr`:
extended ctx size pin, both consumer-stash accessor
round-trips, `NativeSurface` field offsets + inline-struct
alignment, null-safety on all 6 public dispatchers, v3.0
logind-stub pin (catches an accidental non-zero return).

### Metrics — 2026-04-30 (post Step 7.7)

- Module count: **35** (was 34; `surface_v3.cyr` is new).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: 856 assertions (unchanged).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: **335 assertions** (was
  297 at 7.6 close; +38 from 7.7).
- `src/context.cyr`: ~210 lines (was ~190; +20 for the
  `GPU_CONTEXT_SIZE` constant + 4 new accessors + extended
  layout docstring).
- `src/surface_v3.cyr`: 145 lines (new).
- `src/backend_wgpu.cyr`: ~615 lines (was ~590; +25 for real
  surface impls).
- `src/backend_native.cyr`: ~2,870 lines (was ~2,720;
  +150 for `NativeSurface` struct + 4 real surface impls).
- `programs/native_present_e2e.cyr`: ~190 lines (new).
- `dist/mabda.cyr`: regenerated (11417 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Next — 2026-04-30 (post Step 7.7)

**Phase D code-complete.** Mabda v3.0's Tier 1 in-mabda code
is done end-to-end except for WGSL lowering. Two paths forward,
genuinely independent:

1. **8.1 design spike — WGSL frontend choice.** The v3.0 ship-
   blocker per the punchlist's "Hard truths." Doc-only bite:
   compares WGSL parser vs SPIR-V loader vs Tint integration,
   commits to one in a `docs/proposals/v3-wgsl-frontend-choice.md`.
   Same pattern as 7.7's design spike. Implementation (8.2–8.10)
   then follows over multiple sessions.
2. **Tier 2 — `programs/diagnostics/radv_capture/`.** Headless
   Vulkan capture program (C, separate codebase area). Unblocks
   the Layer-2 byte-diff that 6.5 has been waiting on. Self-
   contained, parallel to 8.x.

Plus the long tail of Tier 4 doc updates (CLAUDE.md still
describes v2.5.0) and Tier 5 release engineering (P(-1) audit,
VERSION bump to 3.0.0, etc.). Those run in any order once
Tier 1 closes.

### Metrics — 2026-04-30 (post Step 7.6)

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: **856 assertions** (was 846 at
  7.5 close; +10 from 7.6 stub-wiring tests).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: 297 assertions (unchanged).
- `src/backend.cyr`: ~170 lines (+5 for v3 range walk in
  `is_complete`).
- `src/backend_wgpu.cyr`: ~590 lines (was ~565; +25 for surface
  stubs + builder wiring).
- `src/backend_native.cyr`: ~2,720 lines (was ~2,690; +30 for
  surface stubs + builder wiring).
- `dist/mabda.cyr`: regenerated (11099 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Metrics — 2026-04-30 (post Step 7.5)

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: **846 assertions** (was 836 at
  7.2(b) close; +10 from 7.5 layout asserts).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: 297 assertions (unchanged).
- `src/backend.cyr`: ~165 lines (was ~155; +10 lines for the
  v3 slot offsets + range markers + signature docstring).
- `dist/mabda.cyr`: regenerated (11008 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Metrics — 2026-04-30 (post Step 7.4(b))

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: 836 assertions (unchanged).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: **297 assertions** (was 278
  at 7.4 close; +19 from 7.4(b)).
- `src/backend_native_kms.cyr`: ~1,500 lines (was ~1,330 at
  7.4 close; +170 lines for FB alloc/release + present).
- `dist/mabda.cyr`: regenerated (10964 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Metrics — 2026-04-30 (post Step 7.4)

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: 836 assertions (unchanged).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: **278 assertions** (was 243
  at 7.2(d); +3 from 7.2(d.1) master ioctls + +32 from 7.4).
- `src/backend_native_kms.cyr`: ~1,330 lines (was ~1,180; +150
  lines for master + page-flip + event-read primitives).
- `programs/native_kms_modeset_smoke.cyr`: ~180 lines (was 150;
  +30 lines for SET_MASTER diagnostics).
- `dist/mabda.cyr`: regenerated (10794 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Metrics — 2026-04-30 (post Step 7.2(d))

- Module count: 34 (unchanged).
- `tests/tcyr/mabda.tcyr`: 624 assertions (unchanged).
- `tests/tcyr/mabda_v3.tcyr`: 836 assertions (unchanged).
- `tests/tcyr/mabda_v3_phase_d.tcyr`: **243 assertions** (was 221
  at 7.2(c); +22 from 7.2(d)).
- `src/backend_native_kms.cyr`: ~1,180 lines (was ~970 at 7.2(c);
  +210 lines for the e2e driver + scanout struct + helpers).
- `programs/native_kms_modeset_smoke.cyr`: ~150 lines (new).
- `dist/mabda.cyr`: regenerated (10636 lines).
- Toolchain pin: `cyrius = "5.7.36"` in `cyrius.cyml`.

### Next — 2026-04-30 (post Step 7.2(d))

**Phase D scaffolding code-complete (7.1–7.6).** Only 7.7
remains — and it's the architecture-question step. Two
sub-decisions to make:

1. **wgpu consumer protocol.** How does the consumer pass a
   `WGPUSurface` handle into the slot abstraction? Options:
   side-channel `gpu_context_register_surface_wgpu(ctx, handle)`,
   GpuContext layout extension, or a wider slot signature.
2. **native master-fd protocol.** TTY/kiosk model (caller
   opens + holds master) vs logind-aware delegation via dbus
   TakeDevice. The latter is the proper production path but is
   significant work; v3.0 might commit to TTY/kiosk and
   document logind as a v3.x extension.

Once those are decided, 7.7 ships:
- Public `gpu_surface_*` dispatchers in a new `src/surface_v3.cyr`
  (or extension to existing `src/surface.cyr`).
- Real slot wrappers replacing the stubs.
- `programs/native_present_e2e.cyr` — clear-render-present-hold
  smoke that visibly flips the screen (in a setting where
  master is acquirable).

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

### Security — 2026-04-30 (P(-1) v3 audit + ship-blocker fixes)

P(-1) security audit pass against the v3 delta — the new dual-backend
abstraction (`backend.cyr`, `backend_wgpu.cyr`, `backend_native.cyr`,
`backend_native_kms.cyr`), the public surface API (`surface_v3.cyr`),
and the `GpuContext` 96 → 112-byte growth in `context.cyr`. Audit at
`docs/audit/2026-04-30-audit.md` — 15 findings (0 CRITICAL, 2 HIGH,
7 MEDIUM, 6 LOW). The 5 ship-blockers landed this session; the 10
deferred items file as v3.x backlog with the dispositions documented
in the audit.

**Ship-blocker fixes** (all in tree, all covered by re-verify of
1828 CPU asserts across the three test files):

- **HIGH-1 — `_backend_native_surface_configure` must honour
  consumer width/height.** The native slot was reading the EDID
  preferred mode and silently substituting it for the
  consumer-supplied dims. Fixed in `src/backend_native.cyr:2680-2697`
  to reject loudly (return 0 + tear down state) when consumer dims
  don't match the picked mode. Cross-backend identity preserved.
  Migration guide (`docs/guides/native-migration.md`) documents
  the constraint + the `native_kms_summary` discovery path.
- **HIGH-2 — `gpu_surface_configure_native_logind` master fd leak.**
  Every failure path between `samvada_session_take_device` and the
  slot dispatch leaked the delegated DRM master fd (kernel held
  master until process exit; consumer's "try logind, fall back to
  kiosk" loop accumulated leaks). Fixed in `src/surface_v3.cyr` to
  call `samvada_session_release_device` + `sys_close` + scrub the
  ctx stash on every error path.
- **MED-1 — native `surface_present` return code shape.** The
  native slot was passing through negative kernel errnos where the
  slot ABI declares positive `GPU_ERR_*`. Fixed in
  `src/backend_native.cyr:2738-2746` to route through
  `_native_neg_rc_to_gpu_err` for kernel rcs and map the mabda-
  internal sentinels (`-2`, `-3`) to `GPU_ERR_SURFACE_LOST`.
  Consumer error-handling now matches the wgpu side.
- **MED-3 — `native_rt_create_2d_rgba8` must reject odd dims.**
  `native_pm4_build_render_pass_target`'s viewport math uses
  integer `rt_width / 2`; odd dims silently lost the half-pixel
  and miscalibrated the viewport. Fixed in
  `src/backend_native.cyr:2997-3008` to reject odd width / height
  + non-positive at the allocator boundary, before any ioctl.
  9 new asserts in `tests/tcyr/mabda_v3_phase_d.tcyr` pin the
  rejection contract.
- **MED-6 — `native_kms_present` event-drain loop.** The single-
  read `native_kms_read_event` returned `-3` when the kernel
  queued multiple events per flip (`DRM_EVENT_VBLANK` +
  `DRM_EVENT_FLIP_COMPLETE` is the modal case). Fixed in
  `src/backend_native_kms.cyr:1577-1612` to drain events
  iteratively (walk every event in each read using the header
  `length` field) until `FLIP_COMPLETE` lands; cap at 16 reads
  to prevent pathological event-spew from spinning the present
  loop. The 120-frame `native_present_e2e` may have run by luck
  on the dev box; this fix removes the dependence.
- **LOW-6 (bundled)** — `gpu_surface_release` now zeros the ctx
  stash (`+96` `wgpu_surface_handle`, `+104` `native_card_fd`)
  after the slot's release. Prevents stale-stash reads on a
  subsequent configure that fails before stashing.

**Deferred to v3.x** (audit-tracked, non-blocking): MED-2 (overflow
guards on caller-supplied dims), MED-4 (TOCTOU clamping on
two-pass DRM discovery), MED-5 (`set_master` `-EINVAL` ambiguity),
MED-7 / LOW-5 (bump-allocator leaks in long-running consumers),
LOW-1 / LOW-2 (defense-in-depth nits), LOW-3 (native `gpu_buffer_*`
slot stubs), LOW-4 (`vc` / `ic` ignored in native render).

### Added — 2026-04-30 (verify, audit, then docs wrap-up)

- **`programs/diagnostics/radv_capture/`** — Phase 1 minimum-viable
  Vulkan headless compute that mirrors mabda's
  `native_pm4_build_compute_store_deadbeef` shape. Builds against
  vulkan-headers + `glslangValidator`, dispatches a single-thread
  compute via Mesa RADV, verifies readback returns `0xDEADBEEF`.
  `make dump` runs with `RADV_DEBUG=ibs` to emit the IB byte stream
  for byte-diff against mabda's PM4 composer. Phase 2 (the actual
  diff reduction tooling) is a v3.x backlog bite. Verified end-to-
  end on the dev box (RADV RENOIR / Cezanne).
- **`docs/guides/native-migration.md`** — 1-pager for consumers
  flipping from `BACKEND_KIND_WGPU` to `BACKEND_KIND_AMD`. Covers
  backend selection, byte-polymorphic shader bundles, the v3
  surface API, samvada wiring for the logind path, dimension
  constraints from the audit (HIGH-1 + MED-3), known v3.0
  limitations + their v3.x dispositions, and the consumer-side
  test matrix.

### Changed — 2026-04-30 (toolchain + housekeeping)

- 6 `src/*.cyr` files re-flowed through `cyrius fmt`
  (`backend_native_kms.cyr`, `backend_wgpu.cyr`, `buffer.cyr`,
  `compute.cyr`, `context.cyr`, `texture.cyr`) — continuation-line
  indent normalization, no semantic changes. `dist/mabda.cyr`
  regenerated to pick up the re-flow.
- `src/backend_native.cyr` (137 KiB) NOT re-flowed. `cyrius fmt`
  silently truncates files >128 KiB (same buffer-cap bug as
  cyrlint). Splitting `backend_native.cyr` into smaller modules to
  unblock the fmt gate is a v3.x bite. Documented in the
  `feedback_cyrlint_128k_buffer_cap` memory note.

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
