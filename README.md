# Secp256k1 Find Tool

[![CI](https://github.com/sachn-cs/find/actions/workflows/ci.yml/badge.svg)](https://github.com/sachn-cs/find/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/sachn-cs/find/branch/master/graph/badge.svg)](https://codecov.io/gh/sachn-cs/find)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-red.svg)](LICENSE-MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)
[![Stars](https://img.shields.io/github/stars/sachn-cs/find?style=social)](https://github.com/sachn-cs/find)

> **EDUCATIONAL AND RESEARCH USE ONLY.** This software is for pedagogical exploration of elliptic curve mathematics and high-performance Rust systems engineering. See [DISCLAIMER.md](DISCLAIMER.md).

A high-performance Rust system for secp256k1 private key discovery using a multi-variant range-splitting algorithm. It searches for scalars `j` and offsets `V` such that `x(j·G) = x(P - V·G)`, yielding key candidates `d = V ± j (mod n)`.

## Features

- **512-Variant Search Engine** — Range-splitting using powers of 2 and cumulative summations
- **Batch Normalization** — Montgomery's simultaneous inversion for 630x speedup
- **Parallel Sweep** — Work-stealing data-level parallelism via `rayon`
- **Binary Caching** — Optional precomputation for I/O-bound cache scans
- **Atomic Checkpointing** — Write-then-rename for crash-safe state persistence
- **Structured Observability** — Non-blocking rolling file logs with `tracing`
- **Comprehensive Testing** — 60+ tests including property-based, integration, and audit suites

## Installation

```bash
# Clone the repository
git clone https://github.com/sachn-cs/find.git
cd find

# Build in release mode
cargo build --release

# Or use the Makefile
make build
```

### Requirements

- Rust 1.70 or later
- Supported platforms: Linux, macOS, Windows

## Usage

```bash
# Basic search against a public key
find --pubkey <HEX_SEC1>

# Generate binary cache during search
find --pubkey <HEX> --cache-points

# Custom data/checkpoint directory
find --pubkey <HEX> --output-dir <DIR>

# Custom log directory
find --pubkey <HEX> --log-dir <DIR>
```

### Example

```bash
# Search for a private key corresponding to a known public key
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798

# With precomputation cache
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798 --cache-points
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level filter (e.g., `debug`, `trace`) |

### Constants

| Parameter | Value | Effect |
|-----------|-------|--------|
| `TRILLION` | 1,000,000,000,000 | Segment boundary for logging/pause |
| `CACHE_CHUNK_SIZE` | 1,000,000,000 | 1B points = ~32GB binary cache |
| `BATCH_SIZE` | 32 | Points per batch normalization |
| `MAX_SEARCH` | u64::MAX | Effectively 2^64 |

## Project Structure

```
find/
├── src/
│   ├── lib.rs          # Library root; exports ecc, error, search
│   ├── main.rs         # CLI orchestrator; checkpointing, user interaction
│   ├── ecc.rs          # SEC1 parsing, point arithmetic, scalar conversion
│   ├── search.rs       # Parallel sweep engine, variant index, binary caching
│   ├── error.rs        # Unified FindError hierarchy
│   ├── orchestrator.rs # Session management and resume logic
│   └── persistence.rs  # Checkpoint read/write with atomic operations
├── tests/
│   ├── audit.rs        # End-to-end key recovery verification
│   ├── integration.rs  # Randomized discovery and edge case tests
│   └── orchestrator.rs # Session flow and checkpoint tests
├── benches/
│   └── bench.rs        # Criterion microbenchmarks
├── docs/               # Additional documentation
├── .github/            # CI/CD, issue templates, dependabot
├── Cargo.toml          # Package metadata and dependencies
├── Makefile            # Developer automation commands
└── README.md           # This file
```

## Development

### Available Commands

| Command | Description |
|---------|-------------|
| `make build` | Compile production binary (opt-level=3, lto=fat) |
| `make test` | Run exhaustive test suite |
| `make bench` | Run micro-benchmarks with Criterion |
| `make lint` | Run formatting and clippy checks |
| `make doc` | Generate and open API documentation |
| `make coverage` | Generate HTML coverage report |
| `make clean` | Remove build artifacts and temporary files |
| `make all` | Run lint, test, and build (default) |

### Quick Start

```bash
# Install dependencies and run all checks
make all

# Run tests with increased property-test cases
PROPTEST_CASES=1000 cargo test --release
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| **Language** | Rust (2021 edition) |
| **Cryptography** | k256 (secp256k1 arithmetic) |
| **Parallelism** | rayon (work-stealing) |
| **CLI** | clap 4.4 (derive macros) |
| **Error Handling** | thiserror, anyhow |
| **Serialization** | serde, serde_json |
| **Observability** | tracing, tracing-subscriber, tracing-appender |
| **Testing** | proptest, criterion, tempfile |
| **CI/CD** | GitHub Actions |

## Release Profile

The release binary is optimized for maximum performance:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = 'abort'
strip = true
overflow-checks = true
```

## Roadmap

- [ ] GPU acceleration via CUDA/OpenCL bindings
- [ ] Distributed search across multiple machines
- [ ] WebAssembly compilation for browser-based research
- [ ] REST API for remote search management
- [ ] Additional curve support (secp224r1, secp384r1)
- [ ] Improved progress visualization and ETA estimation
- [ ] Comprehensive benchmarking suite with historical tracking

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](CONTRIBUTING.md) for details on:

- Fork and branch workflow
- Commit message conventions (Conventional Commits)
- Pull request process
- Code quality standards

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Security

For reporting security vulnerabilities, please see our [Security Policy](SECURITY.md). **Do not open public issues for security reports.**

## Mathematical Foundation

For the algorithmic derivation and mathematical proofs, see [ALGORITHMS.md](ALGORITHMS.md).

## Architecture

For system design decisions and trade-offs, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Testing

For the full verification strategy, see [TESTING.md](TESTING.md).

## License

This project is dual-licensed under the [MIT License](LICENSE-MIT) and [Apache License 2.0](LICENSE-APACHE).

You may use this software under the terms of either license at your option.

---

**Disclaimer:** This tool is for educational and research purposes only. See [DISCLAIMER.md](DISCLAIMER.md) for full legal terms.
