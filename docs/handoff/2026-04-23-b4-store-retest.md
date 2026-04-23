# v3 Phase B.4 Live Retest — Agent Handoff

**Date:** 2026-04-23
**Branch:** `v3`
**Prior context:** B.3.d retest passed today (see `docs/handoff/2026-04-22-pm4-fix-retest.md` and Session 8 in the store-blocker issue). With the PM4 builder fixed, Phase B.4 reopens: a real store shader + output-buffer readback. All code is written, all CPU gates are green, **binary is built and on disk but has NOT been fired on hardware** because the current session did not have SSH access to the dev box.

## TL;DR

```bash
./build/native_compute_store
```

Binary is at `build/native_compute_store` (226,232 bytes, built 2026-04-23 13:48 under released cyrius 5.6.13). No rebuild needed unless a `.cyr` file changes first. The source changes since B.3.d are all in this repo, committed and reviewed against the spike that just passed.

**Expected outcome (a):**
```
OK — GPU wrote 0xDEADBEEF via pure-Cyrius dispatch
```
RC=0, `dmesg` silent, `output[0] = 0xDEADBEEF`, sentinel intact at `[1]`. **Phase B.4's stated exit is met.**

If that happens: Phase B is done, and the v3 roadmap can proceed to Phase C (DRM render path) or to the secondary store-verify discussion (second kernel — add-kernel — before shipping, see "What to consider if (a)" below).

## What changed since 2026-04-22

All changes are on branch `v3`, working tree dirty (uncommitted) — user handles git.

### Shader — real store bytes restored

- `src/backend_native.cyr::native_gfx9_shader_store_deadbeef` — replaced the Session 6 diagnostic stub (3 × v_mov + s_endpgm + NOP pad, 84 bytes, did not store) with the real shader. Bytes come from `clang -target amdgcn--amdhsa -mcpu=gfx90c`:

  ```
  +0:  0x7E000200  v_mov_b32_e32 v0, s0
  +4:  0x7E020201  v_mov_b32_e32 v1, s1
  +8:  0x7E0402FF  v_mov_b32_e32 v2, <literal>
  +12: 0xDEADBEEF  literal
  +16: 0xDC738000  global_store_dword v[0:1], v2, off glc slc  (dword 1)
  +20: 0x007F0200  global_store_dword                          (dword 2)
  +24: 0xBF8C0070  s_waitcnt vmcnt(0) lgkmcnt(0)
  +28: 0xBF810000  s_endpgm
  +32..+95: 16 × 0xBF800000   (AMDGPU-ABI prefetch padding)
  ```

  New shader is 32 B of instructions + 64 B of NOP pad = 96 bytes. `return pos + 84` changed to `return pos + 96`.

- `tests/tcyr/mabda.tcyr::test_native_gfx9_shader_store_deadbeef_writes_bytes` — new byte-exact test covering every dword above. Added to the test runner. **Test count 610 → 621.** All green under 5.6.13.

### Store program — PM4 preamble aligned to the working spike

- `programs/native_compute_store.cyr` — rewritten. The Session 6 preamble order (which never worked on hardware) is gone. The new PM4 stream is byte-identical to `programs/native_compute_spike.cyr` (the B.3.d baseline that passed on 2026-04-23) except for three narrow store-specific deltas:

  1. **USER_DATA_0/1 = output VA low/high.** The spike put Mesa's scratch-V# magic bytes (`0x00200040`, `0`) here; the store shader needs s0/s1 to be the real output buffer address, so we overwrite with `out_va & 0xFFFFFFFF` and `(out_va >> 32) & 0xFFFFFFFF`.
  2. **USER_DATA_2/3 = Mesa's scratch-V# stub (`0x00200000`, `0`) kept.** Shader ignores s2/s3, but the HW may implicitly reference them; see the Session 5 analysis in the store-blocker issue. Keeping the spike-matched values is the low-risk choice.
  3. **BO list has 4 entries** (shader + stub + output + IB). Spike had 3.

  VAs are all canonical-high: shader @ `0xFFFF800100000000`, IB @ `0xFFFF800100200000`, output BO @ `0xFFFF800100400000`, stub @ `0x200000` (matches spike).

  RSRC2 kept at `0x8` (USER_SGPR=4, no TRAP_PRESENT). The spike-verified value; we're not introducing TRAP_PRESENT unless the first attempt traps on a memory op.

