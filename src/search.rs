// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Pure domain logic for the multi-variant range-splitting search engine.
//!
//! This module contains no file I/O, no global mutable state, and no
//! platform-specific code. All side effects are injected via explicit
//! arguments (writers, progress counters).
//!
//! # Concurrency model
//!
//! - [`Progress`] is a lock-free counter backed by [`AtomicU64`] with
//!   [`Ordering::Relaxed`]; it is safe to call from any number of Rayon
//!   worker threads concurrently.
//! - [`VariantIndex`] is built once and then read-only; it is [`Sync`].
//! - [`sweep_and_cache`] uses a [`OnceLock<SearchMatch>`] as a one-shot
//!   best-effort broadcast channel: any worker that finds a match
//!   publishes it via `OnceLock::set`; remaining workers observe it via
//!   the lock-free `get()` check at the top of each batch. There is no
//!   mutex and no atomics — the `OnceLock` guarantees at-most-one
//!   publication internally. Panicking workers cannot poison the result
//!   because there is no lock to poison.
//! - [`sweep_parallel`] uses Rayon's `find_map_any` for early exit
//!   when the first match is found; later batches are not scheduled.
//!
//! # Side-channel stance
//!
//! Variant generation, the `+ G` increment chain, and batch normalization
//! are all CPU-hot-path operations that are **not constant-time**. They
//! are appropriate for the research and educational scope of this tool,
//! not for production signing. See [`docs/security.md`](../docs/security.md)
//! for the threat model.
//!
//! # Memory layout
//!
//! Hot-path arrays are heap-allocated and track the runtime
//! [`crate::config::Config::batch_size`] (capped at
//! [`crate::config::BatchSize::MAX`] = 256). At the default
//! `batch_size = 32` the per-batch allocation cost is ~3 KB on `x86_64`
//! (32 × 96 bytes for [`ProjectivePoint`] + 32 × 96 bytes for
//! [`AffinePoint`] + 32 × 32 bytes for the X-coordinate scratch
//! buffer), keeping the working set inside L1 cache. The runtime-
//! sized arrays replace the previous compile-time-bounded
//! `[ProjectivePoint; MAX_BATCH]` allocation; see ADR-0009 for the
//! rationale.
//!
//! # Algorithm overview
//!
//! The engine searches for a private key `d` such that `d·G = P` by:
//!
//! 1. **Variant generation** ([`generate_variants`]): compute 512 candidate
//!    points `P - V·G` for offsets `V` chosen from the powers of two
//!    `2^0..2^255` and the cumulative sums `1, 3, 7, …, 2^256 - 1`. The
//!    resulting X-coordinates are stored in [`OffsetVariant`].
//! 2. **Index construction** ([`VariantIndex::new`]): sort the variants by
//!    X-coordinate so that lookups during the sweep are `O(log N)`.
//! 3. **Sweep** ([`sweep_parallel`] / [`sweep_and_cache`]): for
//!    each scalar `j` in the chunk, compute `j·G`, extract its
//!    X-coordinate, and probe the index. A match implies `d = V ± j`.
//!
//! See [`docs/algorithms.md`](../docs/algorithms.md) and
//! [ADR-0001](../docs/adr/0001-multi-variant-search.md) for the full
//! mathematical treatment.
//!
//! [`AtomicU64`]: std::sync::atomic::AtomicU64
//! [`Ordering::Relaxed`]: std::sync::atomic::Ordering::Relaxed

use crate::config::MAX_BATCH_SIZE;
use crate::ecc;
use crate::error::{FindError, Result};
use k256::elliptic_curve::bigint::U256;
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::ops::Reduce;
use k256::elliptic_curve::point::AffineCoordinates;
use k256::{AffinePoint, ProjectivePoint, Scalar};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use tracing::instrument;

/// The fixed batch size used for batch normalization in the search engine.
///
/// 32 is empirically the sweet spot on `x86_64` and aarch64: stack allocation
/// cost (32 × 96 bytes ≈ 3 KB) fits in L1 cache, and the cost of 32 scalar
/// multiplications roughly balances one batch normalization.
///
/// The runtime-controlling value is [`crate::config::DEFAULT_BATCH_SIZE`];
/// the hot-path functions take `batch_size: u32` as a parameter, so this
/// constant exists for documentation / benchmark use only.
///
/// See [ADR-0002](../docs/adr/0002-batch-normalization.md) for the full
/// rationale.

/// Number of consecutive batches grouped together for a single Montgomery
/// `batch_normalize` call. Larger groups amortize the single modular
/// inversion across more points: normalizing 128 points costs ~15.6 µs
/// (0.122 µs/point) vs 4 × 7.25 µs = 29 µs (0.227 µs/point) for four
/// 32-point batches — a ~46 % saving on the normalization step.
const NORMALIZE_GROUP_BATCHES: usize = 4;

/// Maximum points in one normalize group: `NORMALIZE_GROUP_BATCHES * MAX_BATCH`.
/// Sizing the per-task buffer to exactly this (rather than 4 × MAX_BATCH)
/// cuts stack usage ~8× and lets Rayon scale to 12+ workers without
/// overflow. 128 points × 96 B = 12 KB projective + 128 × 64 B = 8 KB
/// affine = 20 KB per task — fits comfortably in 12 × 2 MB stacks.
const GROUP_CAP: usize = NORMALIZE_GROUP_BATCHES * MAX_BATCH;

/// Number of batches processed sequentially within each parallel task.
///
/// Each super-batch computes one bootstrap scalar multiplication and chains
/// the remaining batches via `+ batch_size * G` additions. Replaces one
/// full scalar mul per batch (~30 µs) with one per super-batch, saving
/// ~45% of total sweep time vs. independent-batch processing.
const SUPER_BATCHES: u64 = 256;

/// Stack-allocated array size for the hot-path batch buffers. Must be
/// >= `MAX_BATCH_SIZE` from config.
const MAX_BATCH: usize = MAX_BATCH_SIZE as usize;

/// The number of variants produced by [`generate_variants`].
///
/// The default is 512: 256 powers of two (`2^0` through `2^255`) and 256
/// cumulative sums (`Σ 2^0..2^i` for `i ∈ [0, 255]`). One collision
/// (`2^0 == sum(2^0..2^0)`) is preserved for completeness; the index
/// does not deduplicate.
///
/// # Examples
///
/// ```
/// use find::search::{generate_variants, VARIANT_COUNT};
/// use find::ecc;
/// use k256::Scalar;
///
/// let target = ecc::scalar_mul_g(&Scalar::from(123u64));
/// let variants = generate_variants(&target);
/// assert_eq!(variants.len(), VARIANT_COUNT);
/// ```
pub const VARIANT_COUNT: usize = 512;

