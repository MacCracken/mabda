# mabda-stdlib-consumer

Minimal "hello GPU" example for mabda. Proves the stdlib-inclusion
contract:

- **The consumer writes no FFI code.** `src/main.cyr` only uses
  mabda's `@public` API — `gpu_context_from_preinit`,
  `texture_from_rgba`, `render_pipeline_create_simple`, etc.
- **The C launcher lives at the edge of the consumer**, not inside
  mabda. `deps/wgpu_main.c` is copied from the mabda reference
  launcher.
- **Everything goes away in v3.0.** When the pure-Cyrius GPU backend
  lands, the `[deps.mabda]` tag bumps and the launcher + C build
  disappear. The source in `src/main.cyr` compiles unchanged.

## Build

```sh
cyrius deps                                # resolves [deps.mabda] from cyrius.cyml
sh ../../deps/fetch-wgpu.sh                # one-time — pulls wgpu-native binaries
make -C deps                               # build the launcher
cyrius build                               # compile the consumer
./build/hello_gpu                          # run
```

`cyrius deps` reads `cyrius.cyml`, clones mabda at the tag pinned
there (`2.5.0`), and creates `lib/mabda.cyr` as a symlink into
`$HOME/.cyrius/cache/mabda/dist/mabda.cyr`. The consumer then
includes it with `include "lib/mabda.cyr"`.

## What this proves

This example is the **regression test for the v3.0 backend swap**.
When the native Cyrius GPU backend lands, the `src/main.cyr` source
must still compile without edits against the new mabda tag. If it
doesn't, the public API contract was broken.

## What the launcher does

The C launcher in `deps/wgpu_main.c` (copied from mabda's reference)
is ~200 lines and does exactly four things:

1. Call `_cyrius_init()` then `alloc_init()` to bring up Cyrius
   globals
2. Pre-initialize the GPU (`instance → adapter → device → queue`)
3. Build the 58-slot function-pointer table with wgpu-native exports
   and mabda's struct-packing shims
4. Call `mabda_main(fn_table_ptr, preinit_ptr)`

None of this leaks into consumer code. When v3.0 replaces wgpu-native
with a pure Cyrius backend, the launcher goes away.
