#!/usr/bin/env bash
# Bundle mabda into a single dist/mabda.cyr for stdlib distribution.
#
# The bundle is byte-reproducible: running this script against an
# unmodified src/ tree always produces the same output, so CI can
# diff-check dist/mabda.cyr against the committed copy.
#
# Strips `include` statements from individual modules — consumers
# provide their own stdlib (dynlib, fmt, alloc, vec, etc.) and must
# populate the wgpu function-pointer table before calling mabda_main.
#
# Produces a minimal file: no banner, no per-module separators. The
# surrounding comments in individual modules serve as documentation.
# Banner text on a bundled file triggered a cc3 buffer limit during
# v2.1.1 development, so the bundler keeps the output as terse as
# possible.
#
# Usage: scripts/bundle.sh

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
VERSION=$(cat "$REPO/VERSION" | tr -d '[:space:]')
OUT="$REPO/dist/mabda.cyr"

mkdir -p "$REPO/dist"

echo "Bundling mabda v${VERSION} -> dist/mabda.cyr"

# Module order MUST match the include sequence in src/mabda.cyr so forward
# references (e.g., cache modules referencing _hash_to_heap_key) resolve
# correctly when the bundle is concatenated.
MODULES=(
    error.cyr
    color.cyr
    capabilities.cyr
    profiler.cyr
    resource.cyr
    wgpu_types.cyr
    wgpu_descriptors.cyr
    wgpu_ffi.cyr
    context.cyr
    buffer.cyr
    typed_buffer.cyr
    gpu_timestamps.cyr
    compute.cyr
    cache_key.cyr
    shader_cache.cyr
    pipeline_cache.cyr
    bind_group_cache.cyr
    vertex.cyr
    blend.cyr
    sampler.cyr
    depth.cyr
    bind_group.cyr
    texture.cyr
    render_target.cyr
    render_pipeline.cyr
    render_pass.cyr
    surface.cyr
    instancing.cyr
    debug.cyr
)

: > "$OUT"
for mod in "${MODULES[@]}"; do
    # Strip `include` lines — the bundle has no dependencies beyond the
    # consumer-provided stdlib.
    grep -v '^include ' "$REPO/src/${mod}" >> "$OUT"
done

# Strip trailing whitespace on every line (keeps lint happy). Don't touch
# blank-line density — cc3's lexer is sensitive to how multi-line constructs
# interact with blank runs.
sed -i 's/[[:space:]]*$//' "$OUT"

LINES=$(wc -l < "$OUT")
BYTES=$(wc -c < "$OUT")
MODULES_COUNT=${#MODULES[@]}
echo "Done: ${MODULES_COUNT} modules, ${LINES} lines, ${BYTES} bytes"
