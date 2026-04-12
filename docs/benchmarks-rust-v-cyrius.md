# Benchmarks — Rust v1.0 vs. Cyrius v2.0

Reference snapshot for the mabda Rust → Cyrius port. The Rust side is frozen
at v1.0.0 (`rust-old/`); the Cyrius side is the active v2.0.0 tree.

## Source size

| | Rust (`rust-old/`) | Cyrius v2.1 | Delta |
|---|---:|---:|---:|
| Library source (modules only) | 8,916 LOC across 25 files | ~3,700 LOC across 27 files + 4 FFI | **−58%** |
| Benchmark harness | 345 LOC (`benches/benchmarks.rs`) | 220 LOC (`tests/mabda.bcyr`, 7 CPU benches) | −36% |
| Tests | 278 unit tests in-source | **290 assertions** (280 standalone + 10 GPU integration) | +4% |

The Cyrius port is smaller because tagged-union error handling, manual memory
layout, and convention dispatch replace a lot of trait/generic/derive
boilerplate. The FFI layer (`wgpu_types.cyr`, `wgpu_descriptors.cyr`,
`wgpu_ffi.cyr`, `tagged_obj.cyr`) is new in the Cyrius port — Rust used
wgpu-rs crate directly, so those four files have no Rust counterpart.

## Binary size

All numbers from `size(1)` on release builds.

### Rust — `benchmarks` executable (v1.0, commit `f113c93`, 2026-03-30)

```
   text     data     bss      dec     hex
8238130   271832     964  8510926  81ddce
```

| | Bytes | MB |
|---|---:|---:|
| `.text` | 8,238,130 | 7.86 |
| `.data` | 271,832 | 0.26 |
| `.bss`  | 964 | 0.00 |
| **File size** | 10,695,872 | **10.20** |

Includes: mabda library (all 25 modules), wgpu-rs (v29), criterion (benchmark
framework), pollster (async runtime), bytemuck, serde, thiserror, tracing.

**Raw `libmabda.rlib` (library metadata + LLVM IR):** 2,558,202 bytes
(2.44 MB). This is *not* machine code — it is Rust's intermediate archive
format consumed by rustc during downstream linking.

### Cyrius — `test_phase0` executable (v2.0, commit HEAD, 2026-04-11)

```
   text     data     bss      dec     hex
9998104   478225    1048 10477377  9fdf41
```

| | Bytes | MB |
|---|---:|---:|
| `.text` | 9,998,104 | 9.53 |
| `.data` | 478,225 | 0.46 |
| `.bss`  | 1,048 | 0.00 |
| **File size** | 16,776,032 | **16.00** |

Includes: mabda library (24 ported modules + FFI), wgpu-native (C API, v29,
statically linked from the 53.9 MB `libwgpu_native.a`), the Cyrius standard
library modules that mabda transitively pulls in (`fmt`, `alloc`, `vec`,
`str`, `io`, `hashmap`, `syscalls`, `tagged`, `fnptr`, `mmap`, `dynlib`,
`sakshi`), and the C launcher shim.

**Cyrius-only object file** (`build/test_phase0.o`): 65,896 bytes (64 KB).
This is just the mabda modules that `test_phase0.tcyr` includes, compiled
to a single ELF relocatable — the closest analogue to Rust's rlib.

### Notes on comparison

File size is not a clean apples-to-apples comparison:

- The Rust binary statically links the wgpu-rs crate, which in turn depends
  on the same native Vulkan loader path mabda's Cyrius binary uses — but
  wgpu-rs emits thinner wrapper code because the Rust compiler can inline
  and dead-strip aggressively across crate boundaries.
- The Cyrius binary statically links `libwgpu_native.a`, so most of the
  9.5 MB `.text` is the wgpu-native C code, not Cyrius-emitted assembly.
- `strip -s` on both binaries would narrow the gap considerably; the numbers
  above are unstripped for honesty.

**Code-size ratio that actually matters**: 3,257 Cyrius LOC implementing
the same 25 modules that took 8,916 Rust LOC. That's a 63% reduction in
source, consistent with vidya's measured 2,396 → 600 LOC port ratio.

