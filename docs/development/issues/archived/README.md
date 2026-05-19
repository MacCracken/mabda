# Archived Issues (mabda)

Resolved issue reports. Kept for history — so the next agent (or
the next person grepping a symptom) can find the fix version
without re-investigating.

**Filing a new issue?** Drop it in the parent
`docs/development/issues/` folder — this folder is history only.

**Convention** mirrors the cyrius-side archive at
`cyrius/docs/development/issues/archived/`:

- File header gains a `— RESOLVED` suffix and a status
  paragraph at the top pointing at the fix version (cyrius
  CHANGELOG section / mabda commit / external repo + commit
  for cross-repo fixes).
- File name is unchanged so external links (cyrius CHANGELOG
  refs, PR descriptions, vidya entries) keep working.
- If a resolved issue returns under a new manifestation, open
  a fresh issue in the parent folder and cross-reference this
  one. Don't resurrect archived files in place.

## Index

| File | Brief | Resolved in |
|------|-------|-------------|
| [`2026-04-28-cyrius-global-init-order.md`](./2026-04-28-cyrius-global-init-order.md) | Top-level `var X = expr;` referencing a `var Y` declared LATER in the same file silently evaluated to 0. Mabda burned ~30 min on a "wedged GPU" hypothesis (`_NATIVE_PERM_FULL = AMDGPU_VM_PAGE_R \| W \| X` evaluated to 0; every BO mapped with no perms; every dispatch TDR'd) before a CPU regression test pinned the cause. | **cyrius v5.7.32** — `cyrlint` `lint_globals_init_order` rule (parse-time AST walk emits warning if init-expr references a var declared later). v5.7.36 added string-literal awareness so identifier-shaped substrings inside `"..."` don't false-positive. Cyrius mirror archived at `cyrius/docs/development/issues/archived/2026-04-28-global-init-order-forward-ref.md`. |
| [`2026-04-28-cyrlint-multi-line-assert.md`](./2026-04-28-cyrlint-multi-line-assert.md) | `cyrius lint` reported phantom "unclosed braces at end of file" + "trailing whitespace" warnings on `tests/tcyr/mabda.tcyr` past ~3270 lines. Re-bisected during Step 3e — real trigger was FILE-SIZE THRESHOLD, not multi-line asserts (the original Step 3d diagnosis). Workaround: split test files / avoid multi-line `assert_eq()` continuations. | **cyrius v5.8.41** — verification slot. Premise check at v5.8.41 entry across 4 synthetic repros (3504-line plain-vars, mabda's actual mabda.tcyr at 2743 lines, doubled mabda content at 5486 lines, 9007-line synthetic with multi-line `assert_eq()` + comments) found bug NOT REPRODUCING — likely fixed by intermediate cyrlint work between v5.7.23 and v5.8.40. v5.8.41 ships a regression-floor gate at `cyrius/tests/regression-cyrlint-large-file.sh` (7010-line synthetic). Mabda's two pre-fix workarounds (split files / avoid multi-line continuations) are no longer required at cyrius >= 5.8.40. |
