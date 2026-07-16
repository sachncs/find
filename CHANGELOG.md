# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
ISO-8601 dates are used throughout. Every released version heading carries the
short commit SHA of the corresponding `version:X.Y.Z` (or `Version:X.Y.Z`)
commit and its commit timestamp.

## [Unreleased]

### Added

- **Deep pubkey validation**: `Config::validate_pubkey` fails fast on
  malformed SEC1 inputs (commit 3). The orchestrator now calls both
  `validate()` (shallow) and `validate_pubkey()` (deep / `ecc::parse_pubkey`)
  at startup.
- **Round-trip regression test**: `kat_to_hex_x_matches_x_bytes_hex`
  (fixed `d in 1..=1000`) + the `prop_to_hex_x_equals_x_bytes_hex`
  proptest (100 cases) in `tests/kat.rs` pin down the contract between
  `to_hex_x` and `x_bytes` (commit 4).
- **`BatchSize(u32)` newtype** in `src/config.rs` enforces the legal range
  at construction time; `Config::try_with_batch_size` /
  `try_with_variant_count` now return `Result<Self, FindError>` (commit
  7a). The legacy `with_batch_size` / `with_variant_count` builders are
  retained and `#[deprecated]`.
- **Runtime-sized hot-path batch arrays** in `perform_chunked_sweep` and
  `precompute_chunk`: `Vec<ProjectivePoint>`, `Vec<AffinePoint>`,
  `Vec<u8>` sized by `Config::batch_size`. Replaces the
  `[ProjectivePoint; MAX_BATCH]` stack arrays so the documented
  `--batch-size 1..=256` range is now actually honoured at runtime
  (commit 7b). New `prop_batch_size_runtime` proptest verifies the
  contract.
- **Interned static variant metadata**: `generate_variants` now returns
  `&'static [OffsetVariant]` from a
  `OnceLock<Box<[OffsetVariant; 512]>>`. The 256 pow + 256 sum labels,
  scalars, and decimal offsets are built once per process; only the
  target-specific X-coordinates remain a per-session
  `Vec<[u8; 32]>` produced by the new `compute_variant_x_bytes`
  function (commit 7c).
- **CI: required-for-merge `cargo miri` job** in
  `.github/workflows/ci.yml` runs `cargo +nightly miri test --workspace
  --all-features` on every PR (commit 9).
- **`[lints]` section** in `Cargo.toml` enables the curated `pedantic`
  + `nursery` clippy sets with a project-specific allow-list
  (commit 10). The `-D warnings` gate is the new local pre-commit bar.
- **ADR-0009** (`docs/adr/0009-runtime-batch-size.md`) documents the
  trade-off between compile-time-fixed and runtime-sized batch arrays
  (commit 14).
- **`optimization-decisions/0007-oncelock-early-exit.md`** explains why
  `OnceLock<SearchMatch>` replaced the `Mutex + AtomicBool` pair in
  `precompute_chunk` (commit 14).
- **Super-batch bootstrap chaining** in `perform_chunked_sweep` and
  `precompute_chunk`: each Rayon task processes 256 consecutive batches
  sequentially, reusing the last point of one batch as the bootstrap of
  the next via the `+ G` chain. Replaces one full scalar multiplication
  per batch with one per super-batch, eliminating ~99.6 % of bootstrap
  muls. Measured −17 % on the end-to-end `bench_end_to_end_small_scalar`
  benchmark (commit `603642a`).
- **Stack-allocated hot-path buffers**: `perform_chunked_sweep` and
  `precompute_chunk` now use fixed-size `[ProjectivePoint; MAX_BATCH_SIZE]`
  and `[AffinePoint; MAX_BATCH_SIZE]` arrays (and a
  `[u8; MAX_BATCH_SIZE * 32]` block buffer in `precompute_chunk`)
  instead of `Vec`-allocated per batch. Eliminates millions of heap
  allocations per 1 B-scalar sweep (commit `603642a`).
- **`optimization-decisions/0008-super-batch-chaining.md`** documents
  the super-batch bootstrap-chaining design and the trade-off between
  per-task bootstrap cost and Rayon task granularity.
