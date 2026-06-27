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
| 1 | GTX 1660 Super | Turing | **current target** — wgpu/Vulkan path validated on hardware (full wgpu E2E suite passes); NVIDIA native backend not yet started |
| 2 | RTX 3060 12 GB | Ampere | queued — begins after the 1660 native bring-up lands |

**Deliberately skipped**

- **RTX 4060 (Ada/Lovelace).** Not purchased — not worth spending on a
  generation that is about to fall a generation behind. The ladder
  skips it rather than buying every generation.

**Out of scope for this host**

- **RTX 5060.** Lives on a separate, much newer machine — not part of
  this bring-up host's ladder.

## Relationship to the roadmap

This ladder is the concrete answer to the v4.0 exit-criteria "bring-up
class." Where the roadmap text previously left the generation open
("TBD … likely Ampere or Lovelace"), the real plan is **Turing first
(GTX 1660 Super), then Ampere (RTX 3060 12 GB)**, with later
generations added as hardware lands.