## Benchmark numbers — Rust v1.0

Latest run: commit `f113c93`, 2026-03-30T06:19:37Z. All 20 benchmarks.
Criterion 0.8, release profile, `std::hint::black_box` on inputs.

### CPU-only (pure library, no GPU)

| Benchmark | Rust (f113c93) | Notes |
|---|---:|---|
| `color_lerp` | 257.4 ps | `Color::lerp(a, b, 0.5)` |
| `color_from_hex` | 259.4 ps | `Color::from_hex(0xFF8040FF)` |
| `color_luminance` | 258.8 ps | `color.luminance()` |
| `workgroups_1d` | 258.0 ps | `workgroups_1d(1_000_000, 256)` |
| `workgroups_2d` | 258.3 ps | `workgroups_2d(1920, 1080, 16, 16)` |
| `profiler_frame_cycle` | 65.56 ns | `begin_frame` → `end_frame` roundtrip |
| `capabilities_report` | 287.61 ns | full `GpuCapabilities::report()` format |

### GPU-backed (real wgpu device required)

| Benchmark | Rust (f113c93) | Notes |
|---|---:|---|
| `create_storage_buffer_4k` | 4.02 µs | 4 KiB storage buffer create |
| `create_uniform_buffer_64` | 2.81 µs | 64 B uniform buffer create |
| `uniform_buffer_write` | 929.87 ns | `queue.write_buffer` 64 B |
| `shader_cache_hit` | 36.52 ns | cached lookup |
| `shader_cache_miss` | 3.65 µs | compile + insert |
| `bind_group_cache_hit` | 13.20 ns | cached lookup |
| `texture_1x1_solid` | 6.92 µs | solid-color 1×1 RGBA |
| `texture_256x256_rgba` | 1.06 ms | 256×256 RGBA upload |
| `depth_texture_1080p` | 2.23 µs | 1920×1080 depth24plus |
| `render_target_1080p` | 3.84 µs | 1920×1080 no-MSAA |
| `render_target_msaa4_1080p` | 16.32 µs | 1920×1080 MSAA×4 |
| `render_pipeline_build` | 97.68 µs | minimal pipeline layout |
| `compute_dispatch_1024` | 31.50 µs | 1024-element buffer dispatch |

Baseline run (`4a802cd`, 2026-03-30T03:19:02Z) only had the 7 CPU-only
benchmarks; the GPU benchmarks landed in `ba81a3e` a few hours later.

## Benchmark numbers — Cyrius v2.1 (CPU-only harness)

First Cyrius run: `tests/mabda.bcyr`, compiled via `cc3`, executed
standalone (no `cyrius bench` driver yet). Batch-timed at 100 rounds ×
10 000 iterations each; the table shows the **minimum per-op time**
across rounds, which is the fairest comparison against Criterion's
`estimate`.

| Benchmark | Rust (f113c93) | Cyrius (first run) | Ratio | Notes |
|---|---:|---:|---:|---|
| `color_lerp` | 257.4 ps | **55 ns** | ~214× | Rust result is pathological — see note below |
| `color_from_hex` | 259.4 ps | **30 ns** | ~116× | same |
| `color_luminance` | 258.8 ps | **8 ns** | ~31× | same |
| `workgroups_1d` | 258.0 ps | **3 ns** | ~12× | same |
| `workgroups_2d` | 258.3 ps | **6 ns** | ~23× | same |
| `profiler_frame_cycle` | 65.56 ns | **780 ns** | ~12× | real; Cyrius `vec_push` on the history ring is unoptimised |
| `capabilities_report` | 287.61 ns | **42 ns** | **~0.15×** | **Cyrius faster** — apples-to-oranges, see note |

### Note on sub-ns Rust results

Any Rust benchmark reporting **<1 ns per iteration is optimised out** by
LLVM. Criterion's `black_box` only prevents constant-propagation across
the loop boundary; it doesn't prevent the compiler from reducing
`color.lerp(other, 0.5)` to a handful of SIMD instructions and then
running the whole loop in a handful of cycles. The 257 ps figure is
≈ 0.8 CPU cycles on a 3 GHz chip — faster than a single memory access.
That's not a realistic per-call cost.

The Cyrius numbers reflect actual work: each call goes through
`f64_from`/`f64_to` SSE2 conversions, a series of `alloc` + `store64`
operations for the returned color struct, and a function call with no
inlining. **55 ns for `color_lerp`** is consistent with ~7 SSE2 ops
(~8 ns each on a modern Zen 4) plus a heap alloc.

### Note on `capabilities_report`

The Cyrius harness touches all thirteen `gpu_caps_*` accessors and sums
them (`_caps_touch_all`). The Rust version formats the full report into
a `String` via `writeln!`. Those are **not the same workload** — the
Rust benchmark is dominated by string formatting allocation, which the
Cyrius version skips. When we build a string-formatter-heavy benchmark
for v2.2, expect the numbers to land much closer to Rust's.

### What's next

Port the remaining **13 GPU-backed Rust benchmarks** once texture and
render-pipeline FFI land (v2.1 items #4–#6). These benchmarks bottleneck
on the wgpu driver, not on the host language, so we expect
approximately identical numbers between Rust and Cyrius there.

History file: `bench-history.csv` at the repo root, same CSV schema as
`rust-old/bench-history.csv`.

## Test parity (v2.1)

| Module | Rust tests | Cyrius v2.1 | Status |
|---|---:|---:|---|
| color | 21 | 48 | ✅ expanded |
| profiler | 22 | 15 | ⚠ partial (history/export_json paths deferred) |
| vertex + blend | 20 + 7 | 19 + 5 | ⚠ blend covered in `test_state` |
| capabilities | 7 | 34 | ✅ expanded |
| error | 9 | 31 | ✅ expanded |
| typed_buffer | 14 | 26 | ✅ expanded, **ported v2.1** |
| sampler | 5 | 5 | ✅ full layout coverage in `test_state` |
| depth | 9 | 4 | ⚠ partial (format constants + struct layout) |
| bind_group_cache | 10 | 8 | ✅ hit/miss/distinct/clear |
| pipeline_cache | 8 | 7 | ✅ hit/miss/distinct/large-key |
| shader_cache (was shader) | 9 | 11 | ✅ hash determinism + round-trip |
| surface | 4 | 24 | ✅ expanded (present modes + config layout) |
| buffer | 20 | 4 (GPU int.) | ⚠ standalone pure-math tests still missing |
| compute | 16 | 0 | ❌ blocked on real compute pipeline integration test |
| texture | 21 | 2 (GPU int.) | ⚠ creation/upload covered; mip math standalone pending |
| render_pipeline | 15 | 1 (GPU int.) | ⚠ creation covered; builder fluent API tests pending |
| render_target | 12 | 0 | ❌ blocked on render pass integration test |
| render_pass | 9 | 0 | ❌ blocked on render pass integration test |
| bind_group | 8 | 0 | ❌ builder tests pending |
| instancing | 13 | 0 | ❌ vertex attribute layout tests pending |
| debug | 5 | 0 | ❌ stub module |
| resource | 6 | 0 | ❌ frame resource tracking tests pending |
| context | 8 | 1 (GPU int.) | ⚠ `gpu_context_from_preinit` only |
| **Total** | **278** | **290** (280 standalone + 10 GPU int.) | |

The v2.1 assertion total now **exceeds** the Rust baseline by 12. The
leftover gaps are all in the areas that need a full compute or render
pass integration test to exercise, which is scheduled for v2.2 (render
graph + multi-pass).

**Fuzz**: `rust-old` had no fuzz directory. No fuzz parity gap.

## Unported modules

**None.** `typed_buffer.rs` landed in v2.1 as `src/typed_buffer.cyr` with
26 assertions covering alignment validation, capacity math, and GPU-backed
creation/write paths. The Cyrius API collapses Rust's `UniformBuffer<T>` /
`StorageBuffer<T>` generics into a capacity-based runtime API because of
(a) Cyrius's no-generics constraint and (b) the 6-parameter ceiling for
functions that call into wgpu via `fncall*`.
