# state_poison — inherited render-state proof harness

Diagnostics, not gates. These tools reproduce the proof behind mabda 4.1.3's
render-state fix: every native render stream must **own** its GPU register
state rather than inherit whatever the previous GPU client left. The
per-register verdicts and the hardware evidence are in
[`../radv_capture_triangle/owned-state-audit.md`](../radv_capture_triangle/owned-state-audit.md).

`make build-gpu-programs` checks only `programs/*.cyr`, so nothing here is swept
into it; build these explicitly (below).

## What is here

| file | what it does |
|---|---|
| `state_poison.cyr` | Submits ONE register-only GFX IB (SET_CONTEXT_REG / SET_UCONFIG_REG packets, NOP pad, no draw, no memory reference) for a **named** spec compiled into the tool: `vte0`, `vte43f`, `clear`, `wrong`, `good`, and the single-register negative controls `n1-indxoff` … `n5-sloc` with their `-restore` specs. `state_poison --list` prints them. |
| `coverage_scan.cyr` | `all [dumpdir]`: solid red 64x64, textured 64x64 and solid red 1920x1080, every pixel checked (optionally dumps the submitted PM4 streams). `canary`: the harness liveness check (see below). |
| `dump_streams.cyr` | CPU only: writes the clear-triangle, untextured and textured pass_draw PM4 streams to a directory. |
| `run_gate.sh` | One poison -> gate step; stops (exit 99) on any kernel ring timeout / reset / fault line. |
| `matrix.sh` | A poison spec before each of the 9 native draw gates + `coverage_scan all`. |
| `native_all_poison.sh` | `make test-native-all`, with a poison spec before every draw gate. |
| `owned_state_audit.py` | Regenerates / checks (`--check`) the register tables in the audit document. |

## GPU safety

- Register/value pairs never come from the command line. Every compiled-in value
  is the kernel clear state, the value radv writes on this GPU, or a value
  already run as a negative control on the Cezanne without a ring timeout.
  Registers with fault/hang potential (CB_COLORn_INFO, stream-out, DFSM,
  SPI_TMPRING_SIZE, RSRC3) are only written with their clear value. Add a spec
  only with that evidence, and never add a value whose effect on the ring is
  unknown.
- `run_gate.sh` / `matrix.sh` / `native_all_poison.sh` scan the kernel log after
  every step and stop at the first amdgpu timeout / reset / fault / coredump.
  If that happens, stop all GPU work and investigate before resubmitting.
- The canary draw only ever inherits `PA_CL_VTE_CNTL`, which the specs set to
  0 or 0x43F — the same draws the 4.1.3 unfixed library ran many times.

## Build

From the repository root (needs `/dev/dri/renderD128` for the GPU tools):

```sh
cyrius build programs/diagnostics/state_poison/state_poison.cyr  build/state_poison
cyrius build programs/diagnostics/state_poison/coverage_scan.cyr build/coverage_scan
cyrius build programs/diagnostics/state_poison/dump_streams.cyr  build/dump_streams
for g in texture_sample array_sample bc_array bilinear_sample compressed_sample \
         cube_sample load_png render render_graph_mq; do
  make build/native_${g}_e2e
done
```

## The proof, step by step

1. **Liveness.** The harness only proves something if the GPU carries register
   values from one submission into the next process's draw (it did on the
   4.1.3 Cezanne after a MODE2 reset). Check it first:

   ```sh
   programs/diagnostics/state_poison/run_gate.sh vte43f "run:coverage_scan canary"   # must exit 0
   programs/diagnostics/state_poison/run_gate.sh vte0   "run:coverage_scan canary"   # must exit 30
   ```

   Exit 30 means the draw inherited `PA_CL_VTE_CNTL = 0` and covered only pixel
   (0,0). If `vte0` gives 0, the GPU reset the state between submissions (or
   another client rewrote it — see step 4) and a matrix run proves nothing.

2. **Fixed library:** every gate passes under every spec.

   ```sh
   REPS=2 programs/diagnostics/state_poison/matrix.sh clear
   REPS=2 programs/diagnostics/state_poison/matrix.sh wrong
   programs/diagnostics/state_poison/native_all_poison.sh clear
   ```

3. **Negative control:** point the matrix at a library that inherits the state
   (e.g. a tree at the pre-4.1.3 commit, with its gates built) — every gate must
   fail:

   ```sh
   MABDA_ROOT=/path/to/old-tree POISON=$PWD/build/state_poison REPS=3 \
     GATES="native_texture_sample_e2e native_render_e2e ..." \
     programs/diagnostics/state_poison/matrix.sh clear
   ```

4. **Other GPU clients.** A display server / login greeter on the same GPU
   (radeonsi: CONTEXT_CONTROL + CLEAR_STATE + its own state on every
   submission) rewrites the poisoned registers within roughly 0.1-1 s. On the
   4.1.3 Cezanne a `vte0` poison survived a draw started immediately 6/6 times,
   3/6 after 500 ms and 0/6 after 1 s. So run gate binaries directly (not via a
   slow build), repeat negative controls (`REPS=3`), and treat a lone pass of a
   broken library as a clobber until repeated.

5. **Restore** with `build/state_poison good` when done.

## Regenerating the audit tables

Inputs: Mesa `src/amd/registers/gfx9.json` (the tag matching the installed
Mesa), `drivers/gpu/drm/amd/amdgpu/clearstate_gfx9.h` at the running kernel's
tag, a radv capture from `../radv_capture_triangle` (`make snoop`), and
`dump_streams` output.

```sh
curl -o /tmp/gfx9.json "https://gitlab.freedesktop.org/api/v4/projects/176/repository/files/src%2Famd%2Fregisters%2Fgfx9.json/raw?ref=mesa-26.2.2"
curl -o /tmp/clearstate_gfx9.h "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/plain/drivers/gpu/drm/amd/amdgpu/clearstate_gfx9.h?h=v7.2.3"
(cd programs/diagnostics/radv_capture_triangle && make snoop)
mkdir -p build/streams && build/dump_streams build/streams
python3 programs/diagnostics/state_poison/owned_state_audit.py \
  --gfx9 /tmp/gfx9.json --clearstate /tmp/clearstate_gfx9.h \
  --radv-init programs/diagnostics/radv_capture_triangle/snoop/cs000_c0-ib0_*.bin \
  --radv-main programs/diagnostics/radv_capture_triangle/snoop/cs000_c1_*.bin \
  --mabda build/streams --check programs/diagnostics/radv_capture_triangle/owned-state-audit.md
```

`--check` exits 1 if the committed tables are stale; without it the tables are
printed. The script also fails if a register in gfx9.json has no verdict or an
owned register differs from both radv and the clear state without a written
reason. `--mabda-before DIR` adds a "before" column from an older library's
`dump_streams` output (for reviewing a change; the committed tables omit it).
radv's VA-bearing values (PGM_LO, CB_COLOR0_BASE, ...) come from its allocator:
regenerate, rather than `--check`, after a Mesa upgrade.
