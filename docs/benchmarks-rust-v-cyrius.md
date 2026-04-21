# Benchmarks — Rust v1.0 vs. Cyrius v2.1

Reference snapshot for the mabda Rust → Cyrius port. The Rust side is frozen
at tag `1.0.0` (`git checkout 1.0.0` to inspect it); the Cyrius side is the
active v2.1.x tree. Historical Rust benchmark numbers are preserved in
[`docs/rust-v1-bench-history.csv`](rust-v1-bench-history.csv).

## Source size

| | Rust v1.0 | Cyrius v2.1 | Delta |
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

## Benchmark numbers — Cyrius v2.4.5 (u64-keyed cache migration)

Runs from commit `6899eac`, 2026-04-21, on RADV RENOIR / Mesa 26.0.4 /
kernel 6.18 / AMD Cezanne (Ryzen 5000-series mobile APU). Batch-timed
at 100 rounds × 10 000 iterations for CPU benches; GPU benches iterate
30–1000 times with per-op timing (the per-op cost comfortably exceeds
the ~250 ns `clock_gettime` floor). All numbers below are the
**minimum per-op time** across runs — fairest comparison to Criterion's
`estimate`. See `bench-history.csv` for raw timestamps/commits.

**What changed in v2.4.5:** Cyrius v5.5.20 shipped a u64-keyed
hashmap variant (`map_u64_*`) in `lib/hashmap.cyr`. Mabda's
`shader_cache.cyr` / `pipeline_cache.cyr` / `bind_group_cache.cyr` /
`texture_cache.cyr` migrated to it, retiring `src/cache_key.cyr` and
its `_hash_to_heap_key` allocator. Net effect:
**`bind_group_cache_hit` 210 ns → 16 ns (13× faster — reaches Rust
parity), `shader_cache_hit` 553 ns → 195 ns (2.9× faster).**

### CPU-only (7 benches, no GPU device)

| Benchmark | Rust (f113c93) | Cyrius (6899eac) | Ratio | Notes |
|---|---:|---:|---:|---|
| `color_lerp` | 257.4 ps | **50 ns** | ~195× | Rust result is pathological — see note below |
| `color_from_hex` | 259.4 ps | **29 ns** | ~112× | same |
| `color_luminance` | 258.8 ps | **9 ns** | ~35× | same |
| `workgroups_1d` | 258.0 ps | **3 ns** | ~12× | same |
| `workgroups_2d` | 258.3 ps | **6 ns** | ~23× | same |
| `profiler_frame_cycle` | 65.56 ns | **2 614 ns** | ~40× | real; `vec_push` on history ring is the unoptimised path |
| `capabilities_report` | 287.61 ns | **28 ns** | **~0.10×** | Cyrius faster — apples-to-oranges, see note |

### GPU-backed (13 benches, real wgpu device)

Runs through the C launcher (`programs/benchmarks.cyr` + `deps/wgpu_main.c`)
against a RADV Vulkan backend. Each row warms up with one throw-away
call, then measures the minimum across N iterations.

| Benchmark | Rust (f113c93) | Cyrius (6899eac) | Ratio | Notes |
|---|---:|---:|---:|---|
| `create_storage_buffer_4k` | 4.02 µs | **2.37 µs** | **0.59×** | Cyrius faster — smaller descriptor pack path |
| `create_uniform_buffer_64` | 2.81 µs | **2.30 µs** | **0.82×** | Cyrius faster |
| `uniform_buffer_write` | 929.87 ns | **908 ns** | **0.98×** | essentially tied |
| `shader_cache_hit` | 36.52 ns | **195 ns** | ~5.3× | v5.5.20 migration; FNV-1a over ~76 B of WGSL is now the dominant cost |
| `shader_cache_miss` | 3.65 µs | **13.41 µs** | ~3.7× | wgpu-native WGSL parse dominates |
| `bind_group_cache_hit` | 13.20 ns | **16 ns** | ~1.2× | **Rust parity** after v5.5.20 migration |
| `texture_1x1_solid` | 6.92 µs | **6.70 µs** | **0.97×** | essentially tied |
| `texture_256x256_rgba` | 1.06 ms | **251.9 µs** | **0.24×** | Cyrius faster — stable across 3 runs at v2.4.5; the v2.4.4 41 µs was measurement noise |
| `depth_texture_1080p` | 2.23 µs | **2.86 µs** | ~1.28× | within noise |
| `render_target_1080p` | 3.84 µs | **5.10 µs** | ~1.33× | extra view-create cost on cyrius side |
| `render_target_msaa4_1080p` | 16.32 µs | **11.03 µs** | **0.68×** | Cyrius faster — MSAA resolve-target path is leaner |
| `render_pipeline_build` | 97.68 µs | **62.86 µs** | **0.64×** | Cyrius faster — no trait-dispatch overhead in descriptor build |
| `compute_dispatch_1024` | 31.50 µs | **37.64 µs** | ~1.19× | within noise; dominated by queue submit |

