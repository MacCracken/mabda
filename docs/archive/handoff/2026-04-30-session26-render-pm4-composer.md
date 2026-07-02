# Session 26 handoff — Native render PM4 composer landed (Steps 6.2 + 6.5)

**Date:** 2026-04-30
**Branch:** `v3`
**Predecessor:** [`2026-04-28-session25c-punch-list-march.md`](2026-04-28-session25c-punch-list-march.md) (closed at "decide 6.2(b) shader-byte path; 6.5/6.6/6.8c/6.9b all gated on shader bytes").

## Where you'll be when you pick this up

The native render path's CPU-testable structure is now end-to-end in tree:
register addresses + minimums (6.2(a) + 6.5(a)), VS+FS shader bytes (6.2(b)), and the full clear-triangle PM4 stream composer (6.5(b)). Every emitted byte is pinned by a CPU value-assert.

**The next bite is one of three things, depending on your appetite:**

1. **Step 6.6 — `native_render_dispatch_simple`** — the GFX-ring submission analog of `native_compute_dispatch_cached`. Self-contained, CPU-testable for structure. Same syscall flow (DRM_AMDGPU_CS chunk submission, syncobj wait), different ring kind. ~1 session.
2. **Layer-2 verification — build `programs/diagnostics/radv_capture/` (phase 1).** Self-contained C program that submits an offscreen `vkCmdDraw` clear-triangle and produces an `RADV_DEBUG=hang` IB dump. Byte-diff against `native_pm4_build_render_clear_triangle` output is the "claim 6.5 done" gate. ~1 session of bring-up. Decision (2026-04-30): this is a **first-class deliverable**, not a workaround — phase 2/3 generalize it into a golden-image harness reusable across mabda's consumers (soorat / rasa / aethersafta / kiran) for UI-level regression testing. Detailed phasing in "What's queued" § Layer-2 below. Likely surfaces 1-3 register-address or value bugs in the PM4 stream to fix.
3. **Phase D side-quest (7.1–7.7)** — DRM/KMS surface / present. Fully unblocked (no shader bytes, no PM4 verification). Different code path than the GFX-ring work.

I lean (1) — keeps the v3.0 critical path moving, doesn't burn cache miss against the unverified PM4 stream.

## What landed in session 26

The `v3` branch tip is `15910b7 38 new register addresses` plus four uncommitted files (the 6.5(b) work + doc/handoff updates). Total 4 commits + 1 pending across the session.

| Commit | Step | What |
|--------|------|------|
| `8a83bfc` | setup | Cyrius pin bump 5.5.20 → 5.7.35 + first `lib/` resolve on the fresh Arch box |
| `1e2cf53` | setup | Cyrius pin → 5.7.36 + dist regenerate (after surfacing the upstream distlib 64-KB truncation bug — see "Tooling deltas" below) |
| `f78f3d6` | 6.2(a) | 12 SH/context register addresses + 6 spec-derived field minimums + capture-protocol doc (`docs/proposals/v3-shader-bytes-capture.md`, 298 lines) |
| `f8438f5` | 6.2(b) | `native_gfx9_shader_solid_red` (FS, 92 B) + `native_gfx9_shader_fullscreen_triangle_vs` (VS, 116 B), VOP2 encodings ground-truthed via `clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -nogpulib` of an equivalent CL kernel + `llvm-objdump -d` |
| `15910b7` | 6.5(a) | 38 new register addresses (pipeline-static + pass-target + UConfig graphics) + 12 value constants, all extracted from authoritative Mesa `gfx9.json`. Caught + fixed scrambled `SPI_SHADER_{POS,Z,COL}_FORMAT` addresses from 6.2(a). |
| (pending) | 6.5(b) | Full PM4 stream composer in 4 blocks (`pipeline_sh / pipeline_ctx / pass_target / draw_tail`) + ACQUIRE_MEM preamble + UConfig + NOP padding. `native_pm4_build_render_clear_triangle(buf, vs_va, fs_va, rt_va, w, h)` is the top-level entry. ~492 B per dispatch padded to 1024. Plus `native_int_to_f32_bits` + `native_f32_neg_bits` viewport helpers. |

