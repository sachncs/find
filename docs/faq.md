# FAQ — Frequently Asked Questions

This document answers conceptual questions about the `find` tool. For operational issues, see [troubleshooting.md](troubleshooting.md). For performance tuning, see [performance.md](performance.md).

## Releases

### What's the difference between 0.1.6 and 0.2.0?

The review-driven pass in commits 1–18 ships 11 breaking API changes,
documented in the [Migration table in README.md](../README.md#migration-016--020).
The biggest ones are:

- `Config::batch_size` is now a `BatchSize` newtype (commit 7a); set it
  via the fallible `Config::try_with_batch_size(...)` builder.
- `--batch-size` is finally honoured at runtime (commit 7b); the
  hot-path arrays are heap-allocated and sized to the runtime value.
- `SearchMatch.candidates: [Scalar; 2]` instead of `[String; 2]`
  (commit 12, breaking); the human-readable hex form is exposed via
  the new `SearchMatch::candidates_hex()` accessor.
- `find::search::generate_variants` returns `&'static [OffsetVariant]`
  (interned via a process-wide `OnceLock`; commit 7c). The X-coordinates
  come from the new `find::search::compute_variant_x_bytes` helper.
- `find::config::SweepRange` is removed (commit 8); the
  `find::search::MAX_BATCH` constant is removed (commit 7b); the MSRV
  is bumped 1.70 → 1.81 for the stable `core::error::Error` trait.

Full migration table and Rust API example for `0.2.0` are in
[`README.md`](../README.md#migration-016--020).

### Is 0.1.6 still supported?

Yes, during the transition. 0.1.x stays in the "Yes — during the 0.2.0
transition window" bucket of the [support matrix](roadmap.md#supported-versions);
0.0.x is not supported. Critical bug fixes for 0.1.x are
backported on a best-effort basis; new development lands on `master` and
ships in 0.2.x.

### What is the Secp256k1 Find Tool?

A high-performance Rust system for educational and research exploration of elliptic curve mathematics. It demonstrates multi-variant range-splitting algorithms for secp256k1 private key discovery.

### Is this tool for recovering lost private keys?

**No.** This tool is for educational and research purposes only. It demonstrates cryptographic algorithms and high-performance systems engineering. See [DISCLAIMER.md](../DISCLAIMER.md) for full legal terms.

### What is secp256k1?

Secp256k1 is the elliptic curve used by Bitcoin and many other cryptocurrencies. It provides 256-bit security and is optimized for efficient computation. For the formal specification, see [references.md](references.md#standards).

### What does the project license cover?

The project is licensed under the MIT License. See [LICENSE-MIT](../LICENSE-MIT) for the full text and [SECURITY.md](../SECURITY.md) for the security policy.

## Installation

### What Rust version do I need?

Rust **1.81** or later (the MSRV was bumped from 1.70 in commit 16 to use the
stable `core::error::Error` trait; the doctest signatures in this crate use
`Box<dyn core::error::Error>`). Install via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Downstream crates that pin MSRV ≤ 1.80 must vendor `core::error::Error`
or delay their upgrade.

### Can I run this on Windows?

Yes. The tool supports Linux, macOS, and Windows. Some features like binary caching may have different performance characteristics on Windows because the parent-directory `fsync` is a no-op on NTFS. See [security.md#filesystem-selection](security.md#filesystem-selection).

### How do I build from source?

```bash
git clone https://github.com/sachncs/find.git
cd find
cargo build --release
```

The binary is at `target/release/find`. For other build options, see [deployment.md](deployment.md).

## Usage

### How do I run a search?

```bash
find --pubkey <HEX_SEC1_PUBLIC_KEY>
```

Example:

```bash
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

For the full CLI reference, see [cli.md](cli.md).

### What does the output mean?

When a match is found, you'll see:

```
============================================================
MATCH DISCOVERED (Variant: 2^10)
Shift scalar V: 1024
Search scalar j: 42
Target candidates (d = V +/- j):
  [1] 0x426
  [2] 0x3e2
Total Search Duration: 2.345s
============================================================
```

- **Variant**: The shift variant that produced the match (e.g. `"2^10"`, `"sum(2^0..2^7)"`)
- **Shift scalar V**: The original unreduced offset value (decimal)
- **Search scalar j**: The scalar that matched the X-coordinate
- **Target candidates**: Two possible private keys (V+j and V-j, both reduced mod n)
- **Total Search Duration**: Wall-clock time of the entire search session

Both candidates are emitted because X-coordinate matching cannot distinguish Y-parity. The caller must verify each externally.

### How long does a search take?

Search time depends on:

- CPU performance
- Size of the search range
- Whether binary caching is used
- Target scalar value

For small scalars (< 1 billion), results are typically found in seconds to minutes. The cached path is ~100× faster on NVMe hardware.

### What is binary caching?

Binary caching precomputes X-coordinates and stores them in a binary file. This allows subsequent searches to skip ECC arithmetic and perform direct file scans. See [ADR-0006](adr/0006-binary-cache-format.md) for the cache format.

### How much disk space does caching require?

Approximately 32 GB per billion points cached. For example:

- 1 billion points ≈ 32 GB
- 10 billion points ≈ 320 GB

To calculate the total disk requirement for a multi-chunk search, see [operations.md#disk-budget](operations.md#disk-budget).

### How does checkpointing work?

After each one-billion-point cache chunk (`CACHE_CHUNK_SIZE`), the tool writes the current state to `data/checkpoint.json` using write-then-rename for atomic persistence. The checkpoint includes an integrity anchor (the X-coordinate of `last_j · G`) that is recomputed and verified on resume. Mismatch raises `ResearchIntegrityError`.

Note: `TRILLION` (`10^12`) is a separate constant used for the **audit boundary** logging message (every 32 trillion steps), not the cache chunk size. The checkpoint fires at the end of every **billion**-scalar chunk.

See [ADR-0003](adr/0003-atomic-checkpointing.md) for the checkpoint design.

## Technical

### How does the algorithm work?

The tool uses multi-variant range-splitting:

1. Generate 512 shift variants (256 powers-of-2, 256 cumulative sums).
2. For each scalar `j`, check if `x(j·G) = x(P - V·G)`.
3. When matched, derive candidates `d = V ± j (mod n)`.

See [algorithms.md](algorithms.md) for the mathematical details and [ADR-0001](adr/0001-multi-variant-search.md) for the design rationale.

### What is batch normalization?

Batch normalization uses Montgomery's simultaneous inversion trick to amortize modular inversion costs across multiple points. For the default batch size of 32 points, this provides approximately **15–20× speedup** in the normalization phase. See [ADR-0002](adr/0002-batch-normalization.md), [performance.md#batch-normalization](performance.md#batch-normalization), and the benchmark in [`benches/bench.rs`](../benches/bench.rs).

### What is the VariantIndex?

The `VariantIndex` is a flat sorted array of 512 entries, sorted by X-coordinate. It provides `O(log 512)` binary search for X-coordinate matching, with excellent L1/L2 cache locality. The full array fits in L1 cache (~16 KB). See [architecture.md#search-layer](architecture.md#search-layer) and [ADR-0001](adr/0001-multi-variant-search.md).

### How does parallelism work?

The tool uses `rayon`'s work-stealing parallelism:

- Range is divided into batches of `Config::batch_size` scalars (default 32; range 1..=256; commit 7b).
- Each worker processes one batch independently.
- `find_map_any()` provides early exit on first match.
- No locks in the hot path: the `VariantIndex` is read-only after construction; `sweep_and_cache`'s cross-batch coordination is a single `OnceLock<SearchMatch>` (commit 6).

A custom Rayon `panic_handler` logs worker panics rather than aborting the process. The search hot path uses `OnceLock` for cross-batch coordination, which has no mutex to be poisoned; the only `Mutex` left in the application is `BinaryCacheWriter`'s non-Unix fallback in `src/persistence.rs`. See [observability.md#rayon-panic-handling](observability.md#rayon-panic-handling) and [optimization-decisions/0007-oncelock-early-exit.md](optimization-decisions/0007-oncelock-early-exit.md).

## Performance

### How can I improve performance?

1. **Use release builds:** `cargo build --release`
2. **Enable binary caching:** `--cache-points` flag
3. **Use fast storage:** NVMe SSDs for cache files
4. **Allocate more CPU cores:** The tool uses all available physical cores
5. **Set the CPU governor to `performance`:** prevents frequency drops during long sweeps
6. **Disable hyperthreading:** reduces cache thrash for the variant index

For the full tuning guide, see [performance.md#tuning-the-runtime-environment](performance.md#tuning-the-runtime-environment).

### What are the performance characteristics?

| Operation | Complexity | Notes |
|---|---|---|
| Variant generation | O(512) | One-time per target |
| Index lookup | O(log 512) | Binary search |
| Sweep (CPU) | O(R) | Linear over range |
| Sweep (I/O) | O(R) | Sequential binary read |
| Batch normalization | 1 inversion + 31 multiplications per 32 points | Montgomery simultaneous inversion |

See [performance.md#complexity](performance.md#complexity) for the full analysis.

## Development

### How do I run tests?

```bash
# Full test suite
make test

# Specific test
cargo test test_name

# With increased property-test cases
PROPTEST_CASES=1000 cargo test --release
```

For the full testing strategy, see [testing.md](testing.md).

### How do I run benchmarks?

```bash
make bench
```

For benchmark interpretation and historical tracking, see [benchmarks.md](benchmarks.md).

### How do I check code quality?

```bash
make lint
```

This runs `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings`.

### How do I generate coverage reports?

```bash
make coverage
```

This runs `cargo tarpaulin` and produces an HTML report. Coverage is also tracked via Codecov in CI.

## Contributing

### How do I contribute?

See [CONTRIBUTING.md](../CONTRIBUTING.md) for detailed guidelines.

### What contributions are accepted?

- Bug fixes
- Performance improvements
- Documentation enhancements
- Test coverage improvements
- Research-aligned features

### What contributions are NOT accepted?

- Features designed for non-educational use
- Changes that compromise mathematical correctness
- Optimizations that sacrifice code clarity without clear benefit

## Security

### How do I report security vulnerabilities?

**Do not open public issues.** See [SECURITY.md](../SECURITY.md) for reporting instructions.

### Is this tool secure?

The tool is designed with security in mind:

- One reviewed `unsafe` call (`libc::fsync` in `src/persistence.rs`); no other application-code `unsafe`. The review-driven pass (commits 1, 6) removed the two unsafes that existed at 0.1.6: `String::from_utf8_unchecked` in `u256_to_decimal`, and the `Mutex + AtomicBool` cross-batch coordination in `sweep_and_cache` (now `OnceLock<SearchMatch>`).
- Input validation on all operations: `Config::validate_fields` (shallow) + `Config::validate_pubkey` (deep SEC1 parse) at the top of `orchestrator::run`; fallible `try_with_batch_size` / `try_with_variant_count` in `main`.
- Checkpoint integrity verification
- Atomic file operations (write-then-rename + parent-dir `fsync` on Unix)
- Required-for-merge `cargo miri` job in CI (commit 9)

For the full security model, see [security.md](security.md).

## See also

- [troubleshooting.md](troubleshooting.md) — Operational issues
- [architecture.md](architecture.md) — System architecture
- [algorithms.md](algorithms.md) — Mathematical foundation
- [operations.md](operations.md) — Runbook
