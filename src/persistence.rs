// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Persistence layer: atomic checkpoints, binary caches, and JSON exports.
//!
//! All I/O side effects are isolated here so that `search` remains a pure
//! domain module. Consumers should use [`Checkpoint`] for durable progress
//! and [`FileCacheWriter`] for binary cache generation.
//!
//! # Responsibilities
//!
//! - **Atomic checkpoints** ([`Checkpoint`]): JSON-encoded progress records
//!   written via write-then-rename, with an integrity anchor (X-coordinate
//!   of `last_j · G`) that allows [`Checkpoint::verify`] to detect
//!   corruption. See [ADR-0003](../docs/adr/0003-atomic-checkpointing.md).
//! - **Binary caches** ([`FileCacheWriter`]): 32-byte X-coordinate blocks
//!   appended to per-chunk files. See
//!   [ADR-0006](../docs/adr/0006-binary-cache-format.md).
//! - **JSON exports** ([`save_variants_to_json`]): a deterministic
//!   `points.json` audit file mapping X-coordinate → offset decimal.
//!
//! # Concurrency
//!
//! - [`FileCacheWriter`] guards its inner [`File`] with a
//!   [`std::sync::Mutex`]. The mutex is uncontended in the typical case
//!   because each write is a single batch of ~1 KiB. Mutex poisoning
//!   surfaces as a panic from the holding thread; we deliberately abort
//!   rather than try to recover the file handle's state (see
//!   [ADR-0008](../docs/adr/0008-mutex-poisoning-policy.md)).
//! - [`perform_cached_sweep`] takes a `&File` and is single-threaded; it
//!   uses a 32 KiB stack scratch buffer to amortise read syscalls.
//!
//! # Platform behaviour
//!
//! On Unix, [`FileCacheWriter::write_block`] uses `pwrite_at` (positional
//! write), allowing arbitrary offsets without seeking. On other platforms
//! it falls back to a mutex-protected `seek + write_all` pair. The fallback
//! still satisfies [`CacheWriter`]'s contract.
//!
//! # Unsafe
//!
//! The only `unsafe` block in this module lives inside
//! [`Checkpoint::save_atomic`]: a best-effort `libc::fsync` on the parent
//! directory's file descriptor. Its `Result` is discarded because the
//! rename is already atomic and `fsync` failure on the parent dir does
//! not compromise that guarantee. See the `# Safety` section on
//! `save_atomic` for details.

use crate::ecc;
use crate::error::{FindError, Result};
use crate::search::{CacheWriter, OffsetVariant, SearchMatch, VariantIndex};
use k256::Scalar;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use tracing::instrument;

/// Durable checkpoint representing persistent search progress.
///
/// A checkpoint stores the last completed scalar index, the associated public
/// key, and an integrity anchor (the X-coordinate of `last_j * G`). The
/// anchor allows [`Checkpoint::verify`] to detect corruption.
///
/// See [ADR-0003](../docs/adr/0003-atomic-checkpointing.md) for the
/// write-then-rename + parent-directory `fsync` design rationale.
///
/// # Invariants
///
/// - `last_x` is the 32-byte big-endian X-coordinate of `last_j · G`,
///   lowercase hex with **no leading `0x` prefix**.
/// - The checkpoint is **only meaningful for the same `pubkey` that was
///   active when it was written**. A different `pubkey` is treated as
///   "no checkpoint" by [`crate::orchestrator::run`].
///
/// # Examples
///
/// ```
/// use find::persistence::Checkpoint;
/// use find::ecc;
/// use k256::Scalar;
///
/// let last_j: u64 = 42;
/// let x = ecc::to_hex_x(&ecc::scalar_mul_g(&Scalar::from(last_j)));
/// let cp = Checkpoint {
///     last_j,
///     pubkey: "02abcd".to_string(),
///     last_x: x,
/// };
/// assert!(cp.verify("02abcd").is_ok());
/// assert!(cp.verify("02ff").is_ok(), // different pubkey -> no-op verify
///     "verify() treats mismatched pubkeys as a fresh start");
/// ```
#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    /// The last successfully completed scalar index.
    pub last_j: u64,
    /// The SEC1 hex-encoded public key associated with this progress.
    pub pubkey: String,
    /// The hex-encoded X-coordinate of \(P = \text{last\_j} \cdot G\).
    pub last_x: String,
}

