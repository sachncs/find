# 0010 — Group-level early-exit flag in `sweep_parallel`

- **Status:** Accepted
- **Date:** 2026-07-16
- **Supersedes:** —
- **Superseded by:** —

## Context

`sweep_parallel` uses Rayon's `find_map_any` to provide
early-exit when a match is found. `find_map_any` cancels other
tasks **between** iterations (i.e., between super-batches) but not
within them. A super-batch is 256 batches × 32 points = 8 192
scalars ≈ 0.9 ms of work.

When the match is in the second super-batch (e.g., j ≈ 12 345 in
the end-to-end benchmark), the other 11 Rayon workers all complete
their first super-batch (~0.9 ms each) before the cancellation
signal propagates. End-to-end wall-clock: ~5.25 ms, of which
~3.5 ms is wasted work in 11 threads.

## Decision

Add a shared `AtomicBool` checked **between normalize groups**
(once every 4 batches, ~0.09 ms of work). The first worker to
find a match sets the flag with `Ordering::Release`; other workers
observe it at the start of the next group with `Ordering::Acquire`
and abandon their super-batches immediately.

`OnceLock` was not used here because `find_map_any` already
provides the result-bearing channel; we only need the
cancellation signal.

## Rationale

The group-level check closes the cancellation window from
"between super-batches" (~0.9 ms granularity) to "between groups"
(~0.09 ms granularity). For the end-to-end benchmark with match
at j=12 345 (super-batch 2, group 2), the wasted work drops from
~3.5 ms (11 full super-batches) to ~0.2 ms (11 groups).

## Consequences

### Positive

- **End-to-end sweep: 5.25 ms → 2.00 ms (−62 % additional).**
- **Cumulative vs. pre-super-batch baseline: 9.3 ms → 2.0 ms
  (4.65× speedup, 78 % reduction).**
- No API changes.
- The `AtomicBool` check is a single relaxed-acquire load per
  group, ~1 ns. Cost is amortized across 128 point-processings.

### Negative

- Adds one `AtomicBool` per `sweep_parallel` call. Trivial.
- `Ordering::Acquire` is slightly more expensive than `Relaxed`
  but ensures the result of the matching worker's `find_map_any`
  return is visible to other workers. Necessary for correctness.

## Alternatives considered

- **`OnceLock<()>` instead of `AtomicBool`:** heavier primitive
  (boxed result slot), no benefit since we don't need to store
  a value — `find_map_any` carries the result.
- **Check between every batch instead of every group:** 64× more
  atomic loads per super-batch, ~64 ns of overhead per super-batch.
  The group-level check is sufficient because each group is
  ~0.09 ms and the matching worker's latency to `find_map_any`
  return is also ~0.09 ms. The work-in-flight is bounded by one
  group's worth, not one batch's worth.
- **Check between every point (inside the `+ G` chain):** would
  break the chain's data dependency — the chain is inherently
  sequential and can't check a flag between adds without
  serializing through memory.
- **Larger `NORMALIZE_GROUP_BATCHES` to make each check rarer:**
  8-batch groups were tested; the larger 256-point normalize
  buffer overflows the 8 MB thread stack with 12+ Rayon workers.
  4-batch groups are the sweet spot.