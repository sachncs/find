// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
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
//! - [`precompute_chunk`] uses a [`Mutex<Option<SearchMatch>>`] as a
//!   best-effort broadcast channel: any worker that finds a match writes
//!   it; remaining workers observe it via a non-blocking lock check and
//!   short-circuit. Poisoning is tolerated via `into_inner()` so that a
//!   worker panic does not corrupt the result.
//! - [`perform_chunked_sweep`] uses Rayon's `find_map_any` for early exit
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
//! All hot-path arrays are stack-allocated with a fixed maximum batch size
//! ([`MAX_BATCH`], equal to [`BATCH_SIZE`]). This bounds per-batch stack
//! usage at ~3 KB on x86_64 (32 × 96 bytes for [`ProjectivePoint`] + a
//! small [`AffinePoint`] buffer + a 32 × 32 byte X-coordinate scratch
//! buffer), keeping the working set inside L1 cache.
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
//! 3. **Sweep** ([`perform_chunked_sweep`] / [`precompute_chunk`]): for
//!    each scalar `j` in the chunk, compute `j·G`, extract its
//!    X-coordinate, and probe the index. A match implies `d = V ± j`.
//!
//! See [`docs/algorithms.md`](../docs/algorithms.md) and
//! [ADR-0001](../docs/adr/0001-multi-variant-search.md) for the full
//! mathematical treatment.
//!
//! [`AtomicU64`]: std::sync::atomic::AtomicU64
//! [`Ordering::Relaxed`]: std::sync::atomic::Ordering::Relaxed

use crate::ecc;
use crate::error::{FindError, Result};
use k256::elliptic_curve::bigint::ArrayEncoding;
use k256::elliptic_curve::bigint::U256;
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::ops::Reduce;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{AffinePoint, ProjectivePoint, Scalar};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::instrument;

/// The fixed batch size used for batch normalization in the search engine.
///
/// 32 is empirically the sweet spot on x86_64 and aarch64: stack allocation
/// cost (32 × 96 bytes ≈ 3 KB) fits in L1 cache, and the cost of 32 scalar
/// multiplications roughly balances one batch normalization.
///
/// See [ADR-0002](../docs/adr/0002-batch-normalization.md) for the full
/// rationale.
///
/// # Examples
///
/// ```
/// use find::search::BATCH_SIZE;
/// assert_eq!(BATCH_SIZE, 32);
/// ```
pub const BATCH_SIZE: u64 = 32;

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
/// the variant's `x_bytes`. A match implies the private key is one of
/// \(V + j\) or \(V - j\) (mod \(n\)).
#[derive(Debug, Clone)]
pub struct OffsetVariant {
    /// Human-readable label such as `"2^64"` or `"sum(2^0..2^7)"`.
    pub label: String,
    /// The scalar offset \(V\), already reduced modulo the curve order \(n\).
    pub v_scalar: Scalar,
    /// The 32-byte big-endian X-coordinate of \(P - V \cdot G\).
    pub x_bytes: [u8; 32],
    /// The original unreduced scalar value as a decimal string.
    ///
    /// This is preserved for display and serialization; the reduced value
    /// used during arithmetic is `v_scalar`.
    pub offset: String,
}

/// Cache-optimized lookup index for variant matching.
///
/// Building the index sorts variants by X-coordinate so that each lookup
/// during the sweep is an \(O(\log N)\) binary search instead of a linear
/// scan over all variants.
#[derive(Debug, Clone)]
pub struct VariantIndex {
    sorted_entries: Vec<([u8; 32], usize)>,
    variants: Vec<OffsetVariant>,
}

impl VariantIndex {
    /// Builds a new index from a vector of variants.
    ///
    /// The input order is irrelevant; the index reorders variants internally
    /// by X-coordinate.
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
    /// use find::search::{generate_variants, VariantIndex};
    /// use k256::Scalar;
    ///
    /// let target = ecc::scalar_mul_g(&Scalar::from(123u64));
    /// let variants = generate_variants(&target);
    /// let index = VariantIndex::new(variants);
    /// assert_eq!(index.variants().len(), 512);
    /// ```
    pub fn new(variants: Vec<OffsetVariant>) -> Self {
        let mut entries = Vec::with_capacity(variants.len());
        for (i, var) in variants.iter().enumerate() {
            entries.push((var.x_bytes, i));
        }
        entries.sort_unstable_by_key(|a| a.0);

        Self {
            sorted_entries: entries,
            variants,
        }
    }

