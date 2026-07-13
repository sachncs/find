// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Unified error model for the `find` system.
//!
//! Every fallible operation in the crate returns [`Result<T>`], which uses
//! [`FindError`] as its error type. This guarantees that callers can
//! programmatically distinguish between transient failures (e.g. I/O) and
//! fatal cryptographic mismatches.
//!
//! # Recovery strategy
//!
//! The variant chosen by an error carries a recommendation about whether
//! the caller can retry, must abort, or must surface the failure to a
//! human:
//!
//! | Variant | Recoverable? | Recommended action |
//! |---|---|---|
//! | [`FindError::EccError`] | Sometimes | Re-check input; usually a programmer error |
//! | [`FindError::ResearchIntegrityError`] | No | Treat as data corruption; abort the session |
//! | [`FindError::InvalidPublicKey`] | No | Reject input; do not retry |
//! | [`FindError::Io`] | Yes | Retry with backoff; escalate after N attempts |
//! | [`FindError::HexError`] | No | Reject input; do not retry |
//! | [`FindError::SerializationError`] | No | Treat as data corruption; abort the session |
//! | [`FindError::CacheCorrupted`] | No | Delete cache and regenerate; do not retry the file |
//!
//! # Thread safety
//!
//! [`FindError`] implements both [`Clone`] (manual) and [`PartialEq`] (manual,
//! via discriminant + `Display`), and is therefore safe to send across
//! threads and to compare in tests.
//!
//! # Extension policy
//!
//! The enum is `#[non_exhaustive]`. External match expressions must
//! include a wildcard arm so that future variants do not break their build.
//! See [ADR-0004](../docs/adr/0004-error-hierarchy.md) for the rationale.

use thiserror::Error;

/// The single error type used throughout the crate.
///
/// Variants are ordered by subsystem: ECC, integrity, parsing, I/O, format,
/// serialization, and cache integrity.
///
/// This enum is `#[non_exhaustive]` to allow future variants to be added
/// without breaking semver. External callers should match on a wildcard arm.
///
/// # Examples
///
/// ```
/// use find::error::FindError;
///
/// fn classify(e: &FindError) -> &'static str {
///     match e {
///         FindError::EccError(_) => "cryptographic",
///         FindError::ResearchIntegrityError(_) => "data-corruption",
///         FindError::InvalidPublicKey(_) => "input-rejected",
///         FindError::Io(_) => "transient",
///         FindError::HexError(_) => "input-rejected",
///         FindError::SerializationError(_) => "data-corruption",
///         FindError::CacheCorrupted(_) => "data-corruption",
///         _ => "unknown",  // required because of #[non_exhaustive]
///     }
/// }
///
/// let e = FindError::Io(std::io::Error::new(std::io::ErrorKind::Other, "x"));
/// assert_eq!(classify(&e), "transient");
/// ```
#[derive(Error, Debug)]
#[non_exhaustive]
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

impl Clone for FindError {
    fn clone(&self) -> Self {
        // Most variants clone directly. The two interesting cases:
        //   - `Io` — `std::io::Error` is `Clone`; we rebuild an equivalent
        //     error using the same `ErrorKind` and message.
        //   - `SerializationError` — `serde_json::Error` is NOT `Clone`,
        //     so we round-trip through `Error::io` with the original
        //     message. The kind and message are preserved, but the
        //     original column/line information is lost.
        match self {
            Self::EccError(s) => Self::EccError(s.clone()),
            Self::ResearchIntegrityError(s) => Self::ResearchIntegrityError(s.clone()),
            Self::InvalidPublicKey(s) => Self::InvalidPublicKey(s.clone()),
            Self::Io(e) => Self::Io(std::io::Error::new(e.kind(), e.to_string())),
            Self::HexError(e) => Self::HexError(*e),
            Self::SerializationError(e) => Self::SerializationError(serde_json::Error::io(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            )),
            Self::CacheCorrupted(s) => Self::CacheCorrupted(s.clone()),
        }
    }
}

