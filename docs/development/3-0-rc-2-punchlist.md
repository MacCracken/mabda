# mabda 3.0.0-rc.2 punchlist

**Goal**: close out every deferred item from the
[2026-04-30 audit](../audit/2026-04-30-audit.md) so that official
`3.0.0` ships with the audit-track empty. rc.2 is the gate; the
official cut happens once rc.2 has had a soak window with no
regressions surfaced.

**Status**: planning. Items below are in expected work order, with
expected effort. None are HIGH severity (those landed in rc.1).

---

## Audit items (10 deferred)

### MED-2 — Caller-supplied dimension overflow guards

- **File**: `src/backend_native_kms.cyr` lines 1218–1220 (modeset
  driver) + 1480–1482 (alloc_fb) + 1496 (forwarded to
  `native_kms_add_fb_xrgb8888`).
- **Fix**: cap `width / height` at 16384 (WebGPU's
  `MAX_TEXTURE_DIMENSION_2D`) and add overflow guards on
  `pitch_raw = width * 4` and `bo_size = pitch * height` before
  the values flow into the FB ioctl. Reject `bo_size > 2^31` to
  match the kernel's `pitches[0]` u32 cap.
- **Test**: 4 new asserts in `mabda_v3_phase_d.tcyr` covering
  `native_kms_alloc_fb` rejection at the 16385 / 65537 / negative
  / overflow boundaries.
- **Effort**: ~1 hour (small targeted patch + asserts).

### MED-4 — Two-pass DRM discovery TOCTOU clamping

- **File**: `src/backend_native_kms.cyr:291–363` (`native_kms_init`)
  + `src/backend_native_kms.cyr:645–680`
  (`native_kms_get_connector_modes`).
- **Fix**: after the fill-pass ioctl, clamp `actual = min(actual,
  capacity)` before exposing the count to the caller. Three-line
  patch in each callsite.
- **Test**: hard to unit-test (would need a kernel-mock harness);
  add an inline comment citing the clamp + expected post-fill
  invariant. Exercised live in the existing kms_summary smoke.
- **Effort**: ~30 minutes.

### MED-5 — `native_drm_set_master` `-EINVAL` ambiguity

- **File**: `src/backend_native_kms.cyr:62–67`.
- **Fix**: stop collapsing `-EINVAL` to success. Either pass it
  through unchanged (let the caller decide based on subsequent
  ioctl results) OR add a probe via `DRM_IOCTL_AUTH_MAGIC` to
  disambiguate "already master" from "operation not supported."
  Pass-through is simpler.
- **Test**: refresh the `test_native_drm_set_master_*` cases in
  `mabda_v3_phase_d.tcyr` to cover the new return-code shape.
- **Effort**: ~30 minutes.

### MED-7 + LOW-5 — Bump-allocator leaks on per-dispatch PM4 scratches

- **Files**: `src/backend_native.cyr:2535-2595`
  (`_backend_native_render_pass_draw`, 1024-byte scratch per draw)
  + `src/backend_native.cyr:2374`
  (`_backend_native_compute_dispatch`, 256-byte scratch per
  dispatch) + `src/backend_native.cyr:2103, 2220` (8-byte syncobj
  handle ptr).
- **Fix**: cache the PM4 scratch on `GpuContext` (alongside the
  cached IB BO at `_NATIVE_IB_VA`) or on the `NativePass` /
  `NativePipeline` structs (which already alloc on configure).
  Reuse a single 1024-byte scratch across compute + render slots.
  Add explicit "scratch is reset on each dispatch, never
  reclaimed" comment to make the new lifetime contract explicit.
- **Test**: long-running stress test that submits N=10000
  dispatches and verifies the bump allocator hasn't grown (would
  need a `bump_allocator_high_water_mark()` helper from the
  Cyrius runtime — file separately if not present). Lighter
  alternative: assert that the scratch ptr is the same value
  across two consecutive dispatches.
- **Effort**: ~3-4 hours. The lifetime-contract change is the
  hard part — every fnpath that touches the cached scratch needs
  to know it's shared and not safe to re-enter mid-flight.

### LOW-1 — `native_drm_set_master` missing `fd > 0` guard

- **File**: `src/backend_native_kms.cyr:62`.
- **Fix**: add `if (fd <= 0) { return -1; }` at the top, matching
  every other ioctl wrapper in the file. Same for
  `native_drm_drop_master`.
