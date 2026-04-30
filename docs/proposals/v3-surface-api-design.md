# v3 Surface API design (Step 7.7)

**Status:** Proposal — review pending
**Date:** 2026-04-30
**Filed for:** Step 7.7 of `docs/development/3-0-punchlist.md`
**Closes:** Phase D code-completion (last open Tier 1 surface item)

## Problem statement

Step 7.6 wired the v3 backend interface's surface slot offsets +
range walk + per-backend stub fns. Both `backend_wgpu_new` and
`backend_native_new` install all 25 slots, `backend_is_complete`
returns 1, but the surface stubs return `0` / `GPU_ERR_OTHER` at
runtime. Two protocol questions block real implementations:

1. **Native master-fd protocol.** SETCRTC + page-flip require DRM
   master, but modern Linux (systemd-logind) retains master in
   the running compositor's session even after vt-switch + sudo.
   Verified live on the dev box (Cezanne + Hyprland): the modeset
   pipeline runs end-to-end through `AddFB2` and only fails at
   the SETCRTC step with EACCES. See
   [`project_phase_d_master_logind_blocker`](../../../.claude/projects/-home-macro-Repos-mabda/memory/project_phase_d_master_logind_blocker.md)
   memory for the full diagnosis.
2. **wgpu surface handle protocol.** wgpu surfaces require a
   platform-specific window handle (X11 / Wayland / HWND) which
   only the consumer can produce. The current slot signature
   `surface_configure(ctx, w, h)` doesn't carry one.

Both questions have the same shape: how does the consumer pass
backend-specific resources into a backend-agnostic slot
interface?

## Design — layered API, both paths supported

The internal slot signatures stay as designed in 7.5. The
**public** API surface gains backend-specific entry points
that stash resources into the GpuContext, then call into the
generic slot abstraction.

```
┌─ Consumer code ────────────────────────────────────────────┐
│                                                            │
│  // Wgpu path                                              │
│  ctx = gpu_context_from_preinit(...);                      │
│  wgpu_surface = wgpu_instance_create_surface(...);         │
│  surface = gpu_surface_configure_wgpu(ctx, wgpu_surface,   │
│                                       w, h);              │
│                                                            │
│  // Native TTY/kiosk path (v3.0)                           │
│  ctx = gpu_context_new_native();                           │
│  card_fd = open("/dev/dri/card0", O_RDWR);                 │
│  drm_set_master(card_fd);   // caller's responsibility    │
│  surface = gpu_surface_configure_native_kiosk(ctx,         │
│                  card_fd, w, h);                          │
│                                                            │
│  // Native logind path (v3.x — stubbed in v3.0)            │
│  surface = gpu_surface_configure_native_logind(ctx, w, h); │
│  // → returns 0 with GPU_ERR_NOT_IMPLEMENTED set in v3.0   │
└────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─ mabda public dispatcher ──────────────────────────────────┐
│  gpu_surface_acquire(ctx, surface) → fb_ptr                │
│  gpu_surface_present(ctx, surface) → 0|err                 │
│  gpu_surface_release(ctx, surface) → 0                     │
│                                                            │
│  These use the slot abstraction directly — no backend-     │
│  specific entry needed because by configure time, all the  │
│  backend-specific state lives on `surface`.               │
└────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─ Backend slot table ───────────────────────────────────────┐
│  backend.surface_configure(ctx, w, h) → surface_ptr        │
│  backend.surface_acquire(ctx, surface) → fb_ptr            │
│  backend.surface_present(ctx, surface) → 0|err             │
│  backend.surface_release(ctx, surface) → 0                 │
│                                                            │
│  The configure-fn reads pre-stashed state from ctx (the    │
│  consumer-passed handles set by the backend-specific       │
│  configure entry). After configure, surface holds          │
│  everything; later slot calls don't need ctx hooks.        │
└────────────────────────────────────────────────────────────┘
```

### Why "stash on ctx" rather than passing through every slot

Two reasons:

1. **Slot signatures stay stable.** The 7.5 slot table commits to
   `(ctx, w, h)` for `configure`. Adding a `void*` opaque arg
   would force a layout bump and break the abstraction's "every
   slot fits in fncall5" promise.
2. **Configure is a one-shot.** The wgpu surface handle / card_fd
   is read once during configure and then lives on the
   `surface_ptr` struct. acquire/present/release don't need it
   again.

GpuContext layout cost: one new u64 slot for the consumer-stashed
handle, set/cleared by the backend-specific configure entry.
GpuContext is currently 96 bytes; growing to 104 is a one-line
ABI change with the standard "every alloc(N) site grows"
migration.

