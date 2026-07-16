# Configuration

The `find` tool has a deliberately small configuration surface. Most behavior is fixed in source; only a handful of environment variables and CLI flags are exposed for run-time tuning.

## Environment variables

| Variable | Default | Effect |
|---|---|---|
| `RUST_LOG` | `info` | Log level filter for `tracing-subscriber` (e.g. `debug`, `trace`, `info`, `warn`, `error`) |
| `RUST_BACKTRACE` | `0` | Set to `1` to print backtraces on panic |
| `CARGO_TERM_COLOR` | (auto) | Standard Cargo color setting; propagated to the build pipeline |

### `RUST_LOG` examples

```bash
# Default: info-level events only
find --pubkey 0279be66...

# Debug-level: per-batch progress, variant construction details
RUST_LOG=debug find --pubkey 0279be66...

# Trace-level: every scalar multiplication, every cache write
RUST_LOG=trace find --pubkey 0279be66...

# Filter to a specific module
RUST_LOG=find::search=debug find --pubkey 0279be66...

# Combine: debug for the search module, info elsewhere
RUST_LOG=info,find::search=debug find --pubkey 0279be66...
```

The full `tracing-subscriber` directive syntax is documented at <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/struct.EnvFilter.html>.

## CLI flags

See [cli.md](cli.md) for the complete flag reference. The CLI flags that affect runtime behavior are:

| Flag | Type | Default | Range | Effect |
|------|------|---------|-------|--------|
| `--output-dir` | `String` | `data` | — | Root for checkpoints, caches, and exported variant metadata |
| `--log-dir` | `String` | `logs` | — | Directory for daily-rolling log files |
| `--cache-points` | `bool` | `false` | — | Generate and persist binary cache files |
| `--batch-size` | `u32` | `32` | `1..=256` | Points per Montgomery batch normalization; honoured at runtime (commit 7b). Out-of-range values produce `FindError::InvalidConfig` and a non-zero exit. |
| `--variants` | `u32` | `512` | `1..=512` | Powers-of-two + cumulative-sum variants. Out-of-range values produce `FindError::InvalidConfig` and a non-zero exit. |

The two tunables flow through the fallible `Config::try_with_batch_size` /
`Config::try_with_variant_count` builders added in commit 7a (replacing
the legacy panicking `with_*` builders, which are now `#[deprecated]`).

## Compile-time constants and runtime defaults

The following are documented for transparency. Items marked **Runtime**
are accessible via `Config` and can be overridden without recompiling;
items marked **Compile-time** require an edit + rebuild.

### Search parameters

| Constant | Defined in | Kind | Value | Purpose |
|---|---|---|---|---|
| `MAX_SEARCH` | `src/orchestrator.rs` | Compile-time | `u64::MAX` (2^64 - 1) | Theoretical upper bound of the search range |
| `MIN_SEARCH_SCALAR` | `src/config.rs` | Compile-time | `1` | Minimum non-zero search scalar (excludes the identity point) |
| `DEFAULT_CACHE_CHUNK_SIZE` | `src/orchestrator.rs` | Runtime (default) | `1_000_000_000` | Scalars per cache chunk (one billion) |
| `BatchSize::DEFAULT.get()` | `src/config.rs` | Runtime (default) | `32` | Default points per batch normalization |
| `BatchSize::MIN ..= MAX` | `src/config.rs` | Compile-time | `1 ..= 256` | Legal range of `Config::batch_size` |
| `MAX_VARIANT_COUNT` | `src/config.rs` | Compile-time | `512` | Largest legal `Config::variant_count` |
| `TRILLION` | `src/orchestrator.rs` | Compile-time | `1_000_000_000_000` | Step size for human-readable audit-boundary logging |

The previous `BATCH_SIZE` constant in `src/search.rs` was redundant with
`config::DEFAULT_BATCH_SIZE` and was removed; the runtime-controlling
value is `Config::batch_size` of type `BatchSize`. The constant pool
was split in commit 7a, the hot-path arrays were moved to heap
allocation in commit 7b (see [ADR-0009](adr/0009-runtime-batch-size.md)),
and the duplicate `search::BATCH_SIZE` was removed during the rename
pass.

### Audit boundary

The orchestrator logs an informational message at every
`32 × TRILLION = 3.2 × 10^13` scalar steps. This is a non-load-bearing
constant used for long-running research observability; it does not
affect correctness.

### Pre-allocation

When `--cache-points` is enabled, the orchestrator pre-allocates the
cache file to `(chunk_end - chunk_start + 1) × 32` bytes before writing.
The pre-allocation is a hint to the file system and may be ignored
(e.g. on filesystems that do not support `fallocate`).

## Lint configuration (commit 10)

The crate enables a curated set of clippy lints via the `[lints]` section
of `Cargo.toml`. The `-D warnings` gate is the local pre-commit bar
(see [CONTRIBUTING.md](../CONTRIBUTING.md)):

