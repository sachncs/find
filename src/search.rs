// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Pure domain logic for the multi-variant range-splitting search engine.
//!
//! This module contains no file I/O, no global mutable state, and no
//! platform-specific code. All side effects are injected via explicit
//! arguments (writers, progress counters).

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
    pub fn new(variants: Vec<OffsetVariant>) -> Self {
        let mut entries = Vec::with_capacity(variants.len());
        for (i, var) in variants.iter().enumerate() {
            entries.push((var.x_bytes, i));
        }
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

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
                    candidates: vec![
                        scalar_to_hex_trimmed(&c1),
                        scalar_to_hex_trimmed(&c2),
                    ],
                }
            })
    }

    /// Returns a slice of the backing variants.
    pub fn variants(&self) -> &[OffsetVariant] {
        &self.variants
    }
}

/// The outcome of a successful match during a search sweep.
#[derive(Debug, Serialize, Deserialize, Clone)]
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
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    /// Atomically adds `n` to the counter and returns the **previous** value.
    pub fn add(&self, n: u64) -> u64 {
        self.counter.fetch_add(n, Ordering::Relaxed)
    }

    /// Reads the current counter value.
    pub fn get(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }
}

/// Abstraction over cache block writes.
///
/// Implementations are responsible for persisting raw 32-byte X-coordinate
/// blocks at arbitrary byte offsets. The trait is object-safe and is
/// intended to be implemented by the [`persistence`] layer so that the search
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
/// collapse to the point-at-infinity are skipped.
///
/// # Performance
///
/// This function performs 512 scalar multiplications and normalizations;
/// it is intended to be called once at the start of a session.
#[instrument(skip(target_p), level = "info")]
pub fn generate_variants(target_p: &ProjectivePoint) -> Vec<OffsetVariant> {
    let mut variants = Vec::with_capacity(512);
    let p = *target_p;

    let mut pow = U256::ONE;
    for i in 0..256 {
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
            tracing::warn!("Variant 2^{} produced identity point; skipping", i);
        }
        pow <<= 1;
    }

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
pub fn perform_chunked_sweep(index: &VariantIndex, start: u64, end: u64) -> Option<SearchMatch> {
    let start = start.max(1);
    if start > end {
        return None;
    }

    const BATCH_SIZE: u64 = 32;
    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = range_len.div_ceil(BATCH_SIZE);

    (0..num_batches)
        .into_par_iter()
        .find_map_any(|batch_idx| {
            let batch_offset = batch_idx * BATCH_SIZE;
            let chunk_start = start.saturating_add(batch_offset);
            let chunk_end = (chunk_start.saturating_add(BATCH_SIZE - 1)).min(end);

            let entries = compute_batch(chunk_start, chunk_end);
            for (j, affine) in entries {
                if let Some(x_bytes) = affine_x_bytes(&affine) {
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

    const BATCH_SIZE: u64 = 32;
    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = range_len.div_ceil(BATCH_SIZE);
    let match_found: Mutex<Option<SearchMatch>> = Mutex::new(None);

    (0..num_batches)
        .into_par_iter()
        .try_for_each(|batch_idx| -> Result<()> {
            if match_found.lock().unwrap().is_some() {
                return Ok(());
            }

            let batch_offset = batch_idx * BATCH_SIZE;
            let chunk_start = start.saturating_add(batch_offset);
            let chunk_end = (chunk_start.saturating_add(BATCH_SIZE - 1)).min(end);
            let entries = compute_batch(chunk_start, chunk_end);

            let mut block = Vec::with_capacity(entries.len() * 32);
            let mut local_match = None;

            for (j, affine) in entries {
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
                block.extend_from_slice(x_bytes);
            }

            if let Some(m) = local_match {
                *match_found.lock().unwrap() = Some(m);
                return Ok(());
            }

            let offset = batch_idx * BATCH_SIZE * 32;
            writer
                .write_block(offset, &block)
                .map_err(FindError::Io)?;
            progress.add(BATCH_SIZE);
            Ok(())
        })?;

    Ok(match_found.into_inner().unwrap())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Computes and batch-normalizes points \(j \cdot G\) for \(j \in [start, end]\).
///
/// Returns a vector of `(j, affine_point)` pairs in ascending order of \(j\).
fn compute_batch(start: u64, end: u64) -> Vec<(u64, AffinePoint)> {
    let count = (end.saturating_sub(start).saturating_add(1)) as usize;
    let mut points = Vec::with_capacity(count);
    for j in start..=end {
        points.push(ecc::scalar_mul_g(&Scalar::from(j)));
    }

    let mut affines = vec![AffinePoint::IDENTITY; points.len()];
    ProjectivePoint::batch_normalize(&points, &mut affines);

    affines
        .into_iter()
        .enumerate()
        .map(|(idx, affine)| (start + idx as u64, affine))
        .collect()
}

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
            assert_ne!(v.x_bytes, [0u8; 32], "All produced variants must have an X-coordinate");
        }
    }
}
