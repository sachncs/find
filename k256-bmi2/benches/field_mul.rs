// Copyright (c) 2026 Sachin
// Released under MIT. See LICENSE-MIT.
//
//! Benchmarks for k256-bmi2's `FieldElement5x52::mul` and
//! `square` against the byte-round-trip path through
//! `k256::FieldElement::mul`. Run with `cargo bench --bench
//! field_mul`.

use criterion::{criterion_group, criterion_main, Criterion};
use k256_bmi2::{limbs_to_be_bytes, be_bytes_to_limbs, FieldElement5x52};

fn mul_bench(c: &mut Criterion) {
    let mut rng_state: u64 = 0xDEADBEEF_CAFEBABE;
    let scalars: Vec<([u64; 5], [u64; 5])> = (0..64)
        .map(|_| {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let a_bytes = limbs_to_be_bytes(&[rng_state, rng_state >> 8, rng_state >> 16, rng_state >> 24, rng_state >> 32]);
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let b_bytes = limbs_to_be_bytes(&[rng_state, rng_state >> 8, rng_state >> 16, rng_state >> 24, rng_state >> 32]);
            (be_bytes_to_limbs(&a_bytes), be_bytes_to_limbs(&b_bytes))
        })
        .collect();

    c.bench_function("k256-bmi2 mul (schoolbook on 5x52 limbs)", |b| {
        b.iter(|| {
            for (a, x) in &scalars {
                let fa = FieldElement5x52(*a);
                let fb = FieldElement5x52(*x);
                std::hint::black_box(fa.mul(&fb));
            }
        });
    });

    c.bench_function("k256-bmi2 square (15-product symmetric form)", |b| {
        b.iter(|| {
            for (a, _) in &scalars {
                let fa = FieldElement5x52(*a);
                std::hint::black_box(fa.square());
            }
        });
    });

    c.bench_function("k256 portable mul via byte round-trip", |b| {
        b.iter(|| {
            for (a, x) in &scalars {
                let a_k = k256::FieldElement::from_bytes(&limbs_to_be_bytes(a).into())
                    .expect("valid bytes");
                let b_k = k256::FieldElement::from_bytes(&limbs_to_be_bytes(x).into())
                    .expect("valid bytes");
                std::hint::black_box(a_k * b_k);
            }
        });
    });
}

criterion_group!(benches, mul_bench);
criterion_main!(benches);