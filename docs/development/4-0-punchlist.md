# Mabda v4.0 — NVIDIA Native Backend Punch List

**Status:** Working document — not started. Tick items off as they land.
**Date opened:** 2026-06-27
**Branch:** `4.0-work`
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
  compute-vs-KMS master parity. (c) **still TODO** — capture an NVK
  compute dispatch for a **known-good QMD + pushbuffer** to byte-diff
  against in N4 (scratch dir, not the repo). Net: only (c)-capture
  remains open in N0.7; resolve it together with the N4/N5 gate.
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

- [ ] **N3.1** — `native_nv_vm_init` (`DRM_NOUVEAU_VM_INIT`, **0x10**) —
  **must run first**, before any channel/BO bind. Sets up the
  userspace-managed GPU VA space (kernel_managed region bookkeeping).
  **CPU assert:** `drm_nouveau_vm_init` struct shape; ioctl number.
- [ ] **N3.2** — `native_nv_channel_alloc` (`DRM_NOUVEAU_CHANNEL_ALLOC`,
  **0x02**, legacy abi16) — engine/class pick + USERD/notifier; returns
  the channel handle `drm_nouveau_exec.channel` references. The AMD `CTX`
  analogue. **CPU assert:** channel-alloc param struct byte layout; pin
  against NVK's `nouveau_ws_context`.
- [ ] **N3.3** — `native_nv_vm_bind` (`DRM_NOUVEAU_VM_BIND`, **0x11**,
  op `MAP`/`UNMAP`) — the `GEM_VA` analogue; reuse the per-ctx VA
  bump-cursor allocator pattern. Honor NVIDIA 64 KiB / big-page alignment
  rules. **CPU assert:** `drm_nouveau_vm_bind` / `vm_bind_op` offsets;
  VA-range isolation; alignment guard.
- [ ] **N3.4** — DRM syncobj create/destroy/wait — **reuse the generic
  DRM wrappers from `backend_native_amdgpu.cyr` verbatim** (driver-agnostic
  `DRM_IOCTL_SYNCOBJ_*`); `drm_nouveau_sync` carries them on `EXEC`.
  **CPU assert:** `drm_nouveau_sync` struct shape (`flags`/`handle`/
  `timeline_value`).
- [ ] **N3.5** — `programs/nvidia_channel_setup.cyr` (mirrors
  `native_submit_setup.cyr`): `VM_INIT` → `CHANNEL_ALLOC` → `GEM_NEW`
  every prerequisite BO (SASS program, QMD, const-buffer, output,
  pushbuffer, semaphore) → `VM_BIND` each → tear down clean. **EXIT:** all
  allocate/bind without error. HW-gated.

### N4 — Compute dispatch proof-of-life (0xDEADBEEF readback) — THE ARC GATE

- [ ] **N4.1** — `src/backend_nvidia_push.cyr` (new; the
  `backend_native_pm4.cyr` analogue): `native_nv_method_header(opcode,
  subc, mthd, count)` = `(opcode<<29)|(count<<16)|(subc<<13)|(mthd>>2)`;
  INLINE form packs a 12-bit immediate in bits 28:16. **CPU assert:**
  header bit-packing against envytools dma-pusher; all bring-up methods
  `< 0x1700` so `(mthd>>2)` fits 12 bits.
- [ ] **N4.2** — `src/backend_nvidia_qmd.cyr` (new; **no AMD analog**):
  256-byte QMDV02_03 bit-pack builder — `PROGRAM_ADDRESS_LOWER/UPPER`,
  `CTA_THREAD_DIMENSION0..2`, `CTA_RASTER_WIDTH/HEIGHT/DEPTH`,
  `REGISTER_COUNT`, `SHARED_MEMORY_SIZE`, `SASS_VERSION` (SM75 numeric —
  confirm from NAK), `CONSTANT_BUFFER[0]` (output VA + size_shifted4 +
  VALID), `QMD_VERSION`/`QMD_MAJOR_VERSION`. Header comment lists every
  `MW(hi:lo)` bit range. **CPU assert:** QMD size == 256; each field's bit
  offset; **byte-diff against the N0.7 captured known-good QMD** (the PM4
  protocol — do not claim correct until the HW-capture diff matches).