## Test posture at session end

| Sweep | Count | Result |
|-------|-------|--------|
| `cyrius test tests/tcyr/mabda.tcyr` | 624 assertions | green |
| `cyrius test tests/tcyr/mabda_v3.tcyr` | **697** assertions (was 484 at session 25c close) | green |
| `cyrius build programs/smoke.cyr` | full include chain links | OK (4 dead-code notes on `debug_*` markers, pre-existing) |
| `cyrius lint src/backend_native.cyr` | 0 warnings | ✅ |
| `cyrius lint tests/tcyr/mabda_v3.tcyr` | 0 warnings | ✅ |

GPU integration programs (`phase0`, `compute_e2e`, `render_e2e`, `render_graph_e2e`) untouched and presumed green — none of today's changes touched the wgpu path or render-graph scaffolding. Re-run as a sanity check before merging if you're risk-averse.

**Total: 1321 CPU assertions all passing. 697 v3 asserts is +213 from the 484 at session 25c close**, of which ~75 are 6.5(b) structural tests, ~50 are 6.5(a) constants, ~58 are 6.2(b) shader bytes, ~30 are 6.2(a) constants.

## Uncommitted right now (clean follow-up commits)

```
M docs/development/3-0-punchlist.md         (status table + 6.5(a/b) entries)
M docs/proposals/v3-shader-bytes-capture.md (status field + tables for FS/VS/6.5(a))
M src/backend_native.cyr                    (266 lines: 6.5(b) composer + helpers)
M tests/tcyr/mabda_v3.tcyr                  (248 lines: 8 new tests, 75 asserts)
?? docs/handoff/2026-04-30-session26-render-pm4-composer.md (this file)
```

Suggested commit shape — one commit per bullet:

1. `feat(backend_native): Step 6.5(b) — clear-triangle PM4 stream composer + 4-block split`
2. `docs(v3): refresh shader-bytes-capture proposal + 3-0 punchlist status for 6.5(b)`
3. `docs(handoff): session 26 — render PM4 composer landed`

## Tooling deltas worth knowing about (this was a fresh-box session)

The user reinstalled Arch + Hyprland mid-session, so a chunk of work was getting the toolchain back. Three things worth carrying forward:

### 1. Cyrius distlib had a 64-KB-per-module truncation bug

Caught while regenerating `dist/mabda.cyr` against the v3 source: `src/backend_native.cyr` is now 77 KB, exceeding cyrius distlib's 64 KB read buffer (`alloc(65536) / file_read_all(..., 65535)` in `cbt/commands.cyr:894`). Silent truncation → bundle missing every fn defined past byte 65535 (`backend_native_new`, `native_texture_*`, `native_rt_*`).

**Fixed upstream in cyrius 5.7.36** (commit `8c75f3f` per the cyrius repo log) — buffer raised 64 KB → 256 KB. Mabda's pin moved 5.7.35 → 5.7.36 in commit `1e2cf53`.

**As of session end, cyrius local is at 5.7.39 — mabda's pin at 5.7.36.** No urgency to re-bump tonight; the 5.7.36 pin is what the regenerated dist was built against. Worth a one-line bump + `cyrius distlib` regen in the morning if you want to track latest.

### 2. Cyrius lint/fmt CLI shape changed in 5.7.x

Bare `cyrius lint` / `cyrius fmt --check` (repo-wide) is gone — both now require an explicit file argument. CLAUDE.md's P(-1) Scaffold Hardening step 1 still describes the bare form; **flag for update** when Tier 4 docs are touched. The new project-wide shape is `cyrius audit`, but it shells out to a `check.sh` that wasn't installed by `install.sh` on the fresh box (caught + handed to the cyrius 5.7.36 audit agent; not yet fixed upstream as of 5.7.36).

For mabda CI/local, the per-file loop is the right invocation:
```sh
for f in src/*.cyr; do cyrius lint "$f" || exit 1; done
for f in src/*.cyr; do cyrius fmt "$f" --check >/dev/null || exit 1; done
```

