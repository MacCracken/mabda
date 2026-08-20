# Stdlib Integration Guide

> **Heads-up:** this guide covers the **wgpu** integration path (C launcher
> + wgpu-native), which is the cross-vendor default and remains load-bearing
> across the entire v3.x line. v3.0 (2026-06-02) added a pure-Cyrius **native
> AMD** backend *alongside* wgpu — not replacing it — so the C launcher does
> NOT disappear at v3.0; per the roadmap the wgpu+C path deprecates then
> retires per-vendor: the **AMD route is deprecated at v4.0.1** (warns but
> still works; `-D MABDA_AMD_WGPU_STRICT` enforces), NVIDIA at v5.0, and the
> launcher/binding itself only at v5.1 — each once that
> vendor's native backend is in production. Either way your `src/main.cyr`
> does not change —
> the `@public` API is identical across backends.

Mabda is a Cyrius stdlib-trusted dep starting with v2.3.0. This guide
walks a new consumer project through the one-time setup to compile
against it.

## 1. Declare the dep

In your `cyrius.cyml`:

```cyml
[package]
name = "my-app"
version = "0.1.0"
cyrius = "6.5.29"

[build]
entry = "src/main.cyr"
output = "build/my-app"

[deps]
stdlib = [
    "string", "fmt", "alloc", "vec", "str", "io", "args",
    "hashmap", "syscalls", "tagged", "fnptr", "mmap", "dynlib", "sakshi",
]

[deps.mabda]
git = "https://github.com/MacCracken/mabda.git"
tag = "4.1.0"
modules = ["dist/mabda.cyr"]
```

`mmap`, `dynlib`, and `sakshi` must be present before `mabda` in the
resolved `lib/` tree — mabda's include chain (`src/lib.cyr`) pulls them
in that order, so declaring the full stdlib set above is required, not
optional. `examples/stdlib-consumer/cyrius.cyml` carries the canonical
list.

### Opt-in feature deps

Two mabda features pull additional deps. Declare them **only** if you
build with the matching `-D` flag — otherwise they are compiled out and
you don't need the dep:

- **PNG / baseline-JPEG asset loading** (`gpu_texture_load_png` /
  `gpu_texture_load_jpeg` in `src/asset_load.cyr`, gated on `#ifdef MABDA_PNG` /
  `#ifdef MABDA_JPEG`) — build with `-D MABDA_PNG` and/or `-D MABDA_JPEG`, add the
  `chitra` decoder (one dep serves both formats), and add its two stdlib deps
  (`thread`, `sankoch`) to the `stdlib` list above (chitra's bundle excludes its
  own stdlib deps, so the consumer supplies them):

  ```cyml
  [deps.chitra]
  git = "https://github.com/MacCracken/chitra.git"
  tag = "0.3.1"
  modules = ["dist/chitra.cyr"]
  ```

- **logind surface delegation** (`gpu_surface_configure_native_logind`,
  gated on `#ifdef MABDA_LOGIND`) — build with `-D MABDA_LOGIND` and add
  the `samvada` dbus client:

  ```cyml
  [deps.samvada]
  git = "https://github.com/MacCracken/samvada.git"
  tag = "0.4.1"
  modules = ["dist/samvada.cyr"]
  ```

Then `cyrius deps` pulls the bundle and creates `lib/mabda.cyr` as a
symlink into `$HOME/.cyrius/cache/mabda/dist/mabda.cyr`.

## 2. Write your consumer code

Use only the `@public` API. Every non-FFI file in mabda's `src/` has
a `# @public` or `# @internal` marker on line 1:

- **`# @public`** — stable API, backend-agnostic across wgpu / native
  AMD / native NVIDIA. Same signatures on every backend. Safe.
