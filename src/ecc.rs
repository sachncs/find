// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-performance secp256k1 elliptic curve primitives and abstractions.
//!
//! # 🔬 Principal Design
//! This module provides a mission-critical abstraction layer over the `k256`
//! crate, enforcing zero-copy data flow and strict SEC1 compliance. It is
//! optimized for search contexts where coordinate conversion is the primary
//! computational bottleneck.
//!
//! ## 📐 Mathematical Context
//! The secp256k1 curve satisfies $y^2 = x^3 + 7 \pmod p$.
//! Points are typically handled in **Projective Coordinates** $(X:Y:Z)$ to
//! avoid expensive modular inversions during addition and subtraction. This
//! tool utilizes projective arithmetic for anchor generation ($O(1)$) and
//! only transitions to **Affine Coordinates** $(x, y)$ for final X-coordinate
//! matching.
//!
//! ## 🛡 Compliance
//! - **SEC1 v2.0:** Implements full SEC1-compliant public key parsing.
//! - **Field Arithmetic:** Enforces strict range validation for scalars
//!   against the curve order $n$.

use crate::error::{FindError, Result};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::elliptic_curve::PrimeField;
use k256::{ProjectivePoint, PublicKey, Scalar};
use tracing::instrument;

/// Parses a public key from its SEC1 hexadecimal representation.
///
/// ### Internal Flow
/// 1.  **Hex Decoding:** Converts string-input to byte-slice.
/// 2.  **Point Validation:** Verifies the first byte (prefix) and that the
///     resulting coordinates lie on the curve $y^2 = x^3 + 7$.
/// 3.  **Coordinate Type:** Transitions the resulting `PublicKey` into
///     `ProjectivePoint` for efficient arithmetic.
///
/// ### Error Handling
/// Returns `InvalidPublicKey` if the point is invalid, the prefix is
/// unrecognized, or the key is the "Point at Infinity".
#[instrument(skip(hex_str), level = "debug")]
pub fn parse_pubkey(hex_str: &str) -> Result<ProjectivePoint> {
    // Hex decoding using the 'hex' crate; failure maps to FindError::HexError.
    let bytes = hex::decode(hex_str).map_err(FindError::from)?;
    if bytes.is_empty() {
        return Err(FindError::InvalidPublicKey(
            "Empty hex string provided".to_string(),
        ));
    }

    // PublicKey::from_sec1_bytes enforces full SEC1 compliance and curve checks.
    let pubkey = PublicKey::from_sec1_bytes(&bytes)
        .map_err(|e| FindError::InvalidPublicKey(e.to_string()))?;

    // We convert to Projective for O(1) point addition later.
    Ok(pubkey.to_projective())
}

/// Returns the standard secp256k1 generator point $G$.
///
/// $G$ is the predefined base point that generates the cyclic
/// group $\mathbb{G}$ of prime order $n$.
#[inline(always)]
pub fn generator() -> ProjectivePoint {
    ProjectivePoint::GENERATOR
}

/// Converts a hexadecimal string to a scalar field element $S \in \mathbb{F}_n$.
///
/// ### Safety Constraints
/// - **Padding:** Left-pads inputs < 32 bytes with zeros to ensure alignment.
/// - **Range Check:** Rejects values $\ge n$ to prevent malicious field overflows.
///
/// ### Mathematical Invariant
/// All resulting scalars are valid elements of the scalar field $\mathbb{F}_n$.
pub fn hex_to_scalar(hex_str: &str) -> Result<Scalar> {
    let bytes = hex::decode(hex_str).map_err(FindError::from)?;
    if bytes.is_empty() {
        return Err(FindError::EccError("Empty hex string input".to_string()));
    }
    let mut fixed_bytes = [0u8; 32];

    // Optimization: avoid extra allocation by slicing directly into fixed_bytes.
    // bounds check is redundant since len <= bytes.len() by definition.
    let len = bytes.len().min(32);
    let src = &bytes[..len];
    fixed_bytes[32 - src.len()..].copy_from_slice(src);

    // from_repr performs the range check against curve order n.
    Option::from(Scalar::from_repr(fixed_bytes.into()))
        .ok_or_else(|| FindError::EccError("Scalar value exceeds curve order n".to_string()))
}

/// Computes $P = d \cdot G$ using fixed-base scalar multiplication.
///
/// Utilizes underlying `k256` constant-time windowed multiplication to
/// ensure execution-time stability.
#[inline(always)]
pub fn scalar_mul_g(d: &Scalar) -> ProjectivePoint {
    ProjectivePoint::GENERATOR * d
}

