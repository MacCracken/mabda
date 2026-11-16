# `gpu_compute_dispatch`'s `bp` means different things on native AMD and native NVIDIA, and NVIDIA binds only one buffer

**Status:** 🔴 **OPEN** — API defect.
**Filed:** 2026-11-02, from ranga 2.3.2 (NVIDIA hardware run).
**Placement:** `src/backend_nvidia.cyr` — `_backend_nvidia_compute_dispatch` (line ~416); `src/compute.cyr` —
`gpu_compute_dispatch` (line ~355) and its doc comment.
**Severity:** **Medium.** No portable consumer can write one dispatch call. A call written for AMD is
silently misread on NVIDIA: it reads a count or handle as a VA.
**Affects:** mabda 4.2.0, native NVIDIA. Native AMD is the reference behaviour.

## Summary

`gpu_compute_dispatch(ctx, shmod, x, y, z, bp)` is one public entry point, but the two native backends read
`bp` differently:

| Backend | What `bp` is | What is bound |
| --- | --- | --- |
| native AMD | a binding array: count at +0, then (handle, VA) pairs (`native_buf_pair_write`) | `count` storage buffers, in order |
| native NVIDIA | a single `NV_BUF` struct; reads `load64(bp + NV_BUF_VA)` | one output pointer at `c[0x0]+0x160` |

A consumer that builds the AMD array, as ranga's pipeline does (3 or 4 bindings: source, destination,
params), hands NVIDIA a pointer whose `+NV_BUF_VA` word is part of the array, not a VA. Nothing rejects
it.

ranga's operations need two or three storage buffers per dispatch (source, destination, a params block),
so even with a SPIR-V compiler they can't run on a one-binding dispatch.

## Suggested fix

- Accept the same binding array on NVIDIA as on AMD (count plus (handle, VA) pairs). Write each VA into
  the param bank at the kernel-param ABI offsets, the way the single output pointer is written now. Refuse
  a count the program doesn't declare, as the AMD path documents.
- Until then, document the NVIDIA meaning of `bp` in `gpu_compute_dispatch`'s comment, and have NVIDIA
  reject a `bp` whose first word looks like a small binding count.
- Pair with N9: the SPIR-V→SASS compiler is where the binding count would come from
  ([`2026-11-02-n9-spirv-to-sass-compiler-request.md`](./2026-11-02-n9-spirv-to-sass-compiler-request.md)).
