# Review-Driven Changes — `find` Repository

This document is the execution plan produced from the elite Rust
review. Each change is one atomic commit; commits are ordered so
the codebase compiles + tests pass at every step.

**Status legend:** `[ ]` todo · `[~]` in progress · `[x]` done

---

## Baseline (commit 0)

- [ ] **Record current performance baseline.**

  ```bash
  cargo bench --bench bench -- --save-baseline current
  ```

  Captures cycle counts for the 6 criterion benchmarks; the
  `current` baseline is later compared against `post-review`
  in commit 14 with `cargo bench --bench bench -- --baseline
  current -- --threshold 5`.

---

## Critical fixes

### C1 · `u256_to_decimal` safety annotation (commit 1)

**File:** `src/search.rs`, function `u256_to_decimal` (around line 1058)

- [ ] Remove the `unsafe { String::from_utf8_unchecked(digits) }` block.
- [ ] Replace with `String::from_utf8(digits).expect("digits are 0..=9 ASCII")` (preferred) **or** keep `unsafe` and add a `// SAFETY:` block above it.
- [ ] Verify `cargo test --workspace --all-features` still passes.
- [ ] Verify `cargo clippy --all-targets --all-features -- -D warnings` clean.
- [ ] Commit: `perf(search): remove unsafe from u256_to_decimal`

### C2 · Thread `--batch-size` / `--variants` into hot path (commits 7a–7c)

The largest change. Breaking change to public API. The runtime
tunables are honored and the dead `SweepRange` is removed in the
same release.

See commits 7a, 7b, 7c, 8 below.

### C3 · `Config::validate` deep validation (commit 3)

**File:** `src/config.rs`

- [ ] Add `Config::validate_pubkey` method that calls
      `ecc::parse_pubkey(&self.pubkey)` and returns
      `Result<(), FindError>`.
- [ ] Keep `Config::validate` as the shallow check (empty /
      whitespace-only).
- [ ] Add `FindError::InvalidConfig(String)` variant in
      `src/error.rs`.
- [ ] Update `orchestrator::run` to call both
      `config.validate()?` and `config.validate_pubkey()?` at
      startup.
- [ ] Add unit test `test_config_validate_pubkey_accepts_valid`
      and `test_config_validate_pubkey_rejects_invalid`.
- [ ] Commit: `feat(config): add Config::validate_pubkey for
      fail-fast on malformed pubkeys`

### C4 · `to_hex_x` ↔ `x_bytes` round-trip test (commit 4)

**File:** `tests/kat.rs` (new test) **or** `src/ecc.rs` `#[cfg(test)]`

- [ ] Add test `kat_to_hex_x_matches_x_bytes_hex` that asserts
      `ecc::to_hex_x(&p) == hex::encode(ecc::x_bytes(&p).unwrap())`
      for `d in 1..=1000`.
- [ ] Add property test `prop_to_hex_x_equals_x_bytes_hex` for
      100 random scalars in `[1, 1_000_000]`.
- [ ] Commit: `tests(kat): add to_hex_x ↔ x_bytes round-trip regression
      test`

### H2 · `to_hex_x` fast path (commit 5)

**File:** `src/ecc.rs`, function `to_hex_x` (around line 232)

- [ ] Replace `affine.to_encoded_point(false)` + `encoded.x()` with
      the direct `AffineCoordinates::x()` pattern already used in
      `x_bytes` and `affine_x_bytes`.
- [ ] The function becomes:

  ```rust
  pub fn to_hex_x(p: &ProjectivePoint) -> String {
      use k256::elliptic_curve::group::Group;
      use k256::elliptic_curve::point::AffineCoordinates;
      if bool::from(p.is_identity()) {
          return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
      }
      let affine = p.to_affine();
      hex::encode(affine.x())
  }
  ```

- [ ] Add `use k256::elliptic_curve::Group;` to the import group.
- [ ] Commit: `perf(ecc): to_hex_x uses AffineCoordinates::x() directly
  (mirrors the x_bytes change)`.

