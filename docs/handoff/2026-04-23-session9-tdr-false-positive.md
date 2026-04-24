# v3 Phase B — Session 9 Finding: B.3.d Was a TDR False Positive

**Date:** 2026-04-23 (session 9, afternoon)
**Branch:** `v3`
**Working tree:** clean after Session 9 revert
**Status:** B.3.d **not actually passing**. Every direct-DRM CS submission we've made — including Session 8's "success" — has been a 10-second Timeout Detection & Recovery (TDR) signal, not real CP execution. The Session 7 PM4 encoder fixes were real and necessary but sit downstream of the actual blocker.

## TL;DR

While preparing B.4 (store shader), I added a WRITE_DATA diagnostic to the spike and found that it never lands in memory despite "dispatch completed (sync-obj signaled)". Systematic isolation revealed:

- **Even an IB with ONLY a NOP packet** hits exactly ~10 seconds and returns RC=0. That's AMDGPU's default TDR — kernel times out the submission, resets the ring, and signals the sync-obj as "done" so userspace doesn't hang forever.
- **Mesa's `cl_probe` completes in 77ms** with correct readback on the same hardware. So the GPU is healthy and the submission path *through libdrm* works fine.
- **`/sys/class/drm/card0/device/compute_reset_mask`** reports `soft queue full` — the kernel has been doing compute-queue resets.

The CP has never actually executed any of our PM4 in any Phase B session. What we thought was "B.3.d passed" was the kernel TDR-resetting a ring and signalling our fence as completed-via-reset. Since `s_endpgm` is a no-op that doesn't write anything, the post-submit readback of our BOs looked indistinguishable from a "successful" run.

## How we got here — chronology of the discovery

1. **Session 7 (2026-04-22) PM4 encoder fixes landed** — ACQUIRE_MEM count off-by-one, DISPATCH_DIRECT shader_type byte. Both real bugs, both fixed byte-exactly against Mesa's `AMD_DEBUG=ib` dump.
2. **Session 8 (2026-04-23 morning)** — live retest of B.3.d spike with the Session 7 fixes. Output: `dispatch completed (sync-obj signaled)`, RC=0, dmesg silent. Marked B.3.d ✅ in docs, bumped toolchain, prepared B.4 store shader + program.
3. **B.4 first iterations (2026-04-23 afternoon)** — real store shader restored in `src/backend_native.cyr`; store program mirrored spike's PM4 preamble. First run: `dispatch completed (sync-obj signaled)` but `output[0]` was still `0xBAADF00D` (seeded sentinel). RC=11 (mismatch). Tried USER_SGPR=2 vs 4, canonical-high vs low output VA, hardcoded-VA-in-shader variant (no USER_DATA dependency). All same result: dispatch "completed", no store.
4. **Diagnostic escalation** — added a CP-side WRITE_DATA packet (not involving the shader at all) to the PM4 stream. Still nothing written. Tried WRITE_DATA first vs last in the IB; tried GFX ring vs COMPUTE ring; tried AMDGPU_GEM_CREATE_CPU_GTT_USWC flag; tried AMDGPU_GEM_CREATE_VM_ALWAYS_VALID. Nothing landed.
5. **Final test — NOP-only IB** — with the compute state preamble and DISPATCH entirely stripped out, leaving only NOP padding. **Exit timing: 9.9s → 10.0s → 15s (actual syncobj timeout on third run)**. This was the smoking gun: the kernel TDR is the only reason our submissions ever "complete."

All reverted to pristine state; Session 9's diagnostic changes are not committed.

## Confirmed facts (experimentally, Session 9)

| Observation | Verified how | Conclusion |
|---|---|---|
| cl_probe runs in 77ms with correct readback | `time ./build/shader/cl_probe` | GPU is healthy, Mesa's path works. |
| Our spike + WRITE_DATA to stub_va never changes stub[0] | `load32(stub_map + 0)` post-submit | CP-side writes aren't reaching memory. |
| Our IB bytes are intact post-submit | `load32(ib_map + N)` reads back what we wrote | CPU→GPU write coherency is fine; the bytes are there. |
| CS ioctl returns 0, sync-obj fires RC=0 | instrumented check | Kernel accepts and "completes" the submission. |
| Empty/NOP-only IB hits exactly ~10s | `time ./build/native_compute_spike` × 3 runs | TDR default is 10s; the sync-obj is firing via reset recovery, not CP completion. |
| `compute_reset_mask` = `soft queue full` | `cat /sys/class/drm/card0/device/compute_reset_mask` | Kernel has been doing compute-queue resets. |
| No dmesg entries for bad_op_irq / MODE2 reset | `sudo dmesg --since "1 minute ago"` silent | TDR here is the "soft" variety — timeout-then-signal without a kernel-level error log. |

