# Benchmarks

The `find` tool includes a Criterion-based micro-benchmark suite that measures the two primary system bottlenecks:

1. **Batch normalization** — sequential vs. Montgomery-simultaneous inversion.
2. **Variant index lookup** — flat-array binary search latency.

This document describes how to run the benchmarks, how to interpret the results, and how the numbers relate to end-to-end search throughput.

## Running the benchmarks

### Standard run

```bash
# From the repository root
make bench

# Or directly
cargo bench
```

The first run is slow because Criterion collects statistical samples. Subsequent runs use the cached statistics and complete in seconds.

### Running a single benchmark

```bash
# Just the batch normalization benchmark
cargo bench --bench bench -- bench_batch_normalization

# Just the index lookup benchmark
cargo bench --bench bench -- bench_index_lookup

# A specific sub-benchmark
cargo bench --bench bench -- batch_normalization_32
```

### Adjusting sample size

```bash
# More samples for tighter confidence intervals
cargo bench -- --sample-size 200

# Faster runs with looser confidence intervals
cargo bench -- --sample-size 10
```

### Saving and comparing

```bash
# Save the current results as a baseline
cargo bench -- --save-baseline main

# Compare the current branch to the baseline
cargo bench -- --baseline main
```

Criterion reports percentage change with confidence intervals. A change of more than ~1% is usually significant for these benchmarks.

## Benchmark: batch normalization

**File:** `benches/bench.rs::bench_batch_normalization`
**Group:** `ecc_throughput`

The benchmark generates 32 points `1·G, 2·G, ..., 32·G` once and then measures the cost of converting them to affine form, comparing two strategies:

| Sub-benchmark | Strategy | Expected relative cost |
|---|---|---|
| `single_normalization` | 32 independent `to_affine()` calls | Baseline (1.0×) |
| `batch_normalization_32` | One `batch_normalize()` call | ~1/15 to 1/20 (0.05–0.07×) |

### What the numbers mean

- A 15–20× speedup in the normalization phase is the **expected** result on modern x86_64 and aarch64 hardware. Smaller speedups (e.g. 5–10×) indicate a hardware or compiler issue worth investigating.
- The absolute time per batch (in nanoseconds) scales with the scalar multiplication cost; this benchmark is dominated by the cost of producing the 32 input points.

### Interpreting regressions

If a future commit shows a 5% regression in `batch_normalization_32`, possible causes are:

- A change to the `k256` dependency.
- A change to the default `Config::batch_size` (formerly `BATCH_SIZE` constant in `search`).
- A change to the `ProjectivePoint::batch_normalize` call site (e.g. switching from `&[ProjectivePoint]` to a different slice type).
- Compiler upgrade affecting the inlined assembly.

The benchmark is sensitive to CPU frequency scaling; run with the governor set to `performance`.

## Benchmark: variant index lookup

**File:** `benches/bench.rs::bench_index_lookup`
**Group:** `flat_index_match`

The benchmark generates 512 variants for a target point and measures the cost of a single `VariantIndex::match_x` call.

| Sub-benchmark | Strategy | Expected latency |
|---|---|---|
| `flat_index_match` | Binary search on flat sorted array | Sub-20 ns per lookup on modern hardware |

### What the numbers mean

- A sub-20 ns lookup means the index fits entirely in L1 cache and the binary search is memory-bound.
- Regressions to 30+ ns indicate that the index has spilled to L2 (or worse, RAM). This is unusual for a 16 KB index; check the CPU frequency and the working set.

### Why not a hash table?

The flat sorted array is faster than a `HashMap` or `BTreeMap` for a 512-entry fixed-size set. The benchmark does not compare against a hash table directly, but the constant factor of hashing is the reason. See [ADR-0001](adr/0001-multi-variant-search.md) for the full discussion.

## Historical tracking

Criterion saves a CSV file at `target/criterion/<group>/<bench>/new/raw.csv` on each run. To track performance over time:

```bash
# Export the CSV
ls target/criterion/*/*/new/raw.csv

# Use a spreadsheet or a custom script to plot
```

For automated historical tracking, integrate the CSV output with a tool of your choice (e.g. `bencher`, GitHub Actions with artifact storage).

## Adding a new benchmark

To add a new micro-benchmark to `benches/bench.rs`:

1. Define a function `fn bench_<name>(c: &mut Criterion)`.
2. Use `c.benchmark_group("<group>")` to group related benchmarks.
3. Use `c.bench_function("<name>", |b| b.iter(|| { ... }))` to register a benchmark.
4. Add the function to the `criterion_group!` macro at the bottom of the file.

The new benchmark will be picked up automatically by `make bench` and `cargo bench`.

### Example template

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_my_new_feature(c: &mut Criterion) {
    let mut group = c.benchmark_group("my_feature");
    group.bench_function("baseline", |b| {
        b.iter(|| {
            // ... work to benchmark ...
            black_box(());
        })
    });
    group.finish();
}

criterion_group!(benches, bench_my_new_feature);
criterion_main!(benches);
```

## What the benchmarks do not measure

- **End-to-end search throughput.** The benchmarks isolate specific operations; a real search includes variant generation, checkpoint I/O, and orchestrator overhead. For end-to-end timing, use the wall-clock output of the binary itself (see [cli.md#on-success-match-found](cli.md#on-success-match-found)).
- **Disk I/O for binary caches.** Criterion runs in-memory; the cached sweep's I/O performance must be measured separately using `iostat` or `iotop` during a real run.
- **Rayon worker overhead.** The `find_map_any` early-exit cost is in the noise for the per-batch operations measured here. For full-process profiles, see [performance.md#profiling](performance.md#profiling).

## See also

- [performance.md](performance.md) — performance characteristics and tuning
- [operations.md#monitoring](operations.md#monitoring) — production monitoring
- [ADR-0001](adr/0001-multi-variant-search.md), [ADR-0002](adr/0002-batch-normalization.md) — algorithm-level decisions
