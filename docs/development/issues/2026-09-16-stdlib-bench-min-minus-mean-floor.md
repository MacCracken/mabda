# `lib/bench.cyr` subtracts a mean timer floor from every window, reports the minimum, and calibrates the floor only once

**Status:** open upstream. Filed in the cyrius repo as `cyrius/docs/development/issues/2026-09-16-mabda-lib-bench-min-minus-mean-floor.md`. mabda works around it in `programs/benchmarks.cyr` (4.1.3). `tests/bcyr/mabda.bcyr` is only
marginally affected; see "Impact on mabda".
**Discovered:** 2026-09-16, mabda 4.1.3 verification. `make bench-gpu` on the Cezanne dev box
(hpet clocksource, floor 1.274 µs) printed `uniform_buffer_write: 2.825us avg (min=0ns ...)`
and `CSV:uniform_buffer_write,0`.
**Toolchain:** `cyrius 6.6.4`. mabda's `lib/bench.cyr` is byte-identical to cyrius
`lib/bench.cyr` at `HEAD` `4f3731e8`.
**Component:** stdlib (`lib/bench.cyr`)
**Severity:** Medium for benchmark consumers, none at runtime. It never affects shipped code.
It produces **wrong benchmark numbers**: minima that read low or 0, and, with an inflated
floor, averages that read low. A regression gate or history built on those rows follows the
clock rather than the code.

Log paths below are in the 4.1.3 verification scratchpad (`verify-logs/`), not in the repo.

## Summary

Three related defects.

### 1. A minimum over windows, net of a mean floor

`bench_clock_overhead_ns()` (`lib/bench.cyr:141`) is the floor. `_bench_calibrate_clock`
(`:112-137`) takes the fastest of 5 batches of 1,024 clock reads, subtracts the fastest empty
loop, and divides by 1,024. That is the **mean** cost of one read in a quiet batch.
`_bench_net` (`:148`) subtracts it from **every** timed window, clamped at 0. Then:

- `bench_stop` (`:186`), `bench_batch_stop` (`:381`) and `bench_run_batch*` (`:296-356`) keep
  `min_ns`, the smallest net window per op.
