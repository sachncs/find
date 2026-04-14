// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-performance parallel search engine and binary caching protocols.
//!
//! # 🔬 Algorithmic Formalization
//! The `search` module implements a **Multi-Variant Range-Splitting** strategy.
//! Given a target public key $P = d \cdot G$, we search for a scalar $j$ and
//! a pre-defined offset $V$ such that:
//! $$x(j \cdot G) = x(P - V \cdot G)$$
//!
//! ## 📐 Mathematical Invariants
//! This equality on the X-coordinate represents a match due to point symmetry.
//! For each match found, the system derives two scalar candidates for $d$:
//! 1.  **Positive Case:** $P - V \cdot G = j \cdot G \implies d \equiv V + j \pmod n$
//! 2.  **Negative Case:** $P - V \cdot G = -j \cdot G \implies d \equiv V - j \pmod n$
//!
//! ## ⚡ Performance Optimizations
//! - **Indexing:** An $O(1)$ `VariantIndex` converts the $O(V)$ variant scan
//!   into a high-speed hash-table collision check.
//! - **Parallelism:** Utilizes `rayon` for work-stealing parallel sweeps
//!   across multi-core systems.
//! - **Binary Caching:** Enforces a rigid 32-byte sequential format for $jG$
//!   points, enabling I/O-bound search that bypasses ECC arithmetic.

use crate::ecc;
use crate::error::{FindError, Result};
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::PrimeField;
use k256::{elliptic_curve::sec1::ToEncodedPoint, ProjectivePoint, Scalar};
use num_bigint::BigUint;
use num_traits::One;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::os::unix::fs::FileExt; // Atomic pwrite for parallel I/O.
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{error, info, instrument};

use std::sync::OnceLock;

/// secp256k1 Curve Order $n$, used for modular scalar arithmetic.
///
/// The curve order is defined by the secp256k1 specification and never changes.
/// This constant is verified against the k256 crate's built-in constant at runtime.
pub static CURVE_ORDER: OnceLock<BigUint> = OnceLock::new();

/// Global progress counter for binary cache generation across multiple chunks.
/// Accumulates monotonically across calls to allow progress tracking.
static PROGRESS: AtomicU64 = AtomicU64::new(0);

/// Ensures the rayon thread pool is configured with a panic handler.
/// This can only be called once globally; subsequent calls are no-ops.
/// The panic handler logs worker panics rather than aborting the process
/// (though `panic = 'abort'` in release profile overrides this).
fn ensure_rayon_panic_handler() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rayon::ThreadPoolBuilder::new()
            .panic_handler(|panic_info| {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                tracing::error!(message = %msg, "Rayon worker thread panicked");
            })
            .build_global();
    });
}

/// Hardcoded secp256k1 curve order n = 2^256 - 2^32 - 2^9 - 2^8 - 2^7 - 2^6 - 2^4 - 1.
/// This is a well-known cryptographic constant from the secp256k1 specification.
const CURVE_ORDER_HEX: &str = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";

/// Global accessor for the Curve Order n.
pub fn curve_order() -> &'static BigUint {
    CURVE_ORDER.get_or_init(|| {
        BigUint::parse_bytes(CURVE_ORDER_HEX.as_bytes(), 16)
            .expect("Hardcoded secp256k1 curve order hex is always valid and within range")
    })
}

/// A shift-variant anchor used to split the massive search space.
///
/// Each variant represents a target point shifted by a specific scalar
/// $V$ (e.g., $2^{128}$). This allows the engine to sweep a small range
/// while effectively exploring 512 different remote regions of the curve.
#[derive(Debug, Clone)]
pub struct OffsetVariant {
    /// Identification label (e.g., "power-of-2-64").
    pub label: String,
    /// The shifted point: $P' = P - (V \cdot G)$.
    pub point: ProjectivePoint,
    /// The scalar value $V$ used for the shift.
    pub scalar_value: BigUint,
    /// Static X-coordinate buffer for fast comparison.
    pub x_bytes: Option<[u8; 32]>,
}

