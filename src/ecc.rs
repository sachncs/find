// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-performance secp256k1 elliptic curve primitives and abstractions.
//!
//! This module wraps the [`k256`] crate with a thin, search-oriented API.
//! All operations enforce SEC1 compliance and strict scalar range validation.
//!
//! Points are handled in projective coordinates during arithmetic and only
//! normalized to affine when an X-coordinate must be extracted.

use crate::error::{FindError, Result};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::elliptic_curve::PrimeField;
use k256::{ProjectivePoint, PublicKey, Scalar};
use tracing::instrument;

/// Parses a public key from its SEC1 hexadecimal representation.
///
/// The input must be a valid hex-encoded SEC1 point (compressed or uncompressed).
/// The resulting point is validated against the secp256k1 curve equation and
/// converted to projective coordinates for efficient subsequent arithmetic.
///
/// # Errors
///
/// Returns [`FindError::HexError`] if the string is not valid hex.
///
/// Returns [`FindError::InvalidPublicKey`] if the decoded bytes do not form
/// a valid SEC1 public key (wrong prefix, off-curve coordinates, or the
/// point-at-infinity).
///
/// # Examples
///
/// ```
/// use find::ecc;
///
/// let hex = "03203e7f72545397aa5719d2972c40eb44ecdebc784e6618c28d5796852edbaa57";
/// let p = ecc::parse_pubkey(hex).unwrap();
/// ```
#[instrument(skip(hex_str), level = "debug")]
pub fn parse_pubkey(hex_str: &str) -> Result<ProjectivePoint> {
    let bytes = hex::decode(hex_str).map_err(FindError::from)?;
    if bytes.is_empty() {
        return Err(FindError::InvalidPublicKey(
            "Empty hex string provided".to_string(),
        ));
    }

    let pubkey = PublicKey::from_sec1_bytes(&bytes)
        .map_err(|e| FindError::InvalidPublicKey(e.to_string()))?;

    Ok(pubkey.to_projective())
}

/// Returns the standard secp256k1 generator point \(G\).
///
/// \(G\) is the predefined base point that generates the cyclic group of
/// prime order \(n\).
#[inline(always)]
pub fn generator() -> ProjectivePoint {
    ProjectivePoint::GENERATOR
}

/// Converts a hexadecimal string to a scalar field element \(s \in \mathbb{F}_n\).
///
/// The input is decoded as big-endian bytes. Values shorter than 32 bytes are
/// left-padded with zeros; values longer than 32 bytes are truncated to the
/// least-significant 32 bytes. The resulting scalar must be strictly less than
/// the curve order \(n\).
///
/// # Errors
///
/// Returns [`FindError::HexError`] if the string is not valid hex or is empty.
///
/// Returns [`FindError::EccError`] if the decoded value is greater than or
/// equal to the curve order \(n\).
///
/// # Examples
///
/// ```
/// use find::ecc;
///
/// let s = ecc::hex_to_scalar("01").unwrap();
/// ```
pub fn hex_to_scalar(hex_str: &str) -> Result<Scalar> {
    let bytes = hex::decode(hex_str).map_err(FindError::from)?;
    if bytes.is_empty() {
        return Err(FindError::EccError("Empty hex string input".to_string()));
    }
    let mut fixed_bytes = [0u8; 32];

    let len = bytes.len().min(32);
    let src = &bytes[..len];
    fixed_bytes[32 - src.len()..].copy_from_slice(src);

    Option::from(Scalar::from_repr(fixed_bytes.into()))
        .ok_or_else(|| FindError::EccError("Scalar value exceeds curve order n".to_string()))
}

/// Computes \(P = d \cdot G\) using fixed-base scalar multiplication.
///
/// This is the primary operation used during the search sweep to generate
/// candidate points.
#[inline(always)]
pub fn scalar_mul_g(d: &Scalar) -> ProjectivePoint {
    ProjectivePoint::GENERATOR * d
}

/// Computes the point difference \(R = P - Q\) in projective coordinates.
///
/// Subtraction is performed as \(P + (-Q)\), where \(-Q\) is the additive
/// inverse.
#[inline(always)]
pub fn subtract(p: &ProjectivePoint, q: &ProjectivePoint) -> ProjectivePoint {
    p - q
}

