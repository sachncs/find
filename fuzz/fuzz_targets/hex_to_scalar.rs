// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: random bytes fed to `hex_to_scalar`.
//!
//! Verifies that the hex-to-scalar converter never panics on arbitrary
//! input and that the result (if Ok) is always a valid secp256k1 scalar.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let hex_str = match std::str::from_utf8(data) {
        Ok(s) => s.to_string(),
        Err(_) => hex::encode(data),
    };

    if let Ok(scalar) = find::ecc::hex_to_scalar(&hex_str) {
        // If the conversion succeeded, the canonical 32-byte encoding
        // returned by k256 must be exactly 32 bytes wide.
        let bytes = scalar.to_bytes();
        assert_eq!(bytes.len(), 32, "scalar bytes must be 32 bytes");
    }
});