impl Checkpoint {
    /// Loads a checkpoint from the given JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::Io`] if the file cannot be read.
    ///
    /// Returns [`FindError::SerializationError`] if the file does not contain
    /// valid JSON.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(FindError::Io)?;
        serde_json::from_str(&content).map_err(FindError::SerializationError)
    }

    /// Verifies the integrity anchor against a recalculated point.
    ///
    /// If the stored [`pubkey`](Checkpoint::pubkey) differs from `pubkey_hex`,
    /// the checkpoint is assumed to belong to a different search and the
    /// verification succeeds (returning `Ok(())`).
    ///
    /// If the pubkeys match but the recalculated X-coordinate does not equal
    /// [`last_x`](Checkpoint::last_x), a [`FindError::ResearchIntegrityError`]
    /// is returned.
    ///
    /// Correctness of the recalculation depends on `k256`'s scalar
    /// multiplication being correct; this is independently verified by
    /// `tests/differential.rs` against the reference C `libsecp256k1`.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::ResearchIntegrityError`] on anchor mismatch.
    pub fn verify(&self, pubkey_hex: &str) -> Result<()> {
        if self.pubkey != pubkey_hex {
            return Ok(());
        }
        let scalar = Scalar::from(self.last_j);
        let expected_p = ecc::scalar_mul_g(&scalar);
        let expected_x = ecc::to_hex_x(&expected_p);
        if expected_x != self.last_x {
            return Err(FindError::ResearchIntegrityError(format!(
                "Checkpoint X-coordinate mismatch: stored {}, expected {}",
                self.last_x, expected_x
            )));
        }
        Ok(())
    }

    /// Atomically persists the checkpoint using write-then-rename.
    ///
    /// The implementation:
    /// 1. Writes JSON to a temporary file next to `path`.
    /// 2. Calls `sync_all` to flush data to the storage device.
    /// 3. Renames the temporary file to `path`.
    /// 4. On Unix, best-effort `fsync` of the parent directory so that the
    ///    rename is durable across crashes (POSIX guarantees a durable rename
    ///    only after the parent's directory entries are flushed).
    ///
    /// # Errors
    ///
    /// Returns [`FindError::Io`] or [`FindError::SerializationError`] on failure.
    ///
    /// # Safety
    ///
    /// On Unix, this function invokes `libc::fsync` on the parent directory's
    /// file descriptor. The call is **best-effort**: its `Result` is
    /// discarded via `let _ =`, so a failed `fsync` does not propagate as an
    /// error. The safety surface is therefore limited to ensuring that the
    /// file descriptor is valid for the duration of the call — which is
    /// guaranteed by the `File` returned from `std::fs::File::open(parent)`
    /// being kept alive in the same scope.
    ///
    /// The single `unsafe { libc::fsync(...) }` block is annotated with an
    /// inline `// SAFETY:` comment that explains the invariant. Because the
    /// return value is discarded, an `fsync` failure cannot cause undefined
    /// behavior; it merely leaves the rename slightly less durable than ideal,
    /// which is acceptable for a research tool that already tolerates I/O
    /// errors at higher layers.
    ///
    /// # Performance
    ///
    /// The `sync_all` on the data file plus the (best-effort) `fsync` on the
    /// parent directory collectively cost one or two disk flushes. For a
    /// research tool checkpointing at the per-chunk granularity (~1 billion
    /// scalars), this is negligible compared to the search work itself.
    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let tmp_path = path.with_extension("json.tmp");
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .map_err(FindError::Io)?;
            let json = serde_json::to_string_pretty(self).map_err(FindError::SerializationError)?;
            file.write_all(json.as_bytes()).map_err(FindError::Io)?;
            file.sync_all().map_err(FindError::Io)?;
        }
        fs::rename(&tmp_path, path).map_err(FindError::Io)?;

        // On Unix, fsync the parent directory so the rename is durable.
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            if let Some(parent) = path.parent() {
                if let Ok(dir) = std::fs::File::open(parent) {
                    // SAFETY: `AsRawFd` is a safe trait method that returns a
                    // valid file descriptor borrowed from `dir`. The descriptor
                    // remains valid for the duration of the call. `fsync` is
                    // safe to call on any open file descriptor; failure is
                    // non-fatal for the rename atomicity guarantee, hence the
                    // discarded return value.
                    let _ = unsafe { libc::fsync(dir.as_raw_fd()) };
                }
            }
        }

        Ok(())
    }
}

