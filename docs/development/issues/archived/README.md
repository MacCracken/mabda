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
| [`2026-06-19-f64-half-two-collide-with-math-stdlib.md`](./2026-06-19-f64-half-two-collide-with-math-stdlib.md) | `color.cyr`'s runtime-init `F64_HALF`/`F64_TWO` shadowed the Cyrius `math` stdlib's compile-time constants of the same name; "last definition wins" silently zeroed a consumer's f64 math (NaN on the non-GPU path). Surfaced by **attn11**, the first consumer to dep both `math` and `lib/mabda.cyr`. | **mabda 3.4.1** — fix #1: renamed to `MABDA_F64_HALF`/`MABDA_F64_TWO` (defs + init + all usages); the dist now exports zero standalone `F64_HALF`/`F64_TWO`, so the collision is gone. |
| [`2026-08-19-native-compute-spike-stale.md`](./2026-08-19-native-compute-spike-stale.md) | `native_compute_spike` had two defects since its first commit, and its completion marker could not see a hang. | **mabda 4.1.3** — repaired and out of `NATIVE_KNOWN_FAIL`. A clean-boot confirmation run stays on the roadmap ("Clean-boot native HW closeout"). Archived 4.2.0. |
| [`2026-08-19-native-saxpy-e2e-mir-id-oor.md`](./2026-08-19-native-saxpy-e2e-mir-id-oor.md) | `native_spirv_saxpy_e2e` red since 4.0.5: `gfx9_compile` returned `MIR_ERR_ID_OOR` (a scratch-buffer overrun). | **mabda 4.1.0** — fixed, HW-verified, runtime self-check plus a static CI gate. Archived 4.2.0. |
| [`2026-08-19-native-spirv-compile-limits.md`](./2026-08-19-native-spirv-compile-limits.md) | Native SPIR-V→GFX9 rejected several constructs with a silent `return 0`, well under its own id cap. | **mabda 4.1.0** — every item fixed and HW-verified; the remaining limits have named error codes. Archived 4.2.0. |
| [`2026-08-19-wgpu-native-29011-breaking.md`](./2026-08-19-wgpu-native-29011-breaking.md) | wgpu-native `v29.0.1.1` is a breaking change despite the patch-level version. | **mabda 4.1.0** — bumped, with both breakages handled. Archived 4.2.0. |
| [`2026-09-16-cycc-nested-call-stack-alignment.md`](./2026-09-16-cycc-nested-call-stack-alignment.md) | A call nested in an expression ran with rsp 8 bytes off 16-byte alignment (C interop only). | **cyrius 6.6.5** — re-checked on 6.7.5; mabda 4.2.0 deleted `scripts/check-ffi-call-alignment.py`. |
| [`2026-09-16-cyrius-deps-tamper-check-stale-index.md`](./2026-09-16-cyrius-deps-tamper-check-stale-index.md) | `cyrius deps` refused an untouched dep cache as "tampered" after a metadata-only change. | **cyrius 6.6.5** — re-checked on 6.7.5 with the filed reproduction. Archived 4.2.0. |
| [`2026-09-16-cyrius-lint-drops-strict-deferrals.md`](./2026-09-16-cyrius-lint-drops-strict-deferrals.md) | `cyrius lint` could not forward `--strict-deferrals` (usage error, or the flag silently dropped). | **cyrius 6.6.5** — mabda 4.2.0's CI and `make lint-deferrals` call the wrapper. |
| [`2026-09-16-stdlib-bench-min-minus-mean-floor.md`](./2026-09-16-stdlib-bench-min-minus-mean-floor.md) | `lib/bench.cyr` subtracted a mean timer floor from every window and reported the minimum. | **cyrius 6.6.5** — archived 4.2.0; mabda's own windowing workaround in `programs/benchmarks.cyr` is on the roadmap backlog. |
| [`2026-09-16-test-stack-array-overruns.md`](./2026-09-16-test-stack-array-overruns.md) | Undersized stack arrays in CPU tests and a HW program (the old texture.tcyr "leading NUL"). | **mabda 4.1.3** — resized, plus the `check-stack-array-sizing` static gate. Archived 4.2.0. |
