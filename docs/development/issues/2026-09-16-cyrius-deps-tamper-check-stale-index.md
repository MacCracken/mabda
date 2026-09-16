# `cyrius deps` refuses an untouched dep cache as "tampered" after a metadata-only change

**Status:** open upstream. Filed in the cyrius repo as `cyrius/docs/development/issues/2026-09-16-mabda-deps-tamper-check-stale-index.md`.
**Discovered:** 2026-09-16, mabda 4.1.3 hardware-verification sweep (example-consumer build)
**Toolchain:** `cyrius 6.6.4`. The code is unchanged at cyrius `HEAD` (`4f3731e8`, `6.6.4-1`).
**Component:** cli (`cyrius deps`, `cbt/deps.cyr`)
**Severity:** Low. Nothing is corrupted and nothing insecure is accepted. The cost is a hard
resolve failure with a misleading "tampered" message. It hits every project on the machine
that resolves the same dep + tag.

## Summary

For a tagged git dep, `cyrius deps` checks the cached checkout in
`$CYRIUS_HOME/deps/<name>/<tag>/` against HEAD (the CVE-21 pin, cyrius v6.2.30). One of its
three checks is `_git_worktree_clean` (`cbt/deps.cyr:2828`, called at `:2017`). It runs:

```
git -C <clone_dir> diff-index --quiet HEAD
```

`diff-index` compares the index with the tree and **trusts the index's cached stat data**
for the working tree. It never refreshes that cache. If a tracked file's stat data no longer
matches the index (mtime, ctime, inode, uid, size), the file is reported as modified even
though its bytes are identical. `_git_worktree_clean` returns 0 and the resolve fails with:

```
error: cached checkout for dep '<name>' has local modifications (working tree != HEAD <sha>)
— refusing tampered cache. Remove the cached clone and re-resolve.
```

Ordinary operations change only metadata:

- `touch` on a cached file
- `cp -a` / `cp -p` / `rsync -a` of the cache (new inodes and ctimes, same content)
- running git in the cache under a **uid-mapped user namespace** (`unshare -rn`). There the
  files appear to be owned by uid 0, so a `git status` rewrites the index with `st_uid = 0`.
  Back at the real uid, every entry mismatches.

The cache is shared by every project on the machine, so one such change breaks every
concurrent `cyrius deps` / `cyrius build` / `cyrius test` that resolves that dep. It stays
broken until someone refreshes the index or deletes the clone. Deleting the clone, which is
what the message advises, forces a network re-clone, so an offline machine cannot recover
that way.

It also looks intermittent. Any **porcelain** git command run in the cache (`git status`,
`git diff`) refreshes the stat cache and writes the index back, which silently "fixes" it.
The first attempt at the repro below hid the bug this way, because it ran a `git diff` to
show the content was unchanged.

## Reproduction

Isolated. `CYRIUS_HOME` points at a scratch directory, so the real `~/.cyrius/deps` cache is
never written, and the dep is a local `file://` repo, so no network is used. Verified
2026-09-16. Run it with `bash`:

```sh
R=$(mktemp -d); CY=$HOME/.cyrius/versions/6.6.4/bin/cyrius
export CYRIUS_HOME=$R/home GIT_ALLOW_PROTOCOL=file
mkdir -p $R/home $R/origin/dist $R/consumer/src
ln -s $HOME/.cyrius/versions/6.6.4/bin $R/home/bin
ln -s $HOME/.cyrius/versions/6.6.4/lib $R/home/lib
printf 'fn foo_answer(): i64 { return 42; }\n' > $R/origin/dist/foo.cyr
git -C $R/origin init -q
git -C $R/origin add -A
git -C $R/origin -c user.name=r -c user.email=r@example.invalid commit -qm v1
git -C $R/origin tag 1.0.0
cat > $R/consumer/cyrius.cyml <<CYML
[package]
name = "wtc-repro"
version = "0.1.0"
language = "cyrius"
cyrius = "6.6.4"

[build]
entry = "src/main.cyr"
output = "build/main"

[deps.foo]
git = "file://$R/origin"
tag = "1.0.0"
modules = ["dist/foo.cyr"]
CYML
printf 'include "lib/foo.cyr"\nfn main(): i64 { return foo_answer(); }\n' > $R/consumer/src/main.cyr
cd $R/consumer
C=$CYRIUS_HOME/deps/foo/1.0.0

$CY deps; echo "exit=$?"                        # 1 deps resolved, exit=0 (fresh clone)
touch -d '2001-01-01 00:00:00' $C/dist/foo.cyr  # metadata only, bytes unchanged
git -C $C diff-index --quiet HEAD; echo "$?"    # 1: what _git_worktree_clean sees
$CY deps; echo "exit=$?"                        # error: ... refusing tampered cache. exit=1
git -C $C update-index -q --refresh
$CY deps; echo "exit=$?"                        # 1 deps resolved, exit=0 (same bytes accepted)
```

A fuller run of the same setup added a `cp -a` case and a real-content-edit control. Its
logs are session scratch from the 4.1.3 verification and are not in the repo:

| Step | `diff-index` | `cyrius deps` |
| --- | --- | --- |
| fresh clone | 0 | exit 0 |
| `touch` (content identical, sha256 checked) | 1 | **exit 1, "refusing tampered cache"** |
| `git update-index -q --refresh` | 0 | exit 0 |
| `cp -a` of the checkout (content identical) | 1 | **exit 1, "refusing tampered cache"** |
| real content edit (control) | 1 | exit 1, correctly refused |

The uid-mapped-namespace case was hit for real in the same sweep. After git ran against the
shared cache inside `unshare -rn`, a normal resolve refused the untouched samvada `1.0.1`
and chitra `1.0.3` checkouts as tampered. A second scratch-clone run isolated the mechanism:
`diff-index` exits 0 on a fresh clone, exits 1 after an in-namespace `git status`, and exits
0 again after a normal-uid `git status`.

## Expected vs actual

- **Expected:** a cached checkout whose tracked file **contents** match HEAD passes the
  tamper check. Only a content difference is refused.
- **Actual:** any stat-cache mismatch is refused as tampering, with advice (delete and
  re-clone) that fails offline.

## Workaround

Refresh the stat cache in the affected checkout. It re-hashes only the files whose stat
changed, and it clears their dirty state only if the content matches the index blob, so it
cannot launder a real edit:

```sh
git -C ~/.cyrius/deps/<name>/<tag> update-index -q --refresh
```

In mabda: don't run dep resolution under `unshare -rn` (or any uid-mapped namespace)
against the shared `~/.cyrius`. If network isolation is needed, use
`GIT_ALLOW_PROTOCOL=file` with an already-populated cache.

## Suggested upstream fix

Refresh before comparing. Either run `git update-index -q --refresh` ahead of
`diff-index --quiet HEAD`, or use `git diff --quiet HEAD`, which refreshes in memory
and compares content. Both were checked (git only, scratch clone, 2026-09-16). Each
variant ran on its own copy of the mutated clone:

| Case | `diff-index` today | refresh + `diff-index` | `diff --quiet HEAD` |
| --- | --- | --- | --- |
| `cp -a`, content same | 1 | 0 | 0 |
| `touch`, content same | 1 | 0 | 0 |
| content edit, same size | 1 | 1 | 1 |
| content edit, same size and mtime | 1 | 1 | 1 |

Whatever the fix, a gate should cover the `touch` and `cp -a` rows. Today nothing
distinguishes "stat changed" from "content changed".

## Upstream status

Filed 2026-09-16 as `cyrius/docs/development/issues/2026-09-16-mabda-deps-tamper-check-stale-index.md`.
