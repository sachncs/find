# 0002 — Cache `format!`-built variant labels in `OnceLock`

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

`generate_variants` builds 512 human-readable variant labels
(`"2^{i}"` and `"sum(2^0..2^{i})"`) per session. Each label is
constructed via `format!`, which allocates a fresh `String`. The
labels are deterministic — they depend only on the index `i`, not
on the target public key — so they are pure waste across sessions.

## Decision

Build the two `[String; 256]` label arrays once via
`std::sync::OnceLock` and return `&'static` references from a
private helper `variant_labels()`. The first call to
`generate_variants` initialises the cache; subsequent calls reuse
the allocations.

## Consequences

**Positive:**
- 0 heap allocations for labels after the first call.
- Eliminates 512 `format!` calls per session.

**Negative:**
- Process-global static state; tests that need to override labels
  cannot (we have none currently).

## References

- Source: [`src/search.rs::variant_labels`](../../src/search.rs)