- [ ] **N4.3** — `src/backend_nvidia.cyr` (new; the `backend_native.cyr`
  analogue) `native_nv_compute_dispatch`: stage pushbuffer —
  `SET_OBJECT(0xC5C0)` on the compute subchannel; preamble
  (`SET_SHADER_LOCAL_MEMORY_A`, `SET_SHADER_SHARED_MEMORY_WINDOW_A`,
  `SET_SHADER_EXCEPTIONS`, `INVALIDATE_SHADER_CACHES_NO_WFI`);
  `SEND_PCAS_A(qmd_va>>8)`; `SEND_SIGNALING_PCAS_B(INVALIDATE|SCHEDULE)`;
  host `SEM_EXECUTE` release (`RELEASE`, `RELEASE_WFI`) to the semaphore
  VA. **CPU assert:** the staged pushbuffer dwords match a golden vector
  (every method header + arg pinned). Keep dispatch state behind a struct
  ptr so the slot signature stays ≤ 6 args.
- [ ] **N4.4** — `native_nv_exec_submit` (`DRM_NOUVEAU_EXEC`, **0x12**):
  one `drm_nouveau_exec_push{va=pushbuf_va, va_len=bytes}` + a sig
  `drm_nouveau_sync` syncobj; wait via the reused syncobj-wait wrapper.
  **CPU assert:** `drm_nouveau_exec` / `exec_push` byte layout; push-count
  / sig-count plumbing.
- [ ] **N4.5** — **Cache coherency (the hardest non-structural bug).**
  Ensure the GPU's write is CPU-visible after the syncobj fires: the SASS
  uses `STG.E.SYS` (+ membar as needed), and the QMD's L2-flush /
  `RELEASE0` (or host `SEM` release) ordering guarantees the CPU sees
  `0xDEADBEEF`, not stale data. Mirror AMD's glc/slc + `s_waitcnt`
  discipline. **CPU assert:** the SASS carries the system-scope store
  encoding; release-after-dispatch ordering is asserted in the staged
  stream.
- [ ] **N4.6** — `programs/nvidia_compute_store.cyr` — **THE ARC GATE**
  (mirrors `native_compute_store.cyr`): build the SASS BO, build QMD +
  pushbuffer, `EXEC`, wait, verify the output BO holds `0xDEADBEEF`, then
  prove a **2nd dispatch on the same channel**. **EXIT:** GPU wrote
  `0xDEADBEEF` and CPU read it back via a pure-Cyrius dispatch. Map exit
  codes to failure classes for unattended runs. HW-gated.
- [ ] **N4.7** — Wire `gpu_context_new_native_nvidia()` (the
  `gpu_context_new_native` analogue) + `backend_nvidia_new()` slot-filler
  for the **v0 block only** (offsets 0..80: ctx_create/release,
  buffer_create/write/read/release, shader_module_create/release,
  compute_dispatch, device_wait_idle) + kind `+80`. Add the
  `BACKEND_KIND_NVIDIA` branch to `gpu_context_from_preinit` routing.
  **CPU assert:** the v0 slots store non-zero fnptrs; `backend_is_complete`
  remains honest (v0-only until later phases fill 88..280).

### N5 — Shader story (hand / precompiled SM75 SASS)

> Mutually gating with N4 — proof-of-life needs a valid QMD **and** valid
> SASS together. Sequence N4 + N5 as one push.

- [ ] **N5.1** — Per the N0.5 doctrine call, produce the store-0xDEADBEEF
  SM75 SASS. If capture: `nvcc -arch=sm_75 -cubin` → `cuobjdump
  --dump-sass` / `nvdisasm -hex`, embed dwords in
  `src/backend_nvidia_sass.cyr` (the `backend_native_shaders.cyr`
  analogue). **CPU assert:** SASS byte count + every dword pinned.
- [ ] **N5.2** — `scripts/disasm-sass.sh` — one-directional verify
  oracle (encode → `nvdisasm` → diff), the SASS analog of
  `scripts/disasm-shaders.sh`. Hand-encoded bytes with no verify are a
  silent-corruption trap.
- [ ] **N5.3** — Extract launch-ABI metadata from the cubin ELF
  (register count, shared-mem size, constant-bank `c[0x0]` param layout)
  and feed it into the N4.2 QMD — they must agree exactly or the SASS
  reads garbage.
