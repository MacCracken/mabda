# Mabda — Claude Code Instructions

## Project Identity

**Mabda** (مبدأ — Arabic: origin, principle, starting point) — GPU
foundation layer for AGNOS. Owns device lifecycle, buffers, compute
dispatch, textures, render pipelines, profiling, and capability
detection.

- **Type**: Cyrius library (include-chain) + dist bundle + dual-backend
  (wgpu C-launcher path + native AMD DRM-ioctl path)
- **License**: GPL-3.0-only
- **Language**: Cyrius 6.2.6+ (`cyrius.cyml: cyrius = "6.2.6"`)
- **Version**: 3.1.0 in tree. 3.0.0 GA shipped 2026-06-02 after the
  24-hour soak gate cleared (`--workload=all` ran 26 h 13 m, RSS flat,
  dmesg Δ = 0; see `docs/handoff/soak-20260601T222652Z/`). 3.0.x tracked
  the toolchain + AGNOS deps (3.0.1 → cyrius 6.0.43, 3.0.2 → 6.2.1,
  3.0.3 → 6.2.6 + samvada 0.4.1); 3.0.4 → P(-1) security-hardening patch
  (full-surface audit 2026-06-14). **3.1.0** → on-device mipmap
  generation: native AMD HW-verified on Cezanne; wgpu `generate` deferred
  (NOT_IMPLEMENTED, blocked on a wgpu compute path). Multi-queue is the
  v3.1.x arc's next feature (3.1.1+). v3.0 ships dual backend (wgpu +
  native AMD); native is `Backend`-slot-abstracted alongside wgpu, AMD
  only in v3.0; NVIDIA/Intel native scoped to v4.0/v5.0.
- **GPU FFI**: dual-path
  - **wgpu** — wgpu-native v29 C API via `deps/wgpu_main.c` launcher
    (65-slot fn table, 7 struct-packing shims)
  - **native AMD** — direct DRM ioctls via `syscall(SYS_IOCTL)`. No
    libdrm. Phase B = compute (PM4 + AMDGPU CS), Phase C = textures
    + render (clear-triangle composer, GFX-ring dispatch), Phase D =
    KMS surface (modeset, page-flip, present)
- **`samvada` dep** — sister AGNOS package (sibling of sakshi/patra/
  sigil) carrying the dbus client for logind master delegation.
  Wired via `[deps.samvada] tag = "0.4.1"`. Consumer programs link
  `samvada/deps/samvada_main.c` to populate the C-shim fn-table.
  v4.0 retires both `wgpu_main.c` and `samvada_main.c` together.

## Goal

One Cyrius library that answers "set up a GPU device, move bytes on
and off it, and draw / compute / present with them" for every AGNOS
downstream. v3.0 ships dual backend (wgpu and native AMD); v4.0
retires the wgpu path for AMD. Backend abstraction is the
load-bearing v3.0 architectural choice — the public API surface
doesn't change between paths.

## Current State (post 3.0.4, 2026-06-15)

