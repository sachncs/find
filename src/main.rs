// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Thin CLI wrapper and tracing bootstrap.
//!
//! All domain logic lives in [`find::orchestrator`]; this file only parses
//! arguments, initializes observability, and renders results.

use clap::Parser;
use find::orchestrator::{self, Config};
use std::time::Instant;
use tracing::{info, info_span};
use tracing_subscriber::prelude::*;

/// Command-line interface for the secp256k1 find tool.
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
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _ = rayon::ThreadPoolBuilder::new()
        .panic_handler(|info| {
            let msg = info
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("unknown panic");
            tracing::error!(message = %msg, "Rayon worker panicked");
        })
        .build_global();

    let _guard = init_tracing(&args.log_dir)?;

    let main_span = info_span!("main_execution");
    let _enter = main_span.enter();

    info!("Initializing find tool v{}", env!("CARGO_PKG_VERSION"));

    let config = Config {
        pubkey: args.pubkey,
        output_dir: args.output_dir,
        cache_points: args.cache_points,
    };

    let start = Instant::now();
    match orchestrator::run(&config)? {
        Some(m) => render_success_report(m, start.elapsed()),
        None => println!("Search completed. No match found."),
    }

    Ok(())
}

/// Initializes tracing with a daily-rolling file appender and a stderr layer.
///
/// The returned guard must remain alive for the duration of the program to
/// ensure that buffered log events are flushed before exit.
fn init_tracing(
    log_dir: &str,
) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = tracing_appender::rolling::daily(log_dir, "find.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    Ok(guard)
}

/// Prints a formatted success report to stdout.
fn render_success_report(m: find::search::SearchMatch, total_time: std::time::Duration) {
    println!("\n{}", "=".repeat(60));
    println!("MATCH DISCOVERED (Variant: {})", m.label);
    println!("Shift scalar V: {}", m.offset);
    println!("Search scalar j: {}", m.small_scalar);
    println!("Target candidates (d = V +/- j):");
    for (i, c) in m.candidates.iter().enumerate() {
        println!("  [{}] 0x{}", i + 1, c);
    }
    println!("Total Search Duration: {:?}", total_time);
    println!("{}", "=".repeat(60));
}