### H1 · Tighten `libc::fsync` `// SAFETY:` placement (commit 2)

**File:** `src/persistence.rs`, function `Checkpoint::save_atomic`
(around line 217)

- [ ] Move the existing `// SAFETY:` text so it sits directly
      above the `let _ = unsafe { libc::fsync(...) }` call (not
      above the `if let Some(parent) = ...` block).
- [ ] Make the comment self-contained: it should explain (a) why
      `fsync` is sound on the borrowed `RawFd`, (b) why discarding
      the `Result` is acceptable, and (c) why this is
      best-effort.
- [ ] Commit: `docs(persistence): tighten libc::fsync SAFETY comment`

### C2 part 1 — `BatchSize` newtype + `Config` API (commit 7a)

**File:** `src/config.rs`, `src/error.rs`, `src/main.rs`

- [ ] Add `FindError::InvalidConfig(String)` variant to
      `src/error.rs` (alphabetical position between `EccError`
      and `InvalidPublicKey`).
- [ ] Add `pub struct BatchSize(u32)` newtype to `src/config.rs`
      with:
      - `pub const MIN: u32 = 1;`
      - `pub const MAX: u32 = MAX_BATCH_SIZE;`
      - `pub const DEFAULT: BatchSize = BatchSize(DEFAULT_BATCH_SIZE);`
      - `pub fn new(size: u32) -> Result<Self, FindError>`
        (returns `InvalidConfig` on out-of-range)
      - `pub fn get(self) -> u32` (returns the inner value)
- [ ] Add `Config::try_with_batch_size(BatchSize) -> Result<Self, _>`.
- [ ] Add `Config::try_with_variant_count(u32) -> Result<Self, _>`.
- [ ] Keep the existing `with_batch_size` / `with_variant_count`
      panicking variants for backward compat; mark them
      `#[deprecated(note = "use try_with_batch_size for fallible
      construction")]`.
- [ ] Change `Config::batch_size` field type from `u32` to
      `BatchSize`. Update all internal users.
- [ ] Update `src/main.rs` to call `try_with_batch_size` /
      `try_with_variant_count` and propagate the `InvalidConfig`
      error as a non-zero exit code (1).
- [ ] Update tests in `src/config.rs` to use the new constructors.
- [ ] Commit: `feat(config): introduce BatchSize newtype + try_with_*
  builders; deprecate panicking with_*`

### C2 part 2 — Heap-allocate hot-path batch arrays (commit 7b)

**File:** `src/search.rs`, `src/orchestrator.rs`

- [ ] Remove `pub const MAX_BATCH: usize = 32;` from the public
      surface of `search.rs` (keep an internal `const
      MAX_BATCH_STACK: usize = 32;` for `Box<[T; N]>` sizing).
- [ ] In `perform_chunked_sweep`, change
      `let mut points = [ProjectivePoint::IDENTITY; MAX_BATCH];` to
      `let mut points: Vec<ProjectivePoint> = vec![ProjectivePoint::IDENTITY; batch_size as usize];`
      and same for `affines`.
- [ ] Add `batch_size: u32` parameter to `perform_chunked_sweep` and
      `precompute_chunk`.
- [ ] Update `orchestrator::run` to pass `config.batch_size.get()`
      to both functions.
- [ ] Update `X * 32` for the block buffer to `batch_size as usize *
      32` (or allocate `Vec<u8>`).
- [ ] Update the block offset calculation
      `batch_idx * BATCH_SIZE * 32` to use the configured batch
      size.
- [ ] Add a property test `prop_batch_size_runtime` in
      `tests/integration.rs` that runs `perform_chunked_sweep`
      with a few different batch sizes and asserts the same
      behaviour.
- [ ] Commit: `perf(search): runtime-sized batch arrays; honour
  config.batch_size`

### C2 part 3 — Intern `OffsetVariant` strings (commit 7c)

**File:** `src/search.rs`, `src/orchestrator.rs`

- [ ] Change `generate_variants` return type from
      `Vec<OffsetVariant>` to `&'static [OffsetVariant]` (where
      each `OffsetVariant` is fully pre-built and interned).
