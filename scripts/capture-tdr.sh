#!/bin/bash
# Trigger a TDR by running native_render_e2e, then capture the
# kernel's at-hang register dump to /tmp/render-tdr.txt. Requires
# sudo (devcoredump is root-readable only).
set -eu
cd "$(dirname "$0")/.."
make test-native-render-e2e >/dev/null 2>&1 || true
DUMP=/sys/class/drm/card1/device/devcoredump/data
if [ ! -e "$DUMP" ]; then
    echo "no devcoredump (test may not have TDRd)" >&2
    exit 1
fi
sudo cat "$DUMP" > /tmp/render-tdr.txt
sudo chmod 644 /tmp/render-tdr.txt
echo "captured: $(wc -c < /tmp/render-tdr.txt) bytes → /tmp/render-tdr.txt"