- **Test**: 2 asserts in `mabda_v3_phase_d.tcyr`.
- **Effort**: ~10 minutes.

### LOW-2 — `_kms_summary_print_u32` 16-byte buffer too small for i64

- **File**: `src/backend_native_kms.cyr:584`.
- **Fix**: bump `var buf[16]` to `var buf[24]` to safely hold a
  20-digit i64. Or add an internal assert that `n <= 16` after
  the `fmt_int_buf` call.
- **Test**: 1 assert that the kms_summary printer doesn't crash
  on a max-i64 input.
- **Effort**: ~10 minutes.

### LOW-3 — Native `gpu_buffer_*` / `gpu_shader_module_*` slot stubs

- **File**: `src/backend_native.cyr:2334–2358`.
- **Decision required**: ship LOW-3 as either:
  - (a) **Real impls** — port the v2 `gpu_buffer_create` / `_write`
    / `_read` / `_release` to use `native_buf_pair_*` private API
    underneath, plus wire `gpu_shader_module_create` to accept
    pre-compiled GFX9 ISA bytes and stash a NativeShaderModule
    handle. ~6-8 hours of careful surface-matching work.
  - (b) **Diagnostic error** — add `GPU_ERR_NOT_IMPLEMENTED` to
    the enum (LOW-3 audit recommendation), return it from the
    stub paths, document the limitation explicitly in the
    migration guide. ~30 minutes.
- **Recommendation**: (b) for rc.2 ship; (a) is v3.x scope. The
  consumers that need native today (the in-tree programs) all use
  `native_buf_pair_*` / shader-library helpers directly; the
  public `gpu_buffer_*` / `gpu_shader_module_*` API only matters
  for cross-backend portable code, and that's blocked by LOW-4
  anyway.
- **Effort** (option b): ~30 minutes.

### LOW-4 — `vc` / `ic` honoured in native render

- **File**: `src/backend_native.cyr:2535`
  (`_backend_native_render_pass_draw`).
- **Fix**: thread `vc` (vertex count) into
  `native_pm4_build_render_draw_tail` as the `index_count` arg of
  the `DRAW_INDEX_AUTO` packet. Reject `ic != 1` for now (instance
  rendering is v3.x scope). With `vc` honoured, cross-backend
  identity for the existing FS+VS pair is restored.
- **Test**: 3 asserts that the PM4 stream produced for `vc = 6` /
  `vc = 36` / `vc = 0` differs in the expected dword field.
- **Effort**: ~1 hour.

---

## Toolchain / scaffold work (rc.2 prerequisites)

### Split `src/backend_native.cyr` to unblock the fmt gate

- **File**: `src/backend_native.cyr` (~3,100 lines, 137 KiB).
- **Why**: `cyrius fmt` and `cyrius lint` both have a 128 KiB
  read-buffer cap (see `feedback_cyrlint_128k_buffer_cap`
  memory). `cyrius lint` already silently passes (it just prints
  truncated warnings); `cyrius fmt --check` produces false-
  positive drift on every CI run. The temporary CI workaround
  (skip files >128 KiB) is in tree as of rc.1, but it leaves a
  large file un-formatted-checked which is not ideal.
- **Fix**: split into four files along the existing internal
  section boundaries:
  1. `backend_native.cyr` — Backend-slot fillers + helpers (~600 lines)
  2. `backend_native_pm4.cyr` — PM4 packet builders + register
     constants (~1,200 lines)
  3. `backend_native_amdgpu.cyr` — AMDGPU ioctl wrappers + BO/VA
     mgmt (~800 lines)
  4. `backend_native_compute.cyr` — Compute dispatch + shaders +
     RT + Pipeline / Pass structs (~500 lines)
  Keep `backend_native_kms.cyr` separate (already 1,596 lines —
  inside the cap).
- **Test**: must keep all 1828 asserts green across the split.
  Re-run `cyrius distlib` and verify the bundle is byte-identical
  modulo include-order (or update the dist comparator if needed).
- **Effort**: ~4-6 hours. The split itself is mechanical; the
  risk is in preserving include-order semantics for the bundle
  generator + ensuring the test files still find every symbol.
- **Then**: drop the `>128 KiB skip` block from `.github/workflows/ci.yml`.

### Six-consumer regression sweep (Tier 2 ship work)

