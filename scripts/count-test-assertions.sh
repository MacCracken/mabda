#!/usr/bin/env bash
# count-test-assertions.sh — reliably sum CPU-test assertions across tests/tcyr/.
#
# Usage: scripts/count-test-assertions.sh              count (run from anywhere)
#        scripts/count-test-assertions.sh --self-test  prove the gate logic below
#
# Each suite runs standalone. A suite is counted ONLY when `cyrius test` exits 0 AND
# prints a well-formed "<N> passed, 0 failed (<N> total)" summary. Anything else —
# a non-zero exit (assertion failure, SIGSEGV = 139, timeout), or an exit 0 with no
# summary at all — is a named FAIL and the script exits non-zero. Before 4.1.3 the
# script ran under `set -e` with the exit status folded into a command substitution:
# a crashing or failing suite aborted the run with no FAIL line naming it, and a
# suite that exited 0 without a summary was silently counted as 0.
#
# THE NUL TRAP (history + root cause). Up to cyrius 6.3.14, tests/tcyr/texture.tcyr
# printed its summary as "\0<N> passed, ..." — the "\n" that assert_summary writes
# first came out as a NUL byte. grep/awk treat input containing a NUL as binary and
# drop the line, so `make test | grep -oE '[0-9]+ passed'` undercounted the total by
# texture's count. Root cause (bisected 2026-09-16): texture.tcyr's
# test_gpu_texture_create_2d_dispatch/_guards declare `var be[256]` but _t5_wire
# memsets BACKEND_SIZE (328) bytes into it — a 72-byte overrun. While array locals
# were shared .bss globals, that overrun zeroed the "\n" literal. cyrius 6.3.15 made
# array locals per-thread stack slots (cyrius CHANGELOG [6.3.15]), so the symptom
# vanished: 0 NUL bytes on 6.6.0-6.6.4, and CYRIUS_STACK_ARRAYS=0 on 6.6.4 brings it
# back. The overrun itself is still there (now a silent stack write) — see
# docs/development/issues/2026-09-16-test-stack-array-overruns.md. The tr -d '\0'
# strip below stays as a defence.
set -euo pipefail

# Parse one suite's captured output + exit code. Prints the assertion count on
# success; prints the reason and returns 1 otherwise.
_suite_count() {
  local name="$1" rc="$2" out="$3"
  if [ "$rc" -ne 0 ]; then
    if [ "$rc" -gt 128 ]; then
      echo "FAIL: $name — cyrius test exited $rc (killed by signal $((rc - 128)))"
    else
      echo "FAIL: $name — cyrius test exited $rc"
    fi
    printf '%s\n' "$out" | grep -E 'passed|failed|FAIL' || true
    return 1
  fi
  local line
  line="$(printf '%s\n' "$out" | grep -E '^[0-9]+ passed, [0-9]+ failed \([0-9]+ total\)$' | tail -1 || true)"
  if [ -z "$line" ]; then
    echo "FAIL: $name — exit 0 but no '<N> passed, <F> failed (<T> total)' summary line"
    return 1
  fi
  local p f t
  read -r p f t < <(printf '%s\n' "$line" | sed -E 's/^([0-9]+) passed, ([0-9]+) failed \(([0-9]+) total\)$/\1 \2 \3/')
  if [ "$f" -ne 0 ] || [ "$p" -ne "$t" ]; then
    echo "FAIL: $name — summary '$line' (failed must be 0 and passed == total)"
    return 1
  fi
  echo "$p"
}

# End-to-end: a throwaway tree holding a COPY of this script, a fake `cyrius` first on
# PATH, and empty .tcyr files whose names select the fake's behaviour. Runs the real
# counting path (capture, NUL strip, exit-code gate), not just _suite_count.
_self_test() {
  local tmp bad=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  mkdir -p "$tmp/bin" "$tmp/scripts" "$tmp/tests/tcyr"
  cp "$0" "$tmp/scripts/count-test-assertions.sh"
  cat > "$tmp/bin/cyrius" <<'FAKE'
#!/usr/bin/env bash
case "$(basename "$2" .tcyr)" in
  a_nul)     printf 'note: x\n\0005 passed, 0 failed (5 total)\n'; exit 0 ;;
  b_ok)      printf '\n7 passed, 0 failed (7 total)\n'; exit 0 ;;
  c_segv)    printf 'note: x\n'; exit 139 ;;
  d_silent)  printf 'note: x\n'; exit 0 ;;
  e_fail)    printf '\n3 passed, 1 failed (4 total)\n'; exit 1 ;;
  f_badsum)  printf '\n3 passed, 1 failed (4 total)\n'; exit 0 ;;
  g_short)   printf '\n3 passed, 0 failed (4 total)\n'; exit 0 ;;
esac
FAKE
  chmod +x "$tmp/bin/cyrius"
  # _case <desc> <want-exit> <regex the output must match> <suite names...>
  _case() {
    local desc="$1" want="$2" pat="$3" out rc=0; shift 3
    rm -f "$tmp/tests/tcyr/"*.tcyr
    local s; for s in "$@"; do : > "$tmp/tests/tcyr/$s.tcyr"; done
    out="$(PATH="$tmp/bin:$PATH" bash "$tmp/scripts/count-test-assertions.sh" 2>&1)" || rc=$?
    if [ "$rc" -ne "$want" ] || ! printf '%s\n' "$out" | grep -qE "$pat"; then
      echo "self-test FAIL: $desc (exit $rc, want $want; output:)"; printf '%s\n' "$out" | sed 's/^/    /'
      bad=1
    else
      echo "self-test ok:   $desc"
    fi
  }
  _case "NUL-prefixed summary is counted"        0 '^TOTAL +12 ' a_nul b_ok
  _case "SIGSEGV suite is a named FAIL"          1 '^FAIL: c_segv .*exited 139 \(killed by signal 11\)' a_nul b_ok c_segv
  _case "exit 0 without a summary is a FAIL"     1 '^FAIL: d_silent .*no .*summary' a_nul b_ok d_silent
  _case "assertion-failure exit is a named FAIL" 1 '^FAIL: e_fail .*exited 1$' a_nul b_ok e_fail
  _case "exit 0 with failed != 0 is a FAIL"      1 '^FAIL: f_badsum ' a_nul f_badsum
  _case "exit 0 with passed != total is a FAIL"  1 '^FAIL: g_short ' a_nul g_short
  return "$bad"
}

if [ "${1:-}" = "--self-test" ]; then
  _self_test
  exit $?
fi

cd "$(dirname "$0")/.."

total=0
files=0
for f in tests/tcyr/*.tcyr; do
  name="$(basename "$f" .tcyr)"
  rc=0
  out="$(cyrius test "$f" 2>&1 | tr -d '\0'; exit "${PIPESTATUS[0]}")" || rc=$?
  n="$(_suite_count "$name" "$rc" "$out")" || { printf '%s\n' "$n"; exit 1; }
  printf '%-26s %5d\n' "$name" "$n"
  total=$((total + n))
  files=$((files + 1))
done

printf '%-26s %5d  (across %d files)\n' "TOTAL" "$total" "$files"
