# Agent hand-over — mabda (2026-11-17)

For the next agent picking this repo up. Read this, then `CLAUDE.md`, then the
[roadmap](roadmap.md) (every planned release, pinned and in order). Keep this file current
when you hand over again; `CHANGELOG.md` stays the record of what shipped.

## Where things stand

- **v4.5.0 is in progress, uncommitted** (VERSION still 4.4.0; CHANGELOG `[4.5.0] — in progress`).
  Landed, texture encoding (both halves):
  - KTX2 zstd / zlib supercompression decode. The consumer registers sankoch's decoders
    (`gpu_ktx2_set_zstd_decoder` / `_zlib_decoder`); mabda's `src/` must not name an optional
    stdlib leaf's functions, even under an `#ifdef` (toolchain-issues E5).
  - The BC7 CPU reference encoder and decoder (`src/texture_bc7.cyr`; the maintainer chose BC7).
    `make test-native-bc7-e2e` compares it texel by texel with the Cezanne's texture unit, which
    decodes BC7 exactly. `tests/tcyr/texture_bc7.tcyr` holds the PSNR bar. About 0.8 ms a block.

  Landed, compressed sRGB ids (17–24, `src/texture_format.cyr`, with `mabda_texfmt_is_srgb` /
  `_unorm_of` / `_srgb_of`). KTX2 / DDS sRGB containers load as them. Native AMD decodes every
  sRGB id it samples (NUM_FORMAT_SRGB in the T#; RGBA8 sRGB sampled raw before), the
  maintainer's call. The gates are `make test-native-srgb-e2e` and its wgpu twin
  `make test-wgpu-srgb-sample-e2e`, which check every texel against the sRGB table, ±1.

  Landed, render-graph backing reuse on native AMD (`rg_build_ctx` in `src/render_graph.cyr`):
  one GTT allocation holds every transient as a view, and with `rg_aliasing` on,
  disjoint-lifetime transients share bytes. `rg_execute_mq` waits for every queue that used a
  slot before a node reuses it. `make test-native-rg-alias-e2e` checks it on the Cezanne, and
  its phase B fails without that wait. Fixed on the way:
  - queues hold 4 merged pending waits (one used to be overwritten);
  - same-queue graph edges wait (a pipeline sync);
  - the planner sizes textures by format;
  - a transient's handle is 0 until it is allocated.

  Next: the language minors (S2 / S3 / S4 / M6).
- **v4.4.0 is released** (tag `4.4.0`; the cut was 2026-11-17). VERSION and the CHANGELOG say 4.4.0. The pins
  are samvada 1.1.2 and chitra 1.4.1, and the audit is `docs/audit/2026-11-17-4.4.0-audit.md`.
  - The audit's 1 HIGH, 2 MEDIUM and 9 LOW are all fixed and tested. The HIGH was GFX9's waits
    ignoring branches; the corpus kernel `wait_skipped_only_use` shows it on the Cezanne.
  - The seat closeout passed on both boxes after the audit fixes and the samvada move
    (`build/logs/seat-closeout-amd-20261117-122835.log`,
    `build/logs/chew-seat-closeout-nvidia-20261117-123117.log`). What is left is the tag, which
    is the maintainer's.
  - The previous release was v4.3.1 (tag `4.3.1` at `979222d`; the render tier's MB2–MB5;
    audit `docs/audit/2026-11-16-4.3.1-audit.md`).
- **What 4.4.0 holds.** Two items landed first: **the scratch-buffer contract** and **the compiler limits v4.1.0 left
  named**, together with the control-flow miscompiles found next to them (see "The compiler's
  control flow" below). **N9 is in progress**: the design is `docs/proposals/n9-spirv-to-sass.md`
  and **N9 is done** (N9.1–N9.6). The modules are `src/sm75_encode.cyr`, `sm75_isel.cyr`,
  `sm75_regalloc.cyr` and `sm75_compile.cyr`; `sm75_compile_spirv` is the CPU compile frame, and
  `backend_nvidia.cyr` wires it into `gpu_shader_module_create_spirv`. On the TU116:
  - `test-nvidia-spirv-compute-e2e`: 26 of the 27 control-flow kernels match every lane
    (`oob_label` is refused by name);
  - `test-nvidia-ranga-kernels-e2e`: ranga's 12 kernels match (bilinear since the f64 work, and
    on AMD since the 1024-id MIR space);
  - `test-nvidia-f64-kernels-e2e`: the f64 corpus, 512 lanes bit-exact against the IEEE oracle;
  - ranga's four GPU suites pass with an NVIDIA context (in a scratch clone), except
    `gpu_kernels`' "flip this when it can" bilinear row, which is ranga's to flip (it fails on
    AMD too now that GFX9 compiles bilinear).

  ranga's kernels are vendored in `tests/spirv/ranga/`. Their goldens come from the SPIR-V
  interpreter in `scripts/gen-spirv-ranga-fixtures.py` (`make check-ranga-fixtures`).
  `test-native-ranga-kernels-e2e` checks the same goldens on AMD. **Every 4.4.0 item is done**
  (2026-11-17); what is left is the cut itself, which is the maintainer's call:
  - N9's follow-ons are all done:
    - latency-table scheduling: `sm75_schedule` is checked by `_ck_sched` in
      `tests/tcyr/sm75_compile.tcyr`, and `make bench-nvidia-compute` measures it;
    - predicate allocation (P0–P6, `_s75_palloc` in `sm75_isel.cyr`);
    - `BSSY` / `BSYNC` reconvergence (`_s75_ropen` / `_s75_rclose`, checked by `_ck_barriers`);
    - `OpFDiv` / f64 (`_s75_f64`, `_s75_fdiv32` / `_s75_fdiv64`; the lowering emits
      `MIR_OP_FDIV` under `spirv_lower_set_target(SPIRV_TARGET_SM75)`).

    `~/mabda-bench-v1` on chew holds a git-archived "before" tree for before/after benches
    (refresh it from the commit you compare against).
  - The seat-only closeout. chew's half passed on 2026-11-16 (`make seat-closeout-nvidia`, all five
    card-node gates). Done on both boxes (2026-11-17). The Cezanne's fourth run passed and
    looked right, after fixes to the size refusal, the non-blocking flip wait, the AMD double
    buffer and the console restore. chew's rerun passed all six gates. Run it from a text console with `sudo systemctl stop sddm`,
    then read `build/logs/seat-closeout-amd-*.log`.
  - The logind path on NVIDIA returning the device: fixed and verified on chew (2026-11-16,
    `make test-nvidia-logind-present-e2e`, the first live-bus run of mabda's logind path).
  - NVIDIA block-linear scanout: done. The layout is checked against the copy engine
    (`test-nvidia-blocklinear-e2e`), and the scanout passed its seat run on chew (2026-11-16).
    A seat run on chew needs a text console (Ctrl-Alt-F3) with `sudo systemctl stop sddm`; a
    terminal in the desktop cannot take DRM master.
  - The consumer check: done (2026-11-17) against the working-tree dist, in scratch clones.
    ranga's suites give the same counts on both GPUs. The only failing rows are the ones ranga
    has filed to flip:
    - item 5, the bilinear "flip this when it can" row, on both GPUs;
    - item 3, `gpu_nvidia`'s 31 refusal rows.

    soorat's P6/MB1, MB2–5, P7 and P8 probes are identical to the 4.3.1 dist. The logs are
    `build/logs/consumer-{ranga,soorat}-*-20261117-113114.log`. It was rerun on the final cut
    tree with the same results (`build/logs/consumer-*-final-*.log`). ranga's five changes are its own call: they are filed as
    `ranga/docs/development/issues/2026-11-16-mabda-n9-nvidia-compute-lift-the-guard.md`
    (item 5, the bilinear row and `gpu_resize`, was appended after the 1024-id work). For the
    soorat probes, keep the `scripts/probes/gpu/` layout and the `rust-old/src/` shaders the
    probes read through `../../../`. A flat copy fails every shader read.
  On chew, test the working tree in `~/mabda-verify-n9` (tar `src tests programs scripts
  Makefile cyrius.cyml cyrius.lock VERSION` over it, then `rm -rf lib && mkdir lib && cyrius
  deps`). To round-trip compiled SASS, copy it there and run `scripts/disasm-sass.sh <file>...`. The oracle lives on `chew`: `nvcc` / `nvdisasm` / `cuobjdump` in
  `/opt/cuda/bin`, scratch in `~/mabda-n9-oracle`. ranga's suites ran from static binaries built
  here against a scratch clone (`~/ranga-n96` on chew holds them). ranga's
  `tests/gpu_spirv.tcyr` needs the 4-argument table-builder call (CHANGELOG, Breaking).
- **The 2026-11-16 compute-ring reset is settled.** A probe hung the GPU at 12:59:20, before its
  fix. The Cezanne has rebooted since, and the seat closeout's `test-native-all` ran on a clean
  boot ("GPU resets this boot: none", `build/logs/seat-closeout-amd-20261117-111849.log`).
  Never run hang-inducing controls on the local Cezanne.
- **Pins:** cyrius **6.7.18** (`cyrius.cyml`; track the latest upstream `VERSION` when you bump),
  wgpu-native **v29.0.1.1**, launcher **83 slots**, deps samvada 1.1.2 / chitra 1.4.1 (since
  4.4.0; samvada 1.1.2 fixed a HIGH in `TakeDevice`).
- **Next release: v4.5.0 — textures + language**: texture encoding, compressed sRGB ids,
  render-graph aliasing, the cyrius 6.7 language minors (S2 / S3 / S4 / M6). Then v5.0 (native
  rendering beyond the vertex-less subset; the NVIDIA scanout follow-ons, which are block-linear
  render targets and a copy-engine present; Intel; NVIDIA wgpu retires) and v5.1. Don't split
  these into more minors without asking.

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
./scripts/count-test-assertions.sh            # the honest count (29 suites, 7932 at 4.4.0)
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

## The compiler's control flow (4.4.0)

- **Conditions live in flags.** An integer compare writes SCC (uniform) or VCC (divergent), not a
  register. Every reader (a branch, a `MIR_OP_ISELECT`) checks whether the selected record just
  before it is its own compare (`fl_id` / `fl_k` in `gfx9_isel`). If not, it re-emits the compare
  via `_gisel_cond_flag`. Never assume a flag survives an instruction: `OpUDiv`, the f64 macros
  and other compares write VCC, and every SALU op writes SCC.
- **Divergent regions are a stack** of saved-EXEC pairs, s[100-2d : 101-2d], up to 8 levels.
  The if / if-else / loop sequences are documented above `_CF_IF` in `src/gfx9_isel.cyr`.
  `_gisel_cf_prepass` finds loops (back-edges) and their exits. An if's else is derived: the
  instruction before the false label is the then-block's branch past it. Shapes the stack can't
  mask fail with `GISEL_ERR_UNSTRUCTURED_CF`; never let them compile.
- **Phi copies go on edges.** `_spirv_emit_phi_moves` emits a parallel copy (through temporaries
  if a source is written on the same edge) typed by the OpPhi, so f64 copies are a register
  pair. On `OpBranchConditional`, forward-edge copies go before the branch. Back-edge copies go
  after it, counted in the branch's flags (`MIR_CB_MOVES_*`), and isel places them past the exit
  test.
- **Probe on hardware before trusting a shape.** Every bug above compiled and passed the CPU
  suite. `tests/spirv/control_flow/*.body` holds them as SPIR-V assembly with a `; want:` /
  `; refuse:` line. Add a kernel there, run `scripts/gen-spirv-cf-fixtures.py` (needs spirv-as /
  spirv-val), and `make test-native-spirv-control-flow-e2e` checks every lane. A hung kernel
  resets the compute ring (see above), so think about termination before dispatching a new loop.

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
- `native_kms_present` overwrites the scanout record's `fb_id` with the FB it flipped to. Never
  read a double buffer's FB ids back from it; NativeSurface keeps A's id at +124.
- A gate that passes its checks can still look broken on screen, and that is a finding. The AMD
  double-buffer bug passed every present gate for months.
- systemd-logind hands out device fds with O_NONBLOCK. Never `read()` a DRM event bare: use
  `native_drm_wait_event` / `native_kms_wait_flip_complete` (poll, then read).
- An AMD native surface must be configured with the connector's exact mode (no scaling; audit
  HIGH-1). Read it first with `native_kms_first_connected_mode` (no master needed). The monitor
  both boxes share runs 2560×1440, and the AMD GPU is `card1` on the Cezanne.