### 3. clang+LLVM AMDGPU is now installed on the box

`clang 22.1.3` + `llvm-objdump` + `llvm-mc` all present (`pacman -S clang llvm`). This unblocks the Layer-1 protocol the compute shader used: hand-encoded shader bytes get cross-checked against `clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -nogpulib` disassembly. Used today for the VS arithmetic VOP2 encodings (`v_lshlrev_b32` 0x12, `v_and_b32` 0x13, `v_add_u32` 0x34). The reference CL kernel + invocation are captured in `docs/proposals/v3-shader-bytes-capture.md` § "Layer 1 — shader ISA bytes via clang".

**The CL kernel is in `/tmp/vs_disas/vid_to_pos.cl`** as of session end — not gitignored, not committed. If you want it durable, copy into the repo (`programs/diagnostics/` or similar) or just rely on the inline copy in the proposal doc.

## Bug catches during the session (each one paid for itself)

These are the catches the protocol justified — every one of them would've been a hardware-debug session if it had reached HW.

### A. Cyrius distlib 64-KB truncation
See "Tooling deltas" #1. Fixed upstream.

### B. Missing backend modules in `cyrius.cyml [lib].modules`
`src/backend.cyr`, `src/backend_wgpu.cyr`, `src/backend_native.cyr` were referenced in `src/lib.cyr`'s include chain but never added to `cyrius.cyml`'s `[lib].modules` array. Build/test paths worked (they walk the include chain); `cyrius distlib` walks `[lib].modules` instead and was emitting a bundle with unresolved `backend_wgpu_new` / `backend_native_new` / `backend_kind`. Fixed in commit `1e2cf53`.

### C. Scrambled `SPI_SHADER_{POS,Z,COL}_FORMAT` addresses
6.2(a) shipped with three SPI shader format register addresses transcribed wrong:

| Register | What 6.2(a) shipped | Authoritative (Mesa gfx9.json) |
|---|---|---|
| `R_SPI_SHADER_POS_FORMAT` | `0xA710` ❌ | `0xA70C` |
| `R_SPI_SHADER_Z_FORMAT` | `0xA708` ❌ | `0xA710` |
| `R_SPI_SHADER_COL_FORMAT` | `0xA70C` ❌ | `0xA714` |

Caught during 6.5(a)'s sweep against the authoritative Mesa register table. Fixed before any HW test ran.

**Lesson now codified in `docs/proposals/v3-shader-bytes-capture.md` § "Authoritative source for register addresses"**: every `R_*` constant must have a comment citing its `gfx9.json` `map.at` value, and the lookup script is in the doc. Without that anchor, a typo in transcription is invisible.

### D. PGM_HI mask
6.5(b)'s pipeline_sh block initially had `vs_pgm_hi = (vs_va >> 40) & 0xFFFFFFFF`. Cyrius's `>>` on i64 is **logical right shift** (no sign-extend), so for `vs_va = 0xFFFF800200000000` this produced `0xFFFF80` (24 bits of high VA in low position). Should match compute's pattern at line ~809: `& 0xFF` (just the meaningful 8 bits, bits 40-47 of VA). Fixed.

### E. CB_COLOR_CONTROL design-doc value 0xCC
`docs/proposals/v3-native-render-design.md` lists `CB_COLOR_CONTROL = 0xCC` as the "NORMAL ROP" value. That's field-shorthand — 0xCC is the ROP3 enum value but needs to be in bits 16-23 of the register, with MODE=CB_NORMAL (1) in bits 4-6. Real composed value: `0xCC0010`. Bare 0xCC would set DEGAMMA_ENABLE bit 3 and put MODE=4 (CB_DECOMPRESS). Fixed; 6.5(a) constants now expose `GFX9_CB_COLOR_CONTROL_NORMAL_COPY = 0xCC0010` as the authoritative composed value.

