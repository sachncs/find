// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-level search session orchestration.
//!
//! Owns the execution loop, checkpoint lifecycle, and strategy selection
//! (cached vs compute-bound). Contains no ECC arithmetic and no direct I/O
//! beyond delegating to [`persistence`].

use crate::ecc;
use crate::error::{FindError, Result};
use crate::persistence;
use crate::search::{self, Progress, SearchMatch, VariantIndex};
use std::path::Path;
use tracing::{info, warn};

/// Scalar step size per research segment: 1 Trillion.
const TRILLION: u64 = 1_000_000_000_000;

/// Manageable binary cache chunk size: 1 Billion = 32GB on disk.
const CACHE_CHUNK_SIZE: u64 = 1_000_000_000;

/// Theoretical maximum search boundary for 64-bit scalars.
const MAX_SEARCH: u64 = u64::MAX;

/// Configuration required to drive a search session.
///
/// All fields are owned strings so that the configuration can outlive the
/// CLI argument parser.
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

/// Runs a complete search session.
///
/// The session proceeds in chunks of [`CACHE_CHUNK_SIZE`] scalars. For each
/// chunk the orchestrator:
///
/// 1. Checks whether a binary cache already exists.
/// 2. If a cache exists, performs an I/O-bound scan.
/// 3. Otherwise, either pre-computes a cache (if `config.cache_points` is
///    true) or runs a CPU-bound parallel sweep.
/// 4. If no match is found, saves an atomic checkpoint and continues.
///
/// If a previous checkpoint exists for the same public key, the search
/// resumes from the stored scalar index after verifying the integrity anchor.
///
/// # Arguments
///
/// * `config` — The search configuration.
///
/// # Returns
///
/// - `Ok(Some(match))` when a candidate is discovered.
/// - `Ok(None)` when the entire 64-bit scalar space is exhausted.
///
/// # Errors
///
/// Returns [`FindError::ResearchIntegrityError`] if an existing checkpoint
/// fails anchor verification.
///
/// Returns [`FindError::Io`] on checkpoint or cache I/O failures.
pub fn run(config: &Config) -> Result<Option<SearchMatch>> {
    let target_p = ecc::parse_pubkey(&config.pubkey)?;
    let variants = search::generate_variants(&target_p);
    persistence::save_variants_to_json(&variants, &config.output_dir)?;

    let index = VariantIndex::new(variants);
    let checkpoints_dir = Path::new(&config.output_dir).join("checkpoints");
    std::fs::create_dir_all(&checkpoints_dir).map_err(FindError::Io)?;

    let checkpoint_file = Path::new(&config.output_dir).join("checkpoint.json");
    let mut current_j: u64 = 0;

    match persistence::Checkpoint::load(&checkpoint_file) {
        Ok(cp) if cp.pubkey == config.pubkey => {
            cp.verify(&config.pubkey)?;
            current_j = cp.last_j;
            info!("Verified integrity. Resuming from j = {}", current_j);
        }
        Ok(_) => {
            warn!("Checkpoint pubkey mismatch. Starting fresh.");
        }
        Err(e) => {
            warn!("No valid checkpoint: {}. Starting fresh.", e);
        }
    }

    let progress = Progress::new();

    loop {
        let chunk_start = current_j.saturating_add(1);
        let chunk_end = current_j.saturating_add(CACHE_CHUNK_SIZE);
        let cache_path = checkpoints_dir.join(format!("chunk_{}.bin", chunk_start));

        info!("--- STARTING SEGMENT [{} ... {}] ---", chunk_start, chunk_end);

        let sweep_result = if cache_path.exists() {
            info!("Cache hit: {}", cache_path.display());
            persistence::perform_cached_sweep(&index, &cache_path, chunk_start
            )?
        } else if config.cache_points {
            info!("Cache miss. Precomputing chunk...");
            let writer = persistence::FileCacheWriter::create(&cache_path)?;
            let expected_len = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) * 32;
            writer.preallocate(expected_len)?;

            let early = search::precompute_chunk(
                chunk_start,
                chunk_end,
                &writer,
                Some(&index),
                &progress,
            )?;

            if early.is_some() {
                early
            } else {
                persistence::perform_cached_sweep(
                    &index, &cache_path, chunk_start
                )?
            }
        } else {
            info!("Cache miss. Running parallel sweep...");
            search::perform_chunked_sweep(&index, chunk_start, chunk_end
            )
        };

        if let Some(m) = sweep_result {
            info!("MATCH FOUND: {}", m.label);
            return Ok(Some(m));
        }

        current_j = chunk_end;
        let boundary_scalar = k256::Scalar::from(current_j);
        let boundary_p = ecc::scalar_mul_g(&boundary_scalar);
        let boundary_x = ecc::to_hex_x(&boundary_p);

        persistence::Checkpoint {
            last_j: current_j,
            pubkey: config.pubkey.clone(),
            last_x: boundary_x,
        }
        .save_atomic(&checkpoint_file)?;

        if current_j > 0 && current_j.is_multiple_of(32 * TRILLION) {
            info!("Audit boundary: 32 trillion steps reached.");
        }

        if current_j == MAX_SEARCH {
            info!("Search space exhausted.");
            break;
        }
    }

    Ok(None)
}
