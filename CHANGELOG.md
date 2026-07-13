# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
