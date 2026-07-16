# 0009 — Deferred `batch_normalize` across 4-batch groups

- **Status:** Accepted
- **Date:** 2026-07-16
- **Supersedes:** —
- **Superseded by:** —

## Context

After the super-batch bootstrap chaining optimization (0008), the
remaining hot-path cost was:

| Operation | Per-batch cost | % of per-batch work |
|---|---|---|
| `+ G` chain (×31) | ~33.8 µs | ~82% |
| `batch_normalize` (×32 points) | ~7.25 µs | ~18% |

`batch_normalize` uses Montgomery's simultaneous inversion: 1 modular
inversion + O(N) multiplications. Measured costs:

| Points normalized | Time | Per-point |
|---|---|---|
| 32 (current) | 7.25 µs | 0.227 µs |
| 128 | 15.56 µs | 0.122 µs |
| 256 | 26.65 µs | 0.104 µs |

The single modular inversion dominates. Amortising it across more
points drops the per-point cost ~46% (32 → 128) and ~54% (32 → 256).

## Decision

Introduce a new constant `NORMALIZE_GROUP_BATCHES = 4` and
restructure the inner loop of `perform_chunked_sweep` and
`precompute_chunk` so that **4 consecutive batches share a single
`batch_normalize` call** on 128 points (4 × 32).

Per group:
1. **Phase 1** — generate all 128 projective points via the chained
   `+ G` loop.
2. **Phase 2** — single `batch_normalize` on the 128 points.
3. **Phase 3** — match each affine point batch-by-batch, writing
   cache blocks and checking `OnceLock` between batches for early
   exit.

The per-batch `+ G` chain is preserved (no change to the inner
arithmetic). Only the `batch_normalize` boundary is widened.

## Rationale

**Per-batch cost breakdown (128-pt groups vs 32-pt groups):**

| Component | 32-pt (old) | 128-pt (new) | Saving |
|---|---|---|---|
| `+ G` chain (31 adds) | 33.8 µs | 33.8 µs | — |
| `batch_normalize` | 7.25 µs × 4 = 29.0 µs | 15.6 µs | -13.4 µs (-46%) |
| `match_x` (32 × ~12ns) | 0.4 µs | 0.4 µs | — |
| **Total per 4-batch group** | **63.2 µs** | **49.8 µs** | **-13.4 µs (-21%)** |
| **Per batch** | **15.8 µs** | **12.5 µs** | **-3.4 µs (-21%)** |

## Consequences

### Positive

- **End-to-end sweep: 7.50 ms → 5.25 ms (−30 %).**
- **Random scalar < 2³² stress test (d ≈ 1.9 B, j ≈ 271 M):
  15.5 s → 10.6 s (−32 %).**
- All 112 existing tests pass; no API changes.
- The two code paths (`perform_chunked_sweep`, `precompute_chunk`)
  remain structurally symmetric — same group size, same early-exit
  semantics.

### Negative

- **Stack per Rayon task: ~48 KB → ~165 KB.** The group buffers are
  `[ProjectivePoint; 4 × MAX_BATCH]` (98 KB) + `[AffinePoint; 4 ×
  MAX_BATCH]` (66 KB) = 164 KB. Previously the per-batch buffers
  were 40 KB (used 8 KB at a time). This is still well within the
  default 2–8 MB Rayon worker stack, but on memory-constrained
  systems with very high worker counts it may need tuning.
- **Early-exit granularity: 4 batches instead of 1.** A match in the
  first batch of a group causes the remaining 3 batches' `match_x`
  to still run (after the group normalize). Worst case: 3 batches of
  useless match work. Negligible compared to the group normalize
  saving.
- **Criterion harness stack overflow with large `end`** — the
  `random_scalar_sweep_lt_2_32` benchmark must use a 2¹⁴ range
  (not 2³²) to fit the benchmark harness's stack budget. The
  runnable `examples/stress.rs` (not committed) covers the full
  2³² range.

## Alternatives considered

- **Group of 8 (256 points):** per-point cost drops further to
  0.104 µs (−54% from 32-pt). Stack usage doubles to ~330 KB per
  task. The extra 7% saving over 128-pt grouping doesn't justify
  the 2× stack pressure. Could be revisited if future workloads
  demand it.
- **Group of 2 (64 points):** per-point cost 0.155 µs (−32% from
  32-pt). Lower per-batch saving (1.5 µs) at half the complexity.
  Not worth the algorithm change for a modest gain.
- **Per-super-batch normalize (256 batches × 32 = 8 192 points):**
  8 192 × 0.087 µs ≈ 713 µs vs 256 × 7.25 µs = 1 856 µs — a
  ~62% saving. But 8 192-point buffers require 768 KB stack per
  task, which exceeds the default worker stack on most systems.
  Rejected.
- **Hand-rolled wNAF batch_normalize for sub-inversion cost:**
  k256's `batch_normalize` is already optimal (1 inversion + N−1
  muls). No further win available without rewriting the
  implementation. Rejected.