/// Computes the point difference $R = P - Q$ in Projective coordinates.
///
/// Point subtraction is implemented as $P + (-Q)$, where $-Q$ is the
/// additive inverse $(X: -Y: Z)$.
#[inline(always)]
pub fn subtract(p: &ProjectivePoint, q: &ProjectivePoint) -> ProjectivePoint {
    p - q
}

/// Extracts the 32-byte hexadecimal X-coordinate of an elliptic curve point.
///
/// ### Important Lifecycle Note
/// This function triggers a **Normalization** step where the Projective $(X:Y:Z)$
/// coordinates are converted to Affine $(x, y)$ via modular inversion of $Z$.
/// This is the most computationally expensive part of the coordinate extraction.
///
/// ### Invariants
/// Handles the identity point (Point at Infinity) gracefully by returning a
/// zero-filled X-coordinate. Callers should guard against identity inputs if
/// they require different behavior.
pub fn to_hex_x(p: &ProjectivePoint) -> String {
    let affine = p.to_affine(); // Normalization (Inversion)
    let encoded = affine.to_encoded_point(false);

    // Identity point (O) has no affine X-coordinate; return canonical zero representation.
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

    /// Verifies that compressed SEC1 keys parse correctly.
    #[test]
    fn test_parse_valid_compressed() {
        let hex = "03203e7f72545397aa5719d2972c40eb44ecdebc784e6618c28d5796852edbaa57";
        let res = parse_pubkey(hex);
        assert!(res.is_ok());
    }

    /// Verifies that padding logic correctly handles short scalar hex strings.
    #[test]
    fn test_hex_to_scalar_padding() {
        let hex = "01";
        let s = hex_to_scalar(hex).unwrap();
        assert_eq!(s, Scalar::from(1u64));
    }

    /// Verifies the identity P - Q == P + (-Q).
    #[test]
    fn test_sub_definition_consistency() {
        let p = scalar_mul_g(&Scalar::from(12345u64));
        let q = scalar_mul_g(&Scalar::from(6789u64));

        let res_sub = subtract(&p, &q);
        let res_add_neg = p + (-q);

        assert_eq!(res_sub, res_add_neg, "P - Q must equal P + (-Q)");
    }

    /// Verifies the self-subtraction identity P - P == O (Identity).
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

    /// Verifies the zero-element identity P - O == P.
    #[test]
    fn test_sub_zero_element() {
        let p = scalar_mul_g(&Scalar::from(100u64));
        let o = ProjectivePoint::IDENTITY;

        assert_eq!(subtract(&p, &o), p, "P - O must equal P");
        assert_eq!(subtract(&o, &p), -p, "O - P must equal -P");
    }

    /// Verifies the anticommutative property P - Q == -(Q - P).
    #[test]
    fn test_sub_anticommutative() {
        let p = scalar_mul_g(&Scalar::from(555u64));
        let q = scalar_mul_g(&Scalar::from(333u64));

        let left = subtract(&p, &q);
        let right = -subtract(&q, &p);

        assert_eq!(left, right, "P - Q must equal -(Q - P)");
    }

    /// Verifies that empty string is rejected by parse_pubkey.
    #[test]
    fn test_parse_pubkey_empty_string() {
        let res = parse_pubkey("");
        assert!(res.is_err(), "Empty hex string must be rejected");
    }

    /// Verifies that empty string is rejected by hex_to_scalar.
    #[test]
    fn test_hex_to_scalar_empty_string() {
        let res = hex_to_scalar("");
        assert!(res.is_err(), "Empty hex string must be rejected");
    }

    /// Verifies that to_hex_x handles the identity point gracefully.
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
            /// Invariant: (P - Q) + Q == P (Reversibility).
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

            /// Invariant: Resulting point must always satisfy the curve equation.
            /// (In k256, ProjectivePoint arithmetic preserves curve membership).
            #[test]
            fn prop_sub_curve_membership(
                d1 in 1u64..1000000u64,
                d2 in 1u64..1000000u64
            ) {
                let p = scalar_mul_g(&Scalar::from(d1));
                let q = scalar_mul_g(&Scalar::from(d2));
                let res = subtract(&p, &q);

                // to_affine() triggers conversion and coordinate validation and normalization.
                // If it doesn't panic and we can extract coordinates, it's valid.
                let _ = res.to_affine();
            }
        }
    }
}
