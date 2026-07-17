// Copyright (c) 2026 Sachin
// Released under MIT. See LICENSE-MIT.
//
//! Throughput measurement for the find hot loop. Two modes:
//!
//! `./sweep_bench --sweep` — runs `search::sweep_parallel` repeatedly
//! over a range that contains the match (defeating DCE), measuring
//! pure sweep throughput across all cores.
//!
//! `./sweep_bench --orchestrator` — runs the full `orchestrator::run`
//! pipeline including variant generation, checkpoint save/load,
//! and the orchestrator loop. This is the metric the user
//! reports as "27-30 M/sec" on M3 Pro.
//!
//! Run with `cargo run --release --example sweep_bench -- --sweep`
//! or `cargo run --release --example sweep_bench -- --orchestrator`.

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map_or("--sweep", String::as_str);

    let rayon_threads = rayon::current_num_threads();
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    eprintln!(
        "Mode: {} ({} rayon threads, {} cores)",
        mode, rayon_threads, cores
    );

    match mode {
        "--sweep" => bench_sweep(),
        "--orchestrator" => bench_orchestrator(),
        other => {
            eprintln!("Unknown mode: {}", other);
            std::process::exit(2);
        }
    }
}

fn bench_sweep() {
    use find::ecc;
    use find::search::{self, VariantIndex};
    use k256::Scalar;

    let known_d: u64 = 5;
    let sweep_start: u64 = 1;
    let sweep_count: u64 = 50_000_000;

    eprintln!("Building variant index for d={}...", known_d);
    let target = ecc::scalar_mul_g(&Scalar::from(known_d));
    let variants = search::generate_variants(&target);
    let x_bytes = search::compute_variant_x_bytes(&target);
    let index = VariantIndex::new(variants, &x_bytes);

    // Warm-up. This will find the match at j ∈ {1, 3, 4}.
    let warm = search::sweep_parallel(&index, sweep_start, sweep_count, 32);
    eprintln!("warm-up match: {:?}", warm.as_ref().map(|m| (m.label, m.j)));
    std::hint::black_box(&index);

    const ITERS: u64 = 5;
    let start = Instant::now();
    let mut sink: u64 = 0;
    for _ in 0..ITERS {
        let m = search::sweep_parallel(&index, sweep_start, sweep_count, 32);
        std::hint::black_box(&index);
        if let Some(ref x) = m {
            sink = sink.wrapping_add(x.j.try_into().unwrap_or(u64::MAX));
        }
    }
    let elapsed = start.elapsed();
    eprintln!("sink (last match j sum): {}", sink);

    let total_scalars = (sweep_count as u128) * (ITERS as u128);
    let elapsed_ns = elapsed.as_nanos() as u64;
    let scalars_per_sec = total_scalars as f64 / (elapsed_ns as f64 / 1e9);
    eprintln!(
        "[sweep] Swept {} scalars in {}ns: {:.3} M/sec aggregate ({:.2} ns/scalar)",
        total_scalars,
        elapsed_ns,
        scalars_per_sec / 1e6,
        elapsed_ns as f64 / total_scalars as f64,
    );
}

fn bench_orchestrator() {
    use find::config::Config;
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let d_hex = "05";
    let target_p = find::ecc::scalar_mul_g(&find::ecc::hex_to_scalar(d_hex).unwrap());
    let encoded = target_p.to_affine().to_encoded_point(true);
    let pubkey = hex::encode(encoded.as_bytes());

    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Second arg is cache_points (true means persisting binary caches
    // for I/O-bound re-runs — the expensive path).
    let cache_points = std::env::args().any(|a| a == "--cache");
    let config = Config::new(
        pubkey,
        output_dir.to_string_lossy().into_owned(),
        cache_points,
    );
    eprintln!("orchestrator with cache_points={}", cache_points);

    const ITERS: u64 = 3;
    let start = Instant::now();
    let mut sink: u64 = 0;
    for _ in 0..ITERS {
        let r = find::orchestrator::run(&config);
        std::hint::black_box(&config);
        if let Ok(Some(ref m)) = r {
            sink = sink.wrapping_add(u64::try_from(m.j).unwrap_or(u64::MAX));
        }
    }
    let elapsed = start.elapsed();
    eprintln!("sink (orchestrator match j sum): {}", sink);

    eprintln!(
        "[orchestrator] {} iterations in {}ns ({:.2} ms/iter)",
        ITERS,
        elapsed.as_nanos(),
        elapsed.as_nanos() as f64 / ITERS as f64 / 1e6,
    );
}
