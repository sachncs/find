#!/usr/bin/env bash
# Profile-guided optimization driver.
#
# Builds the binary twice: once with --profile-generate to instrument
# hot paths, runs a representative workload, then rebuilds with
# --profile-use so the optimiser can use real call frequencies.
#
# Usage: scripts/build-pgo.sh
# Output: target/release-pgo/find (PGO-optimised binary)
#
# Requires Clang. On Linux use `rustup component add llvm-tools-preview`.
# On macOS the system clang ships with xcrun.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Step 1: instrumented build.
echo "=== Step 1: instrumented build ==="
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-profiles" \
    cargo build --release --target-dir target/release-pgo

# Step 2: representative workload.
echo "=== Step 2: representative workload ==="
TARGET_PUBKEY="0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
PROFILE_TMP="$(mktemp -d)"
trap 'rm -rf "$PROFILE_TMP"' EXIT
RUST_LOG=info target/release-pgo/find \
    --pubkey "$TARGET_PUBKEY" \
    --output-dir "$PROFILE_TMP/data" \
    --log-dir "$PROFILE_TMP/logs" \
    2>&1 | head -20 || true  # allow the run to terminate on timeout
# Also exercise the cache path.
RUST_LOG=info target/release-pgo/find \
    --pubkey "$TARGET_PUBKEY" \
    --output-dir "$PROFILE_TMP/data2" \
    --log-dir "$PROFILE_TMP/logs" \
    --cache-points \
    2>&1 | head -20 || true

# Step 3: PGO-optimised build.
echo "=== Step 3: PGO-optimised build ==="
mkdir -p /tmp/pgo-profiles
RUSTFLAGS="-Cprofile-use=/tmp/pgo-profiles" \
    cargo build --release --target-dir target/release-pgo

echo "PGO build complete: target/release-pgo/find"