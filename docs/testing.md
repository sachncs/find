# Testing

This document describes the testing philosophy, methodology, and infrastructure of the `find` tool. For the contribution workflow (including the test-related PR checklist), see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Philosophy: behavioral verification

The testing strategy prioritizes **functional correctness across state transitions** over superficial line coverage. A test is considered valuable if it exercises a real code path with a meaningful invariant, *not* if it merely increases the line-count number on a coverage report.

The strategy is organized into five layers, each with a specific purpose.

## Test categories

### 1. Mathematical invariant testing (property-based)

Using the [`proptest`](https://docs.rs/proptest) crate, the tool verifies that algebraic invariants hold over the 64-bit scalar field.

**Core invariant:** for any randomly generated scalar `d` and any variant shift `V`, the engine **must** be able to recover `d` if `j = |d - V|` is within the search range.

```rust
use proptest::prelude::*;
use find::ecc;
use find::search;

proptest! {
    #[test]
    fn prop_search_finds_any_scalar_in_range(j in 1u64..100_000u64) {
        // (Test scaffolding: build a target point for a known d = V + j)
        // ...

        // Post-review API (commits 7b + 7c + 12):
        let variants = search::generate_variants(&target_p);   // returns &'static [OffsetVariant]
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = search::VariantIndex::new(variants, &x_bytes);
        let result = search::perform_chunked_sweep(&index, j, j, 32);  // batch_size = 32
        prop_assert!(result.is_some());
        let m = result.unwrap();
        prop_assert!(m.candidates.contains(&expected_scalar));  // [Scalar; 2] post-12
        // ...
    }
}
```

Property-based tests in the repository:

- [`tests/integration.rs::prop_search_finds_any_scalar_in_range`](../tests/integration.rs)
- [`tests/integration.rs::prop_batch_size_runtime`](../tests/integration.rs) — exercises `perform_chunked_sweep` over the runtime `Config::batch_size` range (1..=256) and asserts the match is invariant under the batch choice (commit 7b)
- [`tests/integration.rs::prop_precompute_chunk_roundtrip`](../tests/integration.rs)
- [`tests/audit.rs::prop_audit_recovers_any_small_scalar`](../tests/audit.rs)
- [`src/ecc.rs::prop_sub_reversibility`](../src/ecc.rs)
- [`src/ecc.rs::prop_sub_curve_membership`](../src/ecc.rs)
- [`src/ecc.rs::prop_to_hex_x_idempotent`](../src/ecc.rs)
- [`src/search.rs::prop_generate_variants_count`](../src/search.rs) — pinned-length 512 across random targets (commit 7c)
- [`src/search.rs::prop_generate_variants_static_pointer`](../src/search.rs) — verifies the `OnceLock`-interned slice is the same pointer across calls (commit 7c)
- [`src/search.rs::prop_scalar_to_hex_trimmed_inverts`](../src/search.rs)
- [`tests/kat.rs::prop_to_hex_x_equals_x_bytes_hex`](../tests/kat.rs) — 100-case round-trip proptest pinning `to_hex_x` against `x_bytes` (commit 4)

### 2. Randomized discovery verification

A mandatory randomized test executes on every build to ensure the end-to-end pipeline functions correctly:

- **Input:** A seeded, deterministic 6–8 digit scalar (using `ChaCha8Rng`).
- **Process:** Derives a target point `P`, generates 512 variants, and runs a parallel sweep.
- **Goal:** Validates that the engine successfully extracts the correct candidate in a real-world execution flow.

```rust
#[test]
fn test_mandatory_random_6_to_8_digits() {
    use rand::RngExt;     // rand 0.10 extension trait
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    // `random_range` is the rand 0.10 spelling (was `gen_range` in rand 0.8).
    let j: u64 = rng.random_range(100_000..=99_999_999);
    // ...
}
```

The deterministic seed makes the test reproducible. The RNG range (6–8 digit scalars) is the intended operational range of the tool.

### 3. Edge case and boundary analysis

The repository explicitly tests known cryptographic and logic boundaries:

| Test | Scalar | Purpose |
|---|---|---|
| `test_boundary_min_6_digits` | `100_000` | Minimum 6-digit boundary |
| `test_boundary_max_8_digits` | `99_999_999` | Maximum 8-digit boundary |
| `test_edge_repeated_digits` | `111_111` | Repeated-digit pattern |
| `test_edge_alternating_pattern` | `121_212` | Alternating bit pattern |
| `test_edge_palindromic` | `123_321` | Palindromic pattern |
| `test_edge_single_digit` | `1` | Closest search boundary |
| `test_orchestrator_finds_small_scalar` | `5` | Orchestrator-level match |
| `test_orchestrator_finds_small_scalar_with_cache` | `5` | Cache path match |
| `test_orchestrator_resumes_from_checkpoint` | `5` | Checkpoint resume + match |

The collision handling test (`test_indexing_speedup`) ensures that the `VariantIndex` correctly handles mathematically identical shift amounts (e.g. `2^0 == sum(2^0..2^0)`) without logic panics.

### 4. System resilience and I/O

The system-resilience tests verify the persistence layer and the orchestrator's recovery behavior:

| Test | What it verifies |
|---|---|
| `test_checkpoint_roundtrip` | `save_atomic` + `load` are inverses |
| `test_checkpoint_verify_mismatch_pubkeys_is_ok` | A checkpoint for a different pubkey is silently accepted as "not for me" |
| `test_checkpoint_verify_valid` | A valid anchor passes verification |
| `test_checkpoint_verify_corrupted` | A tampered anchor raises `ResearchIntegrityError` |
| `test_cached_sweep_empty_file` | An empty cache returns `Ok(None)` without error |
| `test_cached_sweep_corrupted_size` | A cache whose size is not a multiple of 32 raises `CacheCorrupted` |
| `test_cached_sweep_write_and_read_back` | End-to-end cache write + read with a known match |
| `test_file_cache_writer_create` | `FileCacheWriter::create` makes parent directories |
| `test_file_cache_writer_write_and_read_back` | `FileCacheWriter` round-trip for a known block |
| `test_orchestrator_rejects_malformed_pubkey` | `run()` returns an error for invalid input |
| `test_config_validate_rejects_empty_pubkey` | `Config::validate` rejects whitespace-only pubkey |

The fail-fast parsing tests inject invalid SEC1 prefixes (e.g. `0x05`) and malformed hex strings to ensure no silent failures. The zero-copy integrity is checked via `cargo clippy` to ensure no redundant heap allocations are introduced into the search loop.

### 5. High-precision benchmarking (performance verification)

With the introduction of batch normalization, performance is now a verified pillar. The `criterion` suite validates:

- **Normalization amortization:** ensures simultaneous inversion remains efficient.
- **Index latency:** monitors binary search performance over the flat array.
- **Regression tracking:** validates that new commits do not degrade cryptographic throughput.

See [benchmarks.md](benchmarks.md) for details on the benchmark suite and how to run it.

## Writing tests

### Test layout

```
src/                 # Unit tests live alongside the code they exercise
├── ecc.rs           #     #[cfg(test)] mod tests { ... }
├── error.rs         #     #[cfg(test)] mod tests { ... }
├── search.rs        #     #[cfg(test)] mod tests { ... }
├── persistence.rs   #     #[cfg(test)] mod tests { ... }
├── orchestrator.rs  #     (no unit tests; covered by tests/orchestrator.rs)
└── main.rs          #     #[cfg(test)] mod tests { ... }

tests/               # Integration tests exercise the public API
├── audit.rs         #     End-to-end recovery verification
├── integration.rs   #     Randomized discovery, edge cases, property tests
└── orchestrator.rs  #     Full-session orchestrator behavior

benches/             # Criterion micro-benchmarks
└── bench.rs         #     batch_normalization, index_lookup
```

### Test framework conventions

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that <specific invariant>.
    #[test]
    fn test_<name>() {
        // Arrange
        let input = ...;

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }

    /// Verifies that <edge case> is handled correctly.
    #[test]
    fn test_<edge_case>() {
        // ...
    }
}
```

Conventions:

- All test functions are documented with `///` doc comments explaining the invariant.
- The `#[test]` attribute marks each test.
- The `Arrange / Act / Assert` structure is preferred for clarity.
- For test helpers, use `fn pad_hex(&str) -> String` patterns; helpers are usually module-private.

