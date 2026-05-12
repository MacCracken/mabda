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

**Status**: planning (2026-05-12). Successor to
[`3-0-rc-3-punchlist.md`](3-0-rc-3-punchlist.md). rc.4 starts
once rc.3's 6-hour gate is clean.

---

## Prerequisites

- rc.3 cut clean (6h soak passed, tag `3.0.0-rc.3` exists).
- All P(-1) gates from rc.3 still green (re-run a tight loop
  before starting rc.4's clock: `make test`, distlib regen,
  version-check, smoke build).
- No in-flight diffs against rc.3. The soak runs against a tagged
  immutable cut.

---

## The 24h gate (3.0.0 GA candidate)

**Workload**: same as rc.3's 6h profile, but at 4× the duration.

- `programs/render_graph_e2e` (wgpu) on loop.
- `programs/native_compute_store` on loop.
- `programs/native_present_e2e` (HW+master gated; run from kiosk
  or via samvada-wired consumer).
- Optional: cycle through `programs/native_texture_e2e` and
  `programs/native_render_e2e` to exercise the GFX-ring
  dispatch path under sustained load.

**Sampling cadence**: every 15 min (less aggressive than rc.3's
5 min — over 24h that's 96 samples, plenty of resolution).

- `/proc/self/statm` for each child process — resident-set vs
  time, expect flat or sawtooth.
- `dmesg -w | grep -i 'amdgpu\|drm'` for ring resets / VM faults.
- Per-iteration assert: native compute readback = `0xDEADBEEF`.
- Per-iteration assert: present_e2e's gradient frame counter
  advances (i.e., no page-flip stall).

**24h pass criteria**:

- No monotonic allocator growth (>5% drift over 24h on any
  process).
- Zero GPU readback corruption.
- Zero new `amdgpu` errors in dmesg.
- Zero present stalls (gradient frame count = uptime/16.67ms
  ± slack for samvada signal pumping).

**If 24h clean → 3.0.0 GA cut authorized.** Promote to GA:

1. Final closeout matrix from CLAUDE.md (re-run P(-1) pass).
2. VERSION 3.0.0-rc.3 → 3.0.0.
3. Tag `v3.0.0`. CI release pipeline produces the GA artifacts.
4. CHANGELOG `[3.0.0]` section consolidates the rc.1/rc.2/rc.3/
   rc.4 detail, with forward-pointers from each rc entry.

**The 24h gate does not require the 72h gate to pass first.** GA
ships on 24h-clean by design — the 72h window is a follow-on
observability run, not a release gate.

---

## The 24h → 72h observability run (3.0.x patch backlog)

After GA cuts, **the soak does not stop**. The same workload
continues running through 72h on the same box, on the **shipped
3.0.0 bundle**. Anything that surfaces in the 24h → 72h window
gets filed as:

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
  hours).
- Drift in the dual-backend bench numbers if any (Tier 3 work —
  bench-history.csv gains a `soak_72h` column candidate).
- KMS / page-flip statistics over a multi-day window.
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

---

## What's NOT in rc.4 scope

- Anything that surfaces only after 72h. That's a 3.0.x patch
  candidate by construction; we are explicitly time-boxing the
  release gate.
- New feature work. rc.4 is the last hardening cut.
- 6-consumer regression sweep — rc.3 owns it. rc.4 assumes the
  consumer surface is stable.
- WGSL frontend / NVIDIA / Intel — v3.x and beyond.

---

## Tracking

- File this doc as the reference in any rc.4 / 3.0.0 GA PR.
- When the 24h gate clears, mark `[x]` here, snapshot the
  monitoring logs into `docs/handoff/2026-MM-DD-rc4-24h-clean.md`,
  cut 3.0.0.
- 72h observation continues against the GA bundle; outputs feed
  `docs/handoff/2026-MM-DD-3-0-0-72h-observation.md`.
- VERSION moves 3.0.0-rc.3 → 3.0.0-rc.4 at this punchlist's
  start; → 3.0.0 at the 24h gate pass.
