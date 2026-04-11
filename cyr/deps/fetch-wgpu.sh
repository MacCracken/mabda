#!/bin/sh
# Fetch wgpu-native v29 pre-built binaries
# Run from cyr/deps/
set -e

VERSION="v29.0.0.0"
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
