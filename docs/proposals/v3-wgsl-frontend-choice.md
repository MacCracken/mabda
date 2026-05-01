# v3 WGSL → GFX9 frontend choice (deferred to v3.x)

**Status:** Decision recorded — WGSL frontend deferred to v3.x;
v3.0 ships byte-polymorphic `gpu_shader_module_create`.
**Date:** 2026-04-30
**Filed for:** Step 8.1 — replaces the punchlist's earlier
"WGSL → GFX9 ISA lowering is the v3.0 ship-blocker" framing.
**Closes:** the design question that opened on 2026-04-28
("Frontend choice: WGSL parser vs SPIR-V loader").

## Headline decision

**v3.0 does NOT ship in-mabda WGSL lowering.** The
`gpu_shader_module_create(ctx, bytes_ptr, n)` public API is
already byte-polymorphic at the backend boundary:

- **wgpu backend** interprets the bytes as WGSL UTF-8 source and
  hands them to wgpu-native's compiler.
- **native backend** interprets the bytes as pre-compiled GFX9
  ISA (the format `clang -target amdgcn--amdhsa -mcpu=gfx90c
  -O2 -nogpulib` emits).

Consumers ship two forms of their shaders — WGSL strings for the
wgpu path, GFX9 ISA bundles for the native path. The `Backend`
abstraction handles routing; the consumer's build picks which
bytes flow.

**WGSL → GFX9 lowering becomes a v3.x project.** All four GPU
integration programs in mabda (`phase0`, `compute_e2e`,
`render_e2e`, `render_graph_e2e`) already split this way. The
existing native programs (`native_compute_store`,
`native_render_e2e`) feed hand-encoded ISA from helpers like
`native_gfx9_shader_store_deadbeef` and
`native_gfx9_shader_solid_red`. Phase 8 (8.1–8.10) was queued
under the assumption that v3.0's exit criterion required
"running `examples/stdlib-consumer/` unmodified on native" —
which would have demanded WGSL lowering. Re-research on
2026-04-30 surfaced that the existing API surface admits the
two-form-bundle pattern instead.

## Why no external library

Researched on 2026-04-30. Every candidate either violates "own
the stack" or is otherwise dead.

| Path | Calendar | Deps risk | Verdict |
|---|---|---|---|
| **Tint** (Google C++) | 4–6 weeks integ + ongoing | Drags Chromium's `depot_tools`, abseil, RE2, SPIRV-Tools — huge transitive C++ tree | Violates own-the-stack |
| **Naga** (Rust) | 3–5 weeks FFI + Rust toolchain in CI | Adds rustc + cargo as a build dep; **archived as standalone January 2025**, lives inside the wgpu repo only | Rust-toolchain contagion + archived |
| **gogpu/naga** (Go) | 3–4 weeks integ + Go toolchain | Pure Go, MIT license, but adds Go to the build path | Cleanest external option but still a non-AGNOS toolchain |
| **SPIRV-Cross** | — | Doesn't emit ISA — only GLSL/MSL/HLSL | Dead end |
| **SPIRV-Tools** | — | Parser + validator only, no codegen | Dead end |
| **Mesa ACO** | 8–12 weeks to extract | Tightly coupled to NIR + RADV + meson; not packaged standalone; ABI not stable | Best ISA backend in existence; extraction is a research project |

**Recommendation: roll our own** when v3.x picks this up. ~10–16
weeks for compute-only MVP (~2k LoC parser + ~3k LoC NIR-lite IR
+ ~3–5k LoC ISA selection); another 8–12 weeks for vertex /
fragment / texture sampling. Calibration: `backend_native.cyr`
is ~3,000 lines for PM4/queue/BO plumbing alone, so the LoC
estimate is in-range for a side-project effort.

The roll-our-own posture matches every other "own the stack"
call mabda has made — `sigil` for MMIO, `sakshi` for io_uring,
`samvada` for libsystemd, the wgpu C-launcher's eventual v4.0
retirement. Each was a multi-week investment in a primitive
mabda fully owns.

**Bridge option** if a future deadline forces external dep:
invoke `tint` as an out-of-process CLI (`wgsl → spv`) plus
extract Mesa's `aco_compile_shader` via a thin Cyrius-callable
C-shim against `libvulkan_radeon.so` (the in-tree ACO). Less
code than full ACO extraction but introduces a runtime
libvulkan dep — still better than vendoring Chromium.