## Native path — TTY/kiosk vs logind tradeoffs

### TTY/kiosk model (v3.0 ship target)

**Caller owns the master fd lifecycle.** Public entry:

```cyrius
fn gpu_surface_configure_native_kiosk(ctx, card_fd, w, h) → surface_ptr
```

- Caller opens `/dev/dri/cardN`, calls `DRM_IOCTL_SET_MASTER`,
  passes the resulting fd to mabda.
- mabda runs the modeset (7.2(d)), allocates two FBs (7.4(b)),
  stores everything on `surface_ptr`.
- On `surface_release`, mabda releases mabda-owned resources;
  caller separately closes the card fd + drops master.

**Pros:**
- No new dependencies. Pure Cyrius + existing DRM ioctls.
- Matches the `programs/native_kms_modeset_smoke.cyr`
  pre-existing shape — code we already have.
- Honest: the caller (kiosk app, aethersafta-like compositor,
  embedded display) typically owns the entire display anyway.

**Cons:**
- Doesn't run inside a desktop session by design. Documented as
  "kiosk / TTY app" use case.
- Caller has to know about DRM master, vt-switching, logind
  TakeControl semantics if integrating with a session manager.

**Implementation cost:** ~50 lines of Cyrius. Existing primitives
(7.2(d) `native_kms_modeset_first_connected`, 7.4(b)
`native_kms_alloc_fb` + `native_kms_present`) do all the heavy
lifting.

### Logind-aware model (v3.x)

**mabda asks systemd-logind for a master fd via dbus
`TakeDevice()`.** Public entry:

```cyrius
fn gpu_surface_configure_native_logind(ctx, w, h) → surface_ptr
```

- mabda calls `org.freedesktop.login1.Manager.GetSession` to find
  the caller's session.
- Calls `org.freedesktop.login1.Session.TakeDevice(major, minor)`
  on the DRM device — logind hands back a fd with master
  already granted.
- Listens for `PauseDevice` / `ResumeDevice` signals to
  cooperatively yield master on vt-switch.
- Calls `ReleaseDevice` on teardown.

**Pros:**
- Works inside a running desktop session. No vt-switching
  required.
- The "right" answer for production graphics on systemd Linux —
  this is what wlroots, KWin, mutter all do.
- Cooperative: vt-switching to a different session pauses our
  scanout cleanly.

**Cons:**
- **Requires dbus.** Cyrius has no dbus library; the dbus wire
  protocol is non-trivial (auth via SASL EXTERNAL, message
  marshalling with type signatures, header field encoding).
- Two implementation options:
  - **C shim** wrapping libsystemd's `sd_bus_*` API. Adds a
    libsystemd link dep to mabda. ~200 lines of C + Cyrius
    bindings.
  - **Hand-rolled dbus** in pure Cyrius. ~500–1000 lines for
    the subset we need (auth + method calls + signal listening).
    No new C deps but significant code volume.
- Either path is **multi-week**, not session-bite scoped.

**Implementation cost:** Multi-week. Out of scope for v3.0.

### Recommendation

**Ship both entry points in v3.0; TTY/kiosk fully implemented,
logind as a stub.**

- `gpu_surface_configure_native_kiosk(ctx, card_fd, w, h)` —
  fully implemented in v3.0. Documented as "for kiosk apps,
  embedded displays, headless tests, and compositor backends
  that own the display."
- `gpu_surface_configure_native_logind(ctx, w, h)` — stub in
  v3.0; sets `GPU_ERR_NOT_IMPLEMENTED` and returns 0. Public
  shape committed; v3.x fills in the dbus body without API
  break.

Why both: keeps the surface API forward-compatible. Consumers
that want logind today fall back to `_kiosk` + acquiring master
themselves (or wait for v3.x). When dbus support lands,
swapping `_kiosk` → `_logind` is a one-line change.

## Wgpu path

`WGPUSurface` is created from a window handle by the consumer
(typically via winit, sdl, gtk, or a hand-rolled X11/Wayland
client). mabda treats it as opaque.

**Public entry:**

```cyrius
fn gpu_surface_configure_wgpu(ctx, wgpu_surface, w, h) → surface_ptr
```

- Caller has already obtained a `WGPUSurface` from
  `wgpu_instance_create_surface`. Mabda's existing `src/surface.cyr`
  already documents this as the consumer's responsibility.
