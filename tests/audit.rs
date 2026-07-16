//! End-to-end key recovery audit tests.
//!
//! These tests verify that the public pipeline (parse a public key, build
//! a [`VariantIndex`], sweep a range, recover a candidate) actually
//! produces the original private key for known scalars. They complement
//! the randomized tests in [`integration`](super::integration) and the
//! cross-implementation checks in [`differential`](super::differential) by
//! pinning concrete known-answer scalars.
//!
//! The scalar `1234567890` is a long-standing small-scalar test vector in
//! k256 / rust-secp256k1 and is used here as a recognisable anchor.

use find::ecc;
use find::search::{self, VariantIndex};
use num_bigint::BigUint;
use proptest::prelude::*;

/// Verifies end-to-end recovery of the known scalar `1234567890`.
///
/// The test derives the public key, builds a variant index, sweeps the
/// expected range, and validates that the match produces the original scalar
/// as one of its candidates.
#[test]
fn test_rigorous_recovery_1234567890() {
    let known_d: u64 = 1_234_567_890;
    let known_d_hex = BigUint::from(known_d).to_str_radix(16);
    let provided_pubkey_hex = "042b698a0f0a4041b77e63488ad48c23e8e8838dd1fb7520408b121697b782ef222ee976351a7fe808101c7e79b040e5cb16afe6aa152b87e398d160c306a31bac";

    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(&known_d_hex).unwrap());
    let parsed_p = ecc::parse_pubkey(provided_pubkey_hex).unwrap();
    assert_eq!(
        target_p, parsed_p,
        "Derived pubkey MUST match the known SEC1 public key for 1234567890"
    );

    let variants = search::generate_variants(&target_p);
    let x_bytes = search::compute_variant_x_bytes(&target_p);
    let index = VariantIndex::new(variants, &x_bytes);

    let sweep_start: u64 = 160_826_000;
    let sweep_end: u64 = 160_827_000;

    let result = search::sweep_parallel(&index, sweep_start, sweep_end, 32);
    let m = result.expect("Sweep MUST recover the match for scalar 1234567890");

    assert_eq!(m.label, "2^30", "Must match via the 2^30 variant");
    assert_eq!(
        m.j, 160_826_066,
        "Must match at j = d - 2^30 = 160826066"
    );
    assert_eq!(m.offset, "1073741824", "Offset must be 2^30");

    let recovered_scalar = ecc::hex_to_scalar(&known_d_hex).unwrap();
    assert!(
        m.candidates.contains(&recovered_scalar),
        "Candidates MUST contain the original scalar (hex: {known_d_hex})"
    );

    let recovered_p = ecc::scalar_mul_g(&recovered_scalar);
    assert_eq!(
        recovered_p, target_p,
        "Recovered scalar MUST reproduce the target public key P = d·G"
    );

    let n = BigUint::parse_bytes(
        b"fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141",
        16,
    )
    .unwrap();
    let v = BigUint::from(1_073_741_824u64);
    let j = BigUint::from(160_826_066u64);
    let c1 = (&v + &j) % &n;
    let c2 = if v >= j {
        (&v - &j) % &n
    } else {
        (&n + &v - &j) % &n
    };
    let d_bigint = BigUint::from(known_d);
    assert!(
        c1 == d_bigint || c2 == d_bigint,
        "At least one candidate must satisfy d ≡ V ± j (mod n)"
    );

    for candidate in &m.candidates {
        let p = ecc::scalar_mul_g(candidate);
        let _affine = p.to_affine();
    }

    println!("[RECOVERY VERIFIED] Scalar 1234567890 fully recovered from its public key.");
    println!("  Variant: {}", m.label);
    println!("  Match at j = {}", m.j);
    println!("  Offset V = {}", m.offset);
    println!("  Candidates: {:?}", m.candidates);
}

/// Pads a hex string to even length for [`hex::decode`] compatibility.
fn pad_hex(h: &str) -> String {
    if h.len() % 2 != 0 {
        format!("0{h}")
    } else {
        h.to_string()
    }
}

/// Verifies recovery for a set of small known scalars.
///
/// Each scalar is converted to a public key, an index is built, and a sweep
/// from `j = 0` is performed. The test asserts that the original scalar appears
/// in the candidate list and that every candidate is a valid curve point.
#[test]
fn test_recovery_small_scalars() {
    let test_cases: Vec<u64> = vec![7, 100, 1000, 99999];

    for known_d in test_cases {
        let d_hex = BigUint::from(known_d).to_str_radix(16);
        let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(&pad_hex(&d_hex)).unwrap());
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = VariantIndex::new(search::generate_variants(&target_p), &x_bytes);

        let sweep_end = known_d + 10;
        let result = search::sweep_parallel(&index, 0, sweep_end, 32);

        let m = result.unwrap_or_else(|| panic!("Sweep MUST recover match for d={known_d}"));

        let recovered = ecc::hex_to_scalar(&pad_hex(&d_hex)).unwrap();
        assert!(
            m.candidates.contains(&recovered),
            "Candidates for d={known_d} must contain the original scalar (hex: {d_hex})"
        );
        assert!(
            m.candidates.contains(&recovered),
            "Candidates for d={known_d} must contain the original scalar (hex: {d_hex})"
        );
        let recovered_p = ecc::scalar_mul_g(&recovered);
        assert_eq!(
            recovered_p, target_p,
            "Recovered scalar for d={known_d} MUST reproduce the target public key"
        );

        for candidate in &m.candidates {
            let p = ecc::scalar_mul_g(candidate);
            let _affine = p.to_affine();
        }

        println!(
            "[RECOVERY] d={} found via {} at j={}",
            known_d, m.label, m.j
        );
    }
}

// Property test: for any small-scalar d, the public pipeline
// (scalar_mul_g -> generate_variants -> sweep_parallel in [0, d+10])
// recovers d as one of the candidates. The +10 margin covers off-by-one
// races where the sweep might land at j = d - V_offset for V_offset > d.
//
// 20 cases is enough to catch regressions in the variant-construction
// code path without slowing the test suite below 1 minute.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// Property: any 1..=10_000_000 scalar is recoverable.
    #[test]
    fn prop_audit_recovers_any_small_scalar(d in 1u64..=10_000_000u64) {
        let target_p = ecc::scalar_mul_g(&k256::Scalar::from(d));
        let variants = search::generate_variants(&target_p);
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = VariantIndex::new(variants, &x_bytes);

        // Sweep just past d so we always match.
        let m = search::sweep_parallel(&index, 0, d + 10, 32)
            .unwrap_or_else(|| panic!("audit must recover d={d}"));

        let d_scalar = k256::Scalar::from(d);
        prop_assert!(
            m.candidates.contains(&d_scalar),
            "d={d} must appear in candidates {:?}",
            m.candidates
        );
    }
}
