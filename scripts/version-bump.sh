#!/usr/bin/env bash
# Bump VERSION. `cyrius.cyml` pulls from VERSION via ${file:VERSION}
# so there's nothing else to edit — CHANGELOG.md still gets the new
# `## [X.Y.Z]` header manually.
set -euo pipefail
[ $# -ne 1 ] && { echo "Usage: $0 <version>"; exit 1; }
NEW_VERSION="$1"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "$NEW_VERSION" > "$REPO_ROOT/VERSION"

echo "Bumped to ${NEW_VERSION}"
echo "  VERSION: $(cat "$REPO_ROOT/VERSION")"
echo ""
echo "Next: add '## [${NEW_VERSION}]' header to CHANGELOG.md, then tag and push."