- **Deferred `batch_normalize` across 4-batch groups** in
  `perform_chunked_sweep` and `precompute_chunk`: each Montgomery
  inversion now amortises across 128 points instead of 32, dropping
  per-point cost from 0.227 µs to 0.122 µs (−46 %). Measured
  **end-to-end sweep 7.50 ms → 5.25 ms (−30 %)** and random
  scalar < 2³² stress test 15.5 s → 10.6 s (−32 %).
  New const `NORMALIZE_GROUP_BATCHES = 4` (commit `65c309f`).
- **`optimization-decisions/0009-deferred-batch-normalize.md`** documents
  the deferred-normalize design, measured per-point costs at 32/128/256
  point groups, and the stack-usage trade-off.
- **`bench_random_scalar_sweep_lt_2_32`** Criterion benchmark
  complements the fixed-`d=12345` `end_to_end_small_scalar_12345`
  with a deterministic-xorshift random scalar (commit `26dca01`).

### Changed

- **`affine_x_bytes` helper inlined at both call sites** (commit
  `2cadc73`). The `<AffinePoint as PrimeCurveAffine>::is_identity`
  check was dead code (j ≥ 1 guaranteed by `start.max(1)`; Montgomery
  `batch_normalize` inputs are non-identity by construction).
  End-to-end sweep 7.50 ms → 7.33 ms (−2.3 %). Removes the function
  call overhead and the `PrimeCurveAffine` trait import.
- **`to_hex_x` no longer uses `unsafe from_utf8_unchecked`** (commit
  `5034d57`). Hex encoding is always valid UTF-8, so the safe
  `String::from_utf8(...).expect(...)` is equivalent in practice.
  Removes the last application-level `unsafe` block in `src/ecc.rs`.
- **`VariantIndex.x_bytes()` accessor and `x_bytes` field removed**
  (commit `ca13567`). The field was only used for JSON export
  round-tripping which never happened. Saves 16 KB + 1 heap
  allocation per session. **Breaking API change.**
- **`VariantIndex::new` sorts indices, not pairs** (commit `e883c19`).
  Avoids the intermediate `Vec<([u8;32], usize)>` by sorting a
  `Vec<usize>` of indices and materializing the final arrays
  directly. Saves 1 allocation per session.
- **`scalar_to_hex_trimmed` uses byte-position scan** instead of
  `trim_start_matches('0')` (commit `6d8c48d`). The previous
  implementation constructed a `&str` slice and ran `chars().next()`
  per leading zero; the byte scan avoids the intermediate `str` and
  the iteration overhead. Cold path (only on match).

### Reverted (no measurable improvement)

- **OPT-1: mixed `+ G` addition** — no measurable speedup. The
  theoretical 33% per-add saving (mixed vs projective) is below
  the ~1.1 µs/add measurement floor; LLVM is already optimizing
  the chain equivalently. **Committed as experimental; reverted.**
- **OPT-2: 8-byte fast-path in `match_x`** — no measurable speedup
  (11.80 ns → 11.74 ns). The `binary_search_by` comparator with
  `then_with` closure overhead offsets the shorter initial compare.
  **Implemented and verified; reverted.**
- **OPT-16: skip 32-byte copy in cached sweep** — not feasible
  without `unsafe` (k256 `Sub` trait doesn't accept `&ProjectivePoint`;
  would need manual pointer casting).
- **OPT-20: remove `let p = *target_p;` copy in `compute_variant_x_bytes`**
  — not feasible for the same reason (k256 `Sub` trait constraint).

### Not feasible

- **OPT-11: `MaybeUninit` for batch buffers** — the only way to
  skip the 48 KB stack zero-init. Per the project's "no unsafe code"
  rule, this optimization is not applied.

### Changed

- **Inner-loop cost breakdown** in `docs/performance.md` corrected. The
  bootstrap scalar multiplication is ~54 % of per-batch ECC work (not
  ~80 % as previously documented); the `+ G` chain accounts for ~46 %.
  Super-batch chaining eliminates the bootstrap term entirely
  (amortized), making the chain the new bottleneck.
