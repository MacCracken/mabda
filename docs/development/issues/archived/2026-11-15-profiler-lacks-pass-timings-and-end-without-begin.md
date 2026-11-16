# The profiler has no pass timings, and `profiler_end_frame` without a `profiler_begin_frame` times from clock zero — RESOLVED

**Status:** ✅ **RESOLVED in mabda 4.3.0** (2026-11-16). Every item, against Rust mabda 1.0's `FrameProfiler` source: `profiler_record_pass` / `_pass_count` / `_pass` / `_total_pass_time_ms` and `pass_timing_*` (cleared by `begin_frame` and `reset`; one reused buffer, so steady-state recording allocates nothing); `profiler_end_frame` with no frame begun returns 0.0 (the frame still counted and recorded; the start is cleared at every end); `profiler_best_frame_ms` on an empty history returns `f64::MAX`'s bits; `profiler_frame_history`, `profiler_export_json`, `profiler_export_history_csv` (1.0's formats), and `profile_end_record` (1.0's `ProfileScope`). soorat's four tests (`profiler_record_pass`, `profiler_begin_clears_passes`, `pass_timing_fields`, `profiler_end_without_begin`) are ported verbatim into `tests/tcyr/core.tcyr`. Was: open.
**Filed:** 2026-11-15, from soorat's Cyrius port, M4 part d (`soorat/docs/development/port-parity.md` §5.5;
ledger row L24).
**Placement:** `src/profiler.cyr` (`profiler_with_alpha_and_history`, `profiler_begin_frame`, `profiler_end_frame`,
`profiler_best_frame_ms`).
**Severity:** **Medium** for soorat: its 1.2.0 API re-exports Rust mabda 1.0's `FrameProfiler`, and four of its
eight profiler tests depend on the behaviour below.
**Affects:** mabda 4.2.0 (`HEAD` 2270be9).

## Summary

`src/profiler.cyr` says it is ported from `mabda/src/profiler.rs`, with "pass timing". Against Rust mabda 1.0.0's
`FrameProfiler`:

1. **No pass timings.** 1.0 keeps `pass_times: Vec<PassTiming>`, appends with `record_pass(label, duration_ms)`,
   sums with `total_pass_time_ms()`, and clears the list in `begin_frame` and `reset`. 4.2.0 documents a 24-byte
   `PassTiming` layout (label, label_len, duration_ms) but has no list, no `record_pass` and no total.
2. **`end_frame` without `begin_frame`.** 1.0 holds the start as an `Option<Instant>`, cleared at the end of every
   frame, so an `end_frame` with no matching `begin_frame` times the frame as **0 ms** (and still counts it, updates
   the average and records 0 in the history). 4.2.0 keeps the start in `+0` and never clears it: before any
   `begin_frame` it times from 0 ns, so the "frame" is the monotonic clock's whole uptime; after one, it times from
   that stale start.
3. **`best_frame_ms` with no history.** 1.0 folds from `f64::MAX`, so an empty history gives `f64::MAX`; 4.2.0 gives
   0. (`worst_frame_ms` agrees: 0 either way.)
4. **Missing accessors:** `frame_history()` (oldest first), `export_json()` and `export_history_csv()`.
   `ProfileScope` (RAII) is `profile_begin`/`profile_end`, which fits Cyrius, but `profile_end` returns the duration
   without recording it, where 1.0's scope records it as a pass on drop.

soorat's tests that need 1 and 2 (Rust 1.2.0's `src/profiler.rs`): `profiler_record_pass` (three passes, total
2.8 ms), `profiler_begin_clears_passes`, `pass_timing_fields` and `profiler_end_without_begin` (`end_frame` on a new
profiler returns exactly 0.0).

## Suggested fix

- A pass list in the profiler (a vec of 24-byte `PassTiming` records): `profiler_record_pass(p, label, label_len,
  duration_ms)`, `profiler_pass_count(p)`, `profiler_pass(p, i)`, `profiler_total_pass_time_ms(p)`; cleared by
  `profiler_begin_frame` and `profiler_reset`. A `profile_end_record(p, start_ns, label, label_len)` would give
  `ProfileScope`'s recording form.
- A started flag (or a sentinel start): `profiler_end_frame` returns 0.0 when no frame was begun, and clears the
  flag, as 1.0's `frame_start = None`.
- `profiler_best_frame_ms` returns `f64::MAX`'s bits (`0x7FEFFFFFFFFFFFFF`) on an empty history, or document the 0.
- `profiler_frame_history(p)` (the vec, oldest first); the two exports if a consumer asks.

## Measure / close

soorat's `tests/profiler.tcyr` (M6) ports the eight tests over `profiler_*`; with this fixed they pass without a
soorat-side stand-in.
