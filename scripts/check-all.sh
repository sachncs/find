#!/usr/bin/env bash
# Full verification suite — fmt, lint, test, doc, audit, deny.
#
# Run from repo root: scripts/check-all.sh
# Exits non-zero on the first failure.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

banner() { printf '\n=== %s ===\n' "$1"; }

banner "cargo fmt --check"
cargo fmt --all -- --check

banner "cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

banner "cargo test --all-targets --all-features"
cargo test --all-targets --all-features

banner "cargo test --doc"
cargo test --doc

banner "cargo doc --no-deps --all-features"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

if command -v cargo-audit >/dev/null 2>&1; then
    banner "cargo audit"
    cargo audit
fi

if command -v cargo-deny >/dev/null 2>&1; then
    banner "cargo deny check"
    cargo deny check
fi

if command -v cargo-tarpaulin >/dev/null 2>&1; then
    banner "cargo tarpaulin (coverage)"
    cargo tarpaulin --all-targets --all-features --out xml --timeout 600
fi

echo
echo "All checks passed."