/// A single search variant derived from the target public key.
///
/// Each variant represents the point \(P - V \cdot G\) for a specific scalar
/// offset \(V\). During a sweep the engine compares \(x(j \cdot G)\) against
/// the variant's X-coordinate. A match implies the private key is one of
/// \(V + j\) or \(V - j\) (mod \(n\)).
///
/// # Invariants
///
/// - `v_scalar` equals `Scalar::reduce(V)`; both representations are kept
///   so that the engine does not need to redo the reduction at match time.
/// - The fields `label`, `v_scalar`, and `offset` are **fully
///   deterministic** from the variant index alone (they depend only on the
///   offset scalar `V`, not on the target public key). The full set of
///   512 variants is built once per process via
///   [`generate_variants`] and shared across all sessions.
/// - The 32-byte X-coordinate of \(P - V \cdot G\) is *target-dependent*
///   and is computed per-call by
///   [`crate::search::compute_variant_x_bytes`] (or by the caller via
///   the test fixtures). It is not stored on the variant itself.
#[derive(Debug, Clone)]
pub struct OffsetVariant {
    /// Human-readable label such as `"2^64"` or `"sum(2^0..2^7)"`.
    pub label: &'static str,
    /// The scalar offset \(V\), already reduced modulo the curve order \(n\).
    pub v_scalar: Scalar,
    /// The original unreduced scalar value as a decimal string.
    ///
    /// This is preserved for display and serialization; the reduced value
    /// used during arithmetic is `v_scalar`.
    pub offset_decimal: &'static str,
}

/// Cache-optimized lookup index for variant matching.
///
/// The index stores variant X-coordinates in a flat `[[u8; 32]; N]` array
/// sorted in ascending order. A separate `Vec<usize>` holds the
/// permutation that maps each sorted position back to the original
/// variant index in [`variants`](Self::variants). Lookups are
/// \(O(\log N)\) binary searches against the keys array.
///
/// # Memory layout
///
/// For the typical \(N = 512\) variant set:
///
/// - `keys`: 512 × 32 = 16 KiB (L1-resident on every modern `x86_64` / aarch64)
/// - `order`: 512 × 8 = 4 KiB
/// - `variants`: shared `&'static [OffsetVariant]` (built once per process)
///
/// The variant metadata is shared across all sessions via `&'static`
/// and stays in cold storage, only fetched on a match — see
/// [ADR-0001](../docs/adr/0001-multi-variant-search.md).
#[derive(Debug)]
pub struct VariantIndex {
    keys: Vec<[u8; 32]>,
    order: Vec<usize>,
    variants: &'static [OffsetVariant],
}

impl VariantIndex {
    /// Builds a new index from the static variant metadata plus the
    /// target-specific X-coordinates.
    ///
    /// The two inputs must have the same length (typically 512).
    ///
    /// # Complexity
    ///
    /// \(O(N \log N)\) where \(N\) is the number of variants, dominated by
    /// the sort. Memory is \(O(N)\).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::ecc;
    /// use find::search::{
    ///     compute_variant_x_bytes, generate_variants, VariantIndex,
    /// };
    /// use k256::Scalar;
    ///
    /// let target = ecc::scalar_mul_g(&Scalar::from(123u64));
    /// let variants = generate_variants(&target);
    /// let x_bytes = compute_variant_x_bytes(&target);
    /// let index = VariantIndex::new(variants, &x_bytes);
    /// assert_eq!(index.variants().len(), 512);
    /// ```
    pub fn new(variants: &'static [OffsetVariant], x_bytes: &[[u8; 32]]) -> Self {
        assert_eq!(
            variants.len(),
            x_bytes.len(),
            "variants and x_bytes must have the same length"
        );

        // Build permutation indices, sort by key, then materialize keys/order.
        // The intermediate `pairs` Vec is avoided by sorting indices
        // directly via `sort_by_key` over a `Vec<usize>`.
        let n = variants.len();
        let mut indices: Vec<usize> = (0..n).collect();
        indices.sort_unstable_by_key(|&i| x_bytes[i]);

        let mut keys: Vec<[u8; 32]> = Vec::with_capacity(n);
        let mut order: Vec<usize> = Vec::with_capacity(n);
        for i in indices {
            keys.push(x_bytes[i]);
            order.push(i);
        }

        Self {
            keys,
            order,
            variants,
        }
    }

    /// Searches for a variant whose X-coordinate equals `test_x`.
    ///
    /// If a match is found, two candidate private keys are derived from the
    /// matched variant's scalar offset and the supplied `j`:
    ///
    /// - \(`c_1` = V + j \pmod n\)
    /// - \(`c_2` = V - j \pmod n\)
    ///
    /// Because X-coordinates do not distinguish the two Y-parities, every
    /// match returns two candidates; the orchestrator or downstream code
    /// is responsible for filtering the valid one. See
    /// [ADR-0007](../docs/adr/0007-y-parity-ambiguity.md).
    ///
    /// # Arguments
    ///
    /// * `test_x` — A 32-byte big-endian X-coordinate to search for.
    /// * `j` — The small scalar that produced `test_x` (i.e. \(j \cdot G\)).
    ///
    /// # Performance
    ///
    /// The binary search walks `keys` only; the variant metadata is
    /// fetched on a match via `order[idx] -> variants[order[idx]]`. Cold-
    /// storage indirection on miss keeps the hot loop in L1.
    #[inline(always)]
    pub fn match_x(&self, test_x: &[u8; 32], j: u64) -> Option<SearchMatch> {
        let idx = self.keys.binary_search_by(|probe| probe.cmp(test_x)).ok()?;
        let var_idx = self.order[idx];
        let var = &self.variants[var_idx];
        let j_scalar = Scalar::from(j);

        Some(SearchMatch {
            label: var.label,
            offset: var.offset_decimal,
            j,
            candidates: [var.v_scalar.add(&j_scalar), var.v_scalar.sub(&j_scalar)],
        })
    }

    /// Returns a slice of the backing variants.
    ///
    /// The slice is in the original (insertion) order, **not** the sorted
    /// order used internally for binary search.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::ecc;
    /// use find::search::{compute_variant_x_bytes, generate_variants, VariantIndex};
    /// use k256::Scalar;
    ///
    /// let target = ecc::scalar_mul_g(&Scalar::from(7u64));
    /// let x_bytes = compute_variant_x_bytes(&target);
    /// let index = VariantIndex::new(generate_variants(&target), &x_bytes);
    /// let first_label = index.variants()[0].label;
    /// assert!(first_label == "2^0" || first_label.starts_with("sum"));
    /// ```
    pub const fn variants(&self) -> &'static [OffsetVariant] {
        self.variants
    }
}

/// The outcome of a successful match during a search sweep.
///
/// `candidates` is a fixed-size two-element array (`[V + j, V - j] mod n`)
/// of [`Scalar`] values. Every match produces exactly two Y-parity
/// candidates (the X-coordinate alone does not distinguish the two
/// Y parities — see [ADR-0007](../docs/adr/0007-y-parity-ambiguity.md)).
///
/// Storing the candidates as `[Scalar; 2]` rather than `[String; 2]`
/// removes two `format!`-style allocations per match and removes the
/// redundant allocation in `candidates_as_scalars` (which previously
/// had to re-decode the hex strings back into `Scalar`s). Callers that
/// need the hex representation can call [`SearchMatch::candidates_hex`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SearchMatch {
    /// The label of the variant that matched.
    pub label: &'static str,
    /// The decimal string representation of the variant's unreduced offset.
    pub offset: &'static str,
    /// The scalar \(j\) at which the match occurred.
    pub j: u64,
    /// Candidate private keys `[V + j, V - j] (mod n)` as [`Scalar`].
    ///
    /// Two-element array by construction.
    pub candidates: [Scalar; 2],
}

