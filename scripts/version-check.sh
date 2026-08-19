#!/usr/bin/env bash
# Verify version consistency across VERSION, cyrius.cyml, CHANGELOG.md, README.md.
# `version = "${file:VERSION}"` in the manifest means VERSION is the single
# source of truth — the check below enforces that downstream references
# (CHANGELOG header, README badge) agree.
#
# Wired into `make test-all` so drift cannot escape CI.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

FILE_VERSION=$(tr -d '[:space:]' < VERSION)
fail=0

# Allow the manifest to either inline the version or template it from VERSION.
# The manifest's `version = "${file:VERSION}"` form is valid — so we only
# diff when the manifest has a literal version string.
if grep -qE '^version = "[0-9]' cyrius.cyml; then
    MANIFEST_VERSION=$(grep -E '^version = "' cyrius.cyml | head -1 | sed -E 's/version = "([^"]*)"/\1/')
    if [ "$FILE_VERSION" != "$MANIFEST_VERSION" ]; then
        echo "  FAIL: VERSION ($FILE_VERSION) != cyrius.cyml ($MANIFEST_VERSION)"
        fail=1
    fi
fi

if ! grep -q "^## \[$FILE_VERSION\]" CHANGELOG.md; then
    echo "  FAIL: version $FILE_VERSION missing from CHANGELOG.md"
    fail=1
fi

if [ -f README.md ] && grep -q "^Version:" README.md; then
    README_VERSION=$(grep '^Version:' README.md | head -1 | awk '{print $2}')
    if [ "$README_VERSION" != "$FILE_VERSION" ]; then
        echo "  FAIL: README.md Version ($README_VERSION) != VERSION ($FILE_VERSION)"
        fail=1
    fi
fi

if [ $fail -eq 0 ]; then
    # Name only what was actually compared. The manifest is checked ONLY when it
    # inlines a literal version; with the templated `version = "${file:VERSION}"`
    # form there is nothing to diff, and claiming otherwise overstates the gate.
    checked="VERSION, CHANGELOG.md"
    grep -qE '^version = "[0-9]' cyrius.cyml && checked="$checked, cyrius.cyml"
    [ -f README.md ] && grep -q "^Version:" README.md && checked="$checked, README.md"
    echo "  OK: version $FILE_VERSION consistent across $checked"
fi

exit $fail
