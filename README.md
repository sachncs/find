# Secp256k1 Find Tool

[![CI](https://github.com/sachncs/find/actions/workflows/ci.yml/badge.svg)](https://github.com/sachncs/find/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/sachncs/find/branch/master/graph/badge.svg)](https://codecov.io/gh/sachncs/find)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-red.svg)](LICENSE-MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)
[![Stars](https://img.shields.io/github/stars/sachncs/find?style=social)](https://github.com/sachncs/find)

> **EDUCATIONAL AND RESEARCH USE ONLY.** This software is for pedagogical exploration of elliptic curve mathematics and high-performance Rust systems engineering. See [DISCLAIMER.md](DISCLAIMER.md).

A high-performance Rust system for secp256k1 private key discovery using a multi-variant range-splitting algorithm. It searches for scalars `j` and offsets `V` such that `x(j·G) = x(P - V·G)`, yielding key candidates `d = V ± j (mod n)`.

## Features

- **512-Variant Search Engine** — Range-splitting using powers of 2 and cumulative summations
- **Batch Normalization** — Montgomery's simultaneous inversion for ~15–20× speedup in the normalization phase
- **Parallel Sweep** — Work-stealing data-level parallelism via `rayon` with early-exit
- **Binary Caching** — Optional precomputation for I/O-bound cache scans (~100× speedup on NVMe)
- **Atomic Checkpointing** — Write-then-rename for crash-safe state persistence with integrity anchor
- **Structured Observability** — Non-blocking rolling file logs with `tracing`
- **Comprehensive Testing** — Property-based, integration, orchestrator, audit, KAT, and differential test suites
- **Differential Testing** — Cross-implementation verification against `libsecp256k1` (the reference C implementation)
- **Fuzz Testing** — Three fuzz targets for the public APIs (parse_pubkey, hex_to_scalar, scalar_mul_g)

## Installation

```bash
# Clone the repository
git clone https://github.com/sachncs/find.git
cd find

# Build in release mode
cargo build --release

# Or use the Makefile
make build
```

### Requirements

- Rust 1.70 or later
- Supported platforms: Linux, macOS, Windows (x86_64 and aarch64)

## Quick Start

```bash
# Basic search against a public key
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798

# Generate binary cache during search (~32 GB per billion scalars)
find --pubkey 0279be66... --cache-points

# Tune batch size and variant count (advanced)
find --pubkey 0279be... --batch-size 64 --variants 256

# Custom data and log directories
find --pubkey 0279be66... --output-dir <DIR> --log-dir <DIR>
```

For a guided walkthrough of your first search, see [docs/getting-started.md](docs/getting-started.md).

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level filter (e.g. `debug`, `trace`) |
| `RUST_BACKTRACE` | `0` | Set to `1` for backtraces on panic |

### Compile-time Constants

| Constant | Value | Purpose |
|---|---|---|
| `TRILLION` | `1,000,000,000,000` | Step size for audit boundary logging |
| `CACHE_CHUNK_SIZE` | `1,000,000,000` | Scalars per cache chunk |
| `BATCH_SIZE` | `32` | Points per batch normalization |
| `MAX_SEARCH` | `u64::MAX` | Sweep upper bound |

See [docs/configuration.md](docs/configuration.md) for the full configuration reference.

## Performance

The hot loop is dominated by:

- One bootstrap scalar multiplication per 32-scalar batch (~256 field mults).
- 31 mixed `+G` additions (~12 field mults each).
- Montgomery simultaneous inversion over the 32-point batch.
- A binary-search `match_x` in a 16 KiB L1-resident key array.

The cumulative wall-clock cost of the per-session cold start is dominated by **variant generation** (now 256 scalar multiplications + 256 point doublings + 256 mixed additions, vs. 512 scalar multiplications in the original implementation — see [docs/optimization-decisions/0001-affinepoint-x-direct.md](docs/optimization-decisions/0001-affinepoint-x-direct.md) and following).

For the per-batch hot loop, the dominant cost is the **bootstrap scalar multiplication** (~80% of per-batch cycles). Increasing `BATCH_SIZE` beyond 64 has diminishing returns; the `+G` chain + Montgomery normalize + match together take the remaining 20%.

Tunables:

| Flag | Default | Range | Effect |
|---|---|---|---|
| `--batch-size` | 32 | 1..=256 | Points per Montgomery batch |
| `--variants` | 512 | 1..=512 | Powers-of-two + cumulative sum variants |
| `--cache-points` | false | bool | Persist X-coords to disk for I/O-bound re-runs |

For sustained workloads, the cached sweep path is ~100× faster than the CPU-bound path on NVMe hardware (see [docs/performance.md](docs/performance.md) for the full guide).

Reproduce the published cycle counts:

```bash
cargo bench                       # criterion microbenchmarks
scripts/build-pgo.sh              # profile-guided optimized build
scripts/run-benchmarks.sh         # benchmark wrapper
```

## Research reproducibility

The codebase ships with three independent verification layers:

- **KAT tests** (`tests/kat.rs`): 11 known-answer tests against SEC1 §2.7.1 vectors and `k256` reference outputs.
- **Differential tests** (`tests/differential.rs`): cross-check `k256` against the reference C `libsecp256k1` for 12 boundary scalars (1, 2, 3, 7, 100, 1k, 1M, 1.2G, 2^32, 2^63, u64::MAX, …).
- **Audit tests** (`tests/audit.rs`): end-to-end pipeline (parse → variants → sweep → recover) for the known scalar `1234567890` plus a 20-case proptest over `[2, 10_000]`.

Run all three with `cargo test --all-targets --all-features`.

## Documentation

All project documentation lives under [`docs/`](docs/README.md). The structure is:

- [docs/README.md](docs/README.md) — Index
- [docs/overview.md](docs/overview.md) — Project goals, scope, supported platforms
- [docs/architecture.md](docs/architecture.md) — System architecture, data flow, concurrency
- [docs/algorithms.md](docs/algorithms.md) — Mathematical foundation and pseudocode
- [docs/modules.md](docs/modules.md) — Module-by-module reference
- [docs/cli.md](docs/cli.md) — Full CLI reference
- [docs/configuration.md](docs/configuration.md) — Environment variables and constants
- [docs/observability.md](docs/observability.md) — Logging, tracing, audit boundaries
- [docs/performance.md](docs/performance.md) — Performance characteristics and tuning
- [docs/benchmarks.md](docs/benchmarks.md) — How to run the benchmarks
- [docs/testing.md](docs/testing.md) — Testing strategy and methodology
- [docs/deployment.md](docs/deployment.md) — Build, package, deploy
- [docs/operations.md](docs/operations.md) — Runbook (backup, monitor, scale)
- [docs/troubleshooting.md](docs/troubleshooting.md) — Common errors and resolutions
- [docs/security.md](docs/security.md) — Security model
- [docs/faq.md](docs/faq.md) — Frequently asked questions
- [docs/glossary.md](docs/glossary.md) — Terms and definitions
- [docs/references.md](docs/references.md) — External reading
- [docs/roadmap.md](docs/roadmap.md) — Future work
- [docs/adr/](docs/adr/README.md) — Architecture Decision Records

## Project Structure

```
find/
├── src/
│   ├── lib.rs          # Library root; exports ecc, error, search, config, telemetry
│   ├── main.rs         # CLI wrapper; tracing bootstrap
│   ├── config.rs       # Session configuration types and validation
│   ├── ecc.rs          # SEC1 parsing, point arithmetic, scalar conversion
│   ├── error.rs        # Unified FindError hierarchy
│   ├── orchestrator.rs # Session management and resume logic
│   ├── persistence.rs  # Checkpoint read/write with atomic operations
│   ├── search.rs       # Pure search engine, VariantIndex, binary caching
│   └── telemetry.rs    # Tracing initialization helpers
├── tests/
│   ├── audit.rs        # End-to-end key recovery verification
│   ├── differential.rs # Cross-implementation verification vs libsecp256k1
│   ├── integration.rs  # Randomized discovery and edge case tests
│   ├── kat.rs          # Known-Answer Tests for crypto primitives
│   └── orchestrator.rs # Session flow and checkpoint tests
├── benches/
│   └── bench.rs        # Criterion micro-benchmarks
├── fuzz/               # cargo-fuzz targets (parse_pubkey, hex_to_scalar, scalar_mul_g)
├── docs/               # Architecture, algorithms, operations, ADRs
├── .github/            # CI/CD, issue templates, dependabot
├── Cargo.toml          # Package metadata and dependencies
├── Makefile            # Developer automation commands
├── deny.toml           # cargo-deny license/advisory policy
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
| `make deny` | Run cargo-deny for license/dependency auditing |
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

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for details on:

- Fork and branch workflow
- Commit message conventions (Conventional Commits)
- Pull request process
- Code quality standards
- [Architecture Decision Records](docs/adr/README.md) for substantial design changes

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Security

For reporting security vulnerabilities, please see our [Security Policy](SECURITY.md). **Do not open public issues for security reports.** For the security model, see [docs/security.md](docs/security.md).

## License

This project is licensed under the [MIT License](LICENSE-MIT).

---

**Disclaimer:** This tool is for educational and research purposes only. See [DISCLAIMER.md](DISCLAIMER.md) for full legal terms.