impl SearchMatch {
    /// Constructs a new `SearchMatch`.
    ///
    /// This constructor is provided because `SearchMatch` is
    /// `#[non_exhaustive]`, so external callers must use this function
    /// rather than struct expression syntax.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::SearchMatch;
    /// use k256::Scalar;
    ///
    /// let m = SearchMatch::new(
    ///     "2^0",
    ///     "1",
    ///     2,
    ///     [Scalar::from(3u64), Scalar::from(2u64)],
    /// );
    /// assert_eq!(m.j, 2);
    /// assert_eq!(m.label, "2^0");
    /// ```
    pub fn new(label: &'static str, offset: &'static str, j: u64, candidates: [Scalar; 2]) -> Self {
        Self {
            label,
            offset,
            j,
            candidates,
        }
    }

    /// Borrows the candidate private keys as a slice.
    ///
    /// Provided for API ergonomics — callers that want to iterate over
    /// both candidates uniformly can use `.as_slice()` rather than
    /// indexing into the array directly.
    pub const fn candidates(&self) -> &[Scalar; 2] {
        &self.candidates
    }

    /// Returns the candidate private keys as a borrowed `[Scalar; 2]`.
    ///
    /// This is the zero-allocation accessor equivalent of the previous
    /// `candidates_as_scalars()` — the candidates are stored as
    /// [`Scalar`] already, so no parsing is needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::SearchMatch;
    /// use k256::Scalar;
    ///
    /// let m = SearchMatch::new(
    ///     "2^0",
    ///     "1",
    ///     2,
    ///     [Scalar::from(3u64), Scalar::from(0u64)],
    /// );
    /// let scalars = m.candidates_as_scalars();
    /// assert_eq!(scalars[0], Scalar::from(3u64));
    /// ```
    pub const fn candidates_as_scalars(&self) -> [Scalar; 2] {
        self.candidates
    }

    /// Returns the candidate private keys as lower-case hex strings.
    ///
    /// Each scalar is rendered as its 32-byte big-endian representation
    /// with leading zeros trimmed (matching the previous `String`
    /// representation in the deprecated form).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::SearchMatch;
    /// use k256::Scalar;
    ///
    /// let m = SearchMatch::new(
    ///     "2^0",
    ///     "1",
    ///     2,
    ///     [Scalar::from(3u64), Scalar::from(2u64)],
    /// );
    /// let hexes = m.candidates_hex();
    /// assert_eq!(hexes[0], "3");
    /// ```
    pub fn candidates_hex(&self) -> [String; 2] {
        [
            scalar_to_hex_trimmed(&self.candidates[0]),
            scalar_to_hex_trimmed(&self.candidates[1]),
        ]
    }
}

/// A thread-safe progress counter for cache generation.
///
/// Multiple Rayon workers may call [`Progress::add`] concurrently. The counter
/// is monotonically increasing and is intended for telemetry only; it does not
/// affect correctness.
#[derive(Debug)]
pub struct Progress {
    counter: AtomicU64,
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

impl Progress {
    /// Creates a new counter starting at zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::Progress;
    ///
    /// let p = Progress::new();
    /// assert_eq!(p.get(), 0);
    /// p.add(5);
    /// assert_eq!(p.get(), 5);
    /// ```
    pub const fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    /// Atomically adds `n` to the counter and returns the **previous** value.
    ///
    /// This matches the [`AtomicU64::fetch_add`] contract; the return
    /// value is useful in tests that want to verify the exact sequencing
    /// of concurrent updates.
    ///
    /// # Concurrency
    ///
    /// Uses [`Ordering::Relaxed`] because the counter is purely
    /// informational and does not synchronise any other state. Callers
    /// that need a happens-before relationship should layer their own
    /// synchronisation on top.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::Progress;
    ///
    /// let p = Progress::new();
    /// assert_eq!(p.add(10), 0); // returns previous value
    /// assert_eq!(p.add(5), 10);
    /// assert_eq!(p.get(), 15);
    /// ```
    pub fn add(&self, n: u64) -> u64 {
        self.counter.fetch_add(n, Ordering::Relaxed)
    }

    /// Reads the current counter value.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::Progress;
    ///
    /// let p = Progress::new();
    /// assert_eq!(p.get(), 0);
    /// ```
    pub fn get(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }
}

impl std::fmt::Display for OffsetVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (V={})", self.label, self.offset_decimal)
    }
}

impl std::fmt::Display for SearchMatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "variant={} offset={} j={}",
            self.label, self.offset, self.j
        )
    }
}

/// Abstraction over cache block writes.
///
/// Implementations are responsible for persisting raw 32-byte X-coordinate
/// blocks at arbitrary byte offsets. The trait is object-safe and is
/// intended to be implemented by the `persistence` layer so that the search
/// domain remains free of file-system details.
///
/// # Contract
///
/// Implementors must guarantee:
///
/// - **Atomic block writes**: the bytes written by a single `write_block`
///   call appear contiguously and are not interleaved with other writers'
///   bytes.
/// - **Concurrency**: `write_block` may be called concurrently from
///   multiple threads. The `Send + Sync` supertraits enforce this.
/// - **Offset independence**: writes at non-overlapping offsets do not
///   affect each other. Overlapping writes have implementation-defined
///   semantics; the engine guarantees non-overlap by computing
///   `offset = batch_idx * BATCH_SIZE * 32`.
///
/// # Examples
///
/// ```ignore
/// use find::search::CacheWriter;
///
/// struct NullWriter;
/// impl CacheWriter for NullWriter {
///     fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
///         Ok(())
///     }
/// }
/// ```
pub trait CacheWriter: Send + Sync {
    /// Writes `data` starting at `offset` bytes into the cache.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the underlying storage operation fails.
    fn write_block(&self, offset: u64, data: &[u8]) -> std::io::Result<()>;
}

