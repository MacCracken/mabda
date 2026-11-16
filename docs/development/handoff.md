# Agent hand-over — mabda (2026-11-16)

For the next agent picking this repo up. Read this, then `CLAUDE.md`, then the
[roadmap](roadmap.md) (every planned release, pinned and in order). Keep this file current
when you hand over again; `CHANGELOG.md` stays the record of what shipped.

## Where things stand

- **Released: v4.3.1** (tag `4.3.1` at `979222d`, pushed by the maintainer). It shipped the render
  tier's MB2–MB5 on top of 4.3.0's MB1: everything soorat's GPU half asked for
  (`issues/archived/2026-11-15-render-tier-for-geometry-consumers.md`), every milestone HW-verified.
  Audit: `docs/audit/2026-11-16-4.3.1-audit.md`.
- **v4.4.0 in progress, uncommitted** (VERSION still 4.3.1; CHANGELOG `[4.4.0] — in progress`):
  **the scratch-buffer contract** has landed. Every compiler buffer travels with its byte length
  (see "The compiler's scratch-buffer contract" below). The rest of v4.4.0 is open; the next
  roadmap item is the compiler limits v4.1.0 left named, then N9.
- **Pins:** cyrius **6.7.18** (`cyrius.cyml`; track the latest upstream `VERSION` when you bump),
  wgpu-native **v29.0.1.1**, launcher **83 slots**, deps samvada 1.0.1 / chitra 1.0.3.
- **Next release: v4.4.0 — compilers + NVIDIA**: N9 (SPIR-V → SASS; ranga is waiting), the
  scratch-buffer contract, the compiler limits v4.1.0 left named, NVIDIA block-linear scanout, the
  seat-only HW closeout, the legacy surface-acquire leak, ranga's consumer run. Then **v4.5.0 —
  textures + language**: texture encoding, compressed sRGB ids, render-graph aliasing, the cyrius
  6.7 language minors (S2 / S3 / S4 / M6). Then v5.0 (native rendering beyond the vertex-less
  subset; Intel; NVIDIA wgpu retires) and v5.1. Don't split these into more minors without asking.

## The maintainer's standing rules

These come from `CLAUDE.md` and corrections the maintainer has made. Break none of them.

- **Never commit, push or tag.** The maintainer does all git. Never use the `gh` CLI (curl the GitHub
  API if you must).
- **Scope is theirs.** Don't defer, cut or re-scope roadmap work silently. When something can't be
  done (a hardware block, say), name it, track it with a pointer (roadmap / issue / CHANGELOG), and say so.
  Never call deferred work done.
- **A new launcher slot (`deps/wgpu_main.c`) needs their OK.** Every slot forces every consumer to
  rebuild its launcher copy. 4.3.0's "final" 81-slot revision missed two; 4.3.1 added them (83) with
  approval.
