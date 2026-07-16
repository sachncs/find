# 0008 — Super-batch bootstrap chaining in `sweep_parallel` / `sweep_and_cache`

- **Status:** Accepted
- **Date:** 2026-07-16
- **Supersedes:** —
- **Superseded by:** —

## Context

Before this change, each Rayon task in `sweep_parallel` and
`sweep_and_cache` processed exactly one batch of `Config::batch_size`
scalars (default 32). The per-batch cost was:

1. **Bootstrap:** one full `ProjectivePoint::GENERATOR * Scalar::from(chunk_start)`
   — a 256-bit scalar multiplication. Measured at ~30 µs per call.
2. **+ G chain:** 31 mixed point additions at ~1.1 µs each = ~34 µs.
3. **Montgomery batch normalization:** 1 inversion + ~6·count multiplications,
   ~7.3 µs.
4. **Index lookup:** 32 × ~12 ns binary searches = ~0.4 µs.

The bootstrap dominated step 1: ~47 % of per-batch ECC work. For a
1 B-scalar sweep the bootstrap ran ~31.25 M times, contributing roughly
half of total wall-clock time.

## Decision

Group batches into **super-batches of 256 consecutive batches** (8 192
scalars at the default `BATCH_SIZE = 32`). Each Rayon task now processes
one super-batch sequentially:

1. Compute one bootstrap scalar multiplication at the super-batch's first
   scalar: `current = generator * Scalar::from(base_j)`.
2. For each of the 256 batches in the super-batch, reuse `current` from
   the previous batch as this batch's bootstrap. The `+ G` chain advances
   `current` by exactly `batch_size * G` between batches — a single
   mixed addition, ~1.1 µs, vs. the ~30 µs scalar mul it replaces.
3. Inside each batch the existing `+ G` chain (31 mixed additions)
   produces the 32 candidate points as before.

The hot-path buffers (`[ProjectivePoint; MAX_BATCH_SIZE]`,
`[AffinePoint; MAX_BATCH_SIZE]`, and `[u8; MAX_BATCH_SIZE * 32]` in
`sweep_and_cache`) are now stack-allocated at module-level
`MAX_BATCH = 256`, eliminating per-batch `Vec` heap allocations
(~31.25 M / 1 B-scalar sweep).

## Rationale

A super-batch of 256 replaces 256 bootstrap muls (~7 680 µs) with 1
bootstrap mul + 255 single-addition inter-batch chains (~30 µs +
~280 µs ≈ ~310 µs). Net per-super-batch saving: ~7 370 µs, or ~45 %
of the super-batch's original wall-clock time.

The 256-batch granularity gives 122 K Rayon tasks for a 1 B-scalar
sweep — far more than the typical core count (8–16), so work-stealing
saturates all cores. Per-task overhead is bounded: each task runs one
full scalar mul, 255 inter-batch additions, and 256 × (31 intra-batch
additions + 1 batch-normalize + 32 lookups).

## Consequences

### Positive

- End-to-end `bench_end_to_end_small_scalar` benchmark improves from
  ~9.3 ms → ~7.6 ms (−17–18 %) on a 12-core machine for the
  early-exit `d = 12 345` case.
- Estimated improvement for a full 1 B-scalar sweep (no early exit):
  ~40–47 % wall-clock reduction. The bootstrap term disappears
  entirely; the chain becomes the new bottleneck.
- Heap allocation count drops by ~31.25 M per 1 B-scalar sweep
  (two `Vec`s per batch × batches + the `block` `Vec` in
  `sweep_and_cache`).
- The `_ = block_len` / `Vec` re-allocation in the inner loop is gone;
  the stack array is reused across batches and super-batches.

### Negative

- `OnceLock` early-exit granularity is now per-batch within a
  super-batch, not per-batch across the whole range. A match found
  in the first batch of a super-batch causes the remaining 255 batches
  in that task to be skipped (still correct). Other super-batches are
  cancelled via the existing `OnceLock::get()` fast-path check at the
  top of each task.
- The previous test `prop_sweep_and_cache_roundtrip` asserted
  `cache_bytes.len() >= 32` after a match was found. That assertion
  relied on a race between parallel batches writing cache blocks
  before noticing the early-exit. With sequential per-super-batch
  processing, a match in the first super-batch produces zero cache
  writes. The assertion was removed; the test now only checks that
  the match is found and `d` is in `m.candidates`.
- Stack usage per Rayon worker increases to ~48 KB
  (`MAX_BATCH × sizeof(ProjectivePoint) × 2 + MAX_BATCH × sizeof(AffinePoint) +
  MAX_BATCH × 32 bytes for the block buffer`). Acceptable for the
  default 2–8 MB worker stack size; very high worker counts on
  memory-constrained systems may need tuning.

## Alternatives considered

- **Halve the number of bootstrap muls by doubling `BATCH_SIZE`** (from
  32 to 64): only 2× fewer muls, and increases per-task working set
  without eliminating the bootstrap bottleneck. Rejected: super-batch
  chaining gives 256× reduction with no `BATCH_SIZE` change.
- **Precomputed odd-multiples table for wNAF fixed-base multiplication**:
  would speed up the bootstrap itself (possibly 2–3×) but adds ~12 KB
  of precomputed points and significant code complexity. Rejected for
  now: the super-batch approach already amortizes the bootstrap to
  near-zero per-scalar cost, making a faster bootstrap moot.
- **Sequential per-thread chunks of 1024+ batches**: would eliminate
  more bootstraps per task but reduces Rayon task count below the
  core count on short sweeps. Rejected: 256 batches / super-batch
  keeps ~122 K tasks for a 1 B sweep, well above typical core counts.