### F. (Cumulative test arithmetic errors)
Six test expectations in 6.5(b) had wrong values — header count_minus_1 off-by-one (n vs n+1 confusion), VA-shift result miscomputed for canonical-high VAs, cumulative pass_target offset arithmetic. None were code bugs; all were test expectation bugs. Caught immediately via assert failures, fixed against ground truth. Listed here so future-you doesn't re-derive them: `viewport set_context_reg(n=6) → header 0xC0066900` (not `0xC0056900`); `(0xFFFF800200000000 >> 40) & 0xFF = 0x80` (not `0x01`); `pipeline_ctx is 240 B` (not 228, recount: 17 singletons × 3 dw + 5 + 4 = 60 dw).

## What you'll thank past-you for in 12 hours

**Three things.**

### 1. Mesa `gfx9.json` is the register-address source of truth — the lookup script is in the doc

`docs/proposals/v3-shader-bytes-capture.md` § "Authoritative source for register addresses" has the Python script that extracts register addresses + chip filter for gfx9. The JSON itself is at `/tmp/gfx9.json` (cached this session). Re-fetch from `https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/amd/registers/gfx9.json` if needed. **For ANY new context-reg or SH-reg constant in `src/backend_native.cyr`, run the script first; transcribe with citation comment.** This caught bug C above.

### 2. The PM4 composer is structurally correct — assertions pin every byte

If you're picking up to do Step 6.6 (submission path), you can build on top of `native_pm4_build_render_clear_triangle` with confidence that every emitted dword has been spot-checked. The hazards you should still watch for are SEMANTIC (does this PM4 stream actually produce the right GPU behavior under the AMDGPU UAPI), not STRUCTURAL (does the wire form match the PKT3 spec).

The semantic verification gate is byte-exact-vs-radv-IB-diff (Layer 2). Until Hyprland is up and you can run `vkcube` with `RADV_DEBUG=hang` to capture an equivalent IB, **don't claim 6.5 done**. The Phase B.4 lesson was 12 sessions of "looks right, shipped wrong"; today's bug catches A-F are structurally identical to that class — a typo in a register address or a bitfield position would not have surfaced without the authoritative-source check.

### 3. The 4-block split is load-bearing

`docs/proposals/v3-backend-interface.md` v2.1's ctx-cs vs cs split (pipeline_sh / pipeline_ctx / pass_target / draw_tail) is what the 6.5(b) composer implements. Step 6.8c (native render slot wrappers) will need to slice this same way — `_backend_native_render_pipeline_create` builds the sh + ctx blocks once, `_backend_native_render_pass_begin` builds the pass_target block per-pass, `_backend_native_render_pass_draw` emits draw_tail. **If you find yourself writing a single-block PM4 cache in 6.8c, stop and re-read the v2.1 design.** That was the rejected v2.0 design.

## What's queued for the morning agent (not done tonight)

