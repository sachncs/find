# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Commit Log

Every commit on `master` is recorded here with `git commit id | date | why and what
was changed reasoning`. Generated from `git log --pretty=format:'%h | %ad | %s'
--date=short`. Rows below are in chronological order (oldest first).

| Commit | Date | Why & What |
|---|---|---|
| `e41a3df` | 2026-04-13 | Initial 0.0.1 release: SEC1 pubkey parsing, scalar arithmetic, basic sweep, JSON output. Establishes the project skeleton. |
| `15a159a` | 2026-04-15 | 0.0.2 release: improved ECC arithmetic, expanded error handling, parallel batch normalization, checkpoint support. |
| `fdedc51` | 2026-04-25 | 0.1.0 release: orchestrator module, persistence module, integration/audit tests, refactored ECC. |
| `19a70c7` | 2026-04-26 | 0.1.1 release: CI workflow on Ubuntu/macOS/Windows, PR template, CODEOWNERS, SECURITY.md. |
| `7061521` | 2026-04-26 | 0.1.2 release: minor search optimization fix. |
| `5bd58bd` | 2026-06-19 | ci(deps): bump actions/upload-artifact 4 → 7 — GitHub Action major version. |
| `57f6e08` | 2026-06-19 | ci(deps): bump softprops/action-gh-release 2 → 3. |
| `7b175bd` | 2026-06-19 | ci(deps): bump codecov/codecov-action 4 → 7. |
| `f2e47d5` | 2026-06-19 | ci(deps): bump actions/checkout 4 → 7. |
| `5832613` | 2026-06-19 | chore(deps): bulk Rust dep bump (9 updates). |
| `11d6e7d` | 2026-06-20 | 0.1.3 release: prep cut for the dep-bump. |
| `b2e7b89` | 2026-06-22 | ci(deps): bump actions/download-artifact 4 → 8. |
| `e38fe38` | 2026-06-24 | Merge PR #6: actions/download-artifact-8. |
| `c10bca1` | 2026-06-24 | Merge PR #5: rust-dependencies-9d23bf43ca. |
| `68c4ceb` | 2026-06-24 | Merge PR #4: codecov-action-7. |
| `40d9070` | 2026-06-24 | Merge PR #2: action-gh-release-3. |
| `e332ed5` | 2026-06-24 | Merge PR #1: actions/upload-artifact-7. |
| `944c73b` | 2026-06-24 | Merge PR #3: actions/checkout-7. |
| `e07ac6e` | 2026-06-25 | 0.1.4 release. |
| `cde2bfb` | 2026-06-26 | 0.1.5 release. |
| `1753ab2` | 2026-06-26 | 0.1.6 release: hardening pass — extracted `config` and `telemetry` modules, added `BATCH_SIZE`/`VARIANT_COUNT` constants, `#[non_exhaustive]` on `FindError`/`SearchMatch`, KAT/differential tests, fuzz targets, ADRs 0007/0008. |
| `f2748ad` | 2026-07-06 | chore(deps): bulk Rust dep bump (5 updates, including rand 0.9 → 0.10). |
| `255954f` | 2026-07-06 | Merge PR #8: rust-dependencies-bc89137e2b. |
| `dbb6ba8` | 2026-07-14 | docs(lib): expand crate-level docs with Mermaid dependency graph and Quick-Start doc-test. |
| `f60485d` | 2026-07-14 | docs(main): expand binary-level docs with lifecycle and example. |
| `303edca` | 2026-07-14 | docs(config): expand module docs and add usage examples. |
| `72c49b6` | 2026-07-14 | docs(ecc): expand module docs and add per-function examples + security notes. |
| `4d327dc` | 2026-07-14 | docs(error): add recovery-strategy table and extension policy. |
| `cfb02d5` | 2026-07-14 | docs(orchestrator): add Mermaid lifecycle diagram and strategy notes. |
| `c0b6699` | 2026-07-14 | docs(persistence): add Safety section, platform notes, examples. |
| `d5d7f4f` | 2026-07-14 | docs(search): expand module docs with concurrency model and examples. |
| `d730c26` | 2026-07-14 | docs(search): add pseudocode and performance notes to sweep functions. |
| `3ed077a` | 2026-07-14 | feat(search): promote MAX_BATCH to pub with documentation. |
| `03f0090` | 2026-07-14 | docs(telemetry): expand module docs with global-state and lifecycle notes. |
| `e24d4e4` | 2026-07-14 | docs(search,error): add Examples to constants and FindError. |
| `ed7386f` | 2026-07-14 | docs(search,persistence): document OffsetVariant and Checkpoint invariants. |
| `b725ed1` | 2026-07-14 | docs(config): add struct-level examples and invariants to SweepRange/Config. |
| `4626076` | 2026-07-14 | docs(search,persistence): document CacheWriter contract and FileCacheWriter thread-safety. |
| `0145bcb` | 2026-07-14 | docs(persistence): add Performance section to FileCacheWriter::write_block. |
| `e17c94c` | 2026-07-14 | docs: fix 4 broken intra-doc links surfaced by `RUSTDOCFLAGS=-D warnings cargo doc`. |
| `d45262c` | 2026-07-14 | docs(changelog): document the doc pass under Unreleased. |
| `9813fb5` | 2026-07-14 | fix(tests): switch `rand::Rng` to `rand::RngExt` for `random_range` after rand 0.10 broke the integration test; restored `cargo clippy --all-targets -- -D warnings`. |
| `4c45e2e` | 2026-07-14 | chore(repo): canonicalize all repository URLs to `sachncs/find` (was `sachncs/find` alias everywhere). 20 references across 14 files rewritten. |

