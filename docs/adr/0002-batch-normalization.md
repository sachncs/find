# ADR-0002: Montgomery Simultaneous Inversion for Batch Normalization

- **Status:** Accepted
- **Date:** 2026-04-12
- **Supersedes:** —
- **Superseded by:** —

## Context

The search engine must extract the X-coordinate of each point `j·G` in the sweep. The point is computed in projective coordinates `(X : Y : Z)` to keep addition cheap, but extracting the affine X-coordinate requires a modular division by `Z`:

```
x_affine = X * Z^(-1) (mod p)
```

Modular inversion via Fermat's little theorem, `a^(-1) ≡ a^(p-2) (mod p)`, costs one modular exponentiation — by far the dominant operation in the per-point pipeline. For a 256-bit prime, this is on the order of hundreds of microseconds. If performed independently for every point in a batch of `N`, total cost is `N` inversions.

With the hot path running millions of points per second, sequential normalization is the bottleneck — bench-marked at roughly **one inversion per point** in [benches/bench.rs::bench_batch_normalization](../../benches/bench.rs).

## Decision

We use **Montgomery's simultaneous inversion** to convert `N` projective points to affine form with a single modular inversion and `O(N)` additional multiplications.

The algorithm:

1. Compute prefix products `c_i = Π(Z_j)` for `j ∈ [0, i]`.
2. Invert `c_{N-1}` with one modular exponentiation: `c_{N-1}^(p-2)`.
3. Back-substitute: `Z_i^(-1) = c_{i-1} · c_i^(-1)`.
4. Multiply each `X_i` by `Z_i^(-1)` to obtain the affine X-coordinate.

The `k256` crate exposes this as `ProjectivePoint::batch_normalize(&points, &mut affines)`. We use a fixed batch size of **32 points**, which is empirically the sweet spot on x86_64 and aarch64 — see [performance.md](../performance.md#batch-normalization).

## Consequences

**Positive:**

- Per-point inversion cost drops from one full exponentiation to one multiplication plus one shared exponentiation across the batch. The benchmark shows a **~15–20× speedup** for the normalization phase on `BATCH_SIZE = 32`.
- API is a thin wrapper around the k256 implementation, so the cryptographic correctness is delegated to an audited crate.
- The 32-point batches fit in L1 cache, keeping memory traffic low.

**Negative:**

- All `N` input points must be available before normalization can begin, introducing a small latency cost. In the sweep, this is amortized by the per-batch parallelism of `rayon`.
- Stack-allocated batch arrays (`[ProjectivePoint; MAX_BATCH]`) impose a hard upper limit on batch size. We chose 32 specifically because the k256 batch API requires the input slice and output slice to have the same length; static arrays keep the allocation off the heap.
- A matched batch must be searched serially after normalization. The total batch latency is `(scalar muls + 1 inversion + N-1 multiplications + N lookups)` and is dominated by the scalar multiplications, not the normalization.

## Alternatives Considered

### 1. Sequential per-point normalization
Trivial to implement. Rejected on performance grounds: the benchmark shows it is 15–20× slower on the normalization phase. Total throughput would be bounded by the inversion rate.

### 2. Affine-only arithmetic
Compute and store every point in affine form from the start. Rejected because:
- Point addition in affine form requires a modular inversion per addition — *worse* than the projective approach.
- Storage grows by 2× per point (need to store both X and Y).
- Affine point additions also do not support the same constant-time guarantees as projective arithmetic.

### 3. Jacobian / Lopez-Dahab coordinates
Alternative projective coordinate systems with different operation costs. The k256 crate's `ProjectivePoint` already uses the most efficient representation for secp256k1, and `batch_normalize` is implemented for that type. Switching coordinate systems would require either re-implementing the batch logic or using a different crate.

### 4. Larger batch sizes (64, 128, 256)
Larger batches would reduce the relative cost of the single inversion further (theoretical limit: `N` inversions → 1 inversion). We chose 32 because:
- Stack allocation cost grows linearly with `N`; 32 × 96 bytes (pointive point) ≈ 3 KB is comfortable in L1.
- 32 scalar multiplications per batch complete in roughly the time of one batch normalization on modern x86_64, keeping the pipeline balanced.
- The benchmark shows no measurable benefit beyond `N = 32` on the target hardware.

### 5. Use a different `k256` API
The `k256` crate offers several other methods (`to_affine`, manual `invert` via field arithmetic). All of them amount to "sequential normalization" in cost. `batch_normalize` is the only amortized-inversion path.

## References

- Source: [`src/search.rs::sweep_parallel`](../../src/search.rs), [`src/search.rs::sweep_and_cache`](../../src/search.rs)
- Benchmark: [`benches/bench.rs::bench_batch_normalization`](../../benches/bench.rs)
- Algorithm: [algorithms.md#batch-normalization](../algorithms.md#batch-normalization)
- Wikipedia: <https://en.wikipedia.org/wiki/Montgomery%27s_modular_multiplication#Montgomery_simultaneous_inversion>
- Related: [ADR-0005](0005-pure-search-module.md) — the `search` module is pure so this decision is local to it