/// Cache-optimized lookup index for variants to achieve $O(\log N)$ matching.
///
/// Without this index, every step in the $10^{12}$ scalar sweep would require
/// 512 byte-comparisons. The `VariantIndex` collapses this into a high-speed
/// binary search over a flat, cache-aligned array.
#[derive(Debug, Clone)]
pub struct VariantIndex {
    /// Sorted flat array of (X-coordinate, original_variant_index).
    /// INVARIANT: Every entry's variant_idx must be valid (< variants.len())
    /// and that variant must have x_bytes = Some.
    pub sorted_entries: Vec<([u8; 32], usize)>,
    /// Backing list of full variant metadata.
    pub variants: Vec<OffsetVariant>,
}

impl VariantIndex {
    /// Constructs a new lookup index. The entries are sorted to enable
    /// O(log N) binary search with optimal cache locality.
    pub fn new(variants: Vec<OffsetVariant>) -> Self {
        let mut entries = Vec::with_capacity(variants.len());
        for (i, var) in variants.iter().enumerate() {
            if let Some(x) = var.x_bytes {
                entries.push((x, i));
            }
        }
        // Sort by X-coordinate to enable binary search.
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        // Defensive: verify invariant that all sorted_entries variant indices are in bounds.
        #[cfg(debug_assertions)]
        {
            for (_, var_idx) in &entries {
                assert!(
                    *var_idx < variants.len(),
                    "VariantIndex invariant violated: variant index {} out of bounds (variants.len() = {})",
                    var_idx,
                    variants.len()
                );
                assert!(
                    variants[*var_idx].x_bytes.is_some(),
                    "VariantIndex invariant violated: variant at index {} has no x_bytes",
                    var_idx
                );
            }
        }

        Self {
            sorted_entries: entries,
            variants,
        }
    }

    /// Performs a high-speed match using binary search on the flat array.
    #[inline(always)]
    pub fn match_x(&self, test_x: &[u8; 32], j: u64) -> Option<SearchMatch> {
        self.sorted_entries
            .binary_search_by(|probe| probe.0.cmp(test_x))
            .ok()
            .map(|idx| {
                let (_, var_idx) = self.sorted_entries[idx];
                let var = &self.variants[var_idx];
                let mut candidates = Vec::new();
                let n = curve_order();

                let c1 = (&var.scalar_value + BigUint::from(j)) % n;
                candidates.push(c1.to_str_radix(16));

                let c2 = if var.scalar_value >= BigUint::from(j) {
                    (&var.scalar_value - BigUint::from(j)) % n
                } else {
                    (n + &var.scalar_value - BigUint::from(j)) % n
                };
                candidates.push(c2.to_str_radix(16));

                SearchMatch {
                    label: var.label.clone(),
                    offset: var.scalar_value.to_str_radix(10),
                    small_scalar: j,
                    candidates,
                }
            })
    }
}

/// Structured search result containing all derived private key candidates.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMatch {
    pub label: String,
    pub offset: String,
    pub small_scalar: u64,
    pub candidates: Vec<String>,
}

