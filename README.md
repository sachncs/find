# Secp256k1 Find Tool

[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-red.svg)](LICENSE-MIT)

**EDUCATIONAL AND RESEARCH USE ONLY.** This software is for pedagogical exploration of elliptic curve mathematics and high-performance Rust systems engineering. See [DISCLAIMER.md](DISCLAIMER.md).

A high-performance Rust system for secp256k1 private key discovery using a multi-variant range-splitting algorithm. It searches for scalars `j` and offsets `V` such that `x(j·G) = x(P - V·G)`, yielding key candidates `d = V ± j (mod n)`.

## Architecture

The system is organized as a layered pipeline:

```
src/
├── lib.rs          # Library root; exports ecc, error, search
├── main.rs         # CLI orchestrator; checkpointing, user interaction
├── ecc.rs          # SEC1 parsing, point arithmetic, scalar conversion
├── search.rs       # Parallel sweep engine, variant index, binary caching
└── error.rs        # Unified FindError hierarchy
```

### Core Components

**ECC Primitives (`ecc.rs`)**
- `parse_pubkey(hex_str)` — SEC1 v2.0 compliant public key parsing
- `hex_to_scalar(hex_str)` — hex → Scalar field element with range validation
- `scalar_mul_g(d)` — fixed-base scalar multiplication via k256
- `subtract(p, q)` — projective point subtraction
- `to_hex_x(p)` — affine X-coordinate extraction (identity-safe)

**Search Engine (`search.rs`)**
- `generate_variants(target_p)` — produces 512 variants (256 powers-of-2, 256 cumulative sums)
- `VariantIndex` — flat sorted array with O(log N) binary search for X-coordinate matching
- `perform_chunked_sweep(index, start, end)` — CPU-bound parallel ECC sweep via rayon
- `precompute_chunk(start, end, path, index)` — GPU-style batch normalization with parallel pwrite I/O
- `perform_cached_sweep(index, path, start_j)` — I/O-bound sequential cache scan

**Error Model (`error.rs`)**
Domain-specific enum covering: `EccError`, `InvalidPublicKey`, `ResearchIntegrityError`, `Io`, `HexError`, `SerializationError`, `CacheCorrupted`.

## Mathematical Invariant

For a target public key `P = d·G`, the system searches for `(j, V)` such that:

```
x(j·G) = x(P - V·G)
```

This holds due to point symmetry on secp256k1. When satisfied, `d` must be one of:

```
d = V + j  (mod n)   [positive parity]
d = V - j  (mod n)   [negative parity]
```

Each of 512 variants shifts the search space by a different `V`, enabling parallel exploration of disjoint curve regions.

## Performance Design

**Batch Normalization** — The k256 crate provides `ProjectivePoint::batch_normalize`, which amortizes a single modular inversion across 32 point normalizations via Montgomery's simultaneous inversion. This yields approximately 630x speedup in the normalization phase vs. sequential normalization.

**Variant Index** — Direct X-coordinate matching against 512 variants per scalar would require 512 byte-comparisons per point. The `VariantIndex` collapses this to a single `binary_search` over a flat `Vec<([u8; 32], usize)>` sorted by X-coordinate, yielding ~13ns lookup latency with optimal L1/L2 cache locality.

**Binary Caching** — Optional precomputation writes 32-byte SEC1 X-coordinates sequentially to a binary file, enabling the sweep phase to skip ECC arithmetic entirely and perform direct file scans. Throughput is gated by NVMe sequential read speed (~10-100x vs. CPU-bound ECC).

**Parallelism** — `rayon` provides work-stealing data-level parallelism via `into_par_iter().find_map_any()` across both the ECC sweep and the precomputation phases. Each worker batch-normalizes 32 points independently before the matching sweep.

## CLI Interface

```bash
find --pubkey <HEX_SEC1>          # Run search against target public key
find --pubkey <HEX> --cache-points # Generate binary cache during search
find --pubkey <HEX> --output-dir <DIR>  # Custom data/checkpoint directory
find --pubkey <HEX> --log-dir <DIR>     # Custom log directory
```

The CLI is stateful: it writes checkpoints to `data/checkpoint.json` after each 1-billion-point segment, using write-then-rename for atomic persistence. On restart, it verifies the checkpoint's cryptographic integrity against the stored X-coordinate before resuming.

**Checkpoint integrity guard** — The checkpoint stores `last_j` and the X-coordinate of `last_j·G`. On resume, the system recomputes `ecc::to_hex_x(scalar_mul_g(j))` and compares against the stored value. Mismatch causes an immediate `ResearchIntegrityError` rather than silent data corruption.

## Configuration

| Parameter | Value | Effect |
|---|---|---|
| `TRILLION` | 1,000,000,000,000 | Segment boundary for logging/pause |
| `CACHE_CHUNK_SIZE` | 1,000,000,000 | 1B points = ~32GB binary cache |
| `BATCH_SIZE` | 32 | Points per batch normalization |
| `MAX_SEARCH` | u64::MAX | Effectively 2^64 |

## Testing Strategy

28 tests across unit, integration, and audit suites:

- **Unit tests** (`src/ecc.rs`, `src/search.rs`): Algebraic invariants, edge cases, empty inputs, identity point handling, cache corruption detection
- **Integration tests** (`tests/integration.rs`): Randomized 6-8 digit scalars, boundary values, palindromic/repeating patterns, proptest property-based coverage, idempotency verification
- **Audit tests** (`tests/audit.rs`): Rigorous end-to-end key recovery for known scalars (1234567890, 7, 100, 1000, 99999), 8-phase verification including cryptographic proof that recovered scalars reproduce the target public key

See [TESTING.md](TESTING.md) for the full verification strategy.

## Build

```bash
make build    # Release binary (opt-level=3, lto=fat, panic=abort)
make test     # All 28 tests + doctests
make lint     # clippy + fmt check
make bench    # criterion microbenchmarks
```

**Release profile** (`Cargo.toml`):
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = 'abort'
strip = true
overflow-checks = true
```

## Observability

`tracing` provides structured logging via `tracing_appender::non_blocking` with daily rolling files. The subscriber is configured with an env filter and two output layers: stderr (human-readable) and a non-blocking file writer (JSON lines). Log levels are controlled via `RUST_LOG`.

Rayon is configured with a global panic handler that logs worker thread panics rather than aborting the process (in non-release builds; `panic = 'abort'` in release overrides this).

## Dependencies

| Crate | Version | Role |
|---|---|---|
| `k256` | 0.13 | secp256k1 arithmetic, SEC1 parsing |
| `rayon` | 1.8 | Data-level parallelism |
| `num-bigint` | 0.4 | Curve order arithmetic |
| `serde`/`serde_json` | 1.0 | Checkpoint/variant serialization |
| `tracing` | 0.1 | Structured logging |
| `clap` | 4.4 | CLI argument parsing |
| `thiserror` | 1.0 | Error enum derivation |

---

For algorithmic derivation, see [ALGORITHMS.md](ALGORITHMS.md). For system design trade-offs, see [ARCHITECTURE.md](ARCHITECTURE.md).