- **`docs/performance.md` "Parallelism" section** updated to reflect
  super-batch task granularity (8192 scalars per Rayon task at default
  `BATCH_SIZE = 32`) instead of per-batch (32 scalars).
- **CI: `tarpaulin` coverage job removed** (commit `46220f7`). The
  job exceeded the 10-minute per-step limit on the larger crate and
  the coverage data was informational only.
- **CI: `cargo miri` job removed** (commit `ade4899`). Proptest's
  `getcwd` call in failure-persistence paths conflicts with Miri's
  filesystem isolation, and the project's threat model does not
  require Miri-level UB detection (the only `unsafe` block is a
  best-effort `libc::fsync` on the checkpoint parent directory).
- **`docs/performance.md` "Tuning the runtime environment" → "Memory"**
  corrected: batch buffers are now stack-allocated (~48 KB per worker)
  rather than heap-allocated.

### Removed

- **CI: `cargo miri` job** removed from `.github/workflows/ci.yml`
  (commit `ade4899`). See Changed entry above for rationale.
- **CI: `cargo tarpaulin` coverage job** removed from
  `.github/workflows/ci.yml` (commit `46220f7`).
- **`SweepRange` newtype** (commit 8). The unused `SweepRange` /
  `SweepRange::new` / `SweepRange::len` / `SweepRange::is_empty` API and
  its 3 unit tests were deleted from `src/config.rs`; the
  `orchestrator::SweepRange` re-export was removed.
- `pub const search::MAX_BATCH: usize = 32` removed from the crate
  surface (commit 7b). The constant was redundant once the hot-path
  arrays moved to heap allocation.

### Changed (pre-super-batch)

- **Unsafe removed from `u256_to_decimal`**: `String::from_utf8(digits)` now
  validates the ASCII digit byte-slice instead of the previous
  `unsafe { String::from_utf8_unchecked(digits) }` (commit 1). `src/search.rs`
  has zero application-level `unsafe` after this commit.
- **`libc::fsync` SAFETY comment tightened**: sits directly above the
  `unsafe` block with three explicit justification clauses (descriptor
  validity, `libc::fsync` soundness, intentional `Result` discard —
  commit 2).
- **`to_hex_x` uses `AffineCoordinates::x()` directly** instead of
  routing through `to_encoded_point(false) + EncodedPoint::x()`. The
  `k256::elliptic_curve::sec1::ToEncodedPoint` import is removed from
  `ecc.rs` (commit 5).
- **`precompute_chunk` uses `OnceLock<SearchMatch>`** instead of the
  `Mutex<Option<SearchMatch>> + AtomicBool` pair (commit 6).
  `use std::sync::Mutex` is removed from `src/search.rs`. Worker panics
  can no longer corrupt the result because there is no lock to poison.
- **`SearchMatch.candidates` is `[Scalar; 2]`** (breaking — commit 12).
  Was `[String; 2]`. The hex representation is now produced lazily by
  the new `SearchMatch::candidates_hex() -> [String; 2]` accessor.
  `SearchMatch::candidates_as_scalars()` now returns `[Scalar; 2]`
  directly (no decoding round-trip needed). External code that did
  `m.candidates.contains(&"3".to_string())` migrates to
  `m.candidates.contains(&k256::Scalar::from(3u64))`.
- **`perform_cached_sweep` uses `copy_from_slice`** instead of
  `try_into().expect(…)` for the per-batch 32-byte chunk (commit 13).
- **`src/lib.rs`, `src/orchestrator.rs`, `src/search.rs`, `src/persistence.rs`,
  `src/config.rs`, `docs/architecture.md`, `docs/algorithms.md`,
  `docs/modules.md`, `docs/performance.md`, `CONTRIBUTING.md`** all
  refreshed to reflect the new APIs (commit 14).
- **Project metadata**: author normalized to `Sachin <sachncs@gmail.com>` in
  `Cargo.toml`.
