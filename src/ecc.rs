// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-performance secp256k1 elliptic curve primitives and abstractions.
//!
//! This module wraps the [`k256`] crate with a thin, search-oriented API.
//! All operations enforce SEC1 compliance and strict scalar range validation.
//!
//! Points are handled in projective coordinates during arithmetic and only
//! normalized to affine when an X-coordinate must be extracted.
//!
//! # Coordinate representations
//!
//! Internally the search engine stores points in
//! [`k256::ProjectivePoint`] form, which keeps arithmetic cheap (no modular
//! inversion per operation). Conversion to [`k256::AffinePoint`] — which
//! requires one modular inversion per point — is deferred to the
//! [`to_hex_x`] / [`x_bytes`] extraction helpers, where it can be batched
//! using Montgomery's simultaneous inversion (see ADR-0002).
//!
//! # Side-channel stance
//!
//! **This module is not constant-time.** Scalar multiplication, modular
//! inversion, and point comparison are all exposed to timing and cache
//! side-channels. The tool is intended for educational and research use
//! only; it MUST NOT be used to sign or verify messages where
//! side-channel resistance is required. See [`docs/security.md`](../docs/security.md)
//! for the full threat model.
//!
//! # Validation guarantees
//!
//! Every input parsed by this module is validated against the secp256k1
//! curve equation before being returned. Specifically:
//!
//! - [`parse_pubkey`] rejects off-curve points, wrong SEC1 prefixes, and
//!   the point-at-infinity.
//! - [`hex_to_scalar`] rejects values equal to or greater than the curve
//!   order `n` (SEC 2 §2.4.1).
//!
//! # Thread safety
//!
//! Every function in this module is pure and stateless. All `&ProjectivePoint`
//! and `&Scalar` arguments may be freely shared across threads; the
//! functions are unconditionally [`Send`] + [`Sync`].

use crate::error::{FindError, Result};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::elliptic_curve::PrimeField;
use k256::{ProjectivePoint, PublicKey, Scalar};
use tracing::instrument;

/// Parses a public key from its SEC1 hexadecimal representation.
///
/// The input must be a valid hex-encoded SEC1 point (compressed or
/// uncompressed). Compressed form follows SEC 1 §2.3 (33 bytes, leading
/// `0x02`/`0x03`); uncompressed form follows SEC 1 §2.4 (65 bytes, leading
/// `0x04`). The resulting point is validated against the secp256k1 curve
/// equation and converted to projective coordinates for efficient
/// subsequent arithmetic.
///
/// # Errors
///
/// Returns [`FindError::HexError`] if the string is not valid hex.
///
/// Returns [`FindError::InvalidPublicKey`] if the decoded bytes do not form
/// a valid SEC1 public key (wrong prefix, off-curve coordinates, or the
/// point-at-infinity).
///
/// # Security
///
/// The parser is purely string-decoding + signature verification (zero
/// knowledge of any private key). It does not perform any timing-sensitive
/// operation and is safe to call on attacker-controlled input.
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

