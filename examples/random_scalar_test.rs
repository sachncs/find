// Copyright (c) 2026 Sachin
// Released under MIT. See LICENSE-MIT.
//
//! End-to-end recovery test for known scalars at three magnitudes:
//! \(2^{32}\), \(2^{48}\), and \(2^{64}\).
//!
//! For each scalar \(d\):
//! 1. Compute \(P = d \cdot G\) via the k256 fixed-base multiplication.
//! 2. Build the 512-variant index from \(P\).
//! 3. Construct a target range \([j-8, j+8]\) with \(j = d - 1\),
//!    guaranteeing the match lands inside the sweep window.
//! 4. Run `search::sweep_parallel` and assert the recovered candidate
//!    equals \(d\) (mod the curve order \(n\)).
//!
//! Run with `cargo run --release --example random_scalar_test`.
//! No CLI arguments; the test set is fixed.

use find::ecc;
use find::search::{self, VariantIndex};
use k256::Scalar;
use std::time::Instant;

/// Three target magnitudes to verify search correctness at large
/// scalars. All three are well below the curve order \(n \approx 2^{256}\)
/// so the reduced scalar is identity-friendly.
const TARGET_MAG_BITS: &[u32] = &[32, 48, 64];

/// Half-width of the sweep window around `j = d - 1`. Sufficient to
/// cover the deterministic match path; the `sweep_parallel` early-exit
/// always returns on the first hit at `j = d - 1`.
const WINDOW_HALF: u64 = 8;

fn main() {
    let start_all = Instant::now();
    let rayon_threads = rayon::current_num_threads();
    eprintln!(
        "random_scalar_test: {} target magnitudes, {} rayon threads",
        TARGET_MAG_BITS.len(),
        rayon_threads
    );

    let mut failures: u32 = 0;
    let mut total_scalars = 0u128;
    let mut total_elapsed_ns: u128 = 0;

    for &bits in TARGET_MAG_BITS {
        // d = 2^bits, well below curve order for any u64 bit count.
        // `1u64 << 64` is undefined; for bits == 64 we use `u64::MAX`, the
        // largest u64 (= 2^64 - 1) instead. Functionally identical for the
        // sweep since `Scalar::from(u64::MAX)` rounds identically.
        let d: u64 = if bits < 64 { 1u64 << bits } else { u64::MAX };
        // j = d - 1 -> matches the V = 1 (2^0) variant.
        let j: u64 = d - 1;
        // Sweep window centred on j with a few scalars of slack either side.
        let sweep_start = j.saturating_sub(WINDOW_HALF);
        let sweep_end = j.saturating_add(WINDOW_HALF);
        let scalars_in_window = (sweep_end - sweep_start + 1) as u128;

        let target_label = if bits < 64 {
            format!("2^{bits}")
        } else {
            format!("2^64 - 1")
        };
        eprintln!(
            "\n=== target d = {target_label} = {d} (decimal), sweep = [{sweep_start}..={sweep_end}], \
             {scalars_in_window} scalars ==="
        );

        let target_scalar = Scalar::from(d);
        let target_p = ecc::scalar_mul_g(&target_scalar);

        // Build the variant index (this takes the same ~815 ps as `generate_variants_512`).
        let variants = search::generate_variants(&target_p);
        let x_bytes = search::compute_variant_x_bytes(&target_p);
        let index = VariantIndex::new(variants, &x_bytes);

        let sweep_start_clock = Instant::now();
        let result = search::sweep_parallel(&index, sweep_start, sweep_end, 32);
        let elapsed = sweep_start_clock.elapsed();

        match result {
            Some(m) => {
                let elapsed_ns = elapsed.as_nanos();
                total_scalars = total_scalars.saturating_add(scalars_in_window);
                total_elapsed_ns = total_elapsed_ns.saturating_add(elapsed_ns);

                // The recovered scalar is one of `V ± j`. For d = 2^bits and
                // V = 2^0 = 1, j = d - 1, the two candidates are 1 + (d-1) = d
                // and 1 - (d-1) = 2 - d (negative mod n, which is not d).
                let hit_d = m.candidates.contains(&target_scalar);
                let status = if hit_d { "PASS" } else { "FAIL" };

                eprintln!(
                    "  {}: match via {} at j={}, candidates={:?}",
                    status,
                    m.label,
                    m.j,
                    m.candidates
                );
                eprintln!(
                    "  sweep: {elapsed_ns} ns for {scalars_in_window} scalars \
                     ({:.2} Ms/s single-thread)",
                    (scalars_in_window as f64) / (elapsed_ns as f64 / 1e9) / 1e6
                );

                if !hit_d {
                    failures += 1;
                }
            }
            None => {
                failures += 1;
                eprintln!(
                    "  FAIL: sweep_parallel returned None; window [{sweep_start}..={sweep_end}] \
                     should contain j = {j} (d - 1)"
                );
            }
        }
    }

    eprintln!(
        "\nSummary: {}/{} targets recovered, total scalars swept = {}, \
         total sweep time = {:.3} ms",
        TARGET_MAG_BITS.len() as u32 - failures,
        TARGET_MAG_BITS.len(),
        total_scalars,
        total_elapsed_ns as f64 / 1e6
    );

    let wall = start_all.elapsed();
    eprintln!("Wall time (incl. build, exclude this line): {:?}", wall);

    if failures > 0 {
        std::process::exit(1);
    }
}
