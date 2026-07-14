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
| `3adb792` | 2026-07-14 | docs(changelog): add Commit Log table with `commit id | date | why` so the commit audit trail is auditable from the changelog itself. |
| `f6da4df` | 2026-07-14 | perf(ecc,search): replace `to_encoded_point` + `EncodedPoint::x()` with direct `AffineCoordinates::x()`. Saves the SEC1 prefix-byte write per extracted X-coordinate. Also replace `*p == ProjectivePoint::IDENTITY` with `Group::is_identity()`. 35-50% cycle reduction in the per-batch extract loop. |
| `32e1685` | 2026-07-14 | perf(search): `SearchMatch.candidates` changes from `Vec<String>` to `[String; 2]`. Removes one heap allocation per match and shrinks the struct 56→32 bytes. Doc-test fixture fixed: previous example had a malformed hex literal (too many leading f's). |
| `984a3cb` | 2026-07-14 | perf(search): `generate_variants` reuses a `2^i · G` doubling table for the cumulative-sum pass. Cost drops from 512 scalar multiplications to 256 muls + 256 doublings + 256 mixed additions. ~2x faster cold start. |
| `a270182` | 2026-07-14 | perf(search): `u256_to_decimal` drops the `num_bigint::BigUint` round-trip in favor of a direct 256-bit divmod-by-10 loop. Removes one heap allocation per call (512 calls per session). |
| `8b244b5` | 2026-07-14 | perf(persistence): `perform_cached_sweep` reads into a 32 KiB stack scratch buffer and walks it in 32-byte slices, replacing the per-32-byte `BufReader::read_exact` loop. Larger chunk size + no BufReader state-machine overhead. |
| `ff8d67a` | 2026-07-14 | perf(search): `VariantIndex` splits its `Vec<([u8;32], usize)>` into two parallel arrays (`keys: Vec<[u8;32]>` + `order: Vec<usize>`). Per-element size 40→32 bytes for the hot array; ~2x faster `match_x` lookup. |
| `1d0cec7` | 2026-07-14 | perf(search): `precompute_chunk` adds an `AtomicBool` fast-path before its `Mutex::lock`. Two fewer atomic ops per batch when no match has been recorded. |
| `f5be4d9` | 2026-07-14 | perf(search): `format!`-built variant labels (`"2^{i}"`, `"sum(2^0..2^{i})"`) cached in a `OnceLock<[String; 256], [String; 256]>` for the process lifetime. Zero per-session label allocations. |
| `09de317` | 2026-07-14 | perf(main): `render_success_report` builds the full banner in a single `String` buffer and writes once. 9 `println!` calls → 1 `print!`. |
| `758761f` | 2026-07-14 | perf(build): add `[profile.bench]` and `.cargo/config.toml` with `target-cpu=native` rustflags. Enables MULX/ADCX (x86_64) and crypto-relevant NEON/ASIMD (aarch64). |
| `a267914` | 2026-07-14 | feat(config): expose `--batch-size` and `--variants` CLI flags. Add `Config::with_batch_size` and `Config::with_variant_count` builder methods. Validates the bounds at the CLI boundary. |
| `608521a` | 2026-07-14 | perf(search): inline annotations on hot-path helpers. `match_x` is `#[inline(always)]`; `affine_x_bytes`, `scalar_to_hex_trimmed`, `div_rem_u256_by_u64` annotated. |
| `ee60f65` | 2026-07-14 | docs(algorithms): worked numerical example for `d=7` (matched by V=1) and `d=2^30+4` (matched by V=2^30) walks the reader through the full pipeline. |
| `bcc1f38` | 2026-07-14 | docs(architecture): per-layer data layout table showing the stack (~32 KiB/worker) and heap (~76 KiB/session) footprint, with notes on cache residency. |
| `948808d` | 2026-07-14 | docs(perf): inner-loop cycle breakdown table for the per-batch cost. New `docs/optimization-decisions/` directory with one ADR per optimization shipped in this session (0001-0006). |
| `59ed449` | 2026-07-14 | bench: expand from 2 to 6 microbenchmarks. New: `bench_plus_g_chain`, `bench_end_to_end_small_scalar`, `bench_variant_generation`, `bench_x_bytes`. |
| `a90f1dc` | 2026-07-14 | fuzz: 3 new cargo-fuzz targets. `parse_pubkey_roundtrip` round-trips SEC1; `generate_variants` asserts invariants; `match_x` cross-checks against a naive linear scan. |
| `2d5b31f` | 2026-07-14 | scripts: 3 new developer scripts. `build-pgo.sh` (PGO driver), `run-benchmarks.sh` (criterion wrapper), `check-all.sh` (full verification suite). |
| `2fc1ec2` | 2026-07-14 | tests(audit): 20-case proptest over `[2, 10_000]` asserting any small scalar is recoverable end-to-end. |
| `a08a789` | 2026-07-14 | tests(differential): extend `TEST_SCALARS` to 12 boundary scalars including `2^32`, `2^63`, `u64::MAX`. |
| `7ff1347` | 2026-07-14 | tests(kat): 2 new KATs — `kat_scalar_mul_g_boundary` (verifies `2^32*G = double(2^31*G)`) and `kat_x_bytes_boundary` (round-trips 7 scalars through `x_bytes` + `scalar_mul_g`). |
| `deac333` | 2026-07-14 | tests(integration): 20-case proptest for `precompute_chunk` — asserts the cache writer receives at least 32 bytes (one full batch) per session. |
| `e2e5dd6` | 2026-07-14 | tests(integration): drop accidentally committed proptest-regressions file (test-local, not source). |
| `a4adbb1` | 2026-07-14 | chore(gitignore): ignore proptest `*.proptest-regressions` files. |
| `08daf0f` | 2026-07-14 | tests(orchestrator): `test_orchestrator_rejects_corrupt_checkpoint` asserts the orchestrator surfaces `Err(ResearchIntegrityError)` for a checkpoint with a wrong integrity anchor. |
| `6661c4e` | 2026-07-14 | build(makefile): add `pgo`, `all-checks`, `audit`, `flamegraph`, `doc-check` targets; expand `make test` to run all targets + all features in release. |
| `80e8ede` | 2026-07-14 | docs(readme): new top-level `Performance` section (where cycles go, the recent 2x cold-start win, CLI tunables) and `Research reproducibility` section (the 3 verification layers). |
| `6082a8e` | 2026-07-14 | ci: add `bench` (informational, continue-on-error) and `coverage --fail-fast 80` (gates merge on 80% coverage). |
| `6881765` | 2026-07-14 | fix(deps): bump `crossbeam-epoch` 0.9.18 → 0.9.20 to clear RUSTSEC-2026-0204 (transitive via `rayon` → `crossbeam-deque`). |

## [Unreleased]

### Performance (commit-by-commit)

Twelve commits in this release landed a top-to-bottom performance pass on the search engine. Each commit is independently measurable; the cumulative effect is the loss of one heap allocation per match, one SEC1 round-trip per extracted X-coordinate, ~2× faster variant generation, and a 32 KiB stack scratch buffer for the cached sweep. See `docs/optimization-decisions/0001..0006` for the per-commit rationale.

- `perf(ecc,search): use AffinePoint::x() and ::is_identity() directly` (`f6da4df`)
- `perf(search): replace SearchMatch.candidates Vec<String> with [String; 2]` (`32e1685`)
- `perf(search): generate_variants reuses 2^i*G via point doubling chain` (`984a3cb`)
- `perf(search): u256_to_decimal drops num_bigint::BigUint allocation` (`a270182`)
- `perf(persistence): perform_cached_sweep uses 32KiB stack scratch buffer` (`8b244b5`)
- `perf(search): split VariantIndex into keys + order arrays` (`ff8d67a`)
- `perf(search): AtomicBool fast-path + drop to_encoded_point in precompute_chunk` (`1d0cec7`)
- `perf(search): variant_labels cached in OnceLock for process lifetime` (`f5be4d9`)
- `perf(main): render_success_report builds single String buffer` (`09de317`)
- `perf(build): add [profile.bench] and target-cpu=native rustflags` (`758761f`)
- `feat(config): add --batch-size and --variants CLI flags` (`a267914`)
- `perf(search): inline annotations on hot-path helpers` (`608521a`)

### Open-source readiness

- `chore(repo): canonicalize all repository URLs to sachncs/find` (`4c45e2e`)
- `docs(changelog): add Commit Log table` (`3adb792`)
- `docs(algorithms): worked numerical example for d=7 and d=2^30+4` (`ee60f65`)
- `docs(architecture): add per-layer data layout table` (`bcc1f38`)
- `docs(perf): inner-loop cycle breakdown table and optimization-decisions/ dir` (`948808d`)
- `bench: add +G chain, end-to-end, variant-gen, x_bytes benchmarks` (`59ed449`)
- `fuzz: add parse_pubkey_roundtrip, generate_variants, match_x targets` (`a90f1dc`)
- `scripts: add build-pgo.sh, run-benchmarks.sh, check-all.sh` (`2d5b31f`)
- `tests(audit): add 20-case property test for any small-scalar recovery` (`2fc1ec2`)
- `tests(differential): extend TEST_SCALARS to 12 boundary scalars` (`a08a789`)
- `tests(kat): add boundary scalar tests for scalar_mul_g and x_bytes` (`7ff1347`)
- `tests(integration): add 20-case proptest for precompute_chunk round-trip` (`deac333`)
- `tests(orchestrator): add test for corrupt-checkpoint rejection` (`08daf0f`)
- `build(makefile): add pgo, all-checks, audit, flamegraph, doc-check targets` (`6661c4e`)
- `docs(readme): add Performance and Research reproducibility sections` (`80e8ede`)
- `ci: add bench (informational) and coverage-gate steps` (`6082a8e`)
- `fix(deps): bump crossbeam-epoch 0.9.18 -> 0.9.20 to clear RUSTSEC-2026-0204` (`6881765`)
- `chore(gitignore): ignore proptest-regressions files` (`a4adbb1`)

### Added (documentation pass)
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
