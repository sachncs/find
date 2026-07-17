// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-level search session orchestration.
//!
//! Owns the execution loop, checkpoint lifecycle, and strategy selection
//! (cached vs compute-bound). Contains no ECC arithmetic and no direct I/O
//! beyond delegating to [`persistence`].
//!
//! The [`Config`] type and the related constants live in [`crate::config`].
//!
//! # Session lifecycle
//!
//! ```mermaid
//! flowchart TD
//!     A[validate Config] --> B[parse pubkey]
//!     B --> C[generate 512 variants]
//!     C --> D[save points.json]
//!     D --> E[build VariantIndex]
//!     E --> F{checkpoint exists<br/>with same pubkey?}
//!     F -- yes --> G[verify integrity anchor]
//!     G -- ok --> H[resume at last_j]
//!     G -- mismatch --> X[ResearchIntegrityError]
//!     F -- no --> I[start fresh at j = 0]
//!     H --> J[loop: per chunk of 1B scalars]
//!     I --> J
//!     J --> K{cache file<br/>exists?}
//!     K -- yes --> L[sweep_cached]
//!     K -- no --> M{cache_points<br/>enabled?}
//!     M -- yes --> N[sweep_and_cache + cached_sweep]
//!     M -- no --> O[sweep_parallel]
//!     L --> P{match<br/>found?}
//!     N --> P
//!     O --> P
//!     P -- yes --> Q[return Some match]
//!     P -- no --> R[advance current_j<br/>save atomic checkpoint]
//!     R --> S{current_j ==<br/>MAX_SEARCH?}
//!     S -- yes --> T[return None]
//!     S -- no --> J
//! ```
//!
//! # Strategy selection
//!
//! For each chunk of `DEFAULT_CACHE_CHUNK_SIZE` scalars the orchestrator
//! picks one of three strategies, in this priority order:
//!
//! 1. **Cache hit** — replay the precomputed X-coordinates from disk via
//!    [`persistence::sweep_cached`].
//! 2. **Cache miss with `cache_points`** — precompute the cache via
//!    [`search::sweep_and_cache`] (writing X-coords to disk and checking
//!    the index live). If a match surfaces mid-precompute, the redundant
//!    cached-sweep pass is skipped.
//! 3. **Cache miss without caching** — pure CPU-bound parallel sweep via
//!    [`search::sweep_parallel`], discarding the work after the
//!    segment.
//!
//! # Checkpoint durability
//!
//! Checkpoints are written atomically (write-then-rename + parent-dir
//! `fsync` on Unix) by [`persistence::Checkpoint::save_atomic`]. The
//! integrity anchor (X-coordinate of `last_j · G`) is recomputed at every
//! segment boundary so that a future resume can detect corruption. See
//! [ADR-0003](../docs/adr/0003-atomic-checkpointing.md).
//!
//! # Thread safety
//!
//! [`run`] is single-threaded at the top level. It does spawn its own
//! Rayon worker pool internally via the search-engine entry points.
//! Re-entrant calls are safe as long as each call uses a distinct
//! output directory.

use crate::config::{Config, DEFAULT_CACHE_CHUNK_SIZE, MAX_SEARCH, MIN_SEARCH_SCALAR, TRILLION};
use crate::ecc;
use crate::error::{FindError, Result};
use crate::persistence;
use crate::search::{self, Progress, SearchMatch, VariantIndex};
use std::path::Path;
use tracing::{info, warn};