- **Goal**: build soorat / rasa / ranga / bijli / aethersafta /
  kiran (via soorat) against the rc.1 bundle to surface any
  consumer-side breakage from the v3 surface API + samvada wire-up
  + `gpu_render_target_create` odd-dim rejection.
- **Method**: `cd ~/Repos/<project> && cyrius deps && cyrius build
  programs/smoke.cyr build/<proj>_smoke` for each. File any
  failures as `docs/issues/2026-MM-DD-<project>-rc1-regression.md`.
- **Pass criteria**: every consumer builds against the rc.1 bundle
  without modification. Behavioral changes (e.g., `gpu_surface_*`
  is new — consumers that don't use it shouldn't notice) accepted.
- **Effort**: ~2-3 hours per consumer × 6 = ~12-18 hours of
  walking work. Lots of it is low-friction "build, see what
  breaks, file it." Can be parallelized to a sub-agent.

### radv_capture Phase 2 (optional, bonus)

- **Goal**: add the actual byte-diff reduction tooling on top of
  the rc.1 minimum-viable Phase 1 (`programs/diagnostics/radv_capture/`).
- **Method**:
  1. Add a `--dump-pm4` flag to `programs/native_compute_store.cyr`
     that calls the composer without submitting and prints the
     dword stream.
  2. Write a small extractor (`programs/diagnostics/radv_capture/
     extract_dispatch.sh`) that takes a RADV `--dump=ibs` dump and
     prints just the dispatch tail — the final `DISPATCH_DIRECT`
     packet plus the immediately preceding `COMPUTE_PGM_*` /
     `COMPUTE_USER_DATA_*` / `COMPUTE_NUM_THREAD_X/Y/Z` register
     writes.
  3. `make compare` target that dumps both, extracts dispatch
     tails, runs `diff`.
- **Pass criteria**: the diff is byte-clean for the
  store_deadbeef shape, OR each diff line is annotated with a
  reason ("RADV emits CACHE_FLUSH_AND_INV here, mabda emits
  ACQUIRE_MEM with same-class effect" — an explicit
  not-byte-identical-but-equivalent map).
- **Effort**: ~6-8 hours. Phase 2 is genuinely useful diagnostic
  work; defer if the Tier 2 / consumer sweep is taking longer.
- **Disposition**: bonus. Not ship-blocking for 3.0.0.

---

## Closeout for 3.0.0 (post rc.2)

Once every audit-tracked item above is closed:

1. Run the full P(-1) closeout matrix per CLAUDE.md
   (`make test`, full test files, GPU integration on wgpu + native,
   distlib regen diff-clean, version-check, audit index up to date).
2. Re-run the audit (`docs/audit/YYYY-MM-DD-audit.md`) — must
   surface 0 HIGH and 0 MEDIUM findings new since 2026-04-30.
3. Soak window: 3-day burn-in on the dev box with the consumer
   programs running continuously. Confirm no leak / stutter / OOM
   surfaces.
4. VERSION 3.0.0-rc.2 → 3.0.0.
5. Tag `v3.0.0`. CI release pipeline should produce the same
   artifacts as the rc.1 dry-run.

---

## What's NOT in rc.2 scope

These items live in the v3.x backlog or beyond, NOT rc.2:

- WGSL → GFX9 lowering (separate AGNOS package — naming TBD;
  `karkati` / `tarjuma` candidates per
  `docs/proposals/v3-wgsl-frontend-choice.md`).
- NVIDIA / Intel native paths (v4.0 / v5.0).
- wgpu-native + libsystemd C-shim retirement (v4.0 — both wgpu and
  samvada drop their C shims together).
- Multi-GPU disambiguation in `gpu_surface_configure_native_logind`
  (currently tries minor 0 and 1 only).
- Native render path with multi-shader pairs / instance rendering
  beyond `vc` honouring (LOW-4 fixes the simple case; the broader
  shape is v3.x).
- Mode picker by `(width, height)` rather than EDID-preferred
  (HIGH-1 fixes by failing loudly; v3.x adds the picker).

---

## Tracking

- File this doc + the audit doc as the references in any rc.2 PR.
- Update CHANGELOG `[3.0.0-rc.1]` section with a forward-pointer
  to this doc so the chain is discoverable.
- When each item closes, mark it `[x]` here in tree, then file the
  fix commit referencing the audit ID (e.g.
  `Fix MED-7: cache PM4 scratch on GpuContext (audit 2026-04-30)`).
