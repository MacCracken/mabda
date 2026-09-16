#!/bin/bash
# native_all_poison.sh <spec|none> — `make test-native-all`, with
# `state_poison <spec>` run immediately before every native draw gate.
#
# Same target list and SKIP / KNOWN-FAIL handling as the Makefile's
# test-native-all (targets read from the Makefile). Prints the per-target
# result, a summary, and every amdgpu/drm kernel line logged during the run.
# Exits 0 only if no target failed and the kernel log shows no GPU fault.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${MABDA_ROOT:-$(cd "$here/../../.." && pwd)}
spec=${1:?spec or none}
poison=${POISON:-$root/build/state_poison}
cd "$root" || exit 2
draw=" test-native-texture-sample-e2e test-native-array-sample-e2e test-native-bc-array-e2e \
test-native-bilinear-sample-e2e test-native-compressed-sample-e2e test-native-cube-sample-e2e \
test-native-load-png-e2e test-native-render-e2e test-native-render-graph-mq-e2e "
master=$(grep -E '^NATIVE_NEEDS_MASTER *=' Makefile | sed 's/^[^=]*= *//')
known=$(grep -E '^NATIVE_KNOWN_FAIL *=' Makefile | sed 's/^[^=]*= *//')
targets=$(grep -oE '^test-native[a-z0-9-]*:' Makefile | sed 's/://' | sort -u | grep -v '^test-native-all$')
[ "$spec" = none ] || [ -x "$poison" ] || { echo "missing $poison"; exit 2; }
total=0; pass=0; failed=""; skipped=""; kf=""; poisoned=0
log=$(mktemp)
t0=$(date '+%Y-%m-%d %H:%M:%S')
for t in $targets; do
  total=$((total + 1)); printf '%-46s ' "$t"
  case " $master " in *" $t "*) printf 'SKIP  (needs DRM master)\n'; skipped="$skipped $t"; continue;; esac
  case " $known " in *" $t "*) printf 'KNOWN-FAIL\n'; kf="$kf $t"; continue;; esac
  tag=""
  if [ "$spec" != none ]; then
    case "$draw" in *" $t "*)
      "$poison" "$spec" > /dev/null 2>&1 || { echo "POISON FAILED"; rm -f "$log"; exit 3; }
      poisoned=$((poisoned + 1)); tag=" [poisoned:$spec]";;
    esac
  fi
  if make --no-print-directory "$t" > "$log" 2>&1; then
    printf 'PASS%s\n' "$tag"; pass=$((pass + 1))
  else
    printf 'FAIL%s\n' "$tag"; failed="$failed $t"; tail -3 "$log" | tr -d '\0' | sed 's/^/      /'
  fi
done
rm -f "$log"
echo
echo "native HW gates: $pass passed, $(echo $failed | wc -w) failed, $(echo $skipped | wc -w) skipped," \
  "$(echo $kf | wc -w) known-fail (of $total); poison '$spec' before $poisoned draw gates"
[ -n "$skipped" ] && echo "SKIPPED:$skipped"
[ -n "$kf" ] && echo "KNOWN-FAIL:$kf"
[ -n "$failed" ] && echo "FAILED:$failed"
echo "### kernel since $t0:"
k=$(journalctl -k --since "$t0" --no-pager 2>/dev/null | grep -iE 'amdgpu|drm|gpu')
echo "${k:-(no amdgpu/drm kernel lines)}"
if echo "$k" | grep -qiE 'timeout|reset|fault|wedged|hang|coredump'; then exit 99; fi
[ -z "$failed" ]