- **Any gate the maintainer runs for you must write a log first.** Seat-only gates (a Wayland session,
  a VT with DRM master, `chew`'s console) run where you can't see the terminal. Make the target write
  `build/logs/<gate>-<timestamp>.log` with date, host, kernel, commit and session before you ask, the way
  `test-surface-wayland-e2e`'s recipe in the Makefile does. Run it once yourself, even if it only SKIPs,
  to see the log appear. Then read the log and cite it. Asking without that lost a 12/12 run's output
  on 2026-11-16, and the maintainer was rightly furious.
- **Repair red gates; never offer to retire them.** Don't spring out-of-scope multiple-choice
  questions mid-task. Don't offer `/schedule`.
- **Track the latest cyrius** for the pin. C interop is low priority: Cyrius replaces C.

## How to verify a change (what CI runs, plus the hardware)

```bash
rm -rf lib && mkdir lib && cyrius deps        # lib/ is a real directory, never a symlink
cyrius build programs/smoke.cyr build/mabda_smoke
make test                                     # every tests/tcyr/*.tcyr; gate on exit codes
./scripts/count-test-assertions.sh            # the honest count (25 suites, 6880 at 4.3.1)
cyrius vet programs/smoke.cyr
cyrius distlib                                # run twice: dist/mabda.cyr must not change
./scripts/version-check.sh
make check-stack-array-sizing check-program-write-lengths
./scripts/check-compiler-buffer-sizing.py
make check-count-script check-fuzz-logs check-make-dry-run fuzz example-link test-raw-include
cyrius bench tests/bcyr/mabda.bcyr
```

Lint, deferrals and fmt are **per file**, over `src/*.cyr programs/*.cyr tests/tcyr/*.tcyr
tests/bcyr/*.bcyr fuzz/*.fcyr`:
- `cyrius lint f` must print no `warn`. The 120-column limit counts **bytes**: — and → are 3 each.
- `cyrius lint --strict-deferrals f` must exit 0. It over-matches "deferred", "not yet" and
  `GPU_ERR_NOT_IMPLEMENTED`: reword the first two, mark the identifier `#skip-lint`.
- `cyrius fmt f --check` is judged by its **exit code**. Never `cyrius fmt f > f`: since 6.5.28 that
  rewrites in place and destroys the file.

Hardware:
- **Cezanne (this box, `archaemenid`, AMD Radeon Vega / RADV).** `make test-gpu` (9 wgpu programs, 112
  PASS at 4.3.1) and `make test-native-all` (72 pass, 2 skip for DRM master). The agent's session is a
  tty: no Wayland compositor, no DRM master. Native compute and render work here; only the KMS
  surface gates need master.
- **`chew` (`ssh chew`, NVIDIA GTX 1660 SUPER / TU116, nouveau + NVK).** Never dirty `~/Repos/mabda`
  there. Clone it to `~/mabda-verify-<x>` and tar the local tree over (`src tests programs scripts
  Makefile cyrius.cyml cyrius.lock VERSION deps/wgpu_main.c`, plus `deps/wgpu-native/{include,lib/*.a}`
  the first time). Then `export PATH=$HOME/.cyrius/bin:$PATH` and `rm -rf lib && mkdir lib && cyrius deps`.
  cyrius 6.7.18 is in its `~/.cyrius/versions`; a newer pin needs its version dir tar'd over first.
  `make test-gpu` runs on NVK; the 13 masterless `test-nvidia-*` gates run over ssh; the 5 card-node
  gates need a console. `nvcc` is at `/opt/cuda/bin` (the SM75 oracle for N9).
- **Seat-only gates** (the maintainer runs them, logged):
  - `test-surface-wayland-e2e`, from their Hyprland session. 12/12 at 4.3.1:
    `build/logs/surface_wayland_e2e-20261116-092215.log`.
  - The DRM-master KMS gates and the logind run, from a VT (v4.4.0).

Consumers:
- **soorat** — probes in `soorat/scripts/probes/gpu/`. Build against this checkout with
  `MABDA_DIST=/home/macro/Repos/mabda/dist/mabda.cyr sh build.sh p6_mb1 p6_mb25 p8_gpu_frame`, then run
  `sh run_p6_mb1.sh`, `sh run_p6_mb25.sh` and `./build/p8_gpu_frame 100000 full`. Logs go to
  `build/logs/`. The probe files are **uncommitted in soorat's tree** (`p6_mb1.cyr`, `p6_mb25.cyr`,
  `run_p6_mb*.sh`, the `build.sh` `MABDA_DIST` switch); soorat's git is the maintainer's.
- **ranga** — its four / six GPU suites were run against 4.3.0. For 4.3.1 only the fold
  name-collision scan ran (0 shared function names with ranga or soorat); the full run is in v4.4.0.

## Map of the render tier (4.3.x)

- Public: `src/gpu_types.cyr` (constant names), `error_scope.cyr`, `gpu_bind.cyr`,
  `gpu_pipeline.cyr`, `gpu_draw.cyr`, `gpu_target.cyr` (targets, the pass descriptor, views, copies,
  readback), `gpu_compute_tier.cyr`, `surface_v3.cyr` (Wayland surfaces).
- wgpu: `backend_wgpu.cyr` (the **80-byte texture record** every texture, target, view and surface frame
  shares: layout at "The wgpu texture record"), `backend_wgpu_rtier.cyr` (the per-context render-state
  scratch, `_WGPU_RS_*`), `backend_wgpu_target.cyr`, `backend_wgpu_ctier.cyr`, `backend_wgpu_surface.cyr`.