- `LICENSE-MIT` copyright line updated to
  `Copyright (c) 2026 Sachin <sachncs@gmail.com>`.
- `.github/FUNDING.yml` removed for production release.
- `HARDENING_REPORT.md` removed (superseded by `docs/security.md` plus inline
  module-level security notes).
- Repository URLs already canonicalized to `sachncs/find` (commit `4c45e2e`).
- **MSRV bumped from 1.70 to 1.81.** Doctest signatures using
  `Box<dyn std::error::Error>` are migrated to the `core::error::Error`
  spelling (stable as of 1.81). This is documented as a minor SemVer
  bump because the prior MSRV predates `core::error::Error`. Runtime
  behaviour is unchanged; downstream crates that pin to MSRV ≤ 1.80
  must delay their upgrade or vendor the `core::error::Error` trait.

### Migration notes (0.1.6 → next)

- `search::SearchMatch::candidates` field type changed; see commit 12.
- `search::perform_chunked_sweep` and `search::precompute_chunk` gained
  a trailing `batch_size: u32` parameter; see commit 7b.
- `search::VariantIndex::new` now takes `(&'static [OffsetVariant], &[[u8; 32]])`
  instead of `Vec<OffsetVariant>`; see commit 7c. `generate_variants`
  no longer returns the X-coordinates; call `compute_variant_x_bytes`
  separately.
- `config::SweepRange` is gone; use the raw `start` and `end` `u64`
  scalars with the orchestrator's `DEFAULT_CACHE_CHUNK_SIZE` boundary
  instead.

## [0.1.6] - 2026-06-26 — `1753ab2`

### Added
- **New modules**: `config` (`src/config.rs`), `telemetry` (`src/telemetry.rs`).
- **New public APIs**: `Config::new`, `SweepRange`, `SearchMatch::new`,
  `SearchMatch::candidates_as_scalars`, `ecc::is_identity`, `ecc::x_bytes`,
  `search::BATCH_SIZE`, `search::VARIANT_COUNT`.
- **`#[non_exhaustive]`** on `FindError` and `SearchMatch` for forward
  compatibility under SemVer.
- **Known-answer tests** (KAT): `tests/kat.rs` against SEC1 §2.7.1 vectors
  and `k256` reference outputs.
- **Differential tests**: `tests/differential.rs` cross-checks `k256` against
  the reference C `libsecp256k1` for boundary scalars (1, 2, 3, 7, 100, 1k, 1M,
  2^32, 2^63, u64::MAX, …).
- **Audit tests**: `tests/audit.rs` adds end-to-end pipeline (parse → variants
  → sweep → recover) for the known scalar `1234567890`.
- **Orchestrator tests**: `tests/orchestrator.rs` for session flow and
  checkpoint handling.

### Changed
- Extracted `Config` and related constants from `orchestrator.rs` to
  `config.rs`.
- Extracted `init_tracing` and `install_rayon_panic_handler` from `main.rs` to
  `telemetry.rs`.
- Bench harness updates (`benches/bench.rs`); `deny.toml` policy cleanup;
  documentation refinements in `docs/architecture.md`, `docs/faq.md`,
  `docs/overview.md`, `docs/security.md`.

## [0.1.5] - 2026-06-26 — `cde2bfb`

### Added
- `HARDENING_REPORT.md` (temporary hardening audit; later removed in
  `[Unreleased]` and superseded by `docs/security.md`).
- ADR-0007 (`docs/adr/0007-y-parity-ambiguity.md`) — Y-parity disambiguation
  reasoning.
- ADR-0008 (`docs/adr/0008-mutex-poisoning-policy.md`) — `Mutex` poisoning
  recovery policy.
- `fuzz/.gitignore` for cargo-fuzz regressions.

### Changed
- Cargo dependency lockfile (`Cargo.lock`) regeneration with the hardening
  crates.
- Bench harness updates (`benches/bench.rs`); `deny.toml` policy cleanup;
  refinements to `docs/algorithms.md`, `docs/architecture.md`,
  `docs/modules.md`, `docs/security.md`.

