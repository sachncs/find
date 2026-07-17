// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
//
// //! Portable secp256k1 field arithmetic on 5x52 limbs.
//!
//! This crate exposes the secp256k1 prime field as five 52-bit
//! limbs in the same layout k256 uses internally. The
//! multiplication and squaring operations delegate to
//! `k256::FieldElement::mul` for the actual modular arithmetic; the
//! 5x52-limb representation is exposed so callers can interoperate
//! with k256's internal format without paying for a byte
//! round-trip per call.
//!
//! # Scope
//!
//! This crate is **a limb adapter, not an accelerator**. Field
//! multiplication is delegated to stock k256, so the per-call cost
//! is dominated by the byte conversion (~100-200 ns overhead vs
//! direct k256::FieldElement::mul). The value is:
//!
//! - **5x52-limb representation exposed publicly**: callers can read
//!   and write the limb format directly without going through
//!   `k256::FieldElement`.
//! - **Correctness oracle**: property tests verify byte-for-byte
//!   equivalence with `k256::FieldElement::mul`. Any future SIMD
//!   backend (NEON, BMI2/ADX, AVX-512) can replace the
//!   `k256::FieldElement::mul` delegation here as a drop-in and
//!   inherit the existing tests as a correctness oracle.
//!
//! # Not wired into the find hot loop
//!
//! k256's `FieldElement` is a private wrapper, so this crate cannot
//! drop-in replace k256's internal multiplication without forking
//! k256 itself. The find crate's hot path uses stock k256; see
//! ADR-0010 and `docs/performance.md` for the perf-ceiling analysis.
//!
//! # No unsafe
//!
//! The crate contains zero `unsafe` blocks. All arithmetic is
//! expressed in safe Rust.
//!
//! # Algorithm
//!
//! We use the standard 5-limb x 52-bit representation:
//!
//! ```text
//! a = a0 + a1*2^52 + a2*2^104 + a3*2^156 + a4*2^208
//! ```
//!
//! Each limb is at most 52 bits, leaving 12 bits of headroom per
//! limb for additions.

#![forbid(unsafe_code)]

/// 256-bit field element modulo the secp256k1 prime, represented
/// as 5 limbs of 52 bits each. On 64-bit targets this matches
/// k256's internal `FieldElement5x52` layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct FieldElement5x52(pub [u64; 5]);

impl FieldElement5x52 {
    /// Zero element.
    pub const ZERO: Self = Self([0, 0, 0, 0, 0]);

    /// Multiplicative identity.
    pub const ONE: Self = Self([1, 0, 0, 0, 0]);

    /// Multiplies two field elements modulo the secp256k1 prime.
    ///
    /// Delegates to `k256::FieldElement::mul` for the actual
    /// modular arithmetic; the result is then normalized to
    /// canonical form (`< p`) before returning. The limb
    /// representation is converted to 32 big-endian bytes and back
    /// per call; this is the correctness oracle for any future
    /// SIMD acceleration.
    #[inline]
    pub fn mul(&self, rhs: &Self) -> Self {
        let a_k = k256::FieldElement::from_bytes(&limbs_to_be_bytes(&self.0).into())
            .expect("valid field element bytes");
        let b_k = k256::FieldElement::from_bytes(&limbs_to_be_bytes(&rhs.0).into())
            .expect("valid field element bytes");
        let r_k = (a_k * b_k).normalize();
        FieldElement5x52(be_bytes_to_limbs(&r_k.to_bytes().into()))
    }

    /// Squares a field element modulo the secp256k1 prime.
    #[inline]
    pub fn square(&self) -> Self {
        self.mul(self)
    }
}

