# radv_capture — Layer-2 PM4 reference verifier

**Purpose**: emit a reference AMDGPU IB byte-stream against which
mabda's pure-Cyrius PM4 composer (`src/backend_native.cyr` →
`native_pm4_build_compute_store_deadbeef`) can be byte-diff'd.

**Phase**: 1 (minimum-viable). Proves the toolchain works + dispatch
runs + readback returns `0xDEADBEEF` + RADV emits the IB dump.
Phase 2 (the actual byte-diff reduction) is a separate v3.x bite.

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

## Compare workflow (Phase 2 — not yet implemented)

The intent is to produce a side-by-side comparison:

```sh
# 1. capture RADV reference
make dump

# 2. capture mabda's PM4 (native_compute_store doesn't dump today;
#    add a --dump-pm4 flag in a future bite)
cd ../../..
make test-native-compute-store DUMP_PM4=1 2>mabda.ib.txt

# 3. diff
diff radv.ib.txt mabda.ib.txt
```

Today, step 2 doesn't exist — `programs/native_compute_store.cyr`
runs the dispatch but doesn't expose the composed IB byte stream.
A future bite adds either a `--dump-pm4` flag or a small companion
program (`programs/native_pm4_dump.cyr`) that calls the composer
without submitting and prints the dword stream.

The diff itself will not be byte-clean — RADV emits ~50–80 init /
state / barrier packets that mabda's composer doesn't (mabda's
posture is "minimum-viable PM4 to make the shader run, no general
state setup"). The useful comparison is the **dispatch tail** — the
final `DISPATCH_DIRECT` packet plus the immediately preceding
`COMPUTE_PGM_*` / `COMPUTE_USER_DATA_*` / `COMPUTE_NUM_THREAD_X/Y/Z`
register writes. Phase 2 reduction will write a small parser that
extracts only those packets from each dump.

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
