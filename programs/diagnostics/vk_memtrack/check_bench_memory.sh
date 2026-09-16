#!/usr/bin/env bash
# check_bench_memory.sh [make-target]: run a wgpu program (default bench-gpu) under the
# memtrack Vulkan layer and fail if any CSV row grows live device-local memory past the
# limit in parse_mem.py. Needs wgpu-native (deps/wgpu-native), a Vulkan driver, cc and the
# Vulkan headers. This is how the MSAA render-target leak fixed in 4.1.3 was measured (+3840 MiB
# in the msaa4 row before the fix, 0 after).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
target=${1:-bench-gpu}
out=$(mktemp -d -t mabda-memtrack-XXXXXX)
cc -O2 -shared -fPIC -o "$out/libVkLayer_mabda_memtrack.so" "$here/memtrack_layer.c" -lpthread || exit 2
cp "$here/VkLayer_mabda_memtrack.json" "$out/"
log="$out/$target.log"
( cd "$root" && VK_ADD_LAYER_PATH="$out" VK_INSTANCE_LAYERS=VK_LAYER_MABDA_memtrack \
    make --no-print-directory "$target" ) > "$log" 2>&1
run_rc=$?
echo "log: $log (make exit $run_rc)"
python3 "$here/parse_mem.py" "$log"
parse_rc=$?
[ "$run_rc" -eq 0 ] || exit "$run_rc"
exit "$parse_rc"