- Native: `backend_native_rtier.cyr` (the vertex-less RGBA8 subset).
- Backend slot table: `src/backend.cyr` (632 bytes; blocks documented above the constants).
- Tests: `tests/tcyr/render_tier.tcyr` and `target_tier.tcyr`. They mock the wgpu fn table and copy
  each descriptor during the call, so the bytes are checked against webgpu.h v29.
- HW: `programs/render_tier_e2e.cyr`, `target_tier_e2e.cyr`, `compute_tier_e2e.cyr`,
  `error_channel_e2e.cyr`, `launcher_slots_e2e.cyr`, `surface_wayland_e2e.cyr`,
  `native_render_tier_e2e.cyr`, `nvidia_render_tier_e2e.cyr`.

## The compiler's scratch-buffer contract (4.4.0)

- Every SPIR-V → GFX9 stage that writes a caller buffer takes `(buf, len_in_bytes)` and derives its
  capacity as `len / RECORD`. No stage takes a record count beside a bare pointer any more:
  `mir_mod_init(m, vals, vals_len, instrs, instrs_len, ptrs, ptrs_len, blocks, blocks_len, id_bound)`;
  the table builders `(p, n, out, out_len)` read `id_bound` from the header; the compile context is
  136 bytes, filled by `gfx9_cc_buf(&cc, CC_X, &buf, N)`; `gfx9_compile(cc, spirv, n, isa, isa_len, desc)`.
- Write each length as the literal beside its `var buf[N]`. `scripts/check-compiler-buffer-sizing.py`
  checks every one, and it fails a local buffer passed with a non-literal length. The exception is a
  plain name that the same function bounds first with `if (name > M)`, where `M <= N`.
- **N9 must follow it.** The SM75 path will reuse the SPIR-V parse and the MIR. Give its isel /
  regalloc / encoder the same `(buf, len)` shape and add them to the gate's `CONTRACT` table.
- `_native_compile_spirv_isa` is the pure-CPU production compile frame, and the CPU suite drives it.
  An NVIDIA counterpart should be split the same way.

## Gotchas this arc found

- Take every webgpu.h v29 layout from `offsetof`, never from a comment. The pre-v29 writers were
  wrong (4.3.0 MED-1/2). `WGPUOptionalBool` is False 0, True 1, Undefined 2. `WGPUTextureAspect` All is
  1, not 0.
- wgpu-native treats submitting an invalid command buffer as **fatal**. The launcher's finish shim
  refuses it (returns NULL) and the submit shim skips NULL. A refused copy therefore means "nothing
  happened": check `finish`'s result, never read the destination afterwards (4.3.1 MED-1).
- Stack descriptors (`var d[80]`) need `&d`. Mocks must copy a descriptor during the call: it is dead
  after the call returns.
- Bump-allocating per call is a leak (no free). Per-frame paths use the render-state scratch or the
  stack, and the tests assert 0 bytes of heap growth over 1000 frames.
- wgpu RSS grows about 13 MiB over the first ~100k submissions of any kind, then stays flat. Measure
  leaks after a warm-up.
- RADV contracts f64 `a*b + c` into an FMA (allowed without `NoContraction`), so expect 1 ULP. The
  native GFX9 compiler never fuses.
- Cyrius reserves `mod` and `dyn` as identifiers.
- Test fake function tables must be `WGPU_FN_COUNT x 8` bytes (664 at 83 slots).
- A new `src/` module goes in **both** `src/lib.cyr` and `cyrius.cyml`'s `[lib].modules`, or distlib
  silently leaves it out.
- Before 4.3.1, `backend_get_slot(0, off)` read address `off`. It is null-safe now, but a dispatcher
  must still check for slot 0.

## Open, and where it is tracked

Everything is in [the roadmap](roadmap.md): v4.4.0 (compilers + NVIDIA + the seat-only closeout),
v4.5.0 (textures + language), v5.0 (native rendering, Intel, NVIDIA wgpu retirement), v5.1, and
"blocked upstream" (WASM, `trait Backend`, samvada's Windows `sys_recvmsg`, cyrfmt's file cap).
