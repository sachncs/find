#!/usr/bin/env bash
# Driver for cargo bench.
#
# Runs the criterion suite and saves a baseline for later comparison.
# Usage:
#   scripts/run-benchmarks.sh                  # run all benchmarks
#   scripts/run-benchmarks.sh --save-baseline main
#   scripts/run-benchmarks.sh --baseline main
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Default args: 10 samples for quick smoke runs, 100 for thorough.
SAMPLES="${SAMPLES:-100}"

echo "=== Running cargo bench with $SAMPLES samples per benchmark ==="
cargo bench --bench bench -- --sample-size "$SAMPLES" "$@"

echo
echo "=== Benchmarks complete ==="
echo "Raw CSV output: target/criterion/*/*/new/raw.csv"
echo "HTML reports:   target/criterion/*/*/new/index.html"