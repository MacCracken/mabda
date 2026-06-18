# Mabda

**Mabda** (Arabic: مبدأ — origin, principle, starting point) is the GPU
foundation layer for the [AGNOS](https://github.com/MacCracken) ecosystem.
It wraps the wgpu-native C API and provides shared GPU infrastructure
that all AGNOS GPU consumers build upon.

Written in [Cyrius](https://github.com/MacCracken/cyrius), the AGNOS
systems language.

Version: 3.2.8 (dual backend — wgpu + native AMD; see
*Hardware support* below. GA (3.0.0) cut 2026-06-02. 3.1.0 added on-device
mipmap generation; 3.1.1 added multi-queue coordination (both native AMD
HW-verified). 3.2.0 opens the "texture & shader breadth" arc with
block-compressed textures (BC / ETC2 / ASTC): wgpu creates + uploads +
samples; native AMD stores + reads back. 3.2.1 adds buffer-to-buffer copy on
both backends (native SDMA on a real DMA ring with >4 MiB chaining; wgpu
copy_buffer_to_buffer). 3.2.2 adds native RGBA8 texture *sampling* (GFX9 T#/S#
+ a textured FS). 3.2.3 completes native sampling: **block-compressed (BC1/BC3/
BC4/BC5/BC7) sampling** via SDMA SW_64KB_S tiling + the image_sample FS, and
**bilinear / scaled** filtering — all HW-verified pixel-exact on Cezanne
(ETC2/ASTC are HW-blocked on AMD). 3.2.4 adds **SPIR-V shader ingestion** on
wgpu — a SPIR-V binary is a peer frontend to WGSL (kind-tagged shader-create +
SPIR-V validation + source-kind-aware cache), HW-verified byte-identical to WGSL.
3.2.5 opens the **native SPIR-V → GFX9 compiler** (Phases N.0 + N.1, pure CPU):
operand-parameterized GFX9 instruction encoders proven byte-identical against the
hand-authored shaders (which now emit through them), plus the SPIR-V parser — a
validate/rejection gate over untrusted input + type/constant/decoration lookup
tables. See the CHANGELOG.)

## Features

- **Device lifecycle** — GpuContext creation with adapter/device/queue management
- **Buffer management** — storage, uniform, vertex, index, staging, indirect buffers; synchronous readback; GrowableBuffer with generation tracking
- **Compute pipelines** — shader compilation, bind group layout, dispatch, PingPongBuffer
- **Render pipelines** — `render_pipeline_create_simple` for full-screen effects, legacy `rpb_*` builder for back-compat
- **Textures** — RGBA creation, TextureCache, mip levels, dimension validation
- **Render targets** — offscreen framebuffers with optional MSAA and depth
- **Profiling** — FrameProfiler with EMA smoothing, frame history, explicit scope timing
- **Caching** — ShaderCache, PipelineCache, BindGroupCache for GPU resource deduplication
- **Capabilities** — GPU feature/limit detection, WebGPU compatibility constants

## Quick Start

mabda is an **opt-in** distlib — it is *not* auto-prepended (at ~515 KB it would
blow the stdlib preprocess cap). Its stdlib deps that aren't in the cyrius
auto-prepend union must be `include`d **explicitly, in this order, before
`mabda`** (the base stdlib — `string`/`alloc`/`str`/`fmt`/`vec`/`io`/… — is
auto-prepended by `cyrius`, so only these three are manual):

```cyrius
include "lib/mmap.cyr"     # mmap flags (MAP_SHARED, …) — GPU buffer mapping
include "lib/dynlib.cyr"   # dlopen/dlsym — wgpu-native FFI binding
include "lib/sakshi.cyr"   # structured logging
include "lib/mabda.cyr"    # mabda itself — MUST come last

fn mabda_main(fn_table_ptr, preinit_ptr) {
    color_init();
    wgpu_ffi_init_table(fn_table_ptr);

    # Create GPU context (via C launcher pre-init)
    var res = gpu_context_from_preinit(preinit_ptr);
    var ctx = payload(res);
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
for the full wiring.

## Modules

| Layer | Modules |
|-------|---------|
| **Core** | error, color, capabilities, context, profiler, resource, debug |
| **Buffers** | buffer, typed_buffer, compute (workgroup math, dispatch, PingPongBuffer), gpu_timestamps |
| **Graphics** | vertex, blend, sampler, depth, texture, bind_group, instancing |
| **Render** | render_target, render_pipeline, render_pass, surface |
| **Caching** | cache_key, shader_cache, pipeline_cache, bind_group_cache |
| **FFI** (`@internal`) | wgpu_types, wgpu_descriptors, wgpu_ffi |

## Consumers

| Project | Use Case |
|---------|----------|
| **soorat** | Rendering engine (sprites, PBR, shadows, post-effects) |
| **rasa** | Image editor (GPU compute filters) |
| **ranga** | Image processing (GPU pixel ops) |
| **bijli** | EM simulation (FDTD compute) |
| **aethersafta** | Desktop compositor (GPU compositing) |
| **kiran** | Game engine (via soorat) |

## Architecture

Mabda owns the wgpu-native FFI boundary. Consumers depend on mabda, not
on wgpu directly.

```
Consumer (soorat, bijli, ...)
    ↓
  mabda (GPU abstraction)
    ↓
  wgpu-native C API (via function table + C launcher)        ← shipping today (v2.5.x)
    ↓
  Vulkan / Metal / DX12
```

In v3.0 a second backend lands alongside the wgpu path. It is selected
per-consumer; both coexist. Today the native backend is AMD-only:

```
Consumer (soorat, bijli, ...)
    ↓
  mabda (GPU abstraction — same @public API on both backends)
    ↓
  ┌──────────────────────────────┐  ┌──────────────────────────────┐
  │ wgpu backend (default)       │  │ native backend (v3.0, opt-in)│
  │   wgpu-native + C launcher   │  │   pure Cyrius, AMD/amdgpu    │
  │   Vulkan / Metal / DX12      │  │   DRM ioctls + GFX9 ISA + PM4│
  │   AMD / NVIDIA / Intel       │  │   AMD only (GFX9 verified;   │
  │   on Linux/macOS/Windows     │  │   NVIDIA + Intel = future    │
  │                              │  │   work, see roadmap)         │
  └──────────────────────────────┘  └──────────────────────────────┘
```

## Hardware support

Mabda's two backends have different hardware reach. The native
backend lands one vendor at a time, and **wgpu retires per-chipset**
as each vendor's native path matures — not all-at-once. The full
roadmap is in [docs/development/roadmap.md](docs/development/roadmap.md).

| Backend           | Vendors                  | Status                             |
|-------------------|--------------------------|------------------------------------|
| `wgpu`            | AMD, NVIDIA, Intel (anything wgpu-native + Vulkan/Metal/DX12 supports) | **Default. Shipping.** All v2.x consumers run here. Retires per-chipset as each vendor's native path matures. |
| `native` (AMD)    | AMD                      | **In development (v3.0, branch `v3`).** Compute dispatch verified end-to-end on AMD Cezanne (gfx90c, GFX9). Other GFX families (GFX10/11/12, RDNA*) not yet exercised — same amdgpu / PM4 / DRM ioctl path, but each generation needs its own bring-up. |
| `native` (NVIDIA) | NVIDIA                   | **Scoped to v4.0.** Different submission path entirely (nouveau / nvgpu, no PM4, different ISA). NVIDIA consumers stay on `wgpu` until v4.0 ships. |
| `native` (Intel)  | Intel                    | **Tentative for v5.0.** Different submission path (i915 / Xe, no PM4, Gen ISA). Intel consumers stay on `wgpu` until v5.0 ships. |

**Per-chipset retirement.** When a vendor's native backend has been
in production for a full release cycle on that vendor's hardware,
the wgpu path is retired *for that vendor* — the wgpu binding stays
in-tree to serve the vendors whose native backends haven't shipped
yet. The cutovers (per the roadmap):

- **v4.0** — AMD wgpu retires. AMD consumers run on AMD native only.
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

Requires [Cyrius](https://github.com/MacCracken/cyrius) 5.5.20+ and gcc
(for the GPU integration test only — CPU tests and benchmarks need
only `cyrius`).

```sh
# Resolve stdlib deps
cyrius deps

# Full gate sweep (lint, fmt, vet, version-check, distlib-sync, tests, bench)
make test-all

# CPU-only unit suite (1991 assertions across 3 files)
make test            # mabda.tcyr (633) + mabda_v3.tcyr (968) + mabda_v3_phase_d.tcyr (390)

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
├── src/                 38 modules (28 @public + 10 @internal FFI)
│                        (src/lib.cyr is the single include chain)
├── tests/
│   ├── tcyr/mabda.tcyr           v2 backend-agnostic suite (633 asserts)
│   ├── tcyr/mabda_v3.tcyr        v3 backend + compute + render (968 asserts)
│   ├── tcyr/mabda_v3_phase_d.tcyr  Phase D KMS + 7.7 surface (390 asserts)
│   └── bcyr/mabda.bcyr           CPU-only benchmark harness (9 benches)
├── programs/            10 GPU integration programs + dev spikes + `benchmarks.cyr`
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
├── lib/                 Real dir populated by `cyrius deps` — stdlib copies +
│                        symlinks into ~/.cyrius/deps/. Gitignored; NEVER a
│                        hand-made symlink to a cyrius checkout (corruption bug).
├── deps/                C launcher (wgpu_main.c), wgpu-native v29 binaries
├── dist/mabda.cyr       Bundled library (generated by `cyrius distlib`)
├── examples/stdlib-consumer/   Minimal "hello GPU" reference project
├── docs/                Architecture, roadmap, ADRs, audit history, guides
├── scripts/             version-check.sh, version-bump.sh
├── cyrius.cyml          Package manifest (toolchain pin, [lib], [deps])
├── Makefile             Thin wrapper over `cyrius` CLI + GPU path
├── VERSION              3.0.3
└── CHANGELOG.md
```

The `@public` / `@internal` markers on line 1 of every `src/*.cyr` file
are load-bearing: `@public` = stable API surface that survives the v3.0
backend swap; `@internal` = FFI scaffolding that gets replaced. See
[docs/adr/005-public-api-surface-marking.md](docs/adr/005-public-api-surface-marking.md)
and [docs/stdlib-integration.md](docs/stdlib-integration.md).

The frozen Rust v1.0.0 source lives under git tag `1.0.0`
(`git checkout 1.0.0`). Historical Rust benchmark numbers are preserved
at [docs/rust-v1-bench-history.csv](docs/rust-v1-bench-history.csv).

## Security

Mabda 2.3.0 shipped as the last audit-gated release before first-party
trusted status. Full findings + remediation in
[docs/audit/2026-04-19-audit.md](docs/audit/2026-04-19-audit.md).
Report vulnerabilities via the repository's
[Security tab](../../security/advisories). See [SECURITY.md](SECURITY.md)
for the policy.

## License

GPL-3.0-only
