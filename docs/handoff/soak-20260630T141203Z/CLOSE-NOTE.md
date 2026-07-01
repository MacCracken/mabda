# 24h nvidia-native soak — close note (2026-07-01)

**Verdict: PASS (GA burn-in gate met).** The GPU workload ran clean for the
full duration; the run did not write its final milestone row due to a
harness bug (now fixed), not a workload failure.

## What ran
- `scripts/soak.sh --workload=nvidia-native --stop=24h` on the TU116
  (GTX 1660 SUPER, nouveau, renderD128 — masterless).
- Loops: `nvidia-compute` + `nvidia-render` + `nvidia-texture`.
- Started 2026-06-30T14:12:03Z.

## Result
- **103 of 104 checkpoints PASS**, covering 1s → 23h45m (85500s).
- **RSS dead-flat at 13,528 KB** — a single distinct value across every
  checkpoint. No leak.
- **dmesg Δ=0** throughout — no `nouveau|drm|Xid|GSP` line, no GSP-RM Xid
  fault (the nouveau analogue of an amdgpu ring TDR).
- **~8.69M total dispatches, 0 workload failures** at the 23h45m mark:
  compute 2,969,810 · render 2,980,259 · texture 2,744,042.
- All three per-workload statuses `ok` at teardown; no `dmesg.diff`.

## Why there is no 86400s / "1d0h" row or "soak end" line
The final checkpoint's label formatting hit a quoting bug in
`fmt_elapsed`'s ≥1-day branch:

    printf "%dd%dh" "$((s / 86400)) $((s % 86400 / 3600))"   # one arg "1 0"

Both arithmetic expansions sat inside one quoted string, so `printf` got a
single argument `"1 0"` and `%d` rejected the embedded space
(`line 398: printf: 1 0: invalid number`). Under `set -euo pipefail`, the
failing `printf` inside `label=$(fmt_elapsed "$ms")` aborted the script at
the close — before the 24h row, the "soak end" line, and teardown. The
per-workload loop subshells then self-exited via their
`kill -0 "$MONITOR_PID"` guard (nothing orphaned — the safety path worked).

This branch only fires at `s >= 86400`, so it never affected the 1h/6h runs
and would additionally have crashed every 72h-window checkpoint.

**Fixed** in `scripts/soak.sh` (two separate quoted args):

    printf "%dd%dh" "$((s / 86400))" "$((s % 86400 / 3600))"

Verified labels: 86400→`1d0h`, 108000→`1d6h`, 172800→`2d0h`, 259200→`3d0h`.

## dmesg capture without sudo
`kernel.dmesg_restrict=1` on this box, so bare `dmesg -t` fails for the
user; the script's `journalctl -k` fallback reads the kernel log as the
user, so the regression detector stayed live without `sudo` (baseline 18
`nouveau/drm/Xid` lines, Δ=0 to the end).

## Cumulative burn-in evidence for the GA gate
Three sequential clean `nvidia-native` runs, all RSS-flat / dmesg Δ=0:
- 1h  — `docs/handoff/soak-20260630T071130Z` (13 checkpoints, ~367K dispatches)
- 6h  — `docs/handoff/soak-20260630T081147Z` (33 checkpoints, ~2.19M dispatches)
- 24h — this run (103 checkpoints to 23h45m, ~8.69M dispatches)

≈ 30¾h cumulative, 0 failures. Gate accepted on this evidence (per owner,
2026-07-01) rather than re-running for the missing final row.
