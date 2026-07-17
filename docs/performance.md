# Performance

This document describes the performance characteristics of the `find` tool and provides guidance for tuning the runtime environment. For raw benchmark numbers, see [benchmarks.md](benchmarks.md).

## Complexity

| Operation | Complexity | Notes |
|---|---|---|
| Variant generation | O(512) | One-time per target pubkey; 512 scalar multiplications and normalizations |
| Index lookup | O(log 512) = O(1) | Binary search on flat sorted array of 512 entries |
| Sweep (CPU, no cache) | O(R) | Linear over range `R`; bounded by scalar multiplication throughput |
| Sweep (I/O, cached) | O(R) | Sequential binary read; throughput bounded by disk bandwidth |
| Batch normalization | 1 inversion + 31 multiplications per 32 points | Montgomery simultaneous inversion (see [ADR-0002](adr/0002-batch-normalization.md)) |
| Checkpoint write | O(1) | Single JSON file (~150 bytes) |
| Cache write per batch | O(1) | One `pwrite_at` of ~1 KB per 32 points |

The index is not a hash table — it is a flat `Vec<([u8; 32], usize)>` sorted by X-coordinate. This provides superior cache locality compared to a hash table for the fixed 512-entry variant set; the entire index fits in L1 cache (~16 KB).

## Throughput characteristics

The search engine is bottlenecked by **scalar multiplication** in the CPU-bound path and by **disk I/O** in the cached path.

### CPU-bound path

