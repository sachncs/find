# 🛠 Secp256k1 Find Tool: Developer Automation

.PHONY: all build test bench clean lint doc deny audit all-checks pgo flamegraph help

# Default target: Check and Build
all: lint test build

# Production Build
build:
	cargo build --release

# Run verification suite (release-mode, all targets)
test:
	cargo test --all-targets --all-features --release

# Run micro-benchmarks via the helper script (sets sample size, prints output paths)
bench:
	scripts/run-benchmarks.sh

# Profile-guided optimization (requires Clang)
pgo:
	scripts/build-pgo.sh

# Perform static analysis, formatting, and rustdoc checks
lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo doc --no-deps --all-features

# Run the full verification suite (fmt + clippy + test + doc + audit + deny + tarpaulin)
all-checks:
	scripts/check-all.sh

# Generate code coverage report
coverage:
	cargo tarpaulin --all-targets --all-features --out html --timeout 600

# Generate a perf-flamegraph for a representative run
flamegraph:
	cargo build --release
	perf record -g --target/release/find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798 || true
	perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg || true

# Generate documentation
doc:
	cargo doc --no-deps --open

# Run cargo-deny for license and dependency auditing
deny:
	cargo deny check all

# Run cargo-audit for security advisories
audit:
	cargo install cargo-audit --locked
	cargo audit

# Clean build artifacts, logs, and profiling data
clean:
	cargo clean
	rm -rf logs/*
	rm -rf data/*.tmp
	rm -rf data/checkpoints/*.tmp
	rm -rf target/release-pgo
	rm -f perf.data
	rm -f flamegraph.svg

# Help information
help:
	@echo "Available targets:"
	@echo "  all         - Lint + test + build (default)"
	@echo "  build       - Compile production binary"
	@echo "  test        - Run exhaustive test suite (all targets, all features, release)"
	@echo "  bench       - Run micro-benchmarks (Criterion)"
	@echo "  lint        - Run formatting, clippy, and doc checks"
	@echo "  all-checks  - Run scripts/check-all.sh (full verification suite)"
	@echo "  doc         - Generate and open API documentation"
	@echo "  deny        - Run cargo-deny for license/dependency auditing"
	@echo "  audit       - Run cargo-audit for security advisories"
	@echo "  coverage    - Generate HTML coverage report"
	@echo "  pgo         - Build with profile-guided optimization"
	@echo "  flamegraph  - Capture a CPU flamegraph for a representative run"
	@echo "  clean       - Clean build artifacts and temporary files"