# Security/Correctness Audit — v3.2.1 (buffer-copy, Phase X), 2026-06-15

**Scope:** the v3.2.1 Phase-X buffer-copy diff vs the `3.2.0` tag —
`error.cyr` (`GPU_ERR_TRANSFER`), `backend.cyr` (buffer_copy slot + walk,
`BACKEND_SIZE` 256→264), `backend_native.cyr` (real `NativeBuf` create/
write/read/release, `native_transfer_copy_timeline`, `_backend_native_buffer_copy`,
TRANSFER→DMA ring flip + compute mis-route guard), `backend_native_pm4.cyr`
(SDMA `COPY_LINEAR` + chained builder), `backend_wgpu.cyr` (`_backend_wgpu_buffer_copy`,
buffer-create usage), `buffer.cyr` / `queue.cyr` (public `gpu_buffer_copy` /
`gpu_queue_transfer_copy`).

**Method:** the X.6 work-loop audit run as an adversarial workflow — 4
dimensions (wgpu-copy correctness, backend layout/walk integrity, native
SDMA correctness, test-coverage gaps) in parallel, then each finding
independently re-verified against source to drop false positives. 11 raw
findings → **7 confirmed / 4 dismissed**.

**Result:** **0 CRITICAL / 2 HIGH / 1 MEDIUM** distinct defects (plus test-gap
items), **all fixed in X.6 / X.8** before the cut.

## Confirmed findings + resolutions

| # | Sev | Location | Issue | Resolution |
|---|-----|----------|-------|------------|
| 1 | HIGH | `buffer.cyr` / `queue.cyr` | wgpu `copyBufferToBuffer` requires a 4-byte-multiple size; native SDMA is byte-granular → `gpu_buffer_copy(...,67)` succeeded on AMD but was a non-recoverable wgpu validation error (no error callback registered). Cross-backend asymmetry in a public API. | Both public dispatchers reject non-DWORD sizes (`bytes & 3`) → `GPU_ERR_BUFFER`, uniformly across backends. Tests on both dispatchers + both HW e2es. |
| 2 | HIGH | `backend_native_pm4.cyr` / `backend_native.cyr` | SDMA `COPY_LINEAR` count field is 22 bits (4 MiB); a larger copy spilled past bit 21 and silently corrupted. | X.6: reject >4 MiB loudly. X.8: `native_sdma_build_copy_chained` splits across packets (each ≤4 MiB), rejecting only an IB-overflowing chain. HW-verified 6 MiB across the boundary. |
| 3 | MEDIUM | `backend_wgpu.cyr` | The X.6 comment claimed `gpu_buffer_create` forces `COPY_SRC/COPY_DST`; it didn't — a STORAGE-only buffer copied fine on native (usage ignored) but failed wgpu validation. | `_backend_wgpu_buffer_create` OR's in `COPY_SRC|COPY_DST` (MAP-aware) so any mabda buffer is a copy operand, matching native; comment corrected. |

Test-gap findings (LOW): wgpu copy filler had no CPU guard coverage; `gpu_buffer_copy`
had no positive-path/arg-propagation test; alignment was untested. All addressed
with new asserts in `backend.tcyr` / `native.tcyr`.

## Dismissed on verification (4)

- "wgpu copy leaks the command encoder" / "...command buffer" — **NOT_A_BUG**:
  the established codebase pattern (buffer_read / texture_read / render_pass_end
  all do the same; wgpu-native holds refs through submission). Out of the
  buffer-copy scope; not introduced by Phase X.
- "Pending cross-ring barrier wait dropped when the SDMA submit fails" —
  **NOT_A_BUG**: the wait is read + cleared before submit by design (a barrier
  applies to the next submit only).
- "Guard-order asymmetry between native/wgpu fillers untested" — **NIT**: both
  reach the same error codes via the public dispatchers.

## HW verification

`programs/native_transfer_copy_e2e.cyr` on Cezanne (AMD render node): leg A
(4 KiB `gpu_buffer_copy` round-trip + alignment-guard reject), leg B
(compute-produce → barrier → public `gpu_queue_transfer_copy` consume), leg C
(6 MiB chunked copy, byte-exact across the 4 MiB packet boundary). All other
native HW programs (compute_store / multiqueue / sdma_copy / queue_compute)
re-run clean — no regression from the ring flip or mis-route guard.

**Finding (non-defect, architectural):** producer and consumer cannot OVERLAP
through the single per-context cached IB (the consumer's packet clobbers the
producer's). The e2e serializes with `wait_idle`, matching
`native_multiqueue_e2e`. True overlap needs per-submission IB staging →
tracked as **R.5 (3.2.13)**.
