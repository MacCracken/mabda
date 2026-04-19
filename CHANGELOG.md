# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