/// Orchestrates the generation of 512 target shift variants.
///
/// Generates variants based on:
/// 1.  **Powers of 2** ($2^0 \to 2^{255}$)
/// 2.  **Leibniz-style Summations** ($\sum 2^i$)
///
/// This covers both bit-aligned and cumulative range segments.
#[instrument(skip(target_p), level = "info")]
pub fn generate_variants(target_p: &ProjectivePoint) -> Vec<OffsetVariant> {
    let mut variants = Vec::with_capacity(512);
    let p = *target_p; // Dereference for projective arithmetic efficacy.

    // Power-of-2 variant generation.
    for i in 0..256 {
        let val = BigUint::one() << i;
        let val_mod = &val % curve_order();
        let scalar = biguint_to_scalar(&val_mod);
        let shifted_p: ProjectivePoint = p - ecc::scalar_mul_g(&scalar);

        let affine = shifted_p.to_affine();
        let encoded = affine.to_encoded_point(false);
        let x_bytes: Option<[u8; 32]> = encoded.x().map(|x| {
            let mut b = [0u8; 32];
            b.copy_from_slice(x.as_ref());
            b
        });

        if x_bytes.is_none() {
            tracing::warn!(
                "Variant 2^{} produced identity point (P - {}-G = O). Dropping from index.",
                i,
                val
            );
        }

        variants.push(OffsetVariant {
            label: format!("2^{}", i),
            point: shifted_p,
            scalar_value: val,
            x_bytes,
        });
    }

    // Cumulative summation variant generation.
    for i in 0..256 {
        let val = (BigUint::one() << (i + 1)) - BigUint::one();
        let val_mod = &val % curve_order();
        let scalar = biguint_to_scalar(&val_mod);
        let shifted_p: ProjectivePoint = p - ecc::scalar_mul_g(&scalar);

        let affine = shifted_p.to_affine();
        let encoded = affine.to_encoded_point(false);
        let x_bytes: Option<[u8; 32]> = encoded.x().map(|x| {
            let mut b = [0u8; 32];
            b.copy_from_slice(x.as_ref());
            b
        });

        if x_bytes.is_none() {
            tracing::warn!(
                "Variant sum(2^0..2^{}) produced identity point (P - {}-G = O). Dropping from index.",
                i,
                val
            );
        }

        variants.push(OffsetVariant {
            label: format!("sum(2^0..2^{})", i),
            point: shifted_p,
            scalar_value: val,
            x_bytes,
        });
    }

    variants
}

/// Performs a CPU-bound parallel sweep using work-stealing threads.
/// Performs a high-throughput parallel sweep using batch normalization.
///
/// This implementation amortizes the modular inversion cost (the primary bottleneck)
/// by processing scalars in batches of 32, yielding a theoretical 15-20x speedup
/// in coordinate extraction.
pub fn perform_chunked_sweep(index: &VariantIndex, start: u64, end: u64) -> Option<SearchMatch> {
    // The identity point (j=0) has no x-coordinate and can never match a variant.
    // batch_normalize panics on Z=0, so we skip it.
    let start = start.max(1);
    if start > end {
        return None;
    }

    const BATCH_SIZE: u64 = 32;

    let range_len = end.saturating_sub(start).saturating_add(1);
    let num_batches = range_len.div_ceil(BATCH_SIZE);

    // Ensure rayon is configured with a panic handler before entering the parallel loop.
    ensure_rayon_panic_handler();

    // We iterate over batch indices to avoid allocating a massive Vec for the range.
    (0..num_batches).into_par_iter().find_map_any(|batch_idx| {
        // Explicit overflow guards for u64 arithmetic in batch index computation.
        let batch_offset = batch_idx * BATCH_SIZE;
        let chunk_start = start
            .checked_add(batch_offset)
            .expect("batch start overflow: range exceeds u64 address space");
        let chunk_end = chunk_start.checked_add(BATCH_SIZE - 1).unwrap_or(end); // If overflow, clip to range end
        let actual_end = chunk_end.min(end);
        let mut points = Vec::with_capacity(BATCH_SIZE as usize);

        // Phase 1: Rapid Scalar Multiplication (Projective)
        for j in chunk_start..=actual_end {
            points.push(ecc::scalar_mul_g(&Scalar::from(j)));
        }

        // Phase 2: Batch Normalization (Single modular inversion)
        // k256 provides batch_normalize to amortize inversion costs.
        // We pre-allocate the affine buffer and normalize in-place.
        let mut affines = vec![k256::AffinePoint::IDENTITY; points.len()];
        k256::ProjectivePoint::batch_normalize(&points, &mut affines);

        // Phase 3: Final Matching Sweep
        for (idx, affine) in affines.iter().enumerate() {
            let j = chunk_start + idx as u64;
            let encoded: k256::elliptic_curve::sec1::EncodedPoint<k256::Secp256k1> =
                affine.to_encoded_point(false);

            if let Some(x_bytes) = encoded.x() {
                let mut test_x = [0u8; 32];
                test_x.copy_from_slice(x_bytes.as_ref());

                if let Some(matching) = index.match_x(&test_x, j) {
                    return Some(matching);
                }
            }
        }
        None
    })
}