- **`[lints.rust]`**: `unused_must_use = "warn"` (default).
- **`[lints.clippy]`**: `pedantic = { level = "warn", priority = -1 }` and
  `nursery = { level = "warn", priority = -1 }`, plus a project-specific
  allow-list covering `module_name_repetitions`, `cast_*` (the suite of
  `cast_possible_*` / `cast_precision_loss` / etc.), `similar_names`,
  `struct_excessive_bools`, `too_many_lines`, `module_inception`,
  `inline_always`, `needless_pass_by_value`, `items_after_statements`,
  `unreadable_literal`, and others that would churn without raising
  quality.

Run `cargo clippy --all-targets --all-features -- -D warnings` locally
before pushing a branch.

## `Cargo.toml` features

The `find` crate does not currently expose any `#[cfg(feature = ...)]`
gates. All dependencies and features are static:

| Dependency | Features enabled |
|---|---|
| `k256` | `arithmetic`, `serde`, `bits`, `pkcs8` |
| `tracing-subscriber` | `env-filter` |
| `clap` | `derive`, `env` |
| `libc` (Unix-only) | (none — direct FFI for `libc::fsync`) |

Dev-deps: `proptest` 1.11, `criterion` 0.8, `tempfile` 3.10, `rand` 0.10,
`rand_chacha` 0.10, `num-traits` 0.2, `secp256k1-sys` 0.13 (default-features
disabled, `std`).

## Release profile

The release binary in `Cargo.toml` is optimized for maximum throughput:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = 'abort'
strip = true
overflow-checks = true
```

| Setting | Effect |
|---|---|
| `opt-level = 3` | Maximum LLVM optimization |
| `lto = "fat"` | Link-time optimization across all crates |
| `codegen-units = 1` | Single code generation unit for the whole program (enables inlining across crate boundaries) |
| `panic = 'abort'` | No unwinding; smaller binary, no panic-handler code |
| `strip = true` | Strip debug symbols from the binary |
| `overflow-checks = true` | Enable integer overflow checks even in release mode (correctness > speed) |

The `overflow-checks` setting is intentional: the search engine uses
`saturating_*` arithmetic extensively, but the runtime checks serve as a
safety net for any future code that might use plain `+`/`-`/`*`. A
per-package opt-level override is also set:

```toml
[profile.release.package."*"]
opt-level = 3
```

This forces release-grade codegen for every dependency used in the hot
path (`rayon`, `k256`), preventing misleading cycle counts under
`cargo bench` when implicit `--release` is used.

## Logging configuration

The `tracing-subscriber` is initialized in [`src/main.rs::init_tracing`](../src/main.rs) with:

- A daily-rolling file appender writing to `<log_dir>/find.log.YYYY-MM-DD`.
- A stderr layer that mirrors the same events to the terminal.
- `EnvFilter` initialized from `RUST_LOG` with a default of `info`.

The non-blocking file writer (`tracing_appender::non_blocking`) decouples
log I/O from the CPU-bound sweep. Buffered events are flushed when the
returned `WorkerGuard` is dropped at process exit.

See [observability.md](observability.md) for the full logging model.

## Input validation

The orchestrator validates the configuration before the search begins.
The four failure modes are reported as distinct `FindError` variants:

| Validation | Source | Failure mode |
|---|---|---|
| `Config::pubkey` is non-empty / non-whitespace | `Config::validate_fields` (shallow) | `FindError::InvalidPublicKey("Public key cannot be empty")` |
| `Config::pubkey` parses as a SEC1 point (wrong prefix, off-curve, etc.) | `Config::validate_pubkey` (deep — delegates to `ecc::parse_pubkey`) | `FindError::InvalidPublicKey(...)` or `FindError::HexError(...)` |
| `Config::batch_size` is in `1..=256` | `Config::try_with_batch_size` | `FindError::InvalidConfig(...)` |
| `Config::variant_count` is in `1..=512` | `Config::try_with_variant_count` | `FindError::InvalidConfig(...)` |

Both validations in the first row (`validate` shallow and `validate_pubkey`
deep) run at the top of `orchestrator::run`; both `try_with_*` validations
run at the top of `src/main.rs::main` (so out-of-range CLI flags exit
non-zero before any state is allocated).

`Config::output_dir` is not validated; the directory is created on first
write via `std::fs::create_dir_all`. The `data/` and `checkpoints/`
subdirectories are created as needed.

## Resource budgets

The tool does not impose explicit CPU, memory, or disk quotas.
Recommended resource budgets for a single search session are documented
in [operations.md#resource-budgets](operations.md#resource-budgets) and
[overview.md#compatibility-matrix](overview.md#compatibility-matrix).

## MSRV

The crate's MSRV is **1.81**, declared in
[`Cargo.toml`](../Cargo.toml) (`rust-version = "1.81"`). It was bumped
from 1.70 to 1.81 in commit 16 so the doctest signatures could use the
stable `core::error::Error` trait. Downstream crates that pin MSRV ≤ 1.80
must vendor `core::error::Error` or delay their upgrade.