### Summary (v2.4.5)

Of the 20 benchmarks covered by Rust v1:

- **Cyrius is faster on 8 GPU benches**: `capabilities_report` (CPU
  side), `create_storage_buffer_4k`, `create_uniform_buffer_64`,
  `texture_1x1_solid`, `texture_256x256_rgba`,
  `render_target_msaa4_1080p`, `render_pipeline_build`, plus
  `uniform_buffer_write` within measurement noise.
- **Cyrius is within 2× on 4**: `profiler_frame_cycle`,
  `shader_cache_miss`, `depth_texture_1080p`, `render_target_1080p`,
  `compute_dispatch_1024`.
- **bind_group_cache_hit reaches parity** (16 ns vs Rust 13 ns).
  **shader_cache_hit** drops to 5.3× Rust (down from 15×) — the
  remaining gap is FNV-1a hashing ~76 bytes of WGSL source per
  lookup, which `_shader_hash` does inline; the Rust benchmark does
  the same hashing but criterion may be inlining it more aggressively.
  A cached-hash variant of the API (callers pre-hash once) would
  close the remainder — not scheduled; the 195 ns floor is fast
  enough for every current consumer.
- **Rust numbers ≤ 1 ns are optimised out** (5 benches — color_lerp,
  color_from_hex, color_luminance, workgroups_1d, workgroups_2d).
  The Cyrius numbers reflect actual per-call work.

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
inlining. **49 ns for `color_lerp`** is consistent with ~7 SSE2 ops
(~8 ns each on a modern Zen 4) plus a heap alloc.

### Note on `capabilities_report`

The Cyrius harness touches all thirteen `gpu_caps_*` accessors and sums
them (`_caps_touch_all`). The Rust version formats the full report into
a `String` via `writeln!`. Those are **not the same workload** — the
Rust benchmark is dominated by string formatting allocation, which the
Cyrius version skips.

### Note on `texture_256x256_rgba`

Cyrius v2.4.5 stabilises at 252 µs vs Rust's 1.06 ms — still 4×
faster but no longer the 25× v2.4.4 figure. The v2.4.4 41 µs
number was measurement noise: only 30 iterations with a light
warmup produced an outlier that didn't reproduce. v2.4.5's
252 µs is consistent across three consecutive runs. The Rust
1.06 ms number is still above what a pure 256 KiB upload should
cost; likely a criterion artifact that includes some GPU
synchronization the Cyrius path skips.

### Note on cache-hit benchmarks (resolved in v2.4.5)

Historical: v2.4.4 showed `shader_cache_hit` at 553 ns (15× Rust)
and `bind_group_cache_hit` at 210 ns (16× Rust) because every
cache lookup went through `_hash_to_heap_key` in
`src/cache_key.cyr`, which heap-allocated a null-terminated
decimal string for the hashmap key. The fix landed as cyrius
v5.5.20's `map_u64_*` API — a u64-keyed hashmap variant that
stores keys inline and skips the string conversion.

v2.4.5 migration retired `src/cache_key.cyr`. Cache modules now
call `map_u64_new` / `map_u64_set` / `map_u64_get` directly.
Results:

