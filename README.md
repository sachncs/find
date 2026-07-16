<p align="center">
  <h1 align="center">Secp256k1 Find Tool</h1>
  <p align="center">High-performance secp256k1 private-key discovery using range-splitting and Montgomery batch inversion.</p>
  <p align="center">
    <a href="#installation"><img src="https://img.shields.io/badge/rust-1.81%2B-orange" alt="Rust"></a>
    <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT-green" alt="License"></a>
    <a href="https://github.com/sachncs/find/actions"><img src="https://img.shields.io/github/actions/workflow/status/sachncs/find/ci.yml?branch=master" alt="CI"></a>
    <a href="https://crates.io/crates/find"><img src="https://img.shields.io/crates/v/find" alt="crates.io"></a>
    <a href="https://github.com/sachncs/find/stargazers"><img src="https://img.shields.io/github/stars/sachncs/find" alt="Stars"></a>
  </p>
</p>

**find** is a high-performance Rust system for secp256k1 private key discovery
using a multi-variant range-splitting algorithm. It searches for scalars `j`
and offsets `V` such that `x(j·G) = x(P - V·G)`, yielding key candidates
`d = V ± j (mod n)`.

> **EDUCATIONAL AND RESEARCH USE ONLY.** This software is for pedagogical
> exploration of elliptic curve mathematics and high-performance Rust systems
> engineering. See [DISCLAIMER.md](DISCLAIMER.md).

---

## Features

- **512-Variant Search Engine** — Range-splitting using powers of 2 and cumulative summations; the 512-variant set is interned once per process (no per-session allocations).
- **Runtime-sized batch arrays** — `Vec<ProjectivePoint>` / `Vec<AffinePoint>` / `Vec<u8>` sized against `Config::batch_size` (1..=256). See [ADR-0009](docs/adr/0009-runtime-batch-size.md).
- **Batch Normalization** — Montgomery's simultaneous inversion for ~15–20× speedup in the normalization phase.
- **Parallel Sweep** — Work-stealing data-level parallelism via `rayon` with early-exit.
- **Lock-free cross-batch coordination** — `OnceLock<SearchMatch>` replaces the previous `Mutex + AtomicBool` pair (see [opt-decision 0007](docs/optimization-decisions/0007-oncelock-early-exit.md)).
- **Binary Caching** — Optional precomputation for I/O-bound cache scans (~100× speedup on NVMe).
- **Atomic Checkpointing** — Write-then-rename for crash-safe state persistence with integrity anchor.
- **Structured Observability** — Non-blocking rolling file logs with `tracing`.
- **Comprehensive Testing** — Property-based, integration, orchestrator, audit, KAT, and differential test suites.
- **Differential Testing** — Cross-implementation verification against `libsecp256k1` (the reference C implementation).
- **Fuzz Testing** — Six cargo-fuzz targets for the public APIs (`parse_pubkey`, `parse_pubkey_roundtrip`, `hex_to_scalar`, `scalar_mul_g`, `generate_variants`, `match_x`).
- **Strict Lint Configuration** — Curated `pedantic + nursery` clippy sets with a documented allow-list, gated by `-D warnings`.
- **Required-for-merge `cargo miri`** — Every PR runs `cargo +nightly miri test --workspace --all-features` on `ubuntu-latest`.

---

## Installation

### From crates.io

```bash
cargo install find
```

### From source

```bash
git clone https://github.com/sachncs/find.git
cd find
cargo build --release
```

### Requirements

- Rust **1.81** or later (the MSRV was bumped in commit 16 to use the stable `core::error::Error` trait)
- Supported platforms: Linux, macOS, Windows (x86_64 and aarch64)

---

## Quick Start

### CLI

```bash
# Basic search against a public key
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798

# Generate binary cache during search (~32 GB per billion scalars)
find --pubkey 0279be66... --cache-points

# Tune batch size and variant count (advanced; commits 7a/7b honour both at runtime)
find --pubkey 0279be... --batch-size 64 --variants 256

# Custom data and log directories
find --pubkey 0279be66... --output-dir <DIR> --log-dir <DIR>
```

