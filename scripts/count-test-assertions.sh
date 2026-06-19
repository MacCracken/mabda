#!/usr/bin/env bash
# count-test-assertions.sh — reliably sum CPU-test assertions across tests/tcyr/.
#
# WHY THIS EXISTS: tests/tcyr/texture.tcyr emits its "<N> passed, 0 failed"
# summary line with a LEADING NUL BYTE. POSIX grep/awk treat any line containing
# a NUL as *binary* and silently drop it, so the obvious one-liner
#     make test | grep -oE '[0-9]+ passed'
# undercounts the total by exactly texture.tcyr's 207 (reporting 3776 instead of
# the true 3983). This has fooled humans and review agents alike. This script
# strips NULs (tr -d '\0') before counting and runs each file standalone, so the
# total is correct. Run from the repo root.
set -euo pipefail

cd "$(dirname "$0")/.."

total=0
files=0
for f in tests/tcyr/*.tcyr; do
  out="$(cyrius test "$f" 2>&1 | tr -d '\0')"
  if echo "$out" | grep -qE '[0-9]+ failed' && ! echo "$out" | grep -qE ' 0 failed'; then
    echo "FAIL: $f"
    echo "$out" | grep -E 'passed|failed' || true
    exit 1
  fi
  n="$(echo "$out" | grep -oE '[0-9]+ passed, 0 failed' | tail -1 | grep -oE '^[0-9]+' || echo 0)"
  printf '%-26s %5d\n' "$(basename "$f" .tcyr)" "${n:-0}"
  total=$((total + ${n:-0}))
  files=$((files + 1))
done

printf '%-26s %5d  (across %d files)\n' "TOTAL" "$total" "$files"
