// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Unified error model for the `find` system.
//!
//! Every fallible operation in the crate returns [`Result<T>`], which uses
//! [`FindError`] as its error type. This guarantees that callers can
//! programmatically distinguish between transient failures (e.g. I/O) and
//! fatal cryptographic mismatches.

use thiserror::Error;

/// The single error type used throughout the crate.
///
/// Variants are ordered by subsystem: ECC, integrity, parsing, I/O, format,
/// serialization, and cache integrity.
#[derive(Error, Debug)]
pub enum FindError {
    /// An error originating from elliptic-curve arithmetic or field validation.
    ///
    /// This typically indicates a scalar overflow or an unexpected identity
    /// point encountered during computation.
    #[error("ECC error: {0}")]
    EccError(String),

    /// A research-integrity violation detected during checkpoint verification.
    ///
    /// This indicates that a persisted checkpoint does not match the
    /// recalculated curve point, suggesting data corruption or a logic change.
    #[error("Research integrity violation: {0}")]
    ResearchIntegrityError(String),

    /// The provided public key could not be parsed as a valid SEC1 point.
    #[error("Invalid public key format: {0}")]
    InvalidPublicKey(String),

    /// An underlying I/O operation failed.
    ///
    /// This covers checkpoint writes, cache reads, directory creation, and
    /// log appender failures.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Hexadecimal decoding failed.
    ///
    /// This usually means the input string contains characters outside the
    /// `0-9a-fA-F` range or has an odd length.
    #[error("Hex decoding error: {0}")]
    HexError(#[from] hex::FromHexError),

    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// A cache file is structurally invalid.
    ///
    /// The binary cache format requires every entry to be exactly 32 bytes.
    /// A file size that is not a multiple of 32 triggers this error.
    #[error("Cache file corrupted: {0}")]
    CacheCorrupted(String),
}

/// Convenience alias for [`std::result::Result`] parameterized with [`FindError`].
pub type Result<T> = std::result::Result<T, FindError>;
