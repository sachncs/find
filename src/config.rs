// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Session configuration types and validation.
//!
//! This module owns the [`Config`] struct, the [`BatchSize`] newtype, and the
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
//!
//! # Constants
//!
//! The compile-time constants ([`TRILLION`], [`DEFAULT_CACHE_CHUNK_SIZE`],
//! [`MAX_SEARCH`], [`MIN_SEARCH_SCALAR`]) define the boundaries of the search space
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
pub const MIN_SEARCH_SCALAR: u64 = 1;

/// Default number of points per Montgomery batch-normalization.
///
/// 32 is empirically the sweet spot on `x86_64` and aarch64: stack
/// allocation cost (32 × 96 bytes ≈ 3 KB) fits in L1 cache, and the
/// cost of 32 scalar multiplications roughly balances one batch
/// normalization. See [ADR-0002](../docs/adr/0002-batch-normalization.md).
///
/// Can be overridden via the `--batch-size` CLI flag or
/// [`Config::with_batch_size`]. Allowed range: 1..=256.
pub const DEFAULT_BATCH_SIZE: u32 = 32;

/// Default number of shift variants per session.
///
/// 512 (256 powers of two + 256 cumulative sums) is the documented
/// default. Smaller values reduce the per-session variant-set memory
/// footprint at the cost of missing some small-scalar targets.
/// Allowed range: 1..=512.
pub const DEFAULT_VARIANT_COUNT: u32 = 512;

/// Maximum batch size the engine can address.
///
/// Since commit 7b the hot-path arrays are heap-allocated and sized at
/// runtime against this cap. See ADR-0009.
pub const MAX_BATCH_SIZE: u32 = 256;

/// Maximum variant count the engine can address.
pub const MAX_VARIANT_COUNT: u32 = 512;

/// A bounded number of points per Montgomery batch-normalization.
///
/// A thin newtype that records intent ("batch size") and enforces the
/// legal range at construction time. Replacement for the raw `u32`
/// `Config::batch_size` field; the newtype cannot be silently
/// constructed out of range.
///
/// The legal range is `1..=[``BatchSize::MAX``]`. The default is
/// [`BatchSize::DEFAULT`].
///
/// # Examples
///
/// ```
/// use find::config::BatchSize;
///
/// let bs = BatchSize::new(64).expect("64 is in range");
/// assert_eq!(bs.get(), 64);
///
/// assert!(BatchSize::new(0).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSize(u32);

impl BatchSize {
    /// Smallest legal batch size.
    pub const MIN: u32 = 1;
    /// Largest legal batch size. Capped by the heap allocation budget
    /// of the search engine; see ADR-0009 (commit 14).
    pub const MAX: u32 = MAX_BATCH_SIZE;
    /// Default batch size, used by `Config::new` and tests.
    pub const DEFAULT: Self = Self(DEFAULT_BATCH_SIZE);

    /// Constructs a `BatchSize` from a raw `u32`, returning
    /// [`FindError::InvalidConfig`] on out-of-range values.
    pub fn new(size: u32) -> Result<Self> {
        if (Self::MIN..=Self::MAX).contains(&size) {
            Ok(Self(size))
        } else {
            Err(FindError::InvalidConfig(format!(
                "batch_size {size} out of range {}..={}",
                Self::MIN,
                Self::MAX
            )))
        }
    }

