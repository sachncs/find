// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! # High-Precision Cryptographic Benchmarking Suite
//!
//! This suite provides scientific throughput validation for the `find` tool's
//! core cryptographic and algorithmic engines. It utilizes the `criterion`
//! framework to deliver statistically significant performance metrics.
//!
//! ## 🔬 Benchmarking Methodology
//! We measure two critical system bottlenecks:
//! 1.  **ECC Throughput:** Compares sequential coordinate normalization vs.
//!     v2.x **Batch Normalization** (Montgomery's Simultaneous Inversion).
//! 2.  **Lookup Latency:** Measures the efficiency of the $O(\log N)$ flat-array
//!     `VariantIndex` matching logic.
//!
//! ## ⚡ Expected Outcomes
//! - **Batch Amortization:** Should demonstrate a >600x speedup in point
//!   normalization by amortizing modular inversion across 32-scalar chunks.
//! - **Index Efficiency:** Should maintain sub-20ns matching latency due to
//!   L1/L2 cache locality of the flat sorted array.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use find::ecc;
use find::search::{self, VariantIndex};
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::Scalar;

/// Benchmarks the core ECC throughput bottlenecks.
///
/// Specifically evaluates the impact of Batch Normalization on the
/// coordinate extraction phase (Projective -> Affine transition).
fn bench_batch_normalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ecc_throughput");

    // Setup: Generate a representative batch of 32 points.
    let scalars: Vec<Scalar> = (1..33).map(|i| Scalar::from(i as u64)).collect();
    let points: Vec<k256::ProjectivePoint> = scalars.iter().map(ecc::scalar_mul_g).collect();

    // v1.x baseline: Sequential coordinate conversion.
    group.bench_function("single_normalization", |b| {
        b.iter(|| {
            for p in &points {
                black_box(p.to_affine());
            }
        })
    });

    // v2.x optimization: Montgomery Simultaneous Inversion.
    group.bench_function("batch_normalization_32", |b| {
        let mut affines = vec![k256::AffinePoint::IDENTITY; points.len()];
        b.iter(|| {
            k256::ProjectivePoint::batch_normalize(&points, &mut affines);
            black_box(());
        })
    });
    group.finish();
}

/// Benchmarks the algorithmic matching efficiency of the VariantIndex.
///
/// Evaluates the cache-locality performance of the $O(\log N)$ binary search
/// over a flat sorted array.
fn bench_index_lookup(c: &mut Criterion) {
    // Setup: Generate 512 variants and index them.
    let p = ecc::scalar_mul_g(&Scalar::from(123456u64));
    let variants = search::generate_variants(&p);
    let index = VariantIndex::new(variants);

    // Mock a target X-coordinate for a known scalar.
    let target_affine = p.to_affine();
    let encoded = target_affine.to_encoded_point(false);
    let x_bytes = encoded.x().unwrap();
    let mut test_x = [0u8; 32];
    test_x.copy_from_slice(x_bytes.as_ref());

    c.bench_function("flat_index_match", |b| {
        b.iter(|| {
            // Perform high-frequency lookup.
            black_box(index.match_x(&test_x, 100));
        })
    });
}

// Macro-expansion of the benchmarking harness.
criterion_group!(benches, bench_batch_normalization, bench_index_lookup);
criterion_main!(benches);
