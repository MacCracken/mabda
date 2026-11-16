# Tidy-up: the cyrius 6.7.5 pin is behind its consumers, and `[deps.chitra]`/`[deps.samvada]` reach every consumer though both features are opt-in

**Status:** 🟡 **OPEN** — tidy-up (not blocking).
**Filed:** 2026-11-15, from soorat's Cyrius-port probe P5 (`soorat/scripts/probes/bundle/p5_fold/`, `p5_dep/`;
`bundle/run.sh p5`; `soorat/docs/development/port-parity.md` §1.4; ledger row L12).
**Placement:** `cyrius.cyml` — `[package] cyrius = "6.7.5"`, `[deps.samvada]` (line ~102), `[deps.chitra]`
(line ~108).
**Severity:** **Low.**
**Affects:** mabda 4.2.0 consumers on cyrius 6.7.18 (all of soorat's other dependencies pin 6.7.18).

## Summary

1. **The pin.** P5 compiled a minimal consumer, the README's `mabda_main` (reachable), both from the stdlib
   fold and from `[deps.mabda]` at tag 4.2.0, under `cyrius test --bounds` on 6.7.18. Both gave **0 warnings
   and 0 errors**. The fold and the tag's `dist/mabda.cyr` are byte-identical: sha256 `5cd9f695…`,
   1,370,812 B. The README's "≈ 1.2 MB" is behind too. Moving the pin to 6.7.18 would only record what already
   works.
2. **Production-only dependencies.** The README describes PNG/JPEG decoding (`-D MABDA_PNG` / `MABDA_JPEG`) and
   logind (`-D MABDA_LOGIND`) as opt-in. But `[deps.chitra]` (1.0.3) and `[deps.samvada]` (1.0.1) are
   unconditional, so `cyrius deps` resolves and vendors them for every consumer, and they are prepended to its
   compile:

   | Route | Binary | Preprocessed |
   |---|---|---|
   | stdlib fold | 722,704 B | ≈ 1.97 MB |
   | as a dependency | 1,515,280 B | ≈ 3.45 MB |

   soorat also uses chitra itself (1.4.1). Its root pin wins, but only because of the resolution order.

## Suggested fix

- Pin cyrius 6.7.18, and correct the README's size.
- Mark `[deps.chitra]` and `[deps.samvada]` `optional`, switched on by the features that `#ifdef` them, so a
  consumer that never defines `MABDA_PNG`, `MABDA_JPEG` or `MABDA_LOGIND` doesn't resolve them.

## Measure / close

`sh run.sh p5` in `soorat/scripts/probes/bundle/`. The dependency route no longer lists chitra or samvada in
`cyrius.lock`, and its binary and preprocessed size come down toward the fold's.