### Toolchain pin

- `cyrius.cyml` bumped `5.5.20 → 5.6.13`. Active toolchain on the dev box was already 5.6.14 (in-dev, not released); `cyriusly use 5.6.13` was run to drop back to the released line before rebuilding everything in this session.

### Dist refresh

- `dist/mabda.cyr` regenerated. The regen picked up `+274/-6` lines that were *not* from the pin bump — they're v3 Phase A work (`src/render_graph.cyr` transient aliasing planner) that landed in commit `211a47b phase A` but never got a corresponding `cyrius distlib` regen committed. The refresh is correct and catches up latent drift.

### CHANGELOG

- `## [3.0.0-dev]` section added with the Session 7 PM4 fixes, the 2026-04-23 B.3.d retest pass, the test-count bump, and the Mesa-IB diff methodology note.

## What the next agent should do

### Step 1 — run the retest (single command)

```bash
./build/native_compute_store
```

**Three possible outcomes:**

#### (a) Clean success — Phase B.4 closed

Output includes:
```
output[0] = 0xDEADBEEF (want 0xDEADBEEF)
sentinel intact at [1]
OK — GPU wrote 0xDEADBEEF via pure-Cyrius dispatch
```

RC=0, `dmesg` quiet. This meets Phase B's stated exit ("byte-identical compute output vs wgpu" — the trivially-equivalent constant-store case).

