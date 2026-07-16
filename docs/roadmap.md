# Roadmap

This document describes the project's direction. It is **not** a contract — items may be removed, reprioritized, or kept indefinitely as research scope dictates.

## Current Status

The crate is at **v0.1.6**. The `master` branch carries the **review-driven pass** in commits 1–18 — a sequence of safety, correctness, and API improvements reviewed against the elite-Rust checklist in `todo.md`. The next release is **v0.2.0**, a SemVer-minor bump because commits 7a, 7b, 7c, and 12 are breaking API changes (see [CHANGELOG.md](../CHANGELOG.md) and the [Migration table in README.md](../README.md#migration-016--020)).

All near-term items in the previous roadmap have shipped as part of commits 1–18 — the **Recently delivered** table below consolidates them.

## Recently delivered (commits 1–18)

| Item | Commit(s) | Documentation |
|---|---|---|
| Removed `unsafe` from `u256_to_decimal` | 1 | code inline (no ADR) |
| Tightened the `libc::fsync` `// SAFETY:` block | 2 | inline (no ADR) |
| `Config::validate_pubkey` (deep SEC1 fail-fast) + `FindError::InvalidConfig` | 3 | CHANGELOG |
| `to_hex_x` ↔ `x_bytes` round-trip regression test (`tests/kat.rs`) | 4 | code inline |
| `to_hex_x` uses `AffineCoordinates::x()` directly | 5 | opt-decision 0001 (predates commit) |
| `OnceLock<SearchMatch>` in `sweep_and_cache` | 6 | [opt-decision 0007](../optimization-decisions/0007-oncelock-early-exit.md) |
| `BatchSize` newtype + `try_with_*` fallible builders | 7a | CHANGELOG |
| Heap-allocated hot-path batch arrays + `Config::batch_size` honoured at runtime | 7b | [ADR-0009](../adr/0009-runtime-batch-size.md) |
| Interned `&'static [OffsetVariant]` + `compute_variant_x_bytes` helper | 7c | opt-decisions 0002 |
| Removed `SweepRange` dead newtype | 8 | CHANGELOG |
| Required-for-merge `cargo miri` job in CI | 9 | CHANGELOG + CONTRIBUTING.md |
| Curated `[lints]` configuration (pedantic + nursery with allow-list) | 10 | CHANGELOG |
| `SearchMatch.candidates: [Scalar; 2]` (breaking) + `candidates_hex()` accessor | 12 | CHANGELOG + Migration table |
| `copy_from_slice` in cached sweep (drop `try_into + expect`) | 13 | code inline |
| ADR-0009 + opt-decision 0007 + CHANGELOG rollup + docs refresh | 14 | `docs/` tree |
| Local pre-commit gate (fmt + clippy -D warnings + test + doc + miri) | 14 | CONTRIBUTING.md |
| **Full pre-commit-gate verified locally + benchmark regression gate met** | 15 | commit message records cycle counts |
| MSRV 1.70 → 1.81 for stable `core::error::Error` | 16 | CHANGELOG + Migration table |

These items are "Recently delivered" rather than "Roadmap" because they are already on `master`. See the [CHANGELOG.md](../CHANGELOG.md) and `git log --oneline` for the per-commit breakdown.

## Future work

Items still under consideration, in rough order of likely value:

### Near term
- **Improved progress visualization and ETA estimation.** Currently the orchestrator logs progress per chunk; a TUI would make long-running searches easier to monitor.
- **Comprehensive benchmarking suite with historical tracking.** Integrate `criterion`'s `benches.csv` output with a trend dashboard. The 5% regression gate from commit 15 is in place; historical data is the next step.
- **Pluggable variant generation.** Allow users to define custom variant sets (e.g. focused on a specific range).

### Medium term
- **Additional curve support.** The algorithm generalizes naturally to any short-Weierstrass curve. Initial candidates: `secp256r1` (P-256), `secp384r1` (P-384), `secp224r1` (P-224). The RustCrypto `elliptic-curves` workspace provides crates for each.
- **REST API for remote search management.** A small HTTP layer over the orchestrator would enable multi-machine coordination.
- **Distributed search coordination.** Shared checkpoints and cache files across machines, with a coordinator process that partitions the range and aggregates results.

### Long term
- **GPU acceleration.** A CUDA or OpenCL backend for the sweep. The variant-index and batch-normalization strategy is well-suited to GPU architectures. A proof-of-concept is the first step; a production-ready implementation is significantly more work.
- **WebAssembly compilation.** A `wasm32-unknown-unknown` build would enable browser-based demonstrations of the algorithm.
- **Formal verification.** Mechanized proof of the matching invariant and the batch normalization correctness, ideally in a proof assistant such as Coq or Lean.

## Non-Goals

The following are explicitly **out of scope** and will not be pursued:

- **Production key recovery tooling.** The project is for education and research. Building a tool optimized for "real" key recovery would conflict with the [disclaimer](../DISCLAIMER.md).
- **Wallet integration or address generation.** This is a search engine, not a wallet.
- **Altcoin-specific features.** The tool is curve-general (in principle) but is bound to secp256k1 in the current implementation. Adding per-coin logic is out of scope.
- **Mobile platforms (iOS/Android).** The search workload is CPU- and disk-intensive; mobile is not a practical target. The library could in principle be compiled for these targets, but no official build matrix is planned.

## Versioning Policy

The project follows [Semantic Versioning](https://semver.org/):

- **MAJOR** — incompatible API or behavior change (not expected; the review-driven breaking changes are SemVer-minor because the crate is pre-1.0).
- **MINOR** — backwards-compatible feature addition. The **review-driven pass that closed in commits 1–18** is one such MINOR cycle even though several breaking changes shipped (pre-1.0 SemVer permits this; see the Migration table in [README.md](../README.md#migration-016--020)).
- **PATCH** — backwards-compatible bug fix.

The next release is **0.2.0** (a MINOR bump from the current 0.1.6). See [maintenance/release.md](maintenance/release.md) for the release process and [CHANGELOG.md](../CHANGELOG.md) for the projected `v0.2.0` entry.

## Supported Versions

| Version | Supported |
|---|---|
| 0.2.x | Yes — current stable line (post-review-driven) |
| 0.1.x | Yes — during the 0.2.0 transition window; will move to "deprecated" once the 0.2 series stabilises |
| 0.0.x | No — pre-stable; not recommended for any use |

## Deprecation Policy

When a feature is deprecated:

1. The deprecation is announced in [CHANGELOG.md](../CHANGELOG.md) under `### Deprecated`.
2. The deprecation note in the source code includes the version that will remove the feature and the recommended replacement.
3. Deprecated features remain functional for at least one minor release cycle before removal. The review-driven pass already followed this policy for `Config::with_batch_size` / `with_variant_count` (deprecated in commit 7a; the originals remain in the source tree as `#[deprecated]` shims).

## Contributing Ideas

If you would like to suggest a roadmap item, open a [feature request](../.github/ISSUE_TEMPLATE/feature_request.md) on GitHub. See [CONTRIBUTING.md](../CONTRIBUTING.md) for the contribution workflow.
