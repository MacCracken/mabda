# N9 — SPIR-V→SASS compiler for the NVIDIA backend: a consumer is waiting on it

**Status:** 📋 **OPEN — roadmap request.** N9 has been listed under "Next" since the 4.0.0 CHANGELOG
("deferrable to v4.1"), but it is not in `docs/development/roadmap.md`.
**Filed:** 2026-11-02, from ranga 2.3.2.
**Placement:** a new compiler module, the NVIDIA analogue of `src/spirv_lower.cyr` → `src/gfx9_*.cyr`.
**Severity:** none (feature). It blocks every NVIDIA compute consumer.
**Affects:** mabda 4.2.0, native NVIDIA.

## Why now

ranga has run its GPU suites on real NVIDIA hardware: a GTX 1660 SUPER (TU116, SM75) on nouveau and NVK,
which is exactly the backend's target.

- **Working:** context creation and buffer upload/download.
- **Not working:** compute. The backend takes only precompiled SASS, and a portable consumer ships SPIR-V.

ranga can't ship per-vendor machine code. Its kernels are generated at runtime by its own SPIR-V emitter.

## What ranga needs compiled

The 11 kernels in ranga's `src/gpu_kernels.cyr` (emitted by `src/gpu_spirv.cyr`) all compile and are
HW-verified on native GFX9 today:
- invert, grayscale, fade, brightness/contrast and saturation;
- dissolve and wipe;
- crop and the horizontal and vertical flips;
- nearest-neighbour resize.

They use storage-buffer load and store, integer and f32 arithmetic, bounds guards written as structured
selections, `OpFDiv`, `OpSelect` and GLSL.std.450 clamps. The same SPIR-V passes ranga's own structural
checks. Each kernel binds two or three storage buffers, so N9 also needs multi-binding dispatch
([`2026-11-02-compute-dispatch-bindings-differ-amd-nvidia.md`](./archived/2026-11-02-compute-dispatch-bindings-differ-amd-nvidia.md)).

## Done when

`gpu_shader_module_create_spirv` on an NVIDIA context compiles those kernels, and ranga's four GPU suites
pass on the TU116 host with an NVIDIA context.

## Until then

Reject SPIR-V on NVIDIA loudly
([`2026-11-02-nvidia-shader-module-create-ignores-source-kind.md`](./archived/2026-11-02-nvidia-shader-module-create-ignores-source-kind.md)).
ranga will also refuse compute on NVIDIA contexts on its own side.