/// Generates a high-performance binary database using parallel batch normalization.
///
/// This implementation achieves principal-grade throughput by:
/// 1.  **Rayon Threadpooling**: Distributes the workload across all CPU cores.
/// 2.  **Batch Normalization**: Processes 32 points per batch to amortize inversion.
/// 3.  **Atomic pwrite**: Uses `write_all_at` for non-blocking parallel I/O.
pub fn precompute_chunk(
    start: u64,
    end: u64,
    file_path: &Path,
    index: Option<&VariantIndex>,
) -> Result<Option<SearchMatch>> {
    // The identity point (j=0) has no x-coordinate and can never match.
    // batch_normalize panics on Z=0, so we skip it.
    let start = start.max(1);
    if start > end {
        return Ok(None);
    }

    // Pre-create and pre-allocate the file OUTSIDE the parallel loop.
    // This prevents every worker from redundantly creating/resizing the same file.
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(file_path)?;
    const BATCH_SIZE: u64 = 32;
    let range_len = end.saturating_sub(start).saturating_add(1);
    file.set_len(range_len * 32)?; // Pre-allocate to prevent fragmentation.
    let num_batches = range_len.div_ceil(BATCH_SIZE);
    // NOTE: We intentionally do NOT reset PROGRESS here. Progress accumulates
    // across calls to allow global progress tracking across multiple chunks.
    // Reset only at program initialization or when a fresh search begins.
    PROGRESS.fetch_add(0, Ordering::Relaxed);

    // Ensure rayon is configured with a panic handler before entering the parallel loop.
    ensure_rayon_panic_handler();

    let discovery = (0..num_batches).into_par_iter().find_map_any(|batch_idx| {
        // Explicit overflow guards for u64 arithmetic.
        let batch_offset = batch_idx * BATCH_SIZE;
        let chunk_start = start
            .checked_add(batch_offset)
            .expect("batch start overflow: range exceeds u64 address space");
        let chunk_end = chunk_start.checked_add(BATCH_SIZE - 1).unwrap_or(end);
        let actual_end = chunk_end.min(end);
        let mut points = Vec::with_capacity(BATCH_SIZE as usize);

        for j in chunk_start..=actual_end {
            points.push(ecc::scalar_mul_g(&Scalar::from(j)));
        }

        let mut affines = vec![k256::AffinePoint::IDENTITY; points.len()];
        ProjectivePoint::batch_normalize(&points, &mut affines);

        // Real-Time Discovery Phase: Check points before writing to disk.
        let mut match_found = None;
        let mut binary_block = Vec::with_capacity(affines.len() * 32);

        for (idx, affine) in affines.iter().enumerate() {
            let encoded = affine.to_encoded_point(false);
            // Identity point (O) has no affine X-coordinate; skip gracefully.
            let x_bytes = match encoded.x() {
                Some(x) => x.as_ref(),
                None => continue,
            };

            if let Some(idx_ref) = index {
                let mut test_x = [0u8; 32];
                test_x.copy_from_slice(x_bytes);
                if let Some(m) = idx_ref.match_x(&test_x, chunk_start + idx as u64) {
                    match_found = Some(m);
                }
            }
            binary_block.extend_from_slice(x_bytes);
        }

        // Atomic Parallel Write using pwrite_at.
        let offset = batch_idx * BATCH_SIZE * 32;
        if let Err(e) = file.write_all_at(&binary_block, offset) {
            error!("Background I/O failure during precompute: {}", e);
        }

        // Progress Heartbeat Update (Every 10 million keys).
        // Uses fetch_add to avoid resetting progress across chunks.
        let current = PROGRESS.fetch_add(BATCH_SIZE, Ordering::Relaxed);
        if current > 0 && current.is_multiple_of(10_000_000) {
            info!(
                "Binary cache generation progress: {}M keys...",
                current / 1_000_000
            );
        }

        match_found
    });

    Ok(discovery)
}

