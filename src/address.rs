// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Bitcoin-address codec.
//!
//! Decodes a Base58Check-encoded mainnet Bitcoin address to the
//! 20-byte hash (RIPEMD-160 of SHA-256 of the compressed SEC1 public key).
//!
//! Only the two mainnet types P2PKH (version byte `0x00`) and P2SH (version
//! byte `0x05`) are accepted. Testnet (`0x6f`), Bech32/SegWit, and other
//! non-standard version bytes are rejected with
//! [`FindError::InvalidAddress`](crate::error::FindError::InvalidAddress).
//!
//! # Why this lives in `find`
//!
//! The new `--address <base58>` discovery mode needs to verify a candidate
//! scalar's compressed pubkey against the target address. We never invert
//! hash160 back to a pubkey (that requires Bitcoin's UTXO set as a lookup
//! table and is out of scope); the address is the *target*, not the
//! starting point.
//!
//! # Standard references
//!
//! - Base58Check: <https://en.bitcoin.it/wiki/Technical_background_of_Bitcoin_addresses>
//! - Address formats: BIP-13 (`0x05` P2SH) and BIP-16 / legacy (`0x00` P2PKH).

use crate::error::{FindError, Result};

/// Allowed version bytes for the Bitcoin address codec (mainnet only).
const VALID_VERSION_BYTES: &[u8] = b"\x00\x05";

/// A 20-byte Bitcoin hash (RIPEMD-160 ∘ SHA-256 of the compressed pubkey).
///
/// Newtype wraps `[u8; 20]` so it cannot be confused with arbitrary byte
/// arrays in function signatures. Implements [`Display`] in the canonical
/// lower-case hex form (no `0x` prefix, trimmed of leading zeros).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Address40(pub [u8; 20]);

impl Address40 {
    /// Full 40-character lower-case hex with NO leading-zero trimming.
    /// Used by the address-keyed sweep to label the recovered scalar in
    /// CLI output; matches the 40-char hash40 visual layout that humans
    /// are used to seeing in Bitcoin tools.
    pub fn to_hex_trimmed_padded(&self) -> String {
        let mut s = String::with_capacity(40);
        for &b in &self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// Hex form used by `Display`: lower-case, no `0x`, leading zeros trimmed.
    pub fn to_hex_trimmed(&self) -> String {
        let bytes = self.0;
        let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        if start == bytes.len() {
            return "00".to_string();
        }
        let mut out = String::with_capacity((bytes.len() - start) * 2);
        for &b in &bytes[start..] {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
        }
        out
    }

    /// Decodes a hex string into `Address40`. Accepts either a `0x`/no
    /// prefix; lower or upper case; requires exactly 40 hex chars.
    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(FindError::from)?;
        if bytes.len() != 20 {
            return Err(FindError::InvalidAddress(format!(
                "hash160 must be 20 bytes (40 hex chars); got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 20];
        out.copy_from_slice(&bytes);
        Ok(Self(out))
    }
}

impl std::fmt::Display for Address40 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex_trimmed())
    }
}

/// Decodes a Base58Check-encoded mainnet Bitcoin address to its 20-byte
/// hash40 plus the version byte.
///
/// The decoder rejects:
/// - strings shorter than the address payload (21 bytes raw),
/// - strings that fail the Base58Check checksum,
/// - non-`0x00` / non-`0x05` version bytes (testnet, segwit, etc.).
///
/// Returning `(version_byte, Address40)` lets callers distinguish P2PKH from
/// P2SH if they need to; the current orchestrator doesn't.
pub fn bitcoin_address_to_hash40(addr: &str) -> Result<(u8, Address40)> {
    let bytes = base58check_decode(addr).ok_or_else(|| {
        FindError::InvalidAddress(format!(
            "Base58Check decode failed for input of length {}",
            addr.len()
        ))
    })?;
    if bytes.len() != 21 {
        return Err(FindError::InvalidAddress(format!(
            "decoded body must be 21 bytes (1 version + 20 hash); got {}",
            bytes.len()
        )));
    }
    let version = bytes[0];
    if !VALID_VERSION_BYTES.contains(&version) {
        return Err(FindError::InvalidAddress(format!(
            "version byte 0x{version:02x} is not a mainnet P2PKH (0x00) or P2SH (0x05); \
             got {addr}"
        )));
    }
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&bytes[1..21]);
    Ok((version, Address40(hash)))
}