    /// Returns the inner `u32` value.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for BatchSize {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl std::fmt::Display for BatchSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
/// cfg.validate_fields().expect("non-empty pubkey must validate");
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    /// HEX-encoded SEC1 public key (compressed or uncompressed).
    /// Required when `target_address` is `None`; ignored otherwise.
    pub pubkey: String,
    /// Root directory for checkpoints, caches, and exported variant metadata.
    pub output_dir: String,
    /// Whether to generate and persist binary cache files.
    ///
    /// Enabling this consumes approximately 32GB of disk per billion scalars
    /// but allows subsequent sweeps to run at I/O-bound speeds.
    pub cache_points: bool,
    /// Number of points per Montgomery batch-normalization.
    ///
    /// Defaults to [`BatchSize::DEFAULT`]. Tunable via
    /// [`Config::with_batch_size`] (panicking) or
    /// [`Config::try_with_batch_size`] (fallible). Smaller values
    /// reduce per-batch allocation cost; larger values amortise
    /// the single Montgomery inversion across more points.
    pub batch_size: BatchSize,
    /// Number of shift variants per session.
    ///
    /// Defaults to [`DEFAULT_VARIANT_COUNT`] (512). Tunable via
    /// [`Config::with_variant_count`] (panicking, deprecated) or
    /// [`Config::try_with_variant_count`] (fallible). Smaller values
    /// reduce the variant-set memory footprint at the cost of missing
    /// some small-scalar targets.
    pub variant_count: u32,
    /// Optional inclusive-lower scalar bound for the sweep.
    ///
    /// `Some(n)` means "start the chunked sweep from `n` instead of from
    /// `MIN_SEARCH_SCALAR`". Combined with `range_to`, scopes the entire
    /// session to a single user-specified window without persisted
    /// checkpoints (see ADR-0011).
    pub range_from: Option<u64>,
    /// Optional inclusive-upper scalar bound for the sweep.
    ///
    /// `Some(m)` means "stop the chunked sweep at `m`". Both `range_from`
    /// and `range_to` must be `Some` or both `None`; partial sets are
    /// rejected with `FindError::InvalidConfig` at builder time.
    pub range_to: Option<u64>,
    /// Optional Bitcoin mainnet target address (P2PKH 0x00 or P2SH 0x05).
    ///
    /// When `Some(addr)`, the orchestrator switches from variant-keyed
    /// sweep to a hash40-keyed sweep over `range_from..=range_to`; the
    /// candidate scalars' compressed-pubkey hash is compared against
    /// `addr`. Note that the relationship between target and scalar is
    /// iterated in the **range**, not inverted from the address: the
    /// tool does not (and cannot) recover the private key from an
    /// address without also specifying the candidate range.
    pub target_address: Option<crate::address::Address40>,
}

