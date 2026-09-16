# `cyrius lint` cannot pass `--strict-deferrals` to cyrlint: usage error or silently ignored

**Status:** open upstream (cyrius). Not filed in the cyrius repo from here; this is
mabda's record with a minimal repro.
**Discovered:** 2026-09-16, mabda 4.1.3 verification sweep (untracked-deferral audit)
**Toolchain:** `cyrius 6.6.4`. The code is unchanged at cyrius `HEAD` (`4f3731e8`, `6.6.4-1`).
**Component:** cli (`cyrius lint` dispatcher, `cbt/cyrius.cyr` + `cbt/commands.cyr`)
**Severity:** Low. There is a working form (`cyrlint` directly). But one of the two wrapper
spellings **silently disables the gate** and exits 0, which a CI step reads as a pass.

## Summary

`cyrlint` supports `--strict-deferrals`: exit 2 when a file has untracked deferral markers.
It accepts the flag before or after the path (`programs/cyrlint.cyr`, `main()` argument
loop). The `cyrius lint` wrapper knows only `--strict`:

- `cbt/cyrius.cyr:1104-1124` (the `lint` dispatcher) takes the **first argument that isn't
  `--strict` as the file** and ignores every later argument except `--strict`.
- `cbt/commands.cyr:1547` `cmd_lint(file, strict)` forwards only `--strict` and the file
  to cyrlint.

So:

| Command | What cyrlint receives | Result |
| --- | --- | --- |
| `cyrius lint --strict-deferrals f.cyr` | `cyrlint --strict-deferrals` (the file is dropped) | cyrlint usage text, **exit 1** on every file |
| `cyrius lint f.cyr --strict-deferrals` | `cyrlint f.cyr` (the flag is dropped) | deferrals reported, **exit 0**: the gate is off |
| `cyrlint --strict-deferrals f.cyr` | as typed | **exit 2** (correct) |
| `cyrlint f.cyr --strict-deferrals` | as typed | **exit 2** (correct) |

The flag-first failure is loud. The flag-last form is the dangerous one. It prints the
same `deferral line N: untracked ...` notes as a plain lint, and nothing in the output says
the flag was ignored.

This is the same argument-parser family as cheat-sheet entry A3 (`cyrius fmt --check
<file>` treats the flag as the file).

## Reproduction

Verified 2026-09-16 on cyrius 6.6.4. Run it with `bash` in any empty directory:

```sh
L=$HOME/.cyrius/versions/6.6.4/bin/cyrlint
printf '# a deferred item with no tracking pointer\nfn f(): i64 { return 0; }\n' > d.cyr
cyrius lint --strict-deferrals d.cyr; echo "exit=$?"   # Usage: cyrlint ...  exit=1
cyrius lint d.cyr --strict-deferrals; echo "exit=$?"   # 1 untracked deferrals  exit=0
cyrius lint d.cyr; echo "exit=$?"                      # 1 untracked deferrals  exit=0
$L --strict-deferrals d.cyr; echo "exit=$?"            # 1 untracked deferrals  exit=2
$L d.cyr --strict-deferrals; echo "exit=$?"            # 1 untracked deferrals  exit=2
```

## Expected vs actual

- **Expected:** `cyrius lint <file> --strict-deferrals` and `cyrius lint --strict-deferrals
  <file>` both forward the flag and exit 2 on an untracked deferral, the same as `cyrlint`.
  An unrecognized flag is an error, never a file name and never silently dropped.
- **Actual:** see the table above.

## Impact on mabda

mabda's CI `Lint` step runs `cyrius lint "$f"` and fails only on `warn ` lines. Deferral
notes never fail it, and the wrapper offers no way to make them fail. Three untracked
deferrals (`programs/native_texture_alloc_e2e.cyr`, `programs/nvidia_kms_scanout.cyr`,
`tests/tcyr/compiler_backend.tcyr`) shipped in every release from 4.1.0 to 4.1.2. The
oldest had been in the tree since 2026-06-20. They were only found by running
`cyrlint --strict-deferrals` by hand during the 4.1.3 sweep.

## Workaround

Call `cyrlint` directly, per file:

```sh
for f in src/*.cyr programs/*.cyr tests/tcyr/*.tcyr tests/bcyr/*.bcyr fuzz/*.fcyr; do
  cyrlint --strict-deferrals "$f" || exit 1
done
```

Use the versioned binary (`$HOME/.cyrius/versions/<pin>/bin/cyrlint`) when the pin may
differ from the active toolchain. `~/.cyrius/bin/cyrlint` is whatever version is current.

## Suggested upstream fix

Forward `--strict-deferrals` (and `--exit-with-count`) through the `lint` dispatcher and
`cmd_lint`. Reject any other `-`-prefixed argument with a usage error instead of treating it
as the file. A gate should assert the exit code of all four spellings above.

## Upstream status

Unknown. Not yet reported in the cyrius repo (no matching filing under its
`docs/development/issues/` as of cyrius `4f3731e8`).