- Turing's block-linear GOB is NOT the Tegra / Fermi one that most write-ups give. Bits 4..8 of
  the in-GOB address are y0 y1 x(16) y2 x(32): see `native_nv_bl_offset`, checked against the
  kernel's `nv50_set_pixel_swizzle` and NVK's `CopyGOBTuring2D`.
- Check every uapi struct against `sizeof` / `offsetof` from the header (compile a 3-line C
  probe). `drm_mode_fb_cmd2` sat 4 bytes short for years, because linear scanout never reads
  its `modifier`.
- A fn's local arrays share a frame budget of about 120 KB (15360 slots of 8 bytes). Past it, a
  local array silently gets STATIC storage, with only a build warning (shared and not re-entrant).
  Split a big frame across two fns, as `sm75_compile_spirv` / `_sm75_compile_back` do.
- The MIR id space is `MIR_MAX_CAP_IDS` (1024) on both back ends; the SPIR-V id bound is 256
  (`NATIVE_SHADER_CAP_IDS`); `RA_MAX_REGS` (256) is the register file. A refusal names which
  limit it hit: `MIR_ERR_ID_OOR` is ids, `RA_ERR_{V,S}GPR_OVERFLOW` is registers. Measure before
  restructuring a kernel: bilinear was misread as register pressure for two releases.
- Test fake function tables must be `WGPU_FN_COUNT x 8` bytes (664 at 83 slots).
- A new `src/` module goes in **both** `src/lib.cyr` and `cyrius.cyml`'s `[lib].modules`, or distlib
  silently leaves it out.
- Before 4.3.1, `backend_get_slot(0, off)` read address `off`. It is null-safe now, but a dispatcher
  must still check for slot 0.

## Open, and where it is tracked

Everything is in [the roadmap](roadmap.md): v4.4.0 (compilers + NVIDIA + the seat-only closeout),
v4.5.0 (textures + language), v5.0 (native rendering, Intel, NVIDIA wgpu retirement), v5.1, and
"blocked upstream" (WASM, `trait Backend`, samvada's Windows `sys_recvmsg`, cyrfmt's file cap).