For a guided walkthrough, see [docs/getting-started.md](docs/getting-started.md).

### Rust API (library)

```rust
use find::config::Config;
use find::ecc;
use find::orchestrator;

let pubkey = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
let config = Config::new(pubkey, "data", false)
    .try_with_batch_size(32)?      // 1..=256; returns FindError::InvalidConfig on out-of-range
    .try_with_variant_count(512)?; // 1..=512

let match_ = orchestrator::run(&config)?;
if let Some(m) = match_ {
    println!("MATCH DISCOVERED via {} at j={}", m.label, m.j);
    println!("Candidates (d = V ± j): {:?}", m.candidates_hex());
}
# Ok::<(), find::error::FindError>(())
```

The high-level entry point is `find::orchestrator::run(&Config) -> Result<Option<SearchMatch>, FindError>`. The `[Scalar; 2]` candidate array from [`SearchMatch`] can be inspected directly (`m.candidates`) or converted to hex via the `candidates_hex()` accessor for display.

---

## Configuration

### Environment Variables

| Setting | Env Variable | Default | Description |
|---------|--------------|---------|-------------|
| Log filter | `RUST_LOG` | `info` | Log level filter (e.g. `debug`, `trace`) |
| Backtraces | `RUST_BACKTRACE` | `0` | Set to `1` for backtraces on panic |

### Compile-time Constants and Runtime Defaults

| Symbol | Value | Source | Purpose |
|--------|-------|--------|---------|
| `TRILLION` | `1_000_000_000_000` | `src/orchestrator.rs` | Step size for audit-boundary logging |
| `DEFAULT_CACHE_CHUNK_SIZE` | `1_000_000_000` | `src/orchestrator.rs` | Scalars per cache chunk |
| `BatchSize::DEFAULT.get()` | `32` | `src/config.rs` | Default batch size |
| `BatchSize::MAX` | `256` | `src/config.rs` | Largest legal batch size |
| `MAX_VARIANT_COUNT` | `512` | `src/config.rs` | Largest legal variant count |
| `MAX_SEARCH` | `u64::MAX` | `src/orchestrator.rs` | Sweep upper bound |

The `DEFAULT_BATCH_SIZE` constant is retained as the public default for benchmark / documentation use; the runtime controlling value is `Config::batch_size` of type `BatchSize` (commit 7a).

### CLI Flags

| Flag | Type | Default | Range | Effect |
|------|------|---------|-------|--------|
| `--batch-size` | `u32` | `32` | `1..=256` | Points per Montgomery batch normalization; honoured at runtime (commit 7b) |
| `--variants` | `u32` | `512` | `1..=512` | Powers-of-two + cumulative-sum variants |
| `--cache-points` | `bool` | `false` | — | Persist X-coords to disk for I/O-bound re-runs |

Out-of-range `--batch-size` or `--variants` values are reported as `FindError::InvalidConfig` and cause the binary to exit with a non-zero status (no panic).

See [docs/configuration.md](docs/configuration.md) for the full configuration reference, including the curated `[lints]` config that drives `cargo clippy --all-targets --all-features -- -D warnings`.

---

## Performance

The hot loop is dominated by:

- One bootstrap scalar multiplication per `batch_size`-sized batch (~256 field mults at the default `batch_size = 32`).
- `batch_size - 1` mixed `+G` additions (~12 field mults each).
- Montgomery simultaneous inversion over the `batch_size`-point batch.
- A binary-search `match_x` in a 16 KiB L1-resident key array.

For the per-batch hot loop, the dominant cost is the **bootstrap scalar
multiplication** (~80% of per-batch cycles). Increasing `batch_size` beyond 64
has diminishing returns; the `+G` chain + Montgomery normalize + match together
take the remaining 20%. The batch-size choice now trades against per-batch
allocation cost (the hot-path arrays are heap-allocated and sized at
runtime) — see [ADR-0009](docs/adr/0009-runtime-batch-size.md).

