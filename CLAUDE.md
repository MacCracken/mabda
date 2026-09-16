# Mabda — Claude Code Instructions

## Project Identity

**Mabda** (مبدأ — Arabic: origin, principle, starting point) — GPU
foundation layer for AGNOS. Owns device lifecycle, buffers, compute
dispatch, textures, render pipelines, profiling, and capability
detection.

- **Type**: Cyrius library (include-chain) + dist bundle + dual-backend
  (wgpu C-launcher path + native AMD DRM-ioctl path)
- **License**: GPL-3.0-only
- **Language**: Cyrius — version pinned in `cyrius.cyml` (`cyrius = "x.y.z"`)
- **Version**: `VERSION` file is source of truth, templated into manifest

## Goal

One Cyrius library that answers "set up a GPU device, move bytes on and
off it, and draw / compute / present with them" for every AGNOS
downstream. v3.0 ships dual backend (wgpu and native AMD); the AMD wgpu
path is deprecated at v4.0.1 (warns + still works by default,
`-D MABDA_AMD_WGPU_STRICT` enforces native-only; retirement deferred).
v4.0 added NVIDIA native. Backend abstraction is the load-bearing v3.0
architectural choice — the public API surface doesn't change between
paths.

## Pointers (where things live)

- **Version history / what shipped when** — `CHANGELOG.md`
- **Architecture overview + file layout** — `docs/architecture/overview.md`
- **Roadmap** — `docs/development/roadmap.md`
- **Toolchain quirks cheat-sheet** — `docs/development/2026-04-30-toolchain-issues.md`
  (Class A 128 KiB lint/fmt cap — RESOLVED at cyrius 6.2.20; ⚠ since 6.5.28
  `cyrius fmt <file>` REWRITES IN PLACE and `--dry`/`--check` print a
  diagnostic rather than source, so `cyrius fmt f > f` now DESTROYS f; the
  `fncall6`-into-extern-C `%fs`/TLS
  gotcha — a misdiagnosis resolved in 6.3.26, `var X;` rejection,
  global init order, logical right shift, bump allocator exhaustion in
  tests — read before tripping a toolchain wire)
- **Security audits** — `docs/audit/YYYY-MM-DD-audit.md`
- **Proposals / design notes** — `docs/proposals/`
- **Deeper issue filings** — `docs/development/issues/`
- **Benchmark history** — `docs/benchmarks-rust-v-cyrius.md`,
  `docs/rust-v1-bench-history.csv`

For source/test/program counts and the per-file layout, read the
filesystem — those numbers go stale on every cut and shouldn't be
mirrored here.

## Consumers

soorat (renderer), rasa (image editor), ranga (image processing), bijli
(EM simulation), aethersafta (compositor), kiran (game engine via
soorat). Six-consumer regression sweep is Tier 2 ship work.

## Dependencies

- **Cyrius stdlib** — declared in `cyrius.cyml`, resolved into `lib/` by
  `cyrius deps`
- **`chitra` (AGNOS dep)** — pure-Cyrius image decoder (mabda uses its
  PNG and baseline-JPEG paths). Pinned via `[deps.chitra]` in
  `cyrius.cyml`; `asset_load.cyr` decodes through it to RGBA8. Its bundle excludes its own stdlib deps, so mabda provides
  `thread` + `sankoch`. **Consumers of mabda's dist must likewise add
  `[deps.chitra]` + `thread`/`sankoch`** (the samvada-style pattern).
- **`samvada` (AGNOS dep)** — Cyrius dbus client for logind master
  delegation. Pinned via `[deps.samvada]` in `cyrius.cyml`. Since
  samvada 1.0 it speaks dbus natively in Cyrius: the consumer calls
  `samvada_native_init()` to populate the fn-table that
  `gpu_surface_configure_native_logind` reads through (only compiled
  under `-D MABDA_LOGIND`). mabda never initializes samvada itself.
- **wgpu-native v29** — external C library, downloaded by consumers
  alongside their `deps/wgpu_main.c` launcher. Not a Cyrius dep. The AMD
  *route* through it is deprecated at v4.0.1 (a `gpu_context_from_preinit`
  vendorID guard warns but still allows AMD-on-wgpu; `-D MABDA_AMD_WGPU_STRICT`
  hard-rejects). The binding stays for NVIDIA + Intel until the wgpu+C
  path leaves the tree at v5.1; full AMD retirement is deferred.
- **libsystemd** — no longer required. samvada 1.0.x keeps its
  libsystemd C shim only as an opt-in fallback and plans to delete it at
  samvada 1.1.0. That deletion waits on a mabda live-bus end-to-end run
  of the logind path from a seated session, which has not been done
  (roadmap backlog).

