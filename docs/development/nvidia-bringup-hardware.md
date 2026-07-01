# NVIDIA Native Bring-Up — Hardware Ladder

Tracks the physical GPUs used to bring up mabda's v4.0 NVIDIA native
backend, in order, and why the ladder is shaped this way. Referenced
from the v4.0 section of [roadmap.md](roadmap.md).

## Why a ladder

The native backend is brought up **one GPU generation at a time**, not
once on a single card. Per the roadmap's v4.0 notes, NVIDIA shaders
ship as **SASS (per-SM-class assembly)** and submission uses a
per-class command method format — so the parts of the backend below
the `Backend` contract differ enough between generations that each is
validated on real silicon of that generation. The ladder therefore
walks one representative card per generation rather than assuming a
single card covers the line.

## The bring-up host

A single dedicated host (an older i7 workstation) carries whichever GPU
is currently under bring-up. **The host is wiped clean for every GPU
swap.** Consequence: anything installed on it — drivers, toolchain,
packages, scratch state — is disposable and does **not** carry across
swaps or to other machines. The durable record is the git repo and
this document, never machine-local state.

## The ladder

| # | GPU | Generation | Status |
|---|-----|-----------|--------|
| 1 | GTX 1660 Super | Turing | **current target** — on **nouveau** (host swap done, see below). Native backend **N0–N3 + N5.1/5.2 done & HW-proven**; **N4 compute arc gate in progress (not yet green)** — see [`nvidia-n4-capture-notes.md`](nvidia-n4-capture-notes.md) |
| 2 | RTX 3060 12 GB | Ampere | queued — begins after the 1660 native bring-up lands |

**Deliberately skipped**

- **RTX 4060 (Ada/Lovelace).** Not purchased — not worth spending on a
  generation that is about to fall a generation behind. The ladder
  skips it rather than buying every generation.

**Out of scope for this host**

- **RTX 5060.** Lives on a separate, much newer machine — not part of
  this bring-up host's ladder.

## Current host state — 2026-06-27 (post-reboot, nouveau)

The bring-up box booted on the **nouveau** kernel driver (it ran
proprietary nvidia.ko 610.43.02 earlier the same day; the punchlist's
N0.6 host swap is therefore **done**, accomplished by the reboot rather
than a live unbind). Empirical boot facts, captured from `dmesg`,
`/sys`, and `vulkaninfo`/`clinfo`:

| Fact | Value | Source |
|------|-------|--------|
| Kernel | `7.0.13-arch1-1` (≥ 6.6 ✓, needed for the VM_BIND uAPI) | `uname -r` |
| Bound driver | `nouveau` on **both** `renderD128` and `card0` | `/sys/bus/pci/drivers/nouveau`, uevent |
| Chipset | TU116 `168000a1`; PCI `10DE:21C4` | `dmesg`, uevent |
| GSP-RM | **active**, `RM version: 570.144` (the recommended/conformance-tested config; the "no-GSP boot-clock" question in N0.7 is moot) | `dmesg gsp:` |
| GSP firmware on disk | `nvidia/tu116/gsp/` has 535.113.01 + 570.144 (nouveau loaded 570.144) | `/lib/firmware` |
| DRM nouveau iface | **1.4.2** — ≥ 1.4.0, so `VM_INIT`/`VM_BIND`/`EXEC` uAPI is compiled in | `dmesg [drm] Initialized nouveau 1.4.2` |
| VRAM | 6144 MiB GDDR6 (matches 1660 SUPER) | `dmesg drm: VRAM` |
| Render node perms | `renderD128` is `0666` (render grp + world rw) — masterless-friendly | `ls -l /dev/dri` |
| nvidia.ko | not loaded; `nvidia-smi` absent | `lsmod`, PATH |
| OpenCL | rusticl enumerates the platform but **0 devices** on nouveau (gallium nvc0 CL path, not NVK; irrelevant — mabda's native path is pure DRM ioctl, never OpenCL) | `clinfo` |

**Card-node delta:** with nvidia.ko gone, nouveau is the only DRM driver
and took `card0` (the punchlist N8 KMS text assumes `card1`, which was
the nvidia.ko-era node — update when N8 lands). Compute uses
`renderD128`, which is stable.

**Still empirical (not answered by dmesg):** whether `renderD128`
exposes `VM_INIT`/`VM_BIND`/`EXEC` as `DRM_RENDER_ALLOW` (masterless) —
settled by the N1/N0.7 enum probe — and a known-good QMD + pushbuffer
capture for the N4 byte-diff. Per the host doc's wipe-on-swap rule, none
of this nouveau state is durable; this table is the record.

## Relationship to the roadmap

This ladder is the concrete answer to the v4.0 exit-criteria "bring-up
class." Where the roadmap text previously left the generation open
("TBD … likely Ampere or Lovelace"), the real plan is **Turing first
(GTX 1660 Super), then Ampere (RTX 3060 12 GB)**, with later
generations added as hardware lands.