/// Cross-platform writer for binary cache files.
///
/// Each entry in the cache is a raw 32-byte big-endian X-coordinate. The file
/// is created on first use and may be pre-allocated with
/// [`FileCacheWriter::preallocate`] to reduce fragmentation.
///
/// See [ADR-0006](../docs/adr/0006-binary-cache-format.md) for the
/// on-disk format, pre-allocation strategy, and EOF-validity rules.
///
/// On Unix this implementation uses `pwrite` via [`std::os::unix::fs::FileExt`];
/// on other platforms it falls back to a mutex-protected seek-and-write. The
/// mutex contention is negligible because each write is a single batch of
/// ~1 KiB and occurs infrequently relative to ECC work.
///
/// # Thread safety
///
/// `FileCacheWriter` is `Send + Sync`: the inner [`File`] is wrapped in a
/// [`std::sync::Mutex`] that serialises writes. Concurrent [`CacheWriter`]
/// implementations may be created by sharing a `FileCacheWriter` via
/// `Arc<FileCacheWriter>` or by passing it directly to
/// [`crate::search::precompute_chunk`].
pub struct FileCacheWriter {
    file: std::sync::Mutex<File>,
}

impl FileCacheWriter {
    /// Creates a new cache file, creating parent directories as needed.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::Io`] if the file or its parent directories cannot
    /// be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use find::persistence::FileCacheWriter;
    /// use std::path::Path;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let writer = FileCacheWriter::create(Path::new("data/chunk_1.bin"))?;
    ///     let block = [0u8; 32 * 32]; // one batch of 32 X-coordinates
    ///     find::search::CacheWriter::write_block(&writer, 0, &block)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(FindError::Io)?;
        }
        let file = File::create(path).map_err(FindError::Io)?;
        Ok(Self {
            file: std::sync::Mutex::new(file),
        })
    }

    /// Pre-allocates the file to `len` bytes.
    ///
    /// This is a hint to the file system and may improve sequential-write
    /// performance. It is safe to call multiple times; subsequent calls will
    /// truncate or extend the file as needed.
    ///
    /// # Errors
    ///
    /// Returns [`FindError::Io`] if the file descriptor does not support
    /// truncation.
    ///
    /// # Performance
    ///
    /// On Linux, pre-allocation via `set_len` issues an `ftruncate` which
    /// reserves contiguous disk blocks and reduces fragmentation on
    /// append-heavy workloads. On filesystems that support extents
    /// (ext4, xfs, btrfs), this is a near-free operation once the file is
    /// created.
    pub fn preallocate(&self, len: u64) -> Result<()> {
        // The cache file is append-only and contention is rare; a poisoned
        // mutex implies another writer thread panicked mid-write, which we
        // cannot recover from safely.
        let file = self.file.lock().expect("file cache writer mutex poisoned");
        file.set_len(len).map_err(FindError::Io)?;
        Ok(())
    }
}

impl CacheWriter for FileCacheWriter {
    /// Writes a block of 32-byte X-coordinate entries at `offset`.
    ///
    /// # Performance
    ///
    /// On Unix this is a single `pwrite_at` call — no syscall for seeking,
    /// no per-thread state to coordinate. The underlying kernel call
    /// serialises against other writers via the file's `struct file`
    /// lock; userspace contention is limited to the [`std::sync::Mutex`]
    /// acquisition.
    ///
    /// On other platforms, the implementation falls back to
    /// `seek + write_all`, which is two syscalls per block. The
    /// additional syscall cost is amortised over the data block (~1 KiB),
    /// so the throughput penalty is small in practice.
    ///
    /// # Errors
    ///
    /// Returns any I/O error from the underlying write operation (e.g.
    /// `ENOSPC` on full disk, `EIO` on hardware fault).
    fn write_block(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        // Mutex poisoning means a writer thread panicked while holding the
        // lock; we cannot recover the file handle's state, so we abort.
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            let file = self.file.lock().expect("file cache writer mutex poisoned");
            file.write_all_at(data, offset)
        }
        #[cfg(not(unix))]
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = self.file.lock().expect("file cache writer mutex poisoned");
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(data)
        }
    }
}

