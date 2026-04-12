# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
