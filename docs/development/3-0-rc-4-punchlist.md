# mabda 3.0.0-rc.4 punchlist

**Goal**: extended soak. Burn the rc.3 bundle on the dev box for
**24 hours minimum, up to 3 days**, with the consumer programs
running continuously. The two horizons:

- **24h clean = earliest cut for 3.0.0 GA.** The 24h gate is the
  conservative-but-shippable line. If the bundle holds for a full
  day, we have enough confidence in the dual-backend dispatch
  path + KMS surface + samvada logind story to tag GA.
- **3-day window = focus of the first few 3.0.x patches.**
  Anything that surfaces between 24h and 72h becomes a tracked
  3.0.1+ item, not a GA blocker. The 3-day soak is the *targeted*
  long-run; we'll catch what we can in it, but we don't gate GA
  on running the full window.

**Status**: in-progress (2026-05-19). rc.3 cut + merged into main
(`551cdb3 merge v3 into main: 3.0.0-rc.3 cut`); 6h gate cleared
clean (33/33 checkpoints PASS, 21.6 M total iters, RSS flat at
12 760 KB, dmesg Δ = 0 — see
`docs/handoff/soak-20260519T072831Z/`). Toolchain pin moved
`5.11.64 → 6.0.0` in the working tree post-merge; **`make test`
currently fails under 6.0.0 with `undefined function 'assert'`**.
Resolving that is the rc.4 pre-flight bottleneck — see "Open
investigation" below.

---

## Prerequisites

- [x] rc.3 cut clean (6h soak passed, tag `3.0.0-rc.3` exists on
  `40038c5`, merged to main via `551cdb3`).
- [ ] All P(-1) gates from rc.3 still green under the **6.0.0**
  toolchain: `make test`, `cyrius distlib`, `scripts/version-check.sh`,
  `cyrius build programs/smoke.cyr build/mabda_smoke`. Currently
  blocked on the assert-sweep below.
- [ ] No in-flight diffs against rc.3. The soak runs against a tagged
  immutable cut. Working tree at handoff carries only the
  `cyrius.cyml` pin bump.

---

## Open investigation (2026-05-19 → rc.4 kickoff)

These are the items between "rc.3 merged" and "rc.4 24h clock can
start". None are GA-scope features; all are toolchain / consumer
hygiene the rc.3-cleared bundle deserves before the 24h burn.

### 1. cyrius 6.0.0 transition — assert rename sweep (BLOCKER)

The "fixing renames" pass in cyrius 6.0.0 removed the global
`assert` / `assert_eq` / `assert_neq` / `assert_summary` fn names
that all three test files use. Symptom under 6.0.0:

```
warning: undefined function 'assert'
warning: undefined function 'assert_eq'
warning: undefined function 'assert_neq'
warning: undefined function 'assert_summary'
make: *** [Makefile:53: test] Error 132
```

Tasks:

- [ ] Find the new symbol(s) in cyrius 6.0.0. Grep
  `/home/macro/Repos/cyrius/programs/` for the test-runner shape,
  and check whether 6.0.0 exposes asserts via a stdlib module
  (`tagged.cyr` / `result.cyr` are both still in 6.0.0's lib/) or
  as a test-runner-injected intrinsic.
- [ ] Sweep call sites across all three files:
  - `tests/tcyr/mabda.tcyr` (624 calls)
  - `tests/tcyr/mabda_v3.tcyr` (951 calls)
  - `tests/tcyr/mabda_v3_phase_d.tcyr` (382 calls)
- [ ] Re-run `make test` → confirm 1957/1957 holds under 6.0.0.
- [ ] Re-build the soak binaries (`make build-soak` or per-program
  `cyrius build`) so the 24h run links against the 6.0.0 toolchain
  output, not the 5.11.64 binaries we ran rc.3 on.

### 2. Re-verify wgpu-native path under 6.0.0

The wgpu launcher (`deps/wgpu_main.c`) and the 7 struct-packing
shims were last verified under 5.11.64. 6.0.0 may have changed:

- [ ] `cc5` is still present in 6.0.0/bin (was, as of the lib/
  resolve). Confirm the Makefile's `phase0` path (the one place we
  shell out to `cc5` directly) still links.
