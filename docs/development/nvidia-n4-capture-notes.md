# NVIDIA N4 — NVK Compute Capture Findings (N0.7c)

Decoded reference for building mabda's native compute dispatch (N4), from
a **known-good NVK compute dispatch captured on the TU116** (2026-06-27).
The raw capture + harness live in the session scratch dir (per the
punchlist, capture bytes do **not** go in the repo); this file records the
**decoded understanding** N4 builds against. Re-capture anytime with the
harness (`nouveau_capture.c` LD_PRELOAD interposer + `vk_compute.c`
headless NVK dispatch + `run_capture.sh`).

Capture is **headless over SSH** — Vulkan/NVK compute needs only the render
node, no display/DRM-master.

## Masterless ioctl sequence (real NVK, renderD128)

```
VM_INIT (0x10)           once, first on the fd
NVIF (0x07) x many       device/object tree setup
CHANNEL_ALLOC (0x02)     NVK creates TWO channels: chid=10 COMPUTE, chid=9 COPY(CE)
NVIF (0x07) NEW          create class objects on the channel (see below)
GEM_NEW (0x80) x12       BOs: 9 in VRAM (domain 0x2), 3 in GART (domain 0x4)
VM_BIND (0x11) MAP       synchronous, one per BO, UMD VAs ~0x3ffd_xxxx_000
EXEC (0x12) x4           submits
```

### CHANNEL_ALLOC — correction to N3.2 / the research

NVK passes **`fb_ctxdma_handle = 0`, `tt_ctxdma_handle = 0`** (NOT `~0` /
`GR`). The modern path leaves the channel engine-agnostic at alloc time and
binds the compute engine via the NVIF class object. mabda's N3 uses
`fb=~0, tt=GR`, which **also works** (kernel accepted it, chid returned) —
but `fb=0, tt=0` is the NVK idiom. Returned: `chid`, `pushbuf_domains=0x6`
(VRAM|GART), a notifier handle, `nr_subchan=0` (empty on Turing, as
predicted).

### Compute class object — NVIF, confirmed

