# ADR-0009: Runtime batch size (Config::batch_size as BatchSize newtype)

- **Status:** Accepted
- **Date:** 2026-07-15 (review-driven pass)
- **Supersedes:** —
- **Superseded by:** —

## Context

The hot-path batch arrays (`points: Vec<ProjectivePoint>`, `affines: Vec<AffinePoint>`, `block: Vec<u8>`) in `perform_chunked_sweep` and `precompute_chunk` were previously sized at compile time using the constant `MAX_BATCH = 32`. The runtime `--batch-size` CLI flag accepted values up to 256, but the engine silently ignored anything other than 32 because the stack arrays were stack-allocated with the wrong size.

This made it impossible to:

1. Experiment with batch sizes other than 32 without recompiling.
2. Capture per-batch time profiles in the full `[1, 256]` range claimed by the docs.
3. Reason about the per-batch memory budget at runtime.

The existing `MAX_BATCH: usize = 32` constant was a hard limit imposed by the stack allocation: `[T; 256]` of `[u8; 32]` would have been 8 KiB per array on the stack (L1-resident at 32 elements but spilling at 256), and three such arrays per call would have exceeded most stacks' default guard pages.

## Decision

Move the hot-path arrays from stack to heap allocation and track the runtime [`Config::batch_size`] (a new `BatchSize(u32)` newtype, see `src/config.rs`) at every layer.

- `points: Vec<ProjectivePoint>` — length `count` (= batch_size for non-tail batches)
- `affines: Vec<AffinePoint>` — length `count`
- `block: Vec<u8>` — length `count * 32`

The batch offset calculation tracks the runtime batch size:

```rust
let offset = batch_idx * (batch_size as u64) * 32;
```

Both `perform_chunked_sweep` and `precompute_chunk` gain a trailing `batch_size: u32` parameter; the orchestrator passes `config.batch_size.get()`.

The compile-time `MAX_BATCH` constant is removed from the crate surface (the array size is now runtime, no compile-time bound remains).

## Consequences

**Positive:**
- A single binary now supports the full `--batch-size 1..=256` range promised by the docs.
- The per-batch memory cost is dynamic: small sessions pay less, large sessions pay more (no wasted 3 KiB at batch_size=4).
- A 8-case proptest (`prop_batch_size_runtime` in `tests/integration.rs`) exercises the full range and verifies the resulting match is the same regardless of batch size.

**Negative:**
- A few hundred nanoseconds of allocator overhead per batch in the worst case (heap allocation is bounded by batch count per chunk; amortised across `BATCH_SIZE` scalars).
- The `Vec::with_capacity` calls in the inner loop are visible in flamegraphs (small but present).
- External code that called `perform_chunked_sweep` or `precompute_chunk` needs to be updated to pass `batch_size`.

## References

- Source: [`src/config.rs::BatchSize`](../../src/config.rs), [`src/search.rs::perform_chunked_sweep`](../../src/search.rs), [`src/search.rs::precompute_chunk`](../../src/search.rs)
- Tests: [`tests/integration.rs::prop_batch_size_runtime`](../../tests/integration.rs)
- ADR-0002 (batch normalization) — the algorithmic justification for batched processing
- Commits: 7a (the `BatchSize` newtype), 7b (the runtime-sized arrays)
