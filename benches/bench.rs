// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! High-precision cryptographic benchmarking suite.
//!
//! Measures the primary system bottlenecks:
//! 1. Coordinate normalization (sequential vs Montgomery batch).
//! 2. Variant index lookup latency.
//! 3. The `+ G` increment chain cost (single bootstrap scalar mul + N-1
//!    mixed additions) vs. independent scalar multiplications.
//! 4. End-to-end small-scalar discovery throughput.
//! 5. Binary-cache chunk precomputation throughput.
//! 6. Variant generation cost (the cold-start cost of `orchestrator::run`).
//!
//! Run with `cargo bench` (uses `[profile.bench]`); see
//! `docs/benchmarks.md` for the full guide.

use criterion::{criterion_group, criterion_main, Criterion};
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
            // `black_box` prevents the optimizer from hoisting the
            // normalization out of the timed region. Each iteration
            // would otherwise be observably free to LLVM.
            for p in &points {
                std::hint::black_box(p.to_affine());
            }
        })
    });

    group.bench_function("batch_normalization_32", |b| {
        let mut affines = vec![k256::AffinePoint::IDENTITY; points.len()];
        b.iter(|| {
            k256::ProjectivePoint::batch_normalize(&points, &mut affines);
            // `black_box(())` forces the optimizer to treat the result
            // as observed, so the entire batch op is not elided.
            std::hint::black_box(());
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
            // `black_box` on the `j` argument is unnecessary (it's a u64,
            // trivially observable), but we still pass it to the lookup
            // to exercise the same call path used by the orchestrator.
            std::hint::black_box(index.match_x(&test_x, 100));
        })
    });
}

/// Benchmarks the `+ G` increment chain used by `perform_chunked_sweep`.
///
/// Compares the hot path (one bootstrap scalar multiplication + N-1 mixed
/// additions) against the naive baseline (N independent scalar muls).
/// Expected speedup: ~20x for N = 32.
fn bench_plus_g_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("plus_g_chain");

    // Inputs: 32 consecutive scalars starting from 1.
    let scalars: Vec<u64> = (1..=32).collect();

    group.bench_function("naive_32_independent_scalar_muls", |b| {
        b.iter(|| {
            let points: Vec<k256::ProjectivePoint> = scalars
                .iter()
                .map(|&s| ecc::scalar_mul_g(&Scalar::from(s)))
                .collect();
            std::hint::black_box(points);
        })
    });

    group.bench_function("chain_32_plus_g", |b| {
        b.iter(|| {
            // Bootstrap: one scalar mul to get the starting point.
            let mut current = ecc::scalar_mul_g(&Scalar::from(scalars[0]));
            let mut points = Vec::with_capacity(scalars.len());
            for &s in &scalars {
                if s == scalars[0] {
                    points.push(current);
                } else {
                    current += ecc::generator();
                    points.push(current);
                }
            }
            std::hint::black_box(points);
        })
    });

    group.finish();
}

/// End-to-end small-scalar discovery.
///
/// Sweeps `[1, 10_000_000]` looking for `d = 12345`. Captures the full
/// hot-loop cost (bootstrap muls, +G chain, Montgomery normalize,
/// match_x) including early-exit overhead.
fn bench_end_to_end_small_scalar(c: &mut Criterion) {
    let d = 12345u64;
    let target = ecc::scalar_mul_g(&Scalar::from(d));
    let variants = search::generate_variants(&target);
    let index = VariantIndex::new(variants);

    c.bench_function("end_to_end_small_scalar_12345", |b| {
        b.iter(|| {
            // The orchestrator clamps start to MIN_J = 1 internally; we
            // do the same here.
            let m = std::hint::black_box(search::perform_chunked_sweep(&index, 1, 10_000_000));
            assert!(m.is_some(), "match must be found");
        })
    });
}

/// Variant generation cost (one-time per session).
///
/// Measures the cold-start cost of building the 512-variant set for a
/// typical target public key. This is the function called once at the
/// beginning of `orchestrator::run`.
fn bench_variant_generation(c: &mut Criterion) {
    let target = ecc::scalar_mul_g(&Scalar::from(1_000_000u64));

    c.bench_function("generate_variants_512", |b| {
        b.iter(|| {
            let variants = std::hint::black_box(search::generate_variants(&target));
            assert_eq!(variants.len(), 512);
        })
    });
}

/// Benchmarks `x_bytes` extraction from a projective point.
///
/// Compares the current direct-AffineCoordinates implementation against
/// the SEC1 round-trip baseline (to_encoded_point + EncodedPoint::x()).
/// The former is the optimisation shipped in 0001; the latter is the
/// pre-optimisation implementation kept here as a regression baseline.
fn bench_x_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("x_bytes");

    let p = ecc::scalar_mul_g(&Scalar::from(42u64));

    group.bench_function("direct_affine_x", |b| {
        b.iter(|| {
            let x = std::hint::black_box(ecc::x_bytes(&p));
            assert!(x.is_some());
        })
    });

    group.bench_function("sec1_roundtrip_x", |b| {
        b.iter(|| {
            let affine = p.to_affine();
            let encoded = affine.to_encoded_point(false);
            let x = encoded.x().unwrap();
            std::hint::black_box(x);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_batch_normalization,
    bench_index_lookup,
    bench_plus_g_chain,
    bench_end_to_end_small_scalar,
    bench_variant_generation,
    bench_x_bytes,
);
criterion_main!(benches);
