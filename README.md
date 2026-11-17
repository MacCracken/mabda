# Mabda

**Mabda** (Arabic: مبدأ — origin, principle, starting point) is the GPU
foundation layer for the [AGNOS](https://github.com/MacCracken) ecosystem.
It provides shared GPU infrastructure — over a triple backend (wgpu-native,
a pure-Cyrius native AMD path, and a pure-Cyrius native NVIDIA path) — that
all AGNOS GPU consumers build upon.

Written in [Cyrius](https://github.com/MacCracken/cyrius), the AGNOS
systems language.

Version: 4.4.0 — **triple backend** (wgpu-native + native AMD + native NVIDIA)
behind one stable public API. GA (3.0.0, 2026-06-02) added the pure-Cyrius
**native AMD** path (amdgpu DRM / GFX9 / PM4); v3.1–v3.4 grew it — multi-queue,
block-compressed + array/cube textures, an in-tree SPIR-V→GFX9 f64 compute
compiler, asset loading, and KMS present, all HW-verified on AMD Cezanne.
**v4.0 added a pure-Cyrius native NVIDIA path** (nouveau DRM, Turing/SM75) —
compute, textures, render, and present, HW-proven on a GTX 1660 SUPER (TU116)
and burned in ~30¾h of soak (RSS-flat, dmesg Δ=0). **v4.0.1 deprecates
AMD-on-wgpu** (one-shot warning, still works; `-D MABDA_AMD_WGPU_STRICT` enforces
native-only; retirement deferred). **v4.3.0 starts the render tier** a geometry
renderer draws through (a pipeline descriptor, bind groups, vertex/index buffers,
indexed draws with dynamic offsets, viewport and scissor), and a validation error
returns a `GPU_ERR_*` with wgpu's reason instead of aborting the process. **v4.3.1
completes it** (MB2–MB5): depth targets and shadow passes, sRGB / BGRA8 / RGBA16F textures, MSAA,
texture views, mip generation and copies on wgpu, compute passes and `gpu_compute_dispatch` on
wgpu, and a Wayland window surface. **v4.4.0 compiles SPIR-V for NVIDIA in-tree**
(SPIR-V → SM75 SASS, with control flow, f64 and ranga's kernels), fixes control-flow shapes the
GFX9 compiler ran wrong without an error, widens both compilers to one 1024-id space, adds NVIDIA
block-linear scanout, and passes the seat-only display gates on both GPUs. See
[CHANGELOG.md](CHANGELOG.md) for the full release history and *Hardware support* below.

## Features

- **Three backends, one API** — wgpu-native (cross-vendor default), pure-Cyrius **native AMD** (amdgpu DRM / GFX9 / PM4), pure-Cyrius **native NVIDIA** (nouveau DRM / SM75). Backend chosen at build time via `MABDA_BACKEND_KIND`; the `@public` API is byte-identical across all three.
- **Device lifecycle** — GpuContext creation with adapter/device/queue management
- **Buffer management** — storage, uniform, vertex, index, staging, indirect buffers; synchronous readback; buffer-to-buffer copy; GrowableBuffer with generation tracking
- **Compute pipelines** — shader compilation, bind group layout, dispatch, PingPongBuffer; on wgpu (4.3.1) `gpu_compute_desc_*` pipelines with explicit mixed layouts, compute passes with dynamic offsets, and `gpu_compute_dispatch`
- **Render pipelines** — the `gpu_pipeline_desc_*` descriptor (named entry points, optional fragment stage, vertex layouts incl. the 2D / 3D / skinned / line presets, topology, culling, blending, depth state, explicit bind group layouts); `render_pipeline_create_simple` for full-screen effects, legacy `rpb_*` builder for back-compat
- **Bind groups and draw-time calls** — `gpu_bind_group_layout_*` / `gpu_bind_group_*` (uniform, storage, texture, sampler and comparison-sampler entries), `gpu_sampler_object_create`; vertex and u16/u32 index buffers, `gpu_render_pass_draw_indexed`, dynamic offsets, viewport, scissor, blend constant (wgpu; the native backends serve the vertex-less subset)
- **An error channel** — validation errors return a `GPU_ERR_*` with the API's reason (`gpu_last_error_message`); error scopes; nothing aborts the host process
- **Textures** — RGBA + block-compressed (BC1/3/4/5/7) + array/cubemap; creation, TextureCache, mip levels, GPU sampling, dimension validation
- **Render graph** — DAG pass orchestration with multi-queue scheduling (native runs nodes on distinct HW rings)
- **Render targets and passes** — `gpu_render_target_create` in RGBA8 / sRGB / BGRA8 / RGBA16F / Depth32F with 4x MSAA, the `gpu_pass_desc_*` pass descriptor (up to 8 colour targets, depth with load / clear / store / read-only, resolve), texture views (a cube face or mip level as an attachment), texture copies, mip generation and render-target readback (wgpu; the native backends serve one RGBA8 target)
- **Window surfaces** — a wgpu surface from a Wayland `wl_display` / `wl_surface`, its capabilities, BGRA8 formats, AUTO_VSYNC / AUTO_NO_VSYNC, frames as texture objects, resize; native KMS present on AMD and NVIDIA
- **Asset loading** — KTX2 / DDS parsers in-tree (KTX2 Zstandard / ZLIB supercompression since 4.5.0, through decoders the consumer registers) + PNG / baseline-JPEG via the `chitra` package (opt-in, `-D MABDA_PNG` / `-D MABDA_JPEG`)
- **Native SPIR-V compilers** — in-tree compute-shader lowering to GFX9 (AMD) and, since 4.4.0, to SM75 SASS (NVIDIA): structured control flow, int div/mod, vectors, f64; HW-verified on Cezanne and TU116
- **KMS present** — pure-Cyrius modeset + double-buffered vsync page-flip (native path)
- **Profiling** — FrameProfiler with EMA smoothing, frame history, pass timings, JSON / CSV export, explicit scope timing
- **Caching** — ShaderCache, PipelineCache, BindGroupCache for GPU resource deduplication
- **Capabilities** — GPU feature/limit detection, WebGPU compatibility constants

## Quick Start

mabda is an **opt-in** distlib — it is *not* auto-prepended (at ~1.9 MB — 1,861,396 B at 4.4.0 — it would
blow the stdlib preprocess cap). The base stdlib (`string`/`alloc`/`str`/`fmt`/
`vec`/`io`/…) is auto-prepended by `cyrius`; a minimal consumer includes only
these three manually, in this order, before `mabda`:

```cyrius
include "lib/mmap.cyr"     # mmap flags (MAP_SHARED, …) — GPU buffer mapping
include "lib/dynlib.cyr"   # dlopen/dlsym — wgpu-native FFI binding
include "lib/sakshi.cyr"   # structured logging
include "lib/mabda.cyr"    # mabda itself — MUST come last

fn mabda_main(fn_table_ptr, preinit_ptr) {
    wgpu_ffi_init_table(fn_table_ptr);

    # Create GPU context (via C launcher pre-init).
    # A Result is a (tag, payload) register pair since cyrius 6.6.0 —
    # bind BOTH halves; the payload is the context handle.
    var res_tag, res = gpu_context_from_preinit(preinit_ptr);
    var ctx = res;
    var device = gpu_ctx_device(ctx);
    var queue = gpu_ctx_queue(ctx);

    # Create a storage buffer
    var usage = WGPU_BUFFER_USAGE_STORAGE | WGPU_BUFFER_USAGE_COPY_DST;
    var desc = wgpu_buffer_descriptor("my-buf", usage, 1024, 0);
    var buf = wgpu_device_create_buffer(device, desc);

    # Write data
    var data[64];
    store64(&data, 42);
    wgpu_queue_write_buffer(queue, buf, 0, &data, 64);

    wgpu_buffer_release(buf);
    gpu_context_release(ctx);
    return 0;
}
```

`_cyrius_init()` and `alloc_init()` are called by the C launcher before
`mabda_main` runs — see [docs/stdlib-integration.md](docs/stdlib-integration.md)
for the full wiring. (The C launcher + `wgpu_ffi_init_table` shown above are the
**wgpu path**; native backends select via `MABDA_BACKEND_KIND` +
`gpu_context_new_native()` / `gpu_context_new_native_nvidia()` and don't use the
launcher.)

Extra deps opt in via compile flags: **PNG / JPEG asset loading**
(`-D MABDA_PNG` / `-D MABDA_JPEG`) both pull `[deps.chitra]` (tag 1.4.1) + stdlib
`thread`/`sankoch` (include their `lib/*.cyr` before `mabda`); **logind master
delegation** (`-D MABDA_LOGIND`) pulls `[deps.samvada]` (tag 1.1.2; the consumer
calls `samvada_native_init()` first — pure-Cyrius dbus, no C shim). **KTX2 zstd / zlib
supercompression** (4.5.0) needs no flag: the consumer includes sankoch and registers its
decoders (`gpu_ktx2_set_zstd_decoder(&zstd_decompress)`, `gpu_ktx2_set_zlib_decoder(...)`).
A minimal consumer needs none of these — the dist compiles the flagged paths out.

## Modules

| Layer | Modules |
|-------|---------|
| **Core** | error, color, capabilities, context, profiler, resource, debug |
| **Buffers / compute** | buffer, typed_buffer, compute (workgroup math, dispatch, PingPongBuffer), gpu_timestamps |
| **Graphics** | vertex, blend, sampler, depth, texture, texture_format, bind_group, instancing |
| **Render** | render_target, render_pipeline, render_pass, render_graph, queue, surface, surface_v3 |
| **Render tier** (4.3.x) | gpu_types, error_scope, gpu_bind, gpu_pipeline, gpu_draw, gpu_target, gpu_compute_tier |
| **Assets** | asset_format, asset_load (KTX2 / DDS / PNG+JPEG-via-chitra) |
| **Caching** | shader_cache, pipeline_cache, bind_group_cache |
| **Backend abstraction** (`@internal`) | backend, backend_wgpu, backend_wgpu_rtier, backend_wgpu_target, backend_wgpu_ctier, backend_wgpu_surface, backend_native_rtier |
| **Native AMD** (`@internal`) | backend_native, backend_native_amdgpu, backend_native_pm4, backend_native_shaders, backend_native_kms |
| **SPIR-V → GFX9 compiler** (`@internal`) | spirv_parse, spirv_lower, mir, gfx9_encode, gfx9_isel, gfx9_regalloc, gfx9_waitcnt, gfx9_abi, gfx9_compile |
| **Native NVIDIA** (`@internal`) | backend_nvidia, backend_nvidia_nouveau, backend_nvidia_push, backend_nvidia_qmd, backend_nvidia_sass, backend_nvidia_tex |
| **SPIR-V → SM75 compiler** (`@internal`, 4.4.0) | sm75_encode, sm75_isel, sm75_regalloc, sm75_compile (front end and MIR shared with GFX9) |
| **wgpu FFI** (`@internal`) | wgpu_types, wgpu_descriptors, wgpu_ffi |

## Consumers

| Project | Use Case |
|---------|----------|
| **soorat** | Rendering engine (sprites, PBR, shadows, post-effects) |
| **rasa** | Image editor (GPU compute filters) |
| **ranga** | Image processing (GPU pixel ops) |
| **bijli** | EM simulation (FDTD compute) |
| **aethersafta** | Desktop compositor (GPU compositing) |
| **kiran** | Game engine (via soorat) |
| **puka** | Terminal emulator (GPU text rendering) |

## Architecture

Mabda owns the GPU boundary. Consumers depend on mabda, not on wgpu or the
kernel driver directly. The `@public` API is identical across all backends; the
backend is a compile-time choice (`MABDA_BACKEND_KIND`), routed through an
`@internal` `Backend` slot table.

```
Consumer (soorat, bijli, ...)
    ↓
  mabda (GPU abstraction — same @public API on every backend)
    ↓  @internal Backend slot table
  ┌──────────────────────┐ ┌──────────────────────┐ ┌──────────────────────┐
  │ wgpu (cross-vendor    │ │ native AMD (v3.0)    │ │ native NVIDIA (v4.0)  │
  │ default)              │ │ amdgpu DRM ioctls +  │ │ nouveau DRM +         │
  │ wgpu-native + C       │ │ GFX9 PM4 + SPIR-V→   │ │ clc597 push + SM75    │
  │ launcher → Vulkan /   │ │ GFX9 compiler + KMS  │ │ SASS + KMS present    │
  │ Metal / DX12          │ │ (GFX9 verified)      │ │ (Turing/SM75 verified)│
  └──────────────────────┘ └──────────────────────┘ └──────────────────────┘
```

The wgpu path is the cross-vendor default; **AMD-on-wgpu is deprecated at
v4.0.1** (native-only, warn+allow; `-D MABDA_AMD_WGPU_STRICT` enforces). NVIDIA
+ Intel keep their wgpu route until their native backends are in production
(NVIDIA native shipped v4.0; Intel is tentatively v5.0). See
[ADR 006](docs/adr/006-native-cyrius-gpu-backend.md).

## Hardware support

Mabda's wgpu and native paths have different hardware reach. The native
backend lands one vendor at a time (AMD, then NVIDIA), and **wgpu retires per-chipset**
as each vendor's native path matures — not all-at-once. The full
roadmap is in [docs/development/roadmap.md](docs/development/roadmap.md).

| Backend           | Vendors                  | Status                             |
|-------------------|--------------------------|------------------------------------|
| `wgpu`            | NVIDIA, Intel (+ AMD, **deprecated** at v4.0.1)                        | **Cross-vendor default. Shipping.** AMD-on-wgpu is deprecated at v4.0.1 (warn+allow; `-D MABDA_AMD_WGPU_STRICT` enforces native-only). Retires per-chipset as each vendor's native path matures. |
| `native` (AMD)    | AMD                      | **Shipping (v3.0 GA + the v3.1–v3.4 arcs).** Compute, textures (incl. BC-compressed + arrays/cubes), render, multi-queue, a SPIR-V→GFX9 f64 compiler, and KMS present all verified end-to-end on AMD Cezanne (gfx90c, GFX9). Other GFX families (GFX10/11/12, RDNA*) not yet exercised — same amdgpu / PM4 / DRM ioctl path, but each generation needs its own bring-up. |
| `native` (NVIDIA) | NVIDIA                   | **Shipping (v4.0).** Compute (pre-compiled SASS, and SPIR-V compiled in-tree since 4.4.0), textures (incl. GPU sampling), render, and KMS present (block-linear scanout since 4.4.0) HW-verified on Turing / SM75 (GTX 1660 SUPER / TU116); ~30¾h soak (RSS-flat, dmesg Δ=0). nouveau DRM + clc597 push + SM75 SASS, no PM4. Other SM families not yet exercised — each needs its own bring-up. |
| `native` (Intel)  | Intel                    | **Tentative for v5.0.** Different submission path (i915 / Xe, no PM4, Gen ISA). Intel consumers stay on `wgpu` until v5.0 ships. |

**Per-chipset retirement.** When a vendor's native backend has been
in production for a full release cycle on that vendor's hardware,
the wgpu path is retired *for that vendor* — the wgpu binding stays
in-tree to serve the vendors whose native backends haven't shipped
yet. The cutovers (per the roadmap):

- **v4.0.1** — AMD wgpu **deprecated** (one-shot warning; still works — escape hatch open).
  `-D MABDA_AMD_WGPU_STRICT` enforces native-only; full AMD-wgpu retirement is deferred.
  NVIDIA + Intel still on wgpu.
- **v5.0** — NVIDIA wgpu retires. NVIDIA consumers run on NVIDIA
  native only. Intel still on wgpu.
- **v5.1** — Intel wgpu retires; the wgpu+C path leaves the tree
  entirely. Mabda becomes fully native-Cyrius across every supported
  vendor.

**No consumer is forced onto the native backend before their
chipset's native path is real** — it is opt-in per build, and the
wgpu fallback exists for every vendor until that vendor's native
backend is in production. Each retirement is gated on every consumer
on that vendor having flipped voluntarily; calendar dates are not
the gate.

Linux is the only OS the native backend targets. macOS and Windows
consumers stay on `wgpu`; cross-OS support beyond v5.1 is gated on a
non-wgpu story for those targets, which is not currently scoped.
See [ADR 006](docs/adr/006-native-cyrius-gpu-backend.md) for the
multi-backend rationale.

## Build

Requires [Cyrius](https://github.com/MacCracken/cyrius) 6.7.18+ and gcc
(for the GPU integration test only — CPU tests and benchmarks need
only `cyrius`).

```sh
# Resolve stdlib deps
cyrius deps

# Full gate sweep (lint, fmt, vet, version-check, distlib-sync, tests, bench)
make test-all

# CPU-only unit suite (5695 assertions across 18 domain files)
make test            # globs tests/tcyr/*.tcyr (count via scripts/count-test-assertions.sh)

# CPU-only benchmark harness (9 benches; GPU benches via `make bench-gpu`)
cyrius bench tests/bcyr/mabda.bcyr

# Regenerate the dist bundle
cyrius distlib                                # → dist/mabda.cyr

# GPU integration tests (need wgpu-native in deps/wgpu-native/)
sh deps/fetch-wgpu.sh                         # one-time
make test-gpu                                 # phase0 + compute_e2e + render_e2e + render_graph_e2e
make bench-gpu                                # 13 GPU benches
```

## Project Structure

```
mabda/
├── src/                 57 modules (32 @public + 25 @internal FFI/backend)
│                        (src/lib.cyr is the single include chain)
├── tests/
│   ├── tcyr/                     18 functionality-named domain suites — core,
│   │                            buffer, compute, texture, graphics, render,
│   │                            backend, caches, surface, native, nvidia, kms,
│   │                            queue, asset_load,
│   │                            compiler_{lower,backend,compile,encode}
│   │                            (5695 asserts; `make test` globs them all)
│   └── bcyr/mabda.bcyr           CPU-only benchmark harness (9 benches)
├── programs/            GPU integration programs (wgpu + native) + dev spikes + `benchmarks.cyr`
│   ├── smoke.cyr                Link-check — `cyrius build` entry point
│   ├── phase0.cyr               wgpu buffer/texture/pipeline smoke
│   ├── compute_e2e.cyr          wgpu compute dispatch round-trip
│   ├── render_e2e.cyr           wgpu render-pass clear + readback
│   ├── render_graph_e2e.cyr     wgpu three-node DAG (compute → render → copy)
│   ├── native_compute_store.cyr native compute, write 0xDEADBEEF + readback
│   ├── native_texture_e2e.cyr   native texture round-trip
│   ├── native_render_e2e.cyr    native clear-triangle + pixel verify
│   ├── native_kms_summary.cyr   Phase D KMS topology probe (no master)
│   ├── native_kms_modeset_smoke.cyr  native modeset visual smoke
│   ├── native_present_e2e.cyr   Phase D 120-frame animated present
│   └── benchmarks.cyr           13 GPU benchmarks, Rust-v1 parity set
├── lib/                 Real dir populated by `cyrius deps` — copied stdlib
│                        modules + dep bundles (no symlinks). Gitignored; NEVER a
│                        hand-made symlink to a cyrius checkout (corruption bug).
├── deps/                C launcher (wgpu_main.c), wgpu-native v29 binaries
├── dist/mabda.cyr       Bundled library (generated by `cyrius distlib`)
├── examples/stdlib-consumer/   Minimal "hello GPU" reference project
├── docs/                Architecture, roadmap, ADRs, audit history, guides
├── scripts/             version-check.sh, version-bump.sh
├── cyrius.cyml          Package manifest (toolchain pin, [lib], [deps])
├── Makefile             Thin wrapper over `cyrius` CLI + GPU path
├── VERSION              4.4.0
└── CHANGELOG.md
```

The `@public` / `@internal` markers on line 1 of every `src/*.cyr` file
are load-bearing: `@public` = stable API surface that is byte-identical across
backends (wgpu / native AMD / native NVIDIA); `@internal` = FFI/backend
scaffolding. See
[docs/adr/005-public-api-surface-marking.md](docs/adr/005-public-api-surface-marking.md)
and [docs/stdlib-integration.md](docs/stdlib-integration.md).

The frozen Rust v1.0.0 source lives under git tag `1.0.0`
(`git checkout 1.0.0`). Historical Rust benchmark numbers are preserved
at [docs/rust-v1-bench-history.csv](docs/rust-v1-bench-history.csv).

## Security

Mabda ships as a first-party trusted AGNOS package with a per-arc security
audit. The audit history lives in [docs/audit/](docs/audit/) (latest:
`2026-09-16-audit.md`, v4.1.3).
Report vulnerabilities via the repository's
[Security tab](../../security/advisories). See [SECURITY.md](SECURITY.md)
for the policy.

## License

GPL-3.0-only
