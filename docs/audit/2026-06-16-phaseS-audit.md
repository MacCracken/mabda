# Security/Correctness Audit — v3.2.4 (SPIR-V shader ingestion, Phase S), 2026-06-16

**Scope:** the v3.2.4 Phase S diff vs the `3.2.3` tag — SPIR-V ingestion on the
wgpu path: `wgpu_types.cyr` / `wgpu_descriptors.cyr` (the `WGPUShaderSourceSPIRV`
descriptor + sType + `_spirv_validate`), `backend.cyr` (`ShaderSourceKind`),
`compute.cyr` (`gpu_shader_module_create_spirv` + the kind-forwarding
`gpu_shader_module_create`), `backend_wgpu.cyr` / `backend_native.cyr` (the
widened `(…,kind)` shader-create slot), `shader_cache.cyr` (the kind-folded
length-explicit hash + SPIR-V cache peers), `deps/wgpu_main.c` (the
`ShaderSourceSPIRV` instance feature), `programs/spirv_e2e.cyr`.

**Method:** the SPIRV descriptor offsets + sType were pinned vs the in-tree
`deps/wgpu-native/include/webgpu/webgpu.h` (WGPUShaderSourceSPIRV, codeSize in
words); `_spirv_validate` is CPU-tested across all 6 reject codes; the embedded
e2e SPIR-V binary is `spirv-val`-clean and HW-verified to render byte-identical
to its WGSL twin (`spirv_e2e` cross-source identity). One P(-1)-style adversarial
review workflow (`phaseS-review`, 2 dims: FFI/security + integration) then
re-verified the diff, each finding independently checked against source.

**Result:** **0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 confirmed findings.** One LOW
defense-in-depth note (dismissed) — see below. The FFI-boundary bytes (the SPIRV
descriptor) are pinned + HW-proven; consumer-supplied SPIR-V is structurally
validated (magic/align/bound/cap, incl. byte-swapped detection) before crossing
into wgpu-native's parser.

## Findings

None confirmed.

### Dismissed (LOW, defense-in-depth)
- *"`shader_cache_get_or_compile_spirv` hashes `byte_len` bytes before
  `_spirv_validate` runs."* Factually accurate but not a real bug: the SPIR-V
  cache peers are dead-in-tree (only the round-trip test calls them; the live
  `gpu_shader_module_create_spirv` path validates in the backend slot before any
  byte access and never touches the cache), the "over-read" is the inherent
  FFI caller-supplies-length contract (identical to the WGSL `strlen` path), and
  `_spirv_validate` cannot detect a too-large `byte_len` anyway, so a reorder
  wouldn't close the alleged gap. **Landed the reorder regardless** (validate
  before hashing/caching — a cleaner contract for consumers who use the cache).

## HW/verification
- `spirv_e2e` HW-verified on the wgpu-native box: a SPIR-V module
  (`gpu_shader_module_create_spirv`, on the feature-enabled instance) renders
  **byte-identical** to the equivalent WGSL — cross-source identity over the full
  readback buffer. Exercises S.1 (validate/builder) + S.2 (slot+entry) + S.4
  (instance feature) end-to-end.