### Property-based test conventions

```rust
proptest! {
    /// Invariant: <statement of the property>.
    #[test]
    fn prop_<name>(
        // proptest strategies here
        x in 0u64..1000u64,
    ) {
        // Use prop_assert! rather than assert! for better failure messages.
        prop_assert!(condition(x));
    }
}
```

The `prop_*` prefix distinguishes property tests from example-based tests.

### Determinism

Property tests use deterministic seeds by default. To enable failure case recording, set `PROPTEST_CASES=1000` (or higher) and check the `proptest-regressions/` directory for any failed cases.

## Running the test suite

### Standard runs

```bash
# Full suite (release mode, optimized)
make test

# Direct cargo invocation (lib + integration + doc + benches)
cargo test --all-targets --all-features

# Doc tests separately (often faster feedback)
cargo test --doc

# Strict clippy (mirrors the CI gate)
cargo clippy --all-targets --all-features -- -D warnings

# Doc build must be warning-clean
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Miri on nightly (only required if a PR touches unsafe; ~10-30 min on a fresh sccache)
rustup component add --toolchain nightly miri
cargo +nightly miri setup
cargo +nightly miri test --workspace --all-features
```

### Increased property-test cases

```bash
# Default: 20 cases per proptest
# Increase for thorough pre-release runs
PROPTEST_CASES=1000 cargo test --release
```

