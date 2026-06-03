# 2026-06-01 — Invalid soak: `soak.sh` ran stale pre-fix binaries

**Severity**: process bug (no shipped-code defect). Cost: ~17h of a
24h GA-gate soak run that validated the wrong bundle.

## Symptom

The rc.4 24h soak launched 2026-06-01 04:59:58Z ran green for 17h
(all checkpoints PASS, RSS flat, dmesg Δ=0) — but was burning in
binaries that did **not** represent the bundle we intend to tag
`3.0.0` GA.

## Root cause

`scripts/soak.sh` built each workload binary **only if it was
missing**:

```sh
for b in "${BINS_TO_BUILD[@]}"; do
    if [ ! -x "$b" ]; then
        make "$b" ...
    fi
done
```

`build/native_compute_store` and `build/native_render_e2e` already
existed in the tree, built **2026-05-13 15:50–15:51Z**. The guard saw
them present and skipped the rebuild, so the soak exec'd those stale
artifacts every iteration.

Two independent staleness problems with those binaries:

1. **Pre-bugfix source.** Commit `6c26765 "squashed the bug"` landed
   **2026-05-13 15:53:26Z** — ~2.5 min *after* the binaries were
   built. It rewrote the native render path
   (`backend_native.cyr`, `backend_native_pm4.cyr`, +96 lines in
   `backend_native_shaders.cyr` — the GFX9 shader/PM4 fix that ended
   the "still blocked" TDR saga). HEAD has the fix; the soaked
   binaries did not.
2. **Superseded toolchain.** The binaries were built on the old
   5.11.x toolchain, not the 6.0.x line the GA bundle pins.

The 17h of green was real but meaningless for GA — it validated
superseded code.

## Fix

`scripts/soak.sh` now invokes `make` unconditionally and lets make's
mtime dependency on `programs/*.cyr` + `src/*.cyr` resolve staleness:

```sh
for b in "${BINS_TO_BUILD[@]}"; do
    log "building $b (make resolves up-to-date)"
    make "$b" >> "$SOAK_LOG" 2>&1 || { log "FAIL: $b did not build"; exit 1; }
done
```

make no-ops when the target is genuinely current, so the common path
stays cheap; a stale-source binary now rebuilds before the loop
starts.

## Residual gap

make tracks source mtimes, not the **toolchain** version. A pin bump
with unchanged source won't bump any file mtime, so make alone won't
catch a pure-toolchain staleness. Mitigation: `rm -f build/<soak
binaries>` (or `make clean`) when bumping the `cyrius.cyml` pin, then
let the soak rebuild fresh. This was done for the 6.0.27 relaunch.

## Relaunch

Killed the invalid run, bumped the pin `6.0.16 → 6.0.27` (latest),
re-resolved deps, removed the stale binaries, rebuilt + smoke-tested
all three programs on 6.0.27 (compute readback `0xDEADBEEF`, render
pixel `(0xFF,0x00,0x00,0xFF)`, wgpu graph 5/5), and relaunched the
24h clock at 2026-06-01 22:26:52Z against the fresh binaries
(`docs/handoff/soak-20260601T222652Z/`).

---

## Amendment 2026-06-02 — monitor process died ~15 min before the finish

The 6.0.27 relaunch ran clean for the full window, but a second
`soak.sh` robustness bug surfaced at the very end.

**Symptom.** The last recorded checkpoint was t=23h (85500s,
22:11:52Z) — PASS, RSS flat 13092 KB, dmesg Δ=0. The 86400s/24h
checkpoint **never logged**, yet the workload loops kept running.
At inspection (00:35Z June 3, ~26h elapsed) the three loop subshells
were alive and **reparented to init** (PPID 1), counters still
climbing (compute 53.9M), all `.status` files `ok`, dmesg Δ still 0.
The **monitor process itself was gone** — and it left no
`interrupted` / `soak end` line, so it died on a signal the
`INT TERM` trap doesn't cover (no journal OOM / segv / kill record).

**Root cause.** `nohup sudo soak.sh` shields only the outer `sudo`,
not the inner monitor bash. A SIGHUP on session teardown (or a
SIGPIPE on the checkpoint `tee`) killed the monitor while its
orphaned workload subshells — already in their own `while :` loops —
survived and ran on silently, recording nothing.

**Impact on the GA gate.** None, substantively. The mabda workloads
demonstrably ran **clean past 24h** (26h13m total, 0% RSS drift, 0
dmesg delta, all readbacks/pixels passing). A manual closeout
snapshot was appended to the logdir's `soak.log` + `soak.csv`
(`94408,26h13m-manual,...,PASS`). The only deficiency is the missing
auto-logged 86400s row — a tooling failure, not a workload failure.
**24h gate: substantively PASS.**

**Fix.** Two changes to `soak.sh`:
1. `trap '' HUP PIPE` near the top — the monitor now survives session
   teardown and a broken `tee` pipe, so a long unattended run records
   its final checkpoint.
2. Workload subshells capture `MONITOR_PID=$$` and `kill -0` it each
   iteration — if the monitor ever dies anyway, the loops self-exit
   instead of orphaning into init as silent forever-daemons.

Validated with a 12s smoke: build → checkpoint ramp → final 12s
checkpoint → `soak end — exit=0` → clean teardown.