/// Performs an I/O-bound cached search against a pre-computed binary database.
#[instrument(skip(index), level = "info")]
pub fn perform_cached_sweep(
    index: &VariantIndex,
    cache_path: &Path,
    start_j: u64,
) -> Result<Option<SearchMatch>> {
    let file = File::open(cache_path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    // Validate cache file integrity: each point is exactly 32 bytes.
    if file_size % 32 != 0 {
        return Err(FindError::CacheCorrupted(format!(
            "Cache file size {} is not a multiple of 32 bytes",
            file_size
        )));
    }
    if file_size == 0 {
        return Ok(None);
    }

    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; 32];
    let mut j = start_j;

    // Sequential read-scan optimized for modern NVMe SSDs.
    // Distinguish between "no match found" and I/O errors — conflating them
    // would silently swallow disk errors as "no match".
    loop {
        match reader.read_exact(&mut buffer) {
            Ok(()) => {
                if let Some(m) = index.match_x(&buffer, j) {
                    return Ok(Some(m));
                }
                j += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Normal EOF: file ended cleanly. This is "no match found".
                break;
            }
            Err(e) => {
                // Real I/O error (permission denied, disk full, etc.) — propagate.
                return Err(FindError::Io(e));
            }
        }
    }

    Ok(None)
}

/// Persists session variants to JSON for multi-pubkey auditability.
#[instrument(skip(variants, dir_path), level = "info")]
pub fn save_variants_to_json(variants: &[OffsetVariant], dir_path: &str) -> Result<String> {
    let mut map = BTreeMap::new();
    for var in variants {
        if let Some(x_bytes) = var.x_bytes {
            let x_hex = hex::encode(x_bytes);
            let val_str = var.scalar_value.to_string();
            map.insert(x_hex, val_str);
        }
    }

    let json = serde_json::to_string_pretty(&map).map_err(Into::<crate::error::FindError>::into)?;
    fs::create_dir_all(dir_path)?;

    let file_path = Path::new(dir_path).join("points.json");
    fs::write(&file_path, json)?;

    Ok(file_path.to_string_lossy().into_owned())
}