The per-session cold-start cost of `generate_variants` is now effectively
free on the happy path: the 512-entry variant metadata (label, scalar,
decimal offset) is interned via `OnceLock<Box<[OffsetVariant; 512]>>` once
per process. Only the target-specific `compute_variant_x_bytes` remains
per-session work. See [opt-decision 0002](docs/optimization-decisions/0002-variant-labels-once-lock.md) and the new `optimization-decisions/0007-oncelock-early-exit.md` for the surrounding decisions.

For sustained workloads, the cached sweep path is ~100× faster than the
CPU-bound path on NVMe hardware (see [docs/performance.md](docs/performance.md)
for the full guide).

Reproduce the published cycle counts:

```bash
cargo bench --bench bench                               # criterion microbenchmarks
cargo bench --bench bench -- --baseline current -- --threshold 5  # 5% regression gate
scripts/build-pgo.sh                                   # profile-guided optimized build
scripts/run-benchmarks.sh                              # benchmark wrapper
```

---

## Research Reproducibility

The codebase ships with five independent verification layers:

- **KAT tests** (`tests/kat.rs`): 13 known-answer tests against SEC1 §2.7.1 vectors and `k256` reference outputs, plus the `to_hex_x` ↔ `x_bytes` round-trip regression test from commit 4 (`kat_to_hex_x_matches_x_bytes_hex` + `prop_to_hex_x_equals_x_bytes_hex`).
- **Differential tests** (`tests/differential.rs`): cross-check `k256` against the reference C `libsecp256k1` for 12 boundary scalars (1, 2, 3, 7, 100, 1k, 1M, 1.2G, 2^32, 2^63, u64::MAX, …).
- **Audit tests** (`tests/audit.rs`): end-to-end pipeline (parse → variants → sweep → recover) for the known scalar `1234567890` plus a 20-case proptest over `[2, 10_000]`.
- **Integration tests** (`tests/integration.rs`): randomized discovery + the new `prop_batch_size_runtime` proptest that exercises `--batch-size in 1..=256` and verifies the resulting match is invariant under the batch choice.
- **Unit tests** (in `src/`): every public function in `search`, `ecc`, `config`, and `error` has a unit test (71 lib tests at last count).

Run all five with `cargo test --all-targets --all-features`. The full local pre-commit gate (mirrors CI) is:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo +nightly miri test --workspace --all-features    # required for merges touching unsafe
```

---

## Migration (0.1.6 → 0.2.0)

The next release (`v0.2.0`) ships the breaking changes that landed during the
review-driven pass:

| API | Before (0.1.6) | After (0.2.0) |
|---|---|---|
| `Config::batch_size` field | `pub batch_size: u32` | `pub batch_size: BatchSize` (commit 7a) |
| `Config::{with_batch_size, with_variant_count}` builders | `fn(self, u32) -> Self` (panicking) | `#[deprecated]`; replaced by `try_with_*` builders returning `Result<Self, FindError>` |
| `--batch-size` honoured at runtime | ignored; fixed at `MAX_BATCH = 32` | honoured via heap-allocated runtime-sized batches (commit 7b) |
| `Config::validate` | shallow check only | renamed to `Config::validate_fields`; `Config::validate_pubkey()` deep validation retained (commit 3 + rename pass) |
| `find::search::SearchMatch::candidates` | `pub candidates: [String; 2]` | `pub candidates: [Scalar; 2]` (commit 12, breaking) |
| `SearchMatch::candidates_as_scalars` | `pub fn(&self) -> Result<[Scalar; 2]>` | `pub fn(&self) -> [Scalar; 2]` (no parsing needed) |
| `SearchMatch::candidates_hex()` | (did not exist) | new: returns `[String; 2]` |
| `find::search::generate_variants` | `-> Vec<OffsetVariant>` | `-> &'static [OffsetVariant]` (commit 7c, breaking) |
| `find::search::VariantIndex::new` | `fn(variants: Vec<OffsetVariant>) -> Self` | `fn(variants: &'static [OffsetVariant], x_bytes: &[[u8; 32]]) -> Self` |
| `find::search::OffsetVariant` | carries `x_bytes: [u8; 32]` | no longer carries `x_bytes`; `offset` field renamed to `offset_decimal` (use new `compute_variant_x_bytes` helper) |
| `SearchMatch::small_scalar` | `pub small_scalar: u64` | renamed to `SearchMatch::j: u64` |
| `find::search::perform_chunked_sweep` | — | renamed to `find::search::sweep_parallel` |
| `find::search::precompute_chunk` | — | renamed to `find::search::sweep_and_cache` |
| `find::persistence::perform_cached_sweep` | — | renamed to `find::persistence::sweep_cached` |
| `find::persistence::FileCacheWriter` | `pub struct FileCacheWriter` | renamed to `find::persistence::BinaryCacheWriter` |
| `find::persistence::save_variants_to_json` | — | renamed to `find::persistence::write_variants_json` |
| `find::telemetry::install_rayon_panic_handler` | — | renamed to `find::telemetry::install_worker_panic_handler` |
| `find::search::BATCH_SIZE` const | `pub const BATCH_SIZE: u64 = 32` | **removed** (use `find::config::DEFAULT_BATCH_SIZE`) |
| `find::config::MIN_J` const | `pub const MIN_J: u64 = 1` | renamed to `find::config::MIN_SEARCH_SCALAR` |
| `find::config::SweepRange` | available | **removed** (commit 8) |
| `find::search::MAX_BATCH` const | `pub const MAX_BATCH: usize = 32` | **removed** (commit 7b) |
| Doctest `Box<dyn std::error::Error>` | in 6+ places | replaced with `Box<dyn core::error::Error>` (commit 16, MSRV 1.81) |
| MSRV | 1.70 | **1.81** (commit 16) |