- [ ] **N5.4** — In parallel (sovereignty confidence-builder): hand-encode
  a 2–3 instruction `MOV` / `STG.E.SYS` / `EXIT`, with the correct stall
  count + write-barrier before `EXIT`, verified against `nvdisasm`. Proves
  the team owns the encoding + the scoreboard model on a trivial kernel.
- [ ] **N5.5** — SM75 register-method/value constants (compute class
  method offsets + QMD field values) sourced from `clc5c0.h` /
  `clc5c0qmd.h` / NAK, with citation comments. **CPU assert:** constants
  pinned. Watch global-init order.

### N6 — Textures

- [ ] **N6.1** — `native_nv_texture_create_2d_rgba8` — a linear (or
  block-linear) image BO; reuse the N2 allocator + a texture VA range.
  **CPU assert:** size formula, struct layout, VA-range isolation.
- [ ] **N6.2** — Turing TIC (Texture Image Control) + TSC (Texture
  Sampler Control) descriptor builders in a pool BO; `texfmt`→TIC format
  map (the GFX9 T#/S# analogue). **CPU assert:** TIC/TSC layout + format
  mapping.
- [ ] **N6.3** — Fill v1 texture slots (88..120) on the NVIDIA backend
  (create/write/read/release) — write/read are bounds-checked memcpy on
  the mapping for host-visible images.
- [ ] **N6.4** — `programs/nvidia_texture_e2e.cyr` (mirrors
  `native_texture_e2e.cyr`). **EXIT:** a sampled texel reads back correct.
  HW-gated.

### N7 — Render pipeline + render pass

- [ ] **N7.1** — `native_nv_rt_create_2d_rgba8` + release (reuse N2/N6
  BO allocator + RT VA range). **CPU assert:** struct layout + dims.
- [ ] **N7.2** — `TURING_A` 3D class (0xC597) method encoders (pipeline
  state + draw) in `backend_nvidia_push.cyr` (the render-PM4-composer
  analogue). **CPU assert:** method offsets + state values pinned against
  `clc597.h`.
- [ ] **N7.3** — SM75 VS + FS bytes (fullscreen-triangle VS, solid-color
  FS) in `backend_nvidia_sass.cyr`, verified via the N5.2 oracle.
- [ ] **N7.4** — Fill v2 render slots (120..176) on the NVIDIA backend;
  `programs/nvidia_render_e2e.cyr` (mirrors `native_render_e2e.cyr`).
  **EXIT:** rendered pixels correct on an RT BO. HW-gated.

### N8 — KMS surface / present

- [ ] **N8.1** — `src/backend_nvidia_kms.cyr` — confirm nouveau exposes
  standard DRM atomic KMS on `/dev/dri/card0` (was `card1` under nvidia.ko;
  nouveau is now the sole DRM driver and holds `card0` — re-probe at N8
  time rather than hardcoding); **reuse the generic-DRM
  KMS ioctls** (`GETRESOURCES`/`GETCONNECTOR`/`GETENCODER`/`ADDFB2`/
  `SETCRTC`/`PAGE_FLIP`) and the PRIME render→card bridge from
  `backend_native_kms.cyr`. Discovery → mode-pick → encoder → CRTC →
  AddFB2 → SETCRTC → PAGE_FLIP sequencing is identical. **CPU assert:**
  KMS struct shapes hold on the nouveau fd.
- [ ] **N8.2** — Scanout FB from a nouveau GEM/PRIME BO; handle
  block-linear format modifiers or force a **linear** scanout BO for v0.
  **CPU assert:** FB format/modifier plumbing.
- [ ] **N8.3** — DRM master via the existing direct-`SET_MASTER` (tty) or
  samvada/logind delegation — **unchanged**. Do **NOT** attempt a
  GPU-buffer present on nvidia-drm (foreign-buffer scanout blocked).
- [ ] **N8.4** — Fill v3 surface slots (176..208) on the NVIDIA backend;
  `programs/nvidia_present_e2e.cyr` (mirrors `native_present_e2e.cyr`).
  **EXIT:** an animated frame sequence scans out to a real display.
  HW + master-gated.

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