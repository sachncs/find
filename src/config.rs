// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Session configuration types and validation.
//!
//! This module owns the [`Config`] struct, the [`SweepRange`] newtype, and the
//! default constants used to drive a search session. It is intentionally
//! minimal: it contains no I/O, no arithmetic, and no platform-specific code.
//!
//! # Thread safety
//!
//! Every type in this module is [`Send`] + [`Sync`] and `Clone` (where the
//! fields are cloneable). [`Config`] in particular is cheaply cloneable and
//! safe to pass by reference into a long-running session, including across
//! thread boundaries.
//!
//! # Relationships
//!
//! [`Config`] is the input to [`crate::orchestrator::run`]; the orchestrator
//! clones the [`Config`]'s `pubkey` string into the checkpoint it persists
//! and uses the `output_dir` to locate checkpoints and binary caches.
//! [`SweepRange`] is currently re-exported from
//! [`crate::orchestrator`] for backward compatibility but is otherwise not
//! consumed by the orchestrator's loop (the loop iterates a fixed-size
//! chunk at a time).
//!
//! # Constants
//!
//! The compile-time constants ([`TRILLION`], [`DEFAULT_CACHE_CHUNK_SIZE`],
//! [`MAX_SEARCH`], [`MIN_J`]) define the boundaries of the search space
//! and the granularity of audit logging. They are documented inline.

use crate::error::{FindError, Result};

/// Scalar step size per research segment: 1 Trillion.
///
/// The orchestrator logs an "audit boundary" message every time the
/// processed scalar count crosses a multiple of `32 * TRILLION`. This is
/// informational only.
pub const TRILLION: u64 = 1_000_000_000_000;

/// Manageable binary cache chunk size: 1 Billion.
///
/// Each chunk corresponds to ~32 GB of binary cache on disk.
pub const DEFAULT_CACHE_CHUNK_SIZE: u64 = 1_000_000_000;

/// Theoretical maximum search boundary for 64-bit scalars.
pub const MAX_SEARCH: u64 = u64::MAX;

/// Minimum non-zero search scalar.
///
/// `j = 0` yields the identity point, which cannot match a valid variant
/// because every variant is guaranteed to have a non-zero X-coordinate.
pub const MIN_J: u64 = 1;

/// A bounded inclusive range of `u64` scalars to sweep.
///
/// This is a thin newtype that documents intent and provides validation.
/// It is distinct from `(start, end)` tuples so that callers cannot
/// accidentally swap the bounds.
///
/// # Invariants
///
/// - `start >= MIN_J` (enforced by [`SweepRange::new`], which clamps).
/// - The range is **inclusive on both ends**.
///
/// # Examples
///
/// ```
/// use find::config::SweepRange;
///
/// let r = SweepRange::new(10, 20);
/// assert_eq!(r.start, 10);
/// assert_eq!(r.end, 20);
/// assert_eq!(r.len(), 11); // inclusive on both ends
/// assert!(!r.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepRange {
    /// First scalar (inclusive). Must be ≥ `MIN_J`.
    pub start: u64,
    /// Last scalar (inclusive). Must be ≥ `start`.
    pub end: u64,
}

impl SweepRange {
    /// Constructs a new sweep range, clamping `start` to `MIN_J`.
    ///
    /// # Arguments
    ///
    /// * `start` — First scalar (inclusive). Values below `MIN_J` are
    ///   clamped to `MIN_J` because `j = 0` yields the identity point.
    /// * `end` — Last scalar (inclusive).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::config::SweepRange;
    ///
    /// let r = SweepRange::new(1, 100);
    /// assert_eq!(r.len(), 100);
    /// assert!(!r.is_empty());
    ///
    /// // `start` is clamped to MIN_J = 1.
    /// let clamped = SweepRange::new(0, 10);
    /// assert_eq!(clamped.start, 1);
    /// ```
    pub fn new(start: u64, end: u64) -> Self {
        Self {
            start: start.max(MIN_J),
            end,
        }
    }

