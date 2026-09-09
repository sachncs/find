# ADR-0005: Pure `search` Module with `CacheWriter` Trait

- **Status:** Accepted
- **Date:** 2026-04-12
- **Supersedes:** —
- **Superseded by:** —

## Context

The search engine needs to:

1. Compute `j·G` for a range of `j` in parallel.
2. Normalize batches of points to affine form.
3. Match each X-coordinate against the variant index.
4. Optionally write raw 32-byte X-coordinate blocks to a binary cache for reuse.
5. Report progress for telemetry.

Items 1–3 and 5 are pure compute — they depend only on `k256` and `rayon`. Item 4 is I/O — it requires a `File` and is platform-specific (e.g. `pwrite_at` on Unix vs `seek + write_all` on Windows). Item 5 is also I/O-adjacent: a `Progress` counter is updated concurrently from worker threads.

A monolithic module that mixes compute and I/O would have several problems:

- **Testing.** Tests for the search logic would need to set up a real file system or use extensive mocks.
- **Portability.** Platform-specific `pwrite` handling would leak into the search code.
- **Reusability.** Library users who want only the search logic (no cache, no I/O) would be forced to drag in the persistence layer.
- **Re-entrancy.** Global file handles or `static mut` state would complicate test setup.

## Decision

The `search` module is **pure**: it contains no `use std::fs`, no `use std::path`, and no platform-specific code. All I/O is injected via a small object-safe trait:

```rust
pub trait CacheWriter: Send + Sync {
    fn write_block(&self, offset: u64, data: &[u8]) -> std::io::Result<()>;
}
```

The `persistence` module provides the production implementation, [`BinaryCacheWriter`](../../src/persistence.rs). Tests provide trivial in-memory implementations (`NullWriter`).

Progress reporting is similarly injected: the search engine takes a `&Progress` argument and updates its counter. `Progress` is a thin wrapper over `AtomicU64` with `Relaxed` ordering — see [`src/search.rs::Progress`](../../src/search.rs).

The `CacheWriter` trait is **object-safe** (`Send + Sync` supertraits, no generics) so that the search code can be compiled without monomorphization on the writer type.

## Consequences

**Positive:**

- **Testability.** Tests for `sweep_parallel` and `sweep_and_cache` use a `NullWriter` that always returns `Ok(())`. No file system setup required.
- **Portability.** Platform-specific code (`pwrite_at` on Unix, `seek + write_all` on Windows) is isolated to the `BinaryCacheWriter` implementation in `persistence.rs`.
- **Reusability.** The `search` module can be used by library consumers who want only the compute pipeline, e.g. for in-memory benchmark harnesses.
- **Single responsibility.** The `search` module answers "given a variant index and a scalar range, where is the match?" The `persistence` module answers "how do I save things to disk?". The two are composed by the orchestrator.

**Negative:**

- **Trait dispatch overhead.** Each `writer.write_block(...)` call is a virtual dispatch. The call frequency is low (one call per 32-point batch), so the overhead is negligible in practice.
- **Boilerplate.** Callers must explicitly pass the writer and the progress counter. This is a small price for the testability benefits.
- **API surface.** The `CacheWriter` trait is a public item and is part of the library's stability contract. Adding new methods would be a breaking change.

## Alternatives Considered

### 1. Monolithic search module with direct `File` usage
The simplest implementation. Rejected for the testability, portability, and reusability reasons above.

### 2. Generic `W: CacheWriter` parameter (not trait-object)
A generic `sweep_and_cache<W: CacheWriter>(...)` would enable monomorphization and remove the virtual dispatch. The current implementation actually uses this pattern for `sweep_and_cache` (see [`src/search.rs::sweep_and_cache`](../../src/search.rs)). `sweep_parallel` does not need a writer and is therefore not generic.

### 3. Closures instead of a trait
Pass `&dyn Fn(u64, &[u8]) -> std::io::Result<()>` as the writer. Rejected because:
- The trait documents the contract (object-safety, `Send + Sync`).
- Closures do not have a stable name in rustdoc.
- A trait is more discoverable for downstream library users.

### 4. Channel-based writer
Have the search engine push X-coordinate blocks to a `crossbeam::channel`; a separate writer task drains the channel. This is more flexible (handles async I/O, compression, etc.) but adds a dependency and a layer of indirection. Rejected for simplicity — the current single-threaded write is fast enough that the parallelism benefit is negligible.

### 5. Pass `Arc<File>` directly
The `search` module could accept `Arc<File>` and call `pwrite_at` directly. Rejected because:
- Hard-codes a specific I/O mechanism.
- Leaks platform differences into the search code.
- The test code would need to construct a real `File`, defeating the `NullWriter` test pattern.

## References

- Source: [`src/search.rs`](../../src/search.rs) (trait definition and consumers), [`src/persistence.rs::BinaryCacheWriter`](../../src/persistence.rs) (production implementation)
- Tests: [`src/search.rs::tests::test_sweep_and_cache_finds_match`](../../src/search.rs), [`tests/orchestrator.rs::test_orchestrator_finds_small_scalar_with_cache`](../../tests/orchestrator.rs)
- Architecture: [architecture.md#search-layer](../architecture.md#search-layer)
