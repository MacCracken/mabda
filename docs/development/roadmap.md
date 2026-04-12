# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 24 library modules + 4 FFI modules, 3,257 lines, 93 tests. wgpu-native v29 FFI.

## v2.0.0 (Current) — Cyrius Port

Complete port from Rust to Cyrius. All 24 Rust modules ported (`typed_buffer`
is scheduled for v2.1). GPU FFI operational via C launcher + struct-packing
shims.

### Completed
- All core modules: error, color, capabilities, context, profiler, resource
- All buffer/compute modules: buffer, compute, shader_cache, pipeline_cache, bind_group_cache
- All graphics modules: vertex, blend, sampler, depth, texture, bind_group, instancing
- All render modules: render_target, render_pipeline, render_pass, surface, debug
- FFI layer: wgpu_types, wgpu_descriptors, wgpu_ffi, C launcher
- 89 standalone tests + 4 GPU integration tests (including buffer round-trip)
- Buffer readback round-trip (write → copy → map → verify) — **v1.0 criterion met**
- Cyrius compiler fixes upstreamed: PIC codegen (3.4.12), mmap rename (3.4.12), _cyrius_init (3.4.14)
- GPU discovery via yukti 1.2.0
- Struct-packing shim pattern for wgpu functions with 6+ i64 args (avoids `fncall6` bug)
- Flat project layout (src/, lib/, tests/, deps/ at repo root) matching vidya/cyrius convention
- Vendored stdlib trimmed to 15 actually-used modules (down from 45)
- Benchmarks reference: `docs/benchmarks-rust-v-cyrius.md`

### Pending (v2.1)
- `typed_buffer` port (`uniform_buffer_new`, `storage_buffer_new` — Rust had 14 tests, 352 LOC)
- Standalone `.tcyr` tests for pure-data modules (error, capabilities, blend, sampler, depth, bind_group, caches) — ~125 assertions recoverable
- GPU timestamp profiling (GpuTimestamps)
- Texture creation via FFI (wgpuDeviceCreateTexture)
- Render pipeline creation via FFI (wgpuDeviceCreateRenderPipeline)
- Surface management via FFI (wgpuSurfaceGetCurrentTexture)
- `.bcyr` benchmark harness to parity with Rust's 20 benchmarks

## Backlog (demand-gated)

| Feature | Status | Notes |
|---------|--------|-------|
| Render graph (DAG pass orchestration) | Not started | After render pipeline FFI works |
| Multi-queue coordination | Not started | Additive to GpuContext |
| Compressed textures (BC/ETC2/ASTC) | Not started | Needs texture FFI |
| SPIR-V shader support | Not started | Currently WGSL only |
| Mipmap generation | Not started | Needs texture FFI |
| Image loading (PNG/JPEG) | Not started | Needs C image library (stb_image) |
| WebGPU/WASM target | Not started | Needs Cyrius WASM backend |

## v1.0.0 Criteria (met in Rust, porting to Cyrius)

- [x] All 25 modules ported
- [x] GPU context creation working
- [x] Buffer create/write/release working
- [ ] Compute dispatch end-to-end test
- [ ] Render pipeline end-to-end test
- [ ] Consumer integration (soorat port to Cyrius)

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (load64 at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI |
