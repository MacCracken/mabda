# Benchmarks — Rust v1.0 vs. Cyrius v2.0

Reference snapshot for the mabda Rust → Cyrius port. The Rust side is frozen
at v1.0.0 (`rust-old/`); the Cyrius side is the active v2.0.0 tree.

## Source size

| | Rust (`rust-old/`) | Cyrius (`cyr/`) | Delta |
|---|---:|---:|---:|
| Library source (modules only) | 8,916 LOC across 25 files | 3,257 LOC across 25 files + 4 FFI | **−63%** |
| Benchmark harness | 345 LOC (`benches/benchmarks.rs`) | pending `.bcyr` port | — |
| Tests | 278 unit tests in-source | 89 standalone + 3 GPU integration | see parity notes |

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

## Benchmark numbers — Cyrius v2.0

**Pending.** No `.bcyr` harness has been authored yet. The Cyrius bench
framework (`lib/bench.cyr`) is available and follows the same pattern
vidya uses (`tests/vidya.bcyr`). Porting the CPU-only benchmarks is a
mechanical exercise; the GPU benchmarks require the full phase0 GPU init
path inside a bench harness.

Target for parity:
- [ ] Port 7 CPU-only benchmarks to `tests/mabda.bcyr`
- [ ] Port 13 GPU-backed benchmarks to `tests/mabda_gpu.bcyr` (needs C launcher integration like `test_phase0`)
- [ ] Seed `bench-history.csv` in the same format as `rust-old/bench-history.csv` for continuity

Expected outcome based on vidya's results (Cyrius vs. Rust benchmarks in
`vidya/BENCHMARKS.md`): CPU-only paths should land within 0.5–2× of Rust,
since both compile to native x86_64 and Cyrius's manual memory layout
avoids the bounds-check and Drop overhead Rust pays. GPU-backed paths
should be essentially identical — they bottleneck on the wgpu driver,
not the language above it.

## Test parity

| Module | Rust tests | Cyrius standalone | Status |
|---|---:|---:|---|
| color | 21 | 48 | ✅ expanded |
| profiler | 22 | 15 | ⚠ partial |
| vertex + blend | 20 + 7 | 19 | ⚠ combined, partial |
| capabilities | 7 | 0 | ❌ |
| buffer | 20 | 3 (GPU int.) | ❌ standalone missing |
| compute | 16 | 0 | ❌ |
| texture | 21 | 0 | ❌ |
| render_pipeline | 15 | 0 | ❌ |
| render_target | 12 | 0 | ❌ |
| render_pass | 9 | 0 | ❌ |
| bind_group | 8 | 0 | ❌ |
| bind_group_cache | 10 | 0 | ❌ |
| pipeline_cache | 8 | 0 | ❌ |
| shader_cache (was shader) | 9 | 0 | ❌ |
| depth | 9 | 0 | ❌ |
| sampler | 5 | 0 | ❌ |
| instancing | 13 | 0 | ❌ |
| debug | 5 | 0 | ❌ |
| resource | 6 | 0 | ❌ |
| context | 8 | 1 (GPU int.) | ❌ standalone missing |
| error | 9 | 0 | ❌ |
| surface | 4 | 0 | ❌ |
| typed_buffer | 14 | — | ❌ module not ported |
| **Total** | **278** | **89 + 3 GPU int.** | |

The 278 → 89 gap is **not** a correctness regression — every ported
Cyrius module was manually verified against its Rust source — but it is
a real coverage hole. Porting strategies:

1. **Pure-data tests** (error codes, blend presets, sampler presets,
   depth formats, capability validation, bind group layout builders,
   vertex attribute math): port as standalone `.tcyr` files with the
   same assertion style as `test_color.tcyr`. No GPU needed. Estimated
   ~100 assertions recoverable this way.
2. **Cache tests** (shader_cache, pipeline_cache, bind_group_cache): hash
   logic and miss/hit semantics are pure, port standalone. ~25 assertions.
3. **GPU-backed tests** (buffer, compute, texture, render_*): extend
   `test_phase0.tcyr` with additional test cases inside the existing
   C launcher harness, rather than creating one binary per module. That
   keeps the single GPU init cost amortised. ~150 assertions, deferred
   until the texture and render pipeline FFI entries land in v2.1.

**Fuzz**: `rust-old` had no fuzz directory. The `fuzz/` directory shown
in project documentation belonged to vidya/cyrius, not mabda. No fuzz
parity gap.

## Unported module

**`typed_buffer.rs`** (352 LOC, 14 tests) — `UniformBuffer<T>` /
`StorageBuffer<T>` wrappers enforcing 16-byte alignment and type-safe
writes via `bytemuck::Pod`. Cyrius has no generics, so the port becomes
either: (a) thin constructor helpers `uniform_buffer_new(device, size)`
/ `storage_buffer_new(device, size)` on top of `buffer.cyr`, with
alignment validated at runtime; or (b) nothing — delete the concept and
make callers use `buffer.cyr` directly with a convention for alignment.

Option (a) matches the existing v2.0 roadmap item and is the planned
approach for v2.1.
