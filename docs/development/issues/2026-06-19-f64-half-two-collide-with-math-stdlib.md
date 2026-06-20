# Issue: `color.cyr` `F64_HALF` / `F64_TWO` collide with the Cyrius `math` stdlib (silent NaN on consumers' non-GPU path)

**Discovered:** 2026-06-19 (wiring mabda 3.3.0's native-AMD SPIR-V f64 compute path into **attn11** — first consumer that also depends on the `math` stdlib)
**Component:** `src/color.cyr` — globals `F64_HALF` (L22) / `F64_TWO` (L23), runtime-initialised in its init fn (L44–45); amalgamated into `dist/mabda.cyr` (L222–223, 244–245).
**Severity:** High for any consumer that also uses the Cyrius `math` stdlib. Unlike the benign `duplicate fn 'color_rgb' (last definition wins)` warning, here the two definitions **conflict in value**, so "last wins" silently corrupts the consumer's f64 math.
**Workaround in place:** Yes (consumer-side, attn11) — keep `lib/mabda.cyr` out of the main translation unit; the GPU path stays a separate build until this is fixed (or until cyrius `dep-module-call` namespaces dep symbols). The clean fix belongs here.

## Summary

`dist/mabda.cyr` exports global symbols **`F64_HALF`** and **`F64_TWO`** (from `src/color.cyr`) that collide with the Cyrius **`math` stdlib** constants of the same name. A consumer that includes both `lib/math.cyr` and `lib/mabda.cyr` gets:

```
duplicate symbol 'F64_HALF' redefined with conflicting value (last definition wins)
duplicate symbol 'F64_TWO'  redefined with conflicting value (last definition wins)
```

The two definitions disagree on **initialisation**, which is what makes "last wins" dangerous rather than cosmetic:

| symbol | `lib/math.cyr` (stdlib) | `src/color.cyr` → `dist/mabda.cyr` |
|---|---|---|
| `F64_HALF` | `var F64_HALF = 0x3FE0000000000000;` (compile-time `0.5`) | `var F64_HALF = 0;` → set at runtime to `f64_div(f64_from(1), f64_from(2))` |
| `F64_TWO`  | `var F64_TWO  = 0x4000000000000000;` (compile-time `2.0`) | `var F64_TWO  = 0;` → set at runtime to `f64_from(2)` |

When the `color.cyr` definition wins (included last), `F64_HALF`/`F64_TWO` are **statically `0`** and only become correct *after `color.cyr`'s init fn runs*. A consumer that uses the `math` stdlib but does **not** execute mabda's init path computes with `F64_HALF == F64_TWO == 0`.

## Impact

`ganita`'s `tanh` / `sigmoid` / `atanh` read `F64_TWO` / `F64_HALF` directly (e.g. `f64_div(f64_sub(ex,enx), F64_TWO)`). With those globals at `0`, the consumer hits division by zero → **NaN/inf** on every non-GPU run. Concretely this blocks **attn11** (a transformer using `math`/`ganita` for GELU/tanh) from `include`-ing `lib/mabda.cyr` in its main binary at all — it must keep mabda in a separate translation unit, which defeats a single-binary `--gpu` flag.

This is the same class as the documented benign `duplicate fn 'color_rgb'` warning (also `color.cyr`), but value-conflicting and therefore silently incorrect rather than cosmetic.

## Reproduction

Any program built against a manifest that lists the `math` stdlib **and** includes `lib/mabda.cyr`:

```cyrius
include "lib/math.cyr"     # stdlib: F64_HALF = 0x3FE0…, F64_TWO = 0x4000…
include "lib/mabda.cyr"    # color.cyr: F64_HALF = 0, F64_TWO = 0 (runtime-init) — wins (last)
# now ganita_f64_tanh(x) etc. divide by F64_TWO == 0  ->  NaN
```

cycc 6.2.27 / 6.2.28 emits the two `conflicting value (last definition wins)` warnings at build.

## Proposed fix (any one; #1 preferred)

1. **Namespace the `color.cyr` constants** — rename to `MABDA_F64_HALF` / `MABDA_F64_TWO` (or `_mabda_f64_half`). mabda is *consumed, never modified* by downstreams, so its internal constants shouldn't shadow well-known stdlib names. Cleanest — no behavioural coupling. *(Same remedy would retire the sibling `color_rgb` duplicate: namespace it `mabda_color_rgb`.)*
2. **Reuse the `math` stdlib** — have `color.cyr` depend on `math` and use its `F64_HALF`/`F64_TWO` instead of redefining them.
3. **Match the stdlib values as compile-time constants** — `var F64_HALF = 0x3FE0000000000000;` / `var F64_TWO = 0x4000000000000000;` so "last wins" is at least value-safe (the namespace warning would remain).

## Notes

Found purely on the host side — mabda's native-AMD SPIR-V f64 compute path itself works great on Cezanne (gfx90c), bit-exact end-to-end (fma, layernorm, full op suite). This is only the amalgam symbol collision.
