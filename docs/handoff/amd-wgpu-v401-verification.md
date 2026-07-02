# AMD-hardware verification — v4.0.1 AMD-wgpu deprecation

**Why this file:** CI has no AMD adapter, so the *live* behavior of the v4.0.1
AMD-wgpu deprecation guard can only be proven on an AMD box. This is the
drive-an-agent checklist for that machine. (Repo-tracked so it travels; also
mirrored in machine-local memory `amd-wgpu-hw-verification.md`.)

## What shipped (v4.0.1)
AMD-on-wgpu is **deprecated, not retired**. `gpu_context_from_preinit` reads the
adapter's PCI vendorID (`WGPUAdapterInfo`, offset 80 — `offsetof`-verified vs
vendored v29 `webgpu.h`) and, for AMD (`0x1002`) on the WGPU path:
- **default build:** prints a one-shot deprecation notice to stderr and **still
  creates the context** (escape hatch open);
- **`-D MABDA_AMD_WGPU_STRICT`:** returns `Err(GPU_ERR_AMD_WGPU_DEPRECATED)`.

Guard: `src/context.cyr` `gpu_context_from_preinit` + `_wgpu_warn_amd_deprecated`.
Decode/predicate: `src/wgpu_descriptors.cyr` (`_wgpu_vendor_wgpu_deprecated`).
CPU coverage: `tests/tcyr/backend.tcyr` `test_amd_wgpu_deprecation_v401` / `_e2e`
(mock fn-table drives the real guard; default warn+allow proven, strict is
flag-gated).

## Run on the AMD machine
1. **Default build, AMD adapter (wgpu path):** run a wgpu program
   (`make test-gpu`, `programs/phase0.cyr`, or `render_graph_e2e`). Confirm the
   context is created (Ok) AND the stderr deprecation notice prints exactly once
   ("mabda: DEPRECATION - AMD-on-wgpu is deprecated (v4.0.1) ...").
2. **Strict build:** rebuild the same program with `-D MABDA_AMD_WGPU_STRICT` and
   confirm `gpu_context_from_preinit` now returns `GPU_ERR_AMD_WGPU_DEPRECATED`
   (code 22).
3. **Regression:** if an NVIDIA/Intel adapter is available, confirm it still
   creates a wgpu context with NO notice (guard keys on `0x1002` only). On an
   AMD-only box this is already CPU-covered by the mock e2e.

## Pre-existing AMD HW gates (developer/HW-only, not in CI)
- `make test-native-compute-store`  — amdgpu render node, no master
- `make test-native-render-e2e`     — HW-gated; cache-flush in tree
- `make test-native-kms-modeset`    — HW + DRM master
- `make test-native-present-e2e`    — 120-frame animated present; HW + master
- `make bench-gpu`                  — wgpu benches
- Six-consumer regression sweep (soorat/rasa/ranga/bijli/aethersafta/kiran) —
  none ported yet.

## Known/accepted
One-shot `wgpuAdapterGetInfo` string leak on the wgpu path (LOW, accepted —
`src/wgpu_ffi.cyr` + `docs/audit/2026-07-02-audit.md`); proper free deferred to
the next launcher fn-table revision.
