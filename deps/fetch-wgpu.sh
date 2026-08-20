#!/bin/sh
# Fetch wgpu-native v29 pre-built binaries
#
# ⚠ v29.0.0.0 -> v29.0.1.1 is NOT the patch its version number suggests. wgpu.h
# renumbers the whole WGPUSType_* block (PipelineLayoutExtras deleted, everything
# after it shifts down one) and DELETES WGPUNativeFeature_SpirvShaderPassthrough.
# webgpu.h itself is purely additive, so no wgpu_descriptors.cyr offset moves.
# See docs/development/issues/2026-08-19-wgpu-native-29011-breaking.md before any
# future bump — and diff BOTH headers, not just webgpu.h.
# Run from cyr/deps/
set -e

VERSION="v29.0.1.1"
ARCH="linux-x86_64"
URL="https://github.com/gfx-rs/wgpu-native/releases/download/${VERSION}/wgpu-${ARCH}-release.zip"

echo "Fetching wgpu-native ${VERSION}..."
curl -sL -o wgpu-native.zip "$URL"
unzip -o wgpu-native.zip -d wgpu-native
rm wgpu-native.zip

echo "Building shim..."
gcc -shared -fPIC -o libwgpu_shim.so wgpu_shim.c \
    wgpu-native/lib/libwgpu_native.a \
    -Iwgpu-native/include -lpthread -ldl -lm

echo "Done. wgpu-native ${VERSION} ready in deps/wgpu-native/"
