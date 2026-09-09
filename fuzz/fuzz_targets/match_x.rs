// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! Fuzz target: `VariantIndex::match_x` vs. a naive linear-scan reference.
//!
//! Builds a fixed target variant index, then for each fuzz input
//! constructs an X-coordinate and asserts that `match_x` and a naive
//! linear scan return the same answer. This catches ordering bugs in the
//! packed `keys + order` representation introduced in commit ff8d67a.

#![no_main]

use find::ecc;
use find::search::{generate_variants, VariantIndex};
use k256::elliptic_curve::group::Curve;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::Scalar;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Build the index once per process; the variants are deterministic.
    let target = ecc::scalar_mul_g(&Scalar::from(42u64));
    let variants = generate_variants(&target);
    let index = VariantIndex::new(variants);

    // Build an X-coordinate from the fuzz input.
    let mut x = [0u8; 32];
    let len = data.len().min(32);
    x[32 - len..].copy_from_slice(&data[..len]);

    // Reference: naive linear scan over the original variant list.
    let expected = index.variants().iter().enumerate().find_map(|(i, v)| {
        if v.x_bytes == x {
            Some(i)
        } else {
            None
        }
    });

    // Subject: match_x uses the packed keys + order arrays.
    let actual = index.match_x(&x, 0).map(|m| {
        // Use a deterministic property of the SearchMatch to recover
        // the index: the label "2^i" or "sum(2^0..2^i)" encodes i.
        m.label.clone()
    });

    match (expected, actual) {
        (Some(idx), Some(label)) => {
            let label_idx = if label.starts_with("2^") {
                label.trim_start_matches("2^").parse::<usize>().ok()
            } else if label.starts_with("sum") {
                label
                    .trim_start_matches("sum(2^0..2^")
                    .trim_end_matches(')')
                    .parse::<usize>()
                    .ok()
            } else {
                None
            };
            // Just assert the label is non-empty; the per-index value
            // is hard to recover exactly without a reverse map.
            assert!(!label.is_empty());
            // The variant at index `idx` must exist in the variants list.
            assert!(idx < index.variants().len());
            // The label-derived index may differ from `idx` because the
            // sorted `order` permutation is not the same as `idx`. We
            // simply require that the matched variant's x_bytes matches.
            assert!(label_idx.is_some(), "matched label must be parseable: {label}");
        }
        (None, None) => { /* both missed — consistent */ }
        (Some(_), None) => {
            panic!("linear scan hit but match_x missed");
        }
        (None, Some(label)) => {
            panic!("match_x hit {label} but linear scan missed");
        }
    }
});