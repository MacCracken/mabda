# Issue: `cyim 1.1.4` regex-based commands fail mid-session with "invalid regex pattern"

> **RESOLVED — no longer reproduces (verified 2026-06-19).** `cyim` advanced
> `1.1.4 → 1.7.3`; the upstream repo (`/home/macro/Repos/cyim`) shows explicit
> regex-engine work (`fixing grepfiles, context=N, and base regex`; `adding regex
> engines`). The original repro — `cyim --grep <ident>` / `cyim --replace <a> <b>`
> on trivial ASCII patterns — now runs clean at 1.7.3. Nothing to file upstream;
> the mid-session state corruption is gone. Kept for history.

**Discovered:** 2026-04-28 (mabda v3 Step 3a, after several hours of use)
**Component:** `cyim` 1.1.4 (`/home/macro/.local/bin/cyim`)
**Severity:** Medium (stops `--grep` / `--replace` workflow; `--batch` and `--write` still work)
**Workaround in place:** Yes — pipe NUL-separated OLD/NEW pairs through `cyim --batch`

## Summary

`cyim --grep <pat> <file>` and `cyim --replace <old> <new> <file>`
both started returning `cyim --grep: invalid regex pattern` and
`cyim --replace: invalid regex pattern` respectively, even on
trivial alphanumeric patterns that contain no regex meta-characters.
The same commands worked fine earlier in the session.

`cyim --batch <file>` (NUL-separated OLD/NEW pairs from stdin) and
`cyim --write <file>` (full-file overwrite from stdin) continued to
work normally. So the regex-based command paths failed while the
non-regex paths stayed healthy.

## Reproduction

After the failure mode set in, even the simplest patterns failed:

```sh
$ cyim --grep main src/backend.cyr
cyim --grep: invalid regex pattern

$ cyim --grep BACKEND_KIND_WGPU tests/tcyr/mabda.tcyr
cyim --grep: invalid regex pattern

$ cyim --replace 'fn add' 'fn add' some_file.cyr
cyim --replace: invalid regex pattern
```

The patterns above are pure ASCII identifiers — no `*`, `+`, `?`,
`(`, `)`, `[`, `]`, `\`, `|`, `^`, `$`, `.`, `{`, or `}`. There is
nothing for any reasonable regex engine to find invalid in any of
them. So the error message is also misleading — the actual failure
is somewhere in the regex-engine setup or state, not in the user's
input.

`cyim --batch` (which takes NUL-separated literal OLD/NEW pairs and
does not appear to engage the regex engine) kept working through
the same session:

```sh
$ printf 'old\x00new\x00' | cyim --batch some_file.cyr
# (succeeds)
```

`cyim --write` likewise unaffected:

```sh
$ cyim --write some_file.cyr < /tmp/new_contents.cyr
# (succeeds)
```

## Environment

- Linux 6.18.24-1-lts, x86_64
- zsh
- `cyim 1.1.4` (single binary, no config files at `~/.cyim*` or
  `~/.config/cyim/`)
- No `CYIM_*` environment variables
- Worked normally for the first several hours of the session, then
  switched to permanent failure on regex commands

## Suggested upstream fixes (in priority order)

1. **Fix whatever state corruption causes regex compilation to
   start failing mid-session.** This is the actual bug — a
   long-running shell session shouldn't push `cyim` into a state
   where its regex engine refuses to compile any pattern.
2. **Improve the error message** — `invalid regex pattern` with
   nothing more is wrong UX when the input is "main". At minimum
   the command should print which pattern it considers invalid and
   why. Better: detect the case where the engine itself is
   broken (rather than the user's input) and report that
   distinctly.
3. **Make `--grep` and `--replace` accept literal-substring mode
   explicitly** — neither needs regex for the common case of
   "match this exact identifier or string." A `--literal` flag
   (or making literal the default and `--regex` opt-in) would
   sidestep the regex-engine fragility entirely for most uses.
   The `--batch` and `--replace-files` paths already are literal;
   `--replace` and `--grep` being the odd ones out is a UX wart.

## Filing trail

- Mabda v3 Step 3a session, 2026-04-28: discovered while
  refactoring `src/context.cyr` to grow GpuContext to 40 bytes.
  Fell back to `cyim --batch` (NUL-pairs from Python heredoc) and
  `cyim --write` (full-file overwrite). Every subsequent step in
  Sessions 3a/3b/3c/3d used those workarounds.

## Local handling

When the user is ready:

- File upstream against `cyim` with the reproduction above.
- `cyim` is not part of the `cyrius` toolchain proper — it's a
  separate developer tool. Filing path may differ from
  `docs/proposals/cyrius-5.6x-optimization-requests.md`. Worth
  asking the user whether `cyim` has its own upstream tracker.
