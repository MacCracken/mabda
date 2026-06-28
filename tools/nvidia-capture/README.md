# NVK compute-capture harness (v4.0 N0.7c / N4 bring-up)

Dev tooling to capture a **known-good NVK compute dispatch** on nouveau and
byte-diff mabda's hand-built QMD / pushbuffer / NVIF stream against it. This
is **build-time-only dev tooling — not part of the mabda library build**
(it depends on the CUDA toolkit + Vulkan headers + NVK). It is the
methodology behind the decoded references in
[`../../docs/development/nvidia-n4-capture-notes.md`](../../docs/development/nvidia-n4-capture-notes.md).

The raw capture *output* (NVK's QMD/pushbuffer bytes) deliberately does
**not** live in the repo (per the punchlist); only this harness, which
regenerates it, is preserved.

## Files

| File | What it is |
|---|---|
| `nouveau_capture.c` | `LD_PRELOAD` `ioctl`/`mmap` interposer. Decodes every nouveau DRM ioctl, tracks GEM/mmap/VM_BIND state, and on `EXEC` dumps the pushbuffer + every VM-bound BO (incl. the 256-byte QMD) as raw `.bin`. Logs `GEM_NEW` flags (domain/tile) + `VM_BIND` flags + the NVIF `0xC5C0` class-create payload. |
| `vk_compute.c` | Minimal **headless** NVK Vulkan compute app: dispatches a SPIR-V `store 0xDEADBEEF` shader to a host-visible buffer and reads it back. Runs over SSH (no display/master). |
| `probe.comp` | The GLSL compute shader (`v[0] = 0xDEADBEEF`). |
| `vk_compute_tex.c` | **N6.2** headless NVK Vulkan compute **sampling** app: uploads a 1×1 RGBA8 texel, samples it (NEAREST/clamp), packs + stores. Captures the golden TIC/TSC descriptors + the sampling SASS (bound-texture model). Decoded in [`../../docs/development/nvidia-n6-capture-notes.md`](../../docs/development/nvidia-n6-capture-notes.md). |
| `probe_tex.comp` | The GLSL sampling shader (`v[0] = packUnorm4x8(texture(tex, vec2(0.5)))`). |
| `store_deadbeef.cu` | The CUDA kernel for the **ptxas SASS capture** (N5.1) + the `libdevice` non-contamination hygiene check (ADR 007). |
| `store_tex.cu` | **N6.2c** CUDA `tex2D` kernel — the ptxas **sampling SASS** capture source (bound TEX, TIC 0 / TSC 0x58, out ptr @ `c[0x0][0x168]`). Embedded (STG patched to GPU scope) as `native_nv_sass_sample_tex` in `src/backend_nvidia_sass.cyr`. |
| `run_capture.sh` | Builds everything and runs the dispatch under the interposer → `./nvcap/`. |
| `cuda-hygiene-check.sh` | ADR 007 build-hygiene: compiles `store_deadbeef.cu` for `sm_75`, dumps SASS, verifies no `libdevice`/Attachment-A contamination, prints the EULA sections. |

## Prerequisites (Arch; the bring-up box is wiped per GPU swap)

```
sudo pacman -S cuda vulkan-headers      # vulkan-nouveau + glslc + gcc already present
```
Host must be on **nouveau** with the render node at `/dev/dri/renderD128`
(see `../../docs/development/nvidia-bringup-hardware.md`).

## Run

```
./run_capture.sh            # → ./nvcap/capture.log + *.bin dumps
```

Then decode (examples are in the git history of `nvidia-n4-capture-notes.md`;
the `python3` one-liners that walk the pushbuffer / extract the golden QMD /
disassemble NVK's shader are reproducible against `./nvcap/`).

## Outputs are throwaway

Everything the scripts produce (`nvcap/`, `*.o`, `*.so`, `*.cubin`, `*.spv`,
`*.bin`) is a build artifact — see `.gitignore`. Do **not** commit captured
bytes.
