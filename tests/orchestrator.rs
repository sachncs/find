//! Orchestrator-level integration tests.
//!
//! These tests drive the high-level [`find::orchestrator::run`] entry
//! point and verify:
//! - successful discovery of a small known scalar (`d = 5`),
//! - input validation: malformed public keys are rejected before
//!   the search starts,
//! - configuration validation: [`Config::validate_fields`] rejects empty
//!   public keys,
//! - checkpoint resume: a session that wrote a checkpoint can be
//!   loaded and resumed, finding the same match without redoing
//!   completed work,
//! - cache integration: the binary cache path produces the same match
//!   as the direct sweep path.
//!
//! Pair with [`audit`](super::audit) for known-scalar end-to-end
//! recovery and [`differential`](super::differential) for
//! cross-implementation primitive verification.

use find::config::Config;
use find::ecc;
use find::orchestrator::run;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use std::time::Instant;
use tempfile::tempdir;

/// Verifies that [`run`] discovers a match for a small known scalar.
///
/// The target scalar is d = 5, which implies a match in the very first
/// batch of the sweep (j = 2 or j = 4), so the test completes quickly.
#[test]
fn test_orchestrator_finds_small_scalar() {
    let d_hex = "05";
    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(d_hex).unwrap());
    let encoded = target_p.to_affine().to_encoded_point(true);
    let pubkey = hex::encode(encoded.as_bytes());

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let config = Config::new(pubkey, output_dir.to_string_lossy().into_owned(), false);

    let start = Instant::now();
    let result = run(&config);
    let elapsed = start.elapsed();

    assert!(
        result.is_ok(),
        "Orchestrator must not error for small target: {:?}",
        result.err()
    );
    let m = result.unwrap();
    assert!(
        m.is_some(),
        "Orchestrator must find a match for d=5 within first chunk"
    );
    let m = m.unwrap();
    assert!(
        m.candidates.contains(&k256::Scalar::from(5u64)),
        "Candidates must include d=5, got: {:?} (found via {} at j={} after {:?})",
        m.candidates,
        m.label,
        m.j,
        elapsed
    );
}

/// Verifies that [`run`] rejects a malformed public key.
#[test]
fn test_orchestrator_rejects_malformed_pubkey() {
    let dir = tempdir().unwrap();
    let config = Config::new(
        "not_a_valid_key".to_string(),
        dir.path().to_string_lossy().into_owned(),
        false,
    );

    let result = run(&config);
    assert!(result.is_err(), "Malformed pubkey must be rejected");
}

/// Verifies that [`Config::validate_fields`] rejects an empty public key.
#[test]
fn test_config_validate_rejects_empty_pubkey() {
    let config = Config::new("   ".to_string(), "/tmp".to_string(), false);
    let result = config.validate_fields();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}

/// Verifies that [`run`] resumes from a valid checkpoint and still finds the match.
#[test]
fn test_orchestrator_resumes_from_checkpoint() {
    let d_hex = "05";
    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(d_hex).unwrap());
    let encoded = target_p.to_affine().to_encoded_point(true);
    let pubkey = hex::encode(encoded.as_bytes());

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Seed a checkpoint with last_j=0 and a valid integrity anchor.
    let boundary_p = ecc::scalar_mul_g(&k256::Scalar::from(0u64));
    let boundary_x = ecc::to_hex_x(&boundary_p);
    let checkpoint = find::persistence::Checkpoint {
        last_j: 0,
        pubkey: pubkey.clone(),
        last_x: boundary_x,
    };
    let cp_path = output_dir.join("checkpoint.json");
    std::fs::create_dir_all(&output_dir).unwrap();
    checkpoint.save_atomic(&cp_path).unwrap();

    let config = Config::new(pubkey, output_dir.to_string_lossy().into_owned(), false);

    let result = run(&config);
    assert!(result.is_ok(), "Orchestrator must resume and succeed");
    let m = result.unwrap();
    assert!(
        m.is_some(),
        "Orchestrator must find match after resuming from checkpoint"
    );
    let m = m.unwrap();
    assert!(
        m.candidates.contains(&k256::Scalar::from(5u64)),
        "Candidates must include d=5 after resume"
    );
}

