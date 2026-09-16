#!/bin/sh
# Fetch wgpu-native v29 pre-built binaries
#
# ⚠ v29.0.0.0 -> v29.0.1.1 is NOT the patch its version number suggests. wgpu.h
# renumbers the whole WGPUSType_* block (PipelineLayoutExtras deleted, everything
# after it shifts down one) and DELETES WGPUNativeFeature_SpirvShaderPassthrough.
# webgpu.h itself is purely additive, so no wgpu_descriptors.cyr offset moves.
# See docs/development/issues/2026-08-19-wgpu-native-29011-breaking.md before any
# future bump — and diff BOTH headers, not just webgpu.h.
#
# Unpacks into wgpu-native/ NEXT TO THIS SCRIPT, whatever the caller's working
# directory: `sh deps/fetch-wgpu.sh` from the repo root (README, CONTRIBUTING) and
# `(cd deps && sh fetch-wgpu.sh)` (the stdlib-consumer example, which copies the
# script into its own deps/) both land beside deps/wgpu_main.c. That launcher is the
# only consumer: it includes wgpu-native/include/webgpu/*.h relative to itself, and
# its links use wgpu-native/lib/libwgpu_native.a (the Makefile's WGPU_DIR).
# (Until 4.1.3 the header said "Run from cyr/deps/", where the script lived until
# ef53493 moved it to deps/; run from the repo root it unpacked into ./wgpu-native,
# where nothing looks.)
#
# No shim library is built. Until 4.1.3 this script also compiled wgpu_shim.c into
# libwgpu_shim.so, a leftover of the pre-launcher dlopen path. Nothing loads or links
# it (every shim in the Cyrius fn table lives in deps/wgpu_main.c), and the step made
# the script exit non-zero, after unpacking, wherever wgpu_shim.c was not copied along.
set -eu
cd "$(dirname "$0")"

VERSION="v29.0.1.1"
ARCH="linux-x86_64"
URL="https://github.com/gfx-rs/wgpu-native/releases/download/${VERSION}/wgpu-${ARCH}-release.zip"

echo "Fetching wgpu-native ${VERSION}..."
curl -fsSL -o wgpu-native.zip "$URL"
unzip -o wgpu-native.zip -d wgpu-native
rm wgpu-native.zip

for f in wgpu-native/include/webgpu/webgpu.h wgpu-native/include/webgpu/wgpu.h \
         wgpu-native/lib/libwgpu_native.a; do
    [ -f "$f" ] || { echo "fetch-wgpu.sh: $(pwd)/$f missing after unpack" >&2; exit 1; }
done

echo "Done. wgpu-native ${VERSION} ready in $(pwd)/wgpu-native/"