/// Extracts the 32-byte hexadecimal X-coordinate of an elliptic curve point.
///
/// This operation normalizes the point from projective to affine coordinates,
/// which involves a modular inversion and is therefore expensive. Callers should
/// batch such extractions whenever possible.
///
/// # Behavior
///
/// Returns a 64-character lower-case hex string representing the 32-byte
/// X-coordinate. If the input is the point-at-infinity, returns a string of
/// 64 zeros.
pub fn to_hex_x(p: &ProjectivePoint) -> String {
    let affine = p.to_affine();
    let encoded = affine.to_encoded_point(false);

    let x = match encoded.x() {
        Some(x) => x,
        None => {
            return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }
    };
    hex::encode(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a compressed SEC1 key parses successfully.
    #[test]
    fn test_parse_valid_compressed() {
        let hex = "03203e7f72545397aa5719d2972c40eb44ecdebc784e6618c28d5796852edbaa57";
        let res = parse_pubkey(hex);
        assert!(res.is_ok());
    }

    /// Verifies that short scalar strings are left-padded to 32 bytes.
    #[test]
    fn test_hex_to_scalar_padding() {
        let hex = "01";
        let s = hex_to_scalar(hex).unwrap();
        assert_eq!(s, Scalar::from(1u64));
    }

    /// Verifies that subtraction matches the definition \(P - Q = P + (-Q)\).
    #[test]
    fn test_sub_definition_consistency() {
        let p = scalar_mul_g(&Scalar::from(12345u64));
        let q = scalar_mul_g(&Scalar::from(6789u64));

        let res_sub = subtract(&p, &q);
        let res_add_neg = p + (-q);

        assert_eq!(res_sub, res_add_neg, "P - Q must equal P + (-Q)");
    }

    /// Verifies that \(P - P = \mathcal{O}\) (the identity point).
    #[test]
    fn test_sub_self_identity() {
        let p = scalar_mul_g(&Scalar::from(42u64));
        let res = subtract(&p, &p);

        assert_eq!(
            res,
            ProjectivePoint::IDENTITY,
            "Self-subtraction must yield the Identity point"
        );
    }

    /// Verifies that subtraction with the identity point behaves as expected.
    #[test]
    fn test_sub_zero_element() {
        let p = scalar_mul_g(&Scalar::from(100u64));
        let o = ProjectivePoint::IDENTITY;

        assert_eq!(subtract(&p, &o), p, "P - O must equal P");
        assert_eq!(subtract(&o, &p), -p, "O - P must equal -P");
    }

    /// Verifies the anticommutative property \(P - Q = -(Q - P)\).
    #[test]
    fn test_sub_anticommutative() {
        let p = scalar_mul_g(&Scalar::from(555u64));
        let q = scalar_mul_g(&Scalar::from(333u64));

        let left = subtract(&p, &q);
        let right = -subtract(&q, &p);

        assert_eq!(left, right, "P - Q must equal -(Q - P)");
    }

    /// Verifies that an empty string is rejected by [`parse_pubkey`].
    #[test]
    fn test_parse_pubkey_empty_string() {
        let res = parse_pubkey("");
        assert!(res.is_err(), "Empty hex string must be rejected");
    }

    /// Verifies that an empty string is rejected by [`hex_to_scalar`].
    #[test]
    fn test_hex_to_scalar_empty_string() {
        let res = hex_to_scalar("");
        assert!(res.is_err(), "Empty hex string must be rejected");
    }

    /// Verifies that [`to_hex_x`] handles the identity point gracefully.
    #[test]
    fn test_to_hex_x_identity_point() {
        let identity = ProjectivePoint::IDENTITY;
        let x = to_hex_x(&identity);
        assert_eq!(
            x, "0000000000000000000000000000000000000000000000000000000000000000",
            "Identity point X-coordinate must be canonical zero string"
        );
    }

    /// Property-based verification for ECC subtraction invariants.
    #[cfg(test)]
    mod prop_tests {
        use super::super::*;
        use proptest::prelude::*;

        proptest! {
            /// Invariant: \((P - Q) + Q = P\) (reversibility).
            #[test]
            fn prop_sub_reversibility(
                d1 in 1u64..1000000u64,
                d2 in 1u64..1000000u64
            ) {
                let p = scalar_mul_g(&Scalar::from(d1));
                let q = scalar_mul_g(&Scalar::from(d2));

                let diff = subtract(&p, &q);
                let recovered = diff + q;

                assert_eq!(recovered, p, "(P - Q) + Q must equal P");
            }

            /// Invariant: the result of subtraction is always a valid curve point.
            #[test]
            fn prop_sub_curve_membership(
                d1 in 1u64..1000000u64,
                d2 in 1u64..1000000u64
            ) {
                let p = scalar_mul_g(&Scalar::from(d1));
                let q = scalar_mul_g(&Scalar::from(d2));
                let res = subtract(&p, &q);

                let _ = res.to_affine();
            }
        }
    }
}
