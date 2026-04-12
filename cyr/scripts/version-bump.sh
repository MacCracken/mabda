#!/usr/bin/env bash
# Bump version in VERSION + cyr/cyrius.toml
set -euo pipefail
[ $# -ne 1 ] && echo "Usage: $0 <version>" && exit 1
NEW_VERSION="$1"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "$NEW_VERSION" > "$REPO_ROOT/VERSION"
sed -i "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "$REPO_ROOT/cyr/cyrius.toml"

echo "Bumped to ${NEW_VERSION}"
echo "  VERSION: $(cat "$REPO_ROOT/VERSION")"
echo "  cyrius.toml: $(grep '^version' "$REPO_ROOT/cyr/cyrius.toml")"
echo ""
echo "Next: update CHANGELOG.md, then tag and push."