/// Pure-Rust Base58Check decoder (~30 LOC). Returns the 1 version byte +
/// 20 hash bytes + 4 checksum bytes; checksum is verified by recomputing
/// SHA-256(SHA-256(body)).
fn base58check_decode(s: &str) -> Option<Vec<u8>> {
    // Base58 alphabet (Bitcoin order; matches the `bitcoin` crate).
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    // Count leading '1's — each prepends a 0x00 to the decoded body.
    let leading_zeros = s.bytes().take_while(|&c| c == b'1').count();

    // Streaming BigInt: keep the decoded bytes in `decoded`, multiply by 58
    // and add the next digit each iteration. `decoded[i] * 58 + carry` fits
    // in u16 (max 255 * 58 + carry < 2^16) so byte-by-byte is safe.
    let mut decoded: Vec<u8> = Vec::with_capacity(s.len());
    for ch in s.bytes() {
        let mut carry = ALPHABET.iter().position(|&c| c == ch)? as u16;
        for b in decoded.iter_mut() {
            let sum = u16::from(*b) * 58 + carry;
            *b = (sum & 0xff) as u8;
            carry = sum >> 8;
        }
        while carry > 0 {
            decoded.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    decoded.reverse();
    let mut result = vec![0u8; leading_zeros];
    result.extend(decoded);
    if result.len() < 4 {
        return None;
    }
    // Verify the last 4 bytes are a valid SHA-256(SHA-256(body)) prefix.
    let (body, checksum) = result.split_at(result.len() - 4);
    let hash = sha256(&sha256(body));
    if checksum != &hash[..4] {
        return None;
    }
    Some(body.to_vec())
}

/// Computes SHA-256(input). Used only for Base58Check checksum verification.
fn sha256(input: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(input);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

#[cfg(test)]
mod b58_encode {
    /// Base58 encoder used only by tests to round-trip. Counterpart to
    /// `base58check_decode`.
    pub(super) fn encode(s: &[u8]) -> String {
        const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        // Count leading zero bytes; each becomes a '1' digit on the front.
        let leading = s.iter().take_while(|&&b| b == 0).count();

        // Big-endian base-58 conversion via repeated division by 58.
        let mut digits: Vec<u8> = Vec::with_capacity(s.len());
        let mut acc: Vec<u8> = s.to_vec();
        let mut started = false;
        while !acc.is_empty() && (started || acc.iter().any(|&b| b != 0)) {
            // Divide acc by 58; collect remainders.
            let mut rem: u32 = 0;
            let mut new_acc: Vec<u8> = Vec::with_capacity(acc.len());
            let mut quotient_started = false;
            for &b in &acc {
                let cur = rem * 256 + b as u32;
                let q = cur / 58;
                rem = cur % 58;
                if quotient_started || q > 0 {
                    new_acc.push(q as u8);
                    quotient_started = true;
                }
            }
            digits.push(rem as u8);
            if new_acc.is_empty() {
                // Reached all-zero quotient; loop will terminate next iter.
                new_acc = vec![0u8; acc.len().saturating_sub(1)];
            }
            acc = new_acc;
            started = true;
        }
        digits.reverse();
        let mut out = String::with_capacity(leading + digits.len());
        for _ in 0..leading {
            out.push('1');
        }
        for &d in &digits {
            out.push(ALPHABET[d as usize] as char);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU` is a 34-char string that base58-decodes
    /// to `[0x00, 0xf6, 0xf5, ..., 0xa2]` — the leading '1' Bitcoin convention
    /// produces a single leading 0x00 byte (version 0x00 = P2PKH mainnet),
    /// followed by the 0xf6-..., hash40, and a valid Base58Check checksum.
    /// The hash40 looks low-entropy; it's not a known real address, but the
    /// decoder's Base58Check mechanics accept it. This test pins that
    /// behavior and asserts on the *decoded values* so future changes to
    /// the encoder don't silently break the wire format.
    #[test]
    fn test_users_example_decodes_as_p2pkh_with_0xf6_hash() {
        let (version, addr) = bitcoin_address_to_hash40("1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU")
            .expect("user-input example must decode cleanly");
        assert_eq!(version, 0x00, "version must decode as P2PKH mainnet");
        let expected: [u8; 20] = [
            0xf6, 0xf5, 0x43, 0x1d, 0x25, 0xbb, 0xf7, 0xb1, 0x2e, 0x8a, 0xdd, 0x9a, 0xf5, 0xe3,
            0x47, 0x5c, 0x44, 0xa0, 0xa5, 0xb8,
        ];
        assert_eq!(addr.0, expected, "hash40 must equal the decoded bytes");
    }

    /// A correctly-checksummed but non-standard version byte (0xf6) must be
    /// rejected at the version-byte allowlist step. Constructed from the all-zero
    /// hash40 + version 0xf6 + a recomputed valid SHA-256(SHA-256(body))
    /// checksum, then Base58-encoded.
    #[test]
    fn test_non_standard_version_byte_rejected() {
        // body = [0xf6] + [0u8; 20] = 21 bytes.
        let mut body = vec![0xf6u8];
        body.extend(std::iter::repeat(0u8).take(20));
        let checksum = {
            use sha2::{Digest, Sha256};
            let h1 = Sha256::digest(&body);
            let h2 = Sha256::digest(h1);
            [h2[0], h2[1], h2[2], h2[3]]
        };
        body.extend(checksum);
        // Base58-encode body.
        let encoded = b58_encode::encode(&body);
        let res = bitcoin_address_to_hash40(&encoded);
        let msg = format!("{}", res.expect_err("0xf6 must be rejected"));
        assert!(
            msg.contains("0xf6") || msg.contains("version byte"),
            "expected version-byte rejection; got: {msg}"
        );
    }

    /// Reject a too-short string.
    #[test]
    fn test_too_short_rejected() {
        let res = bitcoin_address_to_hash40("abc");
        assert!(res.is_err());
    }

    /// Reject a string that fails the Base58Check checksum.
    /// Decoding the real Bitcoin genesis-block coinbase P2PKH must succeed
    /// (`1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa`); flipping one byte must fail.
    #[test]
    fn test_bad_checksum_rejected() {
        let res = bitcoin_address_to_hash40("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
        assert!(
            res.is_ok(),
            "baseline genesis address must decode successfully; got: {:?}",
            res
        );
        let (v, _) = res.unwrap();
        assert_eq!(v, 0x00);
        // Flip the last char to corrupt the checksum.
        let bad = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb";
        let bad_res = bitcoin_address_to_hash40(bad);
        assert!(
            bad_res.is_err(),
            "tampered checksum must be rejected, got: {:?}",
            bad_res
        );
    }

    /// Bitcoin test-pattern address (all-zero hash40).
    /// '1111111111111111111114oLvT2' encodes version=0x00 + 20 zero bytes.
    #[test]
    fn test_zero_hash_address_decodes() {
        let res = bitcoin_address_to_hash40("1111111111111111111114oLvT2");
        let (_v, addr) = res.expect("'1111111111111111111114oLvT2' must decode");
        for (i, b) in addr.0.iter().enumerate() {
            assert_eq!(*b, 0, "byte {i} of zero address must be zero");
        }
    }

    /// Address40: hex round-trip with leading-zero trimming.
    #[test]
    fn test_address40_hex_format() {
        let a = Address40([
            1, 2, 3, 0, 0, 0, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17,
        ]);
        assert_eq!(
            a.to_hex_trimmed(),
            "0102030000000405060708090a0b0c0d0e0f1011"
        );
        assert_eq!(Address40::from_hex(&a.to_hex_trimmed()).unwrap(), a);

        // From non-trimmed hex should also work.
        let full = "0000000000000000000000000000000000000000";
        let z = Address40::from_hex(full).unwrap();
        assert_eq!(z, Address40([0u8; 20]));
        assert_eq!(z.to_hex_trimmed(), "00");
    }

    /// `to_hex_trimmed_padded` returns the full 40-character form,
    /// preserving any leading-zero bytes (typical of Bitcoin addresses
    /// that start with `00` in the hash40).
    #[test]
    fn test_address40_padded_hex() {
        let z = Address40([0u8; 20]);
        assert_eq!(z.to_hex_trimmed_padded().len(), 40);
        assert_eq!(z.to_hex_trimmed_padded(), "00".repeat(20));

        let a = Address40([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ]);
        assert_eq!(a.to_hex_trimmed_padded().len(), 40);
        assert_eq!(
            a.to_hex_trimmed_padded(),
            "0102030405060708090a0b0c0d0e0f1011121314"
        );
    }
}
