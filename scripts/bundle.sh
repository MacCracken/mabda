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
# ---------------------------------------------------------------------------
# HISTORICAL NOTE — the "banners break cc3" red herring
# ---------------------------------------------------------------------------
# During v2.1.1 bundler development, the dist file originally included a
# top-of-file banner + per-module `# --- name.cyr ---` separators, and cc3
# rejected the bundle with `error:3719: unexpected end of file` at token
# positions that didn't line up with anything suspicious. The fix appeared
# to be "strip all banner text" and this comment block once said exactly
# that.
#
# Cyrius 3.4.19 proved the real cause: cc3's raw stdin read loop silently
# truncated at 131072 bytes (the old 128KB `input_buf` limit). The mabda
# bundle was 141912 bytes. Everything beyond byte 131072 was dropped on
# the floor with no diagnostic; the parser then ran out of tokens mid-way
# through a function body and reported "end of file" at whatever token
# position the truncation happened to hit.
#
# Stripping banners "worked" by shaving enough bytes to fit under 131072
# for a previous bundle revision — a coincidence fix, not a root cause
# fix. The real fix is in Cyrius 3.4.19: `input_buf` was expanded to 256KB
# and now errors out explicitly on overflow instead of silently truncating.
#
# We still strip the banner + separators because there's no reason to spend
# bytes on them — but that is an economy choice, NOT a workaround for a
# compiler bug. If you find yourself tempted to re-add banner text, go
# ahead: cc3 >= 3.4.19 handles it fine. Just pin your `cyrius-version`.
# ---------------------------------------------------------------------------
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
