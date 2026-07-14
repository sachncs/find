# 0004 — `AtomicBool` fast-path in `precompute_chunk`

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

`precompute_chunk` previously opened a `Mutex<Option<SearchMatch>>`
once per batch to check whether another worker had already found a
match. The lock acquisition + immediate drop paid for two atomic
operations per batch even when no match had been recorded.

## Decision

Add an `AtomicBool` (named `match_published`) that workers spin on
with `Ordering::Relaxed`. The fast path is now a single atomic load.
The mutex is acquired only when a worker actually has a match to
publish; the worker writes through the mutex and then flips the
flag with `Ordering::Release` so other workers observing the
publication via the next load see the written match.

## Consequences

**Positive:**
- Two fewer atomic ops per batch when no match has been recorded.
- Clearer separation: the mutex protects the match payload, the
  atomic flag protects the early-exit decision.

**Negative:**
- One additional `static` per `precompute_chunk` invocation
  (negligible; on the order of bytes).

## References

- Source: [`src/search.rs::precompute_chunk`](../../src/search.rs)