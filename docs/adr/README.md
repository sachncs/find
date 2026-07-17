# Architecture Decision Records

This directory contains the Architecture Decision Records (ADRs) for the `find` project. Each ADR captures a single significant design decision, the context in which it was made, the alternatives that were considered, and the consequences of the choice.

## Process

We follow a lightweight version of the [MADR](https://adr.github.io/madr/) template:

- **Status** — one of `Proposed`, `Accepted`, `Superseded`, `Deprecated`.
- **Context** — the forces at play, including the problem and the constraints.
- **Decision** — the chosen approach, stated as a full sentence.
- **Consequences** — the positive and negative outcomes of the decision.
- **Alternatives Considered** — other options and why they were rejected.
- **References** — pointers to the source code, external standards, and prior art.

## When to write an ADR

Write an ADR when a decision:

- Constrains future design choices (e.g. error model, persistence format).
- Has a non-obvious performance or correctness trade-off.
- Was the subject of meaningful debate during implementation.
- Is likely to be revisited or questioned by future contributors.

ADRs are **immutable once accepted**. If a decision changes, write a new ADR that supersedes the old one and update the index below.

## Index

| ID | Title | Status |
|---|---|---|
| [0001](0001-multi-variant-search.md) | Multi-variant range-splitting search | Accepted |
| [0002](0002-batch-normalization.md) | Montgomery simultaneous inversion for batch normalization | Accepted |
| [0003](0003-atomic-checkpointing.md) | Write-then-rename atomic checkpoints | Accepted |
| [0004](0004-error-hierarchy.md) | Single `FindError` enum | Accepted |
| [0005](0005-pure-search-module.md) | Pure `search` module with `CacheWriter` trait | Accepted |
| [0006](0006-binary-cache-format.md) | Raw 32-byte X-coordinate binary cache | Accepted |
| [0007](0007-y-parity-ambiguity.md) | Y-parity ambiguity: why the engine emits two candidates per match | Accepted |
| [0008](0008-mutex-poisoning-policy.md) | Mutex poisoning policy — **partially superseded** by [ADR-0006](../optimization-decisions/0007-oncelock-early-exit.md): `sweep_and_cache` no longer holds a `Mutex` (commit 6), so the recovery-policy example in 0008 no longer applies to it. 0008 still applies to the non-Unix `BinaryCacheWriter` fallback in `src/persistence.rs`. | Accepted (in part) |
| [0009](0009-runtime-batch-size.md) | Runtime-sized hot-path batch arrays: `Vec<T>` sized against `Config::batch_size ∈ 1..=256` | Accepted |
| [0010](0010-k256-bmi2-portable-scope.md) | k256-bmi2 scope is portable-only: BMI2/ADX removed (placeholder never delivered; deployment target is arm64; k256's `FieldElement` is private so a real drop-in requires a fork). The crate is now a correctness oracle, not an accelerator. | Accepted |
