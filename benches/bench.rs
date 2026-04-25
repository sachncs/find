// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-precision cryptographic benchmarking suite.
//!
//! Measures the two primary system bottlenecks:
//! 1. Coordinate normalization (sequential vs batch).
//! 2. Variant index lookup latency.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use find::ecc;
use find::search::{self, VariantIndex};
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::Scalar;

/// Benchmarks sequential affine normalization against batch normalization.
///
/// A batch of 32 points is normalized sequentially as a baseline, then
/// normalized in a single call using Montgomery's simultaneous inversion.
/// The expected speedup is in the 15–20x range.
fn bench_batch_normalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ecc_throughput");

    let scalars: Vec<Scalar> = (1..33).map(|i| Scalar::from(i as u64)).collect();
    let points: Vec<k256::ProjectivePoint> = scalars.iter().map(ecc::scalar_mul_g).collect();

    group.bench_function("single_normalization", |b| {
        b.iter(|| {
            for p in &points {
                black_box(p.to_affine());
            }
        })
    });

    group.bench_function("batch_normalization_32", |b| {
        let mut affines = vec![k256::AffinePoint::IDENTITY; points.len()];
        b.iter(|| {
            k256::ProjectivePoint::batch_normalize(&points, &mut affines);
            black_box(());
        })
    });
    group.finish();
}

/// Benchmarks the [`VariantIndex::match_x`] lookup latency.
///
/// A flat sorted array of 512 variants is searched via binary search.
/// The expected latency is sub-20 ns due to L1/L2 cache locality.
fn bench_index_lookup(c: &mut Criterion) {
    let p = ecc::scalar_mul_g(&Scalar::from(123456u64));
    let variants = search::generate_variants(&p);
    let index = VariantIndex::new(variants);

    let target_affine = p.to_affine();
    let encoded = target_affine.to_encoded_point(false);
    let x_bytes = encoded.x().unwrap();
    let mut test_x = [0u8; 32];
    test_x.copy_from_slice(x_bytes.as_ref());

    c.bench_function("flat_index_match", |b| {
        b.iter(|| {
            black_box(index.match_x(&test_x, 100));
        })
    });
}

criterion_group!(benches, bench_batch_normalization, bench_index_lookup);
criterion_main!(benches);
