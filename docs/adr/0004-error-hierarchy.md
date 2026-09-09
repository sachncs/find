# ADR-0004: Single `FindError` Enum for the Crate

- **Status:** Accepted
- **Date:** 2026-04-12
- **Supersedes:** —
- **Superseded by:** —

## Context

Every fallible function in the `find` crate needs an error type. The Rust ecosystem offers two broad patterns:

1. **Library errors** (a structured enum) for code that is reused as a library, where callers want to programmatically distinguish failure modes.
2. **Application errors** (`anyhow::Error` or similar) for binaries, where the only consumer is the process itself and a stringified message is sufficient.

The `find` crate is **both**: it ships a binary in `src/main.rs` and is intended to be reusable as a library for downstream research. Each fallible function therefore needs a structured error type, while the binary's `main` benefits from `anyhow`'s context-and-backtrace convenience.

## Decision

We define a single [`FindError`](../../src/error.rs) enum with seven variants:

| Variant | Cause |
|---|---|
| `EccError` | Low-level elliptic-curve failure (scalar overflow, identity point) |
| `ResearchIntegrityError` | Checkpoint anchor mismatch |
| `InvalidPublicKey` | SEC1 parsing failure |
| `Io` | File-system operation failure (`#[from] std::io::Error`) |
| `HexError` | Hex decoding failure (`#[from] hex::FromHexError`) |
| `SerializationError` | JSON serialization failure (`#[from] serde_json::Error`) |
| `CacheCorrupted` | Binary cache file is structurally invalid |

The enum derives `thiserror::Error`, `Clone`, `PartialEq`, and `Debug`. The crate exposes a `Result<T>` type alias. The binary's `main` wraps library errors into `anyhow::Error` at the top level.

## Consequences

**Positive:**

- **Single import.** All library code uses `crate::error::{FindError, Result}`. Callers do not have to learn a module-specific error hierarchy.
- **Programmatic matching.** Downstream library users can match on variants. The `ResearchIntegrityError` is a particularly important programmatic case: it indicates a checkpoint must be deleted or the search restarted.
- **Ergonomic `?` propagation.** The `#[from]` derives make `?` work for `io::Error`, `hex::FromHexError`, and `serde_json::Error` without manual conversion.
- **Test-friendly.** `Clone` and `PartialEq` enable assertions in tests (`assert!(result.unwrap_err().to_string().contains("...")`).
- **Layered error messages.** The `Display` impl prefixes each variant with a short subsystem label (`"ECC error: ..."`, `"I/O error: ..."`) so logs are greppable by subsystem.

**Negative:**

- **Single type for all subsystems.** Callers cannot pattern-match on the *origin* of an error (e.g. "was it the cache layer or the ECC layer?"). The current set of variants is small enough that this is not a practical problem.
- **`Clone` requires manual implementation** because `std::io::Error` and `serde_json::Error` are not `Clone`. We pay this cost in [src/error.rs](../../src/error.rs) in exchange for test ergonomics.

## Alternatives Considered

### 1. Use `anyhow::Result` everywhere
Trivial to write, but downstream library users cannot distinguish failure modes. The `ResearchIntegrityError` case alone justifies a structured type.

### 2. One error type per module (`EccError`, `SearchError`, `PersistenceError`, `OrchestratorError`)
A nested hierarchy that exposes the subsystem of origin. Rejected because:
- Callers must remember the type-conversion path.
- The `?` operator becomes verbose in the orchestrator, which composes all subsystems.
- The error *variants* (overflow, integrity, I/O) are more useful than the *subsystems* (ECC, persistence).

### 3. `thiserror` enum + `anyhow` wrapper at module boundaries
Use a structured enum in each module and convert to `anyhow` at the orchestrator. Rejected as adding ceremony without practical benefit — the current single-enum approach is more concise and equally informative.

### 4. `Box<dyn Error>` returns
Maximum flexibility, no information loss. Rejected because it forces dynamic dispatch on every error and prevents the use of `#[non_exhaustive]` patterns in callers.

### 5. Custom error trait (`pub trait FindErrorTrait: std::error::Error + Send + Sync + 'static`)
A more flexible alternative to a concrete enum. Rejected because the variant set is small and stable; a concrete enum is more discoverable in rustdoc.

## References

- Source: [`src/error.rs`](../../src/error.rs)
- Tests: [`src/error.rs::tests`](../../src/error.rs)
- Related: [ADR-0005](0005-pure-search-module.md) — the `search` module is one of the consumers of `FindError`