## Where the actual blocker lives

**Between "CS ioctl returns 0" and "CP executes our IB," something fails silently.** Candidates (my ranking):

1. **Ring index / ip_instance mismatch.** We hard-code `ring=0, ip_instance=0` in the IB chunk. Mesa's libdrm wrapper auto-selects a valid ring; we may be targeting a kernel-reserved or firmware-managed ring that accepts submissions but never processes them.
2. **Missing CS chunks we don't know about.** Mesa's strace shows it uses `DRM_IOCTL_AMDGPU_CTX` and `DRM_IOCTL_AMDGPU_GEM_VA` + `DRM_IOCTL_AMDGPU_CS`, but **no `DRM_IOCTL_AMDGPU_BO_LIST`**. BOs are declared via `AMDGPU_CHUNK_ID_BO_HANDLES` (0x06) as a CS chunk instead. We tried switching to BO_HANDLES once but got ENOENT — likely a chunk-payload-layout detail (single-u32 handles vs 8-byte bo_list_entry). Worth pursuing.
3. **CS `flags` / IB `flags` bits.** We set both to 0. Mesa may set `AMDGPU_IB_FLAG_EMIT_MEM_SYNC` or similar. We tried EMIT_MEM_SYNC alone — no effect, but that doesn't rule out combinations.
4. **Context priority / flags at ALLOC time.** We pass priority=0, flags=0 to `AMDGPU_CTX_OP_ALLOC_CTX`. Mesa may use AMDGPU_CTX_PRIORITY_NORMAL or a different path.
5. **Accumulated GPU state from this session's TDRs.** Several hits today may have left AMDGPU in a degraded state specific to our DRM fd / context. **Reboot would rule this out.**

## Next-session runbook (after reboot)

### Step 0 — reboot, then health-check

```bash
# Sanity check that GPU is alive and Mesa still works
./build/shader/cl_probe              # expect: RC=0, readback=0xDEADBEEF, < 200ms total
cat /sys/class/drm/card0/device/compute_reset_mask
```

### Step 1 — baseline the current bug

```bash
# Time a single spike run — expect ~10s if the problem is our code, < 1s if it was accumulated GPU state
time ./build/native_compute_spike
```

**Outcome A (< 1s, "success"):** Accumulated state was the problem. The Session 7 PM4 fixes may actually be enough. Add a WRITE_DATA diagnostic to confirm (see below) and re-validate B.3.d and B.4.

**Outcome B (~10s, TDR):** Confirms the blocker is in our submission path, not accumulated state. Proceed to step 2.

### Step 2 — re-add the WRITE_DATA diagnostic to the spike

```cyrius
# After `var pos = 0;` in programs/native_compute_spike.cyr:
store32(pm4 + pos +  0, 0xC0033700);   # PKT3 WRITE_DATA, count=3
store32(pm4 + pos +  4, 0x00110500);   # DST_SEL=MEM_ASYNC | WR_ONE_ADDR | WR_CONFIRM
store32(pm4 + pos +  8, stub_va & 0xFFFFFFFF);
store32(pm4 + pos + 12, (stub_va / 0x100000000) & 0xFFFFFFFF);
store32(pm4 + pos + 16, 0xCAFEBABE);
pos = pos + 20;

# And after `dispatch completed` print:
print_cstr("stub[0] post-submit = "); print_hex32(load32(stub_map + 0));
print_cstr(" (want 0xCAFEBABE if IB ran)\n");
```

If `stub[0] = 0xCAFEBABE` after a clean-reboot run, the CP IS executing our IB — and the question reverts to whether compute DISPATCH_DIRECT specifically is hanging.

### Step 3 — BO_HANDLES chunk path (if Step 2 still shows `stub[0] = 0`)

Mesa's strace shows exactly three AMDGPU ioctls for CS setup per dispatch: CTX, GEM_VA, CS. **No BO_LIST.** BOs are declared inline via `AMDGPU_CHUNK_ID_BO_HANDLES` (0x06) in the CS chunks array. Our one attempt at this got `ENOENT` — likely a payload-layout detail.

Two things to try, in order:

1. **Payload as bare u32 handles** (4 bytes each, no priority). length_dw = num_bos × 1. This matches the plural naming ("BO_HANDLES") and is the simplest interpretation.
2. **Payload as `drm_amdgpu_bo_list_entry` (handle + priority, 8 bytes)** — what we tried in Session 9. Got ENOENT, but maybe with different chunk ordering (BO_HANDLES first in the chunks array, before IB).

Reference: extract Mesa's exact chunks with `ltrace -i -l libdrm_amdgpu.so.1 ./build/shader/cl_probe 2>&1 | grep -i cs_submit_raw` or by building a minimal C program against `libdrm_amdgpu.so` that replicates our spike structure.

