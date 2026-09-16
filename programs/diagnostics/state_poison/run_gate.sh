#!/bin/bash
# run_gate.sh <spec|none> <make-target | run:build-binary [args]> [delay_ms]
#
# One poison -> gate step of the inherited-state proof (README.md). Runs
# `build/state_poison <spec>` (unless spec is `none`), sleeps delay_ms (default
# 0), then the gate: a Makefile target, or `run:<binary> [args]` for a binary
# under build/. Prints the gate output and exit code, then any amdgpu/drm kernel
# lines logged since the step began. Exits 99 — and the caller MUST stop all GPU
# work — if the kernel log shows a ring timeout / reset / fault. Otherwise exits
# with the gate's exit code.
#
# Run from anywhere; MABDA_ROOT defaults to the repo containing this script and
# POISON to $MABDA_ROOT/build/state_poison. Exit codes 97 / 98 / 99 are reserved
# (poison failed / missing / GPU fault).
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${MABDA_ROOT:-$(cd "$here/../../.." && pwd)}
spec=${1:?spec}; gate=${2:?gate}; delay_ms=${3:-0}
poison=${POISON:-$root/build/state_poison}
t0=$(date '+%Y-%m-%d %H:%M:%S')
echo "### $(date -Is) root=$root gate=$gate poison=$spec delay_ms=$delay_ms"
if [ "$spec" != none ]; then
  [ -x "$poison" ] || { echo "missing $poison (see README: build)"; exit 98; }
  "$poison" "$spec"; prc=$?
  echo "poison_exit=$prc"
  [ $prc -eq 0 ] || { echo "### poison failed; gate not run"; exit 97; }
fi
[ "$delay_ms" -gt 0 ] && sleep "$(printf '%d.%03d' $((delay_ms / 1000)) $((delay_ms % 1000)))"
case "$gate" in
  run:*) cmd=${gate#run:}; (cd "$root" && eval "./build/$cmd") 2>&1 | tr -d '\0'; rc=${PIPESTATUS[0]} ;;
  *) (cd "$root" && make --no-print-directory "$gate") 2>&1 | tr -d '\0' \
       | grep -vE '^(cyrius build|[0-9]+ deps resolved|cyrius.lock:|note: [0-9]+ unreachable|OK \()'
     rc=${PIPESTATUS[0]} ;;
esac
echo "### gate_exit=$rc gate=$gate poison=$spec"
sleep 0.2
k=$(journalctl -k --since "$t0" --no-pager 2>/dev/null | grep -iE 'amdgpu|drm|gpu')
if [ -n "$k" ]; then
  echo "### KERNEL:"; echo "$k"
  if echo "$k" | grep -qiE 'timeout|reset|fault|wedged|hang|coredump'; then
    echo "### STOP: GPU fault/reset in the kernel log — stop all GPU work"; exit 99
  fi
else
  echo "### kernel: (no amdgpu/drm lines)"
fi
exit $rc