The full review-driven pass added (without breaking the API): `BatchSize` newtype + `try_with_*` builders, run-time batch sizing, `OnceLock<SearchMatch>` coordination, interned static variant metadata, strict `cargo clippy -D warnings` (with curated pedantic + nursery sets), and the required-for-merge `cargo miri` job.

---

## Documentation

All project documentation lives under [`docs/`](docs/README.md). Highlights:

- [docs/overview.md](docs/overview.md) — Project goals, scope, supported platforms
- [docs/architecture.md](docs/architecture.md) — System architecture, data flow, concurrency, sync primitives
- [docs/algorithms.md](docs/algorithms.md) — Mathematical foundation and pseudocode
- [docs/cli.md](docs/cli.md) — Full CLI reference
- [docs/configuration.md](docs/configuration.md) — Environment variables and constants
- [docs/modules.md](docs/modules.md) — Module-by-module reference for the `find` crate
- [docs/observability.md](docs/observability.md) — Logging, tracing, audit boundaries
- [docs/performance.md](docs/performance.md) — Performance characteristics and tuning
- [docs/testing.md](docs/testing.md) — Testing strategy and methodology
- [docs/security.md](docs/security.md) — Security model
- [docs/glossary.md](docs/glossary.md) — Terms, abbreviations, definitions
- [docs/adr/](docs/adr/README.md) — Architecture Decision Records (0001–0009)
- [docs/optimization-decisions/](docs/optimization-decisions/README.md) — Per-optimization rationale (0001–0007)

---

## Project Structure

