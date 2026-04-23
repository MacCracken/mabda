# v3 Phase B.3.d Retest — Agent Handoff

**Date:** 2026-04-22
**Branch:** `v3`
**Prior context:** 7 sessions chasing a GFX9 GPU wedge. Root cause identified Session 7. **Live retest on hardware not yet performed.** This handoff prepares the next agent to run that retest cleanly.

## TL;DR

A two-dword PM4 encoding bug in `native_pm4_acquire_mem_full_invalidate` silently desynchronized the command-processor stream for every compute dispatch we've built since B.3.b. The CP consumed the SET_SH_REG header that followed ACQUIRE_MEM as stray data, then mis-parsed the rest of the IB as invalid opcodes — manifesting as `gfx_v9_0_bad_op_irq` and a MODE2 reset every time. Sessions 3–6 spent their cycles chasing VA maps, scratch V#s, VMID init, kernel-side PM4 theories, etc. — all downstream of a malformed stream.

Fix is committed to the working tree but **not yet run on real hardware**. The only remaining step for B.3.d is:

```
./build/native_compute_spike
```

**Expected outcome:** "dispatch completed (sync-obj signaled)" followed by "OK". GPU does not wedge. No reboot needed.

If that passes, B.4 unblocks and the agent can resume shader-store experiments.

## What changed in Session 7

### Code

- `src/backend_native.cyr`
  - `native_pm4_acquire_mem_full_invalidate` — count arg `7 → 6`. Header now emits `0xC0055802` byte-exact against Mesa's `AMD_DEBUG=ib` dump.
  - `native_pm4_dispatch_direct` — predicate arg `0 → 2`. Low byte of IT_DISPATCH_DIRECT is `shader_type`, not predicate. Header now `0xC0031502`.
  - Added `native_pm4_set_uconfig_reg_pair` (pair-write helper for TA_CS_BC_BASE_ADDR + _HI).
- `programs/native_compute_spike.cyr` — rewrote PM4 emission to mirror Mesa rusticl's exact preamble order (PGM_HI → STATIC_THREAD_MGMT × 2 → UCONFIG preamble → PGM_LO → RSRC1/2 → TMPRING_SIZE → USER_DATA_2/3 → USER_DATA_0 → ACQUIRE_MEM → RESOURCE_LIMITS → NUM_THREAD → DISPATCH_DIRECT). Register values aligned:
  - `RESOURCE_LIMITS = 0x140` (was `0` — zero caps waves to zero, silent stall)
  - `TMPRING_SIZE = 0x100` (was unset)
  - `STATIC_THREAD_MGMT_SE1/SE2/SE3 = 0` (was `0xFFFFFFFF`; Cezanne has 1 SE).
- `tests/tcyr/mabda.tcyr` — added `test_native_pm4_acquire_mem_layout` (byte-exact header + payload); updated `test_native_pm4_dispatch_direct_layout` and composability test for the new shader_type byte. **610/610 assertions pass.**

### Docs

- `docs/issues/2026-04-21-gfx9-store-blocker.md` — added "Session 7" section with the full PM4-bug analysis.
- `docs/proposals/v3-native-api-principles.md` — status line updated; Phase B status block added reflecting the current B.3.d pending-retest state.

### Memory

- `feedback_pm4_verify_against_mesa_ib.md` saved. Rule: any direct-PM4 code should diff header bytes byte-exactly against `AMD_DEBUG=ib` before ever being run live.

## What the next agent should do

### Step 1 — run the retest (single command)

```bash
./build/native_compute_spike
```

The binary is already built and on disk (`build/native_compute_spike`, 235 KiB, modified 2026-04-22 17:13). No rebuild needed unless you edit a `.cyr` file first.

**Three possible outcomes:**

**(a) Clean success.** Output ends with `dispatch completed (sync-obj signaled)` then `OK`. `dmesg` stays quiet. This closes B.3.d, moves Phase B back to Phase B.4 resumption.

**(b) Sync-obj times out (`FAIL: syncobj_wait errno=110`).** PM4 is well-formed (no reset), but the shader never signals. Likely remaining issues: wrong initiator bits, HQD queue selection, or a residency/cache issue that ACQUIRE_MEM doesn't cover. Re-dump with `AMD_DEBUG=ib` and compare *payload* bytes, not just header.

