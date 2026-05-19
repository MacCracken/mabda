# mabda 3.0.0-rc.3 punchlist

**Goal**: short-soak proof. Burn the rc.2 bundle on the dev box
for **up to 6 hours** with the consumer programs running
continuously. Anything that regresses inside that window gets a
rc.3 cut before we sink the multi-day rc.4 budget. Anything that
surfaces only after 6h is rc.4's problem.

**Status**: planning (2026-05-12). rc.3 was conceived as part of
splitting the original "3-day soak" closeout from
[`3-0-rc-2-punchlist.md`](3-0-rc-2-punchlist.md) into two cuts.

**Gate**: 6-hour clean burn-in. If clean → tag `3.0.0-rc.3`, move
to [`3-0-rc-4-punchlist.md`](3-0-rc-4-punchlist.md). If anything
regresses, fix in tree, re-cut rc.3, restart the 6h clock.

---

## Toolchain prerequisites (rc.2 → rc.3)

### Cyrius pin bump — `5.7.48 → 5.11.28`

- **File**: `cyrius.cyml` (the `cyrius = "..."` line).
- **Done in tree 2026-05-12**. Companion sweep updated the cyrius
  references in CLAUDE.md, docs/guides/{integration,usage,render-graph,
  native-migration}.md, and the toolchain-issues consolidator.
- **Why bumped**: tracking upstream cyrius per the
  `feedback_cyrius_pin_latest` memory rule. 4 minor versions of bug
  fixes + lint/fmt refinements accumulated since rc.2's 5.7.48 pin.
- **Test**: `cyrius build programs/smoke.cyr build/mabda_smoke`
  must link clean. `make test` must surface 1871 CPU asserts green
  across the three test files. `cyrius distlib` must regenerate
  `dist/mabda.cyr` diff-clean (or with only whitespace drift the
  distlib generator might have introduced — verify and re-stage).
- **Effort**: ~30 minutes for the verify pass (the bump itself is
  one line).

### samvada dep — already on `0.2.2`

- No change needed. `[deps.samvada] tag = "0.2.2"` in
  `cyrius.cyml`, local `~/.cyrius/deps/samvada/0.2.2/` resolved.
- **Test**: any consumer program that links
  `samvada/deps/samvada_main.c` should still build.

---

## Pre-soak closeout (P(-1) matrix per CLAUDE.md)

Run before starting the 6h soak clock. If any of these fail, fix
first.

1. `make test` — full 1871 CPU assertion sweep across
   `mabda.tcyr` + `mabda_v3.tcyr` + `mabda_v3_phase_d.tcyr`.
2. `cyrius bench tests/bcyr/mabda.bcyr` — 7 CPU benches, save CSV.
3. Per-file `cyrius lint src/*.cyr` and `cyrius fmt --check
   src/*.cyr` (loop, not bare repo-wide — see
   `feedback_cyrius_lint_fmt_per_file` memory).
4. `cyrius vet programs/smoke.cyr` clean.
5. `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean.
6. `./scripts/version-check.sh` passes (VERSION = 3.0.0-rc.3
   for the cut).
7. GPU integration (wgpu) — `make test-phase0`,
   `make test-compute-e2e`, `make test-render-e2e`,
   `make test-render-graph-e2e` all pass on the dev box.
8. GPU integration (native) — `make test-native-compute-store`,
   `make test-native-texture-e2e`, `make test-native-render-e2e`,
   `make test-native-kms-summary`, `make test-native-kms-modeset`,
   `make test-native-present-e2e` all pass.

## Re-audit

Run the security audit checklist from CLAUDE.md against the
toolchain-bump diff + any in-flight rc.3 fixes. File at
`docs/audit/YYYY-MM-DD-audit.md`. Must surface 0 HIGH / 0 MED
new since 2026-04-30.

## Six-consumer regression sweep

Inherited from the rc.2 punchlist. Tier 2 ship work that has to
land before rc.3 cuts:

- `cd ~/Repos/<project> && cyrius deps && cyrius build
  programs/smoke.cyr build/<proj>_smoke` for each of:
  soorat, rasa, ranga, bijli, aethersafta, kiran-via-soorat.
- File any failures as
  `docs/issues/2026-MM-DD-<project>-rc3-regression.md`.
- Pass criteria: every consumer builds against the rc.3 bundle
  without modification.
- Parallelizable to a sub-agent — ~2-3h per consumer × 6 worst
  case; mostly walking work.

---

## The 6-hour soak

**Box**: the dev workstation, AMD Cezanne (gfx90c). Same hardware
that ran B.4 verification.

**Workload**: run the consumer programs in a loop with the
following backends represented:

- **wgpu path**: `programs/render_graph_e2e` on loop (forces
  encoder + submit churn through the v2.5 DAG).
- **native AMD compute**: `programs/native_compute_store` on
  loop (PM4 dispatch + readback every iteration).
- **native AMD present**: `programs/native_present_e2e` (the
  120-frame animated gradient — gated on DRM master being
  available; if the box isn't in a tty / kiosk session, run from
  a consumer wired through samvada).

**What to watch for**:

- **Bump-allocator growth.** MED-7+LOW-5 in rc.2 was the big
  cleanup; rc.3's soak should confirm no slow drip survived.
  Sample `/proc/self/statm` (or equivalent) every 5 min, plot
  resident-set vs time. Flat or slight saw-tooth = pass; monotonic
  growth = fail, file as rc.3 blocker.
- **GPU readback divergence.** Native compute writes
  `0xDEADBEEF`. Verify every iteration; first byte that doesn't
  match = TDR / scratch corruption / cache flush regression. Cut
  rc.3 immediately, do not continue.
- **KMS / page-flip stutter.** Present_e2e's gradient should
  advance every frame. Stuck frame = page-flip event lost or
  syncobj-wait regression. Same: cut rc.3.
- **dmesg noise.** Tail `dmesg -w` during the soak. New
  `amdgpu` errors (ring resets, VM faults) = treat as native
  backend regression.

**Effort**: 6h elapsed; minimal hands-on once running. Set up,
let it cook, audit the logs at the end.

---

## Exit (rc.3 → rc.4)

- All P(-1) gates pass.
- 6h soak clean (no monotonic allocator growth, no GPU readback
  divergence, no stutter, no new dmesg `amdgpu` errors).
- 6-consumer sweep all green.
- Tag `3.0.0-rc.3` cut, CHANGELOG `### Next` advanced to point at
  rc.4.

## If rc.3 regresses

- Fix in tree, file the bug as
  `docs/issues/2026-MM-DD-rc3-<regression>.md`, link from
  CHANGELOG.
- Re-cut rc.3 (the rc-number doesn't increment for fix cycles —
  rc.3 just absorbs the patch round until it passes 6h clean).
- Only when the 6h gate is clean does rc.4 work begin.

---

## What's NOT in rc.3 scope

- The 24h+ soak — that's rc.4 by construction.
- New audit-tracked features. rc.3 is hardening, not feature
  work. Anything new is v3.0.x or v3.1.
- WGSL → GFX9 lowering (v3.x).
- NVIDIA / Intel native paths (v4.0 / v5.0).
- Multi-GPU disambiguation in `gpu_surface_configure_native_logind`.

---

## Tracking

- File this doc as the reference in any rc.3 PR.
- When the soak passes, mark the gate `[x]` here, snapshot the
  resident-set + dmesg + readback logs into
  `docs/handoff/2026-MM-DD-rc3-soak-clean.md`.
- VERSION moves 3.0.0-rc.2 → 3.0.0-rc.3 at the cut.
