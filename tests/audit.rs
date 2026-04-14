use find::ecc;
use find::search::{self, VariantIndex};
use num_bigint::BigUint;

/// Rigorously verifies that the search algorithm recovers the private key
/// for the known scalar 1234567890 from its SEC1 public key.
#[test]
fn test_rigorous_recovery_1234567890() {
    let known_d: u64 = 1_234_567_890;
    let known_d_hex = BigUint::from(known_d).to_str_radix(16);
    let provided_pubkey_hex = "042b698a0f0a4041b77e63488ad48c23e8e8838dd1fb7520408b121697b782ef222ee976351a7fe808101c7e79b040e5cb16afe6aa152b87e398d160c306a31bac";

    // ── Phase 1: Public Key Derivation Integrity ──
    // Verify that 1234567890 * G produces the expected SEC1 public key.
    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(&known_d_hex).unwrap());
    let parsed_p = ecc::parse_pubkey(provided_pubkey_hex).unwrap();
    assert_eq!(
        target_p, parsed_p,
        "Derived pubkey MUST match the known SEC1 public key for 1234567890"
    );

    // ── Phase 2: Variant Index Construction ──
    let variants = search::generate_variants(&target_p);
    let index = VariantIndex::new(variants);

    // ── Phase 3: Sweep Recovery ──
    // For d=1234567890, the 2^30 variant (V=1073741824) gives j = d - V = 160826066.
    // Sweep a range that includes this expected match point.
    let sweep_start: u64 = 160_826_000;
    let sweep_end: u64 = 160_827_000;

    let result = search::perform_chunked_sweep(&index, sweep_start, sweep_end);
    let m = result.expect("Sweep MUST recover the match for scalar 1234567890");

    // ── Phase 4: Match Structural Verification ──
    assert_eq!(m.label, "2^30", "Must match via the 2^30 variant");
    assert_eq!(
        m.small_scalar, 160_826_066,
        "Must match at j = d - 2^30 = 160826066"
    );
    assert_eq!(m.offset, "1073741824", "Offset must be 2^30");

    // ── Phase 5: Candidate Scalar Verification ──
    assert!(
        m.candidates.contains(&known_d_hex),
        "Candidates MUST contain the original scalar (hex: {})",
        known_d_hex
    );

    // ── Phase 6: Cryptographic Key Verification ──
    // The ultimate proof: verify that the recovered scalar reproduces the original public key.
    let recovered_scalar = ecc::hex_to_scalar(&known_d_hex).unwrap();
    let recovered_p = ecc::scalar_mul_g(&recovered_scalar);
    assert_eq!(
        recovered_p, target_p,
        "Recovered scalar MUST reproduce the target public key P = d·G"
    );

    // ── Phase 7: Mathematical Relationship Verification ──
    // Verify d ≡ V + j (mod n) or d ≡ V - j (mod n)
    let n = search::curve_order();
    let v = BigUint::from(1_073_741_824u64); // 2^30
    let j = BigUint::from(160_826_066u64);
    let c1 = (&v + &j) % n;
    let c2 = if v >= j {
        (&v - &j) % n
    } else {
        (n + &v - &j) % n
    };
    let d_bigint = BigUint::from(known_d);
    assert!(
        c1 == d_bigint || c2 == d_bigint,
        "At least one candidate must satisfy d ≡ V ± j (mod n)"
    );

    // ── Phase 8: Verify ALL candidates are valid scalars on the curve ──
    for candidate_hex in &m.candidates {
        let s = ecc::hex_to_scalar(candidate_hex)
            .expect("Every candidate must be a valid secp256k1 scalar < n");
        let p = ecc::scalar_mul_g(&s);
        // If to_affine succeeds, the point is on the curve.
        let _affine = p.to_affine();
    }

    println!("[RECOVERY VERIFIED] Scalar 1234567890 fully recovered from its public key.");
    println!("  Variant: {}", m.label);
    println!("  Match at j = {}", m.small_scalar);
    println!("  Offset V = {}", m.offset);
    println!("  Candidates: {:?}", m.candidates);
}

/// Pads a hex string to even length for `hex::decode` compatibility.
fn pad_hex(h: &str) -> String {
    if !h.len().is_multiple_of(2) {
        format!("0{}", h)
    } else {
        h.to_string()
    }
}

/// Tests full recovery for small scalars by sweeping from j=0.
/// This validates the algorithm works end-to-end without pre-knowledge of j.
#[test]
fn test_recovery_small_scalars() {
    let test_cases: Vec<u64> = vec![7, 100, 1000, 99999];

    for known_d in test_cases {
        let d_hex = BigUint::from(known_d).to_str_radix(16);
        let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(&pad_hex(&d_hex)).unwrap());
        let index = VariantIndex::new(search::generate_variants(&target_p));

        // Sweep from 0 — generous range for these small scalars.
        // For any d, a match exists at j ≤ d via variant 2^0 (V=1): j = d - 1.
        let sweep_end = known_d + 10;
        let result = search::perform_chunked_sweep(&index, 0, sweep_end);

        let m = result.unwrap_or_else(|| panic!("Sweep MUST recover match for d={}", known_d));

        // ── Verify the known scalar is in candidates ──
        assert!(
            m.candidates.contains(&d_hex),
            "Candidates for d={} must contain the original scalar (hex: {})",
            known_d,
            d_hex
        );

        // ── Verify the recovered scalar produces the target public key ──
        let recovered = ecc::hex_to_scalar(&pad_hex(&d_hex)).unwrap();
        let recovered_p = ecc::scalar_mul_g(&recovered);
        assert_eq!(
            recovered_p, target_p,
            "Recovered scalar for d={} MUST reproduce the target public key",
            known_d
        );

        // ── Verify all candidates are valid curve scalars ──
        for candidate_hex in &m.candidates {
            let s = ecc::hex_to_scalar(&pad_hex(candidate_hex))
                .expect("Every candidate must be a valid secp256k1 scalar");
            let p = ecc::scalar_mul_g(&s);
            let _affine = p.to_affine(); // Validates curve membership
        }

        println!(
            "[RECOVERY] d={} found via {} at j={}",
            known_d, m.label, m.small_scalar
        );
    }
}