```
find/
├── src/
│   ├── lib.rs          # Crate root; #![warn(missing_docs)] + #![warn(rustdoc::broken_intra_doc_links)]
│   ├── main.rs         # CLI binary entry point
│   ├── config.rs       # Config, BatchSize newtype, validation, MAX_BATCH_SIZE / MAX_VARIANT_COUNT
│   ├── ecc.rs          # SEC1 parsing, point arithmetic, hex conversion, to_hex_x
│   ├── error.rs        # FindError (8 variants) + Result alias
│   ├── search.rs       # Pure domain logic: VariantIndex, generate_variants, sweep_parallel,
│   │                   #   sweep_and_cache, compute_variant_x_bytes, CacheWriter trait, Progress
│   ├── persistence.rs  # Checkpoint save/load, BinaryCacheWriter, sweep_cached,
│   │                   #   write_variants_json (the only libc::fsync unsafe lives here)
│   ├── orchestrator.rs # run(&Config) entry point + checkpoint/lifecycle loop
│   └── telemetry.rs    # tracing-subscriber + Rayon panic handler
├── tests/
│   ├── audit.rs        # End-to-end key recovery + 20-case proptest
│   ├── differential.rs # Cross-check vs libsecp256k1 (12 boundary scalars)
│   ├── integration.rs  # Randomized discovery, prop_batch_size_runtime,
│   │                   #   prop_search_finds_any_scalar_in_range, edge cases
│   ├── kat.rs          # 13 known-answer tests + to_hex_x round-trip
│   └── orchestrator.rs # Session flow + checkpoint-resume + corruption rejection
├── benches/
│   └── bench.rs        # Criterion micro-benchmarks (9 cases including x_bytes,
│                       #   batch_normalization, +G chain, end-to-end)
├── fuzz/
│   └── fuzz_targets/   # cargo-fuzz targets (6):
│       ├── parse_pubkey.rs
│       ├── parse_pubkey_roundtrip.rs
│       ├── hex_to_scalar.rs
│       ├── scalar_mul_g.rs
│       ├── generate_variants.rs
│       └── match_x.rs
├── docs/               # Architecture, algorithms, ADRs, optimization decisions
├── Cargo.toml          # Package metadata, deps, [lints], profiles
└── README.md
```

---

## Development

```bash
cargo build --release
cargo test
cargo bench
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo +nightly miri test --workspace --all-features   # required if you touched unsafe
```

The project also ships a `Makefile`:

| Command | Description |
|---------|-------------|
| `make build` | Compile production binary (opt-level=3, lto=fat) |
| `make test` | Run exhaustive test suite |
| `make bench` | Run micro-benchmarks with Criterion |
| `make lint` | Run fmt + clippy + doc checks |
| `make doc` | Generate and open API documentation |
| `make coverage` | Generate HTML coverage report (`cargo tarpaulin`) |
| `make deny` | Run `cargo-deny` for license/dependency auditing |
| `make all-checks` | Run the full verification suite (`scripts/check-all.sh`) |

The **local pre-commit gate** (mirrors CI) is documented in detail in [CONTRIBUTING.md](CONTRIBUTING.md); the `cargo +nightly miri test --workspace --all-features` step is required for any PR that adds or modifies `unsafe` code, and the `cargo bench --bench bench -- --baseline current -- --threshold 5` gate ensures no hot-path change regresses by more than 5%.

---

## Testing

```bash
cargo test --all-targets --all-features
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo +nightly miri test --workspace --all-features   # ~10–30 min on first run
```

---

## Build

```bash
cargo build --release
```

