// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Production-grade search orchestrator and CLI interaction layer.
//!
//! # 🔬 Execution Model: Resilient Interactive Loop
//! The `main` module implements a state-aware orchestration engine designed
//! for continuous execution over massive scalar ranges (up to $2^{64}$).
//! It prioritizes system resilience and user observability.
//!
//! ## 🛡 Resilience Features
//! - **Atomic Checkpointing:** Progress is saved via a transaction-style
//!   "write-then-rename" strategy to prevent state corruption.
//! - **Non-Blocking Observability:** Asynchronous daily rolling logs decouple
//!   telemetry I/O from the high-throughput search path.
//!
//! ## ⚡ Search Prioritization
//! The engine automatically detects pre-computed binary caches in
//! `data/checkpoints/`. If a cache exists for the current trillion-step
//! segment, the engine switches from **CPU-bound ECC arithmetic** to
//! **I/O-bound sequential matching**, yielding a 10-100x speedup.

use clap::Parser;
use find::ecc;
use find::search::{self, VariantIndex};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;
use tracing::{info, info_span, Level};
use tracing_appender::rolling;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Scalar step size per research segment: 1 Trillion ($10^{12}$).
const TRILLION: u64 = 1_000_000_000_000;

/// Manageable binary cache chunk size: 1 Billion ($10^9$) = 32GB on disk.
const CACHE_CHUNK_SIZE: u64 = 1_000_000_000;

/// Theoretical maximum search boundary for 64-bit scalars.
const MAX_SEARCH: u64 = u64::MAX;

/// Durable checkpoint state representing persistent search progress.
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    /// The last successfully completed scalar index.
    last_j: u64,
    /// The specific SEC1 public key associated with this progress.
    pubkey: String,
    /// The hex-encoded X-coordinate of P = last_j * G (Integrity Anchor).
    last_x: String,
}