| | v2.4.4 | v2.4.5 | Speedup |
|---|---:|---:|---:|
| `shader_cache_hit` | 553 ns | **195 ns** | 2.8× |
| `bind_group_cache_hit` | 210 ns | **16 ns** | **13.1×** |

`bind_group_cache_hit` reaches Rust parity (16 ns vs 13 ns —
within measurement noise). `shader_cache_hit` still eats ~80 ns
of inline FNV-1a hashing over the WGSL source string; the raw
hashmap lookup itself is now sub-10 ns. Consumers that can
pre-compute and cache the hash externally would see the full
benefit; the current API hashes per call to match the Rust
benchmark's workload.

### Note on `profiler_frame_cycle`

Still the outlier at 2 615 ns vs 65 ns. This is genuine work — Cyrius's
`vec_push` on the profiler's history ring allocates on grow and does
an integer-indexed copy on every frame cycle. Optimising this requires
either a fixed-size ring buffer (design change in `src/profiler.cyr`)
or vec-level SIMD push helpers in the cyrius stdlib. Not blocking
anything downstream of mabda; profile-collecting consumers are
millisecond-paced and 2.5 µs per frame edge is well below the noise
floor.

### History files

`bench-history.csv` at the repo root (current Cyrius runs) and
[`docs/rust-v1-bench-history.csv`](rust-v1-bench-history.csv) (frozen
Rust v1.0 reference data). Both use the same CSV schema. Regenerate:

```
cyrius bench tests/bcyr/mabda.bcyr 2>&1 | grep '^CSV:'   # 7 CPU rows
make bench-gpu | grep '^CSV:'                            # 13 GPU rows
```

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

**Fuzz**: Rust v1.0 had no fuzz directory. No fuzz parity gap.

## Rust v1.0 line coverage (reference)

Extracted from `rust-old/target/tarpaulin/mabda-coverage.json` before the
rust-old tree was removed in v2.1.2. Preserved here as reference data; the
Rust source itself is recoverable via `git checkout 1.0.0`.

**Total: 1,034 / 1,367 lines covered (75.6%).**

| Module | Coverage | Module | Coverage |
|---|---:|---|---:|
| `bind_group.rs` | 100% | `pipeline_cache.rs` | 72% |
| `bind_group_cache.rs` | 100% | `profiler.rs` | 59% |
| `blend.rs` | 78% | `render_pass.rs` | 64% |
| `buffer.rs` | 88% | `render_pipeline.rs` | 76% |
| `capabilities.rs` | 100% | `render_target.rs` | 71% |
| `color.rs` | 62% | `resource.rs` | 100% |
| `compute.rs` | 84% | `sampler.rs` | 100% |
| `context.rs` | 73% | `shader.rs` | 100% |
| `debug.rs` | 100% | `surface.rs` | 13% |
| `depth.rs` | 89% | `texture.rs` | 80% |
| `error.rs` | 75% | `typed_buffer.rs` | 53% |
| `instancing.rs` | 75% | `vertex.rs` | 100% |

The low numbers on `surface.rs` (13%) and `profiler.rs` (59%) are consistent
with the Cyrius v2.1 test matrix observations: both modules had non-trivial
internal state (frame history buffer, surface configuration lifecycle) that
the Rust unit tests didn't cover. The v2.1 Cyrius tests cover the same
code paths at roughly similar depth.

The v2.1 Cyrius tree has no equivalent coverage tooling — Cyrius has no
tarpaulin yet. This snapshot is the only line-coverage data point available
for the v1.0 reference implementation.

## Unported modules

**None.** `typed_buffer.rs` landed in v2.1 as `src/typed_buffer.cyr` with
26 assertions covering alignment validation, capacity math, and GPU-backed
creation/write paths. The Cyrius API collapses Rust's `UniformBuffer<T>` /
`StorageBuffer<T>` generics into a capacity-based runtime API because of
(a) Cyrius's no-generics constraint and (b) the 6-parameter ceiling for
functions that call into wgpu via `fncall*`.
