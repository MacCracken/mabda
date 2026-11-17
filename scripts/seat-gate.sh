#!/bin/sh
# seat-gate.sh NAME CMD [ARGS...] — run one gate a person runs at a seat, with its whole output
# in build/logs/NAME-<timestamp>.log (and a copy at build/logs/NAME.log).
#
# The log is how the result comes back: the person at the seat often cannot copy anything off
# the console, so every seat gate writes one (4.4.0, the seat-only closeout). The header records
# what decides whether such a gate CAN pass — the tty and session type (a terminal inside a
# desktop session cannot take DRM master) and the boot time (a clean boot or not).
# Prints the log and exits with CMD's status.
name=$1
shift
mkdir -p build/logs
log=build/logs/$name-$(date +%Y%m%d-%H%M%S).log
dirty=""
git diff --quiet 2>/dev/null || dirty="+dirty"
{
    echo "# $name $(date -Is) host=$(uname -n) kernel=$(uname -r)"
    echo "# commit=$(git rev-parse --short HEAD 2>/dev/null)$dirty"
    echo "# tty=$(tty 2>/dev/null) session=${XDG_SESSION_TYPE:-unset} booted=$(uptime -s 2>/dev/null)"
} > "$log"
"$@" >> "$log" 2>&1
rc=$?
cat "$log"
cp "$log" "build/logs/$name.log"
echo "log: $log (exit $rc)"
exit $rc
