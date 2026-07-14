// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: round-trip `scalar_mul_g` followed by `parse_pubkey`.
//!
//! For a random scalar `d`, computes `P = d · G` and serialises it
//! with the SEC1 compressed encoding; then parses the hex string back
//! with [`find::ecc::parse_pubkey`] and asserts equality. This catches
//! round-trip bugs in the public-key pipeline.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Pad or truncate to 32 bytes.
    let mut bytes = [0u8; 32];
    let len = data.len().min(32);
    bytes[32 - len..].copy_from_slice(&data[..len]);

    let opt: Option<k256::Scalar> =
        Option::from(k256::elliptic_curve::PrimeField::from_repr(bytes.into()));
    if let Some(scalar) = opt {
        let p = find::ecc::scalar_mul_g(&scalar);

        // Compressed SEC1 (33 bytes: 0x02|0x03 + 32-byte X).
        let encoded = p.to_affine().to_encoded_point(true);
        let hex = hex::encode(encoded.as_bytes());

        // Round-trip via parse_pubkey.
        let parsed = find::ecc::parse_pubkey(&hex)
            .expect("scalar_mul_g -> parse_pubkey round-trip must succeed");

        // The two representations must agree.
        assert_eq!(
            parsed, p,
            "round-trip mismatch: parse_pubkey of compress(d·G) != d·G"
        );
    }
});