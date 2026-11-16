# NVIDIA `shader_module_create` ignores the source kind, so SPIR-V is executed as SASS and the channel is killed — RESOLVED

**Status:** ✅ **RESOLVED in mabda 4.3.0** (2026-11-16). `_backend_nvidia_shader_module_create` accepts only `SHADER_SRC_SASS`; anything else returns 0 with `SHMOD_ERR_SRC_KIND` before any BO is allocated (and the other refusals now set their `SHMOD_ERR_*` too). CPU test `test_nv_shader_module_create_refuses_non_sass` (`tests/tcyr/nvidia.tcyr`, a hand-built ctx); on the TU116 (`chew`), `nvidia_multibind_e2e` case 1: SPIR-V through `gpu_shader_module_create_spirv` on an NVIDIA context returns 0 + `SHMOD_ERR_SRC_KIND`, no dispatch, no channel kill. SPIR-V on NVIDIA remains the N9 compiler's (`2026-11-02-n9-spirv-to-sass-compiler-request.md`, open). Was: open.
**Filed:** 2026-11-02, from ranga 2.3.2 (first run of its GPU suites against `gpu_context_new_nvidia`).
**Placement:** `src/backend_nvidia.cyr` — `_backend_nvidia_shader_module_create` (line ~375).
**Severity:** **High.** A wrong-but-valid call does not fail. It faults the GPU, the channel is killed, and the
only signal is a later `GPU_ERR_DEVICE_LOST` from the dispatch.
**Affects:** mabda 4.2.0, native NVIDIA (nouveau / SM75). HW: GeForce GTX 1660 SUPER (TU116), nouveau
kernel driver, Mesa 26.2.6 NVK, Linux 7.2.14.

## Summary

`gpu_shader_module_create_spirv(ctx, words, len)` passes `SHADER_SRC_SPIRV` through the backend slot. The
two other backends check that tag. The native AMD backend returns 0 with `SHMOD_ERR_SRC_KIND` for a kind
it can't take, and wgpu rejects anything that isn't SPIR-V or WGSL. The NVIDIA backend **never reads
`kind`**: it copies the bytes verbatim into the program BO as SM75 SASS and returns a valid module. The
first dispatch then runs SPIR-V words as machine code.

## What happens

ranga built its SPIR-V kernels and dispatched them on an NVIDIA context:

- `gpu_shader_module_create_spirv` returns a non-zero module.
- `gpu_compute_dispatch` returns **23** (`GPU_ERR_DEVICE_LOST`). The 4.1.4 job-error detection does
  catch it.
- The kernel log, once per process:

```
nouveau 0000:01:00.0: gsp: Xid:13 Graphics SM Warp Exception on (GPC 1, TPC 0, SM 0): Out Of Range Register
nouveau 0000:01:00.0: gsp: Xid:13 Graphics SM Global Exception on (GPC 1, TPC 0, SM 0): Multiple Warp Errors
nouveau 0000:01:00.0: fifo:000000:0009:0009:[gpu_kernels_nv[782]] errored - disabling channel
nouveau 0000:01:00.0: gpu_kernels_nv[782]: channel 9 killed!
```

nouveau recovered each time, and the device still enumerates through Vulkan afterwards.

## Reproduce

Any SPIR-V module through `gpu_shader_module_create_spirv` on a `gpu_context_new_native_nvidia()` context,
then one `gpu_compute_dispatch`. In ranga, build `tests/gpu_spirv.tcyr` with `gpu_context_new_amd(` replaced
by `gpu_context_new_nvidia(` and run it on the NVIDIA host. The "spirv on hardware" group fails at "the
dispatch is accepted and submitted (got 23, expected 0)".

## Suggested fix

Do what `_backend_native_shader_module_create` does: accept only `SHADER_SRC_SASS` until N9 lands. Return
0 for anything else and set `_gpu_set_last_shader_err(0 - SHMOD_ERR_SRC_KIND)`, so
`gpu_shader_error_name` can say why. A CPU-only test (no GPU) can pin it: create with `SHADER_SRC_SPIRV`
on a hand-built NVIDIA ctx struct and expect 0 plus `SHMOD_ERR_SRC_KIND`.

Related: the N9 request
([`2026-11-02-n9-spirv-to-sass-compiler-request.md`](../2026-11-02-n9-spirv-to-sass-compiler-request.md))
is the real fix for consumers. This issue is about failing loudly until then.