All Cyrius deps are pinned in `cyrius.cyml`. `cyrius deps` resolves
them against the installed toolchain.

### Dependency wiring (HARD RULE)

`lib/` is a **real directory** populated by `cyrius deps` — it contains
per-module copies of the stdlib files declared in `[deps].stdlib`, plus
copies of the git-dep bundles cached under `~/.cyrius/deps/<pkg>/<ver>/`
(`samvada.cyr`, `chitra.cyr`). It contains no symlinks. It is
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
cyrius deps                                          # resolve stdlib + samvada + chitra into lib/
cyrius build programs/smoke.cyr build/mabda_smoke    # link-check
make test                                            # globs all tests/tcyr/*.tcyr
cyrius bench tests/bcyr/mabda.bcyr                   # CPU benchmarks
cyrius distlib                                       # → dist/mabda.cyr
make test-gpu                                        # wgpu integration programs (needs wgpu-native)
make test-native-compute-store                       # native compute (needs amdgpu)
make test-native-render-e2e                          # native render (HW-gated; cache-flush in tree)
make test-native-kms-summary                         # KMS topology probe (works in any session)
make test-native-kms-modeset                         # native modeset (HW + DRM master)
make test-native-present-e2e                         # 120-frame animated present (HW + master)
make bench-gpu                                       # GPU benchmarks (wgpu only today)
```

**Test counting:** use `./scripts/count-test-assertions.sh`. It runs
each suite on its own, gates on its exit code, and counts only
well-formed `N passed, 0 failed` summaries, so a crashing suite is
named instead of silently undercounted. `make test | grep` can't do
that. History: `texture.tcyr`'s summary used to start with a NUL byte
that made grep drop it. The NUL came from a stack-array overrun
(`var be[256]` filled with 328 bytes). It disappeared when cyrius
6.3.15 moved array locals onto the stack; the overrun was fixed in 4.1.3
(`docs/development/issues/2026-09-16-test-stack-array-overruns.md`).

## FFI Architecture

mabda has a wgpu path, native AMD and NVIDIA paths, and one auxiliary
dbus path. Only the wgpu path goes through a C shim; the native paths
issue ioctls directly, and samvada fills its fn-table in Cyrius.

### wgpu path (`deps/wgpu_main.c`)

1. C `main()` calls `_cyrius_init()` then `alloc_init()`
2. C pre-initializes GPU (instance/adapter/device/queue — Vulkan-only
   via `WGPUInstanceExtras { backends = Vulkan }`; default `All` was a
   problem on headless boxes)
3. C builds the function-pointer table (wgpu functions +
   struct-packing shims)
4. C calls `mabda_main(fn_table_ptr, preinit_ptr)` which the consumer
   defines
5. Cyrius calls wgpu via `fncall1`..`fncall6` for scalar-arg entry
   points and struct-packing shims for struct-by-value descriptors —
   **never** `fncall6` with a struct-by-value / float arg (the shim
   marshals those)

### Native AMD path (no C shim)

Compute / render / surface ioctls go directly through
`syscall(SYS_IOCTL)` — no C library, no libdrm. Pure Cyrius. Mappings:

- **`/dev/dri/renderD128`** for compute + render allocator (BO
  create / mmap / GEM close / VA map). AMDGPU-specific ioctls
  (GEM_CREATE / CTX / BO_LIST / GEM_VA / CS) routed through fd with no
  master required.
- **`/dev/dri/cardN`** for KMS surface (MODE_GETRESOURCES /
  MODE_GETCONNECTOR / MODE_GETENCODER / ADDFB2 / SETCRTC / PAGE_FLIP).
  Requires DRM master — see `samvada` path for the in-session master
  story.
- **PRIME bridge** between the two fds for the surface FB story (see
  `phase_d_prime_cross_fd_handle_bridge` vidya entry).

### samvada path (`-D MABDA_LOGIND`)

Consumer programs that use `gpu_surface_configure_native_logind` must
initialize samvada first. Same fn-table pattern, no C:

1. The consumer calls `samvada_native_init()` (samvada >= 1.0). This
   populates samvada's fn-table with its pure-Cyrius dbus backend. The
   libsystemd C shim (`samvada_shim_init()`) is an opt-in fallback.
2. `gpu_surface_configure_native_logind` (`src/surface_v3.cyr`) calls
   `samvada_session_take_device(major, minor)` through that table. If
   samvada was never initialized it gets -22 and returns 0, so the
   consumer can fall back to the kiosk path.
3. With the master fd in hand it dispatches
   `BACKEND_SLOT_SURFACE_CONFIGURE`, and releases the device on failure.

### CPU testing (no GPU, no master, no dbus)

`cyrius test` runs all `tests/tcyr/*.tcyr` files against `src/lib.cyr`
— no wgpu-native, no amdgpu hardware, no libsystemd needed.
Backend-abstraction routing exercised via mock-fnptr sentinels; native
ioctls / wgpu calls / sd_bus calls all surface as null-safety +
struct-shape tests at the Cyrius layer. HW gates live in the
`programs/native_*.cyr` programs.

## Key Constraints

- **Tests are the way.** Every new code path adds a CPU assertion.
  Stack-local `var ctx[112]` for test-scoped buffers (heap-allocated
  tests exhaust the bump allocator — see
  `bump_allocator_exhaustion_in_tests` vidya entry).
- **Own the stack** — every external dep is either an AGNOS package
  (samvada, sakshi, patra, sigil, chitra) or a consumer-provided C
  library (wgpu-native). The AMD wgpu *route* is deprecated at v4.0.1
  (warn+allow; strict flag enforces; binding stays to v5.1). libsystemd
  left the required set with samvada 1.0's native dbus (pinned since
  4.1.2).
- **No magic** — every operation measurable, auditable, traceable.
- **Manual memory** — `alloc / store64 / load64`. Every struct has a
  header comment block with field offsets.
- **Tagged unions for errors** — `Ok(value)` / `Err(gpu_err(...))` via
  `lib/tagged.cyr`.
- **f64 internally, f32 at the GPU boundary** — use `f64_to_f32` only
  when writing to a GPU buffer (a thin wrapper over the native
  `f32_from` builtin since 6.2.18; `src/` carries zero inline asm).
- **Prefix private helpers with `_`** — public API uses descriptive
  names.
- **Struct-pack wgpu args only for struct-by-value / float / variadic
  callees — NOT by arg count.** The old "Cyrius `fncall6` + wgpu-native
  segfaults" rule was a **misdiagnosis** (cyrius 6.3.26): the fault was a
  TLS/`%fs` init problem, not arg count — the glibc C launcher (ADR-004)
  supplies `%fs`, so scalar-arg wgpu entry points call **directly via
  `fncall6`** (`wgpu_command_encoder_copy_buffer_to_buffer`,
  `wgpu_queue_write_texture`, `wgpu_encoder_resolve_query_set`,
  `wgpu_buffer_map_sync`; retired in v4.0.2). A C shim is still required
  for struct-by-value descriptors (begin_render_pass slot 58,
  copy_texture_to_buffer slot 64), `float`/`double`, or variadic callees.
  See cyrius `docs/ffi/fncall-abi.md`.
- **No arg-count ceiling into extern-C; `fncallN` supports N ≤ 8
  (cyrius v5.4.13).** The old "7+ params into wgpu segfaults" belief was
  the same `%fs` misdiagnosis. Keep signatures small for readability, not
  for ABI safety.
- **`var X = expr;` initialization required.** Cyrius rejects bare
  `var X;` declarations — every var needs an initializer. Use
  `var X = 0;` for "to-be-set-later" pattern.
- **`gpu_shader_module_create` is byte-polymorphic.** wgpu reads as
  WGSL, native reads as pre-compiled GFX9 ISA. v3.0 ships consumer
  two-form bundles; in-mabda WGSL → GFX9 lowering is v3.x scope.
- **Phase D ioctl ordering matters.** Discovery → mode-pick → encoder
  → CRTC → AddFB2 → SETCRTC → PAGE_FLIP. Skipping discovery and
  hardcoding IDs breaks across reboots. See `phase_d_kms_sequencing`
  vidya entry.
- **`src/lib.cyr` owns the whole include chain.** Do not add Cyrius
  stdlib includes in individual `src/*.cyr` files.

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, audit history — know what was intended
1. Cleanliness: `cyrius build programs/smoke.cyr` (0 warnings),
   per-file `cyrius lint src/*.cyr` (0 warnings; the bare repo-wide
   form was removed in 5.7.x — see
   `feedback_cyrius_lint_fmt_per_file` memory),
   `cyrius vet programs/smoke.cyr` clean
2. Test sweep: all `tests/tcyr/*.tcyr` pass, `cyrius distlib`
   diff-clean
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
2. **Buffer safety** — every `var buf[N]` and `alloc(N)` verified: N
   in bytes, max offset < N, no adjacent-allocation overflow. The
   struct header comment's byte count must match the actual `alloc`
3. **Integer overflow** — any `a * b` / `a + b` / `a << n` on sizes or
   dimensions gets an overflow guard before use, especially in
   texture / buffer / workgroup math
4. **Divide-by-zero** — any `/` or `%` verifies the divisor is
   non-zero before the operation (workgroup helpers were the
   regression case in 2.3.0)
5. **Syscall return handling** — every `syscall()` return value is
   checked; error paths either recover or deterministically zero any
   output buffer the caller will read
6. **Pointer validation** — no raw deref of consumer-supplied
   pointers; label strings use `wgpu_string_view_len` with an explicit
   length when length is known
7. **FFI descriptor offset review** — every edit to
   `wgpu_descriptors.cyr` cross-referenced against the v29 `webgpu.h`
   layout; field offsets noted in the module header comment block
8. **Shim struct-by-value / float / variadic wgpu callees** — route
   those through a struct-packing shim in `deps/wgpu_main.c`. Scalar-arg
   entry points call directly via `fncallN` (N ≤ 8); the "direct
   `fncall6` crashes wgpu-native" claim was a `%fs`/TLS misdiagnosis the
   glibc C launcher already satisfies (cyrius 6.3.26; see
   `docs/ffi/fncall-abi.md`)
9. **Known CVE check** — review against current wgpu-native / WebGPU /
   GPU-driver CVEs since the prior audit
10. **File findings** — `docs/audit/YYYY-MM-DD-audit.md` with severity,
    file, line, class, mitigation

Severity levels: **CRITICAL** (exploitable immediately) / **HIGH**
(moderate effort) / **MEDIUM** (specific conditions) / **LOW**
(defense-in-depth).

### Task Sizing

- **Low/Medium effort**: batch freely
- **Large effort**: small bites — break into sub-tasks, verify each
- **If unsure**: treat as large

### Closeout Pass (before every minor/major bump)

1. Full CPU suite — `make test` (globs `tests/tcyr/*.tcyr`)
2. Bench baseline — `cyrius bench tests/bcyr/mabda.bcyr`
3. GPU integration (wgpu) — `make test-phase0` passes on a box with
   wgpu-native
4. GPU integration (native) — `make test-native-compute-store` passes
   on a box with amdgpu (HW-gated; requires AMD render node).
   `make test-native-render-e2e` and Phase D programs run from a tty /
   kiosk session OR with samvada+logind wired through a consumer.
5. `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean
6. Version consistency — `./scripts/version-check.sh` passes
7. Consumer check — soorat, rasa, ranga, bijli, aethersafta still
   build against the new bundle (Tier 2 ship work)
8. Audit index up to date — `docs/audit/` has the current
   `YYYY-MM-DD-audit.md` referenced from CHANGELOG

## CI / Release

- **Toolchain pin**: `cyrius = "x.y.z"` in `cyrius.cyml`. CI + release
  both read from the manifest — no hardcoded versions in YAML.
- **Tag filter**: release workflow triggers on `v[0-9]+.[0-9]+.[0-9]+`
  and `[0-9]+.[0-9]+.[0-9]+`. Version-verify step asserts
  `VERSION == git tag`.
- **Lint/fmt/vet gates**: CI fails on any `cyrius lint` warning,
  `cyrius fmt --check` drift, or `cyrius vet` finding.
- **Dist gate**: CI runs `cyrius distlib` and fails if
  `dist/mabda.cyr` drifts from the committed copy.
- **Smoke build**: `cyrius build programs/smoke.cyr` — proves the full
  include chain links.
- **Test/bench**: `make test` (globs all `tests/tcyr/*.tcyr` domain
  suites) + `cyrius bench tests/bcyr/mabda.bcyr`.
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
- Do not pass struct-by-value / `float` / variadic wgpu-native args via
  `fncallN` — those go through a struct-packing shim in
  `deps/wgpu_main.c`. Scalar-arg (handle/u32/u64) callees are fine via
  `fncall6` directly (the 6+-i64-args crash was a `%fs`/TLS misdiagnosis;
  cyrius 6.3.26)
- Do not add Cyrius stdlib includes in individual `src/*.cyr` files —
  `src/lib.cyr` owns the whole include chain
- Do not hardcode Cyrius toolchain versions in CI YAML — read
  `cyrius.cyml`
- Do not shell out to `cc5` directly for library code — go through
  `cyrius <subcommand>`. The one exception is `programs/phase0.cyr`,
  which the Makefile compiles with `printf 'object;\n' | cc5` because
  it needs to be linked against the C launcher.
- Do not mirror version history, file trees, or test counts in
  CLAUDE.md — point to `CHANGELOG.md`, `docs/architecture/overview.md`,
  and the filesystem
