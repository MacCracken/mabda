# Mabda — Claude Code Instructions

## Project Identity

**Mabda** (مبدأ — Arabic: origin, principle, starting point) — GPU
foundation layer for AGNOS. Owns device lifecycle, buffers, compute
dispatch, textures, render pipelines, profiling, and capability
detection.

- **Type**: Cyrius library (include-chain) + dist bundle + C launcher
- **License**: GPL-3.0-only
- **Language**: Cyrius 5.4.7+ (`cyrius.cyml: cyrius = "5.4.7"`)
- **Version**: 2.4.1 — shipping as `lib/mabda.cyr` in the Cyrius stdlib
- **GPU FFI**: wgpu-native v29 C API via `deps/wgpu_main.c` launcher

## Goal

One Cyrius library that answers "set up a GPU device, move bytes on
and off it, and draw / compute with them" for every AGNOS downstream.
Portable enough that swapping wgpu-native for a native AGNOS driver in
v3.x is an implementation detail, not an API break.

## Current State

- **Source**: 29 domain modules under `src/*.cyr`, ~4,000 lines total.
- **Tests**: 309 CPU-only assertions in `tests/tcyr/mabda.tcyr` plus a
  GPU integration test (`programs/phase0.cyr`) driven through the C
  launcher.
- **Benchmarks**: `tests/bcyr/mabda.bcyr` — color, workgroup math,
  profiler, capability report. Reference Rust numbers in
  `docs/benchmarks-rust-v-cyrius.md`.
- **Dist bundle**: `dist/mabda.cyr` — 4,025 lines, ~142 KB.
  `cyrius distlib` regenerates it.
- **Integration**: consumed by soorat, rasa, ranga, bijli, aethersafta,
  kiran (via soorat).

## Consumers

| Project      | Usage                                       |
|--------------|---------------------------------------------|
| soorat       | Renderer — textures, pipelines, render pass |
| rasa         | Image editor — compute shaders, textures    |
| ranga        | Image processing — buffers, compute         |
| bijli        | EM simulation — compute, storage buffers    |
| aethersafta  | Desktop compositor — surfaces, present      |
| kiran        | Game engine (via soorat)                    |

## Dependencies

- **Cyrius stdlib** — `string`, `fmt`, `alloc`, `vec`, `str`, `io`,
  `args`, `hashmap`, `syscalls`, `tagged`, `fnptr`, `mmap`, `dynlib`,
  `sakshi` (ships with Cyrius >= 5.4.7)
- **wgpu-native v29** — external C library, downloaded by consumers
  alongside their `deps/wgpu_main.c` launcher. Not a Cyrius dep.

All Cyrius deps are pinned in `cyrius.cyml`. `cyrius deps` resolves
them against the installed toolchain.

## Quick Start

```bash
cyrius deps                              # resolve stdlib into lib/
cyrius build programs/smoke.cyr build/mabda_smoke   # link-check
cyrius test tests/tcyr/mabda.tcyr        # 309 CPU assertions
cyrius bench tests/bcyr/mabda.bcyr       # CPU benchmarks
cyrius distlib                           # → dist/mabda.cyr
make test-phase0                         # GPU integration (needs wgpu-native)
```

## Architecture (flat — matches yukti / vidya)

```
mabda/
├── src/                 29 GPU library modules — flat, zero transitive includes
│   ├── lib.cyr            — the single include chain (stdlib + domain modules)
│   ├── error.cyr          — GpuErr codes + Result helpers
│   ├── color.cyr          — f64-backed RGBA colour type
│   ├── capabilities.cyr   — WebGPU limit detection
│   ├── profiler.cyr       — CPU-side frame timing, EMA, history
│   ├── resource.cyr       — RAII-ish buffer/texture lifetime tracker
│   ├── wgpu_types.cyr     — usage/format/shader-stage enums
│   ├── wgpu_descriptors.cyr — packed descriptor builders
│   ├── wgpu_ffi.cyr       — 40-entry function-pointer table
│   ├── context.cyr        — GpuContext (instance/adapter/device/queue)
│   ├── buffer.cyr         — low-level buffer helpers
│   ├── typed_buffer.cyr   — uniform/storage buffer metadata
│   ├── gpu_timestamps.cyr — TIMESTAMP_QUERY feature wiring
│   ├── compute.cyr        — compute pipeline + ping-pong helpers
│   ├── cache_key.cyr      — shared hash helper
│   ├── shader_cache.cyr   — WGSL source → module cache
│   ├── pipeline_cache.cyr — (pipeline layout, entry) → pipeline cache
│   ├── bind_group_cache.cyr — (layout, resource-id tuple) → bind group
│   ├── vertex.cyr         — Vertex2D / 3D layouts
│   ├── blend.cyr          — BLEND_* constants
│   ├── sampler.cyr        — sampler descriptor builder
│   ├── depth.cyr          — depth-stencil state + depth-texture helpers
│   ├── bind_group.cyr     — BGL builder
│   ├── texture.cyr        — RGBA8 helpers, create_from_rgba, cache
│   ├── render_target.cyr  — RenderTarget + builder
│   ├── render_pipeline.cyr — pipeline builder, create_simple, draw helpers
│   ├── render_pass.cyr    — RenderPass builder
│   ├── surface.cyr        — surface configuration + acquire/present
│   ├── instancing.cyr     — instance buffer + identity helpers
│   └── debug.cyr          — push/pop debug markers
├── tests/
│   ├── tcyr/mabda.tcyr    — consolidated CPU-only suite (273 assertions)
│   └── bcyr/mabda.bcyr    — CPU-only benchmark harness
├── programs/
│   ├── smoke.cyr          — link-check for the full include chain
│   └── phase0.cyr         — GPU integration test (needs C launcher)
├── dist/mabda.cyr         — bundle for `[deps.mabda]` consumers
├── deps/
│   ├── wgpu_main.c        — C launcher: fn table + struct-packing shims
│   └── wgpu-native/       — external C binaries (gitignored)
├── cyrius.cyml            — package manifest + [lib] + [deps]
├── Makefile               — thin wrapper over `cyrius` CLI + GPU path
└── VERSION                — source of truth, templated into manifest
```

