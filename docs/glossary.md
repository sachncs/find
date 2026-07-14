# Glossary

This glossary defines terms and abbreviations used throughout the documentation. Where a term maps directly to a source-code symbol, the symbol is referenced.

## A

### Affine coordinates
A representation of an elliptic curve point as `(X, Y)` over the field `F_p`. Contrast with [projective coordinates](#projective-coordinates). Conversion from projective to affine requires a modular inversion and is therefore expensive — see [ADR-0002](adr/0002-batch-normalization.md) for how this cost is amortized.

### Audit boundary
A scalar boundary (currently `32 × TRILLION = 3.2 × 10^13`) at which the orchestrator emits a structured log message. Audit boundaries are informational only; they do not affect correctness. See [observability.md](observability.md#audit-boundaries).

## B

### Batch normalization
A technique that amortizes modular inversion across multiple points using Montgomery's simultaneous inversion trick. The `k256` crate exposes this as [`ProjectivePoint::batch_normalize`]. With `BatchSize::DEFAULT = 32`, the per-point cost of affine extraction drops by ~15–20×. See [ADR-0002](adr/0002-batch-normalization.md) and [performance.md](performance.md#batch-normalization).

### `BatchSize`
A `u32` newtype in [`src/config.rs`](../src/config.rs) (commit 7a) that wraps the hot-path batch-size knob. Construction is fallible: `BatchSize::new(u32) -> Result<BatchSize, FindError>`. The legal range is `BatchSize::MIN..=MAX = 1..=256`. `Config::batch_size: BatchSize` is now the runtime-controlling value (passed as `batch_size: u32` to `perform_chunked_sweep` and `precompute_chunk`); the legacy `BATCH_SIZE` constant is retained only as a default / documentation anchor.

### `BATCH_SIZE`
A public constant in [`src/search.rs`](../src/search.rs) (`pub const BATCH_SIZE: u64 = 32`). Retained for benchmark / documentation use only — the runtime-controlling value is [`Config::batch_size`](modules.md#orchestrator) of type [`BatchSize`](modules.md#config). The previous compile-time-bounded `[T; MAX_BATCH]` hot-path arrays are gone (commit 7b).

### Binary cache
A file containing a contiguous sequence of 32-byte big-endian X-coordinates of the form `j·G` for `j ∈ [start, end]`. The cache enables I/O-bound sweeps that bypass ECC arithmetic entirely. See [ADR-0006](adr/0006-binary-cache-format.md) and [operations.md](operations.md#binary-cache-management).

## C

### `CacheCorrupted`
A [`FindError`](modules.md#error) variant raised when a binary cache file is not a multiple of 32 bytes in size.

### `CacheWriter`
A trait defined in [`src/search.rs`](../src/search.rs) that abstracts over the persistence of X-coordinate blocks. The `search` module depends only on this trait, keeping it free of file-system details. See [ADR-0005](adr/0005-pure-search-module.md).

### `Config::validate_pubkey`
A deep SEC1 validation entry point on [`Config`](modules.md#orchestrator) introduced in commit 3. `Config::validate()` is the shallow check (non-empty pubkey); `Config::validate_pubkey()` additionally runs the pubkey through [`ecc::parse_pubkey`](modules.md#ecc), raising `FindError::InvalidPublicKey` (or `FindError::HexError`) on any SEC1 failure. Both run at the top of `orchestrator::run`.

### `CACHE_CHUNK_SIZE`
A constant defined in [`src/orchestrator.rs`](../src/orchestrator.rs). Currently `1_000_000_000` (one billion). The orchestrator processes the scalar space in chunks of this size. Each chunk corresponds to ~32 GB of binary cache on disk.

### Checkpoint
A JSON file (`checkpoint.json` by default) that records the last successfully completed scalar index, the associated public key, and an integrity anchor (the X-coordinate of `last_j · G`). On resume the anchor is recomputed and compared; mismatch raises [`ResearchIntegrityError`](#researchintegrityerror). See [architecture.md](architecture.md#persistence-layer) and [ADR-0003](adr/0003-atomic-checkpointing.md).

### Curve order
The prime order `n` of the secp256k1 group:

```
n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
```

All candidate scalars are reduced modulo `n`; the sweep excludes the identity point (j = 0).

## D

### Discovery
The act of finding a scalar `d` such that `d·G = P` for a given public key `P`. The tool produces two **candidates** per match: `V + j` and `V - j` (mod `n`), because X-coordinate matching cannot distinguish the Y-parity of `P - V·G`.

## E

### `EccError`
A [`FindError`](modules.md#error) variant for low-level elliptic-curve failures (typically scalar overflow or unexpected identity point).

### ECC
Elliptic-curve cryptography. In this project, "ECC" always refers to operations on secp256k1.

## F

### `FindError`
The unified error type returned by every fallible function in the `find` crate. See [modules.md#error](modules.md#error) and [ADR-0004](adr/0004-error-hierarchy.md).

### `find_map_any`
A `rayon` parallel iterator method that searches across batches and returns the first `Some` result, terminating other workers early. Used by both `perform_chunked_sweep` and `precompute_chunk`.

## G

### Generator point (`G`)
The standard base point of secp256k1, defined in SEC1 § 2.7.1. The tool exposes it via [`ecc::generator()`](modules.md#ecc).

## H

### `HexError`
A [`FindError`](modules.md#error) variant raised when hexadecimal decoding fails (invalid character or odd length).

## I

### `InvalidPublicKey`
A [`FindError`](modules.md#error) variant raised when SEC1 parsing fails (wrong prefix, off-curve coordinates, or point-at-infinity input).

### `InvalidConfig`
A [`FindError`](modules.md#error) variant raised when a [`Config`](modules.md#orchestrator) field is out of legal range — specifically `Config::try_with_batch_size` (1..=256) or `Config::try_with_variant_count` (1..=512). Added in commits 3 + 7a. The CLI binary propagates this as a non-zero exit code.

### Identity point (`𝒪`)
The additive identity of the elliptic curve group. Represented as the projective point `(0 : 1 : 0)` in standard notation. Subtraction of a point from itself yields `𝒪`; when `j = 0` the sweep would produce `𝒪` for `j·G` and is therefore clamped to start at `1`.

## J

### `j`
The "search scalar" — the running variable of the sweep. For each `j`, the engine checks `x(j·G) = x(P - V·G)`. The sweep range is `[1, MAX_SEARCH]` where `MAX_SEARCH = u64::MAX`.

## L

### `last_j`
The largest `j` that has been **fully swept** (i.e. the previous chunk's end). The next sweep starts at `last_j + 1`. This is the scalar index recorded in the [checkpoint](#checkpoint).

## M

### `MAX_SEARCH`
The upper bound of the sweep, declared in [`src/orchestrator.rs`](../src/orchestrator.rs) as `u64::MAX`. Effectively `2^64 - 1`.

### `MIN_J`
The minimum search scalar, declared in [`src/orchestrator.rs`](../src/orchestrator.rs) as `1`. The identity point is excluded.

### Montgomery simultaneous inversion
The mathematical technique used for [batch normalization](#batch-normalization). Replaces `N` independent modular inversions with a single inversion plus `O(N)` multiplications. See [algorithms.md](algorithms.md#batch-normalization) and [ADR-0002](adr/0002-batch-normalization.md).

## O

### `OffsetVariant`
A struct in [`src/search.rs`](../src/search.rs) that carries a scalar offset `V` (`v_scalar: Scalar`), the variant label (`"2^i"` or `"sum(2^0..2^i)"`), and the decimal offset string. Since commit 7c it **no longer carries** the target-specific X-coordinate — that lives in a parallel `Vec<[u8; 32]>` produced by [`compute_variant_x_bytes`](modules.md#search).

### `OnceLock`
A [`std::sync::OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html) used in two places after the review-driven pass:

1. `search::generate_variants` — interns the 512-variant metadata array once per process; the public API returns the `&'static` slice.
2. `search::precompute_chunk` — replaces the previous `Mutex + AtomicBool` cross-batch match-coordination pair with a single lock-free `OnceLock<SearchMatch>` (commit 6; see [optimization-decisions/0007](../optimization-decisions/0007-oncelock-early-exit.md)).

Because `OnceLock` has no mutex, there is no poisoning recovery path; panicking workers cannot corrupt the result.

## P

### Point at infinity
See [identity point](#identity-point-𝒪).

### Projective coordinates
A representation of an elliptic curve point as `(X : Y : Z)`, where the affine point is `(X/Z, Y/Z)`. Arithmetic in projective coordinates avoids the cost of modular inversion; the conversion to affine is performed in batches. See [ADR-0002](adr/0002-batch-normalization.md).

### Precomputation
The act of generating a [binary cache](#binary-cache) for a range of scalars. When `--cache-points` is enabled, the orchestrator precomputes each chunk and writes it to disk before performing the I/O-bound sweep.

### Private key
In this context, the scalar `d` such that `P = d·G` is the target public key. The sweep attempts to recover `d`.

### Public key (`P`)
A point on the secp256k1 curve. Accepted as a SEC1 hex string (compressed or uncompressed).

## R

### Range-splitting
The high-level strategy of decomposing the scalar space into multiple smaller ranges (here, 512 [variants](#variant)) and searching them in parallel. See [algorithms.md](algorithms.md#multi-variant-range-splitting) and [ADR-0001](adr/0001-multi-variant-search.md).

### `ResearchIntegrityError`
A [`FindError`](modules.md#error) variant raised when a checkpoint's X-coordinate anchor does not match the value recomputed from `last_j·G`. Indicates corruption or a logic change.

## S

### SEC1
The *Standards for Efficient Cryptography* format for elliptic-curve key encoding. The tool accepts both compressed (33-byte) and uncompressed (65-byte) SEC1 hex inputs. See [references.md](references.md).

### `SearchMatch`
A struct in [`src/search.rs`](../src/search.rs) describing a successful match: the variant label (`label`), the original unreduced offset decimal string (`offset`), the small scalar `j` (`small_scalar: u64`), and the two candidate private keys as `candidates: [k256::Scalar; 2]` (commit 12). The accessor `m.candidates_hex() -> [String; 2]` returns the trimmed-hex form; `m.candidates_as_scalars() -> [Scalar; 2]` returns the underlying scalars.

### Sweep
The core operation: iterating `j` from `start` to `end` and checking each `j·G` against the variant index. May be CPU-bound (`perform_chunked_sweep`) or I/O-bound (`perform_cached_sweep`).

## T

### `TRILLION`
A constant defined in [`src/orchestrator.rs`](../src/orchestrator.rs) with value `1_000_000_000_000` (`10^12`). Used as a step size for human-readable boundary logging and for the [audit boundary](#audit-boundary) (`32 × TRILLION`). Not the cache-chunk size.

## V

### Variant
A specific shift offset `V` used to construct a candidate equality check. The tool uses 256 powers of two (`2^0..2^255`) and 256 cumulative sums (`Σ 2^0..2^i`), totalling 512 variants. See [ADR-0001](adr/0001-multi-variant-search.md).

### `VariantIndex`
A cache-optimized lookup structure in [`src/search.rs`](../src/search.rs) that stores variants sorted by X-coordinate. Lookups are `O(log 512)` binary searches with excellent L1/L2 locality.

## W

### Write-then-rename
The atomic persistence strategy used for checkpoints. The new content is written to a `.tmp` file, flushed to disk, and then renamed over the target. On Unix, the parent directory is also `fsync`-ed. See [ADR-0003](adr/0003-atomic-checkpointing.md).