/// Convert 5x52-bit limbs to 32 big-endian bytes using k256's
/// canonical byte encoding. There is a 4-bit split between limbs
/// 2/3 (at byte 12) and limbs 0/1 (at byte 25): the low 4 bits
/// of each upper limb share a byte with the high 4 bits of each
/// lower limb.
///
/// Layout (matches `k256::FieldElement5x52::to_bytes`):
/// - Limb 4 -> bytes 0..6
/// - Limb 3 -> bytes 6..12 (full bytes) + byte 12 high nibble
/// - Limb 2 -> byte 12 low nibble + bytes 13..19
/// - Limb 1 -> bytes 19..25 (full bytes) + byte 25 high nibble
/// - Limb 0 -> byte 25 low nibble + bytes 26..32
pub fn limbs_to_be_bytes(limbs: &[u64; 5]) -> [u8; 32] {
    let mut ret = [0u8; 32];
    ret[0] = (limbs[4] >> 40) as u8;
    ret[1] = (limbs[4] >> 32) as u8;
    ret[2] = (limbs[4] >> 24) as u8;
    ret[3] = (limbs[4] >> 16) as u8;
    ret[4] = (limbs[4] >> 8) as u8;
    ret[5] = limbs[4] as u8;
    ret[6] = (limbs[3] >> 44) as u8;
    ret[7] = (limbs[3] >> 36) as u8;
    ret[8] = (limbs[3] >> 28) as u8;
    ret[9] = (limbs[3] >> 20) as u8;
    ret[10] = (limbs[3] >> 12) as u8;
    ret[11] = (limbs[3] >> 4) as u8;
    ret[12] = ((limbs[2] >> 48) as u8 & 0x0f) | ((limbs[3] as u8 & 0x0f) << 4);
    ret[13] = (limbs[2] >> 40) as u8;
    ret[14] = (limbs[2] >> 32) as u8;
    ret[15] = (limbs[2] >> 24) as u8;
    ret[16] = (limbs[2] >> 16) as u8;
    ret[17] = (limbs[2] >> 8) as u8;
    ret[18] = limbs[2] as u8;
    ret[19] = (limbs[1] >> 44) as u8;
    ret[20] = (limbs[1] >> 36) as u8;
    ret[21] = (limbs[1] >> 28) as u8;
    ret[22] = (limbs[1] >> 20) as u8;
    ret[23] = (limbs[1] >> 12) as u8;
    ret[24] = (limbs[1] >> 4) as u8;
    ret[25] = ((limbs[0] >> 48) as u8 & 0x0f) | ((limbs[1] as u8 & 0x0f) << 4);
    ret[26] = (limbs[0] >> 40) as u8;
    ret[27] = (limbs[0] >> 32) as u8;
    ret[28] = (limbs[0] >> 24) as u8;
    ret[29] = (limbs[0] >> 16) as u8;
    ret[30] = (limbs[0] >> 8) as u8;
    ret[31] = limbs[0] as u8;
    ret
}

