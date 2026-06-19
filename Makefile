# 🛠 Secp256k1 Find Tool: Developer Automation

.PHONY: all build test bench clean lint doc deny

# Default target: Check and Build
all: lint test build

# Production Build
build:
	cargo build --release

# Run verification suite
test:
	cargo test --release

# Run micro-benchmarks
bench:
	cargo bench

# Perform static analysis and formatting
lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

# Generate code coverage report
coverage:
	cargo tarpaulin --all-targets --all-features --out html --timeout 600

# Generate documentation
doc:
	cargo doc --no-deps --open

# Run cargo-deny for license and dependency auditing
deny:
	cargo deny check all

# Clean build artifacts and logs
clean:
	cargo clean
	rm -rf logs/*
	rm -rf data/*.tmp
	rm -rf data/checkpoints/*.tmp

# Help information
help:
	@echo "Available targets:"
	@echo "  build    - Compile production binary"
	@echo "  test     - Run exhaustive test suite"
	@echo "  bench    - Run micro-benchmarks (Criterion)"
	@echo "  lint     - Run formatting and clippy checks"
	@echo "  doc      - Generate and open API documentation"
	@echo "  deny     - Run cargo-deny for license/dependency auditing"
	@echo "  clean    - Remove build artifacts and temporary files"
