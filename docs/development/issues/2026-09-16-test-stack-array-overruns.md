# Undersized stack arrays in CPU tests and a HW program: the old texture.tcyr "leading NUL" was one

**Status:** ✅ **RESOLVED in 4.1.3** (2026-09-16). All four sites are resized, and
`scripts/check-stack-array-sizing.py` is a static gate for the class (`make
check-stack-array-sizing`, part of `make test-all` and CI). On first run the gate also found
sites outside this filing; see [Found by the gate](#found-by-the-gate).
**Placement:** `tests/tcyr/texture.tcyr:449,467` · `tests/tcyr/queue.tcyr:232,244,258,269,282,294,305,317,328`
· `programs/native_array_sample_e2e.cyr:65-70` · `programs/wgpu_texture_sample_e2e.cyr:146`
(line numbers as filed)
**Discovered:** 2026-09-16, mabda 4.1.3 verification (investigating why the documented
texture.tcyr NUL byte no longer appears)
**Toolchain:** cyrius 6.6.4 (symptom present on cyrius ≤ 6.3.14)
**Severity:** Low. Test and HW-program code only; no `src/` path is affected. Each is a write
past a declared `var buf[N]`. That is exactly the class CLAUDE.md "Security Hardening" step 2
bans. On 6.6.4 every instance happens to be benign (see below), but only because of stack
layout and call ordering.

## Summary

Test helpers memset a whole struct into a stack array that is smaller than the struct. The
arrays were sized for older struct sizes and never grew with them:

| Site | Declared | Written | Overrun |
| --- | --- | --- | --- |
| `texture.tcyr` `test_gpu_texture_create_2d_dispatch` / `_guards`, `var be[256]` via `_t5_wire` | 256 B | `memset(bebuf, 0, BACKEND_SIZE)` = 328 B | **+72 B** |
| `queue.tcyr` 9 tests, `var be[248]` via `_wire_fake_queue_ctx` or a direct `memset(&be, 0, BACKEND_SIZE)` | 248 B | 328 B | **+80 B** |
| `native_array_sample_e2e.cyr` `_sample_and_verify`, `var px[20]` / `var py[20]` | 20 B each | `store64(&px + 24, …)`: 4 × i64 = 32 B | **+12 B** each |
| `wgpu_texture_sample_e2e.cyr` `mabda_main`, `var probes[3]` | 3 B | 3 × `store64` at +0/+8/+16 = 24 B | **+21 B** |

`BACKEND_SIZE` is 328 (`src/backend.cyr:278`) since the v3.4 AA.2 slot at +320. The
`var be[248]` tests date from when it was 248.

## How this explains the texture.tcyr "leading NUL"

CLAUDE.md and `scripts/count-test-assertions.sh` said that `texture.tcyr`'s summary line
starts with a NUL byte. The 2026-06-19 handoff called that "not yet root-caused". Measured
2026-09-16:

1. **The NUL is the `"\n"` that `assert_summary()` writes first**, turned into a zero byte:
   `\0210 passed, 0 failed (210 total)`. Rebuilding mabda `bb3db74` (2026-06-19) with the
   `build/cycc` tracked at cyrius tag `6.2.22` reproduces it: stdout starts `\0 2 0 7`.
2. **It depends on the compiler, not on mabda's source.** mabda `c34084b` (4.0.10) shows the
   NUL with cycc 6.2.22 and not with cycc 6.5.29. Swapping only the stdlib (`lib@6.2.22` ↔
   `lib@6.5.29`) changes nothing.
3. **Bisecting the cycc binaries tracked at cyrius tags** (source `c34084b`, `lib@6.5.29`
   held constant): last NUL at **6.3.14**, first clean at **6.3.15**. cyrius 6.3.15 made
   `var arr[N]` locals per-thread stack slots by default instead of shared `.bss`
   globals (cyrius CHANGELOG [6.3.15]). `CYRIUS_STACK_ARRAYS=0` restores the old layout.
4. **The current tree brings it back under the old layout.** 4.1.3's `texture.tcyr` built
   with cycc 6.6.4 and `CYRIUS_STACK_ARRAYS=0` prints `\0210 passed`. With the default
   stack arrays it prints `\n210 passed`.
5. **Test-by-test isolation** under `CYRIUS_STACK_ARRAYS=0`: only a `main()` that calls just
   `test_gpu_texture_create_2d_guards` reproduces the NUL. The other 24 test fns each print
   `\n`.
6. **Resizing only the two `var be[256]` to `var be[328]`** removes the NUL under
   `CYRIUS_STACK_ARRAYS=0`. With both layouts the output is `\n210 passed`, and the rc and
   assertion count are unchanged.

So the NUL was never a printing quirk. While array locals lived in `.bss`, the 72-byte
overrun zeroed the `"\n"` literal. Since 6.3.15 the same overrun lands on the stack.

## Why it is benign today, and why that is luck

Probed on cycc 6.6.4 with the same local shapes:

- **`ctx[176]` + `be[256|248]`:** `&ctx - &be = 256` in both shapes (`be[248]` gets an 8 B
  parity pad). The overrun zeroes `ctx[0..71]`. Every affected test memsets `ctx` first,
  memsets `be` second (the overrun only rewrites zeros), and stores `ctx+32` last. A future
  test that sets a `ctx` field between the two memsets would silently lose it.
- **`px[20]` / `py[20]`:** each gets a 32 B slot (`&px - &py = 32`), so the 32 B of writes
  stay inside padding. The four probe coordinates and the surrounding scalars read back
  intact. With a different slot rounding, `py + 24` lands on `px[0..7]` and moves probe 0.
  The pixel check would still "pass", but on the wrong pixel.

The `probes[3]` site (added in `2637a52`, 2026-06-15) was missing from this filing's first
version; the review of that version found it.

## Fix (4.1.3)

- `tests/tcyr/texture.tcyr`: both tests declare `var be[328]`. `_t5_wire` now takes the
  buffer's size and refuses (returns -1, writes nothing into it) when it is below
  `BACKEND_SIZE`. `test_t5_backend_buffer_holds_backend_size` asserts
  `BACKEND_SIZE <= _T5_BE_BYTES` (328) and that the refusal works.
- `tests/tcyr/queue.tcyr`: the nine tests declare `var be[328]`. `_wire_fake_queue_ctx` and a
  new `_wire_null_queue_ctx` (for the three null-slot tests, which memset directly before)
  take the size and refuse a short buffer. `test_q_backend_buffer_holds_backend_size`
  asserts the same pair of facts.
- `programs/native_array_sample_e2e.cyr`: `var px[32];` / `var py[32];` (4 × i64).
- `programs/wgpu_texture_sample_e2e.cyr`: `var probes[24];` (fixed in the same 4.1.3 pass).

The sizes are literals. cyrius sizes an array only from a literal or an enum constant, and
`BACKEND_SIZE` is a `var`. An enum declared in the test file does not work either: the
compiler accepts an enum constant as an array size only when its global index is below 1024
(`src/frontend/parse_decl.cyr:56` in cyrius 6.6.4), and every suite that includes
`src/lib.cyr` is far past that. The error message still suggests `enum Sz { N = 16; }
var buf[N];`. The runtime refusal plus the static gate cover the literal drifting from
`BACKEND_SIZE`.

**Proof, `CYRIUS_STACK_ARRAYS=0` (the pre-6.3.15 layout) on cycc 6.6.4:**

| `texture.tcyr` | byte before the summary | NUL bytes in stdout | summary |
|---|---|---|---|
| before (`var be[256]`) | `\0` | 1 | 210 passed, 0 failed |
| after (`var be[328]`) | `\n` | 0 | 214 passed, 0 failed |

With the default stack layout both builds print `\n` and exit 0.

## The gate: `scripts/check-stack-array-sizing.py`

For every `var NAME[N]` / `var NAME: T[N]` (local or global) and every scalar local in
`src/ programs/ tests/ fuzz/ examples/`, it resolves the writes that reach `&NAME`: memset,
memcpy/memmove, `storeNN`, the `read` / `pread64` / `getrandom` / `clock_gettime` /
`gettimeofday` / `nanosleep` syscalls, `ioctl` with a constant `_IOC_READ` request (the
kernel copies `_IOC_SIZE` bytes back), and any function that writes through a parameter,
transitively, with offsets and sizes re-expressed in the caller's arguments. Values resolve
through literals, never-reassigned `var` constants, enum members, single-assignment locals and
simple loop counters. A callee write is skipped only when the call's constant arguments decide
a guard against it (an earlier `if (...) { return ...; }` that is certainly taken, or an
enclosing `if` certainly not). It fails on any write outside the object, and its
`--self-test` covers each detection and exemption path on fixtures.

On the tree before these fixes it reports every site above: 2 in `texture.tcyr`, 9 in
`queue.tcyr`, 4 writes in `native_array_sample_e2e.cyr` and 3 in
`wgpu_texture_sample_e2e.cyr`.

## Found by the gate

The first full run also flagged test buffers outside this filing's scope. All of them are in
test code, and each needs its own owner's change:

- **A real overrun, reached at run time:** `tests/tcyr/native.tcyr`
  `test_native_texture_create_2d_array_guards` and `test_native_texture_create_cube_guards`
  pass a fake context `var ctxf[8]` (fd field 1), and their last case asserts
  "compressed array/cube -> 0 (AA.4)". That case is stale: AA.4 (`c8d574a`) made BC
  arrays and cubes legal, so the call walks past every guard into
  `native_ctx_alloc_texture_va`, which reads its VA cursor from `ctx + 120`, past the 8 B
  array. Whenever those stack bytes are zero it writes `ctx + 120` and the create goes on to
  a GEM_CREATE ioctl on fd 1, which fails, so the call returns 0 and the assertion passes for
  the wrong reason. A CPU-only replay (fd -1) against a zero-filled buffer showed the write
  at +120..+128; with non-zero bytes there the cursor check returns early instead.
- **Latent, guarded only by a loaded field or a function's return value:** `native.tcyr`
  `test_native_tex_chain_size_and_fmt_mipped_guards` (`var ctxbuf[8]`, writes reachable at
  +120) and `test_native_texture_release_zero_handle_safe` (`var tex[32]`, +52),
  `backend.tcyr` `test_backend_wgpu_texture_read_compressed_fail_loud` (`var dst[64]`, told
  n = 999), and `render.tcyr` `test_rg_execute_mq_forwards_v25` (`var fakectx[40]`, +152 and
  +160). The replay showed no write today; a regression in the guard would corrupt the
  stack instead of failing an assertion, which is how the `be[256]` overrun survived.

The gate fails until those buffers are sized for what the callee may write. The fixes (a
full-size, zeroed fake context with fd -1 and a non-sampleable format for the stale BC1
cases, plus a no-VA-reserved assertion) were handed to the owners of those files with this
change.

The first heuristic scan (2026-09-16, before the gate) missed `probes[3]` and also flagged
`native.tcyr` `test_native_render_pass_draw_rejects_bad_vc_ic` (`var pass[8]`). The gate
does not flag that one: every call passes `vc <= 0` or `ic != 1`, which the callee's
guards reject before touching `pass`, and the gate evaluates those guards.
`scripts/check-compiler-buffer-sizing.py` is the older gate of this family, covering the
compiler's scratch buffers only.
