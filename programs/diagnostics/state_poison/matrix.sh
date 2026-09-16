#!/bin/bash
# matrix.sh <spec> — poison matrix: `state_poison <spec>` immediately before each
# of the 9 native draw gates and the coverage scanner, one line per gate.
#
#   MABDA_ROOT  tree whose build/ holds the gate binaries (default: this repo)
#   POISON      poison binary (default: $MABDA_ROOT/build/state_poison)
#   GATES       space-separated `binary[:args]` override (default: the 9 draw
#               gates + coverage_scan:all)
#   REPS        repetitions per gate (default 1). Negative controls need >= 3:
#               another GPU client (e.g. the display server) rewrites
#               context state within ~0.1-1 s, so a slow gate can pass by luck.
#
# Build first: `make build/native_render_e2e ...` (README.md lists them) and
# `cyrius build programs/diagnostics/state_poison/{state_poison,coverage_scan}.cyr`.
# Exits 99 at the first kernel GPU fault/reset line (stop all GPU work), else 0
# if every gate exited 0, else 1.
set -u
here=$(cd "$(dirname "$0")" && pwd)
export MABDA_ROOT=${MABDA_ROOT:-$(cd "$here/../../.." && pwd)}
spec=${1:?spec}
reps=${REPS:-1}
gates=${GATES:-"native_texture_sample_e2e native_array_sample_e2e native_bc_array_e2e \
native_bilinear_sample_e2e native_compressed_sample_e2e native_cube_sample_e2e native_load_png_e2e \
native_render_e2e native_render_graph_mq_e2e coverage_scan:all"}
fail=0
for g in $gates; do
  bin=${g%%:*}; args=""
  [ "$bin" != "$g" ] && args=${g#*:}
  codes=""
  for r in $(seq "$reps"); do
    out=$("$here/run_gate.sh" "$spec" "run:$bin $args" 2>&1); rc=$?
    [ $rc -eq 99 ] && { echo "$out"; echo "STOP: kernel GPU fault/reset"; exit 99; }
    [ $rc -eq 97 ] || [ $rc -eq 98 ] && { echo "$out"; exit $rc; }
    codes="$codes $rc"
    [ $rc -eq 0 ] || fail=1
    cov=$(echo "$out" | grep -v "^###" | grep -E "coverage" | tr -s ' ' | sed 's/^ //' | tr '\n' ';')
  done
  printf '%-10s %-32s exit:%s  %s\n' "$spec" "$bin $args" "$codes" "$cov"
done
exit $fail