- `bench_report` (`:470`) prints that minimum next to the average, and `bench_min_ns` (`:202`)
  hands it to callers (mabda's `CSV:` rows record it).

A window's own clock cost varies from read to read. The minimum over windows of
`op + c_i − mean(c)` is `op − (mean(c) − min(c_i))`, so it reads low by up to that spread, and
reads 0 when the spread exceeds the op. The average stays unbiased. The error per op is divided
by the ops in the window, so single-op windows (`bench_start` / `bench_stop`, or
`bench_batch_stop(b, 1)`) take all of it.

Measured with `lib/bench.cyr` itself (`verify-logs/r2-align/50-benchprobe-local-hpet.log`,
3 runs; the dev box, hpet clocksource):

| | runs 1 / 2 / 3 |
| --- | --- |
| calibrated floor | 1,214 / 1,211 / 1,208 ns |
| one empty window, min / mean | 489 / 1,214; 489 / 1,220; 838 / 1,216 ns |
| empty windows below the floor | 9,460 / 9,193 / 9,403 of 20,000 |
| `getpid` timed one per window (`bench_start`/`bench_stop`), min / avg | **0** / 247; **0** / 356; **0** / 295 ns |
| `getpid`, 128 per window (`bench_batch_stop`), min / avg | 261 / 268; 266 / 278; 258 / 268 ns |

On chew (tsc, window jitter about 5 ns) the same probe shows no bias: one per window reads
min 705 to 709 ns against 708 to 710 ns batched (`r2-align/chew/01-benchprobe-chew-tsc.log`).
The defect scales with clock jitter, which is why it appears on one box and not the other.

`bench_run` (`:242`) is largely immune by construction. It sizes chunks so the floor is at most
1 % of a window, which divides the bias by the chunk size.

### 2. The floor is calibrated once, at first use, and never checked

`bench_clock_overhead_ns` calibrates lazily on its first call and caches the result for the
life of the process. Nothing compares it with the windows actually being measured. If the
~4-6 ms calibration runs during a slow moment, every row of that process subtracts too much,
and the clamp at 0 then pulls **averages** down too, not just minima.

Observed once in the 4.1.3 round-1 verification on chew (tsc)
(`fix-wgpu-programs/06-clock-opprobe-chew-tsc.log`): the calibration returned **3,861 ns**.
The same process measured empty windows at min 727 and mean 1,207 ns. 19,955 of 20,000 empty
windows fell under the floor, and a modelled 700 ns op clamped to 0 in 19,405 of 20,000
windows. The next run on the same boot calibrated 733 ns.

The trigger is **not established**. Round 1 attributed it to a cold CPU, but the round-2
reruns did not reproduce it:

- 3 local runs after 3 s idle, calibrating within 0.5 % of the value after a 500 ms warm-up.
- 3 + 5 runs on chew (intel_pstate powersave) after 5-25 s idle
  (`r2-align/chew/01-*.log`, `02-benchprobe-chew-long-idle.log`), all within 0.5 % as well.

The defect does not depend on the cause. A single unguarded calibration applies whatever the
machine was doing in those few milliseconds to every row. The header's warning that "the floor
moves between reboots" covers boots, not a transient inside one process.

### 3. Integer ns per op

`per_op = elapsed / batch_size` (`:306`, `:327`, `:348`, `:384`; `bench_run` `:256`) truncates to whole
nanoseconds. For ops of a few ns, one unit of truncation is a large fraction of the row, and a
sub-ns shift in the subtracted floor can move a row by a whole unit. In the mabda run below,
`workgroups_2d` goes from 7 to 6.

## Impact on mabda

- **`programs/benchmarks.cyr` (`make bench-gpu`): affected, worked around in 4.1.3.** It warms
  the CPU before calibrating and measures clock jitter right before each row. The uniform-write
  row runs K ops per window, sized so jitter is ≤1 % of a window. A row that reads 0 ns fails
  the run (exit 1). The resource-creating rows must stay at one op per window, so each prints
  its under-read bound (`per-op min may under-read by <= X ns`). The Cezanne run of the new
  benchmarks is still open verification (round-1 review).
- **`tests/bcyr/mabda.bcyr` (`make bench`, CI `Bench` step): marginally affected. No row can
  read 0.** Every row times a batch (10,000 ops per window, or 100 for the two
  `rg_plan_aliasing_stats_*` rows), and the CSV records `bench_min_ns`. Measured on the hpet
  box over 5 interleaved runs with the library floor, no subtraction, and the round-1 inflated
  floor forced to 3,861 ns (`verify-logs/r2-align/53-mabda-bcyr-floor-sensitivity-local-5runs.log`):
  - **10,000-op rows:** within run-to-run spread under every variant, except `workgroups_2d`,
    which reads 6 instead of 7 with the inflated floor (the truncation effect in §3). The worst
    case is (floor − fastest window clock) / 10,000: ≤ 0.07 ns per op with the library floor,
    ≤ 0.34 ns with 3,861 ns.
  - **`rg_plan_aliasing_stats_5` (100 ops per window):** median 827 ns with the library floor,
    799 ns with 3,861 ns. That is −28 ns (−3.4 %), matching the model
    (3,861 − 1,212) / 100 ≈ 26 ns. With the library floor the bias bound is (1,212 − 489) / 100
    ≈ 7 ns (≤ 0.9 %).
  - **`rg_plan_aliasing_stats_30`:** its ~250 ns run-to-run spread swamps a ≤ 26 ns effect.
  - So `bench-history.csv` rows from `mabda.bcyr` are trustworthy to a few percent. The two
    `rg_plan` rows are the ones to discount if a run's printed floor looks inflated against
    earlier runs on the same boot.

## Reproduction (CPU only)

`verify-logs/r2-align/tools/benchprobe.cyr`, built from a mabda checkout with
`cyrius build benchprobe.cyr benchprobe`. It includes `src/lib.cyr` and `lib/bench.cyr`, and
does four things:

1. Calls `_bench_calibrate_clock()` at process start, again after 500 ms of busy work, and a
   third time.
2. Times 20,000 empty `now_ns()` pairs.
3. Sets `_bench_clock_ns` to the warm value and times `syscall(39)` (getpid) with
   `bench_start`/`bench_stop` (one per window) and with `bench_batch_start`/`bench_batch_stop`
   (128 per window).
4. Repeats step 3 with the process-start value.

On an hpet box, step 3's one-per-window row reports `min=0ns` while its average and the
batched row agree at about 260-360 ns.

## Expected vs actual

- **Expected:** a reported minimum is a lower bound on the op, not on the op minus clock jitter.
  The floor subtracted from a process's rows reflects the conditions those rows ran in.
- **Actual:** per-window minima read low or 0 whenever window-to-window clock jitter is
  comparable to the op, and a single calibration taken at a slow moment is subtracted from
  every row of the process.

## Suggested upstream fix

1. Do not subtract a mean from a quantity whose minimum you then report. Either subtract the
   **minimum** single-read cost when computing minima (keeping the mean for averages), or
   report `min` only for windows long enough that jitter is ≤ 1 % of them (what `bench_run`
   already does) and print `min=n/a` otherwise.
2. Check the floor against the run. Re-calibrate (or at least compare) when the first report
   prints, and warn when the median empty window of the run differs from the calibrated floor
   by more than a set fraction. Do the warm-up that mabda's `benchmarks.cyr` now does before
   calibrating.
3. Keep per-op results in sub-ns units (for example fixed-point thousandths), or report a
   batch total alongside the per-op integer.
4. Add a gate: on a host whose empty-window jitter exceeds a few hundred ns, a `getpid` timed
   one per window must not report `min=0`.

## Upstream status

Filed 2026-09-16 as `cyrius/docs/development/issues/2026-09-16-mabda-lib-bench-min-minus-mean-floor.md`.
