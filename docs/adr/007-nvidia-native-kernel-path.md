# ADR 007: NVIDIA native backend — kernel path, shader toolchain, GSP policy

**Status:** Accepted (v4.0 design phase, branch `4.0-work`)
**Date:** 2026-06-27
**Accepted:** 2026-06-27 — maintainer ratified all four forks + the GSP policy. The two CUDA build-hygiene items (below) are follow-ups, not gates.
**Supersedes:** n/a
**Related:** ADR 006 (native Cyrius GPU backend — this ADR is the per-vendor NVIDIA expansion it anticipated), ADR 004 (C launcher FFI — unaffected; wgpu/AMD coexistence unchanged). Punch list: [`docs/development/4-0-punchlist.md`](../development/4-0-punchlist.md). Hardware ladder: [`docs/development/nvidia-bringup-hardware.md`](../development/nvidia-bringup-hardware.md).

## Context

ADR 006 committed mabda to a pure-Cyrius `native` backend, vendor by
vendor, and reserved `BACKEND_KIND_NVIDIA` for v4.0. This ADR records
the four strategic forks that gate the NVIDIA arc before deep coding,
per the punch list's "Hard truths" and Open Questions. Three were
flagged DECISION NEEDED; the bring-up host swap and the N1 probe have
since converted several from "researched" to "proven on silicon."

The arc adds a pure-ioctl NVIDIA path that fills the same `@internal`
`Backend` slot table the AMD native backend fills — **the public API
does not change** (ADR 005). The arc-gating milestone mirrors AMD's
Phase B.4: a compute shader dispatches, writes `0xDEADBEEF`, the CPU
reads it back, via a dispatch driven entirely from Cyrius
`syscall(SYS_IOCTL)` — zero C/Rust userspace libraries at runtime.

Bring-up hardware: GTX 1660 SUPER (TU116, SM75, 6 GB GDDR6), then
Ampere (RTX 3060). As of 2026-06-27 the host boots **nouveau** (it ran
proprietary nvidia.ko 610.43.02 the same morning): kernel 7.0.13,
**GSP-RM 570.144 active**, DRM nouveau **1.4.2** (the `VM_BIND` uAPI),
`renderD128`/`card0`.

## Decision

### Fork 1 — Kernel driver path: **nouveau** (modern `VM_INIT`/`VM_BIND`/`EXEC`), nvidia.ko documented fallback only

Target nouveau's modern uAPI (Linux ≥ 6.6, the path NVK uses), **not**
the legacy `GEM_PUSHBUF` reloc path and **not** nvidia.ko RM as the
shipping path. nouveau maps almost 1:1 onto mabda's amdgpu pattern:
same `_IOC` encoding (DRM type `'d'`=0x64), same render node, same
GEM-handle model, same DRM syncobj; `VM_BIND` ≈ AMD `GEM_VA`, `EXEC` ≈
AMD `CS`.

**Proven on silicon (N1 probe, `programs/nvidia_device_enum.cyr`, on
the TU116):** `DRM_IOCTL_VERSION` → `nouveau 1.4.2`; `GETPARAM` →
chipset `0x168` (NV168/TU116 — *not* NV162, which is TU102), PCI
`0x10DE:0x21C4`; and `DRM_IOCTL_NOUVEAU_VM_INIT` returned **0 on a
plain `renderD128` fd**. The full compute chain
`VM_INIT → CHANNEL_ALLOC → VM_BIND → EXEC` (+ `GEM_NEW`/`GEM_INFO`/
`GEM_CLOSE`/`GETPARAM`/`PRIME`) is `DRM_RENDER_ALLOW` in mainline
`nouveau_ioctls[]` — i.e. masterless on the render node, exactly the
AMD `GEM_CREATE`+`GEM_VA`+`CS` story. KMS (`SETCRTC`/`PAGE_FLIP`/
`ADDFB2`) stays `DRM_MASTER` on `/dev/dri/card0` — the same
compute-vs-KMS master split mabda already has on AMD.