- **Vidya field-notes** — `../vidya/content/cyrius/field_notes/mabda-v3-gpu.cyml` should gain entries for: cyrius distlib 64-KB cap (process lesson), Mesa gfx9.json as authoritative register source (technique), SPI_SHADER_*_FORMAT scrambled-address bug catch (process lesson), clang+llvm-objdump VOP2 cross-check pattern (technique). The `language.cyml` entry for "Cyrius `>>` on i64 is logical not arithmetic" is also worth capturing — caught during PGM_HI bug D.
- **CHANGELOG `[3.0.0-dev]` dated section** — add an entry covering 6.2(a/b) + 6.5(a/b) — the running diary should reflect today's work. Format follows the existing `### Verified — 2026-04-27/28` shape.
- **CLAUDE.md update** — Tier 4 punchlist item; not for tonight. Lists 30 modules / 4500 lines / 387 assertions. Real numbers: 33 modules / ~14,000 lines / 1321 CPU assertions + 32 GPU.
- **Cyrius pin bump** to 5.7.39 (latest local). Optional; mabda is on 5.7.36, which is fine.
- **Layer-2 capture plan — both paths.** Decision (2026-04-30):
  build the headless program as a first-class deliverable. The
  Hyprland-session path stays available for quick spot-checks but
  the headless program is the load-bearing tool. **Both paths land,
  not just one.**

  **(a) Hyprland session + `vkcube` — the spot-check path.**
  Simplest if you're physically at the machine.
  `RADV_DEBUG=hang vkcube` produces an IB dump on the artificial
  hang radv inserts. Use this for one-off "did my latest tweak
  match radv?" diffs while iterating on PM4 fixes. SSH-incompatible
  (X11 forwarding renders on the local GPU; Xvfb lacks DRI3 — both
  confirmed dead-ends in session 25c).

  **(b) Headless Vulkan capture program — the load-bearing path.**
  Self-contained C program that creates an offscreen RGBA8 image
  (no swapchain), submits a `vkCmdDraw` of a clear-triangle, and
  reads pixels back. `RADV_DEBUG=hang` captures the IB the same way.
  Works over SSH. **Long-term payoff: this becomes the foundation
  for UI / golden-image testing across the AGNOS consumers** —
  soorat (renderer), rasa (image editor), aethersafta (compositor),
  kiran (game) all need pixel-level regression tests, and a
  parameterized "submit a draw, capture pixels + IB" harness is
  exactly that primitive. The Layer-2 PM4 diff is just the first
  customer.

  **Suggested phasing for the headless program:**

  1. **Minimum-viable capture (`programs/diagnostics/radv_capture/`).**
     ~50-150 LoC C, written against the Vulkan 1.x C API. Hardcoded
     for the Step 6.5 clear-triangle: 256×256 RGBA8 RT, full-screen
     triangle from vertex_id, solid color FS. Output: stdout shows
     `RADV_DEBUG=hang` IB dump on the artificial hang; a
     side-channel writes the RGBA8 readback to a file. Run via
     `RADV_DEBUG=hang programs/diagnostics/radv_capture/build/capture
     > /tmp/radv_clear_triangle.ib`. ~1 session of bring-up.
  2. **Generalize for golden diff.** Parameterize: `--width`,
     `--height`, `--vshader <path>`, `--fshader <path>`,
     `--vcount <n>`, `--out-pixels <path>`. With this shape, mabda's
     own integration tests (`render_e2e.cyr`, `render_graph_e2e.cyr`)
     can produce reference pixels via the same harness. Becomes a
     CI gate against the wgpu path. Probably its own session,
     scoped after Step 6.5 Layer-2 verify lands.
  3. **Cross-consumer adoption.** Once the harness has
     `--shader-tree <wgsl|spv|isa>` shaping, soorat / aethersafta /
     kiran can land golden-image regression tests against it. The
     `examples/stdlib-consumer/` regression test in Tier 2 of the
     v3.0 punchlist becomes "this harness produces pixel-identical
     output across both backends." A real story.

  **For tonight / morning: phase 1 only.** Get the IB. The other
  phases are queued from the value-add the headless approach
  unlocks; don't scope-creep into them on the same session that
  just needs the radv reference.

## Quick-reference paths

- Punch list: `docs/development/3-0-punchlist.md` — 6.2 + 6.5 sub-bullets refreshed; Tier 1 status table updated.
- Capture protocol: `docs/proposals/v3-shader-bytes-capture.md` — Layer-1 + Layer-2 + three derivation paths + register-address authoritative source script + tables for FS/VS/6.5(a)/6.5(b).
- Backend interface: `docs/proposals/v3-backend-interface.md` v2.1 — 4-block PM4 split, the design that 6.5(b) implements.
- Render design: `docs/proposals/v3-native-render-design.md` — Step 6.1 enumeration, still load-bearing for 6.5(b)+ register choices.
- Test files: `tests/tcyr/mabda_v3.tcyr` — 6.2(a/b) + 6.5(a/b) test sections.
- Source: `src/backend_native.cyr` — section markers `# v3 Step 6.2(a)`, `# v3 Step 6.2(b) — fragment shader`, `# v3 Step 6.2(b) VS`, `# v3 Step 6.5(a)`, `# v3 Step 6.5(b)`.

## Sleep well

The native render path's structure is in tree. Layer-2 verification is the only thing standing between today's work and "claim 6.5 done." Hyprland unblocks that. Until then, Step 6.6 (submission path) is the next bite that doesn't need HW.