### Specific tests

```bash
# Run a single test by name
cargo test --release test_orchestrator_finds_small_scalar

# Run all tests in a single file
cargo test --release --test integration

# Run all tests in a single module
cargo test --release --lib ecc
```

### With output

```bash
# Show stdout (e.g. println! from the audit test)
cargo test --release -- --nocapture
```

## Continuous integration

All changes are validated through GitHub Actions on **Ubuntu, macOS, and Windows** with the following checks (see `.github/workflows/ci.yml`):

| Job | What it checks |
|---|---|
| `fmt` | `cargo fmt --all -- --check` |
| `clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `test` | `cargo test --all-targets --all-features` (matrix: ubuntu/macos/windows) |
| `doc` | `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` |
| `miri` | `cargo +nightly miri test --workspace --all-features` (**required-for-merge** since commit 9; verifies the one reviewed `unsafe` block in `src/persistence.rs`) |
| `audit` | `cargo audit` for security advisories |
| `deny` | `cargo deny check all` for license/dependency auditing |
| `coverage` | `cargo tarpaulin` for code coverage reporting |

The `coverage` job uploads a `cobertura.xml` to Codecov. The `fail_ci_if_error: false` setting means coverage regressions do not block merges, but trends are tracked.

The `miri` job runs on `ubuntu-latest` with the nightly toolchain; it is required-for-merge (no `continue-on-error`). A local PR may opt to skip the miri run when its diff does not touch `unsafe`, but a passing nightly-miri run is still required by CI before merge. See [CONTRIBUTING.md#unsafe-code-changes-must-pass-miri](../CONTRIBUTING.md) for the developer policy.

## Code coverage

The test suite is expected to maintain high coverage on the cryptographic core. Recommended targets:

- **Critical paths** (ECC arithmetic, variant generation, variant index, batch normalization, atomic checkpointing): **100% coverage**.
- **Error handling paths**: 100% of the `FindError` variants must be exercised by at least one test.
- **Overall project**: aim for **>80%** line coverage on new code; track trends but do not block on regressions below 80%.

Coverage is reported via `cargo tarpaulin` and uploaded to Codecov:

```bash
make coverage
```

## Test categories summary

At the last full-suite run (`cargo test --all-targets --all-features`)
the project carries **112 tests across 7 binaries**:

- 71 unit tests in `src/`
- 13 KAT tests in `tests/kat.rs` (plus the 100-case `prop_to_hex_x_equals_x_bytes_hex` proptest)
- 4 tests in `tests/audit.rs` (plus the 20-case `prop_audit_recovers_any_small_scalar` proptest)
- 12 tests in `tests/integration.rs` (plus 3 proptests)
- 6 tests in `tests/orchestrator.rs`
- 3 tests in the binary test mod (`src/main.rs`)
- 3 tests in the differential suite (`tests/differential.rs`)

| Category | Purpose | Tooling |
|---|---|---|
| Unit | Individual functions in `src/` | `#[cfg(test)] mod tests` |
| Integration | Component interactions | `tests/*.rs` |
| Property (algebraic) | Random scalars over a deterministic slice; finds `d = V + j`; reverses subtraction; recovers small scalars; locks `to_hex_x` ↔ `x_bytes` round-trip | `proptest` |
| KAT (Known-Answer Tests) | SEC1 §2.7.1 vectors + `k256` reference outputs | `tests/kat.rs` |
| Differential | `k256` vs reference C `libsecp256k1` | `tests/differential.rs` |
| End-to-end / audit | Full pipeline recovery | `tests/audit.rs` |
| Orchestrator | Session lifecycle, checkpoint resume, corruption rejection | `tests/orchestrator.rs` |
| Miri (CI, required for `unsafe` changes) | Verifies the reviewed `libc::fsync` block; the rest of the codebase is safe by construction | `cargo +nightly miri test` |
| Benchmarks | Performance regression within the 5% gate | `criterion` |
| Audit (CI) | Vulnerability database | `cargo audit` |
| License (CI) | Dependency license compliance | `cargo deny` |
| Coverage (CI) | Line coverage tracking | `cargo tarpaulin` |

## See also

- [CONTRIBUTING.md](../CONTRIBUTING.md) — Contribution workflow, PR checklist
- [benchmarks.md](benchmarks.md) — How to run the benchmark suite
- [security.md](security.md) — Security properties verified by tests
- [architecture.md](architecture.md) — System architecture under test