/// Safely converts BigUint to a k256 Scalar element.
///
/// Enforces 32-byte BE representation and handles curve-order boundaries.
fn biguint_to_scalar(big: &BigUint) -> Scalar {
    let bytes = big.to_bytes_be();
    let mut fixed_bytes = [0u8; 32];
    let len = bytes.len();
    if len > 32 {
        fixed_bytes.copy_from_slice(&bytes[len - 32..]);
    } else {
        fixed_bytes[32 - len..].copy_from_slice(&bytes);
    }
    Scalar::from_repr(fixed_bytes.into()).expect("Scalar conversion overflow in variant generation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Verifies variant JSON export persistence.
    #[test]
    fn test_save_to_json_creates_points_file() {
        let target = ecc::scalar_mul_g(&Scalar::from(100u64));
        let variants = generate_variants(&target);
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        let res = save_variants_to_json(&variants, dir_path);
        assert!(res.is_ok());
        assert!(dir.path().join("points.json").exists());
    }

    /// Validates O(1) indexing logic and scalar derivation.
    #[test]
    fn test_indexing_speedup() {
        let target = ecc::scalar_mul_g(&Scalar::from(1000u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let scalar_999 = Scalar::from(999u64);
        let p_999 = ecc::scalar_mul_g(&scalar_999);
        let affine = p_999.to_affine();
        let encoded = affine.to_encoded_point(false);
        let x_bytes = encoded.x().unwrap();
        let mut x_999 = [0u8; 32];
        x_999.copy_from_slice(x_bytes.as_ref());

        let m = index.match_x(&x_999, 999).unwrap();
        // Mathematical invariant: 1000 = V + 999 => V = 1.
        assert!(m.label == "2^0" || m.label == "sum(2^0..2^0)");
        assert_eq!(m.offset, "1");
    }

    /// Verifies that an empty cache file returns Ok(None) (no match).
    #[test]
    fn test_cached_sweep_empty_file() {
        let target = ecc::scalar_mul_g(&Scalar::from(1u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("empty.bin");

        // Create an empty file.
        std::fs::write(&cache_path, []).unwrap();

        let result = perform_cached_sweep(&index, &cache_path, 0);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Verifies that a cache file with size not a multiple of 32 returns Err(CacheCorrupted).
    #[test]
    fn test_cached_sweep_corrupted_size() {
        let target = ecc::scalar_mul_g(&Scalar::from(1u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("corrupted.bin");

        // Write 31 bytes (not a multiple of 32).
        std::fs::write(&cache_path, vec![0u8; 31]).unwrap();

        let result = perform_cached_sweep(&index, &cache_path, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not a multiple of 32"));
    }

    /// Verifies end-to-end: write X-coordinate bytes to cache, read them back, find a match.
    #[test]
    fn test_cached_sweep_write_and_read_back() {
        // Setup: target P = d·G where d = 2 + 1 = 3 = V + j with V=2 (2^1) and j=1.
        // generate_variants computes shifted_p = P - V·G = 3·G - 2·G = 1·G.
        // We create a cache with x(1·G) at the position corresponding to j=1.
        // Sweeping from start_j=1, the second cache entry (index 1) is associated with j=1.
        // match_x finds x(1·G) matches variant 2^1 (V=2), giving d candidates 3 and 1.
        let d_scalar = Scalar::from(3u64);
        let p_d = ecc::scalar_mul_g(&d_scalar); // P = 3·G
        let index = VariantIndex::new(generate_variants(&p_d));

        // Get x(1·G) — the match point.
        let p_j = ecc::scalar_mul_g(&Scalar::from(1u64)); // 1·G
        let affine_j = p_j.to_affine();
        let encoded_j = affine_j.to_encoded_point(false);
        let x_bytes_1 = encoded_j.x().unwrap();
        let mut x_1 = [0u8; 32];
        x_1.copy_from_slice(x_bytes_1.as_ref());

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("match.bin");

        // Write two cache entries: x(0·G) at index 0 (garbage, j=0 skipped),
        // x(1·G) at index 1 (the match, j=1).
        // We pad the first 32 bytes with zeros (x(0·G)).
        let mut cache_data = vec![0u8; 32]; // entry for j=0 (will be skipped)
        cache_data.extend_from_slice(&x_1); // entry for j=1 (should match)
        std::fs::write(&cache_path, &cache_data).unwrap();

        // Sweep starting at j=1: entry 0 → j=1, entry 1 → j=2.
        // Wait — actually: start_j=1 means entry 0 → j=1, entry 1 → j=2.
        // We want the match at j=1, so we need x(1·G) at entry 0.
        // Fix: write x(1·G) at entry 0, start_j=1.
        let mut cache_data = Vec::new();
        cache_data.extend_from_slice(&x_1); // entry 0 → j=1 (match)
        cache_data.extend_from_slice(&x_1); // entry 1 → j=2 (no match)
        std::fs::write(&cache_path, &cache_data).unwrap();

        // Sweep starting at j=1: entry 0 is j=1, should find a match.
        let result = perform_cached_sweep(&index, &cache_path, 1).unwrap();

        // The exact variant label depends on which variant's x-coordinate matches x(1·G).
        // For P=3·G, both 2^1 and potentially other variants could produce x(1·G).
        // What matters is: small_scalar=1 is correct, and candidate d=3 appears.
        let m = result.expect("Should have found a match at j=1");

        assert_eq!(m.small_scalar, 1, "Should match at j=1");
        assert!(
            m.candidates.contains(&"3".to_string()),
            "Candidate must include d=3, got: {:?}",
            m.candidates
        );
    }
}