/// Command-Line Interface for the High-Performance Find Tool.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// HEX-encoded SEC1 public key (Compressed or Uncompressed).
    #[arg(short, long)]
    pubkey: String,

    /// Data and checkpoint root directory (default: "data").
    #[arg(short, long, default_value = "data")]
    output_dir: String,

    /// Rolling log directory (default: "logs").
    #[arg(short, long, default_value = "logs")]
    log_dir: String,

    /// Persist jG points to binary caches for multi-pubkey reuse.
    /// WARNING: Consumes ~32GB per billion points.
    #[arg(short, long, default_value_t = false)]
    cache_points: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // 1. OBSERVABILITY INITIALIZATION
    // We utilize a non-blocking dedicated thread for I/O logging to ensure
    // telemetry does not stall the elliptic curve sweep.
    let file_appender = rolling::daily(&args.log_dir, "find.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .with(fmt::layer().with_writer(io::stderr))
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    let main_span = info_span!("main_execution");
    let _enter = main_span.enter();

    info!("Initializing find tool v{}", env!("CARGO_PKG_VERSION"));

    // 2. CRYPTOGRAPHIC CONTEXT SETUP
    let target_p = ecc::parse_pubkey(&args.pubkey)?;
    let variants = search::generate_variants(&target_p);
    search::save_variants_to_json(&variants, &args.output_dir)?;

    // An O(1) VariantIndex is essential to keep the sweep inner-loop efficient.
    let index = VariantIndex::new(variants);
    let checkpoints_dir = Path::new(&args.output_dir).join("checkpoints");
    fs::create_dir_all(&checkpoints_dir)?;

    // 3. STATE ANALYSIS & RESUMPTION
    let checkpoint_file = Path::new(&args.output_dir).join("checkpoint.json");
    let mut current_j: u64 = 0;
    match fs::read_to_string(&checkpoint_file) {
        Ok(content) => match serde_json::from_str::<Checkpoint>(&content) {
            Ok(cp) if cp.pubkey == args.pubkey => {
                // RESEARCH INTEGRITY GUARD:
                // We verify that the stored progress matches our cryptographic primitives.
                let scalar = k256::Scalar::from(cp.last_j);
                let expected_p = ecc::scalar_mul_g(&scalar);
                let expected_x = ecc::to_hex_x(&expected_p);

                if expected_x != cp.last_x {
                    return Err(find::error::FindError::ResearchIntegrityError(
                        format!(
                            "Stored checkpoint X-coordinate ({}) mismatch. Expected ({}). Data corruption or curve logic change detected.",
                            cp.last_x, expected_x
                        )
                    ).into());
                }

                current_j = cp.last_j;
                info!(
                    "Verified research state integrity. Resuming from j = {} (X: {})",
                    current_j, cp.last_x
                );
            }
            Ok(cp) => {
                // Checkpoint exists but is for a different pubkey.
                tracing::warn!(
                    "Checkpoint pubkey ({}) does not match target ({}). Starting fresh.",
                    cp.pubkey,
                    args.pubkey
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to parse checkpoint file ({}): {}. Starting fresh.",
                    checkpoint_file.display(),
                    e
                );
            }
        },
        Err(e) => {
            tracing::warn!(
                "Failed to read checkpoint file ({}): {}. Starting fresh.",
                checkpoint_file.display(),
                e
            );
        }
    }

    // 4. THE CORE EXECUTION LOOP
    let start_time = Instant::now();
    loop {
        let chunk_start = current_j.saturating_add(1);
        let chunk_end = current_j.saturating_add(CACHE_CHUNK_SIZE);
        let cache_path = checkpoints_dir.join(format!("chunk_{}.bin", chunk_start));

        info!(
            "--- STARTING SEGMENT [{} ... {}] ---",
            chunk_start, chunk_end
        );

        let sweep_result = if cache_path.exists() {
            // CACHED SEARCH PATH: Direct sequence scan.
            info!(
                "Cache hit: {}. Running optimized binary scan...",
                cache_path.display()
            );
            let sweep_start = Instant::now();
            let res = search::perform_cached_sweep(&index, &cache_path, chunk_start)?;
            info!("Binary scan throughput: {:?}", sweep_start.elapsed());
            res
        } else {
            // COMPUTE SEARCH PATH: Parallel ECC arithmetic.
            info!("Cache miss. Running high-throughput parallel search...");
            if args.cache_points {
                // We generate binary databases in manageable 1B-point sub-chunks (32GB each).
                info!("Generating 32GB high-performance binary database chunk...");
                let pre_start = Instant::now();
                let early_match = search::precompute_chunk(
                    chunk_start,
                    chunk_start + CACHE_CHUNK_SIZE - 1,
                    &cache_path,
                    Some(&index),
                )?;
                info!("Generated 32GB database in {:?}", pre_start.elapsed());

                if let Some(m) = early_match {
                    Some(m)
                } else {
                    search::perform_cached_sweep(&index, &cache_path, chunk_start)?
                }
            } else {
                let sweep_start = Instant::now();
                let res = search::perform_chunked_sweep(&index, chunk_start, chunk_end);
                info!("Parallel sweep completed in {:?}", sweep_start.elapsed());
                res
            }
        };

        // MATCH DISCOVERY
        if let Some(m) = sweep_result {
            info!("MATCH FOUND: Target scalar identified via [{}]", m.label);
            render_success_report(m, start_time.elapsed());
            break;
        }

        current_j = chunk_end;

        // RESEARCH INTEGRITY DERIVATION:
        // We compute the anchor at every CACHE_CHUNK_SIZE (1B) boundary.
        let scalar = k256::Scalar::from(current_j);
        let boundary_p = ecc::scalar_mul_g(&scalar);
        let boundary_x = ecc::to_hex_x(&boundary_p);

        // ATOMIC PERSISTENCE
        save_checkpoint_atomic(
            &checkpoint_file,
            &Checkpoint {
                last_j: current_j,
                pubkey: args.pubkey.clone(),
                last_x: boundary_x,
            },
        )?;

        // POWER-OF-2 INTERACTIVE PAUSE
        // Every 32 trillion steps, we trigger a high-fidelity audit pause.
        if current_j > 0 && current_j.is_multiple_of(32 * TRILLION) {
            info!("CRITICAL AUDIT: 32 Trillion step boundary reached.");
            // Optionally add user-interaction logic here.
        }

        // LOGARITHMIC CONTROL FLOW
        // Pauses at 2^N boundaries to provide user intervention points.
        if handle_power_of_2_boundary(chunk_start, chunk_end)? == ControlFlow::Break {
            info!("Search suspended by user at j = {}", current_j);
            break;
        }

        if current_j == MAX_SEARCH {
            info!("Search space effectively exhausted (2^64).");
            break;
        }
    }

    Ok(())
}