Mandatory primitives the research recipe under-counted, budgeted:
`DRM_NOUVEAU_VM_INIT` (0x10) must run first, before any BO/channel
(else `-ENOSYS`); `DRM_NOUVEAU_CHANNEL_ALLOC` (0x02, legacy abi16) is
still required (the real analog of AMD's `CTX`). `VM_INIT` args use
NVK's verbatim Turing split: `kernel_managed_addr = 1<<39` (512 GiB),
`kernel_managed_size = 1<<39`; mabda's UMD VA allocator stays in
`[0, 512 GiB)`, 64 KiB-aligned, to ride big pages.

**nvidia.ko** stays a documented fallback only (NV magic `'F'`=0x46,
`/dev/nvidiactl`+`/dev/nvidia0`, deep `RmAlloc` tree, no stable UABI —
gvisor carries per-version struct variants). Pure-ioctl-drivable, but
DRM-disjoint and a perpetual re-validation tax. Keep for a short
de-risk spike if ever needed; not the architecture.

### Fork 2 — Submission model: GPFIFO + pushbuffer + QMD (structural port of the AMD path)

Three new Cyrius layers mirror the AMD ones, in load-bearing include
order after `backend_nvidia_nouveau.cyr`: (1) a method/pushbuffer
builder (`backend_nvidia_push.cyr`, the PM4 analogue), core primitive
`header = (opcode<<29)|(count<<16)|(subc<<13)|(mthd>>2)`; (2) a 256-byte
**QMDV02_03** builder (`backend_nvidia_qmd.cyr`, no AMD analog); (3) a
submit wrapper over `CHANNEL_ALLOC` + `VM_BIND` + `EXEC` +
`drm_nouveau_sync`, reusing mabda's syncobj-wait code. Launch via
`SET_OBJECT(TURING_COMPUTE_A=0xC5C0)` then `SEND_PCAS_A(qmd_va>>8)` +
`SEND_SIGNALING_PCAS_B(INVALIDATE|SCHEDULE)`. As with PM4, one wrong
QMD/method bit is a **silent hang, not an error** — byte-diff the
hand-built stream against an NVK capture before claiming correct.

The 6-arg fncall ceiling is a wgpu-path constraint; native fns using
`syscall(SYS_IOCTL)` may take 10–12 args. The binding constraint is the
backend **slot** signature (≤ 6 args) — fold QMD/GPFIFO state behind a
struct pointer.

### Fork 3 — Shader ISA: **ptxas-capture primary, hand-encode insurance track, NAK-port endgame**

Two-phase, mirroring the GFX9 arc, with the sub-options **combined**
not chosen-between:

- **Bring-up (v4.0 gate): ptxas-cubin-capture.** Compile the
  store-`0xDEADBEEF` kernel offline (`nvcc -arch=sm_75 -cubin`),
  extract SASS (`cuobjdump --dump-sass` / `nvdisasm -hex`), embed
  dwords in `src/backend_nvidia_sass.cyr`. CUDA tools are **build-time
  only**; the runtime stays pure-ioctl. This is the only mature
  *from-scratch* option (CuAssembler needs an existing cubin template;
  NAK has no standalone assembler/CLI; turingas is unmaintained since
  2022 with no built-in verification).
- **In parallel: hand-encode the 2–3 instruction kernel (N5.4),
  promoted to a real deliverable.** Cross-reference turingas
  `grammar.py` + NAK `sm70_encode.rs` as two independent encoders, pack
  the Volta/Turing control word (stall/yield/write-barrier/read-barrier/
  wait-mask per arXiv:1804.06826 + 1903.07486), verify via `nvdisasm`
  round-trip (`scripts/disasm-sass.sh`, one-directional oracle). Proves
  the team owns the encoding + software-scoreboard model and is a fully
  sovereign fallback.
- **v4.x endgame: in-tree SPIR-V→SASS via a NAK-ported encoder (N9).**
  Re-implement NAK's tables + scoreboard allocator algorithmically —
  **re-implementation, not vendoring** (vendoring NAK re-imports a Rust
  dep). Realistically v4.1/v4.x, not v4.0 ship.

**Sovereignty note (honest):** this inverts the GFX9 precedent. AMD had
a public ISA spec + open `llvm-mc` (assemble *and* disassemble) +
documented `s_waitcnt` hazards. NVIDIA has none — no ISA spec, no open
assembler, `nvdisasm` is disassemble-only, and Turing adds a mandatory
per-instruction software scoreboard with no GFX9 equivalent. Used
build-time-only to emit inert dwords, this does not put a userspace lib
in the runtime path, but it makes the *build pipeline* less sovereign
than AMD's — which is exactly why the hand-encode track is funded.

**Licensing (resolved — not a blocker):** shipping ptxas-compiled SASS
in GPL-3.0 mabda is clean. Compiler output follows the input, not the
tool (FSF output doctrine); mabda links nothing at runtime (SASS is
inert data, zero Attachment-A redistributables), so the EULA's
redistribution machinery never engages and GPL §7/§10 "further
restriction" never triggers — EULA terms bind whoever runs ptxas, not
mabda's downstream recipients. GPL-3.0 cleanliness ~HIGH. The two
residual items are **build hygiene, not license conflicts**, captured
as action items below. If they ever fail, the fallback is to ship the
`.cu`/`.ptx` under GPL-3.0 and run ptxas in the *consumer's* build,
embedding no NVIDIA-derived bytes in-tree.

### Fork 4 — Memory + present: host-visible-first, present deferred onto nouveau

Compute proof-of-life uses a **host-visible** output BO (nouveau
`GEM_NEW` domain `GART|COHERENT`, the `native_bo_create_gtt` analogue) —
no vidmem, no BAR1, no staging. Defer vidmem; when added, gate
CPU-mapping on actual BAR1 size (stock TU116 BAR1 = 256 MB) with a
host-staging BO + GPU DMA-copy, mirroring AMD. Present is deferred past
compute and then built on nouveau (a normal DRM/KMS driver) — the
generic-DRM KMS ioctls + PRIME bridge + samvada/logind master story all
carry over from `backend_native_kms.cyr`; differences are block-linear
FB modifiers (or force a linear scanout BO) and a nouveau GEM/PRIME
scanout BO. Do **not** attempt a GPU-buffer present on nvidia-drm
(foreign-buffer scanout blocked) — another reason present steers to
nouveau.

### GSP firmware policy — accept as the amdgpu-microcode class (consumer-provided runtime dependency)

GSP is the **same category** as the amdgpu microcode mabda's AMD path
already depends on (verified): both are `Licence: Redistributable` in
the same linux-firmware `WHENCE`, both ship from Debian
`non-free-firmware`, both are no-modify/no-RE, and **neither** carries
non-commercial or field-of-use restrictions. It is kernel-loaded
firmware from `/lib/firmware`, **not** a userspace C/Rust library, so it
does not violate the sovereignty rule. Treat it as a consumer-provided
runtime dependency (the wgpu-native/libsystemd model). **Load-bearing:**
GSP changes clocks and *which generations reach accel* — **not the
submission ABI**; the `VM_INIT`/`VM_BIND`/`EXEC`/`CHANNEL_ALLOC` uAPI +
pushbuffer + QMD bytes mabda emits are firmware-agnostic.

Asymmetry to honor: amdgpu's grant is an unconditional BSD-style binary
grant; GSP's redistribution comes from NVIDIA's EULA "Open Source
Exception" (permitted solely for OSI-licensed OSes, unmodified, ship the
license copy — all satisfied for AGNOS/Linux). The operative grant is
whatever `LICENCE.nvidia` ships beside the targeted blob — read that
exact file (revisions diverge; the trend is *more* permissive).

