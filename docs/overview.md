# Overview

## Mission

The `find` tool is a high-performance Rust system for **educational and research exploration** of secp256k1 elliptic curve mathematics. It demonstrates:

- Multi-variant range-splitting algorithms for parallel scalar search.
- Montgomery's simultaneous inversion for batch coordinate normalization.
- Work-stealing data-level parallelism via `rayon`.
- Crash-safe state persistence with atomic checkpoints.
- Non-blocking observability with structured logging.

The project is **not** intended for, and must not be used for, recovering private keys belonging to others, commercial cryptanalytic deployment, or production environments handling sensitive material. See [DISCLAIMER.md](../DISCLAIMER.md) for full legal terms.

## Goals

- **Mathematical rigor.** Every algorithm is documented with its derivation, complexity, and correctness argument in [algorithms.md](algorithms.md).
- **Engineering excellence.** The codebase follows idiomatic Rust. It contains a single reviewed `unsafe` call in [`src/persistence.rs`](../src/persistence.rs) for `libc::fsync`; see [security.md](security.md) for the audit trail. The search domain is kept free of I/O concerns via the [`CacheWriter`](search.rs) trait. See [architecture.md](architecture.md) and the [ADRs](adr/README.md).
- **Reproducibility.** The full test suite, benchmarks, and verification methodology are documented in [testing.md](testing.md) and [benchmarks.md](benchmarks.md).
- **Maintainability.** Critical engineering decisions are captured as Architecture Decision Records under [docs/adr/](adr/README.md) so that future contributors understand the *why* behind the *what*.

## Non-Goals

- **Production key recovery.** The tool is for educational demonstration only.
- **GPU acceleration.** GPU support is on the [roadmap](roadmap.md) but is not a current objective; all optimization work focuses on CPU throughput via `rayon`.
- **Multi-curve support.** The tool is hard-coded to secp256k1. Adapting to other curves would require changes to the [variant generation](algorithms.md#multi-variant-search) and the project does not aim to provide a generic curve-agnostic framework.
- **REST API / remote management.** Out of scope for the current research mission.

## Scope

In scope:

- A single-curve (secp256k1), single-target (one public key per run) discovery engine.
- Parallel CPU sweep with optional binary cache pre-computation.
- Crash-safe checkpoint and resume.
- Structured observability.
- A clean public crate API (see [modules.md](modules.md)).

Out of scope:

- Multi-curve or multi-target batched search.
- Distributed or networked search.
- GUI or web interface.
- Any non-Rust re-implementation.

## Supported Platforms

| Platform | Architecture | Status |
|---|---|---|
| Linux | x86_64 | Fully supported (primary development platform) |
| Linux | aarch64 | Fully supported (CI-tested) |
| macOS | x86_64 | Fully supported (CI-tested) |
| macOS | aarch64 (Apple Silicon) | Fully supported (CI-tested) |
| Windows | x86_64 | Fully supported (CI-tested) |

The release pipeline builds and ships binaries for all five targets — see [maintenance/release.md](maintenance/release.md) and `.github/workflows/release.yml`.

## Compatibility Matrix

| Component | Minimum | Recommended |
|---|---|---|
| Rust toolchain | 1.70 (declared in `Cargo.toml`) | Latest stable |
| `k256` crate | 0.13 (pinned) | 0.13 |
| Operating system | Linux 5.x / macOS 11 / Windows 10 | Latest LTS |
| CPU | 2 physical cores | 8+ physical cores |
| RAM | 4 GB | 16 GB+ for cache-enabled searches |
| Disk | 10 GB | 100 GB+ NVMe SSD for binary caches |

## Dependency Rationale

| Dependency | Version | Purpose | Rationale |
|---|---|---|---|
| `k256` | 0.13 | secp256k1 arithmetic, point operations, batch normalization | Pure-Rust, audited, and provides Montgomery simultaneous inversion out of the box |
| `rayon` | 1.8 | Work-stealing data-level parallelism | De-facto standard for CPU-bound parallelism in Rust; provides `find_map_any` early-exit semantics |
| `clap` | 4.4 (derive) | Command-line argument parsing | Type-safe derive macros; widely used in the Rust ecosystem |
| `thiserror` | 1.0 | Library error types | Eliminates boilerplate for the [`FindError`](modules.md#error) hierarchy |
| `anyhow` | 1.0 | Application-level error handling | Used in `main.rs` for the binary's top-level error reporting |
| `serde` / `serde_json` | 1.0 / 1.0 | Checkpoint and variant export serialization | Industry standard; well-supported JSON output |
| `tracing` / `tracing-subscriber` / `tracing-appender` | 0.1 / 0.3 / 0.2 | Structured observability with daily-rolling logs | Non-blocking log writer avoids backpressure into the CPU-bound sweep |
| `hex` | 0.4 | Hexadecimal encoding/decoding | Minimal, focused, widely used |
| `num-bigint` | 0.4 | Big integer arithmetic for test helpers and `u256_to_decimal` | Provides `BigUint` for tests and serialization of variant offsets that exceed `u64` |
| `libc` (Unix) | 0.2 | `fsync` on parent directory for durable rename | Standard binding for POSIX filesystem operations |
| `proptest` (dev) | 1.5 | Property-based testing | Verifies algebraic invariants over the 64-bit scalar range |
| `tempfile` (dev) | 3.10 | Isolated test directories | Required for cache, checkpoint, and JSON-export tests |
| `rand` / `rand_chacha` (dev) | 0.8 / 0.3 | Deterministic RNG for the randomized discovery test | Seeded `ChaCha8Rng` keeps the test reproducible |
| `criterion` (dev) | 0.5 | Micro-benchmarks | De-facto Rust benchmarking harness |
| `num-traits` (dev) | 0.2 | Numeric trait imports for tests | Standard numeric trait support |

## Known Limitations

- **Single-target.** Only one public key may be searched per invocation.
- **64-bit scalar range.** The sweep operates over `[1, 2^64)`; larger scalars (i.e. those that exceed `u64::MAX`) cannot be expressed.
- **No GPU acceleration.** The hot path is CPU-only; see the [roadmap](roadmap.md).
- **No distributed search.** Single-process, single-machine. Sharing binary caches across machines is possible (see [operations.md](operations.md)) but no built-in coordination exists.

## Future Work

See [roadmap.md](roadmap.md) for the full forward-looking plan and a list of non-goals that will remain out of scope.
