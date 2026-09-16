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

`src/main.cyr` has **no entry point of its own**. It defines
`mabda_main(fn_table_ptr, preinit_ptr)`, and the C launcher's `main()`
calls it after `_cyrius_init()` + `alloc_init()`. So the consumer is
compiled as an **object** and linked with the launcher and wgpu-native —
the same object-mode + `gcc` flow the mabda `Makefile` uses for its own
wgpu programs (`build/%.o` and `build/phase0`).

> ⚠ **`cyrius build` is not the build.** `cyrius build src/main.cyr <out>`
> makes a standalone ELF that never links `deps/wgpu_main.o` and never calls
> `mabda_main`, so it exits 0 and prints nothing. `cyrius.cyml` deliberately
> has no `[build]` section, so a bare `cyrius build` stops with its usage
> line. (Until mabda 4.1.3 it declared one, and a bare `cyrius build` wrote
> that do-nothing ELF over the linked `build/hello_gpu`.)
> `cyrius check src/main.cyr` is fine as a compile-only check.

This example ships **no `deps/` directory**. The wgpu launcher pieces
are copied in from the mabda repo root (`<mabda>` below). Run from this
directory:

```sh
cyrius deps                                  # resolves [deps.mabda] into lib/mabda.cyr

# 1. The C launcher + wgpu-native (one-time).
mkdir -p deps build
cp <mabda>/deps/wgpu_main.c <mabda>/deps/fetch-wgpu.sh deps/
sh deps/fetch-wgpu.sh                        # unpacks wgpu-native into deps/wgpu-native/
gcc -c deps/wgpu_main.c -Ideps/wgpu-native/include -o deps/wgpu_main.o

# 2. The consumer, as an object file.
printf 'object;\n' | cat - src/main.cyr | cycc > build/hello_gpu.o
objcopy -L memcpy -L memset -L memchr -L strlen -L strchr -L strstr \
        -L memeq -L atoi -L print_num -L println build/hello_gpu.o

# 3. Link launcher + consumer + wgpu-native.
gcc deps/wgpu_main.o build/hello_gpu.o deps/wgpu-native/lib/libwgpu_native.a \
    -lpthread -ldl -lm -o build/hello_gpu

./build/hello_gpu                            # needs a GPU with a Vulkan driver
```

Notes on the steps:

- **`fetch-wgpu.sh` unpacks next to itself**, into `deps/wgpu-native/`,
  from any working directory: `wgpu_main.c` includes
  `wgpu-native/include/webgpu/*.h` relative to itself. It fails if the
  headers or `libwgpu_native.a` are missing after the unpack. (Before
  mabda 4.1.3 it unpacked into the caller's directory and also built an
  unused `libwgpu_shim.so`, exiting non-zero unless `wgpu_shim.c` had been
  copied too; neither is true any more.)
- **`printf 'object;\n' | … | cycc`** is the object-mode compile. `cycc`
  is the Cyrius compiler that `cyrius` itself drives, on your `PATH`
  after a Cyrius install. `lib/mabda.cyr` and the stdlib files resolve
  relative to this directory, so run it from here.
- **`objcopy -L …`** makes the Cyrius stdlib's own `memcpy`, `strlen`,
  `strstr`, … local to the object. Skip it and the link still succeeds,
  but those GLOBAL definitions interpose glibc's for everything in the
  process. The wgpu-native and Vulkan-driver code then calls the Cyrius
  versions, which crashed Mesa's adapter enumeration when `strstr` was
  missed (CHANGELOG 2.4.2). The list is the Makefile's `LOCALIZE_FLAGS`.
- **mabda gates this recipe.** `make example-link` in the mabda repo runs
  steps 2 and 3 against the working tree's `dist/mabda.cyr` (it never
  runs the binary) as part of `make test-all` and CI, and fails if any
  step breaks. It also checks that the object defines every function
  `deps/wgpu_main.c` declares `extern`. Where `deps/wgpu-native/` is
  absent, as on CI runners, it still compiles and checks the object and
  reports only the link step as skipped.
- On success the program prints four `hello_gpu:` lines (`GPU context
  acquired`, `texture uploaded`, `render pipeline ready`, `done`) and
  exits 0. If the launcher's GPU pre-init fails, `mabda_main` prints
  `no GPU available` to stderr and exits 1.

`cyrius deps` reads `cyrius.cyml`, clones mabda at the tag pinned
there (`4.1.4`) into the dep cache `$HOME/.cyrius/deps/mabda/4.1.4/`,
and copies its `dist/mabda.cyr` bundle into `lib/mabda.cyr` (a real
file, not a symlink). The consumer then includes it with
`include "lib/mabda.cyr"`.

### Optional feature opt-ins

This minimal example uses neither. `cyrius deps` already pulls chitra
and samvada in transitively through mabda's own `cyrius.cyml`, and
their `thread` + `sankoch` stdlib leaves through `dist/mabda.deps`, so
a feature-flag build needs no extra deps. Declaring the matching dep
yourself (both commented out in `cyrius.cyml`) pins its version
explicitly:

- **`-D MABDA_PNG`** / **`-D MABDA_JPEG`** (PNG / baseline-JPEG decode) →
  `[deps.chitra]` (tag `1.0.3`), plus `thread` + `sankoch` in `[deps].stdlib`
  (one chitra dep serves both formats).
- **`-D MABDA_LOGIND`** (logind master delegation) →
  `[deps.samvada]` (tag `1.0.1`). Either way, call `samvada_native_init()`
  before configuring the surface (samvada 1.0 is a pure-Cyrius dbus client:
  no C shim, no libsystemd). mabda never initializes samvada itself.

## What this proves

This example is the **`@public`-API-stability regression test across
the wgpu → native transition**. The pure-Cyrius native backends have
now landed (AMD in v3.0, NVIDIA in v4.0), and `src/main.cyr` still
compiles against the v4.1.4 mabda tag. None of its code changes has
been a mabda API change:

- **v4.1.1** moved to the cyrius 6.6.0 `Result` pair-bind
  (`var res_tag, res = gpu_context_from_preinit(preinit_ptr);`), a
  toolchain change.
- **v4.1.3** added `include "lib/thread_local.cyr"`. The launcher has
  called `thread_local_use_foreign_tls()` since mabda 4.0.2, so without
  that include this example stopped linking from 4.0.2 on. Nobody saw it
  because the old recipe here never linked the launcher. It was a
  launcher-contract change, not a public-API one.
- **v4.1.3** also assembled the 269-byte WGSL source in pieces so no
  line exceeds the linter's 120-byte limit. The shader bytes are
  unchanged.

If it ever needs any other code change, the public API contract was
broken.

## What the launcher does

The C launcher in `deps/wgpu_main.c` (copied from mabda's reference)
is ~460 lines and does exactly five things:

1. Call `_cyrius_init()` then `alloc_init()` to bring up Cyrius
   globals
2. Call `thread_local_use_foreign_tls()` so Cyrius thread-locals do
   not clobber glibc's `%fs` (cyrius 6.3.26). It is defined in
   `lib/thread_local.cyr`, which is why `src/main.cyr` includes it
3. Pre-initialize the GPU (`instance → adapter → device → queue`)
4. Build the 67-slot function-pointer table with wgpu-native exports
   and mabda's struct-packing shims
5. Call `mabda_main(fn_table_ptr, preinit_ptr)`

None of this leaks into consumer code — it exists only on the wgpu
path. The native AMD (`gpu_context_new_native`) and native NVIDIA
(`gpu_context_new_native_nvidia`) backends need no C launcher at all.