- mabda stashes `wgpu_surface` on `ctx`, calls the slot's
  `surface_configure(ctx, w, h)`, slot reads back the wgpu
  surface, runs `wgpu_surface_configure` + builds the v3 surface
  state.
- The existing `surface_state_*` API in `src/surface.cyr` stays
  in tree as the lower-level mechanism the v3 wgpu wrapper
  delegates to.

This is the only wgpu entry needed; there's no
`_wgpu_logind`-style alternative. wgpu doesn't care about DRM
master because it goes through Vulkan WSI / X11 / Wayland —
those handle their own session/master semantics.

## API surface sketch

```cyrius
# src/surface_v3.cyr (new module — separate from src/surface.cyr
# which remains the v2.x wgpu lifecycle)

# ================================================================
# Backend-specific configure entries
# ================================================================

# Wgpu: caller pre-creates WGPUSurface from their window.
fn gpu_surface_configure_wgpu(ctx, wgpu_surface, width, height) → surface_ptr;

# Native TTY/kiosk: caller pre-acquires master on card_fd.
fn gpu_surface_configure_native_kiosk(ctx, card_fd, width, height) → surface_ptr;

# Native logind (v3.x stub): mabda asks logind via dbus.
# Returns 0 in v3.0 (GPU_ERR_NOT_IMPLEMENTED).
fn gpu_surface_configure_native_logind(ctx, width, height) → surface_ptr;

# ================================================================
# Generic per-frame API (backend-agnostic)
# ================================================================

# Get the back-buffer to render into.
fn gpu_surface_acquire(ctx, surface) → fb_ptr;

# Submit the back-buffer for display.
fn gpu_surface_present(ctx, surface) → 0|err;

# Tear down the surface (does NOT close consumer-supplied fds).
fn gpu_surface_release(ctx, surface) → 0;
```

**Total: 6 public fns.** 3 backend-specific configure entries +
3 generic per-frame ops.

## Slot impl sketch

### `_backend_wgpu_surface_configure(ctx, w, h)`

```cyrius
fn _backend_wgpu_surface_configure(ctx, w, h) {
    var wgpu_surface = gpu_ctx_wgpu_surface_handle(ctx);
    if (wgpu_surface == 0) { return 0; }  # caller didn't pre-set
    var device = gpu_ctx_device(ctx);
    var format = WGPU_TEXTURE_FORMAT_RGBA8_UNORM;
    var ss = surface_state_new(wgpu_surface, device, format,
                                w, h, PRESENT_VSYNC);
    return ss;
}

fn _backend_wgpu_surface_acquire(ctx, surface) {
    return surface_state_acquire(surface);
}

fn _backend_wgpu_surface_present(ctx, surface) {
    return surface_state_submit_present(surface);
}

fn _backend_wgpu_surface_release(ctx, surface) {
    return surface_state_release(surface);
}
```

Reuses `src/surface.cyr`'s existing `surface_state_*` API. The
wgpu wrappers are ~20 lines total.

### `_backend_native_surface_configure(ctx, w, h)`