## [Unreleased]

### Added
- Comprehensive Rustdoc documentation pass: Mermaid module-dependency graph in `lib.rs`,
  Mermaid session-lifecycle diagram in `orchestrator.rs`, struct-level Examples for
  `SweepRange`, `Config`, `OffsetVariant`, `Checkpoint`, and `FindError`; per-function
  Examples for `is_identity`, `x_bytes`, `to_hex_x`, `Config::new`, `Config::validate`,
  `SweepRange::new`, `Progress::new`/`add`/`get`, `VariantIndex::new`/`variants`,
  `SearchMatch::new`/`candidates_as_scalars`, `generate_variants`, `perform_chunked_sweep`,
  `init_tracing`, `install_rayon_panic_handler`, and the constants `BATCH_SIZE`,
  `VARIANT_COUNT`. Module-level `# Thread safety`, `# Concurrency`, `# Side-channel stance`,
  `# Recovery strategy`, `# Validation guarantees`, `# Coordinate representations`,
  `# Platform behaviour`, `# Global state`, and `# Lifecycle` sections added across
  all seven library modules. `# Safety` section on `Checkpoint::save_atomic` documents
  the single `unsafe { libc::fsync }` block. Pseudocode blocks added to `generate_variants`,
  `perform_chunked_sweep`, and `precompute_chunk`. The `CacheWriter` trait now has a
  `# Contract` section covering atomic block writes, concurrency, and offset independence.
- `find::search::MAX_BATCH` promoted from private to `pub` so downstream consumers and
  benchmark authors can reason about the per-batch stack budget (~3 KB on x86_64).
  Additive API surface change; existing callers unaffected.
- 17 new doc-tests (29 total, 1 `ignore`d) covering the documented public surface.
  All compile under `cargo test --doc`.

### Fixed
- `cargo doc --workspace --no-deps` now produces zero warnings (also with
  `RUSTDOCFLAGS=-D warnings`). Four broken intra-doc links were resolved:
  `search::precompute_chunk` → `crate::search::precompute_chunk`, bare `Mutex`
  → `std::sync::Mutex`, and two `ThreadPoolBuilder::build_global` references
  → `rayon::ThreadPoolBuilder::build_global`.
- Comprehensive open source documentation (CODE_OF_CONDUCT.md, CONTRIBUTING guidelines)
- GitHub issue templates for bug reports and feature requests
- Dependabot configuration for automated dependency updates
- EditorConfig for consistent code formatting across editors
- Gitattributes for line ending normalization
- GitHub Sponsors funding configuration
- Restructured documentation under `docs/` with single source of truth: overview, architecture, algorithms, modules, CLI, configuration, observability, performance, benchmarks, testing, deployment, operations, troubleshooting, security, FAQ, glossary, references, roadmap, maintenance, and ADR directory
- Six Architecture Decision Records (ADRs) capturing major design choices: multi-variant search, batch normalization, atomic checkpointing, error hierarchy, pure search module, binary cache format
- Hardening pass (2026-06-26):
  - **New modules**: `config` (session configuration), `telemetry` (tracing initialization)
  - **New public APIs**: `Config::new`, `SweepRange`, `SearchMatch::new`, `SearchMatch::candidates_as_scalars`, `ecc::is_identity`, `ecc::x_bytes`, `search::BATCH_SIZE`, `search::VARIANT_COUNT`
  - **`#[non_exhaustive]`** on `FindError` and `SearchMatch` for future-proof semver
  - **New tests**: KAT (`tests/kat.rs`), differential (`tests/differential.rs` against `libsecp256k1`), 7 new unit tests, 4 new property tests
  - **Fuzz targets**: `parse_pubkey`, `hex_to_scalar`, `scalar_mul_g` (3 targets via cargo-fuzz)
  - **New ADRs**: 0007 (Y-parity ambiguity), 0008 (mutex poisoning policy)
  - **`secp256k1-sys`** as a dev-dependency for cross-implementation verification

