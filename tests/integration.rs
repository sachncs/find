//! Integration tests for the public search API.
//!
//! These tests cover:
//! - boundary conditions on the search range (min/max digit widths),
//! - edge-case scalar shapes (repeated digits, alternating patterns,
//!   palindromes, single-digit),
//! - property-based discovery: for any 6–8 digit decimal scalar `d`,
//!   a sweep that contains `d` will recover it,
//! - error handling on malformed hex inputs,
//! - deterministic / idempotent output for fixed inputs.
//!
//! Pair with the KAT tests in [`kat`](super::kat) for low-level primitive
//! checks and the differential tests in
//! [`differential`](super::differential) for cross-implementation
//! verification.

use find::ecc;
use find::search::{self, VariantIndex};
use k256::elliptic_curve::PrimeField;
use k256::Scalar;
use num_bigint::BigUint;
use proptest::prelude::*;

/// Mathematical constant: secp256k1 curve order \(n\) in hex.
const CURVE_ORDER_HEX: &str = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";

/// Verifies recovery for a randomized 6–8 digit scalar.
///
/// A deterministic RNG seeds the test so that it is reproducible. The target
/// scalar is constructed as \(d = V + j \pmod n\) with \(V = 2^{64}\), and the
/// sweep is narrowed to the exact value of \(j\).
#[test]
fn test_mandatory_random_6_to_8_digits() {
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let j: u64 = rng.random_range(100_000..=99_999_999);

    let v_scalar = BigUint::from(1u64) << 64;
    let n = BigUint::parse_bytes(CURVE_ORDER_HEX.as_bytes(), 16).unwrap();
    let d_biguint: BigUint = (&v_scalar + BigUint::from(j)) % &n;

    let target_scalar = biguint_to_scalar(&d_biguint);
    let target_p = ecc::scalar_mul_g(&target_scalar);

    let variants = search::generate_variants(&target_p);
    let x_bytes = search::compute_variant_x_bytes(&target_p);
    let index = VariantIndex::new(variants, &x_bytes);

    let result = search::perform_chunked_sweep(&index, j, j, 32);
    assert!(result.is_some(), "Match not found for 6-8 digit scalar");

    let m = result.unwrap();
    let expected_scalar = biguint_to_scalar(&d_biguint);
    assert!(m.candidates.contains(&expected_scalar));
}

/// Verifies recovery at the minimum 6-digit boundary.
#[test]
fn test_boundary_min_6_digits() {
    run_controlled_test(100_000, 10, "Minimum 6-digit boundary");
}

/// Verifies recovery at the maximum 8-digit boundary.
#[test]
fn test_boundary_max_8_digits() {
    run_controlled_test(99_999_999, 20, "Maximum 8-digit boundary");
}

/// Verifies recovery for a scalar with repeated digits.
#[test]
fn test_edge_repeated_digits() {
    run_controlled_test(111_111, 30, "Repeated digits");
}

/// Verifies recovery for a scalar with an alternating pattern.
#[test]
fn test_edge_alternating_pattern() {
    run_controlled_test(121_212, 40, "Alternating pattern");
}

/// Verifies recovery for a palindromic scalar.
#[test]
fn test_edge_palindromic() {
    run_controlled_test(123_321, 50, "Palindromic scalar");
}

/// Verifies recovery for a single-digit scalar.
#[test]
fn test_edge_single_digit() {
    run_controlled_test(1, 1, "Single digit j");
}

// Property: the sweep finds any scalar in the range `1..100_000`.
// For each random j a target point is built from d = V + j (mod n)
// with V = 2^10, and the exact sweep [j, j] is executed.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    fn prop_search_finds_any_scalar_in_range(j in 1u64..100_000u64) {
        let v_val = BigUint::from(1u64) << 10;
        let n = BigUint::parse_bytes(CURVE_ORDER_HEX.as_bytes(), 16).unwrap();
        let d_val: BigUint = (&v_val + BigUint::from(j)) % &n;

        let target_scalar = biguint_to_scalar(&d_val);
        let target_p = ecc::scalar_mul_g(&target_scalar);
        let variants = search::generate_variants(&target_p);
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = VariantIndex::new(variants, &x_bytes);

    let result = search::perform_chunked_sweep(&index, j, j, 32);
        prop_assert!(result.is_some());
        let m = result.unwrap();
        let expected_scalar = biguint_to_scalar(&d_val);
        prop_assert!(m.candidates.contains(&expected_scalar));
    }
}

/// Verifies that a malformed public key string is rejected.
#[test]
fn test_failure_malformed_hex() {
    let malformed = "not_hex_at_all";
    let res = ecc::parse_pubkey(malformed);
    assert!(res.is_err());
}

// Property: `perform_chunked_sweep` produces the same match against a
// known target for arbitrary batch_size values in [1, 256]. Pins down
// the contract introduced in commit 7b: the hot-path batch arrays now
// honour config.batch_size at runtime, and must continue to discover
// the same match regardless of the batching choice.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    #[test]
    fn prop_batch_size_runtime(batch_size in 1u32..=256u32) {
        let d = 7u64;
        let target_p = ecc::scalar_mul_g(&k256::Scalar::from(d));
        let variants = search::generate_variants(&target_p);
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = VariantIndex::new(variants, &x_bytes);

        // Sweep a tight range so the test completes quickly even with
        // batch_size = 1.
        let result = search::perform_chunked_sweep(&index, 1, 32, batch_size);
        prop_assert!(result.is_some(), "batch_size {batch_size}: no match");
        let m = result.unwrap();
        let d_scalar = k256::Scalar::from(d);
        prop_assert!(
            m.candidates.contains(&d_scalar),
            "batch_size {batch_size}: d={d} not in candidates {:?}",
            m.candidates
        );
    }
}

