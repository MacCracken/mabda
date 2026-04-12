# 004 — C launcher with function table for wgpu FFI

## Status: Accepted

## Context

Cyrius compiles to standalone ELF binaries or `.o` relocatable objects. It cannot call C functions directly (no extern declarations, no dynamic linker integration). wgpu-native is a Rust-compiled C library that requires libc, pthreads, and dlopen for Vulkan driver loading.

Three approaches were evaluated:
1. Pure Cyrius: load .so via `dynlib.cyr` (mmap + ELF parsing)
2. System dlopen: bootstrap libc's dlopen from `ld-linux.so`
3. C launcher: compile Cyrius to `.o`, link with C `main()` via gcc

## Decision

Use a C launcher (`wgpu_main.c`) that:
1. Calls `_cyrius_init()` to initialize Cyrius globals (enums, vars)
2. Calls `alloc_init()` to set up the Cyrius heap
3. Pre-initializes the GPU (instance, adapter, device, queue) in C
4. Builds a function table of 40 wgpu function pointers
5. Calls `mabda_main(fn_table_ptr, preinit_ptr)` with the table and GPU handles

**Why not pure Cyrius (option 1):**
- `dynlib.cyr` can read ELF symbol tables but cannot resolve relocations
- wgpu-native .so depends on libc/pthreads — needs the real dynamic linker
- Loading a .so via mmap without relocation processing crashes

**Why not system dlopen (option 2):**
- Calling libc's dlopen from a non-libc process crashes (TLS not initialized)
- Even after bootstrapping dlopen, glibc 2.38+ refuses dlopen from DT_TEXTREL binaries

**Why C launcher works (option 3):**
- C `main()` gets full libc initialization (TLS, pthreads, dynamic linker)
- GPU pre-init happens before Cyrius code runs (avoids TEXTREL issues)
- Function table pattern is clean: Cyrius calls `fncall2(_fp(8), device, desc)`
- C shim wraps by-value struct callbacks (40-byte WGPUCallbackInfo)

## Consequences

- GPU programs need gcc + wgpu-native .a for linking (not standalone Cyrius binaries)
- `_cyrius_init()` must run before `alloc_init()` (init resets global state)
- Symbol clashes (memcpy, memset, strlen) require `objcopy -L` post-processing
- Adding new wgpu functions requires updating both `wgpu_main.c` and `wgpu_ffi.cyr`
- Standalone Cyrius tests (color, profiler, vertex) don't need the C launcher
