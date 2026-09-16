# vk_memtrack — device-memory growth check for wgpu programs

An explicit Vulkan layer that intercepts `vkAllocateMemory` / `vkFreeMemory` and prints live
device memory after every call. `check_bench_memory.sh` builds it, runs `make bench-gpu` (or
another wgpu target) under it, and `parse_mem.py` fails the run if any benchmark row grows
live device-local memory by more than 256 MiB (one allocator block of slack).

```sh
programs/diagnostics/vk_memtrack/check_bench_memory.sh            # bench-gpu
programs/diagnostics/vk_memtrack/check_bench_memory.sh test-render-e2e
```

Why it exists: nouveau exposes no readable device-memory counter to a normal user, and RSS
doesn't include VRAM. This layer counts what the process actually allocated through Vulkan.
It proved the MSAA render-target leak fixed in 4.1.3 (the msaa4 row grew device memory by
3840 MiB on NVK before the fix, 0 after). The count is exact for `vkAllocateMemory`-backed
memory; allocations a driver makes without it are not counted.

Requirements: `deps/wgpu-native`, a Vulkan driver, `cc`, the Vulkan headers, `python3`.
It is a developer diagnostic, not part of `make test` or CI (no GPU there).