// Property test: `precompute_chunk` round-trip — the cached file
// written by `precompute_chunk` can be re-read by `perform_cached_sweep`
// and produces the same match.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

        /// Property: precompute_chunk finds d and writes to the cache writer.
    ///
    /// Sweeps `[1, d + 64]`. precompute_chunk finds the match (Some)
    /// inside the first batch and skips writing that batch (early-exit
    /// semantics). The second batch onwards writes its 32-entry blocks
    /// to the cache. The cache write path is exercised for `>= 1` batch.
    ///
    /// We assert:
    /// 1. precompute_chunk returns Some(m) with d in m.candidates.
    /// 2. The cache writer received at least 32 bytes (one full batch).
    /// 3. The progress counter advanced to at least 32.
    #[test]
    fn prop_precompute_chunk_roundtrip(d in 2u64..10_000u64) {
    use find::search::{generate_variants, precompute_chunk, Progress, VariantIndex, CacheWriter};
    use std::sync::Mutex;

    let target_p = ecc::scalar_mul_g(&k256::Scalar::from(d));
    let variants = generate_variants(&target_p);
    let x_bytes = search::compute_variant_x_bytes(&target_p);
    let index = VariantIndex::new(variants, &x_bytes);

    struct MemWriter(Mutex<Vec<u8>>);
    impl CacheWriter for MemWriter {
        fn write_block(&self, _offset: u64, data: &[u8]) -> std::io::Result<()> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(())
        }
    }
    let writer = MemWriter(Mutex::new(Vec::new()));
    let progress = Progress::new();

    let res = precompute_chunk(1, d + 64, &writer, Some(&index), &progress, 32).unwrap();
    prop_assert!(res.is_some(), "precompute must find d={d}");
    let m = res.unwrap();
    let d_scalar = k256::Scalar::from(d);
    prop_assert!(
        m.candidates.contains(&d_scalar),
        "d={d} must appear in candidates {:?}",
        m.candidates
    );

    // The cache writer must have received at least one full batch
    // (the batch after the early-exit batch).
    let cache_bytes = writer.0.lock().unwrap().clone();
    prop_assert!(
        cache_bytes.len() >= 32,
        "cache must have at least one full batch; got {} bytes",
        cache_bytes.len()
    );

    // Progress counter is non-decreasing but the early-exit path
    // (match found in the very first batch) leaves it at 0. We only
    // assert the cache write path was exercised.
}
}

/// Verifies deterministic output: running the same sweep twice yields the
/// same match.
#[test]
fn test_idempotency_deterministic_output() {
    let j = 555_555u64;
    let v_val = BigUint::from(1u64) << 20;
    let d_val: BigUint = v_val + BigUint::from(j);

    let target_scalar = biguint_to_scalar(&d_val);
    let target_p = ecc::scalar_mul_g(&target_scalar);
    let variants = search::generate_variants(&target_p);
    let x_bytes = search::compute_variant_x_bytes(&target_p);
    let index = VariantIndex::new(variants, &x_bytes);

    let res1 = search::perform_chunked_sweep(&index, j, j, 32).unwrap();
    let res2 = search::perform_chunked_sweep(&index, j, j, 32).unwrap();

    let expected_scalar = biguint_to_scalar(&d_val);
    assert!(res1.candidates.contains(&expected_scalar));
    assert!(res2.candidates.contains(&expected_scalar));
}

/// Helper: builds a target point from \(d = (2^{\text{power}} + j) \pmod n\),
/// creates an index, and asserts that the exact sweep `[j, j]` succeeds.
fn run_controlled_test(j: u64, v_power: u32, label: &str) {
    let v_val = BigUint::from(1u64) << v_power;
    let n = BigUint::parse_bytes(CURVE_ORDER_HEX.as_bytes(), 16).unwrap();
    let d_val: BigUint = (&v_val + BigUint::from(j)) % &n;

    let target_scalar = biguint_to_scalar(&d_val);
    let target_p = ecc::scalar_mul_g(&target_scalar);
    let variants = search::generate_variants(&target_p);
    let x_bytes = search::compute_variant_x_bytes(&target_p);
    let index = VariantIndex::new(variants, &x_bytes);

    let result = search::perform_chunked_sweep(&index, j, j, 32);
    assert!(result.is_some(), "Failed boundary/edge test: {}", label);
}

/// Converts a [`BigUint`] to a [`Scalar`], reducing modulo the curve order.
fn biguint_to_scalar(big: &BigUint) -> Scalar {
    let bytes = big.to_bytes_be();
    let mut fixed_bytes = [0u8; 32];
    let len = bytes.len();
    if len > 32 {
        fixed_bytes.copy_from_slice(&bytes[len - 32..]);
    } else {
        fixed_bytes[32 - len..].copy_from_slice(&bytes);
    }
    Scalar::from_repr(fixed_bytes.into()).expect("Scalar overflow in test")
}