**(c) GPU reset (`gfx_v9_0_bad_op_irq`, kernel dmesg, device hangs).** PM4 still malformed somewhere we haven't caught. Capture `sudo cat /sys/class/drm/card0/device/devcoredump/data > /tmp/gpu_dump.txt` **before** any reboot (dump auto-expires on reboot). Diff our exact IB bytes against Mesa's AMD_DEBUG=ib output — the diff tool of choice is `xxd` on both captures.

Only outcome (c) requires a reboot. Outcomes (a) and (b) leave the GPU healthy.

### Step 2 — if (a), resume B.4

- `programs/native_compute_store.cyr` exists and is the scaffolded verifier. It currently uses an `s_endpgm`-only "shader" (diagnostic stub from Session 6). Restore the actual store variant — `native_gfx9_shader_store_deadbeef` in `src/backend_native.cyr` has the assembly commented. With the PM4 encoder fixed, the store variants are worth re-trying in order: `global_store_dword` via USER_DATA-loaded address first (simplest memory-op encoding on GFX9).
- Exit criterion (from the design doc): byte-identical readback of a known value the shader wrote. The BO is CPU-mapped (GTT) so readback is a plain `load32(ptr)`, no staging dance needed.

### Step 3 — if (a) and B.4 progresses, consider what to ship

Phase B's stated exit is "byte-identical compute output vs wgpu." The wgpu path's reference is `programs/compute_e2e.cyr`. Once the native path matches wgpu on a single trivial kernel, Phase B's exit is *technically* met — but consider whether a second kernel (add-kernel, not just constant-store) is worth landing before declaring B done. The design doc's bench targets (`native_vs_wgpu_compute` ≥ 90%) aren't meaningful on a one-instruction shader.

### Step 4 — documentation + commit

No commits yet — working tree is dirty with Session 7 changes. After verifying the retest outcome:

- Commit the Session 7 fixes with a message that credits the Mesa-IB diff methodology (it's the actually-valuable lesson).
- Update `CHANGELOG.md` under a `## [3.0.0-dev]` section (don't invent a release date).
- Update the "Phase B status (2026-04-22)" block in `v3-native-api-principles.md` to reflect the new reality.

The user handles all `git commit` / `git push` ops. Don't run them.

## Gotchas to respect

- **Don't edit `lib/*.cyr` by hand.** See `CLAUDE.md` HARD RULE. `lib/` is populated by `cyrius deps`; writing through it cross-corrupts the cyrius repo (the bug that motivated the rule).
- **Don't chase register values before verifying PM4 header bytes.** See `feedback_pm4_verify_against_mesa_ib.md`. Six sessions got burned theorizing about VMID/scratch/VA aperture when the actual bug was one bit in a count field.
- **Don't amend the PM4 builder without updating its byte-exact test.** `test_native_pm4_acquire_mem_layout` is the new reference; add an equivalent for any new packet type.
- **One IB wedge = one reboot.** There is no recovery-without-reboot path on this hardware/driver combo. Before every new live run, confirm the PM4 stream builds under `cyrius build` and that `cyrius test tests/tcyr/mabda.tcyr` is green; don't run the spike if either check fails.

## Repo state at handoff

```
branch: v3
working tree: dirty (Session 7 changes uncommitted)

modified:
  docs/issues/2026-04-21-gfx9-store-blocker.md       (+98 / Session 7 analysis)
  docs/proposals/v3-native-api-principles.md         (status + Phase B block)
  programs/native_compute_spike.cyr                  (PM4 rewrite)
  src/backend_native.cyr                             (two packet-encoder fixes + pair helper)
  tests/tcyr/mabda.tcyr                              (new acquire_mem test + updated assertions)

new:
  docs/handoff/2026-04-22-pm4-fix-retest.md          (this document)

CPU tests:  610 passed, 0 failed
Fuzz:       3 harnesses (drm_ioc_encode, pm4_builder, render_graph_aliasing) — run with `cyrius fuzz`
Spike bin:  build/native_compute_spike (235 KiB)
```

## Supporting material

- **Issue doc with full debugging arc:** `docs/issues/2026-04-21-gfx9-store-blocker.md` — Sessions 1–7.
- **Mesa reference PM4 dump:** regenerate any time with `AMD_DEBUG=ib ./build/shader/cl_probe`. The `cl_probe.c` source is a minimal OpenCL program that writes 0xDEADBEEF via Mesa rusticl — the exact same primitive we're trying to hit natively.
- **vidya field notes:** `../vidya/content/cyrius/field_notes/mabda-v3-gpu.toml` and `../vidya/content/direct_drm_gpu_compute/concept.toml` — portable gotcha list for any future direct-DRM work.
