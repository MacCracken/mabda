#!/bin/sh
# seat-closeout.sh amd|nvidia — the seat-only hardware closeout for one box, in one log
# (4.4.0): build/logs/seat-closeout-<box>-<timestamp>.log. Every gate in it also writes its own
# log through scripts/seat-gate.sh.
#
#   amd     (the Cezanne box): make test-native-all with NATIVE_NEEDS_MASTER emptied, so the
#           DRM-master gates (test-native-kms-modeset, test-native-present-e2e) and the
#           samvada live-bus run (test-native-logind-present-e2e) run with the rest; then the
#           linked example consumer (examples/stdlib-consumer's hello_gpu). It must be a CLEAN
#           BOOT: the log lists every GPU reset in this boot's kernel log, and any reset fails
#           the closeout (fixes proven on a GPU that has reset do not count).
#   nvidia  (chew): the five card-node gates — test-nvidia-kms-summary, -kms-scanout,
#           -kms-modeset, -present-e2e and -surface-present-e2e.
#
# Run from a TEXT CONSOLE login (Ctrl-Alt-F3) with the display manager stopped
# (e.g. `sudo systemctl stop sddm`): a terminal inside a desktop session cannot take DRM master.
# Exit 0 only when every step passed (and, for amd, the boot was clean).
box=$1
case "$box" in amd|nvidia) ;; *) echo "usage: seat-closeout.sh amd|nvidia"; exit 2;; esac
mkdir -p build/logs
log=build/logs/seat-closeout-$box-$(date +%Y%m%d-%H%M%S).log
dirty=""
git diff --quiet 2>/dev/null || dirty="+dirty"
{
    echo "# seat-closeout-$box $(date -Is) host=$(uname -n) kernel=$(uname -r)"
    echo "# commit=$(git rev-parse --short HEAD 2>/dev/null)$dirty"
    echo "# tty=$(tty 2>/dev/null) session=${XDG_SESSION_TYPE:-unset} booted=$(uptime -s 2>/dev/null)"
} > "$log"
fail=""
case "$box" in
amd)
    resets=$(journalctl -k -b --no-pager 2>/dev/null | grep -iE "amdgpu.*(ring .*timeout|reset)|gpu reset|mode2")
    if [ -n "$resets" ]; then
        { echo "# GPU resets this boot (NOT a clean boot):"; echo "$resets" | sed 's/^/#   /'; } >> "$log"
        fail="$fail clean-boot"
    else
        echo "# GPU resets this boot: none (a clean boot)" >> "$log"
    fi
    echo "=== test-native-all (NATIVE_NEEDS_MASTER emptied)" >> "$log"
    make --no-print-directory test-native-all NATIVE_NEEDS_MASTER= >> "$log" 2>&1
    r=$?
    echo "=== exit $r" >> "$log"
    [ $r -eq 0 ] || fail="$fail test-native-all"
    echo "=== example consumer (examples/stdlib-consumer hello_gpu)" >> "$log"
    make --no-print-directory example-link >> "$log" 2>&1 && ./build/example-link/hello_gpu >> "$log" 2>&1
    r=$?
    echo "=== exit $r" >> "$log"
    [ $r -eq 0 ] || fail="$fail example-consumer"
    ;;
nvidia)
    for g in test-nvidia-kms-summary test-nvidia-kms-scanout test-nvidia-kms-modeset \
             test-nvidia-present-e2e test-nvidia-surface-present-e2e; do
        echo "=== $g" >> "$log"
        make --no-print-directory "$g" >> "$log" 2>&1
        r=$?
        echo "=== exit $r" >> "$log"
        [ $r -eq 0 ] || fail="$fail $g"
    done
    ;;
esac
if [ -n "$fail" ]; then echo "RESULT: FAILED:$fail" >> "$log"; else echo "RESULT: every step passed" >> "$log"; fi
grep -E "^(# GPU resets|=== |RESULT|native HW gates|SKIPPED|FAILED|KNOWN-FAIL)" "$log"
echo "log: $log"
[ -z "$fail" ]