**Next moves if (a):**
- Commit the Session 7 PM4 fixes + the Session 8 retest closure + the B.4 store-shader work + the pin bump + the dist refresh as one coherent `v3` batch (user handles git).
- Update the Phase B status block in `docs/proposals/v3-native-api-principles.md` to mark B.4 ✅.
- Close `docs/issues/2026-04-21-gfx9-store-blocker.md` with a "Session 9: store verified" section.
- Discuss with user: is Phase B done, or do we want an add-kernel second-shader pass before calling it (the `native_vs_wgpu_compute` bench target of ≥90% isn't meaningful on a one-instruction shader)?

#### (b) Sync-obj timeout (`FAIL: syncobj_wait errno=110`)

PM4 is well-formed (no GPU reset) but the shader either didn't run or didn't write. Differentiate with `output[0]` value on exit:

- **Still `0xBAADF00D`** (sentinel): shader didn't execute. Possibilities: USER_SGPR value wrong (try RSRC2 = 0x04 with USER_DATA_0/1 only); DISPATCH_INITIATOR bits; wave launch blocked on a resource check we haven't modelled.
- **Some other value**: shader ran but to the wrong address. Check VA encoding of USER_DATA_0/1 — are we splitting the canonical-high VA correctly across low/high u32s?

Re-dump Mesa's IB with `AMD_DEBUG=ib ./build/shader/cl_probe` and diff *payload* bytes vs ours, not just header. `cl_probe.c` is at `build/shader/cl_probe.c`.

#### (c) GPU reset (`gfx_v9_0_bad_op_irq`, kernel dmesg, wedge)

PM4 still malformed somewhere. Capture `sudo cat /sys/class/drm/card0/device/devcoredump/data > /tmp/gpu_dump.txt` **before** any reboot (dump auto-expires on reboot).

Most likely culprit given B.3.d passed with the *same* preamble: the store-specific deltas.
- USER_DATA_0/1 being a real VA might be triggering a HW check that the scratch-V# stub values bypassed.
- The canonical-high output VA (0xFFFF800100400000) — maybe the shader VA aperture and data VA aperture have distinct rules and data BOs can't live there.
  - Fallback: move `out_va` to `0x1000000000` (64 GiB, which the Session 6 store program used without reaching this code path) or to `0x200040` (just above the stub — Mesa-like).
- If dispatch wedges but the spike binary (still on disk) doesn't, the delta is one of the above three things only.

### Step 2 — if (a), consider shipping posture

Phase B's stated exit is met by (a). But `native_vs_wgpu_compute ≥ 90%` (from the Validation Benchmarks table) is not measurable on a one-instruction shader — that bench gate is more meaningful on an add-kernel or reduction. Worth a conversation with the user about whether:

- (i) Phase B is done, bench gate is tracked as v3.1-or-later work.
- (ii) Phase B gets a second shader (add-kernel: two inputs, one output, real arithmetic) before being declared done.

Either is defensible; the design doc's letter-of-the-law favors (i), the spirit favors (ii).

### Step 3 — documentation + commit

User handles all git operations. When (a) happens:

- Commit message credits both the Mesa-IB diff methodology (the actually-valuable lesson from the PM4 debugging) and the spike-mirror approach to the store program (the minimum-change path off a known-working baseline).
- Update Phase B status block in `v3-native-api-principles.md` — B.4 ✅.
- Close the store-blocker issue with a final "Session 9: store verified" section, mirroring the Session 8 close format.

## Gotchas to respect

- **Don't edit `lib/*.cyr` by hand.** HARD RULE in CLAUDE.md. `lib/` is a `cyrius deps` output; writing through it cross-corrupts the cyrius repo.
- **Don't chase register values before PM4 byte-diffing.** The Session 1–6 debugging arc spent weeks on VMID/scratch theories when the actual bug was a one-bit count-field off-by-one. If the store wedges, diff *our* IB bytes against Mesa's `AMD_DEBUG=ib` output first. See `feedback_pm4_verify_against_mesa_ib` in auto-memory.
- **One IB wedge = one reboot.** Before running `./build/native_compute_store`, confirm `cyrius test tests/tcyr/mabda.tcyr` is green (expect 621 passing) and `cyrius build programs/smoke.cyr` clean.
- **If modifying `native_gfx9_shader_store_deadbeef`, update the byte-exact test in lockstep.** `test_native_gfx9_shader_store_deadbeef_writes_bytes` in `tests/tcyr/mabda.tcyr`.

## Repo state at handoff

```
branch: v3
working tree: dirty (session 2026-04-23 changes uncommitted)

modified:
  CHANGELOG.md                                       (+57 / [3.0.0-dev] section)
  cyrius.cyml                                        (pin 5.5.20 → 5.6.13)
  dist/mabda.cyr                                     (+274/-6, v3 Phase A refresh)
  docs/issues/2026-04-21-gfx9-store-blocker.md       (+67 / Session 8 retest-passed)
  docs/proposals/v3-native-api-principles.md         (status + Phase B block)
  programs/native_compute_store.cyr                  (full rewrite to mirror spike)
  src/backend_native.cyr                             (real store shader bytes)
  tests/tcyr/mabda.tcyr                              (+new store shader byte-exact test)

new:
  docs/handoff/2026-04-23-b4-store-retest.md         (this document)

CPU tests:  621 passed, 0 failed
Active toolchain: cyrius 5.6.13 (released)
Store bin:  build/native_compute_store (226,232 B, 2026-04-23 13:48)
Spike bin:  build/native_compute_spike (224,984 B, 2026-04-23 13:34; known-passing baseline)
```

## Supporting material

- **B.3.d retest handoff + session log:** `docs/handoff/2026-04-22-pm4-fix-retest.md` and the Session 7/8 sections of the store-blocker issue.
- **Known-working PM4 reference:** `programs/native_compute_spike.cyr` — the store program's PM4 preamble is line-for-line identical except for the three documented deltas.
- **Mesa reference IB:** regenerate any time with `AMD_DEBUG=ib ./build/shader/cl_probe`. Source at `build/shader/cl_probe.c`.
- **vidya field notes:** `../vidya/content/cyrius/field_notes/mabda-v3-gpu.toml` and `../vidya/content/direct_drm_gpu_compute/concept.toml`.
