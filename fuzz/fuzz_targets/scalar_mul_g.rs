// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: `scalar_mul_g` on a scalar derived from random bytes.
//!
//! Verifies that scalar multiplication never panics, even for edge-case
//! scalars (zero, n-1, n, n+1, etc.).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Pad or truncate the input to 32 bytes for Scalar::from_repr.
    let mut bytes = [0u8; 32];
    let len = data.len().min(32);
    bytes[32 - len..].copy_from_slice(&data[..len]);

    let opt: Option<k256::Scalar> =
        Option::from(k256::elliptic_curve::PrimeField::from_repr(bytes.into()));
    if let Some(scalar) = opt {
        let _p = find::ecc::scalar_mul_g(&scalar);
        // If the scalar is valid, the result must be a valid curve point.
        // For this test we just verify the call doesn't panic.
    }
});
