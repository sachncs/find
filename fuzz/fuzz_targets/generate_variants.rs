// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: `generate_variants` invariant.
//!
//! For a random target public key, asserts that:
//! 1. The result has at most 512 entries (variants collapsed to the
//!    identity are skipped).
//! 2. Every produced variant has a non-zero X-coordinate.
//! 3. The label string is well-formed ("2^{i}" or "sum(2^0..2^{i})").
//! 4. All produced X-coordinates are distinct.

#![no_main]

use find::ecc;
use find::search::generate_variants;
use k256::elliptic_curve::group::Curve;
use k256::Scalar;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Use the fuzz input as the scalar offset for a private key.
    let mut bytes = [0u8; 32];
    let len = data.len().min(32);
    bytes[32 - len..].copy_from_slice(&data[..len]);

    let opt: Option<k256::Scalar> =
        Option::from(k256::elliptic_curve::PrimeField::from_repr(bytes.into()));
    let Some(scalar) = opt else { return };
    let target = ecc::scalar_mul_g(&scalar);

    let variants = generate_variants(&target);

    // (1) Upper bound on variant count.
    assert!(
        variants.len() <= 512,
        "variant count {} exceeds 512",
        variants.len()
    );

    // (2) Non-zero X-coordinate.
    for v in &variants {
        assert_ne!(v.x_bytes, [0u8; 32], "variant {} has zero X", v.label);
    }

    // (3) Well-formed label.
    for v in &variants {
        let ok = v.label.starts_with("2^") || v.label.starts_with("sum(2^0..2^)");
        assert!(ok, "variant label not well-formed: {}", v.label);
    }

    // (4) Distinct X-coordinates.
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(
                variants[i].x_bytes, variants[j].x_bytes,
                "duplicate X-coord at indices {i} and {j}"
            );
        }
    }
});