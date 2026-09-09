// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: random bytes fed to `parse_pubkey`.
//!
//! Verifies that the parser never panics, never causes undefined behavior,
//! and never hangs on arbitrary input. The parser may return Err for any
//! malformed input; that is the correct behavior.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert the raw bytes to a hex string (which may include non-hex chars
    // if data contains non-ASCII bytes).
    let hex_str = if let Ok(s) = std::str::from_utf8(data) {
        s.to_string()
    } else {
        // Non-UTF-8 input: encode as a "best-effort" hex string of the
        // original bytes so the parser still receives something.
        hex::encode(data)
    };

    // The parser must handle any input without panicking.
    let _ = find::ecc::parse_pubkey(&hex_str);
});
