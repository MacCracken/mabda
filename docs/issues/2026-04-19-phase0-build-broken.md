# 2026-04-19 — GPU integration build broken on `main`

**Severity:** HIGH — blocks the v2.4.0 closeout gate
(`make test-phase0` / `make test-compute-e2e` / `make test-render-e2e`)

**Affected:** any `programs/*.cyr` that links against
`deps/wgpu_main.c` (i.e. the whole GPU-integration path)

**Status (2026-04-19, late):** both blockers resolved.
- Issue 1 — fixed upstream in **cyrius v5.4.9**, shipped in v5.4.10
  (cyrius roadmap `docs/development/roadmap.md` line 306 links back
  to this file). Mabda bumped `cyrius.cyml` pin `5.4.7 → 5.4.10`.
- Issue 2 — fixed mabda-side (`programs/phase0.cyr` +1 include line).
- Dev-box runtime note: on a host without wgpu-native-compatible
  GPU/driver, `wgpuCreateInstance(NULL)` in `deps/wgpu_main.c`
  `preinit_gpu()` SIGSEGVs during C init. Not a mabda/cyrius bug —
  environment-dependent. Runtime validation requires a wgpu-capable
  host.

---

## Summary

Attempting `make test-phase0` against the current `main` fails at
the link step with two classes of errors:

1. **`undefined reference to _cyrius_init`** — raised by
   `deps/wgpu_main.c:290` where the C launcher calls
   `_cyrius_init()` before `mabda_main()`.
2. **`undefined reference to str_builder_new / str_builder_add_cstr
   / str_builder_build / str_cstr`** — raised at 8 sites in
   `programs/phase0.cyr`.

Neither is caught by CPU-only `cyrius test` because the CPU suite
does not exercise the GPU integration link path, and the GPU path
is not a CI gate (runners lack `wgpu-native`). Both regressed
silently.

## Reproduction

On `main` (commit `865b45b` at time of filing), with
`deps/wgpu-native/` present:

```
make test-phase0
```

Output (trimmed):

```
warning: undefined function 'str_cstr'
warning: undefined function 'str_builder_build'
...
/usr/bin/ld: deps/wgpu_main.o: in function `main':
wgpu_main.c:(.text+0xf32): undefined reference to `_cyrius_init'
/usr/bin/ld: build/phase0.o: in function `mabda_main':
(.text+0x11494): undefined reference to `str_builder_new'
/usr/bin/ld: (.text+0x114b2): undefined reference to `str_builder_add_cstr'
...
collect2: error: ld returned 1 exit status
```

`cc5 --version` reports `cc5 5.4.7`, matching the `cyrius.cyml` pin.

---

## Issue 1 — `_cyrius_init` emitted as LOCAL (upstream toolchain regression) — FIXED in cyrius v5.4.9

### Root cause

`cc5` in `object;` mode currently emits `_cyrius_init` as a LOCAL
symbol, so external callers (the C launcher) cannot link against
it:

```
$ readelf -s build/phase0.o | grep _cyrius_init
  13: 000000000001170b  0 FUNC   LOCAL  DEFAULT  1 _cyrius_init
$ readelf -s build/phase0.o | grep mabda_main
 380: 000000000000ff57  0 FUNC   GLOBAL DEFAULT  1 mabda_main