NVK creates the class object via `DRM_NOUVEAU_NVIF` (0x07) `NEW`, not
CHANNEL_ALLOC and not GROBJ_ALLOC. Two classes are set up on the compute
channel: **`0xC5C0` (TURING_COMPUTE_A)** and **`0xC597` (TURING_A` 3D)** —
NVK initializes both even for compute. **Decision (N4): use NVIF (0x07)** to
mirror NVK (rides the stable UABI — the reason we chose nouveau); the
deprecated `GROBJ_ALLOC (0x04)` 12-byte `{chid,handle,0xc5c0}` remains a
documented shortcut if NVIF packing proves fiddly.

## Compute launch — SEND_PCAS confirmed (Open Question 5 resolved)

The dispatch pushbuffer (small, on compute chid=10, subchannel **subc1** =
the `0xC5C0` object) launches via:

```
INCR  subc1 mthd 0x02b4  arg = qmd_va >> 8      # SEND_PCAS_A
IMMD  subc1 mthd 0x02bc  imm = 0x3              # SEND_SIGNALING_PCAS_B = INVALIDATE|SCHEDULE
```

So the QMD lives in a BO and is launched by address — the AMD-parity
`SEND_PCAS` model (not inline-QMD). `qmd_va>>8` packs into one method arg;
the QMD must be 256-byte aligned (>>8 ⇒ 256 granularity). Method header
format verified: `(op<<29)|(cnt<<16)|(subc<<13)|(mthd>>2)`, op 1=INCR
3=NONINCR 4=IMMD 5=INCR_ONCE — SET_OBJECT decoded correctly with this.

## QMDV02_03 layout (decoded from the golden 256-byte QMD)

Captured QMD was 256-byte-aligned at BO+0x800. Field offsets mabda's
`backend_nvidia_qmd.cyr` (N4.2) must populate (byte offsets into the QMD):

| off | dw | golden value | field |
|---|---|---|---|
| +0x10 | 4 | `0x00000040` | QMD header/version region |
| +0x30 | 12 | `0x1` | `CTA_RASTER_WIDTH` (grid X) |
| +0x34 | 13 | `0x1` | `CTA_RASTER_HEIGHT` (grid Y) |
| +0x38 | 14 | `0x1` | `CTA_RASTER_DEPTH` (grid Z) |
| +0x44 | 17 | `0x22240000` | regcount / barrier / shmem-packed region |
| +0x48 | 18 | `0x00010022` | `CTA_THREAD_DIMENSION0/1` (local size) |
| +0x4c | 19 | `0x00010001` | `CTA_THREAD_DIMENSION2` + flags |
| +0x50 | 20 | `0x00121803` | SASS_VERSION / shared-mem / regcount packed (confirm bit split vs NAK) |
| +0x80 | 32/33 | `0x3f:0xfdf5c000` | `CONSTANT_BUFFER[0]` addr = the `c[0x0]` param bank (output ptr at +0x160 — matches ptxas N5.3) |
| +0x88 | 34/35 | `0x3f:0xfdf7cfc0` | `CONSTANT_BUFFER[1]` addr |
| +0xc0 | 48/49 | `0x3f:0xfdf9c780` | `PROGRAM_ADDRESS` (the SASS program BO) |

(64-bit addresses are split low32 then high8-in-next-word, e.g.
`0x3ffdf5c000` = `lo 0xfdf5c000`, `hi 0x3f`.)

### What N4.2 wires from already-captured facts

- `PROGRAM_ADDRESS` → the VA where mabda VM_BINDs its captured SASS BO
  (`native_nv_sass_store_deadbeef`, N5.1).
- `REGISTER_COUNT = 4`, `SHARED_MEMORY_SIZE = 0` (N5.3, `cuobjdump
  --dump-resource-usage`) → the +0x44/+0x50 region.
- `CONSTANT_BUFFER[0]` = a small BO holding the kernel params; the **output
  buffer pointer goes at c[0x0]+0x160** (ptxas ABI, N5.3). mabda allocates
  this const BO, writes the output VA at +0x160, VM_BINDs it, points
  `CONSTANT_BUFFER[0]` at it.
- grid/thread dims = (1,1,1)/(1,1,1) for the proof-of-life.
- `SASS_VERSION` numeric still to confirm against NAK (the +0x50 packing) —
  the last genuinely-unknown QMD field.

## N4 build order (against this reference)

1. NVIF create `0xC5C0` on the channel (N3.2 follow-on).
2. `backend_nvidia_qmd.cyr` — 256-byte QMD builder; **byte-diff against
   `GOLDEN_QMD.bin`** before trusting it (the PM4 protocol).
3. `backend_nvidia_push.cyr` — method-header builder + the dispatch stream
   (`SET_OBJECT 0xC5C0` on subc1 → `SEND_PCAS_A` qmd_va>>8 → 
   `SEND_SIGNALING_PCAS_B` 0x3 → semaphore release).
4. `native_nv_exec_submit` (`EXEC` 0x12) + syncobj wait (N3.4 wrappers).
5. Cache coherency (N4.5): captured SASS already uses `STG.E.SYS`
   (system-scope) — confirm the QMD L2-flush / release ordering matches the
   golden.

The minimal preamble (N4.3) can be distilled from the 14532-byte NVK
queue-init stream (in scratch) — but most of it is NVK's 3D+compute combined
init; mabda needs only the compute-class subset (shader local/shared mem
sizing, exceptions, cache invalidate) before the first dispatch.

## N4 implementation status — 2026-06-27 (gate NOT yet green; precise state)

Everything is built and CPU-locked (nvidia.tcyr 120 asserts; QMD byte-matches
the golden on all fixed fields; method headers byte-match the capture):
`backend_nvidia_qmd.cyr`, `backend_nvidia_push.cyr`, `native_nv_object_new`
(NVIF 0xC5C0, byte-matched), `native_nv_exec_submit`, and
`programs/nvidia_compute_store.cyr`.

**HW-proven so far:** the gate runs `VM_INIT → CHANNEL_ALLOC → NVIF 0xC5C0 →
GEM_NEW + VM_BIND (×5) → EXEC` with no error, and **the SASS shader executes
end-to-end** — proven because, with the memory windows mis-set to 0, the
dispatch reached `STG.E.SYS` and faulted at the exact store target
(`fault_addr=0x400000=va_out`, Xid 13 "Invalid Address Space"). So program
fetch, QMD read, const-bank read (it returned the param pointer), and the
shader body all work.

**Bug found + fixed:** the generic-addressing memory windows. The compute
preamble methods (captured) are `SET_SHADER_SHARED_MEMORY_WINDOW` (mthd 0x02a0,
`[0, 0xfe000000]`) and `SET_SHADER_LOCAL_MEMORY_WINDOW` (mthd 0x07b0,
`[0, 0xff000000]`) — an earlier extraction bug had read the second dword as 0,
so mabda set the windows to 0, which made low VAs collide with the local
aperture (the fault above). Now set to the captured `0xfe000000`/`0xff000000`.

**Remaining blocker (the open nut):** a generic GLOBAL store **silently hangs**
(syncobj ETIME, no Xid, readback stays 0). A store whose target lands in the
local-memory aperture instead *faults* immediately (Xid 13) — that's how we
know the dispatch *runs* and the shader executes; only the global-store
*completion* is stuck.

**Eliminated on hardware (each tested, none fixed it):**
1. **Window classification** — found+fixed the windows=0 bug (→
   `0xfe000000`/`0xff000000`); changed local-collision faults to global, but
   the global store still hangs.
2. **VA height** — low (1–5 MB), moderate (256 MB), high (255 GiB) all hang.
3. **BO domain** — `GART|MAPPABLE|COHERENT` (0x1C) vs pure `GART` (0x4, NVK's):
   both hang. (Capture confirms NVK uses `tile_mode=0`, `tile_flags=0`,
   `bind_flags=0` — identical to mabda.)
4. **Page kind / VM_BIND flags** — identical to NVK (all 0).
5. **Store scope** — switched the SASS from `STG.E.SYS` (system) to
   `STG.E.STRONG.GPU` (GPU, via `nvcc st.global.cg`; one control-word dword:
   `…c10e904`→`…c114904`). NVK's NAK shader is also GPU-scope (opcode `7386` vs
   our `7986` — a register-vs-uniform-address delta, not scope). Still hangs.
   **(The GPU-scope store is kept — it's correct and matches NVK.)**

**Leading hypothesis (the remaining one): the channel/dispatch init is
insufficient.** A global write that neither faults nor completes is a
*replayable fault that never resolves* — which points at missing per-channel
setup (e.g. the fault buffer / the fuller compute init NVK does in its 14532-
byte queue-init `067` and the multi-class dispatch `082`). NVK's actual
dispatch (capture `082`) issues, in addition to `SET_OBJECT`/windows/`SEND_PCAS`,
several more methods right before the launch:
- compute subchannel (0xC5C0): `0x0298`=0, `0x1424`=0, `0x0244`=0 (likely a
  cache-invalidate / barrier / exceptions trio)
- 3D subchannel (0xC597, subc0): `0x0100`=0, `0x1424`=`[0,0]`,
  `0x0da4`=0x1000, `0x3890`=`[0,1]` (graphics-class housekeeping/barrier)

The 3D-class methods imply NVK's compute channel also has `TURING_A (0xC597)`
SET_OBJECT'd. So N4.3 likely needs: SET_OBJECT the 3D class too, replay the
extra compute methods, and the per-dispatch barrier — i.e. a fuller channel
init than the current minimal stream. Recommended next move: replay NVK's
`082` dispatch sequence verbatim (adapting only the QMD VA), then trim to the
minimal working subset. Raw `082`/`067` dumps + the decode are in the session
scratch (`run_capture.sh` re-captures).
