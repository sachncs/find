# 0005 — `perform_cached_sweep` over a 32 KiB stack scratch buffer

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

The previous `perform_cached_sweep` looped `BufReader::read_exact`
over a 32-byte buffer. Although `BufReader` amortises reads
internally (its default 8 KiB buffer), each `match_x` call still
paid the BufReader state-machine cost and the `read_exact`
return-value check.

## Decision

Read directly into a 32 KiB stack scratch buffer with
`File::read`. Walk the buffer in 32-byte slices and probe the
index slice-by-slice; refill the buffer only when drained.

## Consequences

**Positive:**
- Removes the per-iteration BufReader bookkeeping.
- Larger buffer (32 KiB vs 8 KiB) reduces syscall frequency
  proportionally on cache scans.

**Negative:**
- 32 KiB stack per call (fits in the default 8 MiB stack).
- The cached_sweep function is no longer cancellation-safe at
  arbitrary 32-byte offsets; a partial read means the sweep ends
  cleanly at a 32-byte boundary.

## References

- Source: [`src/persistence.rs::perform_cached_sweep`](../../src/persistence.rs)