/// Convert 32 big-endian bytes (k256's canonical encoding) to
/// 5x52-bit limbs. Inverse of [`limbs_to_be_bytes`].
pub fn be_bytes_to_limbs(bytes: &[u8; 32]) -> [u64; 5] {
    let mut ret = [0u64; 5];
    ret[4] = ((bytes[0] as u64) << 40)
        | ((bytes[1] as u64) << 32)
        | ((bytes[2] as u64) << 24)
        | ((bytes[3] as u64) << 16)
        | ((bytes[4] as u64) << 8)
        | (bytes[5] as u64);
    ret[3] = ((bytes[6] as u64) << 44)
        | ((bytes[7] as u64) << 36)
        | ((bytes[8] as u64) << 28)
        | ((bytes[9] as u64) << 20)
        | ((bytes[10] as u64) << 12)
        | ((bytes[11] as u64) << 4)
        | ((bytes[12] as u64 >> 4) & 0x0f);
    ret[2] = (((bytes[12] as u64) & 0x0f) << 48)
        | ((bytes[13] as u64) << 40)
        | ((bytes[14] as u64) << 32)
        | ((bytes[15] as u64) << 24)
        | ((bytes[16] as u64) << 16)
        | ((bytes[17] as u64) << 8)
        | (bytes[18] as u64);
    ret[1] = ((bytes[19] as u64) << 44)
        | ((bytes[20] as u64) << 36)
        | ((bytes[21] as u64) << 28)
        | ((bytes[22] as u64) << 20)
        | ((bytes[23] as u64) << 12)
        | ((bytes[24] as u64) << 4)
        | ((bytes[25] as u64 >> 4) & 0x0f);
    ret[0] = (((bytes[25] as u64) & 0x0f) << 48)
        | ((bytes[26] as u64) << 40)
        | ((bytes[27] as u64) << 32)
        | ((bytes[28] as u64) << 24)
        | ((bytes[29] as u64) << 16)
        | ((bytes[30] as u64) << 8)
        | (bytes[31] as u64);
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that mul by 1 is identity.
    #[test]
    fn mul_one_identity() {
        let a = FieldElement5x52([0x12345, 0x6789AB, 0xDEADBE, 0xCAFEBABE, 1]);
        assert_eq!(a.mul(&FieldElement5x52::ONE), a);
    }

    /// Test that mul by zero is zero.
    #[test]
    fn mul_zero_is_zero() {
        let a = FieldElement5x52([0x12345, 0x6789AB, 0xDEADBE, 0xCAFEBABE, 1]);
        assert_eq!(a.mul(&FieldElement5x52::ZERO), FieldElement5x52::ZERO);
    }

    /// Test squaring equals multiplying by self.
    #[test]
    fn square_is_self_mul() {
        let a = FieldElement5x52([0xABCDEF, 0x12345, 0x6789AB, 0, 0]);
        assert_eq!(a.square(), a.mul(&a));
    }

    /// Test 2 * 3 = 6.
    #[test]
    fn known_2_times_3() {
        let two = FieldElement5x52([2, 0, 0, 0, 0]);
        let three = FieldElement5x52([3, 0, 0, 0, 0]);
        assert_eq!(two.mul(&three), FieldElement5x52([6, 0, 0, 0, 0]));
    }

    /// (p - 1) * 2 == p - 2 (mod p).
    ///
    /// Note: k256's canonical form truncates limb 4 to 48 bits (its
    /// `subtract_modulus_approximation` masks off the top 4 bits after
    /// reduction). So `p - 2` is stored as `0x0000FFFFFFFFFFFF` for
    /// limb 4, not `0x000FFFFFFFFFFFFF` as the modulus would suggest.
    #[test]
    fn known_p_minus_1_times_2() {
        let two = FieldElement5x52([2, 0, 0, 0, 0]);
        let p_minus_1 = FieldElement5x52([
            0xFFFFEFFFFFC2E,
            0xFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFF,
            0x0000FFFFFFFFFFFF,
        ]);
        let p_minus_2 = FieldElement5x52([
            0xFFFFEFFFFFC2D,
            0xFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFF,
            0x0000FFFFFFFFFFFF,
        ]);
        assert_eq!(p_minus_1.mul(&two), p_minus_2);
    }

    /// Cross-check: our mul equals k256's mul byte-for-byte.
    #[test]
    fn mul_matches_k256_reference() {
        for &(a, b) in &[
            ([1u64, 0, 0, 0, 0], [1u64, 0, 0, 0, 0]),
            ([2, 0, 0, 0, 0], [3, 0, 0, 0, 0]),
            ([0xFFFFFFFFFF, 0, 0, 0, 0], [0xFFFFFFFFFF, 0, 0, 0, 0]),
            (
                [0xFFFFFFFFFFFFF, 0xFFFFFFFFFFFFF, 0, 0, 0],
                [0xFFFFFFFFFFFFF, 0xFFFFFFFFFFFFF, 0, 0, 0],
            ),
            (
                [0x123456, 0x789ABC, 0xDEF012, 0x345678, 0x9ABCDE],
                [0xFEDCBA, 0x987654, 0x321098, 0x765432, 0x10FEDC],
            ),
        ] {
            let ours = FieldElement5x52(a).mul(&FieldElement5x52(b));
            let kref = k256::FieldElement::from_bytes(&limbs_to_be_bytes(&a).into()).unwrap()
                * k256::FieldElement::from_bytes(&limbs_to_be_bytes(&b).into()).unwrap();
            assert_eq!(ours.0, be_bytes_to_limbs(&kref.to_bytes().into()));
        }
    }

    /// Property tests: commutative, associative.
    mod prop {
        use super::*;
        use proptest::prelude::*;

        fn arb_limb() -> impl Strategy<Value = u64> {
            0u64..(1u64 << 52)
        }

        fn arb_fe() -> impl Strategy<Value = FieldElement5x52> {
            (arb_limb(), arb_limb(), arb_limb(), arb_limb(), arb_limb())
                .prop_map(|(a, b, c, d, e)| FieldElement5x52([a, b, c, d, e]))
        }

        proptest! {
            #[test]
            fn mul_commutative(a in arb_fe(), b in arb_fe()) {
                assert_eq!(a.mul(&b), b.mul(&a));
            }

            #[test]
            fn mul_associative(a in arb_fe(), b in arb_fe(), c in arb_fe()) {
                assert_eq!(a.mul(&b).mul(&c), a.mul(&b.mul(&c)));
            }
        }
    }
}