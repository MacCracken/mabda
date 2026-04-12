#!/usr/bin/env bash
# Verify version consistency across VERSION, cyrius.toml, CHANGELOG.md, README.md.
# Exits non-zero if any source disagrees.
#
# Wired into `make test-all` so drift cannot escape CI.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

FILE_VERSION=$(cat VERSION | tr -d '[:space:]')
TOML_VERSION=$(grep '^version = ' cyrius.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

fail=0

if [ "$FILE_VERSION" != "$TOML_VERSION" ]; then
    echo "  FAIL: VERSION ($FILE_VERSION) != cyrius.toml ($TOML_VERSION)"
    fail=1
fi

if ! grep -q "^## \[$FILE_VERSION\]" CHANGELOG.md; then
    echo "  FAIL: version $FILE_VERSION missing from CHANGELOG.md"
    fail=1
fi

if [ -f README.md ]; then
    if grep -q "^Version:" README.md; then
        README_VERSION=$(grep '^Version:' README.md | head -1 | awk '{print $2}')
        if [ "$README_VERSION" != "$FILE_VERSION" ]; then
            echo "  FAIL: README.md Version ($README_VERSION) != VERSION ($FILE_VERSION)"
            fail=1
        fi
    fi
fi

if [ $fail -eq 0 ]; then
    echo "  OK: version $FILE_VERSION consistent across VERSION, cyrius.toml, CHANGELOG.md"
fi

exit $fail