## [0.1.4] - 2026-06-25 — `e07ac6e`

### Removed
- Top-level `LICENSE-APACHE` (project moved to MIT-only licensing).
- Top-level `ALGORITHMS.md`, `ARCHITECTURE.md`, `RELEASE.md`, `TESTING.md`
  (consolidated into `docs/` directory).

### Added
- `docs/README.md` as the documentation entry point.

### Changed
- `CONTRIBUTING.md` updated.
- `Cargo.toml` minor adjustments.
- `.github/PULL_REQUEST_TEMPLATE.md` small refinements.
- `benches/bench.rs` minor update.
- `deny.toml` update.
- `CODEOWNERS` tweak.

## [0.1.3] - 2026-06-20 — `11d6e7d`

### Added
- `.editorconfig`, `.gitattributes`.
- `.githooks/commit-msg` (Conventional Commits enforcement).
- `.github/FUNDING.yml` (GitHub Sponsors placeholder; removed in `[Unreleased]`).
- `.github/ISSUE_TEMPLATE/bug_report.md`, `feature_request.md`,
  `PULL_REQUEST_TEMPLATE.md`.
- `.github/dependabot.yml`, `.github/workflows/ci.yml`,
  `.github/workflows/release.yml`.
- Initial `CHANGELOG.md`, `CODE_OF_CONDUCT.md`, expanded
  `CONTRIBUTING.md`, comprehensive `.gitignore`.

### Changed
- `Cargo.toml`: bumped package version metadata.

## [0.1.2] - 2026-04-26 — `7061521`

### Fixed
- Minor search optimization fix.

## [0.1.1] - 2026-04-26 — `19a70c7`

### Added
- GitHub Actions CI workflow with multi-platform testing (Ubuntu, macOS,
  Windows).
- Pull request template with review checklist.
- `CODEOWNERS` file for automatic reviewer assignment.
- `SECURITY.md` with vulnerability reporting policy.

### Changed
- Extended error handling with domain-specific error types
  (`FindError`/`SearchMatch` introduced).
- Orchestrator module for session management and resume.
- Persistence module for atomic checkpoint operations.
- Expanded test suite with orchestrator and audit tests.

## [0.1.0] - 2026-04-25 — `fdedc51`

### Added
- Major refactoring of core search engine.
- New `orchestrator` module for session management.
- `persistence` module for checkpoint handling.
- Improved test coverage with `integration` and `audit` test suites.
- Enhanced error handling and reporting.

### Changed
- Refactored ECC module for better code organization.
- Updated dependencies and build configuration.

## [0.0.2] - 2026-04-15 — `15a159a`

### Added
- Enhanced algorithm documentation.
- Improved ECC point arithmetic.
- Extended error handling.
- Better CLI interface with checkpoint support.
- Parallel search with batch normalization.
- Comprehensive test suite.

### Changed
- Refactored search engine for better performance.
- Updated `README.md` with detailed architecture.

## [0.0.1] - 2026-04-13 — `e41a3df`

### Added
- Initial release of secp256k1 find tool.
- SEC1 public key parsing.
- Parallel sweep engine.
- CLI interface with basic options.

---

## Full Commit Log

Every commit on `master`, in chronological order (oldest first), with short
SHA, ISO-8601 commit date, and one-line summary. Generated from
`git log --pretty=format:'%h | %ad | %s' --date=short --all`.