- [ ] Intern the labels in `OnceLock<Box<[String; 512]>>` (or use
      the existing `variant_labels` pattern extended to 512
      entries covering both `pow` and `sum` families).
- [ ] Intern the decimal offset strings similarly (they depend
      only on the index, not the target).
- [ ] Update `orchestrator::run` to consume the `&'static` slice
      instead of `Vec<OffsetVariant>`.
- [ ] Update the `prop_generate_variants_count` proptest in
      `src/search.rs::tests` for the new return type.
- [ ] Commit: `perf(search): generate_variants returns &'static
  [OffsetVariant]; remove per-session String allocations`

### Decision 3 — Remove `SweepRange` (commit 8)

**Files:** `src/config.rs`, `src/orchestrator.rs`, `src/lib.rs`

- [ ] Delete `pub struct SweepRange { pub start, pub end }` from
      `src/config.rs`.
- [ ] Delete `impl SweepRange { fn new, fn len, fn is_empty }` from
      `src/config.rs`.
- [ ] Delete the `mod tests { fn test_sweep_range_* }` block in
      `src/config.rs`.
- [ ] Delete `pub use crate::config::SweepRange;` from
      `src/orchestrator.rs`.
- [ ] Delete the `SweepRange` references in `src/lib.rs`
      module-level docs (search for `[`SweepRange`]`).
- [ ] Search `tests/`, `benches/`, `fuzz/`, `docs/` for
      `SweepRange` references and update or delete them.
- [ ] Commit: `refactor(config): remove unused SweepRange newtype`

### Decision 4 — `OnceLock<SearchMatch>` in `precompute_chunk` (commit 6)

**File:** `src/search.rs`, function `precompute_chunk` (around line 896)

- [ ] Remove `match_found: Mutex<Option<SearchMatch>>` and
      `match_published: std::sync::atomic::AtomicBool`.
- [ ] Add `match_once: std::sync::OnceLock<SearchMatch>` at the
      top of the function.
- [ ] Replace the per-batch fast-path check
      `if match_published.load(Ordering::Relaxed) { return Ok(()); }`
      with `if match_once.get().is_some() { return Ok(()); }`.
- [ ] Replace the match-publishing block
      `if let Ok(mut guard) = match_found.lock() { *guard = Some(m);
      match_published.store(true, Ordering::Release); }` with
      `let _ = match_once.set(m);` (the `set` returns `Result<(), T>`
      and we ignore the "already set" case).
- [ ] Replace the final
      `let result = match match_found.into_inner() { Ok(r) => r, ... };`
      with `Ok(match_once.into_inner())`.
- [ ] Remove the `use std::sync::Mutex;` if no longer used.
- [ ] Verify all precompute_chunk tests pass.
- [ ] Update the rustdoc for `precompute_chunk` to reflect the
      new concurrency mechanism.
- [ ] Commit: `perf(search): OnceLock<SearchMatch> replaces
  Mutex+AtomicBool in precompute_chunk`

---

## High-impact improvements

### H3 · Lint configuration (commit 10)

**File:** `Cargo.toml`

- [ ] Add `[lints.rust]` section:
  - `unused_must_use = "warn"`
  - `redundant_closure_for_method_calls = "warn"`
- [ ] Add `[lints.clippy]` section with the curated `pedantic` and
      `nursery` sets, plus the project's justified `allow`s
      (`module_name_repetitions`, `must_use_candidate`,
      `missing_errors_doc`, `missing_panics_doc`, all
      `cast_* lints`, `similar_names`,
      `struct_excessive_bools`, `too_many_lines`,
      `module_inception`).
- [ ] Add `cargo clippy --all-targets --all-features -- -D warnings`
      to the local pre-commit gate (also document in
      `CONTRIBUTING.md`).
- [ ] Add `#[allow(...)]` annotations in the search and persistence
      modules for any pedantic false positives.
- [ ] Commit: `chore(lints): add [lints] section with curated pedantic
  + nursery sets`

