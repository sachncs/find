# Frequently Asked Questions

## General

### What is the Secp256k1 Find Tool?

The Secp256k1 Find Tool is a high-performance Rust system for educational and research exploration of elliptic curve mathematics. It demonstrates multi-variant range-splitting algorithms for secp256k1 private key discovery.

### Is this tool for recovering lost private keys?

**No.** This tool is for educational and research purposes only. It demonstrates cryptographic algorithms and high-performance systems engineering. See [DISCLAIMER.md](../DISCLAIMER.md) for full legal terms.

### What is secp256k1?

Secp256k1 is an elliptic curve used in Bitcoin and other cryptocurrencies. It provides 256-bit security and is optimized for efficient computation.

## Installation

### What Rust version do I need?

Rust 1.70 or later. Install via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Can I run this on Windows?

Yes. The tool supports Linux, macOS, and Windows. Some features like binary caching may have different performance characteristics on Windows.

### How do I build from source?

```bash
git clone https://github.com/sachn-cs/find.git
cd find
cargo build --release
```

## Usage

### How do I run a search?

```bash
find --pubkey <HEX_SEC1_PUBLIC_KEY>
```

Example:

```bash
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

### What does the output mean?

When a match is found, you'll see:

- **Variant**: The shift variant that produced the match
- **Shift scalar V**: The offset applied to the target point
- **Search scalar j**: The scalar that matched the X-coordinate
- **Target candidates**: Two possible private key values

### How long does a search take?

Search time depends on:
- CPU performance
- Size of search range
- Whether binary caching is used
- Target scalar value

For small scalars (< 1 billion), results are typically found in seconds to minutes.

### What is binary caching?

Binary caching precomputes X-coordinates and stores them in a binary file. This allows subsequent searches to skip ECC arithmetic and perform direct file scans, which can be 10-100x faster.

### How much disk space does caching require?

Approximately 32GB per billion points cached. For example:
- 1 billion points ≈ 32GB
- 10 billion points ≈ 320GB

## Technical

### How does the algorithm work?

The tool uses multi-variant range-splitting:
1. Generate 512 shift variants (256 powers-of-2, 256 cumulative sums)
2. For each scalar `j`, check if `x(j·G) = x(P - V·G)`
3. When matched, derive candidates `d = V ± j (mod n)`

See [ALGORITHMS.md](../ALGORITHMS.md) for mathematical details.

### What is batch normalization?

Batch normalization uses Montgomery's simultaneous inversion trick to amortize modular inversion costs across multiple points. For a batch of 32 points, this provides approximately 630x speedup.

### What is the VariantIndex?

The VariantIndex is a flat sorted array of 512 entries, sorted by X-coordinate. It provides O(log N) binary search for X-coordinate matching, with excellent L1/L2 cache locality.

### How does checkpointing work?

After each 1-billion-point segment, the tool writes state to `data/checkpoint.json` using write-then-rename for atomic persistence. On resume, it verifies cryptographic integrity before continuing.

## Performance

### How can I improve performance?

1. **Use release builds**: `cargo build --release`
2. **Enable binary caching**: `--cache-points` flag
3. **Use fast storage**: NVMe SSDs for cache files
4. **Allocate more CPU cores**: The tool uses all available cores
5. **Optimize batch size**: Adjust `BATCH_SIZE` constant

### What are the performance characteristics?

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Variant generation | O(512) | One-time per target |
| Index lookup | O(log 512) | Binary search |
| Sweep (CPU) | O(R) | Linear over range |
| Sweep (I/O) | O(R) | Sequential binary read |

### How does parallelism work?

The tool uses `rayon`'s work-stealing parallelism:
- Range is divided into batches of 32 scalars
- Each worker processes one batch independently
- `find_map_any()` provides early exit on first match
- No locks required (index is read-only)

## Troubleshooting

### Build fails with "error[E0599]"

Ensure you have Rust 1.70+ installed:

```bash
rustc --version
rustup update
```

### Checkpoint corruption error

If you see `ResearchIntegrityError`:
1. The checkpoint file may be corrupted
2. Delete `data/checkpoint.json`
3. Restart the search

### Out of memory during caching

Binary caching requires significant memory. Options:
1. Reduce `CACHE_CHUNK_SIZE` in source code
2. Avoid caching for very large searches
3. Use a machine with more RAM

### Search takes too long

1. Verify you're using a release build
2. Check that all CPU cores are being utilized
3. Consider using binary caching for repeated searches
4. The target scalar may be very large

### No match found

This can mean:
1. The public key is invalid
2. The scalar is outside the search range
3. The algorithm needs more time
4. Check input format (must be valid SEC1 hex)

## Development

### How do I run tests?

```bash
# Full test suite
make test

# Specific test
cargo test test_name

# With increased property-test cases
PROPTEST_CASES=1000 cargo test --release
```

### How do I run benchmarks?

```bash
make bench
```

### How do I check code quality?

```bash
make lint
```

### How do I generate coverage reports?

```bash
make coverage
```

## Contributing

### How do I contribute?

See [CONTRIBUTING.md](../CONTRIBUTING.md) for detailed guidelines.

### What contributions are accepted?

- Bug fixes
- Performance improvements
- Documentation enhancements
- Test coverage improvements
- Research-aligned features

### What contributions are NOT accepted?

- Features designed for non-educational use
- Changes that compromise mathematical correctness
- Optimizations that sacrifice code clarity

## Security

### How do I report security vulnerabilities?

**Do not open public issues.** See [SECURITY.md](../SECURITY.md) for reporting instructions.

### Is this tool secure?

The tool is designed with security in mind:
- No unsafe Rust code
- Input validation on all operations
- Checkpoint integrity verification
- Atomic file operations

## License

### What license is this project under?

Dual-licensed under MIT and Apache 2.0. You may use this software under the terms of either license at your option.

See [LICENSE-MIT](../LICENSE-MIT) and [LICENSE-APACHE](../LICENSE-APACHE) for full text.