```cyrius
fn _backend_native_surface_configure(ctx, w, h) {
    var card_fd = gpu_ctx_native_card_fd(ctx);
    if (card_fd <= 0) { return 0; }   # caller didn't pre-set
    var render_fd = native_ctx_fd(ctx);

    var ns = alloc(NATIVE_SURFACE_SIZE);
    memset(ns, 0, NATIVE_SURFACE_SIZE);

    # 1. Discover topology
    var state = native_kms_init(card_fd);
    if (state == 0) { return 0; }
    store64(ns + NATIVE_SURFACE_STATE, state);

    # 2. Modeset on first connected connector
    var scanout[40];
    var rc = native_kms_modeset_first_connected(card_fd, render_fd,
                                                state, &scanout);
    if (rc != 0) { return 0; }
    memcpy(ns + NATIVE_SURFACE_SCANOUT, &scanout, 40);

    # 3. Allocate the second FB for double-buffering
    var fb_b[32];
    var fb_w = load32(&scanout + NATIVE_KMS_SCANOUT_WIDTH);
    var fb_h = load32(&scanout + NATIVE_KMS_SCANOUT_HEIGHT);
    rc = native_kms_alloc_fb(card_fd, render_fd, fb_w, fb_h, &fb_b);
    if (rc != 0) { ... return 0; }
    memcpy(ns + NATIVE_SURFACE_FB_B, &fb_b, 32);

    store32(ns + NATIVE_SURFACE_CARD_FD,   card_fd);
    store32(ns + NATIVE_SURFACE_RENDER_FD, render_fd);
    store32(ns + NATIVE_SURFACE_FRONT_IS_B, 0);   # scanout (fb_a) is front initially
    return ns;
}

fn _backend_native_surface_acquire(ctx, surface) {
    # Return back-buffer's mapped_addr region
    var front_is_b = load32(surface + NATIVE_SURFACE_FRONT_IS_B);
    if (front_is_b != 0) {
        # fb_b is front → return scanout's address
        return surface + NATIVE_SURFACE_SCANOUT;
    }
    # scanout (fb_a) is front → return fb_b
    return surface + NATIVE_SURFACE_FB_B;
}

fn _backend_native_surface_present(ctx, surface) {
    var card_fd = load32(surface + NATIVE_SURFACE_CARD_FD);
    var front_is_b = load32(surface + NATIVE_SURFACE_FRONT_IS_B);
    var back_fb_id;
    if (front_is_b != 0) {
        back_fb_id = load32(surface + NATIVE_SURFACE_SCANOUT
                            + NATIVE_KMS_SCANOUT_FB_ID);
    }
    if (front_is_b == 0) {
        back_fb_id = load32(surface + NATIVE_SURFACE_FB_B
                            + NATIVE_KMS_FB_FB_ID);
    }
    var seq[4];
    var rc = native_kms_present(card_fd,
                                surface + NATIVE_SURFACE_SCANOUT,
                                back_fb_id, &seq);
    if (rc != 0) { return rc; }
    # Toggle which is front
    if (front_is_b != 0) { store32(surface + NATIVE_SURFACE_FRONT_IS_B, 0); }
    if (front_is_b == 0) { store32(surface + NATIVE_SURFACE_FRONT_IS_B, 1); }
    return 0;
}

fn _backend_native_surface_release(ctx, surface) {
    var card_fd = load32(surface + NATIVE_SURFACE_CARD_FD);
    var render_fd = load32(surface + NATIVE_SURFACE_RENDER_FD);
    var state = load64(surface + NATIVE_SURFACE_STATE);
    native_kms_release_fb(card_fd, render_fd, surface + NATIVE_SURFACE_FB_B);
    native_kms_release_scanout(card_fd, render_fd,
                                surface + NATIVE_SURFACE_SCANOUT);
    native_kms_release(state);
    memset(surface, 0, NATIVE_SURFACE_SIZE);
    return 0;
}
```

`NativeSurface` struct (~120 bytes): card_fd / render_fd /
state ptr / embedded scanout (40 B) / embedded fb_b (32 B) /
front_is_b flag.

**Total: ~150 lines for the native side.** Heavy reuse of
7.2(d) + 7.4(b) primitives.

## E2e program shape

`programs/native_present_e2e.cyr` — visible double-buffered
present. Same architecture as `programs/native_kms_modeset_smoke.cyr`
but with the public `gpu_surface_*` API and a multi-frame loop.

```
1. Open card_fd, drm_set_master(card_fd)
2. ctx = gpu_context_new_native()
3. surface = gpu_surface_configure_native_kiosk(ctx, card_fd, w, h)
4. for frame = 0..120:
     fb = gpu_surface_acquire(ctx, surface)
     # Fill fb's mapped_addr with frame-dependent pattern
     # (e.g., gradient that scrolls across frames)
     gpu_surface_present(ctx, surface)
5. gpu_surface_release(ctx, surface)
6. drm_drop_master(card_fd); close(card_fd)
7. gpu_context_release(ctx)
```

120 frames at 60 Hz = 2 seconds of visible animated content.
Run from a tty after stopping the desktop session, or from a
clean kiosk image.