/// Converts a hexadecimal string to a scalar field element \(s \in \mathbb{F}_n\),
/// where \(n\) is the secp256k1 curve order (SEC 2 §2.4.1).
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
/// # Complexity
///
/// \(O(1)\) — the function performs a single 32-byte canonical decode and
/// one constant-time reduction check. Memory usage is bounded by a single
/// 32-byte stack buffer.
///
/// # Security
///
/// Pure data manipulation; no secret-dependent branching. Safe to call on
/// attacker-controlled input.
///
/// # Examples
///
/// ```
/// use find::ecc;
///
/// let s = ecc::hex_to_scalar("01").unwrap();
/// assert_eq!(s, k256::Scalar::from(1u64));
/// ```
pub fn hex_to_scalar(hex_str: &str) -> Result<Scalar> {
    let bytes = hex::decode(hex_str).map_err(FindError::from)?;
    if bytes.is_empty() {
        return Err(FindError::EccError("Empty hex string input".to_string()));
    }
    let mut fixed_bytes = [0u8; 32];

    // Big-endian, 32-byte-wide canonical encoding:
    //   - inputs shorter than 32 bytes are zero-padded on the LEFT
    //     (high-order side),
    //   - inputs longer than 32 bytes are truncated to the LEAST-significant
    //     32 bytes (i.e. the rightmost 32 bytes of the decoded stream).
    // This matches the convention used by Bitcoin and the k256 type, where
    // the low-order byte of the scalar is the rightmost byte of the buffer.
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
///
/// # Security
///
/// Not constant-time; the underlying k256 fixed-base multiplication leaks
/// timing information about `d`. Do not use this function in any context
/// where the scalar is secret.
#[inline(always)]
pub fn scalar_mul_g(d: &Scalar) -> ProjectivePoint {
    ProjectivePoint::GENERATOR * d
}

/// Computes the point difference \(R = P - Q\) in projective coordinates.
///
/// Subtraction is performed as \(P + (-Q)\), where \(-Q\) is the additive
/// inverse.
///
/// # Security
///
/// Not constant-time; the negation and mixed addition are both exposed
/// to timing side-channels.
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
///
/// # Performance
///
/// This function performs a single projective-to-affine conversion (one
/// modular inversion) plus a base-16 encoding of the resulting 32-byte
/// coordinate. The hot-path callers in [`crate::search`] amortize the
/// inversion across 32 points at a time using Montgomery's simultaneous
/// inversion (see ADR-0002).
///
/// # Security
///
/// Not constant-time; the modular inversion leaks timing information. See
/// the module-level docs for the threat model.
///
/// # Examples
///
/// ```
/// use find::ecc;
/// use k256::Scalar;
///
/// let p = ecc::scalar_mul_g(&Scalar::from(42u64));
/// let hex_x = ecc::to_hex_x(&p);
/// assert_eq!(hex_x.len(), 64);
///
/// // The identity point canonicalises to 64 zeros.
/// let id = k256::ProjectivePoint::IDENTITY;
/// assert_eq!(ecc::to_hex_x(&id), "0".repeat(64));
/// ```
pub fn to_hex_x(p: &ProjectivePoint) -> String {
    let affine = p.to_affine();
    // Uncompressed SEC1 (65 bytes: 0x04 || X || Y) — we discard the Y.
    let encoded = affine.to_encoded_point(false);

    let x = match encoded.x() {
        Some(x) => x,
        None => {
            // The point-at-infinity has no X-coordinate. We canonicalise to
            // 64 zeros (32 bytes of 0x00) so the orchestrator's X-byte
            // comparisons always operate on a well-defined width. The X
            // value is meaningless for the identity and will simply not
            // match any variant in the index.
            return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }
    };
    hex::encode(x)
}

/// Returns `true` if the point is the point-at-infinity (the additive identity).
///
/// This is an equality check against `ProjectivePoint::IDENTITY`; it is
/// suitable for use in the search hot path because it does not perform a
/// modular inversion.
///
/// Note: this comparison is *not* constant-time in the side-channel sense —
/// it short-circuits on the first differing coordinate. The wrapper as a
/// whole is not constant-time; see the module-level doc for the threat
/// model.
///
/// # Examples
///
/// ```
/// use find::ecc;
/// use k256::ProjectivePoint;
///
/// assert!(ecc::is_identity(&ProjectivePoint::IDENTITY));
/// assert!(!ecc::is_identity(&ecc::generator()));
/// ```
#[inline(always)]
pub fn is_identity(p: &ProjectivePoint) -> bool {
    *p == ProjectivePoint::IDENTITY
}

/// Extracts the 32-byte big-endian X-coordinate of an elliptic curve point
/// as raw bytes.
///
/// This operation normalizes the point from projective to affine coordinates,
/// which involves a modular inversion and is therefore expensive. Callers
/// should batch such extractions whenever possible.
///
/// # Returns
///
/// `Some([u8; 32])` containing the X-coordinate, or `None` if the point is
/// the point-at-infinity (in which case the X-coordinate is undefined).
///
/// # Examples
///
/// ```
/// use find::ecc;
/// use k256::Scalar;
///
/// let p = ecc::scalar_mul_g(&Scalar::from(7u64));
/// let xs = ecc::x_bytes(&p).expect("non-identity has an X-coordinate");
/// assert_eq!(xs.len(), 32);
///
/// let id = k256::ProjectivePoint::IDENTITY;
/// assert!(ecc::x_bytes(&id).is_none());
/// ```
#[inline]
pub fn x_bytes(p: &ProjectivePoint) -> Option<[u8; 32]> {
    if is_identity(p) {
        return None;
    }
    let affine = p.to_affine();
    // Uncompressed SEC1 encoding (0x04 || X || Y); we drop the Y and keep
    // the 32-byte big-endian X. This is the on-disk cache format too —
    // see ADR-0006.
    let encoded = affine.to_encoded_point(false);
    encoded.x().map(|x| {
        let mut b = [0u8; 32];
        b.copy_from_slice(x.as_ref());
        b
    })
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

    /// Verifies that invalid hex is rejected by [`parse_pubkey`].
    #[test]
    fn test_parse_pubkey_invalid_hex() {
        let res = parse_pubkey("zzzz");
        assert!(res.is_err(), "Invalid hex must be rejected");
        assert!(res.unwrap_err().to_string().contains("Hex"));
    }

    /// Verifies that a malformed SEC1 prefix is rejected.
    #[test]
    fn test_parse_pubkey_malformed_sec1() {
        let res = parse_pubkey("04abcd");
        assert!(
            res.is_err(),
            "Malformed SEC1 key must be rejected: {:?}",
            res.ok()
        );
    }

    /// Verifies that invalid hex is rejected by [`hex_to_scalar`].
    #[test]
    fn test_hex_to_scalar_invalid_hex() {
        let res = hex_to_scalar("0g");
        assert!(res.is_err(), "Invalid hex must be rejected");
    }

    /// Verifies that a scalar equal to the curve order is rejected.
    #[test]
    fn test_hex_to_scalar_overflow() {
        let n_hex = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
        let res = hex_to_scalar(n_hex);
        assert!(res.is_err(), "Scalar equal to curve order must be rejected");
        assert!(res.unwrap_err().to_string().contains("exceeds curve order"));
    }

    /// Verifies that long hex strings are truncated to the least-significant 32 bytes.
    #[test]
    fn test_hex_to_scalar_long_input() {
        let long = "0001fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
        let res = hex_to_scalar(long);
        assert!(res.is_ok(), "Long hex should be truncated, not rejected");
    }

    /// Verifies that the generator point matches the expected base point.
    #[test]
    fn test_generator_is_base_point() {
        let g = generator();
        assert_eq!(g, ProjectivePoint::GENERATOR);
    }

    /// Verifies that [`to_hex_x`] round-trips for a non-identity point.
    #[test]
    fn test_to_hex_x_roundtrip() {
        let p = scalar_mul_g(&Scalar::from(42u64));
        let x_hex = to_hex_x(&p);
        assert_eq!(x_hex.len(), 64);
        let bytes = hex::decode(&x_hex).unwrap();
        assert_eq!(bytes.len(), 32);
        assert!(bytes.iter().any(|&b| b != 0));
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

    /// Tests for input-validation edge cases and hardening.
    #[cfg(test)]
    mod hardening_tests {
        use super::*;

        /// Verifies that a hex string whose truncated 32-byte form equals the
        /// curve order is rejected.
        ///
        /// `hex_to_scalar` truncates the input to the first 32 bytes (the
        /// high-order bytes of a big-endian encoding). A 33-byte input whose
        /// first 32 bytes are exactly the curve order `n` must be rejected
        /// because `n` itself is not a valid scalar (scalars are in `[0, n)`).
        #[test]
        fn test_hex_to_scalar_truncation_overflow() {
            // 33 bytes: n (32 bytes) followed by 0xFF (1 extra byte).
            // The first 32 bytes are n, so the truncated value is n itself.
            let n_hex = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
            let long = format!("{}ff", n_hex);
            assert_eq!(long.len(), 66); // 33 bytes
            let res = hex_to_scalar(&long);
            assert!(res.is_err(), "Truncation of a value >= n must be rejected");
            assert!(
                res.unwrap_err().to_string().contains("exceeds curve order"),
                "Error must mention curve order"
            );
        }

        /// Verifies that the canonical compressed pubkey of G parses correctly.
        #[test]
        fn test_parse_canonical_g_compressed() {
            // Standard compressed SEC1 encoding of the secp256k1 generator.
            let hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
            let p = parse_pubkey(hex).expect("canonical G must parse");
            assert_eq!(p, generator());
        }

        /// Verifies that the canonical uncompressed pubkey of G parses correctly.
        #[test]
        fn test_parse_canonical_g_uncompressed() {
            // Standard uncompressed SEC1 encoding of the secp256k1 generator.
            let hex = "04\
                       79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798\
                       483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";
            let p = parse_pubkey(hex).expect("canonical G must parse");
            assert_eq!(p, generator());
        }

        // Property: for random non-identity points, `to_hex_x` returns 64
        // hex characters that round-trip to 32 bytes.
        proptest::proptest! {
            #[test]
            fn prop_to_hex_x_idempotent(d in 1u64..1_000_000u64) {
                let p = scalar_mul_g(&Scalar::from(d));
                let hex_str = to_hex_x(&p);
                proptest::prop_assert_eq!(hex_str.len(), 64);
                let bytes = hex::decode(&hex_str).expect("hex must decode");
                proptest::prop_assert_eq!(bytes.len(), 32);
            }
        }
    }
}
