// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Persistence layer: atomic checkpoints, binary caches, and JSON exports.
//!
//! All I/O side effects are isolated here so that [`search`] remains a pure
//! domain module. Consumers should use [`Checkpoint`] for durable progress
//! and [`FileCacheWriter`] for binary cache generation.

use crate::ecc;
use crate::error::{FindError, Result};
use crate::search::{CacheWriter, OffsetVariant, SearchMatch, VariantIndex};
use k256::Scalar;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::Path;
use tracing::instrument;

/// Durable checkpoint representing persistent search progress.
///
/// A checkpoint stores the last completed scalar index, the associated public
/// key, and an integrity anchor (the X-coordinate of `last_j * G`). The
/// anchor allows [`Checkpoint::verify`] to detect corruption.
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
    ///
    /// # Errors
    ///
    /// Returns [`FindError::Io`] or [`FindError::SerializationError`] on failure.
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
        Ok(())
    }
}

/// Cross-platform writer for binary cache files.
///
/// Each entry in the cache is a raw 32-byte big-endian X-coordinate. The file
/// is created on first use and may be pre-allocated with
/// [`FileCacheWriter::preallocate`] to reduce fragmentation.
///
/// On Unix this implementation uses `pwrite` via [`std::os::unix::fs::FileExt`];
/// on other platforms it falls back to a mutex-protected seek-and-write. The
/// mutex contention is negligible because each write is a single batch of
/// ~1 KiB and occurs infrequently relative to ECC work.
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
    pub fn preallocate(&self, len: u64) -> Result<()> {
        let file = self.file.lock().unwrap();
        file.set_len(len).map_err(FindError::Io)?;
        Ok(())
    }
}

impl CacheWriter for FileCacheWriter {
    fn write_block(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            let file = self.file.lock().unwrap();
            file.write_all_at(data, offset)
        }
        #[cfg(not(unix))]
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(data)
        }
    }
}

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
#[instrument(skip(index), level = "info")]
pub fn perform_cached_sweep(
    index: &VariantIndex,
    cache_path: &Path,
    start_j: u64,
) -> Result<Option<SearchMatch>> {
    let file = File::open(cache_path).map_err(FindError::Io)?;
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

    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; 32];
    let mut j = start_j;

    loop {
        match reader.read_exact(&mut buffer) {
            Ok(()) => {
                if let Some(m) = index.match_x(&buffer, j) {
                    return Ok(Some(m));
                }
                j += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(FindError::Io(e)),
        }
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
#[instrument(skip(variants, dir_path), level = "info")]
pub fn save_variants_to_json(variants: &[OffsetVariant], dir_path: &str) -> Result<String> {
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
    use crate::search::{generate_variants};
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
}