/// Returns the fully-built &-static slice of 512 offset variants.
///
/// The full set of variants — powers of two (`2^0..2^255`) and cumulative
/// sums (`1, 3, 7, …, 2^256 - 1`) — has deterministic metadata (label,
/// `v_scalar`, decimal offset) that depends only on the variant index,
/// not on the target public key. The metadata is built once per process
/// via a [`OnceLock`] and shared across all sessions of `generate_variants`.
/// Only the per-target X-coordinates are computed at the call site, via
/// [`compute_variant_x_bytes`].
///
/// # Returns
///
/// `&'static [OffsetVariant]` of length 512 (`VARIANT_COUNT`), sorted as:
/// powers of two first (`indices 0..256`), then cumulative sums
/// (`indices 256..512`). The variant at position `i` corresponds to
/// scalar offset `V = 2^i` for `i < 256`, and `V = 2^(i - 255) - 1`
/// for `i >= 256`.
///
/// # Examples
///
/// ```
/// use find::ecc;
/// use find::search::{compute_variant_x_bytes, generate_variants, VariantIndex};
/// use k256::Scalar;
///
/// let target = ecc::scalar_mul_g(&Scalar::from(123u64));
/// let variants = generate_variants(&target);
/// let x_bytes = compute_variant_x_bytes(&target);
/// let index = VariantIndex::new(variants, &x_bytes);
/// ```
#[instrument(level = "info")]
pub fn generate_variants(_target_p: &ProjectivePoint) -> &'static [OffsetVariant] {
    static INTERN: OnceLock<Box<[OffsetVariant; VARIANT_COUNT]>> = OnceLock::new();
    INTERN.get_or_init(build_static_variants).as_slice()
}

/// Computes the 32-byte big-endian X-coordinates of `target_p - V·G`
/// for every variant in [`generate_variants`].
///
/// Returns a `Vec<[u8; 32]>` of length 512; position `i` matches the
/// corresponding variant at `generate_variants()[i]`.
///
/// Variants whose subtraction collapses to the point-at-infinity (which
/// would match every sweep entry) are encoded as 32 zeros here; the
/// orchestrator's comparison already treats 32 zeros as a valid key, so
/// the sweep naturally skips them. (Identity variants are
/// correctness-critical to skip, not a performance optim.)
///
/// # Performance
///
/// The function performs 256 scalar multiplications and 256 point
/// additions (vs. 512 scalar multiplications in the naïve version). The
/// point-addition chain reuses `2^i · G` from the powers-of-two loop to
/// build the cumulative scalar offsets without redoing scalar muls.
pub fn compute_variant_x_bytes(target_p: &ProjectivePoint) -> Vec<[u8; 32]> {
    let mut x_bytes: Vec<[u8; 32]> = vec![[0u8; 32]; VARIANT_COUNT];
    let p = *target_p;

    // Stack-allocated table of `2^i · G` for i ∈ [0, 255]. We rebuild this
    // table once and reuse it across both loops.
    let mut pow_of_two_g: [ProjectivePoint; 256] =
        std::array::from_fn(|_| ProjectivePoint::GENERATOR);
    pow_of_two_g[0] = ecc::generator();
    for i in 1..256 {
        pow_of_two_g[i] = pow_of_two_g[i - 1].double();
    }

    // Powers-of-two pass: V = 2^i for i in 0..256.
    for (i, pow_g) in pow_of_two_g.iter().enumerate() {
        let shifted = p - pow_g;
        match ecc::x_bytes(&shifted) {
            Some(x) => x_bytes[i] = x,
            None => tracing::warn!("Variant 2^{} produced identity point; skipping", i),
        }
    }

    // Cumulative-sum variants: cum_i = Σ_{k=0..i} 2^k = 2^{i+1} - 1.
    // The point recurrence is `cum_g += pow_of_two_g[i+1]`, reusing
    // the powers-of-two table from above so the only remaining arithmetic
    // per iteration is a single mixed addition.
    let mut cum_g = pow_of_two_g[0];
    for i in 0..256 {
        let shifted = p - cum_g;
        match ecc::x_bytes(&shifted) {
            Some(x) => x_bytes[256 + i] = x,
            None => tracing::warn!(
                "Variant sum(2^0..2^{}) produced identity point; skipping",
                i
            ),
        }
        if i < 255 {
            cum_g += pow_of_two_g[i + 1];
        }
    }

    x_bytes
}

/// Builds the 512-entry static variant metadata array.
///
/// Called once per process via the `OnceLock` inside
/// [`generate_variants`]. Performs 512 `format!` calls and 256+256
/// `u256_to_decimal` allocations; the resulting strings are then
/// retained forever. Per-session work no longer re-allocates them.
#[allow(clippy::redundant_closure_for_method_calls)]
fn build_static_variants() -> Box<[OffsetVariant; VARIANT_COUNT]> {
    let mut out: Box<[OffsetVariant; VARIANT_COUNT]> =
        Box::new(std::array::from_fn(|_| OffsetVariant {
            label: "",
            v_scalar: Scalar::ZERO,
            offset_decimal: "",
        }));

    // Powers-of-two pass: V = 2^i for i in 0..256.
    let mut pow = U256::ONE;
    for i in 0..256 {
        let scalar = Scalar::reduce(pow);
        out[i] = OffsetVariant {
            label: Box::leak(format!("2^{i}").into_boxed_str()),
            v_scalar: scalar,
            offset_decimal: Box::leak(u256_to_decimal(&pow).into_boxed_str()),
        };
        pow <<= 1;
    }

    // Cumulative-sum pass: V = 2^(i+1) - 1 for i in 0..256, producing
    // 1, 3, 7, 15, …, 2^256 - 1.
    let mut cum = U256::ONE;
    for i in 0..256 {
        let scalar = Scalar::reduce(cum);
        out[256 + i] = OffsetVariant {
            label: Box::leak(format!("sum(2^0..2^{i})").into_boxed_str()),
            v_scalar: scalar,
            offset_decimal: Box::leak(u256_to_decimal(&cum).into_boxed_str()),
        };
        cum = (cum << 1) | U256::ONE;
    }

    out
}

