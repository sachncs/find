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
//! # Two discovery modes
//!
//! - `--pubkey <hex>` (default): variant-keyed sweep (X-coordinate
//!   match). Backwards compatible; the user's literal `1PWo3JeB...`
//!   example was an address-format string and decodes to a hash40,
//!   not a SEC1 pubkey, so this mode will reject it.
//! - `--address <base58> --from <scalar> --to <scalar>`: address-keyed
//!   sweep (hash40 match over a scalar range).
//!
//! The two modes are mutually exclusive (clap-level). The address path
//! is the only one that doesn't require a SEC1 pubkey at startup.
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
use find::telemetry::{init_tracing, install_worker_panic_handler};
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
    ///
    /// Mutually exclusive with `--address`. Required unless `--address`
    /// is set (the engine rejects `pubkey`-empty + no target-address at
    /// validate_fields time when in pubkey mode).
    #[arg(short, long, conflicts_with = "address")]
    pubkey: Option<String>,

    /// Base58 Bitcoin mainnet address (P2PKH `0x00` or P2SH `0x05`).
    ///
    /// Switches the engine to address-discovery mode: a hash40-targeted
    /// sweep over the user-specified `[--from, --to]` range. Mutually
    /// exclusive with `--pubkey`.
    #[arg(short = 'a', long = "address", conflicts_with = "pubkey")]
    address: Option<String>,

    /// Inclusive scalar lower bound (decimal or hex with `0x` prefix).
    ///
    /// Default (when `--address` is set, no `--from`): use
    /// `MIN_SEARCH_SCALAR` (= 1).
    #[arg(long, value_name = "HEX_OR_DEC")]
    from: Option<String>,

    /// Inclusive scalar upper bound (decimal or hex with `0x` prefix).
    ///
    /// Default (when `--address` is set, no `--to`): use `u128::MAX`.
    #[arg(long, value_name = "HEX_OR_DEC")]
    to: Option<String>,

    /// Data and checkpoint root directory.
    #[arg(short, long, default_value = "data")]
    output_dir: String,

    /// Rolling log directory.
    #[arg(short, long, default_value = "logs")]
    log_dir: String,

    /// Persist jG points to binary caches for multi-pubkey reuse.
    ///
    /// WARNING: Consumes approximately 32GB per billion points.
    /// Auto-disabled when `--address` is set (cache stores X-coords;
    /// the address sweep does not produce them).
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
    /// Ignored in address mode (no variant table is used).
    #[arg(long, default_value_t = find::config::DEFAULT_VARIANT_COUNT)]
    variants: u32,
}

/// Parse a CLI hex-or-dec string into a u128. Accepts `0x...`, `0X...`,
/// or a plain decimal integer. Empty / unparseable returns an error
/// string that becomes an anyhow message.
///
/// **Type: `u128`** — the user's `0x400000000000000000:...` style inputs
/// exceed `u64::MAX` and must be representable. Strings that overflow
/// `u128::MAX` are rejected here with a parse error before they reach
/// the sweep.
fn parse_hex_or_dec(s: &str) -> anyhow::Result<u128> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u128::from_str_radix(hex, 16).map_err(|e| anyhow::anyhow!("hex {trimmed:?}: {e}"))
    } else {
        trimmed
            .parse::<u128>()
            .map_err(|e| anyhow::anyhow!("decimal {trimmed:?}: {e}"))
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    install_worker_panic_handler();
    let _guard = init_tracing(&args.log_dir)?;

    let main_span = info_span!("main_execution");
    let _enter = main_span.enter();

    info!("Initializing find tool v{}", env!("CARGO_PKG_VERSION"));

    // Build the config. The two entry points share the same builders:
    //
    //   - pubkey mode (default): Config::new(pubkey, ...). The pubkey
    //     field is non-empty and required.
    //   - address mode: we construct Config with a placeholder pubkey
    //     (the orchestrator ignores it) and feed --address through
    //     try_with_target_address.
    //
    // Both branches feed the same set of try_with_* builders afterward
    // and the validate_* checks at the orchestrator entry point decide
    // whether the pubkey string is required.
    let mut config_builder = if let Some(addr_str) = args.address {
        Config::new(
            "[address mode]".to_string(),
            args.output_dir,
            args.cache_points,
        )
        .try_with_target_address(&addr_str)
        .map_err(|e| anyhow::anyhow!("--address: {e}"))?
    } else {
        let pk = args.pubkey.clone().ok_or_else(|| {
            anyhow::anyhow!("--pubkey is required in pubkey mode (or pass --address)")
        })?;
        Config::new(pk, args.output_dir, args.cache_points)
    };

    config_builder = config_builder
        .try_with_batch_size(args.batch_size)
        .map_err(|e| anyhow::anyhow!("--batch-size: {e}"))?
        .try_with_variant_count(args.variants)
        .map_err(|e| anyhow::anyhow!("--variants: {e}"))?;

    if args.from.is_some() || args.to.is_some() {
        let from_str = args
            .from
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--from provided without --to"))?;
        let to_str = args
            .to
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--to provided without --from"))?;
        let from = parse_hex_or_dec(from_str)?;
        let to = parse_hex_or_dec(to_str)?;
        config_builder = config_builder
            .try_with_range(from, to)
            .map_err(|e| anyhow::anyhow!("--from/--to: {e}"))?;
    }

    let config = config_builder;

    let start = Instant::now();
    match orchestrator::run(&config)? {
        Some(match_) => render_success_report(match_, start.elapsed()),
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
fn render_success_report(match_: find::search::SearchMatch, total_time: std::time::Duration) {
    // Build the full banner in a single `String` (one allocation) and write
    // it once. The previous per-line `println!` did nine separate
    // `format!`-style allocations plus nine `STDOUT_LOCK` acquisitions.
    let separator = "=".repeat(60);
    let mut out = String::with_capacity(512);
    out.push('\n');
    out.push_str(&separator);
    out.push('\n');
    use std::fmt::Write;
    let _ = writeln!(out, "MATCH DISCOVERED (Variant: {})", match_.label);
    let _ = writeln!(out, "Shift scalar V: {}", match_.offset);
    let _ = writeln!(out, "Search scalar j: {}", match_.j);
    out.push_str("Target candidates (d = V +/- j):\n");
    let candidates_hex = match_.candidates_hex();
    for (i, c) in candidates_hex.iter().enumerate() {
        let _ = writeln!(out, "  [{}] 0x{}", i + 1, c);
    }
    let _ = writeln!(out, "Total Search Duration: {total_time:?}");
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
        assert_eq!(args.pubkey.as_deref(), Some("abc"));
        assert_eq!(args.output_dir, "data");
        assert_eq!(args.log_dir, "logs");
        assert!(!args.cache_points);
        assert!(args.address.is_none());
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
        assert_eq!(args.pubkey.as_deref(), Some("deadbeef"));
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
        use k256::Scalar;
        let match_ = find::search::SearchMatch::new(
            "2^10",
            "1024",
            42,
            [Scalar::from(1066u64), Scalar::from(982u64)],
        );
        // The function writes to stdout; we just verify it doesn't panic.
        render_success_report(match_, std::time::Duration::from_secs(5));
    }
}
