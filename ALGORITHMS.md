# Algorithmic Reference

This document provides a formal mathematical and algorithmic breakdown of the discovery logic used in the `find` tool.

## Problem Statement

Given an elliptic curve point `P = d·G` on secp256k1, find the scalar `d ∈ [1, n-1]`. The naive approach is a linear search over all possible scalars, which is infeasible at scale.

## Core Algorithm: Multi-Variant Range-Splitting

The system exploits the symmetry of X-coordinates on secp256k1 to split the search space into 512 disjoint regions, each explored in parallel.

### Shift Variants

For each variant anchor `V`, compute a shifted target point:

```
P_V = P - (V · G)
```

The 512 variants use two anchor families:

**Binary anchors** — `V = 2^i` for `i ∈ [0, 255]`
**Cumulative anchors** — `V = Σ(2^0 .. 2^i)` for `i ∈ [0, 255]`

These cover both bit-aligned ranges and their cumulative counterparts, ensuring comprehensive curve coverage.

### Matching Invariant

For each scalar `j` in the sweep range, the system checks:

```
x(j · G) = x(P_V)
```

When equality holds, due to point symmetry on secp256k1 (`x(P) = x(-P)`), the discrete logarithm must satisfy one of:

```
d = V + j  (mod n)   [positive parity]
d = V - j  (mod n)   [negative parity]
```

Both candidates are produced for each match. The system does not know which parity is correct; both must be validated externally against the target public key.

### Mathematical Derivation

Given `P = d·G` and a match at variant `V` with scalar `j`:

```
x(j·G) = x(P - V·G)
       = x(P - V·G)          (by symmetry: x(-Q) = x(Q))
```

Case 1 — direct match: `j·G = P - V·G`
```
→ P = (V + j)·G
→ d ≡ V + j (mod n)
```

Case 2 — symmetric match: `j·G = -(P - V·G)`
```
→ P = (V - j)·G
→ d ≡ V - j (mod n)
```

The negative case requires a modular underflow guard: if `V < j`, compute `(n + V - j) mod n`.

## Batch Normalization

Coordinate extraction from projective to affine form requires a modular inversion of `Z`. Naive sequential normalization performs `N` inversions for `N` points.

The k256 crate provides `ProjectivePoint::batch_normalize`, which applies Montgomery's simultaneous inversion trick. For a batch of `N` points:

1. Compute prefix products `c_i = Π(Z_j)` for `j ≤ i`
2. Invert `c_{N-1}` with a single modular exponentiation `c_{N-1}^{n-2} mod n`
3. Back-substitute to obtain each `1/Z_i` from the prefix products

Complexity shifts from `N` inversions to `1` inversion + `O(N)` multiplications. For `N=32`, this yields approximately 630x speedup in the normalization phase on secp256k1.

## Complexity Analysis

| Operation | Complexity | Notes |
|---|---|---|
| Variant generation | `O(512)` | One-time per target pubkey; projective subtraction + to_affine |
| Index lookup | `O(log 512) = O(1)` | Binary search on flat sorted array of 512 entries |
| Sweep (CPU) | `O(R)` | Linear over range `R`; bounded by scalar multiplication throughput |
| Sweep (I/O) | `O(R)` | Sequential binary read; NVMe throughput ~GB/s |

The index is not a hash table — it is a flat `Vec<([u8; 32], usize)>` sorted by X-coordinate. This provides superior cache locality compared to a hash table for the fixed 512-entry variant set.

## Scalar Arithmetic

All arithmetic on candidate scalars is performed modulo the secp256k1 curve order:

```
n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
```

The negative candidate `V - j` is computed as `(n + V - j) mod n` to handle the case `V < j` without producing a negative intermediate value.

## Parallelism

The system uses `rayon`'s `into_par_iter().find_map_any()` for work-stealing parallelism:

- Range `[start, end]` is divided into batches of 32 scalars
- Each worker processes one batch: scalar multiplication → batch normalization → binary search
- `find_map_any` provides early-exit on first match — the first thread to find a hit terminates the entire search
- The `VariantIndex` reference is shared immutably across all workers (no locks required; the index is read-only after construction)

The global `PROGRESS` atomic counter accumulates across batch boundaries, allowing progress reporting across multiple cache chunks.

## Precomputation and Binary Caching

The optional precomputation phase (`precompute_chunk`) writes X-coordinates to a binary file for reuse across multiple target public keys:

- Format: 32 bytes per X-coordinate, sequential, little-endian representation
- File size: `32 * (end - start + 1)` bytes
- Workers write non-overlapping regions via `pwrite_at` (atomic on POSIX)
- On cache hit, `perform_cached_sweep` reads the binary file sequentially, bypassing ECC arithmetic entirely

The cache file is validated on open: if the file size is not a multiple of 32 bytes, a `CacheCorrupted` error is returned rather than silently producing wrong results.
