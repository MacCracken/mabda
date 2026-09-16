#!/usr/bin/env python3
"""Report device-local memory per bench-gpu CSV row from a memtrack log, and fail when a row
grows it by more than --max-row-growth-mib (default 256 = one allocator block of slack)."""
import argparse, re, sys

ap = argparse.ArgumentParser()
ap.add_argument("--max-row-growth-mib", type=float, default=256.0)
ap.add_argument("logs", nargs="+")
args = ap.parse_args()
bad = 0
for path in args.logs:
    livedl = 0
    prev_dl = None
    start_dl = None
    rows = []
    allocs = frees = failed = 0
    active = False
    for line in open(path, errors="replace"):
        if "MEMTRACK layer active" in line:
            active = True
        m = re.search(r"MEMTRACK (alloc|free) .*live=(\d+) live_devlocal=(\d+)", line)
        if m:
            livedl = int(m.group(3))
            if m.group(1) == "alloc":
                allocs += 1
            else:
                frees += 1
            continue
        if "alloc-FAILED" in line:
            failed += 1
        if line.startswith("=== mabda GPU benchmarks"):
            start_dl = livedl
            prev_dl = livedl
        m = re.match(r"CSV:([a-z0-9_]+),(\d+)", line)
        if m and prev_dl is not None:
            rows.append((m.group(1), int(m.group(2)), livedl, livedl - prev_dl))
            prev_dl = livedl
    print("==", path)
    if not active:
        print("  FAIL: the memtrack layer never loaded")
        bad += 1
        continue
    print("  vkAllocateMemory %d | vkFreeMemory %d | failed allocs %d" % (allocs, frees, failed))
    print("  device-local live: start %.1f MiB, end %.1f MiB" % ((start_dl or 0) / 2**20, livedl / 2**20))
    for name, ns, dl, growth in rows:
        flag = ""
        if growth / 2**20 > args.max_row_growth_mib:
            flag = "  <-- FAIL: grew past the limit"
            bad += 1
        print("  %-28s devlocal=%8.1f MiB  row_growth=%+8.1f MiB%s" % (name, dl / 2**20, growth / 2**20, flag))
    if not rows:
        print("  FAIL: no CSV rows found")
        bad += 1
sys.exit(1 if bad else 0)
