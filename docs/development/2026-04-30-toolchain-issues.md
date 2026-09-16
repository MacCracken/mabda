# Toolchain issues observed during the v3 cycle

**Date filed:** 2026-04-30 (v3.0.0-rc.1 cut)
**Toolchain at filing:** `cyrius 5.7.48` (mabda's pin at filing; bumped to `5.11.28` 2026-05-12 ahead of rc.3 soak)
**Last re-checked against:** `cyrius 6.5.29` (mabda 4.0.10, 2026-08-19) — Class A now resolved upstream; see A1.
**Audience:** mabda contributors + the cyrius toolchain team. Each
entry is the smallest reproducible footprint we hit; cross-referenced
to the deeper bug reports in `docs/development/issues/` and the
auto-memory feedback notes where they exist.

This doc is a consolidated quick-reference. The intent is not to
file new bugs upstream from here — those go in `docs/development/
issues/<date>-<slug>.md` with full repro + severity + upstream
status. This doc is the "if you're new to mabda, here are the
toolchain quirks you'll hit" cheat sheet.

---

## Class A — Silent truncation of large files

### A1. `cyrius lint` and `cyrius fmt` cap reads at 128 KiB — ✅ **RESOLVED upstream (6.2.20)**

> ⚠ **This entry is history, and the workaround it used to recommend is
> now dead code** — but be precise about *why*, because the obvious
> reading is wrong. Re-verified against 6.5.29 during the mabda 4.0.10
> cut (2026-08-19); see the measured behaviour table below.

- **Original symptom** (cyrius ≤ ~5.7.48): lint reported bogus
  "unclosed braces at end of file" warnings near the end of files
  >131,072 bytes; fmt dropped the end-of-file content, so any
  redirect-back-to-file destroyed data. On 2026-04-30 a fmt sweep
  turned `src/backend_native.cyr` (3,088 lines) into 2,947 lines,
  silently losing `NATIVE_RT_STRUCT_SIZE` and the `native_rt_*` fns.
- **Fix upstream**: cyrius **6.2.20** raised `_MAX_FILE` in *both*
  `programs/cyrfmt.cyr` and `programs/cyrlint.cyr` to **1,052,672**
  bytes (1028 KiB) *and* made a ceiling-hitting read **fail loud**
  (`cyrfmt: file too large to format-check (>1028KB) — raise _MAX_FILE`)
  rather than format-check a truncated buffer. Both halves of the bug
  are gone: 8× headroom, and no silent path.
- **Verified in tree**: the 4.0.10 reflow ran in-place `cyrius fmt`
  over 73 files including `tests/tcyr/native.tcyr` (216,256 B) and
  `src/backend_native.cyr` (169,742 B) — both far past the old line.
  `git diff --ignore-all-space` over `src/ programs/ tests/ fuzz/`
  came back **empty** with zero line-count change anywhere.
- **Consequently retired**: the CI size guard (already gone from
  `.github/workflows/ci.yml`), and the "split `backend_native.cyr` to
  unblock the fmt gate" punchlist item — that split is now a
  readability question, not a toolchain one.
- **Measured behaviour on 6.5.29** (each row actually run, not inferred):

  | Command | Effect |
  | --- | --- |
  | `cyrius fmt f.cyr` | rewrites `f.cyr` in place, formatted. Prints **nothing**. `rc=0`. **This is the apply path.** |
  | `cyrius fmt f.cyr --check` | diagnostic naming the first differing line; file untouched. Drift by **exit code** — and it can exit non-zero printing nothing. |
  | `cyrius fmt f.cyr --dry` | same diagnostic plus `WOULD reformat`; file untouched. **Not** a source dump. |
  | `cyrius fmt f.cyr > f.new` | `f.cyr` gets correctly formatted in place; `f.new` is **0 bytes**. The old line-count guard can therefore never pass, so the `mv` never runs. **Litter, not damage.** |
  | `cyrius fmt f.cyr > f.cyr` | ⛔ **DESTROYS `f.cyr`.** The shell truncates it to 0 before cyrfmt opens it; cyrfmt then fails and the 25-byte string `cyrfmt: cannot read file` becomes the file's entire contents. `rc=1`. |

  ⭐ **So A1's original warning — "never run `cyrius fmt $f > $f` blindly" — was
  right, and is now *more* dangerous than when it was written**, since the
  redirect-onto-self case leaves plausible-looking text rather than a short file.
  The `$f.fmt`-plus-guard half of the advice, by contrast, is merely obsolete: it
  cannot damage anything, it just no longer does what it was written to do.
- **Current correct practice**: `cyrius fmt <file> --check` to detect,
  `cyrius fmt <file>` to apply. Guard a bulk in-place sweep with
  `git diff --ignore-all-space` being **empty** — not with a line count,
  which the 4.0.10 sweep would have passed vacuously (73 files reformatted,
  zero line-count change).
- **Memory note**: `feedback_cyrlint_128k_buffer_cap.md` (marked
  obsolete) and `feedback_cyrfmt_check_exit_code_not_stdout.md` (the
  live one).

### A2. `cyrius fmt --check` writes formatted output to stdout
unconditionally, NOT silent on no-drift

- **Symptom**: developers expecting `--check` to be silent (exit
  zero, no output) get the formatted version of the file printed
  to stdout regardless. The "is there drift" signal is whether
  `diff <(cyrius fmt $f --check) $f` is non-empty, NOT whether the
  command produced output.
- **Workaround**: gate scripts always use `diff` + check
  exit code, never check `--check`'s stdout for emptiness.
  CI's `Format check` step does the right thing already.
- **Upstream**: behavioural choice; `--check` could either be
  silent on no-drift OR exit non-zero on drift. Both Rust and Go
  use the latter (`gofmt -l` lists files needing fmt; `cargo fmt
  --check` exits non-zero on drift). Filing as a polish bug.
- **Superseded in cyrius 6.x**: `--check` now reports via exit code
  (0 = clean, non-zero = drift) with no stdout — the Makefile
  `fmt-check` target and CI both key off the exit code.

### A3. `cyrius fmt` requires the file argument BEFORE `--check` (6.4.x, 6.5.x)

- **Symptom**: `cyrius fmt --check <file>` prints
  `Usage: cyrfmt --check <file.cyr>` and exits 1 — for **every**
  file, which reads as "all 57 files drift" if you loop it.
  (Observed on 6.4.64; the usage string itself shows the flag-first
  form, which is exactly the form that fails.)
- **Correct form**: `cyrius fmt <file> --check` — file first, flag
  after. Exit 0 = clean, exit 1 = drift (verified against a
  deliberately misformatted probe file).
- **In tree**: the Makefile `fmt-check` target and CI already use
  the file-first form; only ad-hoc invocations trip this.
- **Still current on 6.5.3** (re-checked at the v4.0.8 pin move):
  `cyrius fmt src/error.cyr --check` exits 0, the flag-first form
  still prints the usage string. Unchanged, not superseded.
- **Upstream**: arg parser treats argv[1] as the file
  unconditionally; the usage string is misleading. Polish bug.

### A4. `cyrius lint` cannot pass `--strict-deferrals` (6.6.4)

- **Issue file**: `docs/development/issues/2026-09-16-cyrius-lint-drops-strict-deferrals.md`
- **Symptom**: `cyrius lint --strict-deferrals <file>` prints cyrlint's usage and exits 1
  for every file. `cyrius lint <file> --strict-deferrals` is worse: it **silently drops the
  flag** and exits 0 over untracked deferrals. The `lint` dispatcher knows only `--strict`,
  so it takes the first other argument as the file and ignores the rest (same family as A3).
  mabda's CI `Lint` step greps only `warn ` lines, so deferrals never gated CI. Three
  untracked ones shipped in 4.1.0–4.1.2 until a hand run of cyrlint found them in 4.1.3.
- **Workaround**: call cyrlint directly, per file:
  `cyrlint --strict-deferrals "$f"` (exit 2 on an untracked deferral; flag before or after
  the path both work). Pin the binary as `$HOME/.cyrius/versions/<pin>/bin/cyrlint` when the
  active toolchain may differ from the pin.
- **Upstream**: open (Low). Forward the flag through `cmd_lint` and reject unknown flags.

---

## Class B — FFI / fncall constraints

### B1. `fncall6` into extern-C wgpu — RESOLVED (misdiagnosis, cyrius 6.3.26)

- **Status**: NOT an ABI bug. cyrius 6.3.26 proved `fncall4/5/6/7`
  arg-passing and 16-byte stack alignment are correct against System V
  AMD64. The historical "wgpu `fncall6` segfaults" symptom was a
  TLS/`%fs` init problem: a glibc-compiled C callee with an array local
  carries `-fstack-protector` and reads its canary from `%fs:0x28`, so
  it faults **regardless of arg count** if `%fs` isn't a glibc thread
  block. It merely correlated with the 6-arg wgpu entry points (which
  have local buffers). The wgpu C launcher (ADR-004) supplies glibc's
  `%fs`, so the direct path is sound.
- **Now**: scalar-arg wgpu entry points call directly via `fncall6`. The
  `wgpu_command_encoder_copy_buffer_to_buffer` / `wgpu_queue_write_texture`
  / `wgpu_encoder_resolve_query_set` / `wgpu_buffer_map_sync` struct-packing
  shims were retired in mabda v4.0.2 (HW-verified byte-exact on Cezanne).
  A shim is still required for genuine struct-by-value descriptors
  (begin_render_pass slot 58, copy_texture_to_buffer slot 64), `float` /
  `double`, and variadic callees.
- **Canonical ref**: cyrius `docs/ffi/fncall-abi.md` ("Extern-C
  prerequisite: a glibc-compatible `%fs`").
- ⚠ **Scope of the 6.3.26 proof (added 2026-09-16):** it covered calls made
  with no value pending on the stack. Its gate calls `fncall4..7` at top level,
  in statement or left-operand position. A call nested as a later argument or
  a right operand runs **8 bytes off** alignment. See B3, which is a real
  codegen bug and a separate one from this `%fs` misdiagnosis.

### B2. 7+-param ceiling into wgpu — RESOLVED (same `%fs` misdiagnosis)

- **Status**: also not real. `fncallN` supports N ≤ 8 since cyrius
  v5.4.13, and a 7+-param cyrius fn that fncalls into a stack-protected
  extern-C callee works given the glibc launcher's `%fs`. The "ceiling"
  was the same canary fault as B1.
- **Now**: keep signatures small for readability, not for ABI safety.
- **Canonical ref**: cyrius `docs/ffi/fncall-abi.md`.

### B3. A call nested in an expression runs with rsp 8 bytes off 16-byte alignment (6.6.4)

- **Issue file**: `docs/development/issues/2026-09-16-cycc-nested-call-stack-alignment.md`
- **Symptom**: SIGSEGV (#GP) inside a C library, often only on one driver. On NVK it was
  `movdqa -0x30(%rbp),%xmm0` in `libvulkan_nouveau.so`, reached from
  `store64(pp, wgpu_device_create_buffer(...))` in `ping_pong_new`. Pure Cyrius code never
  shows it, and nothing warns at compile time.
- **Cause**: cycc pushes each evaluated argument (and the left operand of an arithmetic, shift,
  bitwise or comparison operator) and pads nothing for those pending pushes when it emits a
  nested call. With an odd number pending, the callee is entered 8 bytes off. The shift is
  inherited by everything that callee calls, so a Cyrius helper that reaches C is as exposed
  as the C call itself. Statement-level calls (`var h = f();`, `f();`, `return f();`) are
  aligned. So are the first argument and the left operand of a statement-level call or
  operator, and `&&` / `||` operands.
- ⚠ `fncallN(fp, x())` has **opposite** alignment as a bare statement (an ordinary call into
  `lib/fnptr.cyr`, callee pushed) and in expression position (lowered, callee in a frame slot).
  Do not reason about it from the source text.
- **Workaround**: bind every call that can reach C (a wgpu/samvada fn-table call, or anything
  that eventually makes one) to a local first, then pass the local. Gate:
  `scripts/check-ffi-call-alignment.py`. It reads the compiled objects, checks itself against
  a C leaf at run time, and found 4 sites in `src/` for 4.1.3. Regression test:
  `tests/tcyr/compute.tcyr` `test_ping_pong_new_ffi_calls_stack_aligned`.
- **Upstream**: to file in cyrius (High). aarch64 is unaffected (16-byte pushes). The PE
  backend already force-aligns indirect calls (`ECALLPTR_PE`, v6.0.71) for the same reason.

---

## Class C — Language sharp edges

### C1. `var X;` (bare declaration) is rejected

- **Symptom**: `var foo;` produces a parse error. Every var must
  have an initializer.
- **Workaround**: use `var foo = 0;` for "to-be-set-later" pattern.
  No semantic difference; the type inference picks i64 from the
  literal.
- **Hard rule** in CLAUDE.md.
- **Upstream**: deliberate language design (no uninitialized
  variables). Worth a clearer error message — current "expected
  expression after `=`" is misleading because there's no `=`.

### C2. Global init order — silent zero on forward references

- **Symptom**: file-scope `var X = expr;` evaluated in declaration
  order. If `expr` references a `var Y` declared later in the
  same file, `Y` resolves to `0` (zero-init default), and `X`
  silently holds the wrong value.
- **Workaround**: declare every constant referenced by another
  constant ABOVE the consumer. For complex bit-pattern composes,
  put the building-block flags at the top of the section, the
  composite at the bottom.
- **Issue file**: `docs/development/issues/2026-04-28-cyrius-global-init-order.md`
  (full repro + bench).
- **Upstream status**: slotted for `cyrius 5.7.32` per the
  language-agent review queue (2026-04-28).

### C3. `>>` on i64 is logical right shift, not arithmetic

- **Symptom**: shifting a negative-looking i64 right does NOT
  sign-extend. Bit 63 stays whatever the source had (typically
  0). For DRM canonical-high VAs (`0xFFFF800000000000` territory),
  this means `va >> 8` produces the upper-VA pattern unchanged
  in bits 56-63, but those high bits are still set — the register
  ends up encoding a 64-bit value where the kernel expected a
  packed 40+8-bit pair.
- **Workaround**: mask high VA bits explicitly when packing into
  PM4 registers:
  ```cyrius
  var pgm_lo = (va >> 8) & 0xFFFFFFFF;   # u32 mask
  var pgm_hi = (va >> 40) & 0xFF;        # u8 mask
  ```
- **Memory note**: `feedback_cyrius_logical_right_shift`.
- **Upstream**: deliberate language semantics (i64 has no signed
  vs unsigned at the syntax level). The mask discipline is the
  forever pattern.

### C4. `cyrius lint` and `cyrius fmt` are per-file in 5.7.x

- **Symptom**: bare `cyrius lint` (no file arg) prints help text;
  the repo-wide form was removed.
- **Workaround**: loop in shell — `for f in src/*.cyr; do cyrius
  lint "$f"; done`. CI does this in the `Lint` and `Format check`
  steps.
- **Memory note**: `feedback_cyrius_lint_fmt_per_file`.
- **Upstream**: deliberate split for the per-file cache work in
  5.7.x. Not a bug per se; just a workflow change worth knowing.

### C5. `cyrlint` flags multi-line asserts as warnings

- **Issue file**: `docs/development/issues/2026-04-28-cyrlint-multi-line-assert.md`
- **Symptom**: an `assert_eq(...)` call broken across multiple
  lines triggers a cyrlint "warning: function call appears
  unbalanced" warning even when the call is structurally valid.
- **Workaround**: keep asserts on one line, even when they get
  long. The repo convention is to align args column-wise on a
  single line up to whatever the natural width is, then split
  the file into a smaller test if any one line is genuinely
  unreadable.
- **Upstream**: cyrlint parser confused by multi-line bracket
  matching. Fix lives in cyrlint's expression-walker.

### C6. `cyim` regex pattern parse error

- **Issue file**: `docs/development/issues/2026-04-28-cyim-regex-pattern-error.md`
- **Symptom**: `cyim` (Cyrius IDE / module reverse-lookup tool)
  rejects certain regex patterns that should be valid PCRE-style.
- **Workaround**: avoid the regex form that trips it; use a
  simpler `grep -E` for the few cases mabda needed.
- **Upstream**: cyim's regex engine is using a subset of PCRE
  that excludes some lookahead syntax.

### C7. Call arity is a HARD ERROR since 6.5.1 (it was silent before)

- **Symptom**: code that compiled for months suddenly fails with
  `error:<source>:N: 'fn_name' expects K arguments, got J`. Nothing
  in the file changed — the toolchain pin moved.
- **Why it matters**: before 6.5.1 a call with too few args compiled
  and the missing parameters read whatever happened to be in the
  register/slot. It is not a new restriction; it is a **latent-bug
  detector**, and the errors it emits are real defects that were
  silently wrong at runtime. The 6.4.64 → 6.5.3 bump (v4.0.8) surfaced
  two in mabda: five `tests/tcyr/native.tcyr` call sites that predated
  `_native_texture_tiled_copy`'s / `native_tex_build_tiled_copy_packet`'s
  `slice` parameter, and `tests/bcyr/mabda.bcyr`'s
  `profiler_frame_cycle` bench passing the profiler handle to
  `profile_begin()` (0 params) and `profile_end(start_ns)` — so the
  benchmark never measured the round-trip it was named for.
- **Workaround**: none, and none wanted. Read each error as "this call
  was already wrong"; check what the extra parameter *means* before
  padding with `0`, because the right value is often not zero.
- **Note**: the gate is arity-only. Same-arity misdispatch is a
  separate class that 6.5.2's `_int`-overload fix narrowed.
- **Line numbers in `cyrius test` output are offset — grep instead.**
  The error names `<source>:N`, not the `.tcyr` path, and N is short of
  the real line by the file's pre-`fn` preamble (comment header +
  `include` lines + blank). Measured on `native.tcyr`: real 2158
  reported as 2151, a constant −7 across all five sites, reproduced by
  re-breaking one call. Find the site by grepping the function name the
  error quotes; do not seek to the printed line.
- **Upstream**: intentional, landed 6.5.1.

### C8. A `Result` is a register pair since 6.6.0 — hand-rolled `load64` on it fails SILENTLY

- **Symptom**: none at compile time, wrong values at runtime. Since
  6.6.0 `Result` / `Option` / `Either` are `: stack` enums: a payload
  variant returns the tag in rax and the payload in rdx, with no heap
  box. Code that still treats a Result as a pointer — `load64(r)` for
  the tag, `load64(r + 8)` for the payload — dereferences a register
  value as an address. That address often looks plausible, so the read
  succeeds and returns garbage.
- **What the compiler does catch**: `var r = f();`, `r = f();` and
  `store64(&slot, f());` on a `: stack` return are named errors at the
  offending line ("bind both"). Only the hand-rolled deref gets through.
- **Workaround**: always bind both halves (`var t, v = f();`), test the
  tag with `is_ok(t)` / `is_err_result(t)`, and read the payload as the
  plain variable `v`. On any future Result work, grep for `load64(... + 8)`
  and confirm every hit is one of mabda's own structs. Checked at 4.1.1:
  none of them read a Result. The full migration record is in CHANGELOG
  [4.1.1].
- **Upstream**: intentional (6.6.0 value form).

---

## Class D — Runtime / allocator constraints

### D1. Bump allocator exhaustion in tests

- **Symptom**: a test that heap-allocates many GpuContext-sized
  buffers (or any large struct repeatedly) eventually has `alloc()`
  return `0` (allocator exhausted). The next `store64` to that
  zero pointer SIGSEGVs.
- **Why**: Cyrius's bump allocator is process-lifetime, never
  reclaimed. Tests that mock context creation per-test exhaust
  the slab quickly. Production code is fine because real consumers
  create one ctx and reuse it.
- **Workaround**: use stack-local `var ctx[N]` for test-scoped
  buffers. mabda's pattern: `var ctx[112]; memset(&ctx, 0, 112);`
  in every test that needs a GpuContext-shaped buffer. Zero heap
  pressure, same observable shape.
- **Memory note**: `feedback_bump_allocator_tests` (informally —
  documented inline in `tests/tcyr/mabda_v3_phase_d.tcyr`).
- **Upstream**: deliberate runtime design (no GC, no free).
  Consumers writing long-running tests should know the trick.

### D2. PM4 / FFI stack discipline

- **Symptom**: large stack-locals (`var buf[1024];`) at deeply-
  nested call sites work, but combining them with FFI shim calls
  occasionally tickles a Cyrius prologue bug that overwrites the
  buffer mid-call.
- **Workaround**: keep FFI shim args in heap-allocated buffers
  (separate `alloc(N)`) when the call chain is >3 frames deep.
  Have not hit this in mabda v3 directly — flagged here as
  preventive.
- **Upstream**: speculative / hard to repro. No issue file yet.

### D3. `lib/bench.cyr` minima read low or 0; the floor is calibrated once (6.6.4)

- **Issue file**: `docs/development/issues/2026-09-16-stdlib-bench-min-minus-mean-floor.md`
- **Symptom**: a bench row prints `min=0ns` (and `CSV:<row>,0`) while its average is hundreds of
  ns. That happened to `uniform_buffer_write` in `make bench-gpu` on the hpet dev box. Less
  often, every row of one run reads low, because that process calibrated its floor at a slow
  moment (once, 3,861 ns against a real ~730 ns; trigger not reproduced).
- **Cause**: the floor subtracted from every window is a mean single-read cost, while
  `bench_report` / `bench_min_ns` report the minimum over windows. Window-to-window clock
  jitter (489 to about 1,200 ns on hpet) therefore biases minima low, down to 0 for single-op
  windows. The floor is calibrated lazily once per process and never checked against the run.
  Per-op results are also whole nanoseconds.
- **Workaround**: never time a sub-µs op one per window. Batch it so the jitter is ≤ 1 % of
  the window (`bench_run` does that; `programs/benchmarks.cyr` sizes K per row and prints the
  under-read bound). Warm up before the first `bench_clock_overhead_ns()`. Treat a `0` row as
  invalid, never as fast. `tests/bcyr/mabda.bcyr` batches 100-10,000 ops per window, so its
  rows are good to a few percent: measured ≤ 3.4 % on `rg_plan_aliasing_stats_5` even with the
  inflated floor forced.
- **Upstream**: to file in cyrius (Medium, benchmark consumers only).

---

## Class E — Bundle / distlib gotchas

### E1. distlib silently truncates bundles >64 KiB on cyrius 5.5.x and earlier

- **Symptom**: `cyrius distlib` produces a bundle that's missing
  its tail. The stdlib `[deps].stdlib` references in consumer
  manifests then fail with "module X not found in bundle."
- **Workaround**: pin cyrius ≥ 5.7.36 (the buffer was raised
  64K → 256K in that release). mabda's pin (5.11.28) is well
  above.
- **Upstream**: fixed in 5.7.36. cited above as the precedent
  for the lint + fmt fix in Class A1.

### E2. `cyrius.lock` is tracked but nothing in mabda diffs it

- **Symptom**: a resolve that pulls different stdlib bytes rewrites
  `cyrius.lock` and `lib/`, and neither CI nor the Makefile notices.
  mabda's own lock only ever showed the change in `git diff`.
- **Toolchain status**: before 6.6.4, `cyrius build` / `cyrius deps`
  would silently re-lock a stdlib file whose bytes changed even when the
  pin had not. Since 6.6.4 the lock ends with a `cyrius\t<pin>` line.
  Under an unchanged pin, a snapshot/lock disagreement is refused by name
  and no binary is built; `cyrius deps --relock` is the explicit accept.
  A lock written before 6.6.4 has no pin line, so it is accepted once and
  then gets the line. A **pin bump still re-locks silently**, because
  that is a real dependency change.
- **Workaround**: on every pin or dep bump, snapshot `lib/` hashes
  before `rm -rf lib && mkdir lib && cyrius deps`, then diff the new lock
  by hand (sort both files first, since pre-6.6.3 locks were written in
  readdir order). Record the file count and which files changed in the
  CHANGELOG entry. For 4.1.3 (6.6.2 → 6.6.4): 43 → 43 files, 10 changed.
- **Upstream**: 6.6.4 guards the unchanged-pin case. The pin-bump
  case needs a human by design.

### E3. `cyrius deps` calls an untouched dep cache "tampered" after a metadata-only change (6.6.4)

- **Issue file**: `docs/development/issues/2026-09-16-cyrius-deps-tamper-check-stale-index.md`
- **Symptom**: `error: cached checkout for dep '<name>' has local modifications ... refusing
  tampered cache`, while the files are byte-identical to HEAD. The CVE-21 tamper check
  (`_git_worktree_clean`, cyrius `cbt/deps.cyr:2828`) runs `git diff-index --quiet HEAD`
  without refreshing git's stat cache, so a `touch`, a `cp -a` of the cache, or git run
  under `unshare -rn` (the index gets rewritten with namespace uids) fails the check. The
  cache under `~/.cyrius/deps/` is shared, so every project on the machine that resolves
  that dep fails until the index is refreshed. It looks intermittent: any `git status` or
  `git diff` in the cache refreshes the index and hides it.
- **Workaround**: `git -C ~/.cyrius/deps/<name>/<tag> update-index -q --refresh`. That
  re-hashes changed-stat files and cannot hide a real edit. Don't run dep resolution under
  a uid-mapped namespace against the shared `~/.cyrius`; for an offline resolve use
  `GIT_ALLOW_PROTOCOL=file` with a populated cache. Deleting the clone, which the error
  message suggests, needs the network.
- **Upstream**: open (Low). Refresh before comparing (`update-index -q --refresh`, or
  `git diff --quiet HEAD`); both still refuse a real content edit.

---

## How to file a new toolchain issue

1. Reproduce minimally — smallest Cyrius file that exhibits the
   bug, with the exact toolchain version (`cyrius --version`).
2. Create `docs/development/issues/<YYYY-MM-DD>-<slug>.md` with:
   - Discovered date + toolchain version
   - Component (compiler / runtime / cli / lib)
   - Severity (Critical / High / Medium / Low — blast radius)
   - Summary
   - Reproduction steps
   - Expected vs actual
   - Workaround (if any)
   - Upstream status (filed / scheduled / unknown)
3. Cross-link from this doc by adding a one-paragraph entry to
   the appropriate Class section.
4. If the bug has a memory-note reciprocal (a feedback memory you
   want future agents to consult before tripping the same wire),
   add the memory note name to the entry.

---

## Forward outlook

- The two Class A items (lint / fmt 128 KiB cap) are **resolved
  upstream at cyrius 6.2.20** — buffers raised to 1028 KiB and
  truncation made fail-loud. They no longer tax CI and the local-dev
  workarounds are retired; the entries are kept only so old logs and
  old advice can be recognised as stale. ⚠ The single live hazard in
  that area now points the other way: since 6.5.28 `cyrius fmt <file>`
  REWRITES IN PLACE and `--dry`/`--check` emit a diagnostic rather than
  source, so `cyrius fmt f > f` destroys `f` outright.
- Class B (fncall ABI bugs) is partially mitigated by the wgpu C-
  shim layer, which retires per-vendor (AMD at v4.0.1, NVIDIA at
  v5.0, Intel/full at v5.1 — see roadmap). The fncall ABI bugs
  themselves stay relevant until full removal for any other C-FFI
  mabda might add. B3 (nested-call stack alignment) is a live
  codegen bug that affects every C callee, including the samvada
  dbus table, until cycc is fixed. Keep `scripts/check-ffi-call-alignment.py`
  green.
- Class C is the "this is just how Cyrius is" bucket — these are
  language design decisions or intentional sharp edges. The doc
  serves as onboarding material rather than as a bug list.
- Class D + E are mostly resolved upstream; entries kept for
  historical reference and to flag known-good toolchain floors.