### Changed
- Enhanced README with improved structure, badges, and roadmap
- Expanded CONTRIBUTING.md with detailed development guidelines
- Improved .gitignore with comprehensive coverage for IDEs, OS files, and project artifacts
- Consolidated duplicated content: merged `ARCHITECTURE.md` and `docs/architecture.md` into a single `docs/architecture.md`; moved `ALGORITHMS.md`, `TESTING.md`, and `RELEASE.md` into `docs/`
- Replaced ASCII diagrams with Mermaid diagrams in architecture documentation
- Hardening pass (2026-06-26):
  - Extracted `Config` and related constants from `orchestrator.rs` to `config.rs`
  - Extracted `init_tracing` and `install_rayon_panic_handler` from `main.rs` to `telemetry.rs`
  - `progress.add(BATCH_SIZE)` → `progress.add(count as u64)` in `search.rs` to fix progress overshoot
  - `Mutex::lock().unwrap()` → `lock().expect("...")` in `persistence.rs` for clearer error messages
  - `init_tracing` now creates the log directory if it does not exist

### Fixed
- Repository metadata consistency
- `deny.toml`: corrected malformed cargo-deny URL
- `CONTRIBUTING.md`: fixed `PROPTEST_CODE` → `PROPTEST_CASES` typo
- `CODEOWNERS`: removed stale reference to nonexistent `OWNERS` file
- Documentation: clarified `TRILLION` (audit boundary) vs. `CACHE_CHUNK_SIZE` (cache chunk size) distinction
- Hardening pass (2026-06-26):
  - `Cargo.toml` `[package.bugs]` field removed (not recognized by cargo 1.95+)
  - `src/main.rs` `tracing_appender::fmt::layer()` typo corrected to `tracing_subscriber::fmt::layer()`
  - Documentation drift: `MAX_BATCH` visibility, `2^0` variant deduplication claim, `unsafe` count claim
  - Added `// SAFETY:` comment to the `libc::fsync` block in `persistence.rs`

## [1.0.0] - 2026-04-12

### Added
- **High-Performance Rust Core**: Replaced the Python prototype with a production-grade Rust implementation using `k256` and `rayon`.
- **512-Variant Search Engine**: Implemented range-splitting using powers of 2 ($2^0..2^{255}$) and cumulative summations.
- **Ambiguity Handling**: Added explicit candidate disambiguation to handle Y-parity during X-coordinate matching ($v \pm j$).
- **Structured Observability**: Added non-blocking rolling file logs using `tracing-appender` and daily logs in the `./logs` directory.
- **Export Capabilities**: Added JSON export for generated subtraction variants via the `--output-dir` flag.
- **Comprehensive Testing**: Added property-based tests (`proptest`), unit tests for edge cases, and robust integration tests.
- **Mathematical Documentation**: Added deep architectural and mathematical documentation across the codebase.

### Changed
- Refactored error handling to use `thiserror` for unified, contextual error reporting.
- Optimized critical point arithmetic paths to minimize allocations and redundant coordinate conversions.

### Fixed
- Fixed a panic condition in the variant generator when a subtraction resulted in the Identity point (point at infinity).
- Corrected out-of-range scalar scalar conversion logic for BigUint summations exceeding the curve order.

## [0.1.2] - 2026-04-26

### Fixed
- Minor search optimization fix

## [0.1.1] - 2026-04-26

### Added
- GitHub Actions CI workflow with multi-platform testing (Ubuntu, macOS, Windows)
- Pull request template with review checklist
- CODEOWNERS file for automatic reviewer assignment
- SECURITY.md with vulnerability reporting policy
- Extended error handling with domain-specific error types
- Orchestrator module for session management and resume
- Persistence module for atomic checkpoint operations
- Expanded test suite with orchestrator and audit tests

### Changed
- Refactored search engine with improved parallelism
- Enhanced documentation and testing strategy

## [0.1.0] - 2026-04-25

### Added
- Major refactoring of core search engine
- New orchestrator module for session management
- Persistence module for checkpoint handling
- Improved test coverage with integration and audit tests
- Enhanced error handling and reporting

### Changed
- Refactored ECC module for better code organization
- Updated dependencies and build configuration

## [0.0.2] - 2026-04-15

### Added
- Enhanced algorithm documentation
- Improved ECC point arithmetic
- Extended error handling
- Better CLI interface with checkpoint support
- Parallel search with batch normalization
- Comprehensive test suite

### Changed
- Refactored search engine for better performance
- Updated README with detailed architecture

## [0.0.1] - 2026-04-13

### Added
- Initial release of secp256k1 find tool
- Basic search functionality
- SEC1 public key parsing
- Parallel sweep engine
- CLI interface with basic options