impl Config {
    /// Constructs a new `Config` with the given pubkey, output dir, and cache flag.
    ///
    /// Uses the default batch size and variant count. For tunables, see
    /// [`Config::with_batch_size`] and [`Config::with_variant_count`].
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
    /// assert_eq!(cfg.batch_size, find::config::BatchSize::DEFAULT);
    /// assert_eq!(cfg.variant_count, find::config::DEFAULT_VARIANT_COUNT);
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
            batch_size: BatchSize::DEFAULT,
            variant_count: DEFAULT_VARIANT_COUNT,
            range_from: None,
            range_to: None,
            target_address: None,
        }
    }

    /// Sets the explicit scalar range [`from`, `to`]. Both inclusive.
    ///
    /// Both must be provided and `from <= to`. `to` is also implicit by
    /// the `u64` ceiling; sizes above `u64::MAX` are not representable.
    ///
    /// When `target_address` is also `Some`, the range scopes the hash40
    /// sweep; when it's `None`, the range scopes the variant-keyed sweep.
    ///
    /// Returns `FindError::InvalidConfig` on validation failure.
    pub fn try_with_range(mut self, from: u64, to: u64) -> Result<Self> {
        if from > to {
            return Err(FindError::InvalidConfig(format!(
                "range_from ({from}) must be <= range_to ({to})"
            )));
        }
        self.range_from = Some(from);
        self.range_to = Some(to);
        Ok(self)
    }

    /// Sets the optional target Bitcoin address (mainnet P2PKH or P2SH).
    ///
    /// When set, the orchestrator switches to the address-keyed sweep
    /// mode. The `pubkey` field is still required syntactically (its
    /// empty default is allowed in address mode and is replaced by a
    /// dummy value at parse time) but it is unused for the hash40 path.
    ///
    /// Returns `FindError::InvalidAddress` if the address fails to
    /// Base58Check decode or carries a non-`0x00`/`0x05` version byte.
    pub fn try_with_target_address(mut self, addr: &str) -> Result<Self> {
        let (_v, hash40) = crate::address::bitcoin_address_to_hash40(addr)?;
        self.target_address = Some(hash40);
        Ok(self)
    }

    /// Sets the batch size, returning the updated `Config`.
    ///
    /// # Arguments
    ///
    /// * `size` — Number of points per Montgomery batch-normalization.
    ///   Allowed range: 1..=[`BatchSize::MAX`].
    ///
    /// # Panics
    ///
    /// Panics if `size` is outside the allowed range. Prefer
    /// [`Config::try_with_batch_size`] for fallible construction; this
    /// method is retained for backward compat.
    #[deprecated(note = "use try_with_batch_size for fallible construction")]
    pub fn with_batch_size(mut self, size: u32) -> Self {
        assert!(
            (1..=MAX_BATCH_SIZE).contains(&size),
            "batch_size {size} out of range 1..={MAX_BATCH_SIZE}"
        );
        self.batch_size = BatchSize(size);
        self
    }

    /// Sets the variant count, returning the updated `Config`.
    ///
    /// # Arguments
    ///
    /// * `count` — Number of shift variants to generate. Allowed range:
    ///   1..=[`MAX_VARIANT_COUNT`].
    ///
    /// # Panics
    ///
    /// Panics if `count` is outside the allowed range. Prefer
    /// [`Config::try_with_variant_count`].
    #[deprecated(note = "use try_with_variant_count for fallible construction")]
    pub fn with_variant_count(mut self, count: u32) -> Self {
        assert!(
            (1..=MAX_VARIANT_COUNT).contains(&count),
            "variant_count {count} out of range 1..={MAX_VARIANT_COUNT}"
        );
        self.variant_count = count;
        self
    }

    /// Fallible batch-size setter. Returns
    /// [`FindError::InvalidConfig`] on out-of-range values.
    ///
    /// # Examples
    ///
    /// ```
    /// use find::config::Config;
    ///
    /// let cfg = Config::new("02abcd", "data", false);
    /// let cfg = cfg.try_with_batch_size(64).unwrap();
    /// assert_eq!(cfg.batch_size.get(), 64);
    /// assert!(cfg.try_with_batch_size(0).is_err());
    /// ```
    pub fn try_with_batch_size(mut self, size: u32) -> Result<Self> {
        let bs = BatchSize::new(size)?;
        self.batch_size = bs;
        Ok(self)
    }

    /// Fallible variant-count setter. Returns
    /// [`FindError::InvalidConfig`] on out-of-range values.
    pub fn try_with_variant_count(mut self, count: u32) -> Result<Self> {
        if !(1..=MAX_VARIANT_COUNT).contains(&count) {
            return Err(FindError::InvalidConfig(format!(
                "variant_count {count} out of range 1..={MAX_VARIANT_COUNT}"
            )));
        }
        self.variant_count = count;
        Ok(self)
    }

    /// Shallow-validates that all required fields are non-empty.
    ///
    /// This is a cheap, allocation-free check intended for use at any call
    /// site that wants a quick sanity gate. For a deeper check that the
    /// pubkey actually parses as a SEC1 point on secp256k1, use
    /// [`Config::validate_pubkey`].
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
    /// let ok = Config::new("02abcd", "data", false);
    /// assert!(ok.validate_fields().is_ok());
    ///
    /// let bad = Config::new("   ", "data", false);
    /// assert!(bad.validate_fields().is_err());
    /// ```
    pub fn validate_fields(&self) -> Result<()> {
        // Either a SEC1 pubkey OR an address target must be provided.
        // (Address-targeted mode reuses the pubkey string as an unused
        //  slot for backward-compat with the existing Config surface.)
        if self.pubkey.trim().is_empty() && self.target_address.is_none() {
            return Err(FindError::InvalidPublicKey(
                "Public key cannot be empty (and no --address target was set)".to_string(),
            ));
        }
        Ok(())
    }

    /// Deep-validates that the pubkey parses as a valid SEC1 point.
    ///
    /// This is the fail-fast version of [`Config::validate_fields`] — it actually
    /// decodes the hex string and runs it through
    /// [`ecc::parse_pubkey`](crate::ecc::parse_pubkey). Use this at the
    /// orchestrator's entry point so that a malformed pubkey is surfaced
    /// as `Err(InvalidPublicKey(_))` rather than as a cryptic parse error
    /// later in the session.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::InvalidPublicKey`] on any SEC1 parsing failure
    /// (wrong hex encoding, wrong prefix, off-curve coordinates, etc.).
    ///
    /// # Examples
    ///
    /// ```
    /// use find::config::Config;
    ///
    /// let good = Config::new(
    ///     "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    ///     "data",
    ///     false,
    /// );
    /// assert!(good.validate_pubkey().is_ok());
    ///
    /// let bad = Config::new("not_hex", "data", false);
    /// assert!(bad.validate_pubkey().is_err());
    /// ```
    pub fn validate_pubkey(&self) -> Result<()> {
        // In address-targeted mode the pubkey string is empty by design.
        if self.target_address.is_some() {
            // Skip SEC1 parse but still surface the empty-pubkey invariant
            // if the user wrote one explicitly (rather than relying on CLI
            // default) — that's the only way to land here from main().
            if !self.pubkey.trim().is_empty() {
                crate::ecc::parse_pubkey(&self.pubkey)?;
            }
            return Ok(());
        }
        if self.pubkey.trim().is_empty() {
            return Err(FindError::InvalidPublicKey(
                "Public key cannot be empty".to_string(),
            ));
        }
        crate::ecc::parse_pubkey(&self.pubkey)?;
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
        assert!(config.validate_fields().is_err());
    }

    /// Verifies that whitespace-only pubkeys are rejected.
    #[test]
    fn test_validate_rejects_whitespace_pubkey() {
        let config = Config::new("   ", "/tmp", false);
        assert!(config.validate_fields().is_err());
    }

    /// Verifies that a non-empty pubkey passes validation.
    #[test]
    fn test_validate_accepts_valid_pubkey() {
        let config = Config::new("02abcd", "/tmp", false);
        assert!(config.validate_fields().is_ok());
    }

    /// Verifies that `validate_pubkey` accepts a well-formed SEC1 pubkey.
    #[test]
    fn test_config_validate_pubkey_accepts_valid() {
        let config = Config::new(
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "/tmp",
            false,
        );
        assert!(config.validate_pubkey().is_ok());
    }

    /// Verifies that `validate_pubkey` rejects a malformed pubkey.
    #[test]
    fn test_config_validate_pubkey_rejects_invalid() {
        // Non-hex bytes.
        let bad = Config::new("not_hex_at_all", "/tmp", false);
        assert!(bad.validate_pubkey().is_err());

        // Empty / whitespace-only.
        let empty = Config::new("", "/tmp", false);
        assert!(empty.validate_pubkey().is_err());
        let ws = Config::new("   ", "/tmp", false);
        assert!(ws.validate_pubkey().is_err());
    }

    /// Verifies that `BatchSize::new` accepts legal values.
    #[test]
    fn test_config_try_with_batch_size_in_range() {
        let cfg = Config::new("02abcd", "/tmp", false);
        assert_eq!(
            cfg.clone().try_with_batch_size(1).unwrap().batch_size.get(),
            1
        );
        assert_eq!(
            cfg.clone()
                .try_with_batch_size(BatchSize::MAX)
                .unwrap()
                .batch_size
                .get(),
            BatchSize::MAX
        );
        assert_eq!(cfg.try_with_batch_size(64).unwrap().batch_size.get(), 64);
    }

    /// Verifies that `BatchSize::new` rejects out-of-range values.
    #[test]
    fn test_config_try_with_batch_size_out_of_range() {
        let cfg = Config::new("02abcd", "/tmp", false);
        assert!(matches!(
            cfg.clone().try_with_batch_size(0),
            Err(FindError::InvalidConfig(_))
        ));
        assert!(matches!(
            cfg.try_with_batch_size(BatchSize::MAX + 1),
            Err(FindError::InvalidConfig(_))
        ));
    }

    /// Verifies that `try_with_variant_count` accepts legal values and
    /// rejects out-of-range ones.
    #[test]
    fn test_config_try_with_variant_count() {
        let cfg = Config::new("02abcd", "/tmp", false);
        assert_eq!(
            cfg.clone().try_with_variant_count(1).unwrap().variant_count,
            1
        );
        assert_eq!(
            cfg.clone()
                .try_with_variant_count(MAX_VARIANT_COUNT)
                .unwrap()
                .variant_count,
            MAX_VARIANT_COUNT
        );
        assert!(matches!(
            cfg.clone().try_with_variant_count(0),
            Err(FindError::InvalidConfig(_))
        ));
        assert!(matches!(
            cfg.try_with_variant_count(MAX_VARIANT_COUNT + 1),
            Err(FindError::InvalidConfig(_))
        ));
    }

    /// Verifies that `BatchSize` default + newtype accessors behave.
    #[test]
    fn test_batch_size_newtype() {
        assert_eq!(BatchSize::DEFAULT.get(), DEFAULT_BATCH_SIZE);
        assert_eq!(BatchSize::MIN, 1);
        assert_eq!(BatchSize::MAX, MAX_BATCH_SIZE);
        assert_eq!(BatchSize::new(32).unwrap().get(), 32);
        assert!(BatchSize::new(0).is_err());
    }
}
