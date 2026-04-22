# Cyrius 5.6.x — Mabda Optimization-Request Log

> Living document. Accumulates codegen / optimization patterns mabda
> hits during v3 prep that the cyrius **O1–O6 arc (v5.6.0–v5.6.5)**
> may not cover. When 5.6.x opens each phase, items in this file
> become upstream issues filed against the cyrius repo.
>
> **Not** a bug log. Items here are valid-but-slow patterns where a
> targeted compiler improvement would materially move a mabda or
> consumer benchmark.

## Scope reminder — what 5.6.x already covers

Per `../cyrius/docs/development/roadmap.md`:

- **v5.6.0 O1** — instrumentation + FNV-1a (measurement baseline)
- **v5.6.1 O2** — peephole
- **v5.6.2 O3** — IR-driven passes
- **v5.6.3 O4** — linear-scan regalloc
- **v5.6.4 O5** — maximal-munch (tile walker; RISC-V v5.7.0 inherits)
- **v5.6.5 O6** — slab (measurement-gated, may skip)

Anything that maps cleanly onto peephole / IR / regalloc / tile walker
is already on the track. This file is for **gaps** — specific patterns
in mabda / consumer code where the generic passes likely won't land
the win, OR where a small additional pass would 10×+ a specific hot
path.

## How to add an entry

```markdown
### [YYYY-MM-DD] Short descriptive title

- **Site**: `src/file.cyr:lineno` (or `programs/...`, `deps/...`)
- **Pattern**: code snippet or description of what cyrius emits
- **Measured impact** (optional): bench name + current ns, rough
  estimate of headroom if optimized
- **Suggested optimization class**: peephole | IR | regalloc | tile |
  inlining | escape analysis | alloc-site specialization | FFI |
  other (name a hypothetical new pass)
- **Maps to existing O-phase?**: yes (O2/O3/...) / no / partial — if
  partial, what's left that 5.6.x won't catch
- **Evidence this is a gap**: why the existing phases likely won't
  catch it (e.g., "O2 peephole works on emitted instructions; this
  needs source-level knowledge that a given alloc never escapes")
```

## When to file upstream

- When 5.6.0 ships → file items tagged `O1` / measurement (likely
  none in mabda — these are cyrius-internal).
- When each subsequent O-phase ships → re-run the gap analysis and
  file remaining items tagged against the next phase.
- Items explicitly marked "not covered by any O1–O6 phase" get filed
  as new-pass proposals against `v5.7.x+` or v5.9.x (the "GPU / game
  performance pass" window hinted at in the cyrius roadmap).

## Open items

_(none yet — this log opens at v3.0 branch cut, 2026-04-21)_

## Filed upstream

_(none yet)_

## Candidate areas to watch during v3 work

These are priors — places where mabda hits the codegen hard. No
items filed yet; entries go in **Open items** above once we have a
concrete site + pattern.

1. **FFI call-site specialization.** `fncall1/2/5/N` into wgpu-native
   is on the hottest path. The existing impl goes through `lib/fnptr.cyr`
   indirection. Specializing known-target `fncall`s (callee known at
   cyrius-compile time) to a direct `call` would skip a register load
   and an indirect-branch predictor entry. Likely not covered by O2
   peephole (source-level fact); candidate for O3 IR.

2. **Struct-packing helper inlining.** `wgpu_descriptors.cyr` builds
   packed descriptors via alloc + store64 / store32 at fixed offsets,
   then passes the pointer to `fncall2`. The alloc never escapes
   beyond the call. Escape analysis + stack allocation would remove
   the heap trip — high-frequency during render-graph encode. Not in
   the O1–O6 spec as-listed; candidate for a new escape-analysis
   pass.

3. **`color_lerp` (55 ns, 5× Rust).** The current non-inlined codegen
   heap-allocs temporaries. Bench is in `tests/bcyr/mabda.bcyr`.
   Likely helped by O3 IR + inlining; if not, file as an O3 tuning
   request with this bench as the regression case.

4. **`profiler_frame_cycle` (2.6 µs, Rust-beating? — check).** Same
   heap-alloc pattern in the EMA / history ring buffer. Aggregates
   multiple small allocs per frame tick; escape analysis lifts them.

5. **Bounds-check elision on `load64` / `store64` after provable
   alloc size.** Many mabda modules `alloc(N)` then do N/8 fixed
   offsets. If the compiler can prove the offset < N, the bounds
   check is dead. Classic O3 territory but worth confirming it lands.

6. **Render-graph hot paths.** `src/render_graph.cyr` toposort +
   encode is per-frame; profile under the dual-backend bench harness
   (once built) and file any gaps.

7. **`fncall6` and struct-by-value FFI.** Already known-broken; the
   existing fix is C shims. If a compiler-side fix (proper Win64 /
   SysV ABI handling for fncall6+ with struct-by-value) lands, the
   shim layer collapses — big simplification, not a bench win but a
   maintenance win. Track here as a capability request rather than
   a perf request.

These candidates get promoted to **Open items** once we have (a) a
specific `file:line`, (b) a concrete pattern / codegen observation,
and (c) ideally a measurement.