impl PartialEq for FindError {
    fn eq(&self, other: &Self) -> bool {
        // `serde_json::Error` does not implement `PartialEq`, so we cannot
        // derive this. Instead, compare discriminants first (cheap
        // short-circuit) and then fall back to string equality via the
        // `Display` impl — which all variants produce a stable string for.
        use std::mem::discriminant;
        if discriminant(self) != discriminant(other) {
            return false;
        }
        self.to_string() == other.to_string()
    }
}

/// Convenience alias for [`std::result::Result`] parameterized with [`FindError`].
pub type Result<T> = std::result::Result<T, FindError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that every [`FindError`] variant formats its message correctly.
    #[test]
    fn test_error_display_variants() {
        assert_eq!(
            FindError::EccError("scalar overflow".to_string()).to_string(),
            "ECC error: scalar overflow"
        );
        assert_eq!(
            FindError::ResearchIntegrityError("mismatch".to_string()).to_string(),
            "Research integrity violation: mismatch"
        );
        assert_eq!(
            FindError::InvalidPublicKey("bad prefix".to_string()).to_string(),
            "Invalid public key format: bad prefix"
        );
        assert_eq!(
            FindError::Io(std::io::Error::new(std::io::ErrorKind::Other, "disk full")).to_string(),
            "I/O error: disk full"
        );
        assert_eq!(
            FindError::HexError(hex::FromHexError::OddLength).to_string(),
            "Hex decoding error: Odd number of digits"
        );
        let serde_msg =
            FindError::SerializationError(serde_json::from_str::<i32>("not_json").unwrap_err())
                .to_string();
        assert!(serde_msg.starts_with("Serialization error: expected"));
        assert!(serde_msg.contains("line 1"));
        assert_eq!(
            FindError::CacheCorrupted("bad header".to_string()).to_string(),
            "Cache file corrupted: bad header"
        );
    }

    /// Verifies that [`std::io::Error`] converts into [`FindError::Io`].
    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let find_err: FindError = io_err.into();
        assert!(matches!(find_err, FindError::Io(_)));
        assert!(find_err.to_string().contains("missing"));
    }

    /// Verifies that [`hex::FromHexError`] converts into [`FindError::HexError`].
    #[test]
    fn test_from_hex_error() {
        let hex_err = hex::decode("0z").unwrap_err();
        let find_err: FindError = hex_err.into();
        assert!(matches!(find_err, FindError::HexError(_)));
    }

    /// Verifies that [`serde_json::Error`] converts into [`FindError::SerializationError`].
    #[test]
    fn test_from_serde_error() {
        let serde_err = serde_json::from_str::<i32>("{bad}").unwrap_err();
        let find_err: FindError = serde_err.into();
        assert!(matches!(find_err, FindError::SerializationError(_)));
    }

    /// Verifies that the [`Result`] alias can be used in function signatures.
    #[test]
    fn test_result_alias_ok() -> Result<()> {
        Ok(())
    }

    /// Verifies that the [`Result`] alias propagates errors.
    #[test]
    fn test_result_alias_err() {
        fn inner() -> Result<()> {
            Err(FindError::EccError("fail".to_string()))
        }
        assert!(inner().is_err());
    }

    /// Verifies that [`FindError`] implements [`Clone`].
    #[test]
    fn test_error_clone() {
        let e1 = FindError::EccError("x".to_string());
        let e2 = e1.clone();
        assert_eq!(e1, e2);

        let e3 = FindError::Io(std::io::Error::new(std::io::ErrorKind::Other, "y"));
        let e4 = e3.clone();
        assert_eq!(e3, e4);
    }

    /// Verifies that [`FindError`] implements [`PartialEq`].
    #[test]
    fn test_error_partial_eq() {
        let a = FindError::CacheCorrupted("z".to_string());
        let b = FindError::CacheCorrupted("z".to_string());
        let c = FindError::CacheCorrupted("w".to_string());
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, FindError::EccError("z".to_string()));
    }
}