### H4 · `cargo miri` in CI (commit 9)

**File:** `.github/workflows/ci.yml`

- [ ] Add a new `miri` job with `rust-toolchain: nightly` and
      `components: miri`.
- [ ] Steps: `cargo +nightly miri setup` then
      `cargo +nightly miri test --workspace --all-features`.
- [ ] Add the job to the required-for-merge check set
      (no `continue-on-error`).
- [ ] Document the miri run in `CONTRIBUTING.md` ("unsafe code
      changes must pass miri").
- [ ] Commit: `ci: add required-for-merge cargo miri job`

### H5 · `Config::try_with_batch_size` / `try_with_variant_count` (commit 11)

(Folded into commit 7a since they are inseparable. Tracked
separately for changelog clarity.)

- [ ] Confirm `try_with_*` exist in `src/config.rs` and are
      exercised by `src/main.rs` with proper `?` propagation.
- [ ] Add unit tests `test_config_try_with_batch_size_*` and
      `test_config_try_with_variant_count_*`.
- [ ] Commit: (folded into 7a) — no separate commit.

### A1-A3 · API improvements (commit 12)

**File:** `src/search.rs`, `src/ecc.rs`, `src/config.rs`

- [ ] Add `FindError::InvalidConfig(String)` variant (commit 7a).
- [ ] Add `Config::try_new(pubkey, output, cache) -> Result<Self, _>`
      that combines `new` + `validate_pubkey`.
- [ ] Replace `SearchMatch::candidates: [String; 2]` with
      `[Scalar; 2]` plus a `candidates_hex()` accessor. The
      `candidates_as_scalars` method becomes a `&self -> [Scalar;
      2]` view (no parsing needed).
- [ ] **BREAKING**: external code that does
      `match.candidates.contains(&"3".to_string())` must migrate
      to `match.candidates.contains(&Scalar::from(3u64))`.
- [ ] Update `src/orchestrator.rs`, `src/main.rs`, all tests, and
      all benches to use the new `Scalar` array.
- [ ] Commit: `feat(search): SearchMatch.candidates is [Scalar; 2]
  (breaking)`

### H6 · `try_into().expect()` in cached sweep (commit 13)

**File:** `src/persistence.rs`, function `perform_cached_sweep`
(around line 422)

- [ ] Replace
      `let chunk: [u8; 32] = buffer[buf_pos..buf_pos + 32].try_into().expect("buffer slice is exactly 32 bytes");`
      with
      `let mut chunk = [0u8; 32]; chunk.copy_from_slice(&buffer[buf_pos..buf_pos + 32]);`
- [ ] This removes the `try_into + expect` and the slice-bounds
      check; `copy_from_slice` panics with a clearer message if
      the buffer is exhausted mid-copy (which the surrounding
      `if buf_pos >= buf_len` check prevents anyway).
- [ ] Commit: `refactor(persistence): replace try_into+expect with
  copy_from_slice in cached sweep`

---

## Performance opportunities (lower priority)

### P1 · Cache the integrity-anchor `scalar_mul_g` in orchestrator

**File:** `src/orchestrator.rs`, function `run` (around line 230)

- [ ] After the first chunk, keep `last_anchor_p` (a
      `ProjectivePoint`) and update it per chunk via
      `chunk_size * G` point additions (a known constant).
- [ ] Replace the per-chunk
      `ecc::scalar_mul_g(&Scalar::from(current_j))` with the
      cached `last_anchor_p`.
- [ ] Profile to confirm the saving is non-negligible.
- [ ] Commit: `perf(orchestrator): cache integrity-anchor point;
  advance via point additions`

### P2 · `to_hex_x` reusable thread-local buffer

**File:** `src/ecc.rs` (optional; defer unless profiling shows
allocation)

- [ ] **Defer** until P1 is in and re-profiling shows
      `to_hex_x` allocation cost.
- [ ] Add `thread_local!` with a `RefCell<String>` for the hex
      buffer.
- [ ] Change `to_hex_x` to take a `&mut String` parameter (API
      change) **or** use the `tracing::Span::current`-style
      `with_thread_local` helper.

### P3 · `Cow<'static, str>` for `SearchMatch::label/offset`

- [ ] **Defer.** With commit 12, the API surface is already
      shrinking; this is a follow-up if string interning is
      needed.

### N2 · `OnceLock<SearchMatch>` was implemented in commit 6

(Marked complete by commit 6.)

---

## Documentation improvements (commit 14)

### D1 · `rustdoc::broken_intra_doc_links` deny

**File:** `src/lib.rs`

- [ ] Add `#![warn(rustdoc::broken_intra_doc_links)]` (warning,
      not deny, to keep the door open for nightly-only lints).
- [ ] Verify `cargo doc --no-deps --all-features` is clean under
      `RUSTDOCFLAGS="-D warnings"`.
- [ ] Commit: `chore(docs): warn on broken intra-doc links`

### D2 · New ADR-0009 + optimization-decision 0007

**Files:** `docs/adr/0009-runtime-batch-size.md` (new),
`docs/optimization-decisions/0007-oncelock-early-exit.md` (new)

- [ ] ADR-0009: explain the trade-off between compile-time
      constant batches and runtime-sized batches; justify the
      breaking change.
- [ ] Optimization-decision 0007: explain the `OnceLock` choice
      over `Mutex + AtomicBool` for `precompute_chunk`.
- [ ] Reference both from `CHANGELOG.md` `[Unreleased]`.
- [ ] Commit: `docs(adr,optimization-decisions): ADR-0009 runtime
  batch size; opt-decision 0007 OnceLock early-exit`

### D3 · `CHANGELOG.md` rollup

**File:** `CHANGELOG.md`

- [ ] Append a new `## Commit Log` entry block for commits
      1–14 (the entire review-driven pass).
- [ ] Add an `[Unreleased]` section that lists each
      review-driven change with its commit id and a one-line
      "why".
- [ ] Add a "Removed" section for `SweepRange`.
- [ ] Commit: `docs(changelog): review-driven pass rollup`

### D4 · `docs/modules.md`, `algorithms.md`, `architecture.md`,
`performance.md`

- [ ] Update `docs/architecture.md` data-layout table: hot-path
      batch arrays are now `Vec` / `Box<[T]>`, not `[T; MAX_BATCH]`.
- [ ] Update `docs/performance.md` inner-loop cycle breakdown:
      add the `OnceLock` win to the per-batch table.
- [ ] Update `docs/algorithms.md` if any of the changed APIs
      affect the worked examples.
- [ ] Update `docs/modules.md` to reflect the new public API
      surface (no `SweepRange`, `BatchSize` newtype, `OnceLock` in
      `precompute_chunk`).
- [ ] Commit: `docs(architecture,performance,modules,algorithms):
  reflect review-driven API changes`

### D5 · `CONTRIBUTING.md`

- [ ] Document the local pre-commit gate:
      `cargo fmt --check && cargo clippy --all-targets --all-features
      -- -D warnings && cargo test --all-targets --all-features`.
- [ ] Document the miri requirement for `unsafe` changes.
- [ ] Document the 5% perf regression gate.
- [ ] Commit: `docs(contributing): document local pre-commit gate +
  miri + perf-regression policy`

---

## Final verification (commit 15)

- [ ] **Full verification suite** — all must pass:

  ```bash
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-targets --all-features
  cargo test --doc
  RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
  cargo +nightly miri test --workspace --all-features
  cargo audit
  cargo deny check
  cargo tarpaulin --all-targets --all-features --out xml --timeout 600 --fail-fast 80
  cargo bench --bench bench -- --baseline current -- --threshold 5
  ```

- [ ] **5% perf regression gate**: the bench comparison must
      report `< 5%` regression for every benchmark. If a
      benchmark regresses, the commit must be revised or
      justified in the commit message.
- [ ] **miri clean**: `cargo +nightly miri test` must pass for
      `src/search.rs` and `src/persistence.rs`. If a module
      has unsafe code that miri rejects, the change must be
      reverted or the unsafe use removed.
- [ ] Commit: `chore(verify): pass full verification suite; record
  perf-regression status`

---

## Optional follow-ups (not blocking)

### A4 · MSRV bump to 1.81

**Files:** `Cargo.toml`, `CONTRIBUTING.md`, `CHANGELOG.md`

- [ ] Bump `rust-version` from `1.70` to `1.81` in `Cargo.toml`.
- [ ] Use `core::error::Error` in doctests and `Box<dyn
      core::error::Error + Send + Sync>` in `Result<()>` signatures.
- [ ] Document the bump in `CHANGELOG.md` (semver: minor for
      MSRV bumps that don't break the API).
- [ ] Commit: `chore(msrv): bump to 1.81 for core::error::Error`

### A5 · `core::error::Error` doctest signatures

- [ ] Sweep all doctests using `Box<dyn std::error::Error>` and
      replace with `Box<dyn core::error::Error>` (after MSRV bump).

### N3 · Hashable `SearchMatch` / `OffsetVariant`

- [ ] Defer. No current use case.

### A6 · `no_std` compatibility for `ecc` module

- [ ] Defer. Requires refactoring `k256` features and would
      require explicit `alloc` dependency.

---

## Verification — projected final scores

After all commits:

| Category | Projected |
|---|---|
| Correctness | 95 |
| Safety | 93 |
| Idiomatic Rust | 92 |
| API Design | 90 |
| Performance | 94 |
| Memory Efficiency | 89 |
| Documentation | 97 |
| Testing | 93 |
| Security | 88 |
| Maintainability | 94 |
| Cargo Configuration | 94 |
| Portability | 92 |

**Overall Production Readiness: 95/100.**
**Overall Idiomatic Rust: 92/100.**
**Overall Maintainability: 94/100.**

---

## Commits summary (in order)

| # | Type | Subject | Files |
|---|---|---|---|
| 0 | chore | Record current perf baseline | (none) |
| 1 | perf | Remove unsafe from `u256_to_decimal` | `src/search.rs` |
| 2 | docs | Tighten `libc::fsync` SAFETY comment | `src/persistence.rs` |
| 3 | feat | Add `Config::validate_pubkey` | `src/config.rs`, `src/error.rs`, `src/orchestrator.rs` |
| 4 | tests | Add `to_hex_x` ↔ `x_bytes` round-trip test | `tests/kat.rs` |
| 5 | perf | `to_hex_x` uses `AffineCoordinates::x()` directly | `src/ecc.rs` |
| 6 | perf | `OnceLock<SearchMatch>` in `precompute_chunk` | `src/search.rs` |
| 7a | feat | `BatchSize` newtype + `try_with_*` builders | `src/config.rs`, `src/error.rs`, `src/main.rs` |
| 7b | perf | Runtime-sized batch arrays; honour `config.batch_size` | `src/search.rs`, `src/orchestrator.rs` |
| 7c | perf | `generate_variants` returns `&'static [OffsetVariant]` | `src/search.rs` |
| 8 | refactor | Remove unused `SweepRange` newtype | `src/config.rs`, `src/orchestrator.rs`, `src/lib.rs` |
| 9 | ci | Add required-for-merge `cargo miri` job | `.github/workflows/ci.yml` |
| 10 | chore | Add `[lints]` section with curated pedantic | `Cargo.toml` |
| 11 | (folded into 7a) | (none) | (none) |
| 12 | feat | `SearchMatch.candidates` is `[Scalar; 2]` | `src/search.rs`, tests, benches |
| 13 | refactor | Replace `try_into+expect` with `copy_from_slice` | `src/persistence.rs` |
| 14 | docs | ADR-0009, opt-decision 0007, CHANGELOG rollup, doc updates | `docs/`, `CHANGELOG.md`, `CONTRIBUTING.md` |
| 15 | chore | Full verification suite passes | (none — verification only) |
| 16 (opt) | chore | Bump MSRV to 1.81 | `Cargo.toml`, `CONTRIBUTING.md`, `CHANGELOG.md` |
