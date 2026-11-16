# wgpu backend: every frame leaks its command buffer and 656 B of pass records — about 1.6 KB a frame, ~350 MB an hour at 60 fps

**Status:** 🔴 **OPEN** — memory leak.
**Filed:** 2026-11-15, from soorat's Cyrius-port probe P8 (`soorat/scripts/probes/gpu/src/p8_gpu_frame.cyr`; logs
`soorat/scripts/probes/gpu/build/logs/p8-gpu-*.log`; write-up `soorat/docs/development/port-parity.md` §1.5).
**Placement:**
- `src/backend_wgpu.cyr`, `_backend_wgpu_render_pass_end` (line ~1035): the submitted command buffer is never
  released.
- `deps/wgpu_main.c`, `wgpu_shim_queue_submit_one` (line ~152); the function table (`FN_COUNT 67`) has no
  `wgpuCommandBufferRelease`.
- `src/backend_wgpu.cyr`, `_backend_wgpu_render_pass_begin` (line ~935), and `src/render_pass.cyr`,
  `rpb_pass_new` (line ~29): per-call bump allocations.
- `src/backend_wgpu.cyr`, `_backend_wgpu_render_pass_draw` (line ~966): the textured draw's per-call bump
  allocations.

**Severity:** **High.** Any long-running render loop grows without bound: a game, an editor viewport, a
visualization. soorat is the first consumer to run one.
**Affects:**
- mabda 4.2.0, wgpu backend: both the stdlib bundle shipped with cyrius 6.7.18 and tag `4.2.0`.
- Measured on AMD Cezanne (`0x1002:0x1638`, RADV RENOIR), Mesa 26.2.6, wgpu-native v29.0.1.1.
- The native backends were not measured.

## Summary

The loop ran 100,000 frames of the public tier only:
- three `gpu_buffer_write`s;
- `gpu_render_pass_begin`, `gpu_render_pass_bind_texture`, `gpu_render_pass_draw` and `gpu_render_pass_end`;
- a device wait.

| Loop | RSS, frame 1 → 100,000 | Growth per frame |
|---|---|---|
| the full loop | 96.2 MB → **260.3 MB** | ≈ 1,641 B |
| the same, launcher patched to release each command buffer after submit | 95.9 MB → 108.5 MB at frame 20,000 | 656 B (the Cyrius heap alone) |
| waits only / CPU only | flat at 96.1 MB | 0 |

There are two independent causes.

1. **Command buffers are never released (≈ 985 B per frame of native heap).** `_backend_wgpu_render_pass_end`
   does `wgpu_command_encoder_finish`, then `wgpu_queue_submit_one`, then releases the *encoder* and the
   *pass encoder*, but never the command buffer. `wgpu_shim_queue_submit_one` only calls
   `wgpuQueueSubmit(queue, 1, &cmd)`, and no function-table slot can release a `WGPUCommandBuffer`.
2. **Per-call records come from the global bump allocator (656 B per frame of Cyrius heap).** It never
   frees, and `alloc_used` deltas were measured around single calls:
   - `gpu_render_pass_begin`: **432 B** per call. `rpb_pass_new` (48 B plus a `vec`), the colour attachment,
     and the 48-byte pass record.
   - a textured `gpu_render_pass_draw`: **224 B** per call. The sampler descriptor and the bind-group
     descriptor.
   - Every other per-frame public call: 0 B.

At 60 fps the two together come to about 350 MB an hour (an inference from the per-frame rate).

## Suggested fix

1. **Release after submit.** One line in the launcher:
   ```c
   void wgpu_shim_queue_submit_one(WGPUQueue queue, WGPUCommandBuffer cmd) {
       wgpuQueueSubmit(queue, 1, &cmd);
       wgpuCommandBufferRelease(cmd);   /* wgpu keeps its own reference until the GPU is done */
   }
   ```
   soorat's probe built exactly this as a patched copy, `soorat/scripts/probes/gpu/build/wgpu_main_cbrelease.c`
   (a one-line diff), and the native-heap growth went away. Alternatively, add a `CommandBufferRelease` slot
   and release from `_backend_wgpu_render_pass_end`; the shim fix needs no slot. Either way it is a launcher
   change, so it belongs in the same launcher revision as the render-tier request
   (`2026-11-15-render-tier-for-geometry-consumers.md`), so consumers rebuild once.
2. **Stop bump-allocating per pass and per draw.** Either:
   - one reusable pass record and attachment block per context, with descriptors in scratch owned by the
     context; or
   - a per-frame arena reset at `gpu_render_pass_end`.

   Caching the sampler, and the bind group per (texture, sampler intent), would also remove the per-draw wgpu
   object churn: today each textured draw creates and releases a sampler and a bind group.

## Measure

From `soorat/scripts/probes/gpu/`: `sh build.sh && sh run_p8_gpu.sh`. The probe prints RSS and the
`alloc_used` growth per frame at frames 1, 10, …, 100,000, and the per-call `alloc_used` deltas. A fix shows
flat RSS and 0 B of heap growth per frame.
