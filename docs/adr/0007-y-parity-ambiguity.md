# ADR-0007: Y-Parity Ambiguity and External Validation

- **Status:** Accepted
- **Date:** 2026-06-26 (hardening pass)
- **Supersedes:** —
- **Superseded by:** —

## Context

The search engine matches X-coordinates of `j·G` against the X-coordinates of pre-computed shifted target points `P - V·G`. On secp256k1, the X-coordinate is the same for a point and its negation: `x(P) = x(-P)`. This is the algebraic foundation of the matching invariant — it halves the work for a given match probability — but it also means the engine cannot distinguish the Y-parity of the matched point.

When a match is found at variant `V` with scalar `j`, the engine emits two candidate private keys:

```
d ≡ V + j  (mod n)   [positive parity]
d ≡ V - j  (mod n)   [negative parity]
```

At least one of the two candidates is the true `d` if `j` is in the swept range. The engine has no way to know which one without additional information.

The Y-parity ambiguity is a **fundamental property of X-coordinate matching**, not a defect. Resolving it within the engine would require either:

1. Computing `j·G` and comparing Y-coordinates — adding 512×512 comparison work, defeating the purpose of the matching index.
2. Storing Y-coordinates in the variant index — doubling the index size and slowing the hot path.
3. Using a different matching primitive (e.g., a hash-based baby-step giant-step) — replacing the entire algorithm.

All three options are rejected as out of scope for the research-pedagogical use case.

## Decision

The engine **emits both candidates** and leaves Y-parity disambiguation to the caller. The caller is expected to:

1. Convert each hex candidate to a `Scalar`.
2. Compute `candidate · G`.
3. Compare the resulting public key to the target public key `P`.
4. The candidate whose `candidate · G == P` is the true `d`.

This is a single `scalar_mul_g` per candidate — `O(1)` work — and is trivial to perform downstream.

## Consequences

**Positive:**

- The engine's hot path remains X-coordinate-only, preserving the 15–20× batch normalization speedup and the sub-20 ns index lookup latency.
- External validation is a one-line operation in any secp256k1 library (`k256`, `secp256k1`, `bitcoin-secp256k1`).
- The engine is honest about its scope: it is a **search accelerator**, not a key-disambiguator.

**Negative:**

- The caller must perform two additional `scalar_mul_g` operations per match. This is `O(1)` per match, negligible compared to the search itself.
- The engine's success report (`render_success_report` in `src/main.rs`) explicitly labels the candidates as "candidates (d = V ± j)" to remind the operator that external validation is required.

## Alternatives Considered

### 1. Y-coordinate storage in the variant index
Store both X and Y coordinates per variant. Rejected because:
- The index doubles in size to ~32 KB, pushing it from L1 to L2 cache.
- The matching hot path requires comparing 64 bytes (X+Y) instead of 32 (X), slowing lookups.
- The `match_x` semantics in the API is preserved (no breaking change).

### 2. Y-parity resolution at match time
When a match is found, compute `j·G` and compare Y-coordinates against `P - V·G`. Rejected because:
- Adds a `to_affine` call per match (one extra modular inversion).
- The Y-coordinate comparison is a constant-time operation but adds ~hundreds of microseconds per match.
- The disambiguation can be done externally with the same cost.

### 3. Use a hash-based index (e.g., RIPEMD-160 of compressed pubkey)
Hash the candidate pubkey and store hash collisions. Rejected because:
- Adds hashing overhead to the hot path.
- The hash itself becomes a single point of failure (hash collisions would produce false matches).

## See also

- [algorithms.md#y-parity-ambiguity](../algorithms.md#y-parity-ambiguity) — algorithmic treatment
- [security.md#what-the-security-model-is-not](../security.md#what-the-security-model-is-not) — scope of the engine
- [`src/main.rs::render_success_report`](../../src/main.rs) — output formatting
- [`src/search.rs::VariantIndex::match_x`](../../src/search.rs) — the matching API
