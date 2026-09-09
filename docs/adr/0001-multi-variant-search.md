# ADR-0001: Multi-Variant Range-Splitting Search

- **Status:** Accepted
- **Date:** 2026-04-12 (initial release of v1.0.0)
- **Supersedes:** —
- **Superseded by:** —

## Context

The naive approach to recovering a private key `d` from a public key `P = d·G` on secp256k1 is a linear sweep over the scalar field `[1, n-1]`. For an attacker with no information about `d`, the expected work is `n/2 ≈ 2^127` scalar multiplications — infeasible at any plausible hardware budget.

The tool addresses the *research-pedagogical* problem of demonstrating how the search space can be **decomposed** into smaller, parallel-friendly subproblems using the symmetry `x(P) = x(-P)` of X-coordinates on elliptic curves. The decomposition must:

1. Be deterministic and reproducible for the same target `P`.
2. Be cheap to construct (one-time cost, ideally `O(1)` or `O(log n)`).
3. Enable fully parallel search with no inter-worker coordination beyond early-exit.
4. Produce a small set of candidates per match that can be trivially verified offline.

The use case is **educational**: small private keys, fast completion, focus on the algorithmic structure rather than brute force. The private keys are expected to be in the small-scalar range, e.g. `d < 10^9`.

## Decision

We split the scalar space into **512 disjoint shift variants** consisting of:

- **256 powers of two:** `V_i = 2^i` for `i ∈ [0, 255]`.
- **256 cumulative sums:** `V_i = Σ(2^0 .. 2^i) = 2^{i+1} - 1` for `i ∈ [0, 255]`.

For each variant, we compute the shifted target point `P_V = P - V·G` and store its X-coordinate in a flat, sorted array (the [`VariantIndex`](../../src/search.rs)). The engine then sweeps `j ∈ [1, MAX_SEARCH]` in parallel and tests `x(j·G) = x(P_V)` via `O(log 512)` binary search.

A match implies one of two candidate private keys:

```
d ≡ V + j (mod n)   [positive parity]
d ≡ V - j (mod n)   [negative parity]
```

The tool reports both candidates; external verification is required to disambiguate the Y-parity.

## Consequences

**Positive:**

- Search is **embarrassingly parallel** — batches are independent and the `VariantIndex` is read-only after construction.
- 512 variants provide dense coverage of bit-aligned and cumulative-scalar shift regions. For small `d` (the intended use case), at least one variant places the shifted target near the identity point where matches are common.
- Each variant is computed once per session (512 scalar multiplications and normalizations).
- The flat sorted array provides sub-20 ns lookups via L1/L2-resident binary search — see [`bench_index_lookup`](../../benches/bench.rs).
- Early-exit via `rayon::find_map_any` terminates the search on the first match without global coordination.

**Negative:**

- The strategy is **only effective for small scalars**. For uniformly distributed `d`, expected work is still `n/2 / 512 ≈ 2^118` per variant — still infeasible. The tool is honest about this limitation in its [disclaimer](../../DISCLAIMER.md).
- 512 variants consume ~512 × 32 bytes = 16 KB of memory for the index — negligible.
- The Y-parity ambiguity requires external verification of both candidates; the tool does not validate them.

## Alternatives Considered

### 1. Single shift (V = 0)
Trivially simple: just sweep `j·G` and check `x(j·G) = x(P)`. The tool *also* supports this implicitly when `V = 0` is the 2^(-∞) variant. The 512-variant approach is strictly more general: it covers the same search space as the single-shift case but **also** covers matches where the scalar is a power of two or cumulative sum offset from the target.

### 2. Hash-based variant index
A `HashMap<[u8; 32], usize>` keyed by X-coordinate provides `O(1)` average lookups. We rejected this because:
- For 512 entries, the constant factor of hashing dominates.
- The 512-entry sorted array fits in L1 cache (~16 KB), so binary search is faster in practice.
- A `BTreeMap` was also considered and rejected for the same L1-cache-locality reason.

### 3. Bloom filter on X-coordinates
A Bloom filter would be faster than a hash table lookup but introduces false positives, which would corrupt the candidate list. Rejected on correctness grounds.

### 4. 256 variants (powers of two only)
Halving the variant count saves construction time but degrades the discovery rate for cumulative-sum targets. Empirical testing in [tests/integration.rs](../../tests/integration.rs) shows the 512-variant version catches edge cases (palindromic, alternating, repeated-digit scalars) that the 256-variant version misses.

### 5. Different shift families (e.g. prime-based)
Prime-indexed variants would be more uniform but would not align with the binary decomposition of `d` for the small-scalar case. Rejected as over-engineering for the intended use case.

## References

- Source: [`src/search.rs::generate_variants`](../../src/search.rs), [`src/search.rs::VariantIndex`](../../src/search.rs)
- Tests: [`tests/integration.rs::run_controlled_test`](../../tests/integration.rs), [`tests/audit.rs::test_rigorous_recovery_1234567890`](../../tests/audit.rs)
- Algorithms: [algorithms.md#multi-variant-range-splitting](../algorithms.md#multi-variant-range-splitting)
- Benchmarks: [bench_index_lookup](../../benches/bench.rs)
- Related: [ADR-0002](0002-batch-normalization.md) for the cost of variant construction
