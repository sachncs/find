use find::config::Config;
use find::ecc;
use find::orchestrator;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::Scalar;
use std::time::Instant;

fn main() {
    let d: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok())
        .unwrap_or_else(|| 2_000_000_000 + (std::ptr::null::<u8>() as usize as u64).wrapping_mul(2654435761) % (u32::MAX as u64 - 2_000_000_000));
    let p = ecc::scalar_mul_g(&Scalar::from(d));
    let pubkey_hex = hex::encode(p.to_encoded_point(true));
    let dir = format!("/tmp/find_baseline_{}", d);
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = Config::new(pubkey_hex, &dir, false);
    let start = Instant::now();
    let result = orchestrator::run(&cfg).unwrap();
    let elapsed = start.elapsed();
    match result {
        Some(m) => println!("d={} found=true j={} elapsed={:?}", d, m.small_scalar, elapsed),
        None => println!("d={} NOT FOUND elapsed={:?}", d, elapsed),
    }
}
