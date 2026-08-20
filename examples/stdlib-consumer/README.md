# mabda-stdlib-consumer

Minimal "hello GPU" example for mabda. Proves the stdlib-inclusion
contract:

- **The consumer writes no FFI code.** `src/main.cyr` only uses
  mabda's `@public` API — `gpu_context_from_preinit`,
  `texture_from_rgba`, `render_pipeline_create_simple`, etc.
- **The C launcher lives at the edge of the consumer**, not inside
  mabda, and only on the wgpu path. `deps/wgpu_main.c` is copied from
  the mabda reference launcher. The native AMD and native NVIDIA
  backends need no C launcher at all.
- **The public API is stable across the backend transition.** mabda
  now ships three backends behind one API — wgpu-native (the default
  this example builds), native AMD (shipped v3.0), and native NVIDIA
  (shipped v4.0). AMD-on-wgpu is deprecated at v4.0.1 (warn+allow;
  `-D MABDA_AMD_WGPU_STRICT` hard-rejects), but the source in
  `src/main.cyr` compiles unchanged against every one of them.

## Build

This example ships **no `deps/` directory**. The wgpu launcher pieces
are copied in from the mabda repo root (`<mabda>` below):

```sh
cyrius deps                                # resolves [deps.mabda] from cyrius.cyml
mkdir -p deps
cp <mabda>/deps/wgpu_main.c deps/          # the reference C launcher
cp <mabda>/deps/fetch-wgpu.sh deps/        # the wgpu-native fetcher
sh deps/fetch-wgpu.sh                       # one-time — pulls wgpu-native binaries
# build the launcher object with the repo-root Makefile's
# `deps/wgpu_main.o` target:  gcc -c deps/wgpu_main.c -Ideps/wgpu-native/include
cyrius build                                # compile the consumer
./build/hello_gpu                           # run
```

`cyrius deps` reads `cyrius.cyml`, clones mabda at the tag pinned
there (`4.1.0`), and creates `lib/mabda.cyr` as a symlink into
`$HOME/.cyrius/cache/mabda/dist/mabda.cyr`. The consumer then
includes it with `include "lib/mabda.cyr"`.

### Optional feature opt-ins

This minimal example uses neither, but if you build mabda with a
feature flag you must add the matching dep (both commented out in
`cyrius.cyml`):

- **`-D MABDA_PNG`** / **`-D MABDA_JPEG`** (PNG / baseline-JPEG decode) → add
  `[deps.chitra]` (tag `0.3.1`) plus `thread` + `sankoch` to `[deps].stdlib`
  (one chitra dep serves both formats).
- **`-D MABDA_LOGIND`** (logind master delegation) → add
  `[deps.samvada]` (tag `0.4.1`) and link `samvada/deps/samvada_main.c`
  alongside the launcher.

## What this proves

This example is the **`@public`-API-stability regression test across
the wgpu → native transition**. The pure-Cyrius native backends have
now landed (AMD in v3.0, NVIDIA in v4.0), and `src/main.cyr` still
compiles without edits against the v4.1.0 mabda tag. If it ever
doesn't, the public API contract was broken.

## What the launcher does

The C launcher in `deps/wgpu_main.c` (copied from mabda's reference)
is ~480 lines and does exactly four things:

1. Call `_cyrius_init()` then `alloc_init()` to bring up Cyrius
   globals
2. Pre-initialize the GPU (`instance → adapter → device → queue`)
3. Build the 67-slot function-pointer table with wgpu-native exports
   and mabda's struct-packing shims
4. Call `mabda_main(fn_table_ptr, preinit_ptr)`

None of this leaks into consumer code — it exists only on the wgpu
path. The native AMD (`gpu_context_new_native`) and native NVIDIA
(`gpu_context_new_native_nvidia`) backends need no C launcher at all.