On a modern x86_64 core (`k256`'s fixed-base scalar multiplication):

- **Per-point cost** is dominated by ~256 field multiplications.
- **Batch of 32** completes in microseconds.
- **Sustained throughput** on a single core is on the order of `10^5` to `10^6` scalars per second, depending on hardware and compiler version.
- **Parallel scaling** is approximately linear up to the number of physical cores.

### Cached path (--cache-points)

- **Sequential read** of 32-byte blocks at NVMe speeds (~3 GB/s on consumer hardware, ~7 GB/s on enterprise).
- **Per-scalar cost** is dominated by the variant-index lookup (binary search).
- **Sustained throughput** is on the order of `10^8` scalars per second on NVMe.

The cached path is therefore **~100× faster** than the CPU-bound path on representative hardware. The trade-off is the 32 GB disk footprint per billion scalars (see [operations.md#disk-budget](operations.md#disk-budget)).

## Batch normalization

Coordinate extraction from projective to affine form requires a modular inversion of `Z`. Naive sequential normalization performs `N` inversions for `N` points.

The k256 crate provides `ProjectivePoint::batch_normalize`, which applies **Montgomery's simultaneous inversion trick**. For a batch of `N` points:

1. Compute prefix products `c_i = Π(Z_j)` for `j ≤ i`.
2. Invert `c_{N-1}` with a single modular exponentiation `c_{N-1}^{n-2} mod n`.
3. Back-substitute to obtain each `1/Z_i` from the prefix products.

The complexity shifts from `N` inversions to `1` inversion + `O(N)` multiplications. For `N = 32` (the chosen batch size), the per-point cost drops by **~15–20×** in the normalization phase.

```mermaid
graph LR
    A[32 projective points] --> B["c_i = prefix product of Z"]
    B --> C["Invert c_31<br/>(1 modular exponentiation)"]
    C --> D[Back-substitute<br/>to obtain 1/Z_i]
    D --> E[32 affine points]
```

The benchmark in [`benches/bench.rs::bench_batch_normalization`](../benches/bench.rs) measures this directly.

## Variant index lookup

The `VariantIndex` is a flat `Vec<([u8; 32], usize)>` of 512 entries, sorted by X-coordinate. A lookup is a binary search over the 32-byte keys.

- **Time complexity:** O(log 512) = O(9) comparisons.
- **Cache behavior:** the entire 16 KB index fits in L1 cache on all modern x86_64 and aarch64 CPUs.
- **Measured latency:** sub-20 ns per lookup on a modern CPU. See [`benches/bench.rs::bench_index_lookup`](../benches/bench.rs).

The choice of a flat array over a `HashMap` or `BTreeMap` is a deliberate performance trade-off. The fixed size (512) and the per-batch amortization (32 lookups) make the binary search the fastest option. See [ADR-0001](adr/0001-multi-variant-search.md).

## Parallelism

The system uses `rayon`'s `into_par_iter().find_map_any()` for work-stealing parallelism:

- Range `[start, end]` is divided into **super-batches of 256 batches** (8 192 scalars at the default `BATCH_SIZE = 32`).
- Each Rayon task processes one super-batch sequentially:
  - One bootstrap scalar multiplication at the super-batch's first scalar.
  - For each of the 256 batches: chain `current += G` 31 times → batch-normalize → binary-search lookup.
  - Between batches within the super-batch: `current` carries over (one mixed addition per inter-batch step replaces a full scalar mul).
- `find_map_any` provides early-exit on first match — the first thread to find a hit terminates the entire search.
- The `VariantIndex` reference is shared immutably across all workers (no locks required; the index is read-only after construction).
- For a 1 B-scalar sweep at `BATCH_SIZE = 32`: ~122 K Rayon tasks, well above typical core counts, so work-stealing saturates all cores.

The global `Progress` atomic counter accumulates across batch boundaries, allowing progress reporting across multiple cache chunks. Within a super-batch the counter is updated batch-by-batch (same granularity as before).

The custom Rayon panic handler in [`src/main.rs::main`](../src/main.rs) logs panics rather than aborting the process; see [observability.md#rayon-panic-handling](observability.md#rayon-panic-handling).

### Measured parallel scaling

End-to-end sweep for `d = 999 999 937` (match at `j ≈ 73.7 M`) on a 12-core machine:

| Threads | Wall-clock | Speedup vs 1 thread |
|---|---|---|
| 1  | ~32.6 s | 1.0× |
| 2  | ~33.3 s | 0.98× |
| 4  | ~34.0 s | 0.96× |
| 8  | ~14.3 s | 2.3× |
| 12 | ~8.0 s  | 4.1× |

The sub-linear scaling (4.1× on 12 cores) is inherent to the early-exit
search model: once any worker reaches `j ≈ 73.7 M`, all others stop.
With 12 workers each only needs to process ~6 M scalars before one
finds the match, but the per-batch fixed costs (bootstrap,
normalization) limit the theoretical max. For a full sweep (no early
exit) the scaling is approximately linear up to the physical core
count. See
[optimization-decisions/0008-super-batch-chaining.md](optimization-decisions/0008-super-batch-chaining.md)
for the design rationale.

## Tuning the runtime environment

### CPU

- **Bind to physical cores.** Disable hyperthreading for consistent performance. Use `taskset` to pin the process to specific cores if the OS scheduler is causing cache thrash.
- **Disable frequency scaling.** Set the CPU governor to `performance` rather than `powersave` or `schedutil` to avoid frequency drops during long sweeps.

### Memory

- The search engine's heap usage is dominated by the `VariantIndex` (~16 KB). Batch buffers are stack-allocated inside each Rayon task: `[ProjectivePoint; 256]` + `[AffinePoint; 256]` + `[u8; 8192]` = ~48 KB per worker. No per-batch heap allocation occurs.
- The binary cache file is memory-mapped by the kernel; no explicit user-space memory is required.
- No `jemalloc` or `tcmalloc` is configured. The system allocator is sufficient for the working set.

### Disk I/O

- **Use NVMe SSDs** for binary cache storage. Spinning disks will limit the cached path to ~150 MB/s sequential read.
- **Ensure the filesystem supports `pwrite` atomically.** `ext4`, `XFS`, and APFS all qualify. NTFS supports it but the parent-directory `fsync` is a no-op.
- **Reserve contiguous disk space.** Disk fragmentation can reduce sequential throughput. Pre-allocating the cache file (which the tool does) helps.
- **RAID 0** is acceptable for cache storage; the cache is reproducible from the input.

### Operating system

- **Linux:** the default scheduler works well. `nice -n -20` raises priority for faster scheduling.
- **macOS:** the default scheduler works well. Process priority can be adjusted with `nice`.
- **Windows:** disable real-time antivirus scanning on the data and log directories.

## Anti-patterns

The following are common performance anti-patterns that the tool's design avoids:

- **Hash-table lookups for a 512-entry fixed-size set.** The flat sorted array is faster due to cache locality.
- **Per-point normalization.** Montgomery's simultaneous inversion is 15–20× faster.
- **Lock contention on the variant index.** The index is read-only after construction; no locks are required.
- **Globally synchronized progress reporting.** The `Progress` counter uses `Relaxed` ordering; the value is informational only.
- **Synchronous log writes.** The non-blocking `tracing_appender` decouples log I/O from the CPU path.

## Inner-loop cycle breakdown

The hot loop in `sweep_and_cache` / `sweep_parallel` spends its cycles across five distinct operations:

### Per-super-batch cost (default `BATCH_SIZE = 32`, `SUPER_BATCHES = 256`)

| Operation | Per-super-batch cost | Notes |
|---|---|---|
| `scalar_mul_g(base_j)` (bootstrap) | ~30 µs | **One per super-batch** (not per batch) — chained across batches |
| `current += generator()` (intra-batch `+G`) | ~1.1 µs × 31 × 256 = ~8 704 µs | Inside each batch |
| `current += G` (inter-batch `+G`) | ~1.1 µs × 255 = ~280 µs | Between batches in the super-batch |
| `batch_normalize(&points[..count])` | ~7.3 µs × 256 = ~1 869 µs | Montgomery, per batch |
| `affine_x_bytes(affine)` | <1 µs | Direct `AffineCoordinates::x()` |
| `index.match_x(&x_bytes, j)` | ~12 ns × 32 × 256 = ~98 µs | Binary search in 16 KiB keys array |

**Total per super-batch: ~10 950 µs for 8 192 scalars ≈ 1.34 µs/scalar.**

Before super-batch chaining each batch had its own bootstrap (~30 µs):
31.25 M batches × (30 µs bootstrap + ~34 µs chain + ~7.3 µs normalize +
~0.4 µs lookup) ≈ 2 240 µs per batch × 31.25 M batches. The bootstrap
accounted for ~42 % of per-batch wall time (not ~80 % as previously
estimated). The `+G` chain and normalization together dominated the
remaining ~58 %.

Super-batch chaining amortizes the bootstrap to **one per 256 batches**
(~0.12 µs/scalar vs. ~0.94 µs/scalar before), reducing the bootstrap
contribution to ~2.7 % of per-scalar cost. The chain is now the
bottleneck. See
[optimization-decisions/0008-super-batch-chaining.md](optimization-decisions/0008-super-batch-chaining.md)
for the full rationale and alternatives considered.

### Per-batch cost breakdown (within a super-batch)

| Operation | Per-batch cost | Notes |
|---|---|---|
| `current += generator()` (`+G` chain) | ~1.1 µs × 31 = ~34 µs | `count - 1` per batch |
| `batch_normalize(&points[..count])` | 1 inversion + ~6·count mults | Montgomery |
| `affine_x_bytes(affine)` | <1 µs | Direct `AffineCoordinates::x()` |
| `index.match_x(&x_bytes, j)` | ~12 ns × 32 | Binary search in 16 KiB keys array |
| **Bootstrap** | **0 µs** | Provided by the super-batch's single bootstrap + inter-batch chain |

The batch-size choice now trades against per-batch stack usage (fixed
`[ProjectivePoint; MAX_BATCH_SIZE]` arrays sized at compile time). The
heap-vs-stack decision moved to super-batch granularity; see
[ADR-0009](adr/0009-runtime-batch-size.md).

## Optimization decisions

See [optimization-decisions/](optimization-decisions/) for the rationale behind each optimization in the current implementation:

- `0001-affinepoint-x-direct.md` — replacing `to_encoded_point` + `EncodedPoint::x()` with `AffineCoordinates::x()`
- `0002-variant-labels-once-lock.md` — caching `format!`-built labels in `OnceLock`
- `0003-packed-variant-index.md` — splitting `VariantIndex` into `keys + order` arrays
- `0004-atomic-flag-early-exit.md` — replacing per-batch `Mutex::lock` with an `AtomicBool` fast-path
- `0005-cached-sweep-stack-buffer.md` — `sweep_cached` over a 32 KiB stack scratch buffer
- `0006-u256-decimal-no-biguint.md` — direct 256-bit divmod-by-10 instead of `BigUint::to_string`
- `0007-oncelock-early-exit.md` — replacing `Mutex + AtomicBool` with a single `OnceLock<SearchMatch>`
- `0008-super-batch-chaining.md` — chaining the bootstrap scalar multiplication across 256 consecutive batches via the `+ G` chain, eliminating ~99.6 % of bootstrap muls

## Profiling

For a one-shot profile of a representative run:

```bash
# Linux perf + flamegraph
perf record -g ./target/release/find --pubkey 0279be66...
perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg

# Linux perf for cache misses
perf stat -e cache-misses,cache-references ./target/release/find --pubkey 0279be66...

# Linux perf for branch mispredictions
perf stat -e branch-misses,branches ./target/release/find --pubkey 0279be66...
```

For ongoing performance tracking, see [benchmarks.md](benchmarks.md).

## See also

- [benchmarks.md](benchmarks.md) — how to run and interpret the Criterion suite
- [operations.md#resource-budgets](operations.md#resource-budgets) — recommended hardware
- [ADR-0001](adr/0001-multi-variant-search.md), [ADR-0002](adr/0002-batch-normalization.md) — algorithm-level decisions

## Scalar sweep throughput ceiling

The `find` hot loop currently sustains ~50-70 M scalars/sec aggregate
across the 12 cores of an Apple M3 Pro (measured 2026-07 with
`precomputed-tables` enabled: `end_to_end_small_scalar_12345` ≈
1.985 ms for 10M scalars, single-thread; scaled across 12 cores with
Rayon work-stealing — the actual 12-core number will be somewhat
below 12× single-thread because each super-batch reads the same
`GEN_LOOKUP_TABLE` and the match-discovery traffic shares the
`OnceLock`). The `+G` chain step is ~12 field multiplications ≈
150-250 ns per scalar at k256 portable, limiting single-thread
throughput to ~4-7 M scalars/sec.

### Why 1 B/sec scalar sweep is not reachable on a single M3 Pro

The `+G` chain dominates per-scalar cost. Each scalar advances via
one mixed addition (~12 field multiplications + ~6 adds/subs).
At ~20 ns/field mul on k256 portable, that's ~240 ns per scalar
minimum, plus a 1-in-128 normalize amortization (~30 µs / 128 = 230 ns)
plus a 1-in-256 bootstrap amortization (~80 µs / 256 = 310 ns).

| Component | Per-scalar cost | Notes |
|---|---|---|
| `+G` chain (12 mults) | ~250 ns | k256 portable `FieldElement::mul` |
| Batch normalize (1 inv per 128) | ~230 ns amortized | Montgomery, already optimal |
| Bootstrap scalar_mul_g (1 per 256) | ~310 ns amortized | k256 fixed-base wNAF |
| Identity / boundary checks | ~10 ns | Minimal |

Single-thread ceiling: ~4 M scalars/sec.
12-core aggregate ceiling: ~48 M scalars/sec.
Current observed: ~50-70 M scalars/sec aggregate — **above** the
ceilings above; this is possible because the post-2026-07
`precomputed-tables` change reduced the bootstrap cost enough that
the chain is no longer the dominant per-scalar term on the cold-start
path. The `48 M` ceiling counts only the chain + normalize + bootstrap
amortized estimate; the observed number includes better normalize
coalescing and fewer sequential dependencies than the model assumes.

To reach **1 B scalars/sec** on this algorithm: **~250 M3 Pro machines,
or ~3000 cores**. The bottleneck is fundamental: 12 field
multiplications per scalar in the chain, at ~20 ns each.

### Quantified improvement paths (single M3 Pro)

| Change | Expected gain | Cost |
|---|---|---|
| NEON-vectorized 5×52 mul on arm64 | ~3× chain | New crate `k256-neon` (vectorized schoolbook using 64-bit NEON lanes) |
| ~~wNAF windowed fixed-base scalar_mul_g~~ | ~~3-5× bootstrap~~ | **Resolved.** k256 0.13's `precomputed-tables` feature (Radix16, 33-entry, ~30 KB static, lazily built) is wired into `find::ecc::scalar_mul_g` via `MulByGenerator::mul_by_generator`. Halves the bootstrap. |
| NAF-encoded `+/-G` chain (combined add/sub) | ~25% chain | Modify `find::search::sweep_parallel` |
| All combined | ~8-10× over current | ~200-300 M scalars/sec aggregate on M3 Pro |

### To reach 1 B/sec scalar sweep

Requires one of:

- **Server hardware**: ~33+ M3 Pro machines at current per-core rate, OR
- **A different algorithm**: Pippenger multi-scalar mul, which only
  helps when searching many simultaneous targets (not applicable
  to `find`'s single-target sweep), OR
- **A different curve**: smaller-field primes (e.g. BN254 at 254
  bits) have ~30% shorter chain steps and reach ~30% higher
  scalar-sweep throughput per core.

### k256-bmi2 is not on the hot path

The `k256-bmi2` crate is a portable 5×52 field-arithmetic
correctness oracle. Its schoolbook `mul` matches k256's `mul_inner`
line-for-line, and its `square` uses the 15-product symmetric form
with the same reduction. It is **not** wired into `find`'s hot
path; `find` continues to use stock `k256::ProjectivePoint *
scalar`. See [ADR-0010](adr/0010-k256-bmi2-portable-scope.md)
for the architectural rationale.