### Capture methodology

Primary references for byte-diffing hand-built streams:
**tinygrad `ops_nv.py` (nvidia.ko path only — NOT a nouveau citation),
NVIDIA/open-gpu-kernel-modules headers, and Mesa NVK/NAK source** (the
known-working raw-ioctl clients). NVK is the only one that corroborates
the *nouveau render-node masterless* path; tinygrad drives proprietary
nvidia.ko and proves only nvidia.ko sovereignty. Demote
`valgrind-mmt`+`demmt` to last-resort cross-check (Turing-incomplete,
Ampere-absent). Captures go to a scratch dir, never the repo.

## Why not these alternatives

- **nvidia.ko RM as the shipping path.** Rejected as primary: not a
  stable UABI (per-driver-version struct churn), DRM-disjoint (shares
  none of mabda's `_IOC`/syncobj/PRIME/KMS scaffolding), contradicts the
  auditable/stable ethos. Retained as a documented fallback.
- **Legacy nouveau `GEM_PUSHBUF` reloc path.** Rejected as primary
  (still `DRM_RENDER_ALLOW`, kept as the documented older-kernel
  fallback): the modern `VM_BIND`/`EXEC` uAPI is the AMD-parity shape
  and the one NVK + GSP conformance-test.
- **turingas / CuAssembler as the bring-up assembler.** Rejected:
  turingas is unmaintained + unverified; CuAssembler needs a ptxas
  cubin template (can't create a kernel from scratch). turingas stays a
  *reference* for the hand-encode track, not a dependency.
- **NAK invoked as a build-time assembler.** Rejected: no text
  frontend/CLI; would pull the whole Mesa+Rust+bindgen stack and need
  NIR input. NAK is the *port* target (N9), not an invocable tool.
- **libdrm / libnvidia / CUDA runtime at runtime.** Rejected — the
  whole point is zero C/Rust artifacts in the runtime ioctl path.

## Consequences

**Positive:** AGNOS sovereignty advances to a second native vendor; the
slot table, `_IOC`/syncobj/PRIME/KMS wrappers, master/logind story, and
program ladder all carry over from AMD; masterless headless compute is
HW-proven; the public API is unchanged.

**Negative:** a third maintenance surface (AMD native + NVIDIA native +
wgpu); the 256-byte QMD + SM75 SASS + GSP-gated bring-up are genuinely
new, genuinely silent-failure-prone work; the build pipeline gains a
(build-time-only) closed-toolchain step until the hand-encode/NAK-port
tracks mature; GSP adds a versioned binary blob below the ioctl
boundary that mabda cannot audit.

**Neutral:** GSP is optional on Turing (the box runs it; the compute
path is GSP-conformance-tested) and effectively required from Ampere
on; the no-GSP path is not pursued. N9 (compiler) is a v4.x sub-arc, not
v4.0 ship.

## Action items (build-hygiene — not blockers)

1. **[DONE 2026-06-27] Read the shipped `EULA.txt`** (`/opt/cuda/EULA.txt`,
   CUDA 13.3). Confirms the analyzed clauses: §1.2.1 anti-RE/disassemble is
   scoped to *"the SDK or copies of the SDK"* (not your compiled output);
   the open-source clause restricts subjecting *the SDK* to an OSS license
   (embedding your own SASS in GPL doesn't touch the SDK); §1.3.2 — *"You
   hold all rights, title and interest in and to your applications"* (NVIDIA
   reserves rights only to the SDK, §1.3.1). ptxas-capture is GPL-3.0-clean.
2. **[DONE 2026-06-27] `libdevice` non-contamination check** (harness:
   `store_deadbeef.cu` → `nvcc -arch=sm_75 -cubin` → `cuobjdump`). Result:
   **CLEAN** — exactly one function (`store_deadbeef`), no `__nv_`/libdevice/
   Attachment-A device-code symbols. Captured launch ABI for the N4.2 QMD /
   N5.3 extraction: **`REG:4, SHARED:0, CONSTANT[0]:360`, kernel param at
   `c[0x0][0x160]`**, store via **`STG.E.SYS`** (system-scope — the N4.5
   coherency encoding). The `.cu` + SASS dump get archived in-tree under
   GPL-3.0 when N5.1 lands (they live in the session scratchpad today).
3. **[pending — AGNOS deploy-time, not a mabda build step] Read the exact
   `LICENCE.nvidia`** shipped beside the targeted GSP blob for the AGNOS
   deploy. (mabda itself ships no firmware; this is the consumer's check.)

## Open questions

- **Grid-launch method — `SEND_PCAS` (QMD-in-BO) vs inline-QMD.**
  *Recommend `SEND_PCAS`* for AMD-model parity; inline-QMD removes one
  BO + one `VM_BIND` and may de-risk the very first single-dispatch
  spike. Decide at the N4 spike.
- **N9 (in-tree SPIR-V→SASS compiler) scope/timing** — deferred to
  v4.x; v4.0 ships on captured/hand SASS.

**Still-empirical (resolve in the N0.7(c) / N4–N5 live spike, not by
debate):** does the captured/hand-encoded `STG.E.SYS` actually execute
on TU116 silicon? does GSP-on perturb the channel/USERD/doorbell
preamble (validate against NVK `nvk_queue_init` in the GSP-on mode)?
exact SM75 `SASS_VERSION` numeric the QMD expects (`MW(1023:1016)`, from
NAK)? whether legacy non-GSP Ampere compute outright fails or is merely
unreliable (do not design around "impossible")?

## Follow-up

- N0.7(c): capture an NVK compute dispatch for a known-good QMD +
  pushbuffer to byte-diff in N4 (scratch dir).
- N2 → N3 → N4/N5: extend `backend_nvidia_nouveau.cyr` (GEM, channel,
  VM_BIND) and add `backend_nvidia_{push,qmd,sass}.cyr` +
  `backend_nvidia.cyr`; do not start N6–N8 until
  `programs/nvidia_compute_store.cyr` reads back `0xDEADBEEF` twice on
  one channel.
- Tier 2: bench 4-axis matrix (no perf claims off boot-clock no-GSP),
  consumer regression, `make test-nvidia-*` HW gates, docs (CLAUDE.md
  FFI + GSP dependency), version bump → 4.0.0.