/// Runs a complete search session.
///
/// The session proceeds in chunks of `DEFAULT_CACHE_CHUNK_SIZE` scalars. For each
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
///
/// # Examples
///
/// ```no_run
/// use find::config::Config;
/// use find::orchestrator;
///
/// fn main() -> Result<(), Box<dyn core::error::Error>> {
///     let cfg = Config::new(
///         "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
///         "data",
///         false,
///     );
///     match orchestrator::run(&cfg)? {
///         Some(m) => println!("match: {:?}", m),
///         None => println!("no match found in 64-bit space"),
///     }
///     Ok(())
/// }
/// ```
pub fn run(config: &Config) -> Result<Option<SearchMatch>> {
    config.validate_fields()?;
    config.validate_pubkey()?;

    // Address-targeted discovery mode: skip variant construction entirely.
    // The sweep is over the user-specified range with hash40 comparison
    // instead of X-coordinate comparison.
    if let Some(target) = config.target_address {
        return run_address_mode(config, target);
    }

    let target_p = ecc::parse_pubkey(&config.pubkey)?;
    let variants = search::generate_variants(&target_p);
    let variant_x_bytes = search::compute_variant_x_bytes(&target_p);
    persistence::write_variants_json(variants, &variant_x_bytes, &config.output_dir)?;

    let index = VariantIndex::new(variants, &variant_x_bytes);
    let checkpoints_dir = Path::new(&config.output_dir).join("checkpoints");
    std::fs::create_dir_all(&checkpoints_dir).map_err(FindError::Io)?;

    let checkpoint_file = Path::new(&config.output_dir).join("checkpoint.json");
    let mut current_j: u64;

    match persistence::Checkpoint::load(&checkpoint_file) {
        Ok(cp) if cp.pubkey == config.pubkey => {
            cp.verify(&config.pubkey)?;
            current_j = cp.last_j;
            info!("Verified integrity. Resuming from j = {}", current_j);
        }
        Ok(_) => {
            warn!("Checkpoint pubkey mismatch. Starting fresh.");
            current_j = 0;
        }
        Err(e) => {
            warn!("No valid checkpoint: {}. Starting fresh.", e);
            current_j = 0;
        }
    }

    let progress = Progress::new();

    loop {
        let chunk_start = current_j.saturating_add(1).max(MIN_SEARCH_SCALAR);
        // Detect overflow: `saturating_add` returns `u64::MAX` on overflow,
        // so the comparison `chunk_end < current_j` fires only when we've
        // reached the end of the 64-bit scalar space and cannot extend.
        let chunk_end = current_j.saturating_add(DEFAULT_CACHE_CHUNK_SIZE);
        if chunk_end < current_j {
            info!("Search space exhausted (overflow detected).");
            break;
        }

        // One cache file per chunk, named by the chunk's inclusive lower
        // bound. Reusing an existing cache file replays the segment from
        // disk on subsequent runs.
        let cache_path = checkpoints_dir.join(format!("chunk_{chunk_start}.bin"));

        info!(
            "--- STARTING SEGMENT [{} ... {}] ---",
            chunk_start, chunk_end
        );

        // Three execution strategies, in priority order:
        //   (a) cache hit  — replay the precomputed X-coordinates from disk;
        //   (b) cache miss + cache_points — precompute the cache, writing
        //       X-coords to disk and checking the index live; if a match
        //       surfaces mid-precompute, `early` short-circuits before
        //       re-running the (now-complete) cached sweep;
        //   (c) cache miss, no caching — pure CPU-bound parallel sweep,
        //       discarding the work after the segment.
        let sweep_result = if cache_path.exists() {
            info!("Cache hit: {}", cache_path.display());
            persistence::sweep_cached(&index, &cache_path, chunk_start)?
        } else if config.cache_points {
            info!("Cache miss. Precomputing chunk...");
            let writer = persistence::BinaryCacheWriter::create(&cache_path)?;
            let expected_len = (chunk_end.saturating_sub(chunk_start).saturating_add(1)) * 32;
            writer.preallocate(expected_len)?;

            let early = search::sweep_and_cache(
                chunk_start,
                chunk_end,
                &writer,
                Some(&index),
                &progress,
                config.batch_size.get(),
            )?;

            if early.is_some() {
                // A match was found mid-precompute; skip the redundant
                // cached-sweep pass on the just-written file.
                early
            } else {
                persistence::sweep_cached(&index, &cache_path, chunk_start)?
            }
        } else {
            info!("Cache miss. Running parallel sweep...");
            search::sweep_parallel(&index, chunk_start, chunk_end, config.batch_size.get())
        };

        if let Some(m) = sweep_result {
            info!("MATCH FOUND: {}", m.label);
            return Ok(Some(m));
        }

        // Advance the cursor and persist a checkpoint even when the
        // current segment found nothing — the integrity anchor (last_x)
        // is recomputed at the segment boundary so a future resume can
        // verify that the checkpoint's reported scalar is consistent
        // with the original public key. See ADR-0003.
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

        // 32 × TRILLION = 32 chunks of DEFAULT_CACHE_CHUNK_SIZE (1B) scalars
        // each. Useful as a coarse-grained audit-breadcrumb in long runs.
        // Structured fields so downstream log aggregation can graph the
        // rate of progress without regex parsing.
        if current_j > 0 && current_j % (32 * TRILLION) == 0 {
            info!(current_j = current_j, "audit_boundary");
        }

        if current_j == MAX_SEARCH {
            info!("Search space exhausted.");
            break;
        }
    }

    Ok(None)
}

/// Runs the address-targeted discovery mode (a single-pass sweep that
/// compares compressed-pubkey hash40 against the user's target address).
///
/// Replaces the variant-keyed path entirely. The range is whatever
/// `config.range_from..=config.range_to` is; defaults to `[MIN_SEARCH_SCALAR,
/// MAX_SEARCH]` when both are `None`.
///
/// No checkpoints are persisted in this mode: the search is a
/// one-shot question over a fixed range. No cache file is generated.
/// No binary file is reserved on disk. All output is a single
/// `Some(SearchMatch)` on hit or `Ok(None)` on clean exit.
fn run_address_mode(
    config: &Config,
    target: crate::address::Address40,
) -> Result<Option<SearchMatch>> {
    let start = config
        .range_from
        .unwrap_or(crate::config::MIN_SEARCH_SCALAR);
    let end = config.range_to.unwrap_or(crate::config::MAX_SEARCH);
    info!("address mode: target={} range=[{}, {}]", target, start, end);
    let variants = search::generate_variants(&ecc::generator());
    Ok(search::sweep_address(
        start,
        end,
        config.batch_size.get(),
        target,
        variants,
    ))
}