## What v3.0 ships instead

A small **pre-compiled GFX9 ISA shader library** in tree at
`src/native_gfx9_shaders.cyr` (the existing `native_gfx9_shader_*`
fns, possibly grown). Covers the MVP shaders the AGNOS consumers
need:

- `native_gfx9_shader_solid_red` — fullscreen-triangle clear
  (already in tree, Step 6.2(b)).
- `native_gfx9_shader_fullscreen_triangle_vs` — paired VS for
  the above (already in tree).
- `native_gfx9_shader_store_deadbeef` — minimum-viable compute
  (already in tree, Phase B.4).
- Any additional shaders consumers genuinely need before v3.0
  ship — file as they surface.

**Consumer guidance** (lands in the migration guide, Tier 4):

- For the wgpu path: keep using WGSL strings.
- For the native path: use the in-tree shader library, OR
  hand-encode against the GCN5 ISA spec, OR pre-compile from a
  single source at the consumer's build time using
  `clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -nogpulib`
  + `llvm-objdump -d` for verification.

The consumer-side WGSL → GFX9 build step is **optional**. v3.0
doesn't require it. Consumers that want a single shader source
for both backends can adopt the clang+llvm-objdump dual-build
locally; consumers that just want compute that works can use
the in-tree library.

## When v3.x picks this up

The decision criterion for v3.x scoping: a second AGNOS consumer
hits the WGSL-source-only wall and needs the dual-bundle pattern
to feel native. Until then, the 5–6 in-tree pre-compiled shaders
plus consumer-side hand-encoding cover every concrete need.

The v3.x WGSL frontend project at that point:

1. **Compute subset first.** No textures, no derivatives, no
   wave-uniformity analysis. Just buffer reads + buffer writes
   + arithmetic + control flow. Calibration: ~10–16 weeks.
2. **Add vertex/fragment** when soorat or aethersafta hits the
   wall on graphics. Calibration: 8–12 weeks.
3. **Add texture sampling** when a consumer hits a sampling-
   shader wall. Calibration: 4–6 weeks.

Each phase ships independently with its own version bump and
exit criteria.

## Architectural anchors

- **`gpu_shader_module_create` byte-polymorphism is the load-
  bearing API choice** that lets v3.0 ship without WGSL
  lowering. Any future API change MUST preserve this property —
  a single-form shader API would re-introduce the v3.0 ship
  blocker.
- **The native shader library is the v3.0 dependency** every
  consumer that wants the native path inherits. Treat it as
  versioned alongside mabda; document any addition.
- **WGSL frontend, when it lands, ships as a separate AGNOS
  package** — naming TBD (Sanskrit/Arabic theme), same shape
  as `samvada` / `sakshi`. NOT in mabda's tree. Mabda becomes a
  consumer of it via `[deps.X]` for the `wgsl_to_gfx9_compile`
  call, similar to how mabda v3.0 will consume samvada for
  logind once a desktop-session consumer wires both C shims.

## Summary

- **v3.0 ships:** byte-polymorphic `gpu_shader_module_create` +
  the existing 3 in-tree native shaders + however many more
  the consumer migration surfaces; consumer-side hand-encoding
  documented as the supported workflow.
- **v3.x ships:** a separate AGNOS package for WGSL → GFX9
  compute lowering. Roll-our-own pure-Cyrius implementation.
  ~10–16 weeks calendar.
- **v3.y ships:** vertex/fragment/texture lowering as separate
  phases, each scoped against a real consumer's wall.

The earlier "WGSL is the v3.0 ship-blocker" framing was wrong;
this proposal corrects the architectural anchor and unblocks
v3.0 ship.

---

**Reviewer prompts:**

- Should the in-tree shader library be a separate file
  (`src/native_gfx9_shaders.cyr`) or stay co-located in
  `src/backend_native.cyr` where the existing helpers live?
- Should mabda v3.0 ship a `programs/native_shader_compile.cyr`
  helper that wraps `clang + llvm-objdump` for consumers, or
  is that a consumer-build-system concern?
- Naming for the v3.x WGSL→GFX9 package — `karkati` (Sanskrit
  "translator"), `tarjuma` (Arabic "translation"), or similar?
  Decision can wait until v3.x kickoff.