Exit codes 0–N map to specific failure stages (matching the
kiosk smoke's pattern).

## Migration / consumer guidance

| Consumer type | Path |
|---|---|
| **soorat / kiran (wgpu)** | Use `gpu_surface_configure_wgpu`. WGPUSurface obtained from their window via existing wgpu_instance_create_surface; no change in winit / gtk / sdl integration. |
| **aethersafta (native compositor)** | Use `gpu_surface_configure_native_kiosk`. Aethersafta is the display owner; it manages master itself. This is its native model. |
| **rasa / ranga (image / compute)** | No surface needed — these are compute-heavy. The surface API is opt-in. |
| **bijli (EM sim)** | Same — compute-only, no surface. |

Only consumers that need to PRESENT TO SCREEN (soorat, kiran,
aethersafta) touch this API. The other three keep using the
existing compute / texture surface they already have.

## v3.x logind realization — `samvada` package

When the logind path graduates from stub to real, the dbus
client lives in a **separate Cyrius package named `samvada`**
(Sanskrit *saṃvāda* "dialogue," dropped to ASCII for
filesystem/package-manager friendliness) alongside the
existing AGNOS deps (`sakshi`, `patra`, `sigil` — same shape:
own repo, authored as `src/*.cyr`, bundled via
`cyrius distlib` into `dist/samvada.cyr`, consumed by mabda
through `[deps.samvada]` in `cyrius.cyml` which symlinks the
bundle into the consumer's `lib/samvada.cyr`). Not in mabda's
tree (mabda is a GPU library; dbus is wrong owner) and not in
cyrius stdlib (stdlib is for "every Cyrius program needs this"
— dbus is system-services protocol). **Scope for the logind
subset**: ~500–1000 lines of pure Cyrius covering system-bus
socket connect, SASL EXTERNAL auth, message marshalling (header
fields, type signatures, alignment), method-call + signal
handling, and a minimal type system (int32 / uint32 / string /
object_path / unix_fd). Pure-Cyrius posture matches mabda's
"own the stack" stance — the C-shim-around-libsystemd
alternative would be ~200 LoC faster but adds an external
link dep to mabda. The mabda surface API does **not** change
when this lands: only `_backend_native_surface_configure`'s
logind branch swaps from "return `GPU_ERR_NOT_IMPLEMENTED`" to
real `samvada` calls. Consumers that called
`gpu_surface_configure_native_logind` in v3.0 (and saw the stub
error) Just Work in v3.x without code changes. Package scaffold
is filed as a Tier 6 follow-up in the v3.0 punchlist; the
scaffold itself (empty repo, cyrius.cyml, CI stub) is a
v3.0-era task — real protocol implementation is multi-week
v3.x work.

## Open questions for review

1. **GpuContext layout extension.** Adding fields for
   `wgpu_surface_handle` + `native_card_fd` grows GpuContext
   96 → 112 bytes. Acceptable? Or use a side struct
   (`GpuSurfaceContext`) reachable from ctx via a single ptr?
   Recommendation: extend GpuContext directly — single greppable
   migration matches the precedent set by 4f.iv (32 → 96).
2. **`surface_present` blocking semantics.** 7.4(b)'s
   `native_kms_present` blocks on the vblank event. The wgpu
   path's `wgpu_surface_present` is async (queue submit). Should
   the public `gpu_surface_present` be:
   - (a) Always-blocking — wgpu wrapper polls for completion.
     Simple semantics, slightly worse perf.
   - (b) Always-async — native wrapper returns immediately, then
     a separate `gpu_surface_wait_present` blocks. More work
     for consumers, better perf.
   - (c) Configurable — the configure entry takes a present-mode
     arg.
   Recommendation: **(a) always-blocking** for v3.0. (c) is
   v3.x perf. The wgpu wrapper doesn't actually need polling —
   wgpu_surface_present is already "queued, returns immediately"
   semantics that the kernel completes asynchronously; treating
   it as "complete-on-return" is the wgpu lib's convention
   anyway.
3. **Format choice.** Both backends ship XRGB8888 / RGBA8_UNORM
   only in v3.0. Format negotiation (HDR, 10-bit, etc.) is
   v3.x. OK?
4. **Multi-monitor.** Native side picks the first connected
   connector. No multi-output story in v3.0. OK?

## Summary

- **Both paths supported in v3.0**, but only TTY/kiosk
  fully implemented on the native side. Logind stub committed
  for v3.x.
- **6 public fns**: 3 backend-specific configure entries
  (`_wgpu`, `_native_kiosk`, `_native_logind`) + 3 generic
  per-frame ops (`_acquire`, `_present`, `_release`).
- **Slot impls reuse existing primitives**: ~20 lines wgpu
  (delegates to `src/surface.cyr`), ~150 lines native (uses
  7.2(d) + 7.4(b)).
- **GpuContext grows** 96 → 112 bytes for the consumer-stashed
  wgpu_surface + card_fd fields.
- **E2e program** demonstrates 120-frame animated present from
  a tty / kiosk environment.
- **Total v3.0 work**: ~250 lines of Cyrius, ~30 CPU asserts,
  one new program, one CHANGELOG entry. Ship-blockers: none.

---

**Reviewer prompts:**
- Is the layered API the right shape, or should `gpu_surface_configure`
  just take a discriminator enum and a void* opaque?
- GpuContext extension or sibling struct?
- Which open question (1–4) needs deeper discussion before code lands?
