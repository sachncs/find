# Architecture

This document describes the system architecture of the Secp256k1 Find Tool.

## Design Philosophy

The system is built on three core pillars:

1. **Mathematical Minimality**: Reducing cryptographic overhead using projective coordinates and pre-computed caches
2. **Strict Resilience**: Guaranteeing search state integrity through atomic I/O and non-blocking observability
3. **High-Throughput Parallelism**: Leveraging work-stealing thread pools to saturate all available CPU resources

## System Overview

```
┌─────────────────────────────────────────────────────────┐
│                    CLI Layer (main.rs)                   │
│  • Argument parsing (clap)                              │
│  • Tracing initialization                               │
│  • Result rendering                                     │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│              Orchestrator Layer (orchestrator.rs)        │
│  • Session management                                   │
│  • Checkpoint resume logic                              │
│  • Component wiring                                     │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│              Search Layer (search.rs)                    │
│  • 512-variant generation                               │
│  • Parallel sweep engine                                │
│  • VariantIndex with O(log N) lookup                    │
│  • Binary cache management                              │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│              ECC Layer (ecc.rs)                          │
│  • SEC1 public key parsing                              │
│  • Scalar multiplication (k256)                         │
│  • Point arithmetic                                     │
│  • Batch normalization                                  │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│              Persistence Layer (persistence.rs)          │
│  • Atomic checkpoint I/O                                │
│  • Binary cache read/write                              │
│  • JSON variant export                                  │
└─────────────────────────────────────────────────────────┘
```

## Module Details

### 1. ECC Primitives (`ecc.rs`)

**Responsibility**: Low-level elliptic curve arithmetic on secp256k1.

Key functions:
- `parse_pubkey(hex_str)` — SEC1 v2.0 compliant public key parsing
- `hex_to_scalar(hex_str)` — Hex → Scalar field element with range validation
- `scalar_mul_g(d)` — Fixed-base scalar multiplication via k256
- `subtract(p, q)` — Projective point subtraction
- `to_hex_x(p)` — Affine X-coordinate extraction (identity-safe)

Design decisions:
- Uses projective coordinates (X:Y:Z) to avoid modular inversion during arithmetic
- Zero-copy buffer passing for coordinate slices to minimize allocator overhead
- Batch normalization via Montgomery's simultaneous inversion for 630x speedup

### 2. Search Engine (`search.rs`)

**Responsibility**: Implementation of the 512-variant range-splitting engine.

Key components:
- `generate_variants(target_p)` — Produces 512 variants (256 powers-of-2, 256 cumulative sums)
- `VariantIndex` — Flat sorted array with O(log N) binary search
- `perform_chunked_sweep(index, start, end)` — CPU-bound parallel ECC sweep
- `precompute_chunk(start, end, path, index)` — GPU-style batch normalization
- `perform_cached_sweep(index, path, start_j)` — I/O-bound sequential cache scan

Performance characteristics:
- Batch processing: 32 points per batch normalization
- Work-stealing parallelism via `rayon`
- Early exit on first match via `find_map_any()`
- Binary cache bypasses ECC arithmetic entirely for I/O-bound scans

### 3. Persistence Layer (`persistence.rs`)

**Responsibility**: Crash-safe state management and data export.

Features:
- Atomic checkpoint operations (write-then-rename)
- Binary cache validation (size checks, corruption detection)
- JSON export for variant data
- Sequential binary I/O for maximum throughput

### 4. Orchestrator (`orchestrator.rs`)

**Responsibility**: High-level session management.

Features:
- Checkpoint resume with cryptographic integrity verification
- Configuration management
- Component wiring
- Error recovery

### 5. Error Model (`error.rs`)

**Responsibility**: Unified error hierarchy across all layers.

Error types:
- `EccError` — Cryptographic operation failures
- `InvalidPublicKey` — Malformed input rejection
- `ResearchIntegrityError` — Checkpoint corruption detection
- `Io` — File system operations
- `HexError` — Encoding issues
- `SerializationError` — JSON/deserialization failures
- `CacheCorrupted` — Binary cache validation failures

## Data Flow

### Search Pipeline

1. **Input**: User provides HEX-encoded SEC1 public key
2. **Parsing**: `ecc::parse_pubkey` validates and decodes the key
3. **Variant Generation**: `search::generate_variants` produces 512 shift points
4. **Index Construction**: `VariantIndex` sorts variants by X-coordinate for O(log N) lookup
5. **Sweep**: Parallel scalar multiplication → batch normalization → X-coordinate matching
6. **Match**: When `x(j·G) == x(P-V·G)`, derive candidates `d = V ± j (mod n)`
7. **Output**: Display candidates and verification information

### Checkpoint Flow

1. After each 1-billion-point segment, serialize state to `data/checkpoint.json`
2. Use write-then-rename for atomic persistence
3. On resume, verify checkpoint integrity by recomputing X-coordinate
4. Continue from stored `last_j` value

## Mathematical Foundation

The algorithm exploits point symmetry on secp256k1:

Given target `P = d·G`, search for `(j, V)` such that:

```
x(j·G) = x(P - V·G)
```

This holds because `x(P) = x(-P)` on elliptic curves. When satisfied:

```
d = V + j  (mod n)   [positive parity]
d = V - j  (mod n)   [negative parity]
```

The 512 variants shift the search space by different `V` values, enabling parallel exploration of disjoint curve regions.

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Variant generation | O(512) | One-time per target pubkey |
| Index lookup | O(log 512) = O(1) | Binary search on flat array |
| Sweep (CPU) | O(R) | Linear over range R |
| Sweep (I/O) | O(R) | Sequential binary read |

## Scalability

### Vertical Scaling

- Utilizes all available CPU cores via work-stealing parallelism
- Batch normalization amortizes expensive operations
- Binary caching reduces CPU-bound work to I/O-bound

### Horizontal Scaling

- Binary cache files can be shared across machines
- Checkpoint files enable distributed search coordination
- Future: REST API for remote search management

## Security Considerations

- No unsafe Rust code in the codebase
- Checkpoint integrity verification prevents silent data corruption
- Input validation on all public key parsing
- Atomic file operations prevent partial writes

## Future Enhancements

- GPU acceleration via CUDA/OpenCL bindings
- Distributed search across multiple machines
- WebAssembly compilation for browser-based research
- REST API for remote search management