- [ ] The fncall1/2/5 ABI into wgpu-native — exercise via
  `make test-phase0` + `make test-render-graph-e2e` on a box with
  wgpu-native.

### 3. Re-verify native AMD path under 6.0.0

The PM4 byte composers + GFX9 ISA shaders are pure Cyrius — no C
shim — so the rc.3 TDR fixes (EXP encoding + SPI_SHADER_COL_FORMAT
field values) should ride through unchanged. But:

- [ ] `make test-native-compute-store` clean readback under 6.0.0.
- [ ] `make test-native-render-e2e` (the formerly-TDR'd path) clean
  pixel under 6.0.0.
- [ ] `make test-native-texture-e2e` clean under 6.0.0.
- [ ] `make test-native-kms-summary` topology probe clean (no-master).

### 4. Six-consumer regression sweep (carried over from rc.3)

This was on the rc.3 punchlist exit list but didn't land before the
tag cut. rc.4 doesn't claim it as scope (per the bottom of this
doc), but the dist bundle the consumers pull from changed at rc.3,
so we owe one clean pass against the rc.3 bundle before the 24h
clock starts:

- [ ] soorat — renderer (textures, pipelines, render pass)
- [ ] rasa — image editor (compute, textures)
- [ ] ranga — image processing (buffers, compute)
- [ ] bijli — EM simulation (compute, storage buffers)
- [ ] aethersafta — desktop compositor (surfaces, present)
- [ ] kiran — via soorat

Pass criterion: every consumer builds against the rc.3 bundle
under the 6.0.0 toolchain (i.e., after the assert sweep). Parallelizable
to a sub-agent. Any regression → `docs/issues/2026-MM-DD-<project>-rc3-regression.md`,
fix in the consumer's tree (mabda's surface is rc.3-frozen).

### 5. `present` workload — DRM master story

`programs/native_present_e2e` (120-frame animated gradient) is
still gated on DRM master. The rc.3 6h soak ran `--workload=all`
(compute + wgpu + render) without `present` because the box was
in a normal desktop session and logind held master (see
[[project_phase_d_master_logind_blocker]]).

Tasks:

- [ ] Decide whether the rc.4 24h run includes `present`. Two
  paths:
  - Run from a tty / kiosk session — minimal samvada wiring, but
    the dev box loses desktop for 24h.
  - Wire through a samvada consumer — exercise the
    `gpu_surface_configure_native_logind` path, but adds another
    moving piece to the soak.
- [ ] If yes: add a fourth workload to `scripts/soak.sh` (slot is
  already wired — `--workload=present` works, just not bundled
  into `all`).

---

## The 24h gate (3.0.0 GA candidate)

**Workload**: `scripts/soak.sh --workload=all --stop=24h` — the
recipe that cleared the 6h gate, run 4× longer. Bundle is
`compute + wgpu + render` in parallel by default. Adding
`present` is an open decision per investigation item #5.

The soak script handles:

- Per-workload iteration counters + status files in `iterations/`
- Dynamic CSV columns (`iters_<name>`)
- Exponential-then-15-min checkpoint cadence (~96 samples/24h —
  aligned with the rc.4 target stated below)
- amdgpu/drm dmesg baseline + final + diff-on-fail
- Resident-set sampling (`statm`)
- Per-iteration self-asserts inside each program (no external
  oracle needed; each program returns non-zero on readback / pixel
  / frame-count mismatch)

**Sampling cadence** (built into the script):

- Exponential early ramp catches infant mortality (1s / 2s / 4s /
  8s / 16s / 32s / 60s / 120s / 240s / 480s / 960s / 1920s).
- 15-min cadence through the rest of the 24h window (~96 samples
  total — matches the rc.4 target).

**24h pass criteria**:

- No monotonic allocator growth (>5% RSS drift over 24h on any
  process — rc.3 sat at 0% over 6h, so any drift is genuinely
  new under sustained load).
- Zero GPU readback corruption (compute writes 0xDEADBEEF every
  iter; first mismatch ends the run).
- Zero new `amdgpu` errors in dmesg (script captures the baseline
  + final; diff written only on FAIL).
