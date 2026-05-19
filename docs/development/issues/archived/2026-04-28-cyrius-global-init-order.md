# Issue: Cyrius global init order — silent zero for forward refs — RESOLVED

**Status:** ✅ **RESOLVED in cyrius v5.7.32** (cyrlint global-init-
order forward-ref warning shipped). `cyrius lint` now emits a
warning at every top-level `var X = expr;` whose `expr` references
a `var` declared at a line > X's line. Mabda's repro
(`_NATIVE_PERM_FULL = AMDGPU_VM_PAGE_R | W | X` evaluating to 0
because the perm constants were declared 274 lines later) would
now warn at `cyrius lint` time before the silent miscompile lands
in CI / hardware test. v5.7.36 added string-literal awareness so
identifier-shaped substrings inside `"..."` literals don't
false-positive. Regression gate at
`cyrius/tests/regression-lint-global-init-order.sh` (4 sub-tests).
Cyrius-side mirror archived 2026-05-03 to
`cyrius/docs/development/issues/archived/2026-04-28-global-init-
order-forward-ref.md`.

**If you hit a NEW variant of this bug**: file a NEW issue with
fresh repro details + repo location; reference this issue's
RESOLVED status. Don't reopen this one.

**Discovered:** 2026-04-28 (mabda v3 Step 4f.ii — BO page perms tightening)
**Component:** `cyrius` compiler / runtime — global initializer evaluation
**Severity:** Medium (silent miscompile; surfaces as runtime zeros that
look like working code)
**Toolchain at discovery:** `cyrius 5.7.23` (also reproduces at `5.7.28`); resolved by `cyrius 5.7.32`

## Summary

Cyrius initializes top-level `var X = expr;` declarations in source
declaration order. If `expr` references a constant declared **later**
in the same file, the reference resolves to `0` (the default
zero-initialized value) at the time `X` is evaluated. No warning,
no error — `X` ends up holding the wrong value, and downstream
consumers silently see the wrong value too.

## Reproduction

```cyrius
# In src/foo.cyr (or any single Cyrius file):

var COMPUTED = FLAG_A | FLAG_B | FLAG_C;   # → 0, not 7

# ... 200 lines later ...

var FLAG_A = 0x1;
var FLAG_B = 0x2;
var FLAG_C = 0x4;

# COMPUTED reads as 0 throughout the program.
```

A trivial smoke test surfaces it:

```cyrius
fn test() {
    assert_eq(COMPUTED, FLAG_A | FLAG_B | FLAG_C, "computed = OR of flags");
    # → fails with "got 0, expected 7"
}
```

## Why it bit hard during Step 4f.ii

I added named perm-bitmask constants near the top of
`src/backend_native.cyr`:

```cyrius
var _NATIVE_PERM_FULL = AMDGPU_VM_PAGE_READABLE
                      | AMDGPU_VM_PAGE_WRITEABLE
                      | AMDGPU_VM_PAGE_EXECUTABLE;
```

…at line 117. The `AMDGPU_VM_PAGE_*` constants live at line 391+ in
the same file. Result: `_NATIVE_PERM_FULL` evaluated to `0` at load
time. Every BO got mapped with `perms = 0` — no read, no write, no
execute. Every dispatch TDR'd at the 10-second AMDGPU timeout with
"post-dispatch marker stale" and "output unchanged."

Visible failure mode looked like a wedged GPU — Session 23's
"3 MODE2/boot before permanent wedge" rule made this a plausible
diagnosis, and I burned ~30 minutes investigating that hypothesis
before a CPU regression test (`assert_eq(_NATIVE_PERM_FULL, R|W|X)`)
returned `got 0, expected 14` and pinned the actual cause.

The fix was a one-line move: `_NATIVE_PERM_*` block relocated
below the `AMDGPU_VM_PAGE_*` defs. Code that was "wrong since the
moment I typed it" started working immediately.

## Why this is worth fixing upstream

The misdiagnosis cost matters. Hardware iteration on AMDGPU is
expensive — every TDR puts the firmware closer to a permanent
wedge (per Session 23's rule) and complicates any other work
running on the same GPU. Spending half an hour on the wrong
hypothesis because the language silently swallowed a forward
reference is a trap that any new contributor will fall into.

Beyond the specific bit-flag pattern, this affects:

- Any computed-from-other-constants global (size calculations,
  page perms, bit-packed enum compositions)
- Refactors that reorder top-level declarations
- Pattern of "define module constants near the top of the file
  for grep-ability"

## Suggested upstream fixes (in priority order)

1. **`cyrius lint` warning for forward references in top-level
   initialisers.** Walk the file; if any `var X = expr` references
   a symbol that appears later in the source, emit a warning. The
   warning text should be specific: "global '_NATIVE_PERM_FULL' at
   line 117 references 'AMDGPU_VM_PAGE_READABLE' defined at line
   391 — this evaluates to 0 at load time. Move the dependency
   above, or move the reference below."
2. **Compile-time error** if the language requires declaration
   order. The current behaviour (silent zero) is the worst of
   both worlds — neither permissive (let me forward-reference) nor
   strict (tell me I can't). Pick one.
3. **Runtime check** that warns when zero-initialized memory is
   read by a `var` initializer that the compiler can statically
   identify as a forward reference. Cheaper than option 2 but
   still surfaces the bug deterministically.

## Filing trail

- Mabda v3 Step 4f.ii, 2026-04-28: encountered + diagnosed +
  fixed in a single session. Memory note:
  `feedback_cyrius_global_init_order.md`.
- Workaround applied locally: place computed-constant blocks
  AFTER their dependencies; CPU regression tests now assert the
  *value* of every computed perm constant.