- **`# @internal`** — FFI scaffolding (`wgpu_types`,
  `wgpu_descriptors`, `wgpu_ffi`). Will be replaced. Do not
  reference these from consumer code.

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
3. **Build the function table.** Populates function pointers for every
   wgpu entry mabda uses. Slots 28/42/48
   (`wgpuCommandEncoderCopyBufferToBuffer` / `...ResolveQuerySet` /
   `wgpuQueueWriteTexture`) are the raw all-scalar wgpu functions, called
   directly via `fncall6` (the old "`fncall6`-plus-wgpu crash" was a
   `%fs`/TLS misdiagnosis resolved in cyrius 6.3.26; struct-packing
   retired at v4.0.2). The shims that remain:
   - `wgpu_shim_buffer_map` — natural 6-arg async→sync bridge
     (`wgpuBufferMapAsync` + `wgpuDevicePoll`); called via `fncall6`
   - `wgpu_shim_command_encoder_begin_render_pass` /
     `..._copy_texture_to_buffer` — genuine struct-by-value descriptors,
     packed into `WgpuBeginPassArgs` / `WgpuCopyTexToBufArgs`
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

## 5. When the wgpu path retires (and what stays)

The native Cyrius backends did **not** replace the wgpu path — v3.0
added native AMD *alongside* wgpu, v4.0 added native NVIDIA. As of the
v4.1.0 baseline the wgpu launcher (`deps/wgpu_main.c`), wgpu-native,
the `wgpu_ffi_init_table` bootstrap, and your libC link are all still
in the tree and still the cross-vendor default. Retirement happens
**per vendor**, and only once that vendor's native backend is in
production:

- **AMD-on-wgpu — deprecated at v4.0.1.** Still works: a context that
  resolves to an AMD adapter under `MABDA_BACKEND_KIND == WGPU` warns
  and continues. Build with `-D MABDA_AMD_WGPU_STRICT` to turn the
  warning into a hard reject once you've moved to the native AMD
  backend (`gpu_context_new_native`). Actual retirement is deferred.
- **NVIDIA-on-wgpu — slated to deprecate at v5.0**, in favor of the
  native NVIDIA backend (`gpu_context_new_native_nvidia`, shipped
  v4.0).
- **The C launcher / wgpu binding itself — leaves the tree at v5.1**,
  after both native paths are in production. That is when
  `deps/wgpu_main.c`, `deps/wgpu-native/`, the
  `wgpu_ffi_init_table(fn_table_ptr)` line, the `make -C deps` step,
  and the libC dependency finally go away and a wgpu-free consumer
  becomes pure Cyrius.

To move off the wgpu launcher today (native AMD or NVIDIA), you skip
the C launcher entirely and call the native entry points directly —
`gpu_context_new_native()` (AMD, amdgpu DRM/GFX9) or
`gpu_context_new_native_nvidia()` (NVIDIA, nouveau DRM/SM75) — instead
of the launcher's `gpu_context_from_preinit(preinit_ptr)`. Backend is
selected at compile time via `MABDA_BACKEND_KIND`.

**What stays, on every backend:**

- Your `src/main.cyr` source, unchanged
- The `texture_from_rgba(device, queue, rgba_ptr, width, height,
  label)`, `compute_dispatch(device, queue, cp, bind_group, dims_xyz)`,
  `render_pipeline_create_simple(device, module, color_format)` API —
  identical signatures across wgpu / native AMD / native NVIDIA
- The `mabda_main` entry point when using the wgpu launcher

The `examples/stdlib-consumer/` project is the regression test: if it
still compiles against the current mabda tag, the contract held.

## Known transitional warnings

When compiling the bundled `dist/mabda.cyr`, cc5 emits
`undefined function` warnings for the 65 wgpu function-table slots.
These are **expected and benign** — the slots are globals populated
by the C launcher at runtime. They did **not** become real Cyrius
definitions in v3.0/v4.0: the native AMD and NVIDIA backends are
separate code paths, not reimplementations of these slots. The
warnings persist for as long as the wgpu path is in the tree, and go
away only when the wgpu binding itself is removed at v5.1.

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

Mabda runs a security audit each P(-1) / release pass; the dated
findings + remediation records live in
[`docs/audit/`](audit/), running through
[`2026-07-02-audit.md`](audit/2026-07-02-audit.md) as of the v4.0.1
baseline (the `2026-04-19-audit.md` pass referenced above is the older
2.3.0 stdlib-candidate record, retained for history). Every HIGH / MED
finding lands with a regression assertion in the relevant
`tests/tcyr/*.tcyr` domain suite. See
[`SECURITY.md`](../SECURITY.md) for the reporting policy.