Release profile (`Cargo.toml`):

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = 'abort'
strip = true
overflow-checks = true
```

---

## Release

Versions follow [Semantic Versioning](https://semver.org/). The crate is currently at `0.1.6`; the next release (`0.2.0`) is the SemVer-minor bump that ships the breaking API changes documented in [Migration](#migration-016--020) above. Releases are tagged via `version:X.Y.Z` commits in [CHANGELOG.md](CHANGELOG.md) and published to crates.io via the CI workflow (`release.yml`).

---

## Tech Stack

| Category | Technology |
|----------|------------|
| Language | Rust (2021 edition, **MSRV 1.81**) |
| Cryptography | k256 0.13 (`arithmetic`, `serde`, `bits`, `pkcs8`) — pure-Rust, audited |
| Parallelism | rayon 1.8 (work-stealing); `find_map_any` for early-exit |
| CLI | clap 4.4 (derive) |
| Error Handling | `thiserror` 2 (library), `anyhow` 1 (binary) |
| Serialization | serde 1 + serde_json 1 (checkpoint + points.json export) |
| Observability | `tracing` 0.1 + `tracing-subscriber` 0.3 (`env-filter`) + `tracing-appender` 0.2 |
| Hot-path encoding | hex 0.4, `k256::elliptic_curve::bigint::U256` (no BigUint) |
| Hex + big-int (test helpers) | `num-bigint` 0.5 |
| POSIX (Unix only) | `libc` 0.2 (the one reviewed `unsafe`: `libc::fsync` in `src/persistence.rs`) |
| Testing | `proptest` 1.11, `criterion` 0.8, `tempfile` 3, `rand` 0.10, `rand_chacha` 0.10, `num-traits` 0.2 |
| CI/CD | GitHub Actions (Ubuntu + macOS + Windows); `cargo miri` on `ubuntu-latest` is required-for-merge |

---

## Recently delivered (review-driven pass, commits 1–16)

The following items shipped in commits 1–16 of the `master` branch and are described in detail in their respective commits and ADRs:

- **Removed unsafe from `u256_to_decimal`** (commit 1) — the only `unsafe` in `src/search.rs` is gone.
- **Tightened `libc::fsync` SAFETY comment** (commit 2) — three-clause self-contained justification.
- **`Config::validate_pubkey`** + **`FindError::InvalidConfig`** (commit 3) — deep SEC1 fail-fast at session start.
- **`to_hex_x` ↔ `x_bytes` round-trip regression test** (commit 4).
- **`to_hex_x` uses `AffineCoordinates::x()` directly** (commit 5) — drops the SEC1 framing round-trip.
- **`OnceLock<SearchMatch>` replaces `Mutex + AtomicBool`** (commit 6) — see [optimization-decisions/0007](docs/optimization-decisions/0007-oncelock-early-exit.md).
- **`BatchSize` newtype + `try_with_*` builders** (commit 7a) — the foundation for runtime-sized batches.
- **Heap-allocated hot-path batch arrays** (commit 7b) — `--batch-size` is finally honoured at runtime; see [ADR-0009](docs/adr/0009-runtime-batch-size.md).
- **`generate_variants -> &'static [OffsetVariant]`** (commit 7c) — interned metadata; per-session `compute_variant_x_bytes`.
- **`SweepRange` removed** (commit 8) — dead newtype purged.
- **Required-for-merge `cargo miri` job** (commit 9) — `.github/workflows/ci.yml::miri`.
- **Curated `[lints]` section** (commit 10) — pedantic + nursery with a documented allow-list.
- **`SearchMatch.candidates: [Scalar; 2]`** (commit 12) — breaking; new `candidates_hex()` accessor.
- **`copy_from_slice` in cached sweep** (commit 13) — drops `try_into + expect`.
- **ADR-0009, optimization-decision 0007, CHANGELOG rollup, doc refresh** (commit 14).
- **MSRV 1.70 → 1.81** (commit 16) — enables stable `core::error::Error`.

## Roadmap

- Profile-guided optimization (PGO) CI integration
- WebAssembly target (WASI) for browser-based research tooling (`wasm32-unknown-unknown`)
- Formal verification of `+G` chain correctness and matching invariant
- Extended KAT/differential coverage (bigger `TEST_SCALARS` set)
- `secp256r1` / `secp384r1` (RustCrypto `elliptic-curves`) — see [docs/roadmap.md](docs/roadmap.md) for the full list

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the fork-and-branch workflow, commit
conventions (Conventional Commits), and [Architecture Decision Records](docs/adr/README.md).
PRs that touch `unsafe` code MUST pass `cargo +nightly miri test --workspace
--all-features` locally; see [CONTRIBUTING.md#unsafe-code-changes-must-pass-miri](CONTRIBUTING.md).

## Code of Conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities to **sachncs@gmail.com** — see [SECURITY.md](SECURITY.md).
For the security model, see [docs/security.md](docs/security.md).

## License

[MIT](LICENSE-MIT) © 2026 Sachin

---

**Disclaimer:** This tool is for educational and research purposes only. See [DISCLAIMER.md](DISCLAIMER.md) for full legal terms.
