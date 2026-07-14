# 0007 — `OnceLock<SearchMatch>` replaces `Mutex<Option<SearchMatch>> + AtomicBool` in `precompute_chunk`

- **Status:** Accepted
- **Date:** 2026-07-15 (review-driven pass)
- **Supersedes:** —
- **Superseded by:** —

## Context

`precompute_chunk` previously used a two-part cross-worker coordination mechanism:

1. A `Mutex<Option<SearchMatch>>` holding the first discovered match.
2. An `AtomicBool` named `match_published` flipped to `true` under `Release` ordering the moment the mutex recorded a match.

The pattern worked, but it had three subtle costs:

- The fast-path check (`if match_published.load(Relaxed)`) was a separate atomic load from the mutex acquisition. Workers paid two atomic ops per batch in the no-match case.
- Reasoning about the `Release`/`Acquire` pair required careful thought about which thread observed what; this is fragile to refactor.
- The `Mutex` itself could be poisoned if a worker panicked while holding the lock. `into_inner()` recovers the inner value, but the panic branch is a code path that needs testing.

## Decision

Replace the two-part mechanism with a single `std::sync::OnceLock<SearchMatch>`. The check at the top of each batch becomes `if match_once.get().is_some() { return Ok(()); }`. A worker that finds a match publishes via `let _ = match_once.set(m);`. The final extraction is `Ok(match_once.into_inner())`.

`OnceLock` provides the same happens-before guarantees as our manual Acquire/Release pairing — first write wins, later writes are observed as `Err(T)` and discarded — and its `get()` is implemented as a single Acquire-load against thread-local state.

## Consequences

**Positive:**
- One fewer atomic op per batch in the no-match case (the `get()` is cheaper than the manual `Relaxed` load on most platforms because of inline caching).
- No mutex at all: categorically no poison handling, no `is_ok_and` workarounds, no `into_inner()` recovery path.
- Smaller state: 16 bytes (the OnceLock) vs 16 + 1-byte-aligned for the (mutex, bool) pair.

**Negative:**
- `OnceLock<T>` requires `T: Sync`, which [`SearchMatch`] satisfies (the inner `String` fields are `Send + Sync` and `Scalar` is `Send + Sync`). No actual negative.
- One static allocation per `precompute_chunk` invocation (negligible: tens of bytes per sweep chunk).

## References

- Source: [`src/search.rs::precompute_chunk`](../../src/search.rs)
- ADR-0008 (mutex poisoning policy) — implicitly superseded for this specific use case
- Commit: 6

[`SearchMatch`]: ../../src/search.rs