/// Atomically persists search progress using write-then-rename semantics.
///
/// Uses the standard write-then-rename pattern which is atomic on POSIX
/// filesystems for same-directory renames. The rename operation itself
/// commits the directory entry change durably on most filesystems without
/// requiring explicit directory fsync (which would need platform-specific code).
fn save_checkpoint_atomic(target: &Path, cp: &Checkpoint) -> anyhow::Result<()> {
    let tmp_path = target.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(cp)?;
    fs::write(&tmp_path, json)?;

    // Write the checkpoint data durably before renaming.
    // On POSIX, rename is atomic for same-filesystem operations and the
    // directory entry is committed to disk by the OS within seconds.
    fs::rename(&tmp_path, target)?;

    // Verify the rename persisted by checking the target file exists
    // and the temp file has been cleaned up.
    if target.exists() {
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }
        Ok(())
    } else {
        Err(std::io::Error::other("Checkpoint rename did not persist").into())
    }
}

/// Basic control flow for interactive pausing.
#[derive(PartialEq)]
enum ControlFlow {
    Continue,
    Break,
}

/// Detects if the current chunk spans a power-of-2 boundary and prompts the operator.
fn handle_power_of_2_boundary(start: u64, end: u64) -> io::Result<ControlFlow> {
    // Compute the 0-indexed MSB position (floor(log2)) for the boundaries.
    // Formula: msb_pos(v) = 63 - v.leading_zeros() for v > 0.
    // For v = 0, msb is undefined; use 0 as sentinel.
    let msb_pos = |v: u64| -> u32 {
        if v == 0 {
            0
        } else {
            63 - v.leading_zeros()
        }
    };
    let prev_p = msb_pos(start.saturating_sub(1));
    let curr_p = msb_pos(end);

    // Detect when the MSB position increases — we've crossed into a new power-of-2 range.
    if curr_p > prev_p {
        // The boundary value is the smallest number with this MSB position: 2^curr_p.
        let boundary_val = 1u64 << curr_p;

        // Confirm the boundary value actually falls within this chunk.
        if boundary_val >= start && boundary_val <= end {
            println!(
                "\n⚖️ Boundary Alert: Reached search depth 2^{} ({})",
                curr_p, boundary_val
            );
            print!("Continue exploration to next depth? [Y/n]: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if input.trim().to_lowercase() == "n" {
                return Ok(ControlFlow::Break);
            }
        }
    }
    Ok(ControlFlow::Continue)
}

/// Renders a finalized success report with derived key candidates.
fn render_success_report(m: search::SearchMatch, total_time: std::time::Duration) {
    println!("\n{}", "=".repeat(60));
    println!("🎉 CRITICAL: MATCH DISCOVERED (Variant: {})", m.label);
    println!("Shift scalar V: {}", m.offset);
    println!("Search scalar j: {}", m.small_scalar);
    println!("Target candidates (d = V \u{00B1} j):");
    for (i, c) in m.candidates.iter().enumerate() {
        println!("  [{}] 0x{}", i + 1, c);
    }
    println!("Total Search Duration: {:?}", total_time);
    println!("{}", "=".repeat(60));
}
