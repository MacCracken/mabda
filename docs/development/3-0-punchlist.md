# Mabda v3.0 — Release Punch List

**Status:** Working document. Tick items off as they land.
**Date opened:** 2026-04-28
**Branch:** `v3`
**Roadmap reference:** [`roadmap.md` § v3.0](roadmap.md#v30--dual-backend-amd-native-added-alongside-c-path)

> Native compute dispatch verified end-to-end on AMD Cezanne (gfx90c)
> on 2026-04-28 — see
> [`docs/handoff/2026-04-28-session25-b4-verified.md`](../handoff/2026-04-28-session25-b4-verified.md).
> That is **the foundation**, not the release. v3.0 is roughly 15%
> complete by exit criteria; the bulk is still ahead. This punch list
> is the path from "compute store passes" to "v3.0 ships."

## Hard truths up front

Read these before sequencing.

- **WGSL → GFX9 ISA lowering is the single largest unknown.** Roadmap
  declares it in scope for v3.0; today the native path takes
  pre-compiled GFX9 bytes only. If lowering slips, v3.0's
  `examples/stdlib-consumer/` cannot run unmodified on `native` —
  which fails the exit criterion. Worth a sober design spike before
  committing to a v3.0 ship date.
- **Consumer cutover takes calendar time, not just code.** Even with
  all six consumers technically able to flip, "running native in
  production across a full release cycle" is what gates AMD wgpu
  retirement at v4.0. v3.0 ship just opens that window.
- **Tier 1 alone is multi-month.** Backend abstraction + Phase C
  native (texture + render) + Phase D native (surface) + WGSL
  lowering is the bulk of the work. The verified compute dispatch is
  the foundation, not the cap.
- **Linux + AMD only.** v3.0's native backend covers AMD on Linux.
  NVIDIA / Intel / macOS / Windows consumers continue on `wgpu`.
  Don't accept scope-creep that pretends otherwise.

## Tier 1 — Code completeness

The native backend currently proves **compute dispatch only**. Roadmap
exit criteria require all four integration shapes (`phase0`,
`compute_e2e`, `render_e2e`, `render_graph_e2e`) plus
`examples/stdlib-consumer/` to pass under **both** backends on AMD.

### Backend abstraction layer (Phase B5 — proposal already drafted)

Proposal: [`docs/proposals/v3-backend-interface.md`](../proposals/v3-backend-interface.md).

- [ ] `src/backend.cyr` — 11-slot `Backend` struct + `BACKEND_KIND_*`
  constants + null-slot helpers + struct-layout assertions
- [ ] `src/backend_wgpu.cyr` — fill all 11 slots with wgpu calls;
  existing modules' wgpu wiring moves here
- [ ] Refactor `src/buffer.cyr`, `src/compute.cyr`, etc. to dispatch
  through `ctx->backend->slot`; public API surface unchanged
- [ ] `MABDA_BACKEND_KIND` compile-time constant in `src/lib.cyr`
- [ ] `GpuContext` grows from 32 → 40 bytes (backend ptr at +32);
  single greppable migration of every `alloc(32)`
- [ ] Smoke test: Cyrius-fn-to-Cyrius-fn `fncall` through a struct
  slot — a shape we haven't extensively exercised; do this **first**
  before committing to the layout

### Native backend slot fills (Phases C + D)

- [ ] `Backend.compute_dispatch` on native — lift the working
  `native_compute_store` flow into a reusable function. Today it's
  470 lines inline in the program; needs multi-dispatch per submit
  and BO ownership across calls.
- [ ] **Phase C — texture path on native.** `Backend.texture_create
  / write / release`. Tile / format / mip / view work; AMD-specific
  surface layouts.
- [ ] **Phase C — render pipeline + render pass on native.**
  `Backend.render_pipeline_create / render_pass_begin / end / draw`.
  Graphics ring, vertex fetch, MRT, depth/stencil. Substantial.
- [ ] **Phase D — surface + present on native.** DRM/KMS scanout,
  page flips, vblank sync. Required by soorat / aethersafta.
- [ ] **WGSL → GFX9 ISA lowering.** Roadmap-mandated for v3.0. Today
  native takes pre-compiled ISA only; consumers ship WGSL. Without
  lowering, no v2.x consumer can run on native unmodified.
- [ ] Native equivalents of `phase0`, `compute_e2e`, `render_e2e`,
  `render_graph_e2e` — preferably by making the existing programs
  backend-agnostic (dispatch through `Backend`) rather than
  duplicating

### Phase B.4 follow-ups (small)

Captured in the B.4-verified handoff; clean up before the abstraction
work or you'll inherit them later.

- [ ] Tighten BO page perms from `R | W | X` everywhere back to
  per-BO minimums + add a regression test that catches the
  narrowing failure mode
- [ ] Memory-model design: how does native BO management surface to
  the `Resource` tracker (`src/resource.cyr`)?
- [ ] Error-model mapping: `errno` → `GpuErr` codes. The codes exist
  in `src/error.cyr`; the mapping doesn't. Pick a convention before
  the abstraction work locks it in.
- [ ] BO ownership lifecycle so `native_compute_store`-shaped flows
  can dispatch many times without leaking. Per-context IB ring
  buffer; per-context BO list.

## Tier 2 — Integration & regression

- [ ] `examples/stdlib-consumer/` builds and runs under **both**
  backends on AMD
- [ ] Six-consumer regression sweep — soorat / rasa / ranga / bijli /
  aethersafta / kiran all build and run on `wgpu` (default
  unchanged); zero behavioural changes
- [ ] At least one consumer (likely soorat or compute-heavy bijli)
  runs a CI matrix entry under `native` on AMD hardware
- [ ] All 387 CPU assertions still pass — they're backend-agnostic
  and should be untouched, but verify after every Tier 1 step
- [ ] All 13 GPU benches pass under `native` on AMD (today they only
  run under `wgpu`)

## Tier 3 — Performance evidence (the v3.0 "story")

The whole reason v3.0 is *dual* and not *swap* is the bench matrix —
without it, v3.0 has no measurement story.

- [ ] `bench-history.csv` schema — add `backend` column
  (forward-compatible; existing rows get `wgpu` retroactively)
- [ ] `make bench-gpu` runs the 13-bench suite under each backend
  on AMD
- [ ] Run all four matrix cells: 13 benches × {wgpu, native} ×
  {pre-5.6.x, post-5.6.x}
- [ ] `docs/benchmarks-rust-v-cyrius.md` refreshed to tell the
  four-quadrant story (cyrius codegen wins isolated from backend
  architecture wins)
- [ ] FFI overhead per-call number landed — the C-launcher cost the
  native path eliminates, with method
- [ ] Submit-to-completion timing for `Backend.compute_dispatch`
  integrated through `src/profiler.cyr` instead of the hand-rolled
  `mono_ns()` print in `native_compute_store`
- [ ] Honest perf framing — per the
  `feedback_honest_perf_framing` memory: report numbers as
  scale-labeled baselines, not "parity with X" until Phase D + 5.6.x
  give fair comparisons

## Tier 4 — Documentation

- [ ] `CLAUDE.md` updated for v3 architecture — currently still
  describes v2.5.0 single-backend; needs Backend layer + AMD-only
  native scope language
- [ ] `docs/stdlib-integration.md` — backend selector documented
  (consumer-facing)
- [ ] Consumer migration guide (1-pager) — how to flip an existing
  v2.x consumer to `BACKEND_KIND_AMD`
- [ ] `@public` surface audit per ADR 005 — confirm no `Backend`
  types leak through the boundary; ADR 005 audit script (if it
  exists) clean
- [ ] All 6 ADRs cross-referenced consistently — pass through ADR
  001-006 and check links / "Related" lines
- [ ] `CHANGELOG` `[3.0.0-dev]` running diary becomes a single dated
  `[3.0.0] — YYYY-MM-DD` section with the full delta vs 2.5.0
- [ ] `programs/native_compute_store.cyr` diagnostic scaffolding
  (`0xC0FFEE12` marker, IB hex dump, mono_ns timing) — decision: keep
  in-tree as a diagnostic program, move to a `programs/diagnostics/`
  subdir, or strip down to a minimal example. Pick one.

## Tier 5 — Release engineering (CLAUDE.md "Closeout Pass")

Ordered roughly the way they'll need to run.

- [ ] **P(-1) audit pass** — last one was
  [`docs/audit/2026-04-19-audit.md`](../audit/2026-04-19-audit.md)
  against 2.2.0. Required before any minor/major bump per CLAUDE.md.
  v3.0 audit will need to cover specifically: PM4 emitter input
  validation, every `syscall()` return check in
  `src/backend_native.cyr`, integer overflow on workgroup math,
  BO-perm tightening, label-string handling, `fncall*` discipline
  on the new abstraction layer.
- [ ] `VERSION` bump `2.5.0` → `3.0.0` + `cyrius.cyml` cross-check
  via `scripts/version-check.sh`
- [ ] Toolchain pin decision — currently `5.7.23` (bumped during
  Session 25). Confirm this is the release target, or pin to
  whatever 5.6.x stable shipped most recently. Document the choice.
- [ ] `cyrius distlib` regenerate — `dist/mabda.cyr` will grow
  significantly (Backend layer + both backend implementations); CI
  gate must not drift
- [ ] Lint / fmt / vet clean across the **whole repo** (not just
  touched files) — `cyrius lint src/*.cyr programs/*.cyr`,
  `cyrius fmt --check`, `cyrius vet programs/smoke.cyr`
- [ ] `.github/workflows/release.yml` tag filter + version-verify
  still work against the `v3.0.0` shape
- [ ] Soak window — run the new bundle in CI for N days (suggest
  ≥7) against the consumer integration tests without regression
- [ ] Pre-release: tag `v3.0.0-rc.1`; let consumers smoke-test
  before the final tag
- [ ] After ship: update `CHANGELOG`'s `[Unreleased]` section to
  empty; archive v3 punch list as
  `docs/development/archive/3-0-punchlist.md`

## Tier 6 — Forward tracking (file now, work later)

These keep the v4.0 / v5.0 commitments visible from the v3.0 ship.

- [ ] **ADR 007 placeholder** for NVIDIA native (v4.0). Status:
  `Deferred to v4.0`. Single page; documents the
  nouveau-vs-nvidia.ko design-spike question, SASS/PTX choice. The
  file's existence keeps the commitment visible from the v3.0 ship.
- [ ] (Optional) ADR 008 placeholder for Intel native (v5.0) — same
  shape. Could also wait until v5.0 design opens.
- [ ] Issue tracker entries for known v3.0-deferred items: BO perms
  tightening, multi-dispatch IB ring, `Resource` tracker
  integration. Anything that the audit flags but doesn't block ship.

## Recommended sequencing

This is the smallest-bites-first order. Each step is verifiable
end-to-end before the next; nothing past Step 4 starts until Step 4
clean.

1. **Backend abstraction smoke test.** Land `src/backend.cyr` with
   the 11-slot struct + layout assertions. Add a single Cyrius-fn
   that registers a stub backend with one slot and proves the
   `fncall` through a struct slot works. Smallest possible; unblocks
   everything.
2. **`backend_wgpu.cyr` filling all slots.** Refactor `src/buffer.cyr`,
   `src/compute.cyr`, etc. to dispatch through `ctx->backend`. Run
   all 387 CPU assertions + the four integration programs under
   `wgpu` and prove zero regression. This is the load-bearing
   abstraction-validation step — if the abstraction breaks anything
   here, fix it before going further.
3. **Lift `native_compute_store` into `Backend.compute_dispatch`.**
   First native slot lit. Native equivalent of `compute_e2e` runs.
4. **Phase B.4 follow-ups** (BO perms, error-model mapping, IB ring,
   Resource integration). Cleans up the foundation before the bigger
   work. Don't skip.
5. **Phase C native** (texture + render pipeline). Biggest single
   chunk; budget multiple weeks. Probably split into 5a (texture),
   5b (render pipeline + render pass), 5c (render_graph replay on
   native).
6. **Phase D native** (surface). Gated on Phase C.
7. **WGSL → GFX9 lowering** — can be parallel to Phase C/D once the
   SPIR-V vs WGSL frontend choice is made. May want to spike this
   at step 4 instead, depending on the answer.
8. **Bench harness + matrix + perf docs** once all four programs
   pass under both backends.
9. **Tier 4 documentation** — runs alongside everything; finalize
   at end.
10. **Tier 5 release engineering** — P(-1) audit, version bump,
    distlib regen, soak, RC, ship.

## Status snapshot (update as we go)

| Tier | Status | Last touched |
|------|--------|-------------|
| Tier 1 — Code completeness | not started | — |
| Tier 2 — Integration & regression | not started | — |
| Tier 3 — Performance evidence | not started | — |
| Tier 4 — Documentation | partially: handoff + CHANGELOG `[Unreleased]` written; CLAUDE.md / stdlib-integration / migration guide pending | 2026-04-28 |
| Tier 5 — Release engineering | not started | — |
| Tier 6 — Forward tracking | not started | — |

## Notes / decisions captured along the way

(Append-only log; date each entry. Decisions made during the
punch-list work that future-you should know about.)

- **2026-04-28** — Punch list opened. Phase B.4 verified on AMD
  Cezanne; native compute dispatch foundation in place. Backend
  interface proposal drafted (`docs/proposals/v3-backend-interface.md`),
  not yet approved.
