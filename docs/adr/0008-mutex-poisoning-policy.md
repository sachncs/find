# ADR-0008: Mutex Poisoning Policy

- **Status:** Accepted
- **Date:** 2026-06-26 (hardening pass)
- **Supersedes:** —
- **Superseded by:** —

## Context

The search engine and persistence layer use `std::sync::Mutex` for two distinct purposes:

1. **Cross-batch match coordination** in [`src/search.rs::sweep_and_cache`](../../src/search.rs): a `Mutex<Option<SearchMatch>>` shared across Rayon workers. The lock is held briefly to check for a match and to store a match.
2. **File-handle serialization** in [`src/persistence.rs::BinaryCacheWriter`](../../src/persistence.rs): a `Mutex<File>` that serializes `seek + write_all` on platforms without `pwrite_at` (non-Unix).

A `Mutex` is **poisoned** when a thread panics while holding the lock. The default behavior of `lock().unwrap()` is to panic again when called on a poisoned mutex. The default behavior of `lock().expect("...")` is to panic with the provided message. The `into_inner()` method recovers the inner value from a poisoned mutex.

The repository's previous code mixed these patterns: `lock().unwrap()` in the persistence layer, and `lock().is_ok_and(|g| ...)` followed by `if let Ok(mut guard) = lock()` in the search layer. This policy is now made explicit.

## Decision

The repository uses **two distinct mutex poisoning policies** depending on the use case:

### Policy A — Recover from poisoning (search engine)

For the cross-batch `match_found` mutex in `sweep_and_cache`:

- **Read path** (`is_ok_and` check): tolerates poisoning. If the mutex is poisoned, the check returns `false` and the worker proceeds. A poisoned mutex means a previous worker panicked; the data inside may be valid or `None` — either is acceptable for the early-exit optimization.
- **Write path** (`if let Ok(mut guard) = lock()`): tolerates poisoning. If the mutex is poisoned, the match is dropped. The orchestrator's outer loop will still detect the match via the `try_for_each` return value.

The `into_inner()` call at the end of `sweep_and_cache` explicitly handles poisoning:

```rust
let result = match match_found.into_inner() {
    Ok(r) => r,
    Err(poisoned) => {
        tracing::warn!("Precompute worker panicked; extracting partial result");
        poisoned.into_inner()
    }
};
```

This policy preserves robustness: a worker panic does not abort the entire search.

### Policy B — Panic with a clear message (persistence layer)

For the file-handle mutex in `BinaryCacheWriter`:

- `lock().expect("file cache writer mutex poisoned")` is used everywhere the lock is acquired.

The persistence layer is a **thin wrapper** over a `File`; a poisoned mutex there indicates a fundamental I/O corruption (e.g., a thread holding the lock panicked during a write, leaving the file in an unknown state). The safest action is to refuse further writes and let the caller observe the panic.

## Consequences

**Positive:**

- The search engine is **panic-tolerant**: a worker panic is logged and the search continues.
- The persistence layer is **fail-fast**: a poisoned mutex indicates a real problem and surfaces it immediately.
- The policies are documented in this ADR, eliminating the previous implicit inconsistency.

**Negative:**

- The two policies require a reviewer to understand which pattern applies where. The naming (`expect("...")` vs. `is_ok_and`) is the visual cue.
- A future contributor must read this ADR before changing the pattern in either layer.

## Alternatives Considered

### 1. Single policy: always recover via `into_inner()`
Trivial to apply uniformly. Rejected because it would mask real problems in the persistence layer (a poisoned file mutex indicates an I/O state that should not be silently recovered).

### 2. Single policy: always panic on poisoning
Trivial to apply uniformly. Rejected because it would make the search engine brittle to single-worker panics, which the engine's `rayon::panic_handler` is designed to tolerate.

### 3. Replace `Mutex` with `parking_lot::Mutex`
`parking_lot::Mutex` does not poison, eliminating the issue entirely. Rejected because:
- The repository is a research project; adding a new dependency for a single use case is not justified.
- The two policies above are well-understood and documented.

### 4. Use atomics instead of `Mutex<Option<SearchMatch>>`
The match storage could be an `AtomicPtr<SearchMatch>` or similar. Rejected because the match is a non-trivial struct (`SearchMatch` contains `String`s and a `Vec<String>`) that does not fit cleanly into an atomic.

## See also

- [`src/search.rs::sweep_and_cache`](../../src/search.rs) — Policy A implementation
- [`src/persistence.rs::BinaryCacheWriter`](../../src/persistence.rs) — Policy B implementation
- [`src/main.rs`](../../src/main.rs) — custom Rayon panic handler
