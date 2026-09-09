# 0004 — `AtomicBool` fast-path in `sweep_and_cache`

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

`sweep_and_cache` previously opened a `Mutex<Option<SearchMatch>>`
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
- One additional `static` per `sweep_and_cache` invocation
  (negligible; on the order of bytes).

## References

- Source: [`src/search.rs::sweep_and_cache`](../../src/search.rs)