    /// Returns the number of scalars in the range. Returns 0 if `start > end`.
    pub fn len(&self) -> u64 {
        if self.start > self.end {
            0
        } else {
            self.end.saturating_sub(self.start).saturating_add(1)
        }
    }

    /// Returns `true` if the range is empty (start > end).
    pub fn is_empty(&self) -> bool {
        self.start > self.end
    }
}

/// Configuration required to drive a search session.
///
/// All fields are owned strings so that the configuration can outlive the
/// CLI argument parser.
///
/// # Examples
///
/// ```
/// use find::config::Config;
///
/// let cfg = Config::new(
///     "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
///     "data",
///     false,
/// );
/// cfg.validate().expect("non-empty pubkey must validate");
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    /// HEX-encoded SEC1 public key (compressed or uncompressed).
    pub pubkey: String,
    /// Root directory for checkpoints, caches, and exported variant metadata.
    pub output_dir: String,
    /// Whether to generate and persist binary cache files.
    ///
    /// Enabling this consumes approximately 32GB of disk per billion scalars
    /// but allows subsequent sweeps to run at I/O-bound speeds.
    pub cache_points: bool,
}

impl Config {
    /// Constructs a new `Config` with the given pubkey, output dir, and cache flag.
    ///
    /// # Arguments
    ///
    /// * `pubkey` — A hex-encoded SEC1 public key (compressed or uncompressed).
    /// * `output_dir` — Filesystem path for checkpoints, binary caches, and
    ///   the exported `points.json` audit file. Created if it does not exist.
    /// * `cache_points` — If `true`, the orchestrator will pre-compute and
    ///   persist binary caches for each chunk (~32 GB per billion scalars).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::config::Config;
    ///
    /// let cfg = Config::new(
    ///     "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    ///     "data",
    ///     false,
    /// );
    /// assert_eq!(cfg.pubkey.len(), 66);
    /// assert!(!cfg.cache_points);
    /// ```
    pub fn new(
        pubkey: impl Into<String>,
        output_dir: impl Into<String>,
        cache_points: bool,
    ) -> Self {
        Self {
            pubkey: pubkey.into(),
            output_dir: output_dir.into(),
            cache_points,
        }
    }

    /// Validates that all required fields are non-empty and well-formed.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::InvalidPublicKey`] if the pubkey string is empty
    /// or whitespace-only.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::config::Config;
    ///
    /// let ok = Config::new("0279be...", "data", false);
    /// assert!(ok.validate().is_ok());
    ///
    /// let bad = Config::new("   ", "data", false);
    /// assert!(bad.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if self.pubkey.trim().is_empty() {
            return Err(FindError::InvalidPublicKey(
                "Public key cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that empty pubkeys are rejected.
    #[test]
    fn test_validate_rejects_empty_pubkey() {
        let config = Config::new("", "/tmp", false);
        assert!(config.validate().is_err());
    }

    /// Verifies that whitespace-only pubkeys are rejected.
    #[test]
    fn test_validate_rejects_whitespace_pubkey() {
        let config = Config::new("   ", "/tmp", false);
        assert!(config.validate().is_err());
    }

    /// Verifies that a non-empty pubkey passes validation.
    #[test]
    fn test_validate_accepts_valid_pubkey() {
        let config = Config::new("02abcd", "/tmp", false);
        assert!(config.validate().is_ok());
    }

    /// Verifies that `SweepRange::new` clamps `start` to `MIN_J`.
    #[test]
    fn test_sweep_range_clamps_start() {
        let r = SweepRange::new(0, 100);
        assert_eq!(r.start, MIN_J);
        assert_eq!(r.end, 100);
    }

    /// Verifies that `SweepRange::len` and `is_empty` behave as expected.
    #[test]
    fn test_sweep_range_len_and_empty() {
        let r = SweepRange::new(1, 10);
        assert_eq!(r.len(), 10);
        assert!(!r.is_empty());

        let empty = SweepRange::new(100, 1);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    /// Verifies that `SweepRange::len` saturates on overflow.
    #[test]
    fn test_sweep_range_len_saturates() {
        let r = SweepRange::new(0, u64::MAX);
        assert_eq!(r.len(), u64::MAX);
    }
}
