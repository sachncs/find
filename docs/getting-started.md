# Getting Started

This guide will help you get up and running with the Secp256k1 Find Tool.

## Prerequisites

- **Rust**: Version 1.70 or later (install via [rustup](https://rustup.rs/))
- **Operating System**: Linux, macOS, or Windows
- **Storage**: At least 32GB free disk space if using binary caching

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/sachn-cs/find.git
cd find

# Build in release mode (recommended for production use)
cargo build --release

# The binary will be at target/release/find
```

### Using Make

```bash
# Build with optimizations
make build

# Run all checks (lint, test, build)
make all
```

## Quick Start

### Basic Search

```bash
# Run a search against a public key
./target/release/find --pubkey <HEX_SEC1_PUBLIC_KEY>

# Example with a known public key
./target/release/find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

### With Binary Caching

```bash
# Generate binary cache during search (requires ~32GB per billion points)
./target/release/find --pubkey <HEX> --cache-points

# Later runs can reuse the cache for faster scans
```

### Custom Directories

```bash
# Specify custom data and log directories
./target/release/find --pubkey <HEX> --output-dir /path/to/data --log-dir /path/to/logs
```

## Understanding the Output

When a match is found, you'll see output like:

```
============================================================
MATCH DISCOVERED (Variant: 2^10)
Shift scalar V: 1024
Search scalar j: 42
Target candidates (d = V +/- j):
  [1] 0x426
  [2] 0x3e2
Total Search Duration: 2.345s
============================================================
```

### Output Fields

- **Variant**: The shift variant that produced the match
- **Shift scalar V**: The offset applied to the target point
- **Search scalar j**: The scalar that matched the X-coordinate
- **Target candidates**: The two possible private key values (V+j and V-j modulo n)

## Checkpointing

The tool automatically saves progress to `data/checkpoint.json` after each 1-billion-point segment. If the search is interrupted, it will resume from the last checkpoint on the next run.

### Checkpoint Integrity

On resume, the system verifies the checkpoint's cryptographic integrity by recomputing the X-coordinate of the last scalar. Mismatch causes an error rather than silent data corruption.

## Logging

Logs are written to the specified log directory (default: `logs/`) with daily rotation. Log files are named `find.log.YYYY-MM-DD`.

### Controlling Log verbosity

```bash
# Set log level via environment variable
RUST_LOG=debug ./target/release/find --pubkey <HEX>

# Trace level for maximum detail
RUST_LOG=trace ./target/release/find --pubkey <HEX>
```

## Next Steps

- Read [Architecture](architecture.md) for system design details
- Review [Deployment](deployment.md) for production deployment guidance
- Check [FAQ](faq.md) for common questions and issues
- See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines

## Troubleshooting

### Build Fails

Ensure you have Rust 1.70+ installed:

```bash
rustc --version
rustup update
```

### Out of Memory

Binary caching requires significant memory. Reduce `CACHE_CHUNK_SIZE` or avoid caching for very large searches.

### Checkpoint Corruption

If you encounter checkpoint errors, delete the `data/checkpoint.json` file and restart the search.

For more issues, see the [FAQ](faq.md) or open an issue on GitHub.
