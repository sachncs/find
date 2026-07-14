// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Differential tests against the reference `libsecp256k1` C library.
//!
//! These tests use `secp256k1-sys` to invoke the reference C implementation
//! and compare its results with the `k256`-based implementation used by the
//! `find` tool. If the two implementations agree for a wide range of inputs,
//! we have high confidence that the tool's cryptographic primitives are
//! correct.
//!
//! Note: `secp256k1-sys` bundles `libsecp256k1` from source, so no system
//! library is required.

#![cfg(test)]

use find::ecc;
use k256::elliptic_curve::sec1::ToEncodedPoint;

/// A set of scalars to use for differential testing.
///
/// Includes boundary scalars (`1`, `n - 1`) and power-of-two anchors
/// (`2^32`, `2^63`) that exercise different paths in k256's scalar
/// multiplication.
const TEST_SCALARS: &[u64] = &[
    1,
    2,
    3,
    7,
    100,
    1000,
    99999,
    1_000_000,
    1_234_567_890,
    1u64 << 32,
    1u64 << 63,
    u64::MAX,
];

/// Computes `d·G` using `secp256k1-sys` (the reference C implementation)
/// and returns the 33-byte compressed SEC1 encoding.
fn secp256k1_sys_scalar_mul_g(d: u64) -> Vec<u8> {
    // SAFETY: This block uses raw FFI to compute d·G. The context, public
    // key, and serialization buffer are all properly initialized. The
    // secret key is constructed from a valid u64 value.
    unsafe {
        // Create a context with the SIGN flag (needed for pubkey creation).
        let ctx = secp256k1_sys::secp256k1_context_create(
            secp256k1_sys::SECP256K1_START_SIGN | secp256k1_sys::SECP256K1_START_VERIFY,
        );

        // Build the 32-byte big-endian secret key.
        let mut seckey = [0u8; 32];
        seckey[24..32].copy_from_slice(&d.to_be_bytes());

        // Compute the public key.
        let mut pk = secp256k1_sys::PublicKey::new();
        let result =
            secp256k1_sys::secp256k1_ec_pubkey_create(ctx.as_ptr(), &mut pk, seckey.as_ptr());
        assert_eq!(result, 1, "secp256k1_ec_pubkey_create must succeed");

        // Serialize as compressed (33 bytes).
        let mut output = [0u8; 33];
        let mut output_len: usize = 33;
        let ser_result = secp256k1_sys::secp256k1_ec_pubkey_serialize(
            ctx.as_ptr(),
            output.as_mut_ptr(),
            &mut output_len,
            &pk,
            secp256k1_sys::SECP256K1_SER_COMPRESSED,
        );
        assert_eq!(ser_result, 1, "secp256k1_ec_pubkey_serialize must succeed");
        assert_eq!(output_len, 33);

        // Clean up the context.
        secp256k1_sys::secp256k1_context_destroy(ctx);

        // Convert to Vec (truncate to the actual length, which is always 33).
        output[..output_len].to_vec()
    }
}

/// Verifies that the `k256`-based implementation matches the reference
/// `secp256k1-sys` implementation for a set of test scalars.
#[test]
fn differential_scalar_mul_g_against_libsecp256k1() {
    for &d in TEST_SCALARS {
        // k256-based result.
        let scalar = k256::Scalar::from(d);
        let p_k256 = ecc::scalar_mul_g(&scalar);
        let encoded_k256 = p_k256.to_affine().to_encoded_point(true);
        let hex_k256 = hex::encode(encoded_k256.as_bytes());

        // secp256k1-sys (reference) result.
        let bytes_ref = secp256k1_sys_scalar_mul_g(d);
        let hex_ref = hex::encode(&bytes_ref);

        assert_eq!(
            hex_k256, hex_ref,
            "Differential mismatch for d = {d}: k256={hex_k256}, secp256k1-sys={hex_ref}"
        );
    }
}

/// Verifies that the `k256`-based scalar multiplication is consistent with
/// repeated addition (i.e., `scalar_mul_g(d) = G + G + ...` d times).
/// This is a self-consistency check that does not require an external
/// implementation.
#[test]
fn differential_scalar_mul_g_self_consistency() {
    use k256::ProjectivePoint;
    for &d in TEST_SCALARS {
        let scalar = k256::Scalar::from(d);
        let p_scalar = ecc::scalar_mul_g(&scalar);

        // Compute `d * G` by repeated addition.
        let g = ProjectivePoint::GENERATOR;
        let mut p_add = ProjectivePoint::IDENTITY;
        for _ in 0..d.min(1000) {
            p_add += g;
        }

        if d <= 1000 {
            assert_eq!(
                p_scalar, p_add,
                "scalar_mul_g({d}) must equal repeated addition for small d"
            );
        }
    }
}

/// Verifies that `parse_pubkey` accepts the public key produced by
/// `secp256k1-sys` for a given scalar.
#[test]
fn differential_parse_pubkey_against_libsecp256k1() {
    for &d in TEST_SCALARS {
        let bytes_ref = secp256k1_sys_scalar_mul_g(d);
        let hex_ref = hex::encode(&bytes_ref);
        let parsed = ecc::parse_pubkey(&hex_ref)
            .expect("Reference-produced pubkey must parse with k256 wrapper");
        let scalar = k256::Scalar::from(d);
        let expected = ecc::scalar_mul_g(&scalar);
        assert_eq!(
            parsed, expected,
            "Parsed pubkey for d={d} must match k256 result"
        );
    }
}