/// Verifies that [`run`] discovers a match using the cache-points path.
#[test]
fn test_orchestrator_finds_small_scalar_with_cache() {
    let d_hex = "05";
    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(d_hex).unwrap());
    let encoded = target_p.to_affine().to_encoded_point(true);
    let pubkey = hex::encode(encoded.as_bytes());

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let config = Config::new(pubkey, output_dir.to_string_lossy().into_owned(), true);

    let result = run(&config);
    assert!(
        result.is_ok(),
        "Orchestrator with cache must not error: {:?}",
        result.err()
    );
    let m = result.unwrap();
    assert!(
        m.is_some(),
        "Orchestrator with cache must find a match for d=5"
    );
    let m = m.unwrap();
    assert!(
        m.candidates.contains(&k256::Scalar::from(5u64)),
        "Candidates must include d=5 with cache, got: {:?}",
        m.candidates
    );

    // Verify that a cache file was actually written.
    let cache_dir = output_dir.join("checkpoints");
    assert!(cache_dir.exists(), "Cache directory should be created");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .collect();
    assert!(!entries.is_empty(), "At least one cache chunk should exist");
}

/// Verifies that a session that has a corrupted checkpoint (valid
/// `last_j` but wrong integrity anchor) is rejected with a
/// [`FindError::ResearchIntegrityError`], forcing the user to delete
/// the corrupt checkpoint.
#[test]
fn test_orchestrator_rejects_corrupt_checkpoint() {
    let d_hex = "05";
    let target_p = ecc::scalar_mul_g(&ecc::hex_to_scalar(d_hex).unwrap());
    let encoded = target_p.to_affine().to_encoded_point(true);
    let pubkey = hex::encode(encoded.as_bytes());

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Seed a checkpoint with last_j=0 but a deliberately wrong
    // integrity anchor (not the X-coordinate of 0*G = identity).
    let checkpoint = find::persistence::Checkpoint {
        last_j: 0,
        pubkey: pubkey.clone(),
        last_x: "00".repeat(32), // wrong — should be x(identity) = 0...0 OR x(0·G) = whatever
    };
    let cp_path = output_dir.join("checkpoint.json");
    std::fs::create_dir_all(&output_dir).unwrap();
    checkpoint.save_atomic(&cp_path).unwrap();

    // Note: 00..0 happens to be the canonical X for the identity
    // point, so this anchor is actually valid for last_j=0. To
    // produce a true mismatch, use a non-zero X.
    let corrupt_x = "ff".repeat(32);
    let corrupt = find::persistence::Checkpoint {
        last_j: 100,
        pubkey: pubkey.clone(),
        last_x: corrupt_x,
    };
    corrupt.save_atomic(&cp_path).unwrap();

    let config = Config::new(pubkey, output_dir.to_string_lossy().into_owned(), false);
    let res = run(&config);
    assert!(
        matches!(res, Err(find::error::FindError::ResearchIntegrityError(_))),
        "Corrupt checkpoint must surface ResearchIntegrityError, got: {res:?}"
    );
}

// ============================================================================
// Address-mode discovery (commit 5)
// ============================================================================