- **Source**: 38 domain modules under `src/*.cyr`, ~12,500 lines (the
  former 137-KiB `backend_native.cyr` was split into four files at
  rc.2 — `_amdgpu.cyr` / `_shaders.cyr` / `_pm4.cyr` / `.cyr` —
  to land under cyrius lint/fmt's 128 KiB cap).
- **Tests**: **2076 CPU-only assertions** across **11 functionality-named
  domain files** under `tests/tcyr/` (reorganized 2026-06-15 from the old
  version-named mabda/mabda_v3/mabda_v3_phase_d trio — see the v3.1 test
  reorg). Each file is a standalone suite (own `main()` + `assert_summary`)
  mirroring the `src/` domains; `make test` globs them all:
  - `core` 145 (error/color/capabilities/profiler/resource/debug/obs)
  - `buffer` 53 · `compute` 26 · `texture` 41 · `graphics` 61
  - `render` 122 · `backend` 290 (abstraction + wgpu FFI + dispatch mocks)
  - `caches` 35 · `surface` 73 · `kms` 335
  - `native` 895 (amdgpu/PM4/GFX9 ISA/native textures+render)
  Splitting also kept every file under the 128 KiB lint/fmt cap (the old
  `mabda_v3.tcyr` had grown to 148 KiB, past the cap).
  Plus GPU integration programs: `phase0`, `compute_e2e`,
  `render_e2e`, `render_graph_e2e` for wgpu; `native_compute_store`,
  `native_texture_e2e`, `native_render_e2e`, `native_mipmap_e2e`
  (v3.1 — mip-chain generate verified vs a CPU box-filter on Cezanne)
  for native; plus `native_kms_summary`, `native_kms_modeset_smoke`,
  `native_present_e2e` for Phase D.
- **Benchmarks**: `tests/bcyr/mabda.bcyr` — 9 CPU benches. GPU
  benches via `make bench-gpu` (13 benches, Rust v1 parity set on
  the wgpu path). Reference Rust numbers in
  `docs/benchmarks-rust-v-cyrius.md`. Native-on-AMD bench cell
  is Tier 3 work pending a consumer flip.
- **Dist bundle**: `dist/mabda.cyr` — ~12,500 lines.
  `cyrius distlib` regenerates it.
- **Integration**: consumed by soorat, rasa, ranga, bijli, aethersafta,
  kiran (via soorat). Six-consumer regression sweep is Tier 2 ship
  work.

## Why v3.0 ships dual

- **Backend abstraction layer** — `Backend` struct (208 bytes,
  25 fnptr slots) + `backend_wgpu_new` / `backend_native_new`
  fillers. Public API (`gpu_buffer_*` / `gpu_compute_dispatch` /
  `gpu_texture_*` / `gpu_render_*` / `gpu_surface_*`) routes through
  `ctx->backend->slot` via fncall1/2/3/5 — same call-site shape on
  both backends, the slot impls differ.
- **WGSL → GFX9 lowering deferred to v3.x.** The earlier "v3.0
  ship-blocker" framing was wrong. `gpu_shader_module_create` is
  byte-polymorphic at the backend boundary — wgpu reads bytes as
  WGSL UTF-8, native reads as pre-compiled GFX9 ISA. Consumers
  ship two-form bundles in v3.0; in-mabda WGSL frontend is a v3.x
  pure-Cyrius project. See
  `docs/proposals/v3-wgsl-frontend-choice.md`.
- **Logind master gate solved by samvada.** `gpu_surface_configure_native_logind`
  routes through `samvada_session_take_device` for the master fd;
  `gpu_surface_configure_native_kiosk` is the alternate path where
  the caller manages master themselves.

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
- **`samvada` (AGNOS dep)** — Cyrius dbus client for logind master
  delegation. Pinned via `[deps.samvada] tag = "0.4.1"` in
  `cyrius.cyml`. mabda doesn't link libsystemd directly — consumer
  programs link `samvada/deps/samvada_main.c` which calls into
  `samvada_main(table)` to populate the static fn-table that
  mabda's `_native_logind` slot reads from.
- **wgpu-native v29** — external C library, downloaded by consumers
  alongside their `deps/wgpu_main.c` launcher. Not a Cyrius dep.
- **libsystemd** — needed by samvada's C shim (`samvada_main.c`).
  Consumer-provided link, not a mabda direct dep. Drops at v4.0
  alongside wgpu-native.

All Cyrius deps are pinned in `cyrius.cyml`. `cyrius deps` resolves
them against the installed toolchain.

### Dependency wiring (HARD RULE)

`lib/` is a **real directory** populated by `cyrius deps` — it contains
per-module copies of the stdlib files declared in `[deps].stdlib`, plus
symlinks into `~/.cyrius/deps/<pkg>/<ver>/dist/` for bundled deps
(`mabda.cyr`, `patra.cyr`, `sakshi.cyr`, `sigil.cyr`, etc.). It is
gitignored (`/lib/` in `.gitignore`) — a build artifact, not source.

**NEVER** replace `lib/` with a symlink to a cyrius checkout (e.g.
`ln -s /home/macro/Repos/cyrius/lib lib`) or to `~/.cyrius/lib`. The
repo previously shipped exactly that symlink and it caused a recurring
corruption bug: any agent working in this repo (formatting, linting,
refactoring, dead-code cleanup) that wrote to `lib/<anything>.cyr`
actually wrote through the symlink into the **cyrius** repo. `mabda`
has no visibility into who else `include`s those files, so a dead-code
pass against `lib/dynlib.cyr` would silently delete fns that
`cyrius/lib/fdlopen.cyr` depends on. CI then broke in the cyrius repo,
not mabda's — making the root cause invisible.

Legitimate setup — both CI and local dev — is:

```
rm -rf lib && mkdir lib && cyrius deps
```

Never edit `lib/*.cyr` by hand. If the stdlib needs a fix, fix it in
the `cyrius` repo, cut a release, bump `cyrius = "x.y.z"` in
`cyrius.cyml`, re-run `cyrius deps`.

## Quick Start

```bash
cyrius deps                                          # resolve stdlib + samvada into lib/
cyrius build programs/smoke.cyr build/mabda_smoke    # link-check
make test                                            # 2076 CPU assertions across 11 domain files
cyrius bench tests/bcyr/mabda.bcyr                   # 9 CPU benchmarks
cyrius distlib                                       # → dist/mabda.cyr
make test-gpu                                        # wgpu integration programs (needs wgpu-native)
make test-native-compute-store                       # native compute (needs amdgpu)
make test-native-render-e2e                          # native render (HW-gated; cache-flush in tree)
make test-native-kms-summary                         # KMS topology probe (works in any session)
make test-native-kms-modeset                         # native modeset (HW + DRM master)
make test-native-present-e2e                         # 120-frame animated present (HW + master)
make bench-gpu                                       # 13 GPU benchmarks (wgpu only today)
```

**Before tripping toolchain wires**: read
[`docs/development/2026-04-30-toolchain-issues.md`](docs/development/2026-04-30-toolchain-issues.md) —
consolidated cheat-sheet of cyrius lint/fmt 128 KiB cap, fncall6
ABI bug, `var X;` rejection, global init order, logical right shift,
bump allocator exhaustion in tests. Cross-references the deeper
`docs/development/issues/` filings + memory notes.

## Architecture (flat — matches yukti / vidya)

```
mabda/
├── src/                 38 GPU library modules — flat, zero transitive includes
│   ├── lib.cyr                      — single include chain (stdlib + domain modules + samvada)
│   ├── error.cyr                    — GpuErr codes + Result helpers
│   ├── color.cyr                    — f64-backed RGBA colour type
│   ├── capabilities.cyr             — WebGPU limit detection
│   ├── profiler.cyr                 — CPU-side frame timing, EMA, history
│   ├── resource.cyr                 — RAII-ish buffer/texture lifetime tracker
│   ├── wgpu_types.cyr               — @internal: usage/format/shader-stage enums
│   ├── wgpu_descriptors.cyr         — @internal: packed descriptor builders
│   ├── wgpu_ffi.cyr                 — @internal: 65-entry wgpu fn-pointer table
│   ├── backend.cyr                  — @internal: Backend struct (208 B, 25 slots)
│   │                                  + BACKEND_KIND_* + null-slot helpers
│   ├── backend_wgpu.cyr             — @internal: wgpu fillers for all 25 slots
│   ├── context.cyr                  — GpuContext (120 B; dual-interpretation
│   │                                  +0..+24 + backend ptr + native cache + surface stash
│   │                                  + PM4 scratch slot)
│   ├── backend_native_amdgpu.cyr    — @internal: DRM/AMDGPU/GEM/syncobj/CS-submit
│   │                                  ioctl wrappers (foundational layer, no PM4 deps)
│   ├── backend_native_shaders.cyr   — @internal: GFX9 ISA shader builders + GFX9
│   │                                  graphics register addresses + value minimums
│   ├── backend_native_pm4.cyr       — @internal: PM4 packet primitives + compute +
│   │                                  render PM4 stream composers (pure byte builders)
│   ├── backend_native.cyr           — @internal: native AMD slot fillers + dispatch
│   │                                  drivers + native_texture/_rt/_render_pipeline +
│   │                                  ctx accessors + backend_native_new()
│   ├── backend_native_kms.cyr       — @internal: KMS surface ioctls (modeset/page-flip/PRIME)
│   ├── buffer.cyr                   — public gpu_buffer_* dispatch through ctx->backend
│   ├── typed_buffer.cyr             — uniform/storage buffer metadata
│   ├── gpu_timestamps.cyr           — TIMESTAMP_QUERY feature wiring
│   ├── compute.cyr                  — public gpu_compute_dispatch + ping-pong helpers
│   ├── shader_cache.cyr             — WGSL source → module cache (u64-keyed, v2.4.5)
│   ├── pipeline_cache.cyr           — u64 hash → pipeline cache
│   ├── bind_group_cache.cyr         — u64 hash → bind group cache
│   ├── vertex.cyr                   — Vertex2D / 3D layouts
│   ├── blend.cyr                    — BLEND_* constants
│   ├── sampler.cyr                  — sampler descriptor builder
│   ├── depth.cyr                    — depth-stencil state + depth-texture helpers
│   ├── bind_group.cyr               — BGL builder
│   ├── texture.cyr                  — public gpu_texture_* dispatch through ctx->backend
│   ├── render_target.cyr            — public gpu_render_target_* + RenderTargetBuilder
│   ├── render_pipeline.cyr          — public gpu_render_pipeline_* + builder + create_simple
│   ├── render_pass.cyr              — public gpu_render_pass_* + RenderPassBuilder
│   ├── render_graph.cyr             — DAG pass orchestration (v2.5.0)
│   ├── surface.cyr                  — v2 wgpu surface_state_* lifecycle
│   ├── surface_v3.cyr               — public gpu_surface_* dispatchers (Step 7.7):
│   │                                  configure_wgpu / _native_kiosk / _native_logind +
│   │                                  acquire / present / release
│   ├── instancing.cyr               — instance buffer + identity helpers
│   └── debug.cyr                    — push/pop debug markers
├── tests/
│   ├── tcyr/                        — 11 functionality-named domain suites
│   │   │                              (2076 asserts total; `make test` globs
│   │   │                              `tests/tcyr/*.tcyr`). Each standalone
│   │   │                              (own main + assert_summary), self-
│   │   │                              contained (needed mocks inlined).
│   │   ├── core.tcyr  buffer.tcyr  compute.tcyr  texture.tcyr
│   │   ├── graphics.tcyr  render.tcyr  backend.tcyr  caches.tcyr
│   │   └── surface.tcyr  native.tcyr  kms.tcyr
│   └── bcyr/mabda.bcyr              — CPU benchmark harness (9 benches)
├── programs/
│   ├── smoke.cyr                    — link-check for the full include chain
│   ├── phase0.cyr                   — wgpu GPU smoke (buffer/texture/pipeline)
│   ├── compute_e2e.cyr              — wgpu compute dispatch round-trip
│   ├── render_e2e.cyr               — wgpu render pass clear + pixel verify
│   ├── render_graph_e2e.cyr         — wgpu 3-node DAG (compute → render → copy)
│   ├── benchmarks.cyr               — 13 GPU benches, Rust-v1 parity (wgpu)
│   ├── native_compute_store.cyr     — native compute, write 0xDEADBEEF + readback
│   ├── native_texture_e2e.cyr       — native texture round-trip
│   ├── native_render_e2e.cyr        — native render: clear-triangle + pixel verify
│   ├── native_kms_summary.cyr       — KMS topology probe (Phase D, no-master)
│   ├── native_kms_modeset_smoke.cyr — native modeset visual smoke (red screen)
│   └── native_present_e2e.cyr       — 7.7 e2e: 120-frame animated gradient
├── dist/mabda.cyr                   — bundle for [deps.mabda] consumers
├── deps/
│   ├── wgpu_main.c                  — C launcher: wgpu fn table + struct-packing shims
│   └── wgpu-native/                 — external C binaries (gitignored)
├── lib/                             — populated by `cyrius deps` (gitignored):
│   ├── string.cyr / fmt.cyr / ...   — Cyrius stdlib copies
│   └── samvada.cyr                  — symlink into ~/.cyrius/deps/samvada/0.4.1/dist/
├── cyrius.cyml                      — package manifest + [lib] + [deps] + [deps.samvada]
├── Makefile                         — wrapper over `cyrius` CLI + GPU paths + native programs
└── VERSION                          — source of truth, templated into manifest
```

## FFI Architecture

mabda has two GPU paths and one auxiliary dbus path; each uses the
fn-table-via-C-shim pattern.

### wgpu path (`deps/wgpu_main.c`)

1. C `main()` calls `_cyrius_init()` then `alloc_init()`
2. C pre-initializes GPU (instance/adapter/device/queue — Vulkan-only
   via `WGPUInstanceExtras { backends = Vulkan }`; see v2.4.2 CHANGELOG
   for why the default `All` was a problem on headless boxes)
3. C builds the function-pointer table (65 wgpu functions +
   7 struct-packing shims)
4. C calls `mabda_main(fn_table_ptr, preinit_ptr)` which the consumer defines
5. Cyrius calls wgpu via `fncall1`/`fncall2`/`fncall5` and struct-packed
   shims — **never** `fncall6` with a struct-by-value arg

### Native AMD path (no C shim)

Compute / render / surface ioctls go directly through
`syscall(SYS_IOCTL)` — no C library, no libdrm. Pure Cyrius.
Mappings:

- **`/dev/dri/renderD128`** for compute + render allocator (BO
  create / mmap / GEM close / VA map). AMDGPU-specific ioctls
  (GEM_CREATE / CTX / BO_LIST / GEM_VA / CS) routed through fd
  with no master required.
- **`/dev/dri/cardN`** for KMS surface (MODE_GETRESOURCES /
  MODE_GETCONNECTOR / MODE_GETENCODER / ADDFB2 / SETCRTC /
  PAGE_FLIP). Requires DRM master — see `samvada` path for the
  in-session master story.
- **PRIME bridge** between the two fds for the surface FB story
  (see `phase_d_prime_cross_fd_handle_bridge` vidya entry).

### samvada path (`samvada/deps/samvada_main.c`)

Consumer programs that use `gpu_surface_configure_native_logind`
link `samvada/deps/samvada_main.c` alongside their wgpu launcher.
Same fn-table pattern:

1. C `main()` builds the samvada 9-slot table (sd_bus_*) and calls
   `samvada_main(table)` to populate samvada's static reference.
2. Cyrius calls `samvada_session_take_device(major, minor)` etc.
   via fncall through the table.
3. mabda's `_backend_native_surface_configure_logind` slot reads
   the master fd back from samvada and stashes on
   `gpu_ctx_native_card_fd` for the slot dispatch.

### CPU testing (no GPU, no master, no dbus)

`cyrius test` runs the three `tests/tcyr/*.tcyr` files against
`src/lib.cyr` — no wgpu-native, no amdgpu hardware, no libsystemd
needed. Backend-abstraction routing exercised via mock-fnptr
sentinels; native ioctls / wgpu calls / sd_bus calls all surface
as null-safety + struct-shape tests at the Cyrius layer. HW gates
live in the `programs/native_*.cyr` programs.

## Key Constraints

- **Tests are the way** — 2076 CPU assertions across 11 domain test
  files + a dozen GPU/HW programs. Every new code path adds an
  assertion. Stack-local `var ctx[112]` for test-scoped buffers
  (heap-allocated tests exhaust the bump allocator — see
  `bump_allocator_exhaustion_in_tests` vidya entry).
- **Own the stack** — every external dep is either an AGNOS package
  (samvada, sakshi, patra, sigil) or a consumer-provided C library
  (wgpu-native, libsystemd-via-samvada). wgpu-native and libsystemd
  both retire at v4.0; the trajectory is pure-Cyrius all the way
  down.
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
  `fncall6_ceiling_into_extern_c` vidya entry.
- **`var X = expr;` initialization required.** Cyrius rejects bare
  `var X;` declarations — every var needs an initializer. Use
  `var X = 0;` for "to-be-set-later" pattern.
- **`gpu_shader_module_create` is byte-polymorphic.** wgpu reads as
  WGSL, native reads as pre-compiled GFX9 ISA. v3.0 ships consumer
  two-form bundles; in-mabda WGSL → GFX9 lowering is v3.x scope.
- **Phase D ioctl ordering matters.** Discovery → mode-pick → encoder
  → CRTC → AddFB2 → SETCRTC → PAGE_FLIP. Skipping discovery and
  hardcoding IDs breaks across reboots. See
  `phase_d_kms_sequencing` vidya entry.

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, audit history — know what was intended
1. Cleanliness: `cyrius build programs/smoke.cyr` (0 warnings),
   per-file `cyrius lint src/*.cyr` (0 warnings; the bare repo-wide
   form was removed in 5.7.x — see
   `feedback_cyrius_lint_fmt_per_file` memory),
   `cyrius vet programs/smoke.cyr` clean
2. Test sweep: 2076+ assertions pass across all 11 domain test files,
   `cyrius distlib` diff-clean
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
2. Cleanliness check — `make test` (globs all `tests/tcyr/*.tcyr`)
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

1. Full CPU suite — `make test` runs all three files
   (all 11 `tests/tcyr/*.tcyr` domain suites); 2076+
   asserts pass.
2. Bench baseline — `cyrius bench tests/bcyr/mabda.bcyr`
3. GPU integration (wgpu) — `make test-phase0` passes on a box with
   wgpu-native
4. GPU integration (native) — `make test-native-compute-store`
   passes on a box with amdgpu (HW-gated; requires AMD render node).
   `make test-native-render-e2e` and Phase D programs run from a
   tty / kiosk session OR with samvada+logind wired through a
   consumer.
5. `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean
6. Version consistency — `./scripts/version-check.sh` passes
7. Consumer check — soorat, rasa, ranga, bijli, aethersafta still build
   against the new bundle (Tier 2 ship work)
8. Audit index up to date — `docs/audit/` has the current
   `YYYY-MM-DD-audit.md` referenced from CHANGELOG

## CI / Release

- **Toolchain pin**: `cyrius = "6.2.6"` in `cyrius.cyml`. CI + release
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
- **Test/bench**: `make test` (globs all `tests/tcyr/*.tcyr` domain suites
  files) + `cyrius bench tests/bcyr/mabda.bcyr`.
- **GPU integration is local only** — CI runners don't have
  wgpu-native or amdgpu hardware; `make test-phase0` /
  `make test-native-*` are developer gates.

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
