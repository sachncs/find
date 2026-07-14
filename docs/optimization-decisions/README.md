# Optimization Decisions

This directory contains short ADRs (Architecture Decision Records) that capture
the rationale behind each performance optimization in the current
implementation. The decisions are written in the same format as
[`../adr/`](../adr/) so they can be read end-to-end with the rest of the
design history.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-affinepoint-x-direct.md) | Use `AffineCoordinates::x()` instead of `to_encoded_point` | Accepted |
| [0002](0002-variant-labels-once-lock.md) | Cache `format!`-built variant labels in `OnceLock` | Accepted |
| [0003](0003-packed-variant-index.md) | Split `VariantIndex` into `keys + order` arrays | Accepted |
| [0004](0004-atomic-flag-early-exit.md) | `AtomicBool` fast-path in `precompute_chunk` | Accepted |
| [0005](0005-cached-sweep-stack-buffer.md) | `perform_cached_sweep` over a 32 KiB stack scratch buffer | Accepted |
| [0006](0006-u256-decimal-no-biguint.md) | Direct 256-bit divmod-by-10 instead of `BigUint::to_string` | Accepted |

The corresponding implementation commits can be found in
[`../../CHANGELOG.md`](../../CHANGELOG.md) under the **Commit Log** table.