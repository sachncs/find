// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Unified error model for production-grade reliability and system resilience.
//!
//! # 🔬 Principal Design
//! This module defines the `FindError` hierarchy, which categorizes all
//! possible failure modes into a structured system using `thiserror`.
//!
//! ## 📐 Architectural Justification
//! By using a domain-specific error enum, we ensure that:
//! 1.  **Context is Preserved:** Errors across library boundaries (e.g., `k256`,
//!     `std::io`) are wrapped with application-specific context.
//! 2.  **Actionability:** Callers can programmatically distinguish between
//!     transient I/O failures (recoverable) and fatal cryptographic mismatches.
//! 3.  **Type Safety:** The system avoids the use of `Box<dyn Error>` in
//!     internal logic, guaranteeing deterministic failure handling.

use thiserror::Error;

/// Core error hierarchy for the `find` system.
#[derive(Error, Debug)]
pub enum FindError {
    /// Failure during elliptic curve arithmetic or field validation.
    /// Typically indicates a scalar overflow or identity point encounter.
    #[error("ECC error: {0}")]
    EccError(String),

    /// Research integrity failure: checkpoint state does not match recalculated point.
    #[error("Research integrity violation: {0}")]
    ResearchIntegrityError(String),

    /// Failure to parse a SEC1 public key. This indicates that the input
    /// violates the SEC1 v2.0 standard for secp256k1 points.
    #[error("Invalid public key format: {0}")]
    InvalidPublicKey(String),

    /// Failure in underlying I/O operations (checkpointing, logging, caches).
    /// Primarily used for persistence-layer resilience.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failure in hexadecimal decoding; indicates malformed user input or
    /// corrupted variant data.
    #[error("Hex decoding error: {0}")]
    HexError(#[from] hex::FromHexError),

    /// Failure to serialize search states or variants to JSON.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Cache file integrity check failed (e.g., size not a multiple of 32 bytes,
    /// or truncated file detected during read).
    #[error("Cache file corrupted: {0}")]
    CacheCorrupted(String),
}

/// Convenience alias for operations using the unified `FindError` model.
pub type Result<T> = std::result::Result<T, FindError>;
