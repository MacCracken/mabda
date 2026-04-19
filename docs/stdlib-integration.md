# Stdlib Integration Guide

> **Heads-up:** this guide documents a **transitional** integration
> path. The C launcher + wgpu-native requirement is scaffolding that
> exists only because the native Cyrius GPU backend isn't ready yet.
> When v3.0 lands, every section marked **[transitional]** below
> disappears. Your `src/main.cyr` does not change.

Mabda is a Cyrius stdlib-trusted dep starting with v2.3.0. This guide
walks a new consumer project through the one-time setup to compile
against it.

## 1. Declare the dep

In your `cyrius.cyml`:

```cyml
[package]
name = "my-app"
version = "0.1.0"
cyrius = "5.4.7"

[build]
entry = "src/main.cyr"
output = "build/my-app"

[deps]
stdlib = ["alloc", "string", "fmt", "syscalls", "tagged", "fnptr"]

[deps.mabda]
git = "https://github.com/MacCracken/mabda.git"
tag = "2.3.0"
modules = ["dist/mabda.cyr"]
```

Then `cyrius deps` pulls the bundle and creates `lib/mabda.cyr` as a
symlink into `$HOME/.cyrius/cache/mabda/dist/mabda.cyr`.

## 2. Write your consumer code

Use only the `@public` API. Every non-FFI file in mabda's `src/` has
a `# @public` or `# @internal` marker on line 1:

- **`# @public`** — stable API, survives the v3.0 backend swap. Safe.
- **`# @internal`** — FFI scaffolding (`wgpu_types`,
  `wgpu_descriptors`, `wgpu_ffi`, `cache_key`). Will be replaced.
  Do not reference these from consumer code.

Consumer entry points look like this:

```cyrius
include "lib/mabda.cyr"

fn mabda_main(fn_table_ptr, preinit_ptr) {
    color_init();
    wgpu_ffi_init_table(fn_table_ptr);       # [transitional]
    var res = gpu_context_from_preinit(preinit_ptr);
    # ... use gpu_ctx_device, texture_from_rgba, compute_dispatch, etc.
    return 0;
}
```

The `mabda_main` name is required — the C launcher calls it by symbol.
`_cyrius_init()` and `alloc_init()` run inside the launcher before
`mabda_main` fires.

## 3. The C launcher [transitional]

Mabda ships a reference launcher at `deps/wgpu_main.c` (copy it into
your project's `deps/` directory). It does exactly four things:

1. **Bring up Cyrius globals.** `_cyrius_init()` then `alloc_init()`.
   Order matters — init resets globals, alloc must come after.
2. **Pre-initialize the GPU.** `wgpuCreateInstance` →
   `wgpuInstanceRequestAdapter` → `wgpuAdapterRequestDevice` →
   `wgpuDeviceGetQueue`. Packages the four handles into a
   `WgpuPreinit` struct.
3. **Build the 58-slot function table.** Populates function pointers
   for every wgpu entry mabda uses, including struct-packing shims
   that route around Cyrius's `fncall6`-plus-wgpu crash bug:
   - `wgpu_shim_copy_buffer_to_buffer` — 6-arg wgpu call packed into
     `WgpuCopyArgs`
   - `wgpu_shim_buffer_map` — 6-arg wgpu call packed into `WgpuMapArgs`
   - `wgpu_shim_queue_write_texture` — 6-arg wgpu call packed into
     `WgpuWriteTextureArgs`
   - `wgpu_shim_resolve_query_set` — 6-arg wgpu call packed into
     `WgpuResolveArgs`
   - `wgpu_shim_queue_submit_one` — 1-command submit convenience
   - `wgpu_shim_create_command_encoder` / `..._finish` — label-taking
     wrappers (wgpu v29 is sensitive to descriptor padding)
   - `wgpu_shim_get_timestamp_period_bits` — f32→i64 bit reinterpret
4. **Call `mabda_main(fn_table_ptr, preinit_ptr)`.**

## 4. Build it

```sh
# First time only:
cyrius deps                                   # resolve [deps.mabda]
sh deps/fetch-wgpu.sh                         # download wgpu-native binaries

# Every build:
make -C deps                                  # compile wgpu_main.c [transitional]
cyrius build                                  # compile your .cyr source
./build/my-app                                # run
```

The Makefile rule for the launcher is roughly:

```make
deps/wgpu_main.o: deps/wgpu_main.c
	gcc -c -Ideps/wgpu-native/include deps/wgpu_main.c -o $@

build/my-app.o: src/main.cyr
	printf 'object;\n' | cat - src/main.cyr | cc5 > $@

build/my-app: build/my-app.o deps/wgpu_main.o
	gcc deps/wgpu_main.o build/my-app.o \
	    deps/wgpu-native/lib/libwgpu_native.a \
	    -lpthread -ldl -lm -o $@
```

See `examples/stdlib-consumer/` in the mabda repo for a complete
runnable project.

## 5. What disappears in v3.0

When the native Cyrius GPU backend lands:

- `deps/wgpu_main.c` — gone
- `deps/wgpu-native/` — gone
- The `wgpu_ffi_init_table(fn_table_ptr)` line — gone
- The `make -C deps` step — gone
- Your dependency on libC — gone; the consumer becomes pure Cyrius

**What stays:**

- Your `src/main.cyr` source, unchanged
- The `gpu_context_from_preinit`, `texture_from_rgba`,
  `compute_dispatch`, `render_pipeline_create_simple` API —
  identical signatures
- The `mabda_main` entry point, now called by a pure-Cyrius
  bootstrap instead of a C launcher

The `examples/stdlib-consumer/` project is the regression test: if
it still compiles against the v3.0 mabda tag, the contract held.

## Known transitional warnings

When compiling the bundled `dist/mabda.cyr`, cc5 emits
`undefined function` warnings for the 58 wgpu function-table slots.
These are **expected and benign** — the slots are globals populated
by the C launcher at runtime. In v3.0 these warnings disappear
because the functions become real Cyrius definitions.

If you see any warning that is **not** a wgpu function-table slot,
that's a bug; please file it at
[github.com/MacCracken/mabda/issues](https://github.com/MacCracken/mabda/issues).

## Debugging tips

- If `mabda_main` is never called, the launcher's GPU pre-init likely
  failed (no wgpu adapter). Run the launcher with
  `WGPU_BACKENDS=vulkan` or `WGPU_BACKENDS=gl` to force a specific
  backend.
- If you get a `Shader not provided` panic from wgpu-native, check
  that `WGPU_STYPE_SHADER_SOURCE_WGSL` is `0x02` in your FFI
  constants — mabda's v2.0 tree shipped with the wrong value and was
  silently compiled. Fixed in v2.1.0.
- If `wgpuDeviceCreateQuerySet` panics, you forgot to opt the device
  into `TIMESTAMP_QUERY` when requesting it. Mabda's
  `gpu_timestamps_supported(device)` checks `wgpuDeviceHasFeature`,
  not `wgpuAdapterHasFeature` — the distinction matters.
- If a surface configured via `surface_state_new` misbehaves on
  < 2.3.0, you're hitting the HIGH-1 shadow bug fixed in 2.3.0.
  Bump your mabda tag to 2.3.0 or later. See
  [`docs/audit/2026-04-19-audit.md`](audit/2026-04-19-audit.md).

## Security

Mabda 2.3.0 shipped the last audit-gated stdlib-candidate pass.
Findings + remediation in
[`docs/audit/2026-04-19-audit.md`](audit/2026-04-19-audit.md). Every
HIGH / MED finding landed with a regression assertion in
`tests/tcyr/mabda.tcyr`. See `SECURITY.md` for the policy.
