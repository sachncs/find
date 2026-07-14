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

- Range `[start, end]` is divided into batches of 32 scalars.
- Each worker processes one batch: scalar multiplication → batch normalization → binary search.
- `find_map_any` provides early-exit on first match — the first thread to find a hit terminates the entire search.
- The `VariantIndex` reference is shared immutably across all workers (no locks required; the index is read-only after construction).

The global `Progress` atomic counter accumulates across batch boundaries, allowing progress reporting across multiple cache chunks.

The custom Rayon panic handler in [`src/main.rs::main`](../src/main.rs) logs panics rather than aborting the process; see [observability.md#rayon-panic-handling](observability.md#rayon-panic-handling).

## Tuning the runtime environment

### CPU

- **Bind to physical cores.** Disable hyperthreading for consistent performance. Use `taskset` to pin the process to specific cores if the OS scheduler is causing cache thrash.
- **Disable frequency scaling.** Set the CPU governor to `performance` rather than `powersave` or `schedutil` to avoid frequency drops during long sweeps.

### Memory

- The search engine's heap usage is dominated by the `VariantIndex` (~16 KB) and the orchestrator's stack-allocated batch arrays (~3 KB per worker).
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

The hot loop in `precompute_chunk` / `perform_chunked_sweep` spends its cycles across five distinct operations:

| Operation | Per-batch cost | Notes |
|---|---|---|
| `scalar_mul_g(chunk_start)` (bootstrap) | ~256 field mults | One per batch |
| `current += generator()` (`+G` chain) | ~12 field mults | `count - 1` per batch |
| `batch_normalize(&points[..count])` | 1 inversion + ~6·count mults | Montgomery |
| `affine_x_bytes(affine)` | ~1 µs | Direct `AffineCoordinates::x()` |
| `index.match_x(&x_bytes, j)` | ~10 ns | Binary search in 16 KiB keys array |

The dominant cost is the **bootstrap scalar multiplication** in step 1, not the `+G` chain. For a typical 32-point batch the bootstrap takes ~80% of the time; the chain + normalize + match together take the remaining 20%. This means increasing `BATCH_SIZE` beyond ~64 has diminishing returns — the per-batch cost is dominated by the single bootstrap mul, not by the per-point chain.

## Optimization decisions

See [optimization-decisions/](optimization-decisions/) for the rationale behind each optimization in the current implementation:

- `0001-affinepoint-x-direct.md` — replacing `to_encoded_point` + `EncodedPoint::x()` with `AffineCoordinates::x()`
- `0002-variant-labels-once-lock.md` — caching `format!`-built labels in `OnceLock`
- `0003-packed-variant-index.md` — splitting `VariantIndex` into `keys + order` arrays
- `0004-atomic-flag-early-exit.md` — replacing per-batch `Mutex::lock` with an `AtomicBool` fast-path
- `0005-cached-sweep-stack-buffer.md` — `perform_cached_sweep` over a 32 KiB stack scratch buffer
- `0006-u256-decimal-no-biguint.md` — direct 256-bit divmod-by-10 instead of `BigUint::to_string`

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