## FFI Architecture

GPU programs go through the C launcher (`deps/wgpu_main.c`):

1. C `main()` calls `_cyrius_init()` then `alloc_init()`
2. C pre-initializes GPU (instance/adapter/device/queue)
3. C builds the function-pointer table (40 wgpu functions + struct-packing shims)
4. C calls `mabda_main(fn_table_ptr, preinit_ptr)` which the consumer defines
5. Cyrius calls wgpu via `fncall2` / `fncall5` — **never** `fncall6` directly

For standalone library testing (no GPU), `cyrius test` runs
`tests/tcyr/mabda.tcyr` against `src/lib.cyr` — no wgpu-native needed.

## Key Constraints

- **Tests are the way** — 309 CPU assertions + 1 GPU integration test,
  all passing. Every new code path adds an assertion.
- **Own the stack** — if AGNOS wraps something external, depend on the
  AGNOS crate (sakshi, yukti, patra). wgpu-native is the sole C dep
  and it is consumer-provided, not vendored.
- **No magic** — every operation measurable, auditable, traceable.
- **Manual memory** — `alloc / store64 / load64`. Every struct has a
  header comment block with field offsets.
- **Tagged unions for errors** — `Ok(value)` / `Err(gpu_err(...))` via
  `lib/tagged.cyr`.
- **f64 internally, f32 at the GPU boundary** — use `f64_to_f32` only
  when writing to a GPU buffer.
- **Prefix private helpers with `_`** — public API uses descriptive names.
- **Struct-pack wgpu args with 6+ parameters.** Cyrius `fncall6` +
  wgpu-native segfaults. Wrap via a C shim in `deps/wgpu_main.c`
  that takes `(handle, struct_ptr)` and call via `fncall2` — see
  `wgpu_command_encoder_copy_buffer_to_buffer` and `wgpu_buffer_map_sync`
  for the canonical pattern.
- **6-parameter ceiling for Cyrius fns that fncall into wgpu.** Pure
  Cyrius functions can take 12+ args without issue, but the moment one
  internally `fncall*`s into wgpu-native, any signature with 7+ params
  reliably segfaults. Fold into a struct pointer or split. See
  `feedback_cyrius_param_ceiling.md`.

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, audit history — know what was intended
1. Cleanliness: `cyrius build programs/smoke.cyr` (0 warnings),
   `cyrius lint` (0 warnings), `cyrius fmt --check` diff-clean,
   `cyrius vet programs/smoke.cyr` clean
2. Test sweep: 286+ assertions pass, `cyrius distlib` diff-clean
3. Benchmark baseline: `cyrius bench tests/bcyr/mabda.bcyr`, save CSV
4. Internal deep review — gaps, optimizations, correctness, docs
5. External research — wgpu-native / WebGPU / GPU-driver CVE sweep
   since last pass
6. Security audit (see below) — file findings in
   `docs/audit/YYYY-MM-DD-audit.md`
7. Additional tests from findings — each HIGH/MED fix lands with an
   assertion that would have caught the original bug
8. Post-review benchmarks — prove the wins (if any)
9. Documentation audit — CLAUDE.md, roadmap, CHANGELOG, audit index
10. Repeat if heavy

### Work Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check — `cyrius test tests/tcyr/mabda.tcyr`
3. Test + benchmark additions for new code
4. Internal review — performance, memory, correctness
5. If any FFI / buffer / texture math changed: re-run the audit
   checklist against the diff