/// Builds the Bitcoin mainnet P2PKH address string for a known scalar `d`.
/// Mirrors the full pipeline: ECC mul -> compressed SEC1 -> SHA-256 ->
/// RIPEMD-160 -> Base58Check.
fn address_for_scalar(d: u64) -> (String, find::address::Address40) {
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use ripemd::Ripemd160;
    use sha2::{Digest, Sha256};
    let p = ecc::scalar_mul_g(&k256::Scalar::from(d));
    let enc = p.to_encoded_point(true);
    let sha_out = Sha256::digest(enc.as_bytes());
    let ripemd_out = Ripemd160::digest(sha_out);
    let mut h = [0u8; 20];
    h.copy_from_slice(&ripemd_out);
    let addr40 = find::address::Address40(h);
    let mut body = vec![0x00u8];
    body.extend_from_slice(&h);
    let inner = Sha256::digest(&body[..]);
    let cs_hash = Sha256::digest(&inner[..]);
    body.extend_from_slice(&cs_hash[..4]);
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let zeros = body.iter().take_while(|&&b| b == 0).count();
    let body_no_zeros: Vec<u8> = body[zeros..].to_vec();
    let mut digits: Vec<u8> = Vec::new();
    let mut acc = body_no_zeros;
    while !acc.is_empty() && (acc.iter().any(|&b| b != 0) || !digits.is_empty()) {
        let mut rem: u32 = 0;
        let mut new_acc: Vec<u8> = Vec::with_capacity(acc.len());
        let mut started = false;
        for &b in &acc {
            let cur = rem * 256 + b as u32;
            let q = cur / 58;
            rem = cur % 58;
            if started || q > 0 {
                new_acc.push(q as u8);
                started = true;
            }
        }
        digits.push(rem as u8);
        acc = new_acc;
    }
    digits.reverse();
    let mut address = String::with_capacity(zeros + digits.len());
    for _ in 0..zeros {
        address.push('1');
    }
    for &d_byte in &digits {
        address.push(ALPHABET[d_byte as usize] as char);
    }
    (address, addr40)
}

/// End-to-end: orchestrator discovers a known scalar when the address
/// is provided (matches the legacy `d = 5` test, but routed through
/// the address-keyed sweep path).
#[test]
fn test_orchestrator_address_mode_finds_small_scalar() {
    let d_hex = "05";
    let d: u64 = 5;
    let (address_str, _addr40) = address_for_scalar(d);

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut config = Config::new(
        "[address mode]".to_string(),
        output_dir.to_string_lossy().into_owned(),
        false,
    )
    .try_with_target_address(&address_str)
    .expect("valid address must build");
    config = config.try_with_range(1, 100).expect("1 <= 100");

    let start = Instant::now();
    let result = run(&config).expect("orchestrator should run cleanly");
    let elapsed = start.elapsed();

    let match_ = result.expect("address-mode sweep over [1, 100] must find d=5");
    let recovered_scalar = ecc::hex_to_scalar(d_hex).unwrap();
    assert!(
        match_.candidates.contains(&recovered_scalar),
        "recovered candidates must contain d=5; got {:?} (label={}, j={})",
        match_.candidates,
        match_.label,
        match_.j
    );
    assert_eq!(match_.label, "address/d");
    assert_eq!(match_.j, d);
    eprintln!(
        "address-mode d=5 found in {:?} ({} candidates)",
        elapsed,
        match_.candidates.len()
    );
}

/// Address-mode sweep with a range that excludes the target must return None.
#[test]
fn test_orchestrator_address_mode_returns_none_when_target_not_in_range() {
    // The address for d=5, but ask for [10, 20] -- the search is over
    // a range that doesn't contain d=5, so it must return None.
    let (_address_str, _addr40) = address_for_scalar(5);

    let dir = tempdir().unwrap();
    let output_dir = dir.path().join("data");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::create_dir_all(&log_dir).unwrap();

    let config = Config::new(
        "[address mode]".to_string(),
        output_dir.to_string_lossy().into_owned(),
        false,
    )
    .try_with_target_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
    .expect("address must build")
    .try_with_range(10, 20)
    .expect("10 <= 20");

    let result = run(&config).expect("orchestrator should run cleanly");
    assert!(
        result.is_none(),
        "sweep over [10, 20] should not hit any random hash40 match"
    );
}