/// Size of the stack-allocated scratch buffer used by [`perform_cached_sweep`].
///
/// 32 KiB is enough to amortise one `read` syscall per ~1000 X-coordinates
/// (each coordinate is 32 bytes). The buffer is stack-allocated to keep
/// the working set in L1 cache and to avoid heap pressure during the sweep.
const CACHED_SWEEP_BUF_SIZE: usize = 32 * 1024;

/// Performs an I/O-bound search against a pre-computed binary cache.
///
/// The cache is expected to contain a contiguous sequence of 32-byte
/// big-endian X-coordinates starting at `start_j`. The file is validated
/// (size must be a multiple of 32) before scanning begins.
///
/// # Arguments
///
/// * `index` — The variant index to match against.
/// * `cache_path` — Path to the binary cache file.
/// * `start_j` — The scalar value corresponding to the first entry in the file.
///
/// # Errors
///
/// Returns [`FindError::CacheCorrupted`] if the file size is not a multiple
/// of 32 bytes.
///
/// Returns [`FindError::Io`] on any read error other than clean EOF.
///
/// # Performance
///
/// Reads the file in 32 KiB chunks (≈1024 X-coordinates per syscall) into a
/// stack-allocated buffer, then walks the buffer in 32-byte slices. The
/// chunk size is small enough to keep the buffer in L1 cache but large
/// enough to keep `read` syscalls off the hot path. This replaces the
/// earlier `BufReader::read_exact(&mut [0u8; 32])` loop which, although
/// buffered internally, paid per-call overhead for every 32-byte match.
#[instrument(skip(index), level = "info")]
pub fn perform_cached_sweep(
    index: &VariantIndex,
    cache_path: &Path,
    start_j: u64,
) -> Result<Option<SearchMatch>> {
    let mut file = File::open(cache_path).map_err(FindError::Io)?;
    let metadata = file.metadata().map_err(FindError::Io)?;
    let file_size = metadata.len();

    if file_size % 32 != 0 {
        return Err(FindError::CacheCorrupted(format!(
            "Cache file size {} is not a multiple of 32 bytes",
            file_size
        )));
    }
    if file_size == 0 {
        return Ok(None);
    }

    let mut buffer = [0u8; CACHED_SWEEP_BUF_SIZE];
    let mut j = start_j;
    let mut buf_pos = CACHED_SWEEP_BUF_SIZE;
    let mut buf_len = CACHED_SWEEP_BUF_SIZE;

    loop {
        // Refill the buffer if the previous read drained it.
        if buf_pos >= buf_len {
            match file.read(&mut buffer) {
                Ok(0) => break, // clean EOF
                Ok(n) => {
                    buf_pos = 0;
                    buf_len = n;
                }
                Err(e) => return Err(FindError::Io(e)),
            }
        }

        // Slice out the next 32-byte X-coordinate and probe the index.
        let chunk: [u8; 32] = buffer[buf_pos..buf_pos + 32]
            .try_into()
            .expect("buffer slice is exactly 32 bytes");
        buf_pos += 32;

        if let Some(m) = index.match_x(&chunk, j) {
            return Ok(Some(m));
        }
        j += 1;
    }

    Ok(None)
}

