# radv_capture — Layer-2 PM4 reference verifier

**Purpose**: emit a reference AMDGPU IB byte-stream against which
mabda's pure-Cyrius PM4 composer (`src/backend_native_pm4.cyr` →
`native_pm4_build_compute_store_deadbeef`) can be byte-diff'd.

**Phase**: 2 (byte-diff reduction landed at v3 rc.2). Phase 1
proved the toolchain works + dispatch runs + readback returns
`0xDEADBEEF` + RADV emits the IB dump. Phase 2 adds:

  - `programs/native_pm4_dump.cyr` — runs the mabda composer
    without submitting and prints the dword stream on stdout
    (CI-safe; no GPU access).
  - `extract_dispatch.sh` — normalizes a PM4 dump (mabda format
    OR RADV `--dump=ibs` format) into one-packet-per-line form,
    decoding compute-state SET_SH_REGs, DISPATCH_DIRECT, and
    ACQUIRE_MEM into a stable shape suitable for `diff`.
  - `make compare` — runs both extractors and produces the
    side-by-side diff.

**Hardware**: requires AMD GPU with a Mesa RADV driver loaded
(`vulkan-radeon` package on Arch). Vendor-specific `RADV_DEBUG=ibs`
output is the load-bearing capability — other ICDs (NVIDIA, AMDVLK)
won't emit the IB dump.

## Build

```sh
make             # builds radv_capture binary + compiles shader.spv
```

Requires `vulkan-headers`, `glslang` (`glslangValidator`), and a
linker that finds `libvulkan.so` (the loader, in `vulkan-icd-loader`).

## Run

```sh
make run         # bare dispatch + readback; exits 0 if slot[0] == 0xDEADBEEF
make dump        # same, but with RADV_DEBUG=ibs → stderr → radv.ib.txt
```

`make dump` produces:
- `radv.stdout.txt` — the readback verification line
- `radv.ib.txt` — RADV's full IB dump (every PM4 packet on every
  command-buffer the dispatch produced, plus init / state / barrier
  packets RADV emits around the user dispatch)

## Compare workflow (Phase 2)

```sh
# 1. capture RADV reference (writes radv.ib.txt — needs HW + RADV)
make dump

# 2. side-by-side diff. Builds programs/native_pm4_dump as a
#    side-effect; runs the extractor over both dumps; emits the
#    diff focused on compute-state SET_SH_REG / ACQUIRE_MEM /
#    DISPATCH_DIRECT packets. CI-safe except for step 1.
make compare
```

Skip step 1 if you just want to see what mabda emits:

```sh
make diff-tail   # only mabda — no RADV / no GPU needed
```

The full RADV `--dump=ibs` output is ~600-800 dwords (init / state /
barrier / cleanup preamble around the user dispatch); mabda emits
~64 dwords (minimum-viable shader-run, no general state setup).
A direct byte-diff on the raw streams is meaningless. `make compare`
runs `extract_dispatch.sh` over both dumps to filter down to the
**compute dispatch tail** — the SET_SH_REGs that touch
`COMPUTE_*` registers (PGM_LO/HI, RSRC1/RSRC2, USER_DATA_*,
NUM_THREAD_*, RESOURCE_LIMITS, TMPRING_SIZE,
STATIC_THREAD_MGMT_SE*), the ACQUIRE_MEM that flushes caches
before the dispatch, and the final `DISPATCH_DIRECT` packet
itself. That's where actual semantic divergence (wrong register
value, missing register write, wrong workgroup count) shows up.

### Example output

```
$ make diff-tail
SET_SH_REG      reg=0xB834  COMPUTE_PGM_HI                   vals=0x00000080
SET_SH_REG      reg=0xB858  COMPUTE_STATIC_THREAD_MGMT_SE0   vals=0xFFFFFFFF,0x00000000
SET_SH_REG      reg=0xB864  COMPUTE_STATIC_THREAD_MGMT_SE2   vals=0x00000000,0x00000000
SET_SH_REG      reg=0xB830  COMPUTE_PGM_LO                   vals=0x01000000
SET_SH_REG      reg=0xB848  COMPUTE_PGM_RSRC1                vals=0x002C0040,0x00000008
SET_SH_REG      reg=0xB860  COMPUTE_TMPRING_SIZE             vals=0x00000100
SET_SH_REG      reg=0xB908  COMPUTE_USER_DATA_2              vals=0x00004000,0xFFFF8001
SET_SH_REG      reg=0xB900  COMPUTE_USER_DATA_0              vals=0x00400000,0xFFFF8001
ACQUIRE_MEM     (6 data dwords)
SET_SH_REG      reg=0xB854  COMPUTE_RESOURCE_LIMITS          vals=0x00000140
SET_SH_REG      reg=0xB81C  COMPUTE_NUM_THREAD_X             vals=0x00000001,0x00000001,0x00000001
DISPATCH_DIRECT dim=(1,1,1)  initiator=0x00000045
```

### Pass criteria

After `make compare` produces a diff, every diff line must fall
into one of:

  1. **Byte-clean** — no diff at all on a register line. Mabda
     and RADV both emit the same `vals=...` for that register.
  2. **Equivalent-but-not-identical** — the diff line corresponds
     to a known-equivalent shape captured in the table below.
     Document new entries here with a one-line rationale + Mesa
     source pointer.

### Known equivalents (RADV vs mabda)

| RADV emits | mabda emits | Why equivalent |
|------------|-------------|----------------|
| `EVENT_WRITE CACHE_FLUSH_AND_INV` post-dispatch | `WRITE_DATA(WR_CONFIRM=1)` CP marker | Both flush + signal CP returned. mabda's marker also probes IB-execution liveness; RADV doesn't need that since it's well-trusted. |
| Multiple `ACQUIRE_MEM` packets across init+state | Single `ACQUIRE_MEM` full-invalidate before dispatch | mabda has no init/state phase — one full-invalidate covers the same surface. |

(Table is empty until a HW capture is done; entries land in the
PR that adds them.)

### Extractor flags

`./extract_dispatch.sh --all < input` — emit every PM4 packet in
the stream, not just compute-relevant ones. Useful for inspecting
RADV's full preamble; too noisy for `diff`.

## Dependencies

- `vulkan-headers` — `/usr/include/vulkan/vulkan.h`
- `vulkan-icd-loader` — `libvulkan.so`
- `vulkan-radeon` — RADV ICD (Mesa)
- `glslang` — `glslangValidator` for compute shader compile

On Arch:
```sh
sudo pacman -S vulkan-headers vulkan-icd-loader vulkan-radeon glslang
```

## Why this exists

mabda owns its PM4 composer end-to-end. A correctness regression in
the composer (wrong register address, wrong field layout, missing
ACQUIRE_MEM / CACHE_FLUSH_AND_INV) would otherwise be diagnosed by
running the program on real hardware and observing "the buffer
isn't 0xDEADBEEF" — which is true but not very actionable.
Comparing against RADV's known-good PM4 surfaces "you wrote 0x14
where RADV writes 0x18 in this register" almost immediately.

This is the same diagnostic posture as comparing a Cyrius compiler
output against `objdump -d` of `clang`-compiled C — known-good
codegen as a teaching tool, not as a runtime dependency.

## Output format note

RADV's IB dump format is documented in
`mesa/src/amd/vulkan/radv_cs_dump.c` and matches the format
`umr` emits. Each IB starts with `--- IB X ---` headers, then
hex dwords with mnemonic decode where RADV recognises the packet.
This is the canonical AMDGPU PM4 textual format — safe to grep,
diff, and parse.
