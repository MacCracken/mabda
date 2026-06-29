# Mabda v4.0 — NVIDIA Native Backend Punch List

**Status:** In progress. **N0–N5.3 + N4.7 + all of N6 (textures) + ALL of N7
(render, N7.1–N7.5) done on the TU116 — BOTH arc gates GREEN (N4 compute + N7.4
render), and the render path is now reachable through the PUBLIC `gpu_render_*`
API (N7.5). All 7 NVIDIA HW gates pass (compute store/api, texture
roundtrip/sample, render-target, render-e2e, render-api). The NVIDIA backend
fills v0+v1+v2 (compute + textures + render). **N8 (present): N8.1 KMS probe +
N8.2 scanout FB + N8.3 LIVE MODESET + N8.4 ANIMATED PRESENT all DONE + HW-proven
— a pure-Cyrius nouveau stack drove a real monitor: solid RED (modeset), then a
scrolling blue→red gradient (~2s, vsync'd double-buffered PAGE_FLIP), restoring
the console cleanly each time. The generic AMD KMS helpers walk + drive nouveau;
only the card-node opener is nouveau-specific — no separate
backend_nvidia_kms.cyr. **N8.5 DONE + HW-PROVEN — present is now reachable
through the PUBLIC `gpu_surface_*` API**: the NVIDIA backend fills the v3 surface
slots (176..208) and `nvidia_surface_present_e2e.cyr` scanned out the ~2s
scrolling gradient on the TU116 through `gpu_surface_configure_native_kiosk` /
acquire / present / release, then restored the console. **This closes the v4.0
NVIDIA arc: compute → textures → render → present, all public-API-reachable on
real NVIDIA silicon. ALL 8 NVIDIA HW gates green.**** Toolchain on cyrius 6.3.1;
main merged (garbage-free). (2026-06-28)**
**Date opened:** 2026-06-27
**Branch:** `4.0-work`

> **THE ARC GATE IS GREEN (2026-06-28):** `make test-nvidia-compute-store`
> reads back `0xDEADBEEF` twice on the same channel via a pure-Cyrius
> nouveau dispatch (exit 0, stable across runs). **Root cause of the
> 2026-06-27 "global-store hang" was a relative-vs-absolute syncobj-timeout
> bug**, not a dispatch-init gap: the gate passed a bare `1e9` to
> `native_syncobj_wait` (which takes an **absolute** CLOCK_MONOTONIC
> deadline), so the DRM wait returned ETIME instantly — long before
> nouveau's async EXEC fence signaled. The dispatch had always completed.
> The QMD/SASS/method stream were correct all along; the "five eliminated
> causes" were red herrings (except the real memory-windows fix). Full
> writeup in [`nvidia-n4-capture-notes.md`](nvidia-n4-capture-notes.md)
> § N4 implementation status — GATE GREEN. The NVK-capture harness is
> preserved at `tools/nvidia-capture/`.
>
> **Resume here (next session):** **N8 is COMPLETE + HW-PROVEN — the v4.0 NVIDIA
> arc (compute → textures → render → present) is closed, all reachable through
> the public API on real NVIDIA silicon (ALL 8 HW gates green).** N8.5 (the last
> piece) is done: the NVIDIA backend fills the v3 surface slots (176..208) and
> `programs/nvidia_surface_present_e2e.cyr` scanned out the ~2s gradient on the
> TU116 through `gpu_surface_configure_native_kiosk` / acquire / present /
> release, then restored the console.
> **Two fronts remain for the v4.0 cut, neither blocking the arc:**
> (1) **v4.0 ship work** — **soak burn-in** (the NVIDIA analogue of the AMD
> rc.3/rc.4 gates, now wired into `scripts/soak.sh`: `--workload=nvidia-native`
> loops compute+render+texture masterless on renderD128, `--workload=nvidia-present`
> loops the public-API present on a tty; the dmesg watch is driver-aware —
> `nouveau|drm|Xid|GSP`, so a GSP-RM Xid fault trips the detector like an amdgpu
> TDR. A 5s masterless smoke ran ~561 dispatches, 0 fail, flat RSS, dmesg Δ=0 —
> the real gate is a 6h/24h run with `sudo` for dmesg capture); bundle the NVIDIA
> modules into the dist `[lib].modules` (today the whole NVIDIA backend is
> source-only, absent from the v3.4.5 dist); wire the compile-time
> `MABDA_BACKEND_KIND==NVIDIA` branch in `gpu_context_from_preinit`; bump
> `VERSION` → 4.0.0; CHANGELOG; six-consumer regression sweep; audit. (2) **N9**
> (SPIR-V→SASS in-tree compiler) — the v4.x endgame so consumers ship one shader
> form; its own sub-arc, deferrable to v4.1. Pick either; the present arc no
> longer gates anything.
> Done: v0 compute, v1 texture roundtrip, N6.2 sampling, N7.1 render-target, N7.2a
> draw capture/decode, N7.2 clc597 draw encoder, N7.3 VS/FS SPH+SASS, N7.4 the
> render gate, **N7.5 the public render API**.
>
> **Done + HW-proven:** N1 enum (`make test-nvidia-enum`), N2 GEM roundtrip
> (`-mem-roundtrip`), N3 channel/VM/sync setup (`-channel-setup`), **N4
> compute store (`-compute-store`)**, **N4.7 public-API compute
> (`-compute-api`)**, **N6 texture roundtrip (`-texture-e2e`)**, **N6.2 GPU
> texture sampling (`-texture-sample-e2e`)**, **N7.1 render target
> (`-render-target`)**, **N7.4 RENDER GATE (`-render-e2e`)**, **N7.5 public
> render API (`-render-api`)**. CPU suite: `tests/tcyr/nvidia.tcyr` (283
> asserts). ADR 007 **Accepted**.
**Roadmap reference:** [`roadmap.md` § v4.0](roadmap.md#v40--nvidia-native-backend-amd-wgpu-retirement-deferred-to-401)
**Bring-up hardware:** Turing first — GTX 1660 Super (TU116, SM75, 6 GB GDDR6) — then Ampere (RTX 3060). Tracked in [`nvidia-bringup-hardware.md`](nvidia-bringup-hardware.md).

> This arc adds a **pure-ioctl native NVIDIA path** that fills the same
> `@internal` `Backend` slot table (`src/backend.cyr`,
> `BACKEND_SIZE=328`, `BACKEND_KIND_NVIDIA=3` already reserved) that the
> AMD native backend fills. The public API does not change. The
> arc-gating milestone mirrors AMD's Phase B.4: a compute shader
> dispatches, writes `0xDEADBEEF` to a buffer, the CPU reads it back —
> via a dispatch driven entirely from Cyrius `syscall(SYS_IOCTL)`, zero
> C/Rust userspace libraries.
>
> The AMD arc structure (`backend_native_amdgpu.cyr` ioctl wrappers →
> `backend_native_shaders.cyr` ISA → `backend_native_pm4.cyr` command
> stream → `backend_native.cyr` integration + slot fills, plus the
> `programs/native_*` bring-up ladder) is the template. What **reuses**:
> the slot table, the Cyrius struct/ioctl idioms, the `_IOC` encoder,
> generic-DRM syncobj/PRIME/KMS wrappers, the DRM-master/samvada-logind
> story, the buffer-as-mapped-BO shape, and the bring-up program ladder.
> What **fully replaces**: the BO allocator, the GPU context/channel, the
> submission engine (GPFIFO + QMD vs CS + PM4), and the shader ISA (SM75
> SASS vs GFX9).

---

## Hard truths up front

Read these before sequencing. Three of the four design forks have a
research recommendation **plus** an adversarial caveat that materially
changes the work; two of those still need the maintainer's strategic
call before deep work starts (flagged **DECISION NEEDED** and repeated
in the Open Questions section).

### Fork 1 — Kernel driver path: nouveau vs nvidia.ko  **(DECISION NEEDED)**

- **Recommendation: target nouveau's modern `VM_INIT` / `VM_BIND` /
  `EXEC` uAPI** (Linux ≥ 6.6, the path NVK uses), **not** the legacy
  `CHANNEL_ALLOC`/`GEM_PUSHBUF` reloc path and **not** nvidia.ko RM as
  the shipping path. nouveau maps almost 1:1 onto mabda's existing
  amdgpu pure-ioctl pattern: same `_IOC` encoding (DRM type `'d'`=0x64),
  same render node (`/dev/dri/renderD128`), same GEM-handle model, same
  DRM syncobj wait — `VM_BIND` ≈ AMD `GEM_VA`, `EXEC` ≈ AMD `CS`. Both
  paths are genuinely libnvidia/CUDA-free (proven by tinygrad's `NV`
  backend and gvisor's nvproxy for nvidia.ko, and by NVK for nouveau);
  the choice is *which ioctl surface*, not *whether* sovereignty is
  reachable.
- **Caveat A — the new uAPI is not "6 frozen ioctls written once."** Two
  primitives the research recipe under-counted are mandatory and must be
  hand-encoded too: **`DRM_NOUVEAU_VM_INIT` (0x10) must be called first**,
  before any BO or channel is created (the kernel header mandates it for
  the `VM_BIND` uAPI), and **`DRM_NOUVEAU_CHANNEL_ALLOC` (0x02, legacy
  abi16) is still required** — `drm_nouveau_exec.channel` references a
  channel created through it, with engine/class select + USERD/notifier
  setup (this is the real analog of AMD's `CTX` step). Still pure ioctl,
  but budget for it.
- **Caveat B — GSP firmware.** Turing nouveau init/reclocking is
  delegated to NVIDIA's on-GPU GSP-RM firmware (`nouveau.config=NvGspRm=1`;
  default for Turing from Linux 6.18). This is **kernel-loaded firmware
  from `/lib/firmware`, the same category as the amdgpu microcode the AMD
  native path already relies on — NOT a userspace C/Rust library — so it
  does not violate sovereignty.** But: it is a versioned binary blob
  mabda cannot audit (confirm redistribution license for AGNOS
  consumers, treated like the consumer-provided wgpu-native/libsystemd
  model); the "GSP is a soft/performance-only blocker" framing is
  **optimistic** — the new-uAPI compute path is conformance-tested only
  *with* GSP, so plan as if GSP is a hard prerequisite until a live spike
  proves boot-clock no-GSP `EXEC` works; and **the soft-blocker framing
  expires after Turing — Ampere (RTX 3060, rung 2) effectively requires
  GSP-RM to initialize at all.**
- **Caveat C — the bring-up box is not running nouveau today.** It runs
  proprietary nvidia.ko 610.43.02 bound to `renderD128`/`card1`; nouveau
  is present-but-unbound. Switching is a real, untested step: blacklist
  nvidia, unload `nvidia_drm/modeset/uvm`, bind nouveau with
  `NvGspRm=1`, confirm kernel ≥ 6.6 and the `nvidia/tu116/gsp` blob is
  present. The host doc says the box is wiped per GPU swap, so this is an
  accepted bring-up step, not a regression.
- **nvidia.ko RM API = documented fallback only.** Pure-ioctl-drivable
  and currently installed/performant, but it shares none of mabda's DRM
  scaffolding (magic `'F'`=0x46, `/dev/nvidiactl`+`/dev/nvidia0`, a deep
  `RmAlloc` object tree, `NVOS21/32/33/46/54/64` structs) and is **not a
  stable UABI** — gvisor carries per-driver-version struct variants
  (V545…V610), a perpetual re-validation tax that contradicts mabda's
  auditable/stable ethos. Keep it as a short de-risking spike (gvisor's
  V610 variants match the installed 610.43.02), not the architecture.

### Fork 2 — Submission model: GPFIFO + pushbuffer + QMD (no PM4)

- **Recommendation: structural port of the AMD path.** Three new Cyrius
  layers mirror the AMD ones: (1) a method/pushbuffer builder (the PM4
  analogue), core primitive `native_nv_method_header =
  (opcode<<29)|(count<<16)|(subc<<13)|(mthd_byte_offset>>2)`, opcodes
  `1=INCR / 3=NON_INCR / 4=INLINE / 5=INCR_ONCE` (envytools dma-pusher,
  GF100+/Turing form); (2) a **256-byte QMDV02_03** builder (the
  dispatch descriptor — *no AMD analog*, a fixed bit-packed binary
  struct); (3) a submit wrapper over `CHANNEL_ALLOC` + `VM_BIND` +
  `EXEC` + a `drm_nouveau_sync` syncobj, reusing mabda's existing
  syncobj-wait code. Launch via `SET_OBJECT(TURING_COMPUTE_A=0xC5C0)`
  then `SEND_PCAS_A(qmd_va>>8)` + `SEND_SIGNALING_PCAS_B(INVALIDATE|SCHEDULE)`;
  signal completion with an in-QMD `RELEASE0` or a host-class
  `SEM_EXECUTE` release. Class IDs verified: compute 0xC5C0
  (`clc5c0.h`), channel 0xC46F (`clc46f.h`), usermode doorbell 0xC361
  (`clc361.h`).
- **Caveat — correctness ≠ feasibility.** Like PM4, one wrong QMD or
  method bit yields a **silent hang/TDR, not an error**. Apply the exact
  protocol mabda used for PM4: byte-diff the hand-built QMDV02_03 and
  pushbuffer against an NVK-captured known-good stream before claiming
  correct. The minimal per-channel init preamble (shader local/shared
  memory sizing, exceptions, cache invalidate) must be pinned against
  NVK's `nvk_queue_init` / nouveau nvc0 compute init — under-initializing
  shared/local memory is a common silent-hang cause. Confirm the numeric
  `SASS_VERSION` the QMD expects for SM75 from NAK (the header only names
  the field, `MW(1023:1016)`).
- **fncall ceiling note:** the 6-arg ceiling is a *wgpu-path* constraint
  (Cyrius-fn → `fncall*` → wgpu-native). The native path uses
  `syscall(SYS_IOCTL)` directly, so pure-Cyrius native fns legitimately
  take 10–12 args. The constraint that *does* bind: the backend **slot**
  dispatch (Cyrius-fn-via-fnptr-via-struct-slot) — keep slot signatures
  ≤ 6 args; fold QMD/GPFIFO state behind a struct pointer.

### Fork 3 — Shader ISA: hand-precompiled SM75 SASS first, compiler later  **(DECISION NEEDED on build-time toolchain)**

- **Recommendation: two-phase, mirroring the GFX9 arc.** Bring-up:
  reuse mabda's proven "capture-the-bytes" pattern
  (`docs/archive/proposals/v3-shader-bytes-capture.md`) — compile a
  trivial "store 0xDEADBEEF" kernel **offline** with ptxas, extract the
  SM75 SASS with `cuobjdump --dump-sass` / `nvdisasm -hex` (both support
  `sm_75`), embed the dwords in `src/backend_nvidia_sass.cyr`. The
  embedded cubin carries ptxas-correct per-instruction control bits (the
  hard part) baked in, so the proof-of-life kernel needs no scheduler.
  Long-term: an in-tree SPIR-V→SASS compiler ported algorithmically from
  Mesa NVK's NAK (Fork-9 below).
- **Caveat A — this INVERTS the GFX9 precedent's sovereignty.** GFX9's
  recommended path was hand-encoding against a *public* ISA spec with the
  *open* llvm-mc as oracle. NVIDIA publishes **no** ISA spec and **no**
  open assembler — `nvdisasm` is **disassemble-only** (so "round-trip
  oracle / direct analog of llvm-mc" is loose; the oracle is
  one-directional: encode → `nvdisasm` → diff, exactly as NAK's
  `nvdisasm_tests.rs` does). The easy way to *produce* SM75 bytes is
  ptxas/NVRTC (closed CUDA C/C++) or NAK (Rust). Used **offline at build
  time** to emit inert embedded dwords, this does **not** put a userspace
  lib in the runtime ioctl path (consistent with the rules), but it makes
  the *build pipeline* less sovereign than AMD's. **The doctrine call —
  closed ptxas-cubin-capture vs the open-but-incomplete turingas /
  CuAssembler vs pure hand-encode — needs the maintainer's decision.**
- **Caveat B — "embed the bytes and it runs" is understated.** A ptxas
  cubin reads its output pointer and grid/block dims from the
  driver-managed constant bank `c[0x0]` at CUDA-defined offsets, and
  carries regcount / shared-mem in its ELF. Running it via raw ioctl
  means the QMD (Fork 2) must replicate that constant-bank/param layout
  and reflect regcount/shmem **exactly**, or the SASS reads garbage. So
  **the shader-bytes work is coupled to the submission work — proof-of-life
  cannot be gated on shader bytes alone.** For the very first spike, a
  trivial kernel can sidestep the param ABI by mapping the output at a
  mabda-chosen fixed VA and patching a MOV-immediate into the captured
  SASS, but that re-introduces the encoding knowledge capture was meant
  to avoid.
- **Caveat C — sovereignty leak risk.** CUDA tools may be used ONLY to
  *compile* offline, never to *launch*. If anyone shortcuts the
  QMD/constant-bank work by using the CUDA userspace runtime to set up or
  launch the kernel — even "just to get DEADBEEF first" — the constraint
  breaks. Enforce the build-time/runtime boundary explicitly.
- The Turing control-bit / **software-scoreboard** model (per-instruction,
  mandatory, ~6 barriers, single-issue — Volta dropped dual-issue) is the
  single biggest delta vs GFX9 and the principal new cost of the
  long-term compiler. Even a 2–3 instruction hand-encoded `MOV/STG/EXIT`
  needs a correct stall count and a write-barrier before `EXIT`/readback.

### Fork 4 — Memory model + present

- **Recommendation: host-visible-first allocator; defer present.** For
  the compute proof-of-life, make the output a **host-visible buffer**
  (nouveau `GEM_NEW` domain `GART|COHERENT` — the direct analog of
  `native_bo_create_gtt`) — no vidmem, no BAR1, no staging. Defer vidmem
  entirely; when added, **gate CPU-mapping of vidmem on actual BAR1 size**
  (stock Turing TU116 BAR1 = 256 MB, no official ReBAR) and use a
  host-staging BO + GPU DMA-copy, mirroring AMD VRAM/GTT.
- **Present: defer past compute, then build it on nouveau.** nouveau is a
  normal DRM/KMS driver — `GETRESOURCES`/`ADDFB2`/`SETCRTC`/`PAGE_FLIP`/
  atomic + the render-node→card-node PRIME bridge + the samvada/logind
  master story all carry over from `src/backend_native_kms.cyr` largely
  unchanged. Differences: block-linear FB format modifiers (or force a
  linear scanout BO) and the scanout BO comes from nouveau GEM/PRIME.
- **Caveat — do NOT attempt a GPU-buffer present on nvidia-drm.** It
  accepts atomic KMS + dumb buffers but a foreign/non-nvkms buffer cannot
  be used as a scanout framebuffer (the accurate constraint; nvidia-drm
  *does* import foreign dma-buf, it just can't scan it out). The only
  pure-ioctl present on the installed-as-is nvidia.ko host is a CPU blit
  into a sysmem dumb-buffer FB — a throwaway fallback, not the
  architecture. This is another reason present steers to nouveau.

### Conventions carried over from the AMD arc (honor in every step)

- **Every new code path adds a CPU assertion.** CPU tests run with no
  GPU/master/dbus against `src/lib.cyr`; native ioctl shapes surface as
  null-safety + struct-shape tests.
- **Stack-local `var buf[N]` for dispatch/test-scoped buffers.** The bump
  allocator never reclaims; per-dispatch `alloc()` leaks and exhausts it.
  Long-lived per-context BOs (channel, GPFIFO, USERD, pushbuffer,
  semaphore, QMD) are alloc'd once at context create; per-submit scratch
  (GPFIFO entry, exec_push, sync arrays) is stack-local `var`.
- **`var X = expr;` is mandatory** — Cyrius rejects bare `var X;`. Use
  `var X = 0;` for to-be-set-later.
- **Manual little-endian struct packing must be byte-exact to the
  x86_64 kernel/RM UAPI.** `drm_nouveau_exec` / `exec_push` /
  `vm_bind_op` / `sync`, and (fallback) `NVOS21/33/46/54/64`, plus the
  GPFIFO entry and the 256-byte QMD — every struct gets a header comment
  with field offsets; a wrong size EINVALs or silently corrupts.
- **`_IOC` encoder reuses verbatim**: nouveau = DRM type `'d'`=0x64;
  nvidia.ko fallback = NV magic `'F'`=0x46 with `NV_ESC_*` nr. Watch
  **global-init declaration order** — a const referencing another const
  must be declared *after* it (the `_NATIVE_PERM_*` zero-value bug).
- **Keep new NVIDIA modules split under the 128 KiB cyrius lint/fmt
  per-file cap** (this drove the 4-way split of `backend_native`); lint
  **per-file** (the bare repo-wide form was removed in 5.7.x).
- **`src/lib.cyr` owns the entire include chain** — no stdlib includes in
  individual `src/*.cyr`. The NVIDIA modules have a load-bearing order
  (nouveau ioctl wrappers → SASS → pushbuffer → QMD → integration),
  mirroring amdgpu → shaders → pm4 → backend_native.
- **Never edit `lib/*.cyr`** and never replace `lib/` with a symlink —
  it's a `cyrius deps`-populated artifact; edits write through into the
  cyrius repo.
- **Test-count NUL trap:** any new `.tcyr` whose summary line leads with
  a NUL byte is dropped by `grep`/`awk`. Use
  `./scripts/count-test-assertions.sh`.

---

## Tier 1 — Code completeness

Phases mirror the AMD ladder (B.1 enum → B.2 mem → B.3 submit-setup →
B.4 compute → C texture → C render → D present). Each step is sized for
an afternoon to a few days.

### N0 — Design spike, ADR 007, host setup, ioctl-capture methodology

- [ ] **N0.1** — Baseline the bring-up box as-shipped (regression
  backstop before any swap): `nvidia-smi` enumerates the GTX 1660 Super
  (TU116, driver 610.43.02, CUDA 13.3); run a CUDA driver-API vectorAdd
  and the existing wgpu/Vulkan E2E suite; record state in
  `nvidia-bringup-hardware.md`.
- [ ] **N0.2** — Clone `NVIDIA/open-gpu-kernel-modules` **@ tag
  610.43.02** (exact match to the installed module) for the authoritative
  UAPI: `nv_escape.h`, `nvos.h`, `class/clc5c0.h` (compute) / `clc46f.h`
  (channel) / `clc361.h` (doorbell), `classes/compute/clc5c0qmd.h` (QMD).
  Clone Linux `include/uapi/drm/nouveau_drm.h` for the nouveau path. Read
  only — no code yet.
- [x] **N0.3 (Fork 1) — DECIDED: nouveau.** ADR 007 landed
  ([`docs/adr/007-nvidia-native-kernel-path.md`](../adr/007-nvidia-native-kernel-path.md)),
  capturing nouveau-primary / nvidia.ko-fallback + caveats A–C. The call
  is now also HW-proven (N1 probe: masterless `VM_INIT` on the TU116).
  ADR 007 **Accepted** 2026-06-27.
- [x] **N0.4 (capture methodology) — DECIDED.** Recorded in ADR 007:
  primary references = **open-gpu-kernel-modules headers + Mesa NVK/NAK +
  tinygrad `ops_nv.py` (nvidia.ko path only — NOT a nouveau citation)**;
  `valgrind-mmt`+`demmt` demoted to last-resort. nvidia.ko de-risk spike
  deemed unnecessary (nouveau came up masterless first try). Captures →
  scratch dir.
- [x] **N0.5 (Fork 3 doctrine) — DECIDED: ptxas-capture primary +
  hand-encode insurance + NAK-port endgame.** Recorded in ADR 007. The
  GPL-3.0 licensing concern is resolved (compiler-output-follows-input,
  zero runtime linkage → no license conflict; ~HIGH confidence); the two
  residual items (read CUDA 13.3 `EULA.txt`; `libdevice` non-contamination
  check) are build-hygiene action items in ADR 007, **not** gates.
- [x] **N0.6** — Host swapped to nouveau. **Done via reboot 2026-06-27**
  (the box ran nvidia.ko 610.43.02 the same morning; the reboot bound
  nouveau instead of a live unbind). Confirmed: kernel `7.0.13-arch1-1`
  (≥ 6.6 ✓); `nvidia/tu116/gsp/` present (535.113.01 + 570.144); nouveau
  bound to `renderD128` **and `card0`** (not `card1` — nvidia.ko was the
  card1-era node); nvidia.ko unloaded. dmesg shows GSP-RM **570.144
  active** and `Initialized nouveau 1.4.2`. Full evidence table in
  [`nvidia-bringup-hardware.md`](nvidia-bringup-hardware.md) § Current
  host state. The final `DRM_IOCTL_NOUVEAU_GETPARAM`-reports-nouveau
  verification rolls into the N1.2/N1.3 enum probe.
- [ ] **N0.7** — **Live spike — the riskiest unknowns, before committing
  the ladder.** Partially resolved by the post-reboot boot facts:
  (a) **MOOT — GSP is up** (RM 570.144), the recommended config, so the
  no-GSP boot-clock path is not pursued (and is a hard prereq at the
  Ampere rung anyway); the new-uAPI compute path is the one GSP
  conformance-tests. (b) **SETTLED — masterless confirmed on HW.** The
  N1 probe (`programs/nvidia_device_enum.cyr`) ran on the TU116 and
  `DRM_IOCTL_NOUVEAU_VM_INIT` returned **0** on a plain `renderD128` fd
  (`VM_INIT rc: 0 → VA space initialized masterless`). Confirms the
  source finding that the whole `VM_INIT`→`CHANNEL_ALLOC`→`VM_BIND`→`EXEC`
  chain is `DRM_RENDER_ALLOW` (mainline `nouveau_ioctls[]`); full AMD
  compute-vs-KMS master parity. (c) **DONE — NVK compute dispatch
  captured** (headless over SSH) via an LD_PRELOAD ioctl interposer +
  minimal NVK compute app (harness in scratch; `run_capture.sh`
  reproducible). Yielded the **golden 256-byte QMD**, the dispatch
  pushbuffer, the NVIF `0xC5C0` class-create payloads, and the full
  masterless ioctl sequence. Decoded findings (QMD field offsets, the
  `SEND_PCAS_A`(0x02b4)/`SEND_SIGNALING_PCAS_B`(0x02bc) launch, the
  `CHANNEL_ALLOC fb=0/tt=0` correction, chid roles) recorded in
  [`nvidia-n4-capture-notes.md`](nvidia-n4-capture-notes.md). **N0.7
  fully closed.**
- [x] **N0.8** — Confirmed in `src/backend.cyr`: `BACKEND_KIND_NVIDIA=3`
  (`backend.cyr:80`), `kind` value at slot offset `+80` (`backend.cyr:31`),
  `BACKEND_SIZE=328` (`backend.cyr:277`) — no struct change. The
  `ShaderSourceKind` enum exists (`SHADER_SRC_GFX9=2`); a native-NVIDIA
  pre-compiled-SASS tag (e.g. `SHADER_SRC_SASS`) will be added when N5
  lands. CPU asserts to be written alongside the N1 probe tests
  (`BACKEND_KIND_NVIDIA == 3`; kind at `+80`). (NB: the struct's inline
  "176 bytes" header comment is stale — it predates the surface/mipmap/
  array-bind slot blocks; the live `BACKEND_SIZE` is 328.)

### N1 — Device enum / open the right device node

- [x] **N1.1** — `src/backend_nvidia_nouveau.cyr` landed (new module;
  the `backend_native_amdgpu.cyr` analogue). Reuses `native_ioc_encode` /
  `native_open_render_node` / `native_drm_version` verbatim; opens
  `/dev/dri/renderD128`; `DRM_IOCTL_VERSION` → driver name `"nouveau"`
  (HW-confirmed, version 1.4.2). Full verified ioctl/struct table in the
  module header. **CPU asserts** (`tests/tcyr/nvidia.tcyr`): GETPARAM +
  VM_INIT ioctl numbers re-derived from `native_ioc_encode`; struct sizes;
  `native_nv_is_nouveau` driver-name check (incl. null-safety). 0 lint /
  fmt / vet warnings.
- [x] **N1.2** — `DRM_IOCTL_NOUVEAU_GETPARAM` wired (`native_nv_getparam`).
  **HW-confirmed on TU116:** `CHIPSET_ID=11` → **360 = `0x168` = NV168/TU116**
  (the earlier "NV162" was wrong; 0x162 is TU102), `PCI_VENDOR=3` →
  `0x10DE`, `PCI_DEVICE=4` → `0x21C4`. **CPU assert:** getparam struct
  shape + param constants + the TU116 identity + boot0→chipset decode.
  Module wired into `src/lib.cyr` after the AMD native block. (No
  `cyrius.cyml` change — `backend_nvidia_nouveau.cyr` is a `src/` include,
  not an external dep; the punchlist's "and `cyrius.cyml`" only applies if
  a future NVIDIA module pulls a new dep, which N1 does not.)
- [x] **N1.3** — `programs/nvidia_device_enum.cyr` + `make test-nvidia-enum`
  landed and **passing on HW**. Prints driver name/version, chipset, PCI
  ids, and runs the N0.7(b) masterless `VM_INIT` probe. Exit 0; mapped
  failure-class exit codes (1=open, 2=version, 3=not-nouveau, 4=getparam,
  5=master-required) for unattended runs.
- [ ] **N1.4** — (fallback path, only if Fork 1 → nvidia.ko) document the
  `/dev/nvidiactl`+`/dev/nvidia0` open + `NV_ESC_RM_ALLOC(NV01_ROOT)` +
  `NV2080_CTRL_GPU_GET_INFO` enum in ADR 007; no code unless the fallback
  is taken.

### N2 — Memory BO roundtrip (alloc + CPU map + readback)

- [x] **N2.1** — `native_nv_gem_new` (`GEM_NEW`) landed in
  `backend_nvidia_nouveau.cyr` + handle/map_handle/size accessors. Domain
  set used: **`GART|MAPPABLE|COHERENT` (0x1C)** — `MAPPABLE` added over
  the punchlist's bare `GART|COHERENT` to guarantee a CPU `map_handle`;
  **HW-verified it works first try** (no EINVAL). **CPU asserts:**
  `drm_nouveau_gem_new`/embedded `gem_info` byte-exact offsets; all six
  domain flags + the HOST composite + disjointness.
- [x] **N2.2** — `native_nv_gem_info` (`GEM_INFO`) + map_handle accessor.
  Roundtrip uses `GEM_NEW`'s returned `map_handle` directly →
  `cyr_mmap(0, size, RW, MAP_SHARED, fd, map_handle)`; the
  mapped-pointer-IS-the-data shape carries over from AMD. **CPU assert:**
  map-offset plumbing (`map_handle` at +24). (Note: a pure CPU roundtrip
  needs **no** `VM_INIT`/`VM_BIND` — that's GPU VA, N3.)
- [x] **N2.3** — `native_nv_gem_close`: **reused the generic
  `native_gem_close`** (`DRM_IOCTL_GEM_CLOSE`, 0x40086409) from the AMD
  module verbatim — driver-agnostic, no new code. Already asserted in
  `native.tcyr`.
- [x] **N2.4** — `programs/nvidia_mem_roundtrip.cyr` +
  `make test-nvidia-mem-roundtrip` landed and **passing on HW**
  (`gem_new handle=1`, 4096 bytes byte-identical, exit 0). Mapped
  failure-class exit codes (1=open, 2=gem_new, 3=mmap, 4=mismatch).

### N3 — Channel + GPFIFO + VM + submit setup

- [x] **N3.1** — `native_nv_vm_init` (`VM_INIT` 0x10) — landed in N1 (the
  masterless probe), CPU-asserted, and re-exercised first in the N3 setup.
  Must be the **first ioctl on the fd** (kernel `-ENOSYS` otherwise).
  HW-proven (rc=0).
- [x] **N3.2** — `native_nv_channel_alloc` (`CHANNEL_ALLOC` 0x02) +
  `native_nv_channel_id` + `native_nv_channel_free`. Fields pinned from
  NVK: `fb_ctxdma_handle=~0`, `tt_ctxdma_handle=GR(0x01)` (compute runs on
  GR). **HW-proven** (chid=9 on the TU116). Kernel auto-allocates the
  GPFIFO/USERD/notifier and binds the channel to the uvmm — the UMD never
  touches USERD/doorbell (EXEC uAPI). **SCOPE FINDING:** the compute
  **class object `TURING_COMPUTE_A 0xC5C0`** is NOT created by
  CHANNEL_ALLOC; it needs a separate `NVIF` (0x07) or deprecated
  `GROBJ_ALLOC` (0x04) step before EXEC — **deferred to N4 + a DECISION
  (NVIF vs GROBJ).** Not needed for the N3.5 setup milestone. **CPU
  assert:** ioctl number + 88-byte layout + chid accessor.
- [x] **N3.3** — `native_nv_vm_bind_map` / `native_nv_vm_unbind`
  (`VM_BIND` 0x11). Synchronous (flags=0, sync counts 0 — kernel EINVALs
  otherwise); op_ptr → stack vm_bind_op. Safe VA base `0x10000` (64 KiB,
  below the 1<<39 kernel split). **HW-proven** (MAP+UNMAP @ va 0x10000).
  **CPU assert:** ioctl number + vm_bind_op offsets + alignment constants.
- [x] **N3.4** — `native_nv_sync_build` (drm_nouveau_sync, 16B); syncobj
  create/destroy/wait **reused verbatim** from `backend_native_amdgpu.cyr`
  (`native_syncobj_*`, driver-agnostic). HW-proven (create/destroy).
  **CPU assert:** sync struct shape (flags/handle/timeline_value).
- [x] **N3.5** — `programs/nvidia_channel_setup.cyr` +
  `make test-nvidia-channel-setup`, **passing on HW**: `VM_INIT` →
  `CHANNEL_ALLOC` → `GEM_NEW` → `VM_BIND` MAP/UNMAP → syncobj → clean
  teardown, exit 0. Mapped failure-class exit codes (1–7). (Multi-BO
  alloc/bind of the full prerequisite set lands with N4's dispatch.)

### N4 — Compute dispatch proof-of-life (0xDEADBEEF readback) — THE ARC GATE

- [x] **N4.1** — `src/backend_nvidia_push.cyr` landed: `native_nv_method_header`
  / `native_nv_method_inline` + `native_nv_push_dispatch`. Headers
  **byte-match the NVK capture** (`SEND_PCAS_A`→`0x200120AD`,
  `SEND_SIGNALING_PCAS_B`→`0x800320AF`). Includes the captured
  shared/local-memory-window methods (`0x02a0`=`0xfe000000`,
  `0x07b0`=`0xff000000`). **CPU asserts** pin every dword.
- [x] **N4.2** — `src/backend_nvidia_qmd.cyr` landed (QMDV02_**02** —
  Turing 0xC5C0 uses NVK's Volta branch; version stamp 2.2). `REGISTER_COUNT_V`
  (not legacy), `SASS_VERSION`=0, the SM-config/window fields all pinned via
  the qmd-bitfield-verify pass. **Byte-diffed against `GOLDEN_QMD.bin`:** every
  fixed field matches; only program addr / regcount / cbuf VA differ as
  intended. **CPU asserts** pin all fields.
  <!-- original N4.2 spec, retained for reference -->
  256-byte QMDV02_03 bit-pack builder — `PROGRAM_ADDRESS_LOWER/UPPER`,
  `CTA_THREAD_DIMENSION0..2`, `CTA_RASTER_WIDTH/HEIGHT/DEPTH`,
  `REGISTER_COUNT`, `SHARED_MEMORY_SIZE`, `SASS_VERSION` (SM75 numeric —
  confirm from NAK), `CONSTANT_BUFFER[0]` (output VA + size_shifted4 +
  VALID), `QMD_VERSION`/`QMD_MAJOR_VERSION`. Header comment lists every
  `MW(hi:lo)` bit range. **CPU assert:** QMD size == 256; each field's bit
  offset; **byte-diff against the N0.7 captured known-good QMD** (the PM4
  protocol — do not claim correct until the HW-capture diff matches).
- [x] **N4.3** — Dispatch staging in `native_nv_push_dispatch`
  (`backend_nvidia_push.cyr`) + `native_nv_object_new` (NVIF 0xC5C0). Stream:
  `SET_OBJECT(0xC5C0)` → shared/local memory windows → `SEND_PCAS_A` →
  `SEND_SIGNALING_PCAS_B`. **This minimal stream is SUFFICIENT** — HW-proven
  by the green gate. The 2026-06-27 belief that it needed NVK's fuller
  multi-class init (the `0x0298`/`0x1424`/`0x0244` invalidates + 0xC597
  housekeeping) was wrong: those NVK methods are texture-pool binds + cache
  hygiene irrelevant to a texture-free global-store kernel, and the actual
  blocker was the syncobj-timeout bug (§ N4 notes). A dedicated
  `backend_nvidia.cyr` + ≤6-arg slot wrapper lands with N4.7.
- [x] **N4.4** — `native_nv_exec_submit` (`EXEC` 0x12) landed: one
  `exec_push{va,va_len}` + a sig `drm_nouveau_sync` syncobj; waits via the
  reused `native_syncobj_wait`. **HW-runs** (EXEC returns 0; channel
  accepts the submit). **CPU asserts** pin the exec/exec_push layout.
- [x] **N4.5** — The captured SASS uses **`STG.E.SYS`** (system-scope store)
  — the coherency encoding the gate needs (CPU-asserted). The
  nouveau EXEC out-syncobj fence provides the release/visibility ordering;
  HW-proven (readback correct after the fence signals).
- [x] **N4.6** — `programs/nvidia_compute_store.cyr` + `make
  test-nvidia-compute-store` — **THE ARC GATE — GREEN (2026-06-28).** Runs
  `VM_INIT → CHANNEL_ALLOC → NVIF 0xC5C0 → GEM_NEW/VM_BIND → EXEC → fence
  wait → readback`, reading `0xDEADBEEF` twice on the same channel (exit 0,
  stable). Two real bugs fixed end-to-end: the memory-windows extraction bug
  (2026-06-27) and the relative-vs-absolute `native_syncobj_wait` deadline
  bug (2026-06-28, the true blocker). Full diagnosis in
  [`nvidia-n4-capture-notes.md`](nvidia-n4-capture-notes.md) § GATE GREEN.
- [x] **N4.7** — **DONE + HW-proven via the public API.** `src/backend_nvidia.cyr`
  landed: `gpu_context_new_native_nvidia()` (persistent channel + NVIF 0xC5C0 +
  QMD/pushbuf/param BOs + a bump VA allocator) + `backend_nvidia_new()` filling
  the **v0 block** (0..80: ctx_create_from_preinit[stub]/release,
  buffer_create/write/read/release, shader_module_create/release,
  compute_dispatch, device_wait_idle) + kind `+80`. Added `SHADER_SRC_SASS=3`
  + the NVIDIA shader-kind branch in `gpu_shader_module_create`.
  `programs/nvidia_compute_api.cyr` + `make test-nvidia-compute-api` reads back
  `0xDEADBEEF` through the **public** gpu_buffer/shader/compute API (not raw
  ioctls). **CPU asserts** (`nvidia.tcyr`, now 145): v0 slots non-zero, kind ==
  NVIDIA, `backend_is_complete` honest (incomplete past v0 by design), field-map
  offsets pinned. **DEFERRED to the v4.0 cut** (paired with adding the NVIDIA
  modules to `[lib].modules`): the compile-time `MABDA_BACKEND_KIND==NVIDIA`
  branch in `gpu_context_from_preinit` — a *bundled* `gpu_context_from_preinit`
  can't call `gpu_context_new_native_nvidia()` until that fn is in the dist
  bundle. Until the cut, opt in via the explicit entry point (the AMD pattern).

### N5 — Shader story (hand / precompiled SM75 SASS)

> Mutually gating with N4 — proof-of-life needs a valid QMD **and** valid
> SASS together. Sequence N4 + N5 as one push.

- [x] **N5.1** — `src/backend_nvidia_sass.cyr` landed via ptxas-capture
  (CUDA 13.3 `nvcc -arch=sm_75 -cubin`; `.text.store_deadbeef` = 128 bytes
  / 8 instructions extracted). `native_nv_sass_store_deadbeef(dst)` writes
  the program byte-identical to the cubin section. SASS:
  `MOV R0,0xdeadbeef` / `ULDC.64 UR4,c[0x0][0x160]` / `STG.E.SYS [UR4],R0`
  / `EXIT`. Wired into `src/lib.cyr` (SASS slot, after the nouveau ioctl
  module). **CPU asserts:** byte/dword counts + key dwords pinned. lint/
  fmt clean.
- [x] **N5.2** — `scripts/disasm-sass.sh` — one-directional oracle
  (`nvdisasm -b SM75`, the SASS analog of `disasm-shaders.sh`). The
  embedded dwords round-trip to the exact expected SASS (verified
  2026-06-27). Dwords kept in sync with the module (same hand-maintained
  list pattern as the AMD oracle).
- [~] **N5.3** — Launch-ABI metadata extracted (`cuobjdump
  --dump-resource-usage`): **`REG:4, SHARED:0`, kernel param (output ptr)
  at `c[0x0][0x160]`, size 8**. Pinned as `NV_SASS_STORE_DEADBEEF_*`
  constants in the module. **Feeds the N4.2 QMD** (REGISTER_COUNT=4,
  SHARED_MEMORY_SIZE=0) and the constant-bank/param reflection — closes
  when the QMD consumes them at N4.
- [ ] **N5.4** — In parallel (sovereignty confidence-builder): hand-encode
  a 2–3 instruction `MOV` / `STG.E.SYS` / `EXIT`, with the correct stall
  count + write-barrier before `EXIT`, verified against `nvdisasm`. Proves
  the team owns the encoding + the scoreboard model on a trivial kernel.
- [ ] **N5.5** — SM75 register-method/value constants (compute class
  method offsets + QMD field values) sourced from `clc5c0.h` /
  `clc5c0qmd.h` / NAK, with citation comments. **CPU assert:** constants
  pinned. Watch global-init order.

### N6 — Textures

- [x] **N6.1** — `_backend_nvidia_texture_create_2d_rgba8` landed
  (`backend_nvidia.cyr`) — a host-visible **linear** RGBA8 image BO (row
  pitch = width*4, no tiling) VM_BIND'd at a fresh VA via the bump
  allocator; dims capped (`validate_dimensions`) so width*height*4 can't
  overflow; v0 surfaces fit one 64 KiB BO (<=128x128). **CPU assert:**
  NV_TEX field map + struct size + RGBA8 tag (`nvidia.tcyr`).
- [x] **N6.2a** — **Capture + decode an NVK texture-sampling dispatch —
  DONE.** Harness landed (`tools/nvidia-capture/probe_tex.comp` +
  `vk_compute_tex.c`; NVK reads back the sampled texel, PASS). Golden TIC +
  TSC descriptors, the sampling QMD (same shape as the store QMD), and the
  sampling SASS all decoded in
  [`nvidia-n6-capture-notes.md`](nvidia-n6-capture-notes.md). **Key finding:**
  the `TEX` instruction encodes the **TIC/TSC index as immediates** (the
  *bound* model, not bindless) and the dispatch is byte-shape-identical to the
  N4 store dispatch — so sampling needs only populated+bound TIC/TSC pools + a
  TEX shader, no new dispatch methods.
- [x] **N6.2b** — **DONE.** `src/backend_nvidia_tex.cyr`:
  `native_nv_tic_build_2d_rgba8` (PITCH-layout linear RGBA8 2D, arbitrary
  W/H/VA) + `native_nv_tsc_build_nearest_clamp`, **byte-diffed against the
  N6.2a golden** + the empirical multi-size capture. Field layout from
  envytools `gm200_texture.xml`/`g80_texture.xml` (Mesa NIL is now Rust, not
  C). **Key correction:** mabda emits `HEADER_VERSION=PITCH(2)`, NOT the
  golden's BLOCKLINEAR — linear data would gob-swizzle otherwise. **CPU
  asserts** pin every dword (`nvidia.tcyr` 176; full decode in the N6 notes).
- [x] **N6.2c-i** — **bound-texture sampling SASS — DONE.**
  `native_nv_sass_sample_tex` (`backend_nvidia_sass.cyr`): ptxas capture of
  `store_tex.cu` (`tex2D` → pack → store), 256 B / 16 instrs. Bound TEX
  (TIC 0 / TSC 0x58 immediates), output ptr @ `c[0x0][0x168]`, REG:10; the
  captured `STG.E.SYS` patched to `.STRONG.GPU` (`.SYS` hangs on GART — the
  store-kernel finding), `nvdisasm`-verified. **CPU asserts** pin the bytes +
  ABI (`nvidia.tcyr` 186).
- [x] **N6.2c-ii** — the sampling **dispatch** + `nvidia_texture_sample_e2e.cyr`
  — **DONE + HW-proven.** `native_nv_push_dispatch_sample`
  (`backend_nvidia_push.cyr`) binds the TIC/TSC pools
  (`SET_TEX_HEADER_POOL` 0x1574 / `SET_TEX_SAMPLER_POOL` 0x155C, A/B/C =
  upper/lower/MAXIMUM_INDEX) + the `NO_WFI` texture-header (0x0244) / sampler
  (0x1424) / SKED (0x0298) cache invalidates before `SEND_PCAS`; offsets
  verified vs clc5c0.h + NVK. **A UINT TIC** (`native_nv_tic_build_2d_rgba8_uint`,
  dw0 `0x58D49208`) — the sampling SASS reads raw integer texels (TEX → PRMT,
  no denormalize), so a UNORM header would pack float garbage. `make
  test-nvidia-texture-sample-e2e` reads back **two distinct 1x1 texels**
  (0x44332211, then 0x8899AABB from a repointed TIC) — the result provably
  tracks the bound texture. **CPU asserts** pin the UINT dw0 + every pushbuffer
  method (`nvidia.tcyr` 216). Closes N6.4. Decode in
  [`nvidia-n6-capture-notes.md`](nvidia-n6-capture-notes.md).
- [x] **N6.3** — v1 texture slots (88..120) filled on the NVIDIA backend
  (create_2d_rgba8 / write / read / release) — write/read are bounds-checked
  memcpy on the coherent host mapping. Wired in `backend_nvidia_new`; the
  texture-range `backend_is_complete` walk passes for NVIDIA (render range
  still zero, so the backend stays honestly incomplete). **CPU asserts** in
  `nvidia.tcyr` (slots non-zero + field map).
- [x] **N6.4** — **DONE + HW-proven, both halves.** (a) CPU lifecycle:
  `programs/nvidia_texture_e2e.cyr` + `make test-nvidia-texture-e2e` create a
  16x16 RGBA8 texture, write a pattern, read it back **byte-identical**,
  through the PUBLIC `gpu_texture_*` API. (b) GPU-sampled-texel:
  `programs/nvidia_texture_sample_e2e.cyr` + `make test-nvidia-texture-sample-e2e`
  (N6.2c-ii) — a bound-texture TEX compute dispatch samples a texel via
  TIC/TSC pools and reads it back. **N6 (textures) is complete.**

### N7 — Render pipeline + render pass

- [x] **N7.1** — **DONE + HW-proven.** `_backend_nvidia_render_target_create_2d_rgba8`
  / `_release` (`backend_nvidia.cyr`) fill the v2 render slots 120/128: a
  host-visible LINEAR RGBA8 color surface (one 64 KiB BO, VM_BIND'd at a fresh
  bump VA, ≤128x128), the `NV_RT` struct (40 B — the AMD `NATIVE_RT` shape).
  `programs/nvidia_render_target.cyr` + `make test-nvidia-render-target`
  allocates **two live RTs** (distinct VAs, RT#2 = RT#1 + 0x10000), checks
  geometry, and proves the mapping backs memory (CPU sentinel write/read across
  the span) — all through the PUBLIC `gpu_render_target_*` API. **CPU asserts**
  (`nvidia.tcyr` `test_nv_backend_v2_render_slots`, 228 total): slots 120/128
  wired, 136..168 honestly still zero, RT field map. The GPU draw-into-it is
  N7.2-N7.4.
- [x] **N7.2a** — **Capture + decode an NVK triangle draw — DONE.** Harness
  `tools/nvidia-capture/{vk_render.c, probe_render.vert, probe_render.frag}`
  captured a known-good NVK `0xC597` (TURING_A) draw on the TU116 (center
  pixel `0xFF996633`, PASS); the 141-method draw stream was decoded + **4-group
  adversarially verified** (clc597.h / NVK / nvc0 gallium / envytools / NAK)
  into a complete mabda render design in
  [`nvidia-n7-capture-notes.md`](nvidia-n7-capture-notes.md). KEY: NVK draws via
  MME macros mabda can't use → the **direct** draw is `SET_DRAW_CONTROL_A/B`
  (0x0260/64) + `DRAW_VERTEX_ARRAY_BEGIN_END_A/B` (0x0270/74) with a **must-emit**
  `SET_PRIMITIVE_TOPOLOGY_CONTROL=1` (0x1948); mabda renders to a **LINEAR**
  color target (`SET_COLOR_TARGET_MEMORY=LAYOUT_PITCH 0x1000`, WIDTH=byte-pitch,
  128B-aligned) and CPU-reads it with **no CE blit**; graphics shaders need a
  **128-byte SPH** before the SASS (`PROGRAM_ADDRESS` points at it, SASS at
  +0x80). Two open HW risks flagged for N7.4: an **L2/ROP→memory flush** before
  the CPU read, and the **vertex-distributor** inactive-attribute markers.
- [x] **N7.2** — **DONE (CPU).** `native_nv_push_draw` (`backend_nvidia_push.cyr`)
  builds the full clc597 (TURING_A `0xC597`) draw pushbuffer from the verified
  N7.2a design — SET_OBJECT(0xC597) on subc0 → surface clip + **LINEAR** color
  target (byte pitch, MEMORY=LAYOUT_PITCH) + CT select/write → viewport/scissor
  (scale = w/2,h/2 via `native_int_to_f32_bits`) → raster gate + cull + disables
  → VS/FS bind (programs point at the SPH) → must-emit topology-control →
  fused `SET_DRAW_CONTROL_A/B` + `DRAW_VERTEX_ARRAY_BEGIN_END_A/B`. Takes a
  56-byte `NV_DRAW_PARAMS` struct; 288-byte / 72-dword stream. **CPU asserts**
  pin every load-bearing method/arg (`nvidia.tcyr` `test_nv_push_draw`, 257
  total). The `0xC597` NVIF-create wires into the context at N7.4 (HW).
- [x] **N7.3** — **DONE.** `native_nv_sass_render_vs` / `native_nv_sass_render_fs`
  (`backend_nvidia_sass.cyr`): the two graphics programs as **128-byte SPH (v4)
  + SM75 SASS**, sliced verbatim from the N7.2a capture (NVK compiling
  `probe_render.{vert,frag}`) and re-emitted from the raw bytes. FS (272 B) =
  UMOV 4 color constants → MOV R0-3 → EXIT (`vec4(0.2,0.4,0.6,1.0)`); VS (352 B)
  = ALD `a[0x2fc]` (VertexID) → fullscreen-triangle pos → AST.128 `a[0x70]`
  (gl_Position) → EXIT. **Both SELF-CONTAINED** (no cbuf/UBO/descriptor reads —
  the VS's only input is the hardware-supplied VertexID), so the native render
  path needs zero constant-buffer setup. `nvdisasm`-verified (round-trip);
  **CPU asserts** pin the SPH headers + key SASS instrs (`nvidia.tcyr`
  `test_nv_sass_render`, 271 total). The inactive-attribute markers are a draw
  concern handled at N7.4 (the VertexID sysval needs no vertex-buffer fetch).
- [x] **N7.4** — **THE RENDER GATE — GREEN (HW-proven).**
  `programs/nvidia_render_e2e.cyr` + `make test-nvidia-render-e2e`: creates the
  Turing 3D class (`0xC597`), binds a **linear** RGBA8 color target, runs a
  vertex-less fullscreen-triangle draw (N7.3 VS/FS SPH+SASS via N7.2
  `native_nv_push_draw`), EXEC + syncobj, and the CPU reads back the rendered
  solid color **`0xFF996633`** — the render analogue of the N4 compute gate, a
  pure-Cyrius clc597 draw, no libdrm/GFX/CUDA. **Both flagged HW risks turned
  out NOT to apply to mabda's path** (first-try green): no explicit L2 flush
  (the syncobj + a HOST-coherent BO suffices, exactly like the N4 compute STG),
  and no vertex-distributor inactive-attribute markers (the VertexID-only VS
  draws fine without them on a fresh channel). No param bank either (the shaders
  are self-contained). All 6 NVIDIA HW gates green.
- [x] **N7.5** — **DONE + HW-proven. N7 (render) COMPLETE.** The v2 render
  slots 136..168 (`render_pipeline_create/release`, `render_pass_begin/draw/end`)
  are filled on the NVIDIA backend (`backend_nvidia.cyr`): an `NV_PIPE` struct
  (VS/FS program VAs + regcounts) + an `NV_PASS` struct (ctx/RT/clear); `pass_draw`
  assembles the draw params, builds `native_nv_push_draw` (now `vertex_count`-
  parameterized), and EXECs on the channel like compute_dispatch.
  `gpu_context_new_native_nvidia` now NVIF-creates `0xC597` alongside `0xC5C0`.
  `programs/nvidia_render_api.cyr` + `make test-nvidia-render-api` renders the
  triangle through the PUBLIC `gpu_render_*` API and reads back `0xFF996633`.
  **CPU asserts** (`nvidia.tcyr` 283): the whole v2 render range wired +
  NV_PIPE/NV_PASS field maps; `backend_is_complete` still 0 (no v3 surface/
  mipmap — honest). All 7 NVIDIA HW gates green (the `0xC597` context-create
  change is regression-clean).

### N8 — KMS surface / present

- [x] **N8.1** — **DONE + HW-PROVEN.** `programs/nvidia_kms_summary.cyr` +
  `make test-nvidia-kms-summary` scans `/dev/dri/card0..9` for the **nouveau**
  card (driver-name verified via `DRM_IOCTL_VERSION` + `native_nv_is_nouveau`,
  not hardcoded) and **reuses the generic-DRM KMS helpers** from
  `backend_native_kms.cyr` (`native_kms_init`/`native_kms_summary`/
  `native_drm_mode_get_connector`/mode enum) to walk the topology. **Ran clean
  on the TU116 from a desktop session — the AMD generic helpers walked nouveau
  with ZERO driver-specific chokes**, confirming the present path needs **no
  `backend_nvidia_kms.cyr`** (KMS is driver-agnostic). Topology: 4 connectors /
  11 encoders / 4 CRTCs; **HDMI-A-1 (conn 49) connected → encoder 50 → CRTC 67,
  preferred mode 2560x1440@59Hz** (41 modes); the 3 DP connectors disconnected;
  all encoders possible-CRTC mask 0x0F; framebuffer_extent ..16384x16384. **CPU
  asserts** (`nvidia.tcyr` `test_nv_kms_topology_constants`, 288). The topology
  print needs card-node read perm (`root:video` + logind ACL; no DRM master) —
  the agent context lacks it (clean EACCES verdict), the user ran it.
- [x] **N8.2** — **DONE + HW-PROVEN.** `programs/nvidia_kms_scanout.cyr` +
  `make test-nvidia-kms-scanout`: allocates a **linear VRAM** XRGB8888 256x256
  BO on the render node (masterless `GEM_NEW`, no VM_INIT), PRIME-bridges it onto
  the KMS card fd (`native_kms_import_bo`), and `ADDFB2`s a scanout FB
  (`native_kms_add_fb_xrgb8888`). **Ran clean on the TU116**: VRAM bo handle 1 →
  PRIME card handle 1 → `ADDFB2 fb_id=124 PASS` (VRAM BO alloc verified even in
  the masterless agent context; the card-side PRIME+ADDFB2 from a desktop
  session, no DRM master). Confirms the **render→card PRIME bridge is in-driver
  on nouveau** (no foreign-buffer issue) and **linear VRAM is scanout-capable**.
  All generic-DRM but the card-node open. Refactor: the one nouveau-specific KMS
  primitive `native_nv_open_card_node` (driver-verified card0..9 scan) now lives
  in `backend_nvidia_nouveau.cyr` (shared by N8.1/N8.2). **CPU asserts**
  (`nvidia.tcyr`, 289): the scanout VRAM domain + the card-node driver gate.
  v0 forces linear (block-linear modifiers are a later bite).
- [x] **N8.3** — **DONE + HW-PROVEN — a pure-Cyrius nouveau modeset lit up a
  real monitor.** `programs/nvidia_kms_modeset.cyr` + `make
  test-nvidia-kms-modeset`: `SET_MASTER` (from a tty) → discovery (first
  connected connector → encoder/CRTC, with an `encoder_id == 0` fallback to the
  first CRTC for an unbound connector) → nouveau VRAM scanout BO filled red →
  PRIME → `ADDFB2` → `SETCRTC`. **Ran on the TU116: HDMI-A-1 / conn 49 / CRTC 67
  / 2560x1440 turned solid RED for 3s.** The foreign-buffer-scanout caveat is a
  proprietary-nvidia-drm issue; on nouveau (single driver, card0 + renderD128
  same GPU) the render→card PRIME bridge is in-driver. **Clean restore:** added
  `native_kms_get_crtc` (generic-DRM GETCRTC) — the program SAVES the console's
  CRTC config before the modeset and RE-BINDS it on teardown (re-SETCRTC the
  original FB+mode) instead of disabling, so the screen returns exactly as found
  (no black console on a bare tty). Needs DRM master → run from a tty.
- [x] **N8.4** — **DONE + HW-PROVEN — an animated present scanned out to a real
  monitor.** `programs/nvidia_present_e2e.cyr` + `make test-nvidia-present-e2e`:
  two nouveau VRAM scanout FBs (`_alloc_scanout_fb`: BO→PRIME→ADDFB2, mmap'd),
  modeset FB0, then a **double-buffered, vsync-locked PAGE_FLIP loop** — fill the
  back FB with a scrolling blue→red gradient, `native_kms_page_flip(...
  DRM_MODE_PAGE_FLIP_EVENT ...)`, block on the flip-complete vblank via
  `native_drm_read_event`, swap. **Ran on the TU116: ~2s of a scrolling
  blue→red gradient on HDMI-A-1, then the console restored** (the N8.3 GETCRTC
  save/restore). 120 frames, tear-free, paced to the panel's refresh. Generic-DRM
  PAGE_FLIP/read_event; nouveau-specific only the scanout BO + card-node open.
  Needs DRM master → run from a tty.
- [x] **N8.5** — **DONE + HW-PROVEN — an animated present scanned out to a real
  monitor through the PUBLIC `gpu_surface_*` API.** Wires present to the public
  surface API. The NVIDIA
  backend now fills the v3 surface slots (176..208) in `src/backend_nvidia.cyr`:
  `_backend_nvidia_surface_configure` (KMS discovery → GETCRTC save → two VRAM
  scanout FBs via `_nv_surface_alloc_fb` BO→PRIME→ADDFB2 → SETCRTC FB0),
  `_acquire` (returns the back FB; consumer reads `NV_SFB_MAPPED`), `_present`
  (PAGE_FLIP back FB + vblank block + toggle), `_release` (restore saved CRTC →
  free FBs → release state), registered in `backend_nvidia_new()`. NvSurface is
  232 B (two 40-B FB sub-structs + a 104-B saved-CRTC blob). These slots **don't
  reuse** `native_kms_modeset_first_connected` / `native_kms_alloc_fb` — those
  bake in the AMDGPU `native_bo_create_gtt` allocator; the nouveau path needs
  `native_nv_bo_create_dom` (VRAM), so configure hand-rolls the modeset from the
  generic discovery/PRIME/ADDFB2/SETCRTC helpers (same split N8.4 used).
  `programs/nvidia_surface_present_e2e.cyr` + `make
  test-nvidia-surface-present-e2e` drive it through
  `gpu_surface_configure_native_kiosk` / `_acquire` / `_present` / `_release`
  (the nouveau analogue of `programs/native_present_e2e.cyr`). **Ctx-layout fix
  shipped with it:** the NVIDIA compute ctx had `NV_CTX_PARAM_VA`/`ADDR` sitting
  on the reserved v3 surface-stash offsets +96/+104 (`wgpu_surface_handle` /
  `native_card_fd`), so the kiosk configure dispatcher's `card_fd` write would
  clobber the compute param-bank pointer — relocated to +112/+120. Builds clean,
  `nvidia.tcyr` 314 pass (added `test_nv_backend_v3_surface_slots` covering slot
  presence + NvSurface/NV_SFB field maps + the ctx relocation), full CPU sweep
  green, dist diff-clean. **RAN ON THE TU116 via `make
  test-nvidia-surface-present-e2e` from a tty: the same ~2s scrolling blue→red
  gradient as N8.4, then a clean console restore — but flowing entirely through
  `gpu_surface_configure_native_kiosk` → `_acquire` → `_present` → `_release`,
  the backend-agnostic API a compositor consumes.** This closes the v4.0 NVIDIA
  arc: **compute → textures → render → present**, all public-API-reachable on
  real NVIDIA silicon. **ALL 8 NVIDIA HW gates green.**

### N9 — SPIR-V → SASS in-tree compiler (its own v4.x sub-arc)

> Defer until N4–N5 land and the AMD compiler patterns are stable. This is
> realistically a **v4.1 / v4.x** sub-arc, not v4.0 ship — note it in the
> roadmap. mabda already shipped a SPIR-V→GFX9 compute compiler, so the
> in-tree-compiler precedent exists.

- [ ] **N9.1** — Reuse `spirv_parse.cyr` / `spirv_lower.cyr` / `mir.cyr`
  frontend and the isel/regalloc skeleton **unchanged** (frontend is
  vendor-agnostic).
- [ ] **N9.2** — New SM70/75 encoder module (mirror of `gfx9_encode.cyr`),
  byte-pinned against the `nvdisasm` round-trip oracle. Encoding tables
  ported algorithmically from Mesa NVK's NAK (`src/nouveau/compiler`) —
  **re-implementation, not vendored/linked** (license hygiene: vendoring
  NAK's Rust re-imports a Rust dep).
- [ ] **N9.3** — Generalize `gfx9_waitcnt.cyr` into a Turing
  software-scoreboard / control-bit allocator (~6 barriers, single-issue,
  per-instruction stall counts). The principal new algorithmic cost.
  Confirm the exact barrier count against NAK source + arXiv:1804.06826
  before designing the allocator.
- [ ] **N9.4** — End-to-end: hand-built IR for "store 0xDEADBEEF" →
  encoder → bytes that match the N5 hand/captured `store_deadbeef`
  byte-for-byte (CI check), then `gpu_shader_module_create` accepts
  SPIR-V on the NVIDIA path and lowers it.

---

## Tier 2 — Ship work

- [ ] **Bundle the NVIDIA modules into the dist (`[lib].modules`) + enable
  compile-time routing.** The v4.0 NVIDIA modules
  (`backend_nvidia_nouveau` / `_sass` / `_push` / `_qmd` / `backend_nvidia`)
  are deliberately **source-only** today — in `src/lib.cyr`'s include chain
  (so programs + CPU tests build) but **NOT** in `cyrius.cyml` `[lib].modules`
  (so `dist/mabda.cyr` still ships v3.4.4 without them). At the v4.0 cut, add
  them to `[lib].modules` in dependency order (after `backend_native_amdgpu`
  for `native_syncobj_*` / `native_open_render_node`; the integration module
  last) AND re-enable the deferred `MABDA_BACKEND_KIND==NVIDIA` branch in
  `gpu_context_from_preinit` (`src/context.cyr` — currently a comment). These
  two flips are a **pair**: a bundled `gpu_context_from_preinit` can't call
  `gpu_context_new_native_nvidia()` until that fn is in the bundle. Then
  `cyrius distlib` regen + verify the bundle compiles clean for a consumer
  (no unresolved NVIDIA symbol). Mirrors how the AMD modules joined the
  bundle at v3.0.
- [ ] **Bench harness — 4-axis matrix.** Widen `bench-history.csv` /
  `make bench-gpu` to vendor × {wgpu, native} × {pre-/post-5.6.x} ×
  bench; add NVIDIA rows; AMD's table is unaffected. **Perf-credibility
  caveat:** do **not** make perf claims off a boot-clock no-GSP nouveau
  config — GSP-RM reclocking must be on for representative Turing clocks.
- [ ] **Consumer regression.** soorat (smoke consumer) builds and runs
  under `BACKEND_KIND_NVIDIA` on NVIDIA hardware; the six-consumer sweep
  (soorat / rasa / ranga / bijli / aethersafta / kiran) still builds on
  wgpu (default unchanged); zero behavioural changes; AMD native + wgpu
  paths unregressed.
- [ ] **CI cell.** NVIDIA HW is a **developer/local gate** (CI runners
  lack the GPU, exactly like the AMD native gates). Add
  `make test-nvidia-compute-store` (and later `-texture` / `-render` /
  `-present`); document the HW gating. CPU suite (`make test`) stays the
  CI backstop.
- [ ] **Docs.** `CHANGELOG.md` `[4.0.0]` entry; roadmap v4.0 section
  pruned to shipped once it lands (+ note N9 as v4.x); **ADR 007** final
  (driver path + capture methodology + SASS doctrine); architecture
  overview FFI section gains the nouveau path; `CLAUDE.md` FFI
  Architecture + Quick Start + Dependency wiring updated (GSP firmware
  documented as a consumer-provided runtime dependency, the
  wgpu-native/libsystemd model). Audit index references the current
  `docs/audit/YYYY-MM-DD-audit.md`.
- [ ] **Version bump → 4.0.0.** `VERSION` file; `./scripts/version-check.sh`
  passes; `cyrius distlib` regenerates `dist/mabda.cyr` diff-clean;
  per-file `cyrius lint` (0 warnings, modules under 128 KiB);
  `./scripts/count-test-assertions.sh` total recorded.

---

## Open questions / DECISIONS NEEDED

> **Status (2026-06-27):** items **1–4 are DECIDED** and recorded in
> [ADR 007](../adr/007-nvidia-native-kernel-path.md) — driver path
> (nouveau, HW-proven masterless), capture methodology, SASS doctrine
> (ptxas-primary + hand-encode insurance; GPL licensing resolved), and
> GSP policy (amdgpu-microcode class). Items **5–6 remain open**
> (grid-launch method → decide at the N4 spike; N9 → deferred to v4.x).
> The text below is retained as the recommendation + tradeoff rationale
> behind those calls.

Strategic calls that should be made (and landed in ADR 007) **before**
deep work, each with the recommendation and the one-line tradeoff.

1. **Driver path — nouveau vs nvidia.ko.** *Recommend nouveau* (modern
   `VM_INIT`/`VM_BIND`/`EXEC`), nvidia.ko as documented fallback.
   *Tradeoff:* nouveau reuses mabda's DRM scaffolding and rides a frozen
   mainline UABI but needs unbinding nvidia + GSP firmware + an untested
   host swap; nvidia.ko is installed/performant now but is a huge,
   version-drifting, DRM-disjoint surface (perpetual struct re-validation).
2. **Bring-up host config & capture references.** *Recommend* a short
   nvidia.ko de-risk spike, then swap to nouveau; **primary references =
   tinygrad `ops_nv.py` + open-gpu-kernel-modules + NVK/NAK**, demoting
   `valgrind-mmt`+`demmt` to last-resort. *Tradeoff:* keeping nvidia.ko
   longer eases capturing a known-good submission stream, but every day
   on it risks accreting an nvidia.ko-shaped backend you then throw away.
3. **SASS build-time toolchain doctrine.** *Recommend* ptxas-cubin-capture
   for bring-up speed. *Tradeoff:* ptxas/nvdisasm are closed (build-time
   only — runtime stays pure-ioctl), making the build pipeline less
   sovereign than AMD's open-llvm-mc precedent; the open alternative
   (turingas/CuAssembler) is incomplete and slower to bring up.
4. **GSP firmware policy.** *Recommend* treating the GSP blob as a
   consumer-provided runtime dependency (wgpu-native/libsystemd model),
   confirmed redistributable for AGNOS. *Tradeoff:* it is a binary mabda
   cannot audit and becomes **mandatory at the Ampere rung** — "own the
   whole stack" now includes a firmware blob below the ioctl boundary.
5. **Grid-launch method — `SEND_PCAS` (QMD-in-BO) vs inline-QMD.**
   *Recommend* `SEND_PCAS` for AMD-model parity (command stream
   references a buffer by VA). *Tradeoff:* inline-QMD removes one BO + one
   `VM_BIND` and may de-risk the very first single-dispatch spike.
6. **N9 scope/timing.** *Recommend* deferring the in-tree SPIR-V→SASS
   compiler to v4.x. *Tradeoff:* shipping v4.0 on captured/hand SASS gets
   NVIDIA consumers a native path sooner but leaves shader *generation*
   non-sovereign until the compiler lands.

**Still-empirical unknowns (resolve in the N0.7 spike, not by debate):**
does no-GSP boot-clock compute dispatch succeed on TU116? are
`VM_INIT`/`VM_BIND`/`EXEC` `DRM_RENDER_ALLOW` (masterless)? what is the
exact SM75 `SASS_VERSION` numeric the QMD expects? what is the minimal
NVK channel-init preamble that still lets a trivial kernel run?

---

## Sequencing

Do the **riskiest, highest-uncertainty work first**, before committing to
the full ladder:

1. **N0 spike (N0.6 + N0.7) + the two upfront decisions (N0.3–N0.5).**
   Prove the host swap, answer the GSP / masterless questions, and
   capture a known-good QMD + pushbuffer. If nouveau Turing compute does
   not come up here, fall back per ADR 007 before writing backend code.
2. **N1 → N2 → N3 → N4/N5 together (the arc gate).** N4 and N5 are
   mutually gating — proof-of-life needs a correct QMD **and** correct
   SASS in the same push, with the N4.5 cache-coherency ordering right.
   **Do not start N6–N8 until `programs/nvidia_compute_store.cyr` reads
   back `0xDEADBEEF` twice on the same channel.** Treat a hang as a
   silent QMD/method-bit error and byte-diff against the N0.7 capture.
3. **Only after the gate is green:** N6 (texture) → N7 (render) → N8
   (present), reusing the generic-DRM KMS + PRIME + samvada/logind layer
   wholesale.
4. **N9 (compiler) and Tier 2 ship work** trail the HW-verified ladder.
   N9 is a v4.x sub-arc; v4.0 ships on captured/hand SASS.

The whole arc is structurally de-risked by the AMD precedent: the slot
table, ioctl idioms, syncobj/PRIME/KMS wrappers, master/logind story, and
program ladder all carry over. The genuinely new, genuinely risky pieces
are exactly four — the channel/GPFIFO submit engine, the 256-byte QMD,
SM75 SASS encoding, and the GSP-gated host bring-up — and the sequencing
above front-loads all four into the N0.7 spike + the N4/N5 gate.