6. Documentation — update CHANGELOG, roadmap, docs
7. Version check — `./scripts/version-check.sh` passes
8. Return to step 1

### Security Hardening (before release)

1. **Input validation** — every function accepting consumer-supplied
   data (buffer sizes, texture dimensions, workgroup counts,
   descriptor fields, label strings) validates bounds, types, ranges
   before use
2. **Buffer safety** — every `var buf[N]` and `alloc(N)` verified:
   N in bytes, max offset < N, no adjacent-allocation overflow. The
   struct header comment's byte count must match the actual `alloc`
3. **Integer overflow** — any `a * b` / `a + b` / `a << n` on sizes
   or dimensions gets an overflow guard before use, especially in
   texture / buffer / workgroup math
4. **Divide-by-zero** — any `/` or `%` verifies the divisor is
   non-zero before the operation (workgroup helpers were the
   regression case in 2.3.0)
5. **Syscall return handling** — every `syscall()` return value is
   checked; error paths either recover or deterministically zero
   any output buffer the caller will read
6. **Pointer validation** — no raw deref of consumer-supplied
   pointers; label strings use `wgpu_string_view_len` with an
   explicit length when length is known
7. **FFI descriptor offset review** — every edit to
   `wgpu_descriptors.cyr` cross-referenced against the v29
   `webgpu.h` layout; field offsets noted in the module header
   comment block
8. **`fncall6` avoidance** — any wgpu-native call taking 6+ i64
   arguments goes through a struct-packing shim in
   `deps/wgpu_main.c`; direct `fncall6` reliably crashes against
   wgpu-native (see `feedback_fncall6_wgpu` memory)
9. **Known CVE check** — review against current wgpu-native /
   WebGPU / GPU-driver CVEs since the prior audit
10. **File findings** — `docs/audit/YYYY-MM-DD-audit.md` with
    severity, file, line, class, mitigation

Severity levels: **CRITICAL** (exploitable immediately) / **HIGH**
(moderate effort) / **MEDIUM** (specific conditions) / **LOW**
(defense-in-depth).

### Task Sizing

- **Low/Medium effort**: batch freely
- **Large effort**: small bites — break into sub-tasks, verify each
- **If unsure**: treat as large

### Closeout Pass (before every minor/major bump)

1. Full CPU suite — `cyrius test tests/tcyr/mabda.tcyr` passes
2. Bench baseline — `cyrius bench tests/bcyr/mabda.bcyr`
3. GPU integration — `make test-phase0` passes on a box with wgpu-native
4. `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean
5. Version consistency — `./scripts/version-check.sh` passes
6. Consumer check — soorat, rasa, ranga, bijli, aethersafta still build
   against the new bundle
7. Audit index up to date — `docs/audit/` has the current
   `YYYY-MM-DD-audit.md` referenced from CHANGELOG

## CI / Release

- **Toolchain pin**: `cyrius = "5.4.7"` in `cyrius.cyml`. CI + release
  both read from the manifest — no hardcoded versions in YAML.
- **Tag filter**: release workflow triggers on `v[0-9]+.[0-9]+.[0-9]+`
  and `[0-9]+.[0-9]+.[0-9]+`. Version-verify step asserts
  `VERSION == git tag`.
- **Lint/fmt/vet gates**: CI fails on any `cyrius lint` warning,
  `cyrius fmt --check` drift, or `cyrius vet` finding.
- **Dist gate**: CI runs `cyrius distlib` and fails if
  `dist/mabda.cyr` drifts from the committed copy.
- **Smoke build**: `cyrius build programs/smoke.cyr` — proves the
  full include chain links.
- **Test/bench**: `cyrius test tests/tcyr/mabda.tcyr` + `cyrius bench
  tests/bcyr/mabda.bcyr`.
- **GPU integration is local only** — CI runners don't have
  wgpu-native; `make test-phase0` is a developer gate.

## CHANGELOG Format

```markdown
## [X.Y.Z] — YYYY-MM-DD
### Added — new features
### Changed — changes to existing features
### Fixed — bug fixes
### Breaking — breaking changes with migration guide
```

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies (wgpu-native is the exception,
  consumer-provided)
- Do not skip `cyrius test` before claiming changes work
- Do not commit `build/`, `deps/wgpu-native/`, or `deps/*.o`
- Do not call wgpu-native functions with 6+ i64 args via `fncall6` —
  always go through a struct-packing shim in `deps/wgpu_main.c`
- Do not add Cyrius stdlib includes in individual `src/*.cyr` files —
  `src/lib.cyr` owns the whole include chain
- Do not hardcode Cyrius toolchain versions in CI YAML — read
  `cyrius.cyml`
- Do not shell out to `cc5` directly for library code — go through
  `cyrius <subcommand>`. The one exception is `programs/phase0.cyr`,
  which the Makefile compiles with `printf 'object;\n' | cc5` because
  it needs to be linked against the C launcher.
