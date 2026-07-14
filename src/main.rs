// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Thin CLI wrapper and tracing bootstrap.
//!
//! All domain logic lives in [`find::orchestrator`]; this file only parses
//! arguments, initializes observability, and renders results.
//!
//! # Lifecycle
//!
//! The binary performs five steps in order:
//!
//! 1. Parse CLI flags with [`clap`].
//! 2. Install the Rayon panic handler so worker panics are logged rather
//!    than aborting the process.
//! 3. Initialise tracing (daily-rolling file appender + stderr layer).
//! 4. Construct a [`find::config::Config`] and delegate to
//!    [`find::orchestrator::run`].
//! 5. Render the [`find::search::SearchMatch`] to stdout, or print a
//!    "no match" message if the entire scalar space was exhausted.
//!
//! # Errors
//!
//! Errors from the orchestrator propagate as [`anyhow::Error`] and produce a
//! non-zero exit status. Argument parse errors are emitted by `clap`.
//!
//! # Threading
//!
//! The binary is single-threaded at the top level; the orchestrator manages
//! its own Rayon worker pool internally.

use clap::Parser;
use find::config::Config;
use find::orchestrator;
use find::telemetry::{init_tracing, install_rayon_panic_handler};
use std::time::Instant;
use tracing::{info, info_span};

/// Command-line interface for the secp256k1 find tool.
///
/// This struct is private to the binary; the library exposes
/// [`find::config::Config`] for programmatic users.
///
/// # Examples
///
/// ```ignore
/// use clap::Parser;
///
/// #[derive(Parser)]
/// struct Args {
///     #[arg(short, long)]
///     pubkey: String,
/// }
///
/// let args = Args::parse_from(["find", "--pubkey", "0279be..."]);
/// assert_eq!(args.pubkey, "0279be...");
/// ```
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// HEX-encoded SEC1 public key (compressed or uncompressed).
    #[arg(short, long)]
    pubkey: String,

    /// Data and checkpoint root directory.
    #[arg(short, long, default_value = "data")]
    output_dir: String,

    /// Rolling log directory.
    #[arg(short, long, default_value = "logs")]
    log_dir: String,

    /// Persist jG points to binary caches for multi-pubkey reuse.
    ///
    /// WARNING: Consumes approximately 32GB per billion points.
    #[arg(short, long, default_value_t = false)]
    cache_points: bool,

    /// Number of points per Montgomery batch-normalization.
    ///
    /// 32 is the default. Smaller values reduce per-batch stack
    /// usage; larger values amortise the single Montgomery
    /// inversion across more points.
    #[arg(long, default_value_t = find::config::DEFAULT_BATCH_SIZE)]
    batch_size: u32,

    /// Number of shift variants to generate (256 + 256 max).
    ///
    /// 512 is the documented default. Smaller values reduce the
    /// variant-set memory footprint at the cost of missing some
    /// small-scalar targets.
    #[arg(long, default_value_t = find::config::DEFAULT_VARIANT_COUNT)]
    variants: u32,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    install_rayon_panic_handler();
    let _guard = init_tracing(&args.log_dir)?;

    let main_span = info_span!("main_execution");
    let _enter = main_span.enter();

    info!("Initializing find tool v{}", env!("CARGO_PKG_VERSION"));

    // Build the config; the with_* builders assert on out-of-range
    // values, so we validate the user-provided args up front.
    let mut config = Config::new(args.pubkey, args.output_dir, args.cache_points);
    if !(1..=find::config::MAX_BATCH_SIZE).contains(&args.batch_size) {
        return Err(anyhow::anyhow!(
            "--batch-size must be in 1..={}",
            find::config::MAX_BATCH_SIZE
        ));
    }
    if !(1..=find::config::MAX_VARIANT_COUNT).contains(&args.variants) {
        return Err(anyhow::anyhow!(
            "--variants must be in 1..={}",
            find::config::MAX_VARIANT_COUNT
        ));
    }
    config = config
        .with_batch_size(args.batch_size)
        .with_variant_count(args.variants);

    let start = Instant::now();
    match orchestrator::run(&config)? {
        Some(m) => render_success_report(m, start.elapsed()),
        None => println!("Search completed. No match found."),
    }

    Ok(())
}

/// Prints a formatted success report to stdout.
///
/// The output is a fixed-width ASCII banner containing the matched variant
/// label, the shift scalar `V`, the discovered search scalar `j`, the two
/// candidate private keys `V ± j (mod n)`, and the wall-clock duration.
///
/// This function performs no I/O beyond stdout and does not panic under
/// normal [`find::search::SearchMatch`] input.
fn render_success_report(m: find::search::SearchMatch, total_time: std::time::Duration) {
    // Build the full banner in a single `String` (one allocation) and write
    // it once. The previous per-line `println!` did nine separate
    // `format!`-style allocations plus nine `STDOUT_LOCK` acquisitions.
    let separator = "=".repeat(60);
    let mut out = String::with_capacity(512);
    out.push('\n');
    out.push_str(&separator);
    out.push('\n');
    use std::fmt::Write;
    let _ = writeln!(out, "MATCH DISCOVERED (Variant: {})", m.label);
    let _ = writeln!(out, "Shift scalar V: {}", m.offset);
    let _ = writeln!(out, "Search scalar j: {}", m.small_scalar);
    out.push_str("Target candidates (d = V +/- j):\n");
    for (i, c) in m.candidates.iter().enumerate() {
        let _ = writeln!(out, "  [{}] 0x{}", i + 1, c);
    }
    let _ = writeln!(out, "Total Search Duration: {:?}", total_time);
    out.push_str(&separator);
    out.push('\n');
    print!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that [`Args`] parses with minimal required arguments.
    #[test]
    fn test_args_parse_minimal() {
        let args = Args::parse_from(["find", "--pubkey", "abc"]);
        assert_eq!(args.pubkey, "abc");
        assert_eq!(args.output_dir, "data");
        assert_eq!(args.log_dir, "logs");
        assert!(!args.cache_points);
    }

    /// Verifies that [`Args`] parses with all flags set.
    #[test]
    fn test_args_parse_full() {
        let args = Args::parse_from([
            "find",
            "--pubkey",
            "deadbeef",
            "--output-dir",
            "/tmp/out",
            "--log-dir",
            "/tmp/log",
            "--cache-points",
            "--batch-size",
            "64",
            "--variants",
            "256",
        ]);
        assert_eq!(args.pubkey, "deadbeef");
        assert_eq!(args.output_dir, "/tmp/out");
        assert_eq!(args.log_dir, "/tmp/log");
        assert!(args.cache_points);
        assert_eq!(args.batch_size, 64);
        assert_eq!(args.variants, 256);
    }

    /// Verifies that `Args` accepts the `--batch-size` and `--variants`
    /// defaults when not specified.
    #[test]
    fn test_args_defaults_for_tuning() {
        let args = Args::parse_from(["find", "--pubkey", "abc"]);
        assert_eq!(args.batch_size, find::config::DEFAULT_BATCH_SIZE);
        assert_eq!(args.variants, find::config::DEFAULT_VARIANT_COUNT);
    }

    /// Verifies that [`render_success_report`] formats a match without panicking.
    #[test]
    fn test_render_success_report() {
        let m = find::search::SearchMatch::new(
            "2^10",
            "1024",
            42,
            ["1066".to_string(), "982".to_string()],
        );
        // The function writes to stdout; we just verify it doesn't panic.
        render_success_report(m, std::time::Duration::from_secs(5));
    }
}
