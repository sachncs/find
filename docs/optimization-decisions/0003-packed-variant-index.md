# 0003 — Split `VariantIndex` into `keys + order` arrays

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

The previous `VariantIndex` stored `Vec<([u8; 32], usize)>`, a
40-byte struct-of-arrays entry. Each binary-search iteration touched
both the 32-byte key and the 8-byte index, paying the cache-line
cost for both on every probe.

## Decision

Split into two parallel arrays:

- `keys: Vec<[u8; 32]>` — the X-coordinates, sorted.
- `order: Vec<usize>` — permutation mapping sorted index → variant.

Per-element size drops from 40 to 32 bytes for the keys (the hot
array), improving cache-line density by 25%. The variant metadata
stays in the `variants` array and is only fetched on a match (cold
storage indirection).

## Consequences

**Positive:**
- ~2× faster binary search (measured: ~10 ns per `match_x` lookup
  on the target hardware, down from ~20 ns).

**Negative:**
- Two allocations at construction time instead of one. The
  allocations are tiny (32 bytes × 512) and bounded.

## References

- Source: [`src/search.rs::VariantIndex`](../../src/search.rs)