```

User-defined functions (`mabda_main`) are global; the
compiler-generated `_cyrius_init` is local. The cyrius linker
(`cyrld`) handles this correctly because each module has its own
`_cyrius_init` which it stubs together in a `_start` call chain
(see `cyrius/programs/cyrld.cyr:1038-1105`). But when the entry
point is C's `main()` — not cyrius's `_start` — the single
`_cyrius_init` of the compiled object must be visible externally.

Per the project memory `project_textrel_blocker.md`, this symbol
was explicitly fixed at cyrius 3.4.14 ("`_cyrius_init` export
(3.4.14)"). It has since regressed in the 5.x line — confirmed
against 5.4.7.

### Scope

- **Upstream:** cyrius toolchain. The `object;` directive should
  emit `_cyrius_init` with GLOBAL binding (or provide an explicit
  attribute like `@export` / `pub fn`).
- **Downstream:** every project that links a cyrius-compiled object
  into a C-entry-point executable. Today that's mabda
  (`programs/phase0.cyr`). Consumers of mabda that roll their own
  C launcher (soorat, rasa, ranga, bijli, aethersafta, kiran) will
  hit the same wall the moment they update their cyrius pin to
  5.4.7.

### Resolution

Fixed upstream in **cyrius v5.4.9** — the cyrius team (who track
mabda's issues directly from this doc — see
`cyrius/docs/development/roadmap.md` line 306) flipped the
`STB_LOCAL` binding for `_cyrius_init` to `STB_GLOBAL` in
`src/backend/x86/fixup.cyr` `EMITELF_OBJ` path, with cyrld updated
to rename per-module inputs (`_cyrius_init` → `_cyrius_init_<mod>`)
before stitching the call chain.

Mabda bumped `cyrius.cyml` `cyrius = "5.4.7" → "5.4.10"`; link
verified clean with `make test-phase0` (no `undefined reference`
on `_cyrius_init`). No mabda-side workaround (`objcopy
--globalize-symbol`) needed.

Regression test committed upstream at
`cyrius/tests/regression-object-init.sh` — cyrius CI asserts
`_cyrius_init` is `GLOBAL` in every `object;` mode output going
forward.

---

## Issue 2 — `phase0.cyr` missing `include "lib/str.cyr"` (mabda-side bug) — FIXED

### Root cause

`programs/phase0.cyr:305-311` builds the WGSL triangle shader via:

```
var _sb = str_builder_new();
str_builder_add_cstr(_sb, "@vertex fn vs_main(...");
...
var wgsl = str_cstr(str_builder_build(_sb));
```

Those four symbols live in `lib/str.cyr`. Phase0's selective include
block (lines 10-17) lists `string`, `fmt`, `alloc`, `syscalls`,
`fnptr`, `hashmap`, `vec`, `tagged` — but **not** `str`. `cc5` warns
(`warning: undefined function 'str_cstr'`) but still produces an
object; the linker fails later.

Most likely introduced when the WGSL source was refactored from a
single string literal into the two-half `str_builder` form without
the include chain being updated.

### Proposed fix

Add the missing include to `programs/phase0.cyr` (and to the new
`programs/compute_e2e.cyr` and `programs/render_e2e.cyr` while they
are being authored):

```
include "lib/str.cyr"
```

Placed between `lib/vec.cyr` and `lib/tagged.cyr` to match the
ordering convention in `src/lib.cyr`.

### Regression prevention

- Escalate `cc5` `undefined function` warnings to hard errors for
  `programs/*.cyr` builds. Currently they print but the object is
  still produced, so CI misses it.
- Add a `make build-gpu-programs` target that stops at the object
  stage (no wgpu-native link required) — runnable in CI — and fails
  on any `cc5` warning. This catches the missing-include class
  without needing `libwgpu_native.a` on the runner.

---

## v2.4.0 impact

The v2.4.0 exit criteria include:
- `make test-phase0` passes on a box with wgpu-native
- Two new programs (`compute_e2e.cyr`, `render_e2e.cyr`) cover the
  last two v1.0 criteria

Both depend on the GPU integration build path working. Until issues
1 + 2 land (at minimum the local workarounds), v2.4.0 cannot close.

## Resolution order (executed)

1. ✅ **Cyrius v5.4.9** landed Issue 1 fix with regression test. v5.4.10
   shipped with additional majra-related thread.cyr fixes.
2. ✅ **Mabda cyrius.cyml pin** bumped `5.4.7 → 5.4.10`.
3. ✅ **Mabda Issue 2 fix** — one-line `include "lib/str.cyr"` added
   to `programs/phase0.cyr`. New programs (`compute_e2e.cyr`,
   `render_e2e.cyr`) carry the include from day one.
4. ⏳ **Queued for v5.4.10+ (cyrius side):** `--strict` flag (or
   equivalent) to escalate `undefined function` warnings to errors
   at compile. Closes the Issue-2-class of bug for all downstreams.
5. ⏳ **Pre-2.4.0 (mabda side):** `make build-gpu-programs` CI target
   that compiles-but-doesn't-link every `programs/*.cyr` and fails
   on any `cc5` warning. Runs without wgpu-native on CI runners.
   Remove once the upstream `--strict` flag lands and we can use it
   directly.

## Dev-box runtime note

After applying all fixes, `make test-phase0` links clean and the
binary starts executing. On a host without a wgpu-native-compatible
GPU / driver, `wgpuCreateInstance(NULL)` SIGSEGVs inside the C
preinit path (before any cyrius code runs). Instrumentation confirms
`_cyrius_init` and `alloc_init` both succeed; the crash is in the C
preinit, in `wgpu-native` itself.

This is **environment-dependent**, not a mabda or cyrius defect.
The v2.4.0 compute/render E2E programs will share this characteristic:
compile-clean + link-clean in CI, runtime validation requires a
dev box with a working wgpu-native stack.

## References

- Memory: `project_textrel_blocker.md` — records the 3.4.14 fix
- Memory: `feedback_fncall6_wgpu.md` — adjacent FFI gotcha, already
  worked around
- Upstream: `cyrius/programs/cyrld.cyr:732,1103` — where the cyrius
  linker treats `_cyrius_init` as a module-local symbol (this is
  correct for cyrius-linked binaries; the issue is that `object;`
  mode should override it for external linkers)