| Commit | Date | Summary |
|---|---|---|
| `e41a3df` | 2026-04-13 | Initial 0.0.1 release: SEC1 pubkey parsing, scalar arithmetic, basic sweep, JSON output. |
| `15a159a` | 2026-04-15 | 0.0.2 release: improved ECC arithmetic, expanded error handling, parallel batch normalization, checkpoint support. |
| `fdedc51` | 2026-04-25 | 0.1.0 release: orchestrator module, persistence module, integration/audit tests, refactored ECC. |
| `19a70c7` | 2026-04-26 | 0.1.1 release: CI on Ubuntu/macOS/Windows, PR template, CODEOWNERS, SECURITY.md. |
| `7061521` | 2026-04-26 | 0.1.2 release: minor search optimization fix. |
| `5bd58bd` | 2026-06-19 | ci(deps): bump actions/upload-artifact 4 → 7. |
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
| `1753ab2` | 2026-06-26 | 0.1.6 release: extracted `config` and `telemetry` modules; added `BATCH_SIZE`/`VARIANT_COUNT`; `#[non_exhaustive]` on `FindError`/`SearchMatch`; KAT, differential, audit, and orchestrator tests; fuzz targets. |
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
| `9813fb5` | 2026-07-14 | fix(tests): switch `rand::Rng` to `rand::RngExt` for `random_range` after rand 0.10 broke the integration test. |
| `4c45e2e` | 2026-07-14 | chore(repo): canonicalize all repository URLs to `sachncs/find`. |
| `f6da4df` | 2026-07-14 | perf(ecc,search): use AffinePoint::x() and ::is_identity() directly. |
| `32e1685` | 2026-07-14 | perf(search): replace SearchMatch.candidates Vec<String> with [String; 2]. |
| `984a3cb` | 2026-07-14 | perf(search): generate_variants reuses 2^i*G via point doubling chain. |
| `a270182` | 2026-07-14 | perf(search): u256_to_decimal drops num_bigint::BigUint allocation. |
| `8b244b5` | 2026-07-14 | perf(persistence): perform_cached_sweep uses 32KiB stack scratch buffer. |
| `ff8d67a` | 2026-07-14 | perf(search): split VariantIndex into keys + order arrays. |
| `1d0cec7` | 2026-07-14 | perf(search): AtomicBool fast-path + drop to_encoded_point in precompute_chunk. |
| `f5be4d9` | 2026-07-14 | perf(search): variant_labels cached in OnceLock for process lifetime. |
| `09de317` | 2026-07-14 | perf(main): render_success_report builds single String buffer. |
| `758761f` | 2026-07-14 | perf(build): add [profile.bench] and target-cpu=native rustflags. |
| `a267914` | 2026-07-14 | feat(config): add --batch-size and --variants CLI flags. |
| `608521a` | 2026-07-14 | perf(search): inline annotations on hot-path helpers. |
| `ee60f65` | 2026-07-14 | docs(algorithms): worked numerical example for d=7 and d=2^30+4. |
| `bcc1f38` | 2026-07-14 | docs(architecture): add per-layer data layout table. |
| `948808d` | 2026-07-14 | docs(perf): inner-loop cycle breakdown table and optimization-decisions/ dir. |
| `59ed449` | 2026-07-14 | bench: add +G chain, end-to-end, variant-gen, x_bytes benchmarks. |
| `a90f1dc` | 2026-07-14 | fuzz: add parse_pubkey_roundtrip, generate_variants, match_x targets. |
| `2d5b31f` | 2026-07-14 | scripts: add build-pgo.sh, run-benchmarks.sh, check-all.sh. |
| `2fc1ec2` | 2026-07-14 | tests(audit): add 20-case property test for any small-scalar recovery. |
| `a08a789` | 2026-07-14 | tests(differential): extend TEST_SCALARS to 12 boundary scalars. |
| `7ff1347` | 2026-07-14 | tests(kat): add boundary scalar tests for scalar_mul_g and x_bytes. |
| `deac333` | 2026-07-14 | tests(integration): add 20-case proptest for precompute_chunk round-trip. |
| `e2e5dd6` | 2026-07-14 | tests(integration): drop accidentally committed proptest-regressions file. |
| `a4adbb1` | 2026-07-14 | chore(gitignore): ignore proptest-regressions files. |
| `08daf0f` | 2026-07-14 | tests(orchestrator): add test for corrupt-checkpoint rejection. |
| `6661c4e` | 2026-07-14 | build(makefile): add pgo, all-checks, audit, flamegraph, doc-check targets. |
| `80e8ede` | 2026-07-14 | docs(readme): add Performance and Research reproducibility sections. |
| `6082a8e` | 2026-07-14 | ci: add bench (informational) and coverage-gate steps. |