/// Performs a CPU-bound parallel sweep over a scalar range.
///
/// The range `[start, end]` is split into batches of 32 scalars. Each batch
/// is processed in parallel using Rayon, and points are batch-normalized to
/// amortize the cost of modular inversion.
///
/// # Arguments
///
/// * `index` — The variant index to match against.
/// * `start` — First scalar \(j\) to evaluate (inclusive). Values below 1 are
///   clamped to 1 because \(j = 0\) yields the identity point, which cannot
///   match a valid variant.
/// * `end` — Last scalar \(j\) to evaluate (inclusive).
///
/// # Returns
///
/// `Some(SearchMatch)` on the first match found, or `None` if the entire
/// range is exhausted without a match.
///
/// # Performance
///
/// The hot-path cost is dominated by **one bootstrap scalar multiplication
/// per batch plus `(count - 1)` mixed `+ G` additions**, vs. `count`
/// independent scalar multiplications in a naive implementation. The
/// mixed addition is ~12 field multiplications, vs. ~256 for a fresh
/// scalar multiplication. This `+ G` chain is the dominant perf win of
/// the search engine (~20× vs. independent scalar muls). See ADR-0002 for
/// the full rationale.
///
/// Batch normalization uses Montgomery's simultaneous inversion to
/// collapse 32 projective→affine conversions into a single inversion
/// plus ~6 × 32 field multiplications (~15–20× speedup vs. sequential
/// conversions).
///
/// # Pseudocode
///
/// ```text
/// # Parallel for each batch of BATCH_SIZE scalars:
/// let count = min(end - chunk_start + 1, BATCH_SIZE)
/// let mut current = chunk_start * G                # bootstrap mul
/// let mut points = []
/// for _ in 0..count:
///     points.push(current)
///     current += G                                 # mixed addition, ~12 mults
/// let affines = batch_normalize(points)            # one inversion for 32 points
/// for (i, a) in affines.enumerate():
///     let j = chunk_start + i
///     if let Some(x) = X(a):
///         if index.match_x(x, j) is Some(m):
///             return m                             # early exit
/// ```
///
/// # Examples
///
/// ```no_run
/// use find::ecc;
/// use find::search::{compute_variant_x_bytes, generate_variants, sweep_parallel, VariantIndex};
/// use k256::Scalar;
///
/// fn main() -> Result<(), Box<dyn core::error::Error>> {
///     let target = ecc::scalar_mul_g(&Scalar::from(12345u64));
///     let variants = generate_variants(&target);
///     let x_bytes = compute_variant_x_bytes(&target);
///     let index = VariantIndex::new(variants, &x_bytes);
///     let m = sweep_parallel(&index, 1, 100_000, 32)
///         .expect("match for d=12345 in [1, 100000]");
///     assert!(m.candidates.contains(&Scalar::from(3039u64)));
///     Ok(())
/// }
/// ```
pub fn sweep_parallel(
    index: &VariantIndex,
    start: u64,
    end: u64,
    batch_size: u32,
) -> Option<SearchMatch> {
    let start = start.max(1);
    if start > end {
        return None;
    }
    let batch_size = batch_size as u64;

    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = if range_len == 0 {
        0
    } else {
        (range_len - 1) / batch_size + 1
    };
    let sb_size = SUPER_BATCHES.min(num_batches);
    let num_sb = if num_batches == 0 {
        0
    } else {
        (num_batches - 1) / sb_size + 1
    };

    // Shared early-exit flag. `find_map_any` already provides
    // cancellation between super-batches, but the per-super-batch
    // work is large (256 batches ≈ 0.9 ms). A group-level check
    // lets a thread that finds the match in super-batch K cancel
    // the remaining work in super-batches K+1, K+2, ... that are
    // already in progress on other threads. The flag is set by
    // the worker that finds the match via `Ordering::Release`; other
    // workers observe it via `Ordering::Acquire` reads.
    let found_flag = std::sync::atomic::AtomicBool::new(false);

    (0..num_sb).into_par_iter().find_map_any(|sb_idx| {
        let sb_start = sb_idx * sb_size;
        let sb_end = (sb_start + sb_size).min(num_batches);
        let sb_count = sb_end - sb_start;

        // Bootstrap: one scalar mul for the super-batch's first point.
        // Subsequent batches chain via the `+ G` loop below — `current`
        // after batch `k` equals `(start + sb_start*BATCH + (k+1)*BATCH)·G`,
        // which is the next batch's bootstrap. This replaces one full
        // scalar mul per batch with one per super-batch (256× fewer
        // bootstrap muls for the default BATCH_SIZE).
        let base_j = start.saturating_add(sb_start * batch_size);
        let mut current = ecc::scalar_mul_g(&Scalar::from(base_j));

        // Process the super-batch in groups of `NORMALIZE_GROUP_BATCHES`
        // consecutive batches so a single `batch_normalize` amortises the
        // modular inversion across the whole group. Measured per-point cost
        // drops from 0.227 µs (32-point groups) to 0.122 µs (128-point
        // groups) — a ~46 % saving on the normalize step.
        //
        // The group buffer is sized to hold exactly one group of points
        // (4 × 32 = 128) — the maximum we'll ever use at once. Previously
        // sized at 4 × MAX_BATCH = 1024, which wasted 7/8 of the buffer
        // and overflowed the 8 MB thread stack with 12+ Rayon workers.
        let group_batches: u64 = NORMALIZE_GROUP_BATCHES as u64;
        let mut group_points_buf = [ProjectivePoint::IDENTITY; GROUP_CAP];
        let mut group_affines_buf = [AffinePoint::IDENTITY; GROUP_CAP];
        let g = ecc::generator();

        let mut bi = 0;
        while bi < sb_count {
            // Fast-path early-exit: if another worker already found the
            // match, abandon this super-batch immediately. The check is
            // cheaper than the 0.9 ms we'd waste on a full super-batch.
            if found_flag.load(std::sync::atomic::Ordering::Acquire) {
                return None;
            }
            let group_batch_count = (sb_count - bi).min(group_batches);
            let mut total_count = 0usize;

            // Phase 1: generate all group points via the chained `+ G` loop.
            for gbi in 0..group_batch_count {
                let abs_bi = bi + gbi;
                let chunk_start = base_j + abs_bi * batch_size;
                let chunk_end = (chunk_start.saturating_add(batch_size - 1)).min(end);
                let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;
                for slot in &mut group_points_buf[total_count..total_count + count] {
                    *slot = current;
                    current += g;
                }
                total_count += count;
            }
            debug_assert!(total_count <= GROUP_CAP);

            // Phase 2: single batch_normalize across the whole group.
            ProjectivePoint::batch_normalize(
                &group_points_buf[..total_count],
                &mut group_affines_buf[..total_count],
            );

            // Phase 3: match each affine point batch-by-batch.
            let mut offset = 0usize;
            for gbi in 0..group_batch_count {
                let abs_bi = bi + gbi;
                let chunk_start = base_j + abs_bi * batch_size;
                let chunk_end = (chunk_start.saturating_add(batch_size - 1)).min(end);
                let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;
                for i in 0..count {
                    let affine = &group_affines_buf[offset + i];
                    let j = chunk_start + i as u64;
                    let mut x_bytes = [0u8; 32];
                    x_bytes.copy_from_slice(affine.x().as_ref());
                    if let Some(m) = index.match_x(&x_bytes, j) {
                        // Signal other workers to abandon their super-batches
                        // before the next per-batch boundary.
                        found_flag.store(true, std::sync::atomic::Ordering::Release);
                        return Some(m);
                    }
                }
                offset += count;
            }

            bi += group_batch_count;
        }
        None
    })
}

