# Toolchain issues observed during the v3 cycle

**Date filed:** 2026-04-30 (v3.0.0-rc.1 cut)
**Toolchain at filing:** `cyrius 5.7.48` (mabda's pin at filing; bumped to `5.11.28` 2026-05-12 ahead of rc.3 soak)
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

### A1. `cyrius lint` and `cyrius fmt` cap reads at 128 KiB

- **Symptom**: lint reports bogus "unclosed braces at end of file"
  warnings near the end of files >131,072 bytes; fmt drops the
  end-of-file content (its stdout is shorter than the input by
  the bytes-past-131072), so any redirect-back-to-file destroys
  data.
- **Affected today**: `src/backend_native.cyr` (137 KiB).
- **Workaround in tree**: CI fmt-check skips files >128 KiB
  (`.github/workflows/ci.yml`). Splitting `backend_native.cyr` into
  smaller modules to drop the workaround is in the rc.2 punchlist.
- **Workaround for local dev**: never run `cyrius fmt $f > $f`
  blindly. Use the line-count guard pattern:
  ```sh
  cyrius fmt $f > $f.fmt && \
    [ $(wc -l < $f) = $(wc -l < $f.fmt) ] && \
    mv $f.fmt $f || { echo "REJECT truncation"; rm -f $f.fmt; }
  ```
- **Memory note**: `feedback_cyrlint_128k_buffer_cap.md` — covers
  both lint and fmt since they share the buffer-cap bug class.
- **Upstream**: distlib has the same bug class; it was fixed in
  cyrius 5.7.36 (`cbt/commands.cyr` raised 64K → 256K). lint and
  fmt need the same treatment. Real fix: bump the buffer to
  ≥524288 (matching distlib's 256K precedent — actually 4× because
  fmt produces output that may be larger than the input).

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

### B2. 7+-param ceiling into wgpu — RESOLVED (same `%fs` misdiagnosis)

- **Status**: also not real. `fncallN` supports N ≤ 8 since cyrius
  v5.4.13, and a 7+-param cyrius fn that fncalls into a stack-protected
  extern-C callee works given the glibc launcher's `%fs`. The "ceiling"
  was the same canary fault as B1.
- **Now**: keep signatures small for readability, not for ABI safety.
- **Canonical ref**: cyrius `docs/ffi/fncall-abi.md`.

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

- The two Class A items (lint / fmt 128 KiB cap) are the most
  immediate friction; they tax CI today and add manual workaround
  steps to local dev. Bumping the toolchain buffers is small + high
  leverage.
- Class B (fncall ABI bugs) is partially mitigated by the wgpu C-
  shim layer, which retires per-vendor (AMD at v4.0.1, NVIDIA at
  v5.0, Intel/full at v5.1 — see roadmap). The fncall ABI bugs
  themselves stay relevant until full removal for any other C-FFI
  mabda might add.
- Class C is the "this is just how Cyrius is" bucket — these are
  language design decisions or intentional sharp edges. The doc
  serves as onboarding material rather than as a bug list.
- Class D + E are mostly resolved upstream; entries kept for
  historical reference and to flag known-good toolchain floors.