- Zero present stalls (only applicable if `present` is bundled
  into the run — see investigation item #5).

**If 24h clean → 3.0.0 GA cut authorized.** Promote to GA:

1. Final closeout matrix from CLAUDE.md (re-run P(-1) pass).
2. VERSION 3.0.0-rc.3 → 3.0.0.
3. Tag `3.0.0`. CI release pipeline produces the GA artifacts.
4. CHANGELOG `[3.0.0]` section consolidates the rc.1/rc.2/rc.3/
   rc.4 detail, with forward-pointers from each rc entry.

**The 24h gate does not require the 72h gate to pass first.** GA
ships on 24h-clean by design — the 72h window is a follow-on
observability run, not a release gate.

---

## The 24h → 72h observability run (3.0.x patch backlog)

After GA cuts, **the soak does not stop**. The same workload
continues running through 72h on the same box, on the **shipped
3.0.0 bundle**. Invocation: `scripts/soak.sh --workload=all --stop=72h`
(or re-target the existing 24h run by editing `--stop` mid-flight
if the script supports that — check before relying on it).
Anything that surfaces in the 24h → 72h window gets filed as:

- `docs/issues/2026-MM-DD-3-0-0-soak-<symptom>.md`
- triaged into the 3.0.1 / 3.0.2 / 3.0.3 patch stream depending on
  severity.

**Why not block GA on 72h?** The 24h gate is the *responsible*
shipping line — past 24h, the marginal information we gain is
real but small, and the cost of holding GA back is large
(consumers waiting on the rc.4 bundle to flip). The 3-day
window is **observation, not gating**. We commit to running it
and triaging what surfaces, but we don't refuse to ship 3.0.0 on
its strength.

**Expected outputs of the 72h window**:

- Long-tail allocator behaviour data (saw-tooth shape over many
  hours — rc.3's 6h flat-line is the starting point).
- Drift in the dual-backend bench numbers if any (Tier 3 work —
  bench-history.csv gains a `soak_72h` column candidate).
- KMS / page-flip statistics over a multi-day window (only if
  `present` ends up bundled per investigation item #5).
- dmesg / journalctl signal-to-noise on the amdgpu driver
  surface under sustained mabda load.

These feed back into:

- 3.0.1+ patch backlog (anything actionable).
- v3.1 planning (anything that suggests a profile or surface
  needs refinement).
- The eventual rc.5 / 3.1.x rc cadence (the soak strategy
  pioneered here is meant to outlive 3.0.0).

---

## If rc.4 regresses inside 24h

- Fix in tree. File the bug as
  `docs/issues/2026-MM-DD-rc4-<regression>.md`, link from
  CHANGELOG.
- Re-cut rc.4 (rc-number doesn't increment for fix cycles).
- Decision: does this regression invalidate rc.3 too? If so,
  fall back to rc.3 + 6h re-gate before rc.4 restart. If the bug
  is purely 24h-surface (e.g., long-running counter overflow),
  rc.3 stays valid.
- Soak result tree from the failed run (`docs/handoff/soak-<ts>/`)
  is the primary evidence — `dmesg.diff` only writes on FAIL, so
  if it's present, that's where the kernel-side signal lives.

---

## What's NOT in rc.4 scope

- Anything that surfaces only after 72h. That's a 3.0.x patch
  candidate by construction; we are explicitly time-boxing the
  release gate.
- New feature work. rc.4 is the last hardening cut.
- WGSL → GFX9 lowering (v3.x).
- NVIDIA / Intel native paths (v4.0 / v5.0).
- Anything in the 6.0.0 toolchain that isn't a strict
  source-compat shim. If 6.0.0 introduces new lints or fmt rules
  that flag rc.3-clean code, the response is *adjust the call
  sites*, not *refactor*. rc.4 is hardening, not modernization.

---

## Tracking

- File this doc as the reference in any rc.4 / 3.0.0 GA PR.
- When the 24h gate clears, mark `[x]` here, snapshot the
  monitoring logs into `docs/handoff/2026-MM-DD-rc4-24h-clean.md`,
  cut 3.0.0.
- 72h observation continues against the GA bundle; outputs feed
  `docs/handoff/2026-MM-DD-3-0-0-72h-observation.md`.
- VERSION moves 3.0.0-rc.3 → 3.0.0-rc.4 once the assert sweep
  lands and the rebuilt binaries clear a fresh `make test`; →
  3.0.0 at the 24h gate pass.