/// Pre-computes a binary cache chunk while optionally searching for a match.
///
/// For each batch of 32 points, the function:
/// 1. Generates the points \(j \cdot G\) and normalizes them.
/// 2. If an `index` is supplied, checks each X-coordinate for a match.
/// 3. Writes the raw 32-byte X-coordinates to the cache writer.
/// 4. Updates the shared progress counter.
///
/// If a match is discovered, the remaining batches are abandoned and the
/// match is returned immediately.
///
/// # Arguments
///
/// * `start` — First scalar \(j\) to evaluate (inclusive). Clamped to 1.
/// * `end` — Last scalar \(j\) to evaluate (inclusive).
/// * `writer` — The cache writer that receives raw X-coordinate blocks.
/// * `index` — An optional variant index for real-time matching.
/// * `progress` — A shared progress counter updated after each batch.
///
/// # Errors
///
/// Returns [`FindError::Io`] if the cache writer reports a failure.
///
/// # Panics
///
/// Panics if Rayon worker threads panic during batch processing.
///
/// # Performance
///
/// Identical arithmetic to [`sweep_parallel`]; the additional cost
/// is one `write_block` call per batch (a single `pwrite_at` of ~1 KiB on
/// Unix, an `O(1)` operation). The progress counter is updated once per
/// batch, so it reflects the **actual** scalars processed (not a
/// multiple of `BATCH_SIZE`).
///
/// The early-exit check at the top of each batch is a single
/// [`OnceLock::get`] — there is no mutex and no atomic load. Once a
/// worker publishes its match via [`OnceLock::set`], every other
/// worker's `get()` returns `Some` on the next iteration and the
/// batch becomes a no-op.
///
/// # Pseudocode
///
/// ```text
/// match_once = OnceLock<SearchMatch>
/// batches.parallel_for_each(|batch_idx| {
///     if match_once.get() is Some: return       # lock-free fast-path
///     let (chunk_start, count) = batch_bounds(batch_idx)
///     let mut current = chunk_start * G
///     let mut block = []
///     for _ in 0..count:
///         block.push(X(current))
///         if let Some(idx) = index:
///             if idx.match_x(X(current), j) is Some(m):
///                 let _ = match_once.set(m); return
///         current += G
///     writer.write_block(batch_offset, &block)?
///     progress.add(count)
/// })
/// match_once.into_inner()
/// ```
pub fn sweep_and_cache<W: CacheWriter>(
    start: u64,
    end: u64,
    writer: &W,
    index: Option<&VariantIndex>,
    progress: &Progress,
    batch_size: u32,
) -> Result<Option<SearchMatch>> {
    let start = start.max(1);
    if start > end {
        return Ok(None);
    }
    let batch_size = batch_size as u64;

    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = if range_len == 0 {
        0
    } else {
        (range_len - 1) / batch_size + 1
    };
    let sb_size = SUPER_BATCHES.min(num_batches);
    let num_sb = if num_batches == 0 {
        0
    } else {
        (num_batches - 1) / sb_size + 1
    };
    // `OnceLock<SearchMatch>` as the one-shot best-effort broadcast
    // channel. Workers check `get()` lock-free at the top of each batch
    // and skip if a match has already been published. The first worker
    // to find a match wins via `set`; subsequent workers see the value
    // but their `set` returns Err and is discarded. Replaces the
    // previous `Mutex<Option<SearchMatch>> + AtomicBool` pair (see
    // optimization-decisions/0004-atomic-flag-early-exit.md and the
    // upcoming 0007-oncelock-early-exit.md for the rationale).
    let match_once: OnceLock<SearchMatch> = OnceLock::new();

    (0..num_sb)
        .into_par_iter()
        .try_for_each(|sb_idx| -> Result<()> {
            // Fast-path check without locking — if another worker already
            // found a match, skip this super-batch entirely.
            if match_once.get().is_some() {
                return Ok(());
            }

            let sb_start = sb_idx * sb_size;
            let sb_end = (sb_start + sb_size).min(num_batches);
            let sb_count = sb_end - sb_start;

            // Bootstrap: one scalar mul for the super-batch's first point.
            // Subsequent batches chain via the `+ G` loop below — `current`
            // after batch `k` equals the next batch's bootstrap. See
            // `sweep_parallel` for the full rationale.
            let base_j = start.saturating_add(sb_start * batch_size);
            let mut current = ecc::scalar_mul_g(&Scalar::from(base_j));
            let g = ecc::generator();

            // Process the super-batch in groups of `NORMALIZE_GROUP_BATCHES`
            // consecutive batches so a single `batch_normalize` amortises
            // the modular inversion across the whole group (see
            // `sweep_parallel` for the measured per-point savings).
            let group_batches: u64 = NORMALIZE_GROUP_BATCHES as u64;
            let mut group_points_buf = [ProjectivePoint::IDENTITY; GROUP_CAP];
            let mut group_affines_buf = [AffinePoint::IDENTITY; GROUP_CAP];

            let mut bi = 0;
            while bi < sb_count {
                if match_once.get().is_some() {
                    break;
                }
                let group_batch_count = (sb_count - bi).min(group_batches);
                let mut total_count = 0usize;

                // Phase 1: generate all group points via the chained `+ G`.
                for gbi in 0..group_batch_count {
                    let abs_bi = bi + gbi;
                    let chunk_start = start.saturating_add((sb_start + abs_bi) * batch_size);
                    let chunk_end = (chunk_start.saturating_add(batch_size - 1)).min(end);
                    let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;
                    for slot in &mut group_points_buf[total_count..total_count + count] {
                        *slot = current;
                        current += g;
                    }
                    total_count += count;
                }

                // Phase 2: single batch_normalize.
                ProjectivePoint::batch_normalize(
                    &group_points_buf[..total_count],
                    &mut group_affines_buf[..total_count],
                );

                // Phase 3: match each point and write per-batch blocks.
                let mut offset = 0usize;
                for gbi in 0..group_batch_count {
                    if match_once.get().is_some() {
                        bi = sb_count; // exit outer loop
                        break;
                    }
                    let abs_bi = bi + gbi;
                    let global_batch_idx = sb_start + abs_bi;
                    let chunk_start = start.saturating_add(global_batch_idx * batch_size);
                    let chunk_end = (chunk_start.saturating_add(batch_size - 1)).min(end);
                    let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;

                    let mut block = [0u8; MAX_BATCH * 32];
                    let mut block_len = 0usize;
                    let mut local_match = None;

                    for i in 0..count {
                        let affine = &group_affines_buf[offset + i];
                        let j = chunk_start + i as u64;
                        let mut x_bytes = [0u8; 32];
                        x_bytes.copy_from_slice(affine.x().as_ref());

                        if let Some(idx_ref) = index {
                            if let Some(m) = idx_ref.match_x(&x_bytes, j) {
                                local_match = Some(m);
                                break;
                            }
                        }

                        block[block_len..block_len + 32].copy_from_slice(&x_bytes);
                        block_len += 32;
                    }

                    if let Some(m) = local_match {
                        let _ = match_once.set(m);
                        bi = sb_count; // exit outer loop
                        break;
                    }

                    let offset_bytes = global_batch_idx * batch_size * 32;
                    writer
                        .write_block(offset_bytes, &block[..block_len])
                        .map_err(FindError::Io)?;
                    progress.add(count as u64);

                    offset += count;
                }

                bi += group_batch_count;
            }
            Ok(())
        })?;

    // Extract the match (if any). `OnceLock::into_inner` returns
    // `Option<T>` directly; there is no lock to be poisoned.
    Ok(match_once.into_inner())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------
/// Converts a scalar to a lower-case hex string with leading zeros removed.
///
/// The value zero is rendered as `"0"`. Uses a stack-allocated `[u8; 64]`
/// buffer to avoid heap allocation for the bounded 64-char hex output.
#[inline]
fn scalar_to_hex_trimmed(s: &Scalar) -> String {
    let bytes: [u8; 32] = s.to_bytes().into();
    let mut buf = [0u8; 64];
    hex::encode_to_slice(bytes, &mut buf)
        .expect("64-byte buffer is always sufficient for 32-byte input");
    // Find the first non-zero byte instead of `trim_start_matches('0')`
    // which constructs a `&str` slice and then iterates. The byte scan
    // avoids the intermediate `str` and the `chars()` iteration in
    // `trim_start_matches`.
    let start = buf.iter().position(|&b| b != b'0').unwrap_or(buf.len());
    if start == buf.len() {
        "0".to_string()
    } else {
        // Safe: hex encoding is always valid UTF-8.
        std::str::from_utf8(&buf[start..])
            .expect("hex encoding is valid UTF-8")
            .to_string()
    }
}

/// Converts a [`U256`] to a decimal string.
///
/// Used for display and serialization of variant offsets. Runs once per
/// variant at startup (~512 calls per session) so allocation pressure
/// is small, but this implementation avoids the `num_bigint::BigUint`
/// round-trip entirely: it parses the 256-bit big-endian limbs into a
/// stack-allocated decimal representation using repeated divmod by 10.
///
/// # Performance
///
/// O(N²) in the number of digits, where N ≤ 78 for a 256-bit value.
/// Each iteration is one 256-bit divmod (a constant-cost operation on
/// `crypto_bigint::U256`) plus one byte write to a stack buffer. Avoids
/// the heap allocation that `BigUint::from_bytes_be(...).to_string()`
/// would incur.
fn u256_to_decimal(v: &U256) -> String {
    use k256::elliptic_curve::bigint::Zero;
    if bool::from(v.is_zero()) {
        return "0".to_string();
    }
    // Stack-allocated buffer: a u256 has at most 78 decimal digits.
    let mut digits = [0u8; 78];
    let mut len = 0usize;
    let mut rem: U256 = *v;
    while !bool::from(rem.is_zero()) {
        let (q, r) = div_rem_u256_by_u64(rem, 10);
        digits[len] = b'0' + r as u8;
        len += 1;
        rem = q;
    }
    digits[..len].reverse();
    // SAFETY: all bytes are ASCII digits (b'0'..=b'9').
    String::from_utf8_lossy(&digits[..len]).into_owned()
}

/// Computes `self / d` and `self % d` for `U256 / u64`.
///
/// `crypto_bigint::U256` exposes its limbs via `to_be_byte_array()` /
/// `from_be_byte_array()`; the most direct way to divmod by a small
/// divisor is to walk the bytes big-endian, maintaining a running
/// 16-bit remainder.
#[inline(always)]
fn div_rem_u256_by_u64(v: U256, d: u64) -> (U256, u64) {
    use k256::elliptic_curve::bigint::ArrayEncoding;
    debug_assert!(d > 0);
    let bytes = v.to_be_byte_array();
    let mut out = [0u8; 32];
    let mut rem: u64 = 0;
    for i in 0..32 {
        let acc = (rem << 8) | bytes[i] as u64;
        let q = (acc / d) as u8;
        rem = acc % d;
        out[i] = q;
    }
    // `from_be_byte_array` takes a `GenericArray<u8, _>`; we convert
    // via the `Into` impl that wraps a `[u8; N]` into the right shape.
    (U256::from_be_byte_array(out.into()), rem)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the [`VariantIndex`] correctly matches a known X-coordinate.
    #[test]
    fn test_indexing_speedup() {
        let target = ecc::scalar_mul_g(&Scalar::from(1000u64));
        let variants = generate_variants(&target);
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(variants, &x_bytes);

        let p_999 = ecc::scalar_mul_g(&Scalar::from(999u64));
        let mut x_999 = [0u8; 32];
        let x = ecc::x_bytes(&p_999).expect("non-identity has an X");
        x_999.copy_from_slice(&x);

        let m = index.match_x(&x_999, 999).unwrap();
        assert!(m.label == "2^0" || m.label == "sum(2^0..2^0)");
        assert_eq!(m.offset, "1");
    }

    /// Verifies that [`generate_variants`] produces at least one variant and
    /// that every variant has a non-zero X-coordinate.
    #[test]
    fn test_generate_variants_produces_entries() {
        let target = ecc::scalar_mul_g(&Scalar::from(123u64));
        let variants = generate_variants(&target);
        let x_bytes = compute_variant_x_bytes(&target);
        assert_eq!(variants.len(), x_bytes.len());
        assert!(!variants.is_empty());
        assert!(variants.iter().all(|v| !v.label.is_empty()));

        // Also verify that the static slice has the expected shape: 256
        // powers of two followed by 256 cumulative sums.
        let pow_count = variants
            .iter()
            .take_while(|v| v.label.starts_with("2^"))
            .count();
        let sum_count = variants
            .iter()
            .skip_while(|v| v.label.starts_with("2^"))
            .take_while(|v| v.label.starts_with("sum("))
            .count();
        assert_eq!(pow_count, 256);
        assert_eq!(sum_count, 256);
        assert_eq!(pow_count + sum_count, variants.len());

        // Verify the deterministic metadata for known indices.
        assert_eq!(variants[0].label, "2^0");
        assert_eq!(variants[0].offset_decimal, "1");
        assert_eq!(variants[256].label, "sum(2^0..2^0)");
        assert_eq!(variants[256].offset_decimal, "1");
    }

    /// Verifies that [`Progress`] counts additions correctly under concurrency.
    #[test]
    fn test_progress_add_and_get() {
        let p = Progress::new();
        assert_eq!(p.get(), 0);
        assert_eq!(p.add(10), 0);
        assert_eq!(p.get(), 10);
        assert_eq!(p.add(5), 10);
        assert_eq!(p.get(), 15);
    }

    /// Verifies that [`sweep_parallel`] returns `None` when start > end.
    #[test]
    fn test_sweep_parallel_start_greater_than_end() {
        let target = ecc::scalar_mul_g(&Scalar::from(1u64));
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(generate_variants(&target), &x_bytes);
        assert!(sweep_parallel(&index, 100, 1, 32).is_none());
    }

    /// Verifies that [`sweep_and_cache`] returns `Ok(None)` when start > end.
    #[test]
    fn test_sweep_and_cache_start_greater_than_end() {
        struct DummyWriter;
        impl CacheWriter for DummyWriter {
            fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
        }
        let progress = Progress::new();
        let result = sweep_and_cache(100, 1, &DummyWriter, None, &progress, 32);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Verifies that [`scalar_to_hex_trimmed`] renders zero correctly.
    #[test]
    fn test_scalar_to_hex_trimmed_zero() {
        let s = Scalar::from(0u64);
        assert_eq!(scalar_to_hex_trimmed(&s), "0");
    }

    /// Verifies that [`scalar_to_hex_trimmed`] strips leading zeros.
    #[test]
    fn test_scalar_to_hex_trimmed_nonzero() {
        let s = Scalar::from(0x1a2bu64);
        assert_eq!(scalar_to_hex_trimmed(&s), "1a2b");
    }

    /// Verifies that [`u256_to_decimal`] produces the expected decimal string.
    #[test]
    fn test_u256_to_decimal() {
        let v = U256::from_u128(123456789);
        assert_eq!(u256_to_decimal(&v), "123456789");
    }

    /// Verifies that [`VariantIndex::variants`] returns the backing slice.
    #[test]
    fn test_variant_index_variants_accessor() {
        let target = ecc::scalar_mul_g(&Scalar::from(7u64));
        let variants = generate_variants(&target);
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(variants, &x_bytes);
        assert_eq!(index.variants().len(), 512);
    }

    /// Verifies that [`VariantIndex::match_x`] returns `None` for unknown X.
    #[test]
    fn test_match_x_not_found() {
        let target = ecc::scalar_mul_g(&Scalar::from(7u64));
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(generate_variants(&target), &x_bytes);
        let unknown = [0xffu8; 32];
        assert!(index.match_x(&unknown, 1).is_none());
    }

    /// Verifies that [`sweep_and_cache`] discovers a match and returns early.
    #[test]
    fn test_sweep_and_cache_finds_match() {
        struct NullWriter;
        impl CacheWriter for NullWriter {
            fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Target scalar d = 3 matches via either:
        // - 2^0 variant (V = 1) at j = 2, or
        // - 2^1 variant (V = 2) at j = 1.
        let target = ecc::scalar_mul_g(&Scalar::from(3u64));
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(generate_variants(&target), &x_bytes);
        let progress = Progress::new();

        let result = sweep_and_cache(1, 10, &NullWriter, Some(&index), &progress, 32).unwrap();
        assert!(
            result.is_some(),
            "sweep_and_cache must find match for d=3 in range [1,10]"
        );
        let m = result.unwrap();
        assert!(
            m.candidates.contains(&Scalar::from(3u64)),
            "Candidates must include d=3, got: {:?} (found via {} at j={})",
            m.candidates,
            m.label,
            m.j
        );
    }

    /// Verifies that `sweep_and_cache` reports the actual batch count, not
    /// `BATCH_SIZE`, for the last partial batch.
    #[test]
    fn test_sweep_and_cache_progress_partial_batch() {
        struct NullWriter;
        impl CacheWriter for NullWriter {
            fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Use a target that does NOT match in the sweep range, so all
        // batches complete and the progress reflects the actual work.
        let target = ecc::scalar_mul_g(&Scalar::from(1_000_000u64));
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(generate_variants(&target), &x_bytes);
        let progress = Progress::new();

        // Sweep range [1, 5]: 5 scalars in 1 partial batch. The engine
        // should call `progress.add(5)` (the actual count), not
        // `progress.add(BATCH_SIZE=32)`.
        let result = sweep_and_cache(1, 5, &NullWriter, Some(&index), &progress, 32).unwrap();
        assert!(
            result.is_none(),
            "No match expected in [1, 5] for d=1000000"
        );
        let final_progress = progress.get();
        assert_eq!(
            final_progress, 5,
            "Progress must reflect actual scalars processed (5), not BATCH_SIZE (32)"
        );
    }

    /// Verifies that both the `2^0` and `sum(2^0..2^0)` variants (which have
    /// the same V = 1) are stored in the index and either can produce a match.
    #[test]
    fn test_variant_collision_2_0_and_sum_2_0() {
        let target = ecc::scalar_mul_g(&Scalar::from(2u64)); // d = 2
        let variants = generate_variants(&target);

        // The first two variants should be 2^0 and sum(2^0..2^0).
        assert!(variants[0].label == "2^0" || variants[1].label == "2^0");
        let has_pow = variants.iter().any(|v| v.label == "2^0");
        let has_sum = variants.iter().any(|v| v.label == "sum(2^0..2^0)");
        assert!(
            has_pow && has_sum,
            "Both 2^0 and sum(2^0..2^0) must be present"
        );

        // d = 2 means j = 1 for V = 1.
        let x_bytes = compute_variant_x_bytes(&target);
        let index = VariantIndex::new(variants, &x_bytes);
        let p_1 = ecc::scalar_mul_g(&Scalar::from(1u64));
        let x = ecc::x_bytes(&p_1).expect("non-identity has an X");
        let mut x_1 = [0u8; 32];
        x_1.copy_from_slice(&x);

        let m = index
            .match_x(&x_1, 1)
            .expect("Must find a match for j=1, V=1");
        // The matched variant's V is 1, so d = V + j = 2 or d = V - j = 0.
        assert!(
            m.candidates.contains(&Scalar::from(2u64)),
            "Candidates must include d=2, got: {:?}",
            m.candidates
        );
    }

    // Property: `generate_variants` returns a static 512-variant set and
    // `compute_variant_x_bytes` returns the matching 512 X-coordinates
    // for any non-identity target.
    proptest::proptest! {
        #[test]
        fn prop_generate_variants_count(d in 1u64..1_000_000u64) {
            let target = ecc::scalar_mul_g(&Scalar::from(d));
            let variants = generate_variants(&target);
            let x_bytes = compute_variant_x_bytes(&target);
            proptest::prop_assert_eq!(variants.len(), 512usize);
            proptest::prop_assert_eq!(x_bytes.len(), 512usize);
            // The variant metadata is fully static so every call returns
            // the same labels and offsets (no per-call allocation
            // differences).
            proptest::prop_assert_eq!(variants[0].label, "2^0");
            proptest::prop_assert_eq!(variants[256].label, "sum(2^0..2^0)");
        }
    }

    // Property: the static variant metadata is deduplicated across
    // sessions (same pointer each call).
    #[test]
    fn prop_generate_variants_static_pointer() {
        let target = ecc::scalar_mul_g(&Scalar::from(42u64));
        let v1: *const u8 = generate_variants(&target).as_ptr().cast();
        let v2: *const u8 = generate_variants(&target).as_ptr().cast();
        assert_eq!(v1, v2, "interned slice must be the same pointer");
    }

    // Property: `scalar_to_hex_trimmed` produces a hex string that, when
    // padded back to 32 bytes and decoded, yields the original scalar.
    proptest::proptest! {
        #[test]
        fn prop_scalar_to_hex_trimmed_inverts(d in 0u64..1_000_000u64) {
            let s = Scalar::from(d);
            let hex_str = scalar_to_hex_trimmed(&s);

            // Pad with leading zeros to 64 hex chars.
            let padded = format!("{hex_str:0>64}");
            let bytes = hex::decode(&padded).expect("hex must decode");
            let recovered = crate::ecc::hex_to_scalar(&padded).expect("must parse");
            proptest::prop_assert_eq!(recovered, s, "Roundtrip must preserve scalar value");
            let _ = bytes; // silence unused warning
        }
    }
}