/// Persists variant metadata to a JSON file for auditability.
///
/// The output file is named `points.json` and is placed inside `dir_path`.
/// It maps each variant's X-coordinate (hex) to its decimal offset string.
///
/// # Errors
///
/// Returns [`FindError::Io`] or [`FindError::SerializationError`] on failure.
///
/// # Returns
///
/// The absolute path of the written file.
///
/// # Performance
///
/// Output is sorted by X-coordinate (via [`std::collections::BTreeMap`])
/// so that the file is byte-stable across runs for the same public key.
/// This makes the file diff-friendly and useful for reproducibility checks.
/// Sorting adds an \(O(N \log N)\) cost where \(N\) is the variant count
/// (typically 512).
///
/// # Examples
///
/// ```no_run
/// use find::ecc;
/// use find::persistence::save_variants_to_json;
/// use find::search;
/// use k256::Scalar;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let target = ecc::scalar_mul_g(&Scalar::from(123u64));
///     let variants = search::generate_variants(&target);
///     let path = save_variants_to_json(&variants, "data")?;
///     println!("wrote {}", path);
///     Ok(())
/// }
/// ```
#[instrument(skip(variants, dir_path), level = "info")]
pub fn save_variants_to_json(variants: &[OffsetVariant], dir_path: &str) -> Result<String> {
    // BTreeMap (not HashMap) is used so that the on-disk JSON output is
    // deterministically ordered by X-coordinate. This makes the file
    // byte-stable across runs for the same public key, which is valuable
    // for audit diffing and reproducibility checks.
    let mut map = BTreeMap::new();
    for var in variants {
        let x_hex = hex::encode(var.x_bytes);
        map.insert(x_hex, var.offset.clone());
    }

    let json = serde_json::to_string_pretty(&map).map_err(FindError::SerializationError)?;
    fs::create_dir_all(dir_path).map_err(FindError::Io)?;

    let file_path = Path::new(dir_path).join("points.json");
    fs::write(&file_path, json).map_err(FindError::Io)?;

    Ok(file_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::generate_variants;
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use k256::Scalar;
    use tempfile::tempdir;

    /// Verifies that [`save_variants_to_json`] creates a valid JSON file.
    #[test]
    fn test_save_to_json_creates_points_file() {
        let target = crate::ecc::scalar_mul_g(&Scalar::from(100u64));
        let variants = generate_variants(&target);
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        let res = save_variants_to_json(&variants, dir_path);
        assert!(res.is_ok());
        assert!(dir.path().join("points.json").exists());
    }

    /// Verifies that an empty cache file yields `Ok(None)`.
    #[test]
    fn test_cached_sweep_empty_file() {
        let target = crate::ecc::scalar_mul_g(&Scalar::from(1u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("empty.bin");
        std::fs::write(&cache_path, []).unwrap();

        let result = perform_cached_sweep(&index, &cache_path, 0);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Verifies that a cache file whose size is not a multiple of 32 is rejected.
    #[test]
    fn test_cached_sweep_corrupted_size() {
        let target = crate::ecc::scalar_mul_g(&Scalar::from(1u64));
        let variants = generate_variants(&target);
        let index = VariantIndex::new(variants);

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("corrupted.bin");
        std::fs::write(&cache_path, vec![0u8; 31]).unwrap();

        let result = perform_cached_sweep(&index, &cache_path, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not a multiple of 32"));
    }

    /// Verifies end-to-end cache write and read-back with a known match.
    #[test]
    fn test_cached_sweep_write_and_read_back() {
        let d_scalar = Scalar::from(3u64);
        let p_d = crate::ecc::scalar_mul_g(&d_scalar);
        let index = VariantIndex::new(generate_variants(&p_d));

        let p_j = crate::ecc::scalar_mul_g(&Scalar::from(1u64));
        let x_1 = {
            let encoded = p_j.to_affine().to_encoded_point(false);
            let mut b = [0u8; 32];
            b.copy_from_slice(encoded.x().unwrap().as_ref());
            b
        };

        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("match.bin");

        let mut cache_data = Vec::new();
        cache_data.extend_from_slice(&x_1); // entry 0 -> j=1
        cache_data.extend_from_slice(&x_1); // entry 1 -> j=2
        std::fs::write(&cache_path, &cache_data).unwrap();

        let result = perform_cached_sweep(&index, &cache_path, 1).unwrap();
        let m = result.expect("Should have found a match at j=1");

        assert_eq!(m.small_scalar, 1, "Should match at j=1");
        assert!(
            m.candidates.contains(&"3".to_string()),
            "Candidate must include d=3, got: {:?}",
            m.candidates
        );
    }

    /// Verifies that [`Checkpoint::save_atomic`] and [`Checkpoint::load`] are inverses.
    #[test]
    fn test_checkpoint_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cp.json");

        let cp = Checkpoint {
            last_j: 42,
            pubkey: "abc".to_string(),
            last_x: "00".repeat(32),
        };
        cp.save_atomic(&path).unwrap();
        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(loaded.last_j, 42);
        assert_eq!(loaded.pubkey, "abc");
    }

    /// Verifies that [`Checkpoint::verify`] succeeds when pubkeys mismatch.
    #[test]
    fn test_checkpoint_verify_mismatch_pubkeys_is_ok() {
        let cp = Checkpoint {
            last_j: 0,
            pubkey: "abc".to_string(),
            last_x: "00".repeat(32),
        };
        assert!(cp.verify("def").is_ok());
    }

    /// Verifies that [`Checkpoint::verify`] succeeds for a valid anchor.
    #[test]
    fn test_checkpoint_verify_valid() {
        let last_j = 7u64;
        let expected_p = crate::ecc::scalar_mul_g(&Scalar::from(last_j));
        let expected_x = crate::ecc::to_hex_x(&expected_p);

        let cp = Checkpoint {
            last_j,
            pubkey: "dummy".to_string(),
            last_x: expected_x,
        };
        assert!(cp.verify("dummy").is_ok());
    }

    /// Verifies that [`Checkpoint::verify`] fails when the anchor is corrupted.
    #[test]
    fn test_checkpoint_verify_corrupted() {
        let last_j = 7u64;
        let expected_p = crate::ecc::scalar_mul_g(&Scalar::from(last_j));
        let expected_x = crate::ecc::to_hex_x(&expected_p);

        let cp = Checkpoint {
            last_j,
            pubkey: "dummy".to_string(),
            last_x: expected_x.replace('0', "1"),
        };
        assert!(cp.verify("dummy").is_err());
        assert!(cp
            .verify("dummy")
            .unwrap_err()
            .to_string()
            .contains("mismatch"));
    }

    /// Verifies that [`FileCacheWriter::create`] makes parent directories.
    #[test]
    fn test_file_cache_writer_create() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a/b/cache.bin");
        let writer = FileCacheWriter::create(&nested).unwrap();
        assert!(nested.exists());
        let meta = std::fs::metadata(&nested).unwrap();
        assert!(meta.is_file());

        writer.preallocate(64).unwrap();
        assert_eq!(std::fs::metadata(&nested).unwrap().len(), 64);
    }

    /// Verifies that [`FileCacheWriter`] can write blocks and read them back.
    #[test]
    fn test_file_cache_writer_write_and_read_back() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.bin");
        let writer = FileCacheWriter::create(&path).unwrap();

        let data = b"0123456789abcdef0123456789abcdef";
        writer.write_block(0, data).unwrap();
        writer.write_block(32, data).unwrap();

        let read_back = std::fs::read(&path).unwrap();
        assert_eq!(read_back.len(), 64);
        assert_eq!(&read_back[..32], &data[..]);
        assert_eq!(&read_back[32..], &data[..]);
    }

    // Property: random checkpoints roundtrip through `save_atomic` + `load` + `verify`.
    proptest::proptest! {
        #[test]
        fn prop_checkpoint_roundtrip_with_random_j(j in 0u64..1_000_000u64) {
            let dir = tempdir().unwrap();
            let path = dir.path().join("cp.json");

            // Compute the integrity anchor.
            let expected_p = crate::ecc::scalar_mul_g(&Scalar::from(j));
            let expected_x = crate::ecc::to_hex_x(&expected_p);

            let cp = Checkpoint {
                last_j: j,
                pubkey: "test_pubkey".to_string(),
                last_x: expected_x.clone(),
            };
            cp.save_atomic(&path).unwrap();
            let loaded = Checkpoint::load(&path).unwrap();

            proptest::prop_assert_eq!(loaded.last_j, j);
            proptest::prop_assert_eq!(loaded.pubkey.as_str(), "test_pubkey");
            proptest::prop_assert_eq!(loaded.last_x.as_str(), expected_x.as_str());

            // Verify must pass for the matching pubkey.
            proptest::prop_assert!(loaded.verify("test_pubkey").is_ok());
        }
    }
}