### Step 4 — last-resort: link against libdrm_amdgpu

If step 3 fails, temporarily add `libdrm_amdgpu` as a C-side dep and route the CS submit through `amdgpu_cs_submit_raw2` while keeping BO creation / PM4 building in Cyrius. This would *prove* the gap is specifically in our direct-ioctl CS submission (not PM4 or BO mapping), narrowing the fix surface. This is a temporary diagnostic — the sovereign goal is direct-ioctl.

## What's already committed on `v3` (correct + useful — don't revert)

The following Session 9 prep work is real and stays valid once the submission path is fixed:

- **`src/backend_native.cyr::native_gfx9_shader_store_deadbeef`** — real 6-instruction store kernel from `clang -target amdgcn--amdhsa -mcpu=gfx90c`, with 16-dword NOP padding. Size: 96 bytes.
- **`tests/tcyr/mabda.tcyr::test_native_gfx9_shader_store_deadbeef_writes_bytes`** — 11 byte-exact assertions. Test count: 610 → 621 (all green under 5.6.13).
- **`programs/native_compute_store.cyr`** — rewritten to mirror the spike's Mesa-verified PM4 preamble with three store-specific deltas (USER_DATA_0/1 = output VA; USER_DATA_2/3 = spike's scratch-V# stub; 4-BO list).
- **`cyrius.cyml`** pinned to `5.6.13` (released line).
- **`dist/mabda.cyr`** regenerated to catch up with v3 Phase A.
- **Docs** (CHANGELOG, v3 proposal, store-blocker issue) — all updated to reflect Session 7/8 status.

Once the submission path is fixed, these are ready to use — no re-work needed.

## What to revert in docs

The following docs were written on the *premise* that B.3.d passed. They need amending once the retest runs post-reboot:

- **`CHANGELOG.md`** — the `## [3.0.0-dev]` block has "Verified — 2026-04-23" subsection claiming B.3.d passed live. That claim is false. Replaced this session with a "Discovered later same day — was TDR false positive" note.
- **`docs/proposals/v3-native-api-principles.md`** — "Phase B status (2026-04-23)" block has B.3.d as ✅. Downgraded to ⚠️.
- **`docs/issues/2026-04-21-gfx9-store-blocker.md`** — Session 8 claims "Closed." Added Session 9 section retracting the close.

## Gotchas to respect (unchanged from prior handoffs + new)

- **Don't edit `lib/*.cyr` by hand.** HARD RULE in CLAUDE.md.
- **Don't chase register values before PM4 byte-diffing** — this is still true, but note it only matters *once* submissions actually reach the CP.
- **Every TDR-induced reset leaves the compute queue in a degraded state.** If you see the spike run taking ~10s *after a fresh reboot*, stop immediately and debug the submission path before more runs. Each TDR hit compounds the "soft queue full" state per `/sys/class/drm/card0/device/compute_reset_mask`.
- **`time ./build/native_compute_spike` is the quickest diagnostic.** Fast (< 1s) = real completion. Slow (~10s) = TDR.

## Repo state at handoff

```
branch: v3
working tree: clean (Session 9 diagnostic changes reverted)

committed through Session 8 (aa66a0a ... 063911a):
  - Session 7 PM4 encoder fixes (real + necessary)
  - B.4 prep (real store shader, byte-exact test, aligned store program)
  - Toolchain pin 5.5.20 → 5.6.13
  - Dist refresh (v3 Phase A catch-up)

uncommitted (after Session 9):
  <nothing — everything reverted, this handoff will be added>

CPU tests: 621 passed, 0 failed (5.6.13)
GPU integration (direct-DRM): BLOCKED — submissions reach kernel but not CP.
GPU integration (Mesa cl_probe, control): works, 77ms.
```

## Supporting material

- **Mesa reference IB:** regenerate with `AMD_DEBUG=ib ./build/shader/cl_probe`.
- **Prior handoffs:** `docs/handoff/2026-04-22-pm4-fix-retest.md` (Session 7 setup), `docs/handoff/2026-04-23-b4-store-retest.md` (what we thought was the B.4 path — now amended by this doc).
- **Issue doc:** `docs/issues/2026-04-21-gfx9-store-blocker.md` — Session 9 section captures the retract.
- **vidya field notes:** `../vidya/content/direct_drm_gpu_compute/concept.toml` — the TDR false-positive gotcha should land here as a capitalized warning for any future direct-DRM work on any AGNOS project.
- **Auto-memory:** consider saving `feedback_verify_gpu_actually_ran.md` — *any* direct-DRM work on AMD must include a WRITE_DATA-or-equivalent diagnostic in the first test IB that the CPU can read back, so TDR false positives can't hide.
