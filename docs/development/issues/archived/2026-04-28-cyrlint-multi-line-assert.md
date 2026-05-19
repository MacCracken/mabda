# Issue: `cyrius lint` reports phantom "unclosed braces at end of file" on large test files

**Status:** ✅ **RESOLVED in cyrius v5.8.41** (verification slot,
2026-05-03). Premise check at v5.8.41 entry could not reproduce
the bug across 4 synthetic repros (3500-line plain-vars file,
mabda's actual mabda.tcyr at 2743 lines, doubled mabda content
at 5486 lines, and a 9007-line synthetic with 600 fns + multi-
line `assert_eq()` calls + comments) — all returned 0 warnings.
Likely fixed by intermediate cyrlint refactor / brace-tracker /
string-literal-awareness work between v5.7.23 (filing) and
v5.8.40 (premise check); this issue file was never updated
post-resolution. v5.8.41 ships a regression-floor gate at
`cyrius/tests/regression-cyrlint-large-file.sh` that locks the
fixed state into CI: a 7010-line synthetic file matching this
issue's repro shape (multi-line `assert_eq()` calls, 700 fns,
test_groups, comments) must produce 0 "unclosed braces" / 0
"trailing whitespace" warnings or the gate fails.

**If you hit a NEW variant of this bug**: file a NEW issue with
fresh repro details + repo location; reference this issue's
RESOLVED status. Don't reopen this one.

**Discovered:** 2026-04-28 (mabda v3 Step 3d, refined Step 3e)
**Component:** `cyrius lint` (toolchain — `cyrius 5.7.23` at filing; resolved by `cyrius 5.8.41`)
**Severity:** Low (false positive, not blocking compile/test, but
breaks CI gating per `mabda/CLAUDE.md`)
**Workarounds (no longer needed at cyrius >= 5.8.40; kept for historical reference):**
1. Avoid multi-line column-aligned `assert_eq()` continuations
2. Avoid pushing total `tests/tcyr/mabda.tcyr` length past ~3270 lines
   without splitting into separate test files

## Summary

`cyrius lint` reports two classes of false positives on
`tests/tcyr/mabda.tcyr` once the file grows past ~3270 lines:

```
warn line N: trailing whitespace
warn line N+1: unclosed braces at end of file
```

(Sometimes only the second warning appears.) The warnings reference
line numbers far from any actual offending content. Brace counts
audit balanced. Whitespace inspection shows no trailing characters.
`cyrius test` runs the file cleanly and passes every assertion.

I initially diagnosed this as a multi-line `assert_eq()` parser
bug (Step 3d). Re-bisected during Step 3e (2026-04-28) and the
real trigger is **file size threshold**, not formatting:

| State                                 | Lines | Lint    |
|---------------------------------------|-------|---------|
| End of Step 3d                        | 3240  | clean   |
| After bisect-strip Step 3e content    | 3236  | clean   |
| Step 3e content fully re-added        | 3309  | warning |

A bisect inside Step 3e showed adding the 7th `var x = 0;`
declaration in a row was the visible trigger — but renaming the
variable, packing multiple per line, or moving them elsewhere
didn't help. The common factor is total file line count. The
warning text is misleading; the actual heuristic appears to be
file-length-related.

## Reproduction

```sh
# In any large mabda test file:
git checkout v3
cyrius lint tests/tcyr/mabda.tcyr
# 0 warnings (clean at HEAD)

# Append ~70 lines of valid test content (any new fns, vars,
# captures with single-line assert_eq calls).
# Re-run:
cyrius lint tests/tcyr/mabda.tcyr
# warn line N: trailing whitespace
# warn line N+1: unclosed braces at end of file
```

The warning reproduces deterministically once the file grows past
the threshold, regardless of what the new content is or whether
the new content has any actual lint issues. Trimming earlier
content back below the threshold clears it.

## What I tried that did NOT help

- Renaming the 7th-added variable (every name fails uniformly)
- Packing multiple `var` declarations per line
- Moving the captures and tests into a different ordering
- Replacing wrapped multi-line `assert_eq()` calls with single-line
  forms (this WAS necessary in Step 3d for a related symptom, but
  is not sufficient to clear the size-based warning at scale)
- Removing the section comment block (made the warning count go
  from 1 to 2, suggesting the heuristic involves more than one
  miscount)

## Why it matters

Per `mabda/CLAUDE.md` ("CI / Release"): *"CI fails on any
`cyrius lint` warning, `cyrius fmt --check` drift, or `cyrius vet`
finding."*

This means: as `tests/tcyr/mabda.tcyr` grows organically (which it
does with every new feature step in the v3 work), it will hit a
hard-to-predict size threshold and start failing CI on a working,
test-passing file. Without a known cause, every new contributor
will burn an hour bisecting their own innocent additions.

## Suggested upstream fixes (in priority order)

1. **Identify what file-size-related state the linter is mishandling.**
   The warning text ("unclosed braces") is wrong for the actual
   problem; whatever heuristic fires past ~3270 lines needs to be
   checked against ground truth (a real `count('{') - count('}')`
   pass) before emitting.
2. **Fix the underlying bug** — likely a buffer / parser-state
   limit, possibly a count overflow, possibly an off-by-one in a
   chunk-based parse. Without source access I can't be specific.
3. **As a stopgap, change the warning text** to something less
   misleading. "Internal linter limit reached near line N (file
   may be too large)" would at least tell the developer the
   problem is in the tooling rather than their code.

## Filing trail

- mabda v3 Step 3d work, 2026-04-28: first noticed; mis-diagnosed
  as a multi-line assert wrapping bug.
- mabda v3 Step 3e work, 2026-04-28: re-bisected to confirm
  file-size dependency; the multi-line wrapping was a coincidence
  (Step 3d's final inline-assert version cleared it not because
  inline asserts fix it but because removing the wrapped lines
  shrank the file enough to drop below threshold).
- `tests/tcyr/mabda.tcyr` currently sits at 3307 lines with one
  visible warning. The file passes `cyrius test` cleanly.

## Local handling for v3.0 release

Mabda's v3.0 punch list (`docs/development/3-0-punchlist.md`)
already requires `cyrius lint` clean as a Tier 5 release-engineering
gate. To clear that gate without an upstream fix, mabda has two
options:

1. **Split `tests/tcyr/mabda.tcyr` into multiple files**, each
   below the threshold. Run them all from the Makefile.
2. **Wait for the upstream fix** before final v3.0 cut.

Option 1 is the safer path — splits also make the test suite easier
to navigate as it grows.
