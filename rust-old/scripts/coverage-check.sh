#!/usr/bin/env bash
set -euo pipefail

# Run cargo-tarpaulin and fail if line coverage drops below the threshold.
#
# Usage:
#   ./scripts/coverage-check.sh          # default threshold: 70%
#   ./scripts/coverage-check.sh 75       # custom threshold
#
# Requires: cargo-tarpaulin (cargo install cargo-tarpaulin)

THRESHOLD="${1:-70}"

echo "Running coverage check (threshold: ${THRESHOLD}%)"
echo ""

# Run tarpaulin with all features, capture the summary line
OUTPUT=$(cargo tarpaulin --all-features --skip-clean --out Stdout 2>&1)

# Extract coverage percentage from the last line (e.g., "75.42% coverage, ...")
COVERAGE_LINE=$(echo "$OUTPUT" | tail -1)
COVERAGE_PCT=$(echo "$COVERAGE_LINE" | grep -oP '^\d+\.\d+' || echo "0")

echo "$COVERAGE_LINE"
echo ""

# Compare using awk (bash can't do float comparison)
PASS=$(echo "$COVERAGE_PCT $THRESHOLD" | awk '{print ($1 >= $2) ? 1 : 0}')

if [ "$PASS" -eq 1 ]; then
    echo "PASS: ${COVERAGE_PCT}% >= ${THRESHOLD}% threshold"
    exit 0
else
    echo "FAIL: ${COVERAGE_PCT}% < ${THRESHOLD}% threshold"
    exit 1
fi
