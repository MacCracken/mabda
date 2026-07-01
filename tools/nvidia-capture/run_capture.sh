#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
glslc -fshader-stage=comp probe.comp -o probe.spv
gcc -shared -fPIC -O2 nouveau_capture.c -o nouveau_capture.so -ldl
gcc -O2 vk_compute.c -o vk_compute -lvulkan
rm -rf nvcap && mkdir -p nvcap
echo "=== running NVK compute dispatch under capture interposer ==="
NV_CAP_DIR=./nvcap LD_PRELOAD=./nouveau_capture.so ./vk_compute probe.spv
echo "=== capture artifacts ==="; ls -l nvcap/