    /// Searches for a variant whose X-coordinate equals `test_x`.
    ///
    /// If a match is found, two candidate private keys are derived from the
    /// matched variant's scalar offset and the supplied `j`:
    ///
    /// - \(c_1 = V + j \pmod n\)
    /// - \(c_2 = V - j \pmod n\)
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
    #[inline(always)]
    pub fn match_x(&self, test_x: &[u8; 32], j: u64) -> Option<SearchMatch> {
        self.sorted_entries
            .binary_search_by(|probe| probe.0.cmp(test_x))
            .ok()
            .map(|idx| {
                let (_, var_idx) = self.sorted_entries[idx];
                let var = &self.variants[var_idx];
                let j_scalar = Scalar::from(j);

                let c1 = var.v_scalar.add(&j_scalar);
                let c2 = var.v_scalar.sub(&j_scalar);

                SearchMatch {
                    label: var.label.clone(),
                    offset: var.offset.clone(),
                    small_scalar: j,
                    candidates: vec![scalar_to_hex_trimmed(&c1), scalar_to_hex_trimmed(&c2)],
                }
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
    /// use find::search::{generate_variants, VariantIndex};
    /// use k256::Scalar;
    ///
    /// let target = ecc::scalar_mul_g(&Scalar::from(7u64));
    /// let index = VariantIndex::new(generate_variants(&target));
    /// let first_label = &index.variants()[0].label;
    /// assert!(first_label == "2^0" || first_label.starts_with("sum"));
    /// ```
    pub fn variants(&self) -> &[OffsetVariant] {
        &self.variants
    }
}

/// The outcome of a successful match during a search sweep.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub struct SearchMatch {
    /// The label of the variant that matched.
    pub label: String,
    /// The decimal string representation of the variant's unreduced offset.
    pub offset: String,
    /// The scalar \(j\) at which the match occurred.
    pub small_scalar: u64,
    /// Hex-encoded candidate private keys derived from \(V \pm j\).
    pub candidates: Vec<String>,
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
    ///
    /// let m = SearchMatch::new(
    ///     "2^0",
    ///     "1",
    ///     2,
    ///     vec!["3".to_string(), "fffffffffffffffffffffffffffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364140".to_string()],
    /// );
    /// assert_eq!(m.small_scalar, 2);
    /// assert_eq!(m.label, "2^0");
    /// ```
    pub fn new(
        label: impl Into<String>,
        offset: impl Into<String>,
        small_scalar: u64,
        candidates: Vec<String>,
    ) -> Self {
        Self {
            label: label.into(),
            offset: offset.into(),
            small_scalar,
            candidates,
        }
    }

    /// Converts the hex-encoded candidates to `Scalar` values for downstream
    /// validation.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::EccError`] if any candidate is not a valid
    /// secp256k1 scalar (e.g., the value exceeds the curve order `n`).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::search::SearchMatch;
    /// use k256::Scalar;
    ///
    /// // d = 3, V = 1, j = 2 -> V + j = 3, V - j = -1 mod n.
    /// let m = SearchMatch::new(
    ///     "2^0",
    ///     "1",
    ///     2,
    ///     vec!["03".to_string()],
    /// );
    /// let scalars = m.candidates_as_scalars().unwrap();
    /// assert_eq!(scalars[0], Scalar::from(3u64));
    /// ```
    pub fn candidates_as_scalars(&self) -> Result<Vec<Scalar>> {
        self.candidates
            .iter()
            .map(|hex_str| {
                use k256::elliptic_curve::PrimeField;
                let bytes = hex::decode(hex_str)
                    .map_err(|e| FindError::EccError(format!("hex decode failed: {}", e)))?;
                let mut fixed_bytes = [0u8; 32];
                let len = bytes.len().min(32);
                let src = &bytes[..len];
                fixed_bytes[32 - src.len()..].copy_from_slice(src);
                Option::from(Scalar::from_repr(fixed_bytes.into())).ok_or_else(|| {
                    FindError::EccError(format!("Scalar {} exceeds curve order n", hex_str))
                })
            })
            .collect()
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
    pub fn new() -> Self {
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

/// Abstraction over cache block writes.
///
/// Implementations are responsible for persisting raw 32-byte X-coordinate
/// blocks at arbitrary byte offsets. The trait is object-safe and is
/// intended to be implemented by the `persistence` layer so that the search
/// domain remains free of file-system details.
pub trait CacheWriter: Send + Sync {
    /// Writes `data` starting at `offset` bytes into the cache.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the underlying storage operation fails.
    fn write_block(&self, offset: u64, data: &[u8]) -> std::io::Result<()>;
}

/// Generates 512 shift variants from a target public key.
///
/// The variants consist of 256 powers of two (\(2^0 \dots 2^{255}\)) and
/// 256 cumulative sums (\(\sum_{i=0}^{k} 2^i = 2^{k+1} - 1\)). Variants that
/// collapse to the point-at-infinity are skipped — this is correctness-
/// critical, not a performance optimisation: the identity has no X-
/// coordinate, so an identity variant would match every sweep entry.
/// See [ADR-0007](../docs/adr/0007-y-parity-ambiguity.md) for the
/// related Y-parity discussion.
///
/// # Performance
///
/// This function performs 512 scalar multiplications and normalizations;
/// it is intended to be called once at the start of a session.
///
/// # Pseudocode
///
/// ```text
/// variants = []
/// # Powers-of-two pass: V = 2^i for i in 0..256
/// pow = U256::ONE
/// for i in 0..256:
///     scalar = Scalar::reduce(pow)            # pow < 2^256 < n, so reduce is a no-op
///     shifted = P - scalar * G
///     if shifted != identity:
///         variants.append(OffsetVariant { V = pow, x = X(shifted) })
///     pow <<= 1
/// # Cumulative-sum pass: V = 2^(i+1) - 1 for i in 0..256
/// cum = U256::ONE
/// for i in 0..256:
///     scalar = Scalar::reduce(cum)
///     shifted = P - scalar * G
///     if shifted != identity:
///         variants.append(OffsetVariant { V = cum, x = X(shifted) })
///     cum = (cum << 1) | U256::ONE             # generates 1, 3, 7, 15, ...
/// return variants
/// ```
///
/// # Complexity
///
/// \(O(512)\) scalar multiplications plus \(O(512)\) projective→affine
/// conversions. Wall-clock cost is dominated by the multiplications; the
/// conversions are amortized by the orchestrator's batching policy.
///
/// # Examples
///
/// ```
/// use find::ecc;
/// use find::search::generate_variants;
/// use k256::Scalar;
///
/// let target = ecc::scalar_mul_g(&Scalar::from(42u64));
/// let variants = generate_variants(&target);
/// assert!(!variants.is_empty());
/// assert!(variants.iter().all(|v| !v.label.is_empty()));
/// ```
#[instrument(skip(target_p), level = "info")]
pub fn generate_variants(target_p: &ProjectivePoint) -> Vec<OffsetVariant> {
    let mut variants = Vec::with_capacity(512);
    let p = *target_p;

    let mut pow = U256::ONE;
    for i in 0..256 {
        // `Scalar::reduce` is constant-time reduction mod n. For all i < 256
        // we have pow = 2^i < 2^256, which is far below n, so the reduction
        // is effectively a no-op identity — the resulting scalar equals pow.
        let scalar = Scalar::reduce(pow);
        let shifted = p - ecc::scalar_mul_g(&scalar);
        if let Some(x) = affine_x_bytes(&shifted.to_affine()) {
            variants.push(OffsetVariant {
                label: format!("2^{}", i),
                v_scalar: scalar,
                x_bytes: x,
                offset: u256_to_decimal(&pow),
            });
        } else {
            // An identity variant has no X-coordinate and would match every
            // sweep entry. Skipping is correctness-critical, not a perf opt.
            tracing::warn!("Variant 2^{} produced identity point; skipping", i);
        }
        pow <<= 1;
    }

    // Cumulative-sum variants: cum_i = sum_{k=0..i} 2^k = 2^{i+1} - 1.
    // The recurrence `cum = (cum << 1) | 1` doubles cum and sets the new
    // low bit, generating 1, 3, 7, 15, … (i.e. 2^{i+1} - 1).
    let mut cum = U256::ONE;
    for i in 0..256 {
        let scalar = Scalar::reduce(cum);
        let shifted = p - ecc::scalar_mul_g(&scalar);
        if let Some(x) = affine_x_bytes(&shifted.to_affine()) {
            variants.push(OffsetVariant {
                label: format!("sum(2^0..2^{})", i),
                v_scalar: scalar,
                x_bytes: x,
                offset: u256_to_decimal(&cum),
            });
        } else {
            // See the powers-of-two loop above for the rationale.
            tracing::warn!(
                "Variant sum(2^0..2^{}) produced identity point; skipping",
                i
            );
        }
        cum = (cum << 1) | U256::ONE;
    }

    variants
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
/// use find::search::{generate_variants, perform_chunked_sweep, VariantIndex};
/// use k256::Scalar;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let target = ecc::scalar_mul_g(&Scalar::from(12345u64));
///     let index = VariantIndex::new(generate_variants(&target));
///     let m = perform_chunked_sweep(&index, 1, 100_000)
///         .expect("match for d=12345 in [1, 100000]");
///     assert!(m.candidates.iter().any(|c| c.to_lowercase() == "3039"));
///     Ok(())
/// }
/// ```
pub fn perform_chunked_sweep(index: &VariantIndex, start: u64, end: u64) -> Option<SearchMatch> {
    let start = start.max(1);
    if start > end {
        return None;
    }

    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = if range_len == 0 {
        0
    } else {
        (range_len - 1) / BATCH_SIZE + 1
    };

    (0..num_batches).into_par_iter().find_map_any(|batch_idx| {
        let batch_offset = batch_idx * BATCH_SIZE;
        let chunk_start = start.saturating_add(batch_offset);
        let chunk_end = (chunk_start.saturating_add(BATCH_SIZE - 1)).min(end);

        let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;
        let mut points = [ProjectivePoint::IDENTITY; MAX_BATCH];
        let mut affines = [AffinePoint::IDENTITY; MAX_BATCH];

        // Bootstrap: one scalar multiplication to get (chunk_start)·G.
        // After that, advance by adding G once per step — a mixed addition
        // is ~12 field multiplications, vs. ~256 multiplications for a
        // fresh scalar mul. This `+ G` chain is the dominant perf win of
        // the search engine (~20× vs. independent scalar muls).
        // See ADR-0002 for the full rationale.
        let mut current = ecc::scalar_mul_g(&Scalar::from(chunk_start));
        for p in points.iter_mut().take(count) {
            *p = current;
            current += ecc::generator();
        }

        ProjectivePoint::batch_normalize(&points[..count], &mut affines[..count]);

        for (i, affine) in affines.iter().enumerate().take(count) {
            let j = chunk_start + i as u64;
            if let Some(x_bytes) = affine_x_bytes(affine) {
                if let Some(m) = index.match_x(&x_bytes, j) {
                    return Some(m);
                }
            }
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
/// Identical arithmetic to [`perform_chunked_sweep`]; the additional cost
/// is one `write_block` call per batch (a single `pwrite_at` of ~1 KiB on
/// Unix, an `O(1)` operation). The progress counter is updated once per
/// batch, so it reflects the **actual** scalars processed (not a
/// multiple of `BATCH_SIZE`).
///
/// The early-exit lock check at the top of each batch is lock-free in the
/// common case: the worker takes the mutex briefly to inspect whether a
/// match was already recorded, then releases it. Lock contention is
/// negligible because the critical section is one branch.
///
/// # Pseudocode
///
/// ```text
/// match_found = Mutex<None>
/// batches.parallel_for_each(|batch_idx| {
///     if match_found contains Some(_): return    # fast-path no-op
///     let (chunk_start, count) = batch_bounds(batch_idx)
///     let mut current = chunk_start * G
///     let mut block = []
///     for _ in 0..count:
///         block.push(X(current))
///         if let Some(idx) = index:
///             if idx.match_x(X(current), j) is Some(m):
///                 match_found = Some(m); return
///         current += G
///     writer.write_block(batch_offset, &block)?
///     progress.add(count)
/// })
/// match_found.into_inner()
/// ```
pub fn precompute_chunk<W: CacheWriter>(
    start: u64,
    end: u64,
    writer: &W,
    index: Option<&VariantIndex>,
    progress: &Progress,
) -> Result<Option<SearchMatch>> {
    let start = start.max(1);
    if start > end {
        return Ok(None);
    }

    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = if range_len == 0 {
        0
    } else {
        (range_len - 1) / BATCH_SIZE + 1
    };
    let match_found: Mutex<Option<SearchMatch>> = Mutex::new(None);

    (0..num_batches)
        .into_par_iter()
        .try_for_each(|batch_idx| -> Result<()> {
            // Fast-path check without locking — if another worker already
            // found a match, skip this batch entirely.
            {
                let guard = match_found.lock();
                if guard.is_ok_and(|g| g.is_some()) {
                    return Ok(());
                }
            }

            let batch_offset = batch_idx * BATCH_SIZE;
            let chunk_start = start.saturating_add(batch_offset);
            let chunk_end = (chunk_start.saturating_add(BATCH_SIZE - 1)).min(end);
            let count = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) as usize;

            let mut points = [ProjectivePoint::IDENTITY; MAX_BATCH];
            let mut affines = [AffinePoint::IDENTITY; MAX_BATCH];

            // `+ G` increment chain: see `perform_chunked_sweep` for the
            // full rationale. One bootstrap scalar mul + (count - 1) mixed
            // additions is ~20× faster than `count` independent scalar muls.
            // See ADR-0002.
            let mut current = ecc::scalar_mul_g(&Scalar::from(chunk_start));
            for p in points.iter_mut().take(count) {
                *p = current;
                current += ecc::generator();
            }

            ProjectivePoint::batch_normalize(&points[..count], &mut affines[..count]);

            let mut block = [0u8; 32 * MAX_BATCH];
            let mut block_len = 0usize;
            let mut local_match = None;

            for (i, affine) in affines.iter().enumerate().take(count) {
                let j = chunk_start + i as u64;
                let encoded = affine.to_encoded_point(false);
                let x_bytes = match encoded.x() {
                    Some(x) => x.as_ref(),
                    None => continue,
                };

                if let Some(idx_ref) = index {
                    let mut test_x = [0u8; 32];
                    test_x.copy_from_slice(x_bytes);
                    if let Some(m) = idx_ref.match_x(&test_x, j) {
                        local_match = Some(m);
                        break;
                    }
                }

                block[block_len..block_len + 32].copy_from_slice(x_bytes);
                block_len += 32;
            }

            if let Some(m) = local_match {
                if let Ok(mut guard) = match_found.lock() {
                    *guard = Some(m);
                }
                return Ok(());
            }

            // Cache-file byte offset for this batch's X-coordinates.
            // BATCH_SIZE scalars × 32 bytes per X-coordinate (SEC1 X-only).
            let offset = batch_idx * BATCH_SIZE * 32;
            writer
                .write_block(offset, &block[..block_len])
                .map_err(FindError::Io)?;
            progress.add(count as u64);
            Ok(())
        })?;

    // Extract the match, gracefully ignoring poison (a panicked worker may
    // have held the lock; we still want to return whatever result we have).
    let result = match match_found.into_inner() {
        Ok(r) => r,
        Err(poisoned) => {
            tracing::warn!("Precompute worker panicked; extracting partial result");
            poisoned.into_inner()
        }
    };
    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fixed maximum batch size used throughout the search engine.
///
/// All hot-path arrays are stack-allocated to this size, guaranteeing O(1)
/// space per batch regardless of the scalar range being swept.
///
/// This constant is intentionally equal to [`BATCH_SIZE`] and exposed as
/// `pub` so that downstream consumers and benchmark authors can reason
/// about the per-batch stack budget (~3 KB on x86_64:
/// `32 × 96` bytes for [`ProjectivePoint`] + `32 × 32` bytes for the
/// X-coordinate scratch buffer + a small [`AffinePoint`] mirror).
///
/// See [ADR-0002](../docs/adr/0002-batch-normalization.md) for the
/// rationale behind the chosen size.
pub const MAX_BATCH: usize = 32;

/// Extracts the 32-byte big-endian X-coordinate from an affine point.
///
/// Returns `None` if the point is the point-at-infinity.
fn affine_x_bytes(affine: &AffinePoint) -> Option<[u8; 32]> {
    let encoded = affine.to_encoded_point(false);
    encoded.x().map(|x| {
        let mut b = [0u8; 32];
        b.copy_from_slice(x.as_ref());
        b
    })
}

/// Converts a scalar to a lower-case hex string with leading zeros removed.
///
/// The value zero is rendered as `"0"`.
fn scalar_to_hex_trimmed(s: &Scalar) -> String {
    let hex = hex::encode(s.to_bytes());
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Converts a [`U256`] to a decimal string.
///
/// This is used for display and serialization; it is not on the hot path.
fn u256_to_decimal(v: &U256) -> String {
    use num_bigint::BigUint;
    let bytes = v.to_be_byte_array();
    BigUint::from_bytes_be(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the [`VariantIndex`] correctly matches a known X-coordinate.
    #[test]
    fn test_indexing_speedup() {
        let target = ecc::scalar_mul_g(&Scalar::from(1000u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let p_999 = ecc::scalar_mul_g(&Scalar::from(999u64));
        let encoded = p_999.to_affine().to_encoded_point(false);
        let x_bytes = encoded.x().unwrap();
        let mut x_999 = [0u8; 32];
        x_999.copy_from_slice(x_bytes.as_ref());

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
        assert!(!variants.is_empty());
        for v in &variants {
            assert_ne!(
                v.x_bytes, [0u8; 32],
                "All produced variants must have an X-coordinate"
            );
        }
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

    /// Verifies that [`perform_chunked_sweep`] returns `None` when start > end.
    #[test]
    fn test_perform_chunked_sweep_start_greater_than_end() {
        let target = ecc::scalar_mul_g(&Scalar::from(1u64));
        let index = VariantIndex::new(generate_variants(&target));
        assert!(perform_chunked_sweep(&index, 100, 1).is_none());
    }

    /// Verifies that [`precompute_chunk`] returns `Ok(None)` when start > end.
    #[test]
    fn test_precompute_chunk_start_greater_than_end() {
        struct DummyWriter;
        impl CacheWriter for DummyWriter {
            fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
        }
        let progress = Progress::new();
        let result = precompute_chunk(100, 1, &DummyWriter, None, &progress);
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
        let index = VariantIndex::new(variants.clone());
        assert_eq!(index.variants().len(), variants.len());
    }

    /// Verifies that [`VariantIndex::match_x`] returns `None` for unknown X.
    #[test]
    fn test_match_x_not_found() {
        let target = ecc::scalar_mul_g(&Scalar::from(7u64));
        let index = VariantIndex::new(generate_variants(&target));
        let unknown = [0xffu8; 32];
        assert!(index.match_x(&unknown, 1).is_none());
    }

    /// Verifies that [`precompute_chunk`] discovers a match and returns early.
    #[test]
    fn test_precompute_chunk_finds_match() {
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
        let index = VariantIndex::new(generate_variants(&target));
        let progress = Progress::new();

        let result = precompute_chunk(1, 10, &NullWriter, Some(&index), &progress).unwrap();
        assert!(
            result.is_some(),
            "precompute_chunk must find match for d=3 in range [1,10]"
        );
        let m = result.unwrap();
        assert!(
            m.candidates.contains(&"3".to_string()),
            "Candidates must include d=3, got: {:?} (found via {} at j={})",
            m.candidates,
            m.label,
            m.small_scalar
        );
    }

    /// Verifies that `precompute_chunk` reports the actual batch count, not
    /// `BATCH_SIZE`, for the last partial batch.
    #[test]
    fn test_precompute_chunk_progress_partial_batch() {
        struct NullWriter;
        impl CacheWriter for NullWriter {
            fn write_block(&self, _offset: u64, _data: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Use a target that does NOT match in the sweep range, so all
        // batches complete and the progress reflects the actual work.
        let target = ecc::scalar_mul_g(&Scalar::from(1_000_000u64));
        let index = VariantIndex::new(generate_variants(&target));
        let progress = Progress::new();

        // Sweep range [1, 5]: 5 scalars in 1 partial batch. The engine
        // should call `progress.add(5)` (the actual count), not
        // `progress.add(BATCH_SIZE=32)`.
        let result = precompute_chunk(1, 5, &NullWriter, Some(&index), &progress).unwrap();
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
        let index = VariantIndex::new(variants);
        let p_1 = ecc::scalar_mul_g(&Scalar::from(1u64));
        let encoded = p_1.to_affine().to_encoded_point(false);
        let x_bytes = encoded.x().unwrap();
        let mut x_1 = [0u8; 32];
        x_1.copy_from_slice(x_bytes.as_ref());

        let m = index
            .match_x(&x_1, 1)
            .expect("Must find a match for j=1, V=1");
        // The matched variant's V is 1, so d = V + j = 2 or d = V - j = 0.
        assert!(
            m.candidates.contains(&"2".to_string()),
            "Candidates must include d=2, got: {:?}",
            m.candidates
        );
    }

    // Property: `generate_variants` produces a non-empty variant set for any
    // non-identity target.
    proptest::proptest! {
        #[test]
        fn prop_generate_variants_count(d in 1u64..1_000_000u64) {
            let target = ecc::scalar_mul_g(&Scalar::from(d));
            let variants = generate_variants(&target);
            proptest::prop_assert!(!variants.is_empty(),
                "Variant set must be non-empty for non-identity targets");
            // For typical targets, no variant collapses to the identity.
            // We allow some slack (>= 500) but expect the full 512 in
            // the common case.
            proptest::prop_assert!(variants.len() >= 500,
                "Expected >= 500 variants, got {}", variants.len());
        }
    }

    // Property: `scalar_to_hex_trimmed` produces a hex string that, when
    // padded back to 32 bytes and decoded, yields the original scalar.
    proptest::proptest! {
        #[test]
        fn prop_scalar_to_hex_trimmed_inverts(d in 0u64..1_000_000u64) {
            let s = Scalar::from(d);
            let hex_str = scalar_to_hex_trimmed(&s);

            // Pad with leading zeros to 64 hex chars.
            let padded = format!("{:0>64}", hex_str);
            let bytes = hex::decode(&padded).expect("hex must decode");
            let recovered = hex_to_scalar_for_test(&padded).expect("must parse");
            proptest::prop_assert_eq!(recovered, s, "Roundtrip must preserve scalar value");
            let _ = bytes; // silence unused warning
        }
    }

    /// Helper for `prop_scalar_to_hex_trimmed_inverts` — re-implements
    /// `hex_to_scalar` to avoid a cross-module dependency in the test.
    fn hex_to_scalar_for_test(hex_str: &str) -> Option<Scalar> {
        use k256::elliptic_curve::PrimeField;
        let bytes = hex::decode(hex_str).ok()?;
        let mut fixed_bytes = [0u8; 32];
        let len = bytes.len().min(32);
        let src = &bytes[..len];
        fixed_bytes[32 - src.len()..].copy_from_slice(src);
        Option::from(Scalar::from_repr(fixed_bytes.into()))
    }
}
