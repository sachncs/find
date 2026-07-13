# Troubleshooting

This document covers common errors, their causes, and resolutions. For conceptual questions, see [faq.md](faq.md). For deployment-time issues, see [deployment.md](deployment.md) and [operations.md](operations.md).

## Build errors

### `error[E0599]: no function or associated item named 'GENERATOR' found for struct 'ProjectivePoint'`

You are likely using an incompatible version of the `k256` crate. Pin the version:

```toml
k256 = { version = "0.13", features = ["arithmetic", "serde", "bits", "pkcs8"] }
```

### `error: package 'find' v1.0.0 (...) cannot be built because it requires rustc 1.70 or newer`

Update your Rust toolchain:

```bash
rustup update
rustup install stable
```

### `error: linking with 'cc' failed` (Linux, missing `gcc`)

Install a C compiler:

```bash
# Debian / Ubuntu
sudo apt-get install build-essential

# Fedora / RHEL
sudo dnf install gcc

# Arch
sudo pacman -S base-devel
```

### `error: failed to run custom build command for 'k256 v0.13.x'`

The `k256` crate requires `rustc 1.65` or newer. Update your toolchain (see above).

### `warning: unused import`

This is a lint warning, not an error. It is enabled by `#![warn(missing_docs)]` and `cargo clippy`. Resolve by removing the import or using `#[allow(unused_imports)]` if intentional.

## Runtime errors

### `Error: Invalid public key format: ...`

The `--pubkey` value could not be parsed as a SEC1 point. Common causes:

- **Wrong prefix.** SEC1 compressed keys start with `02` or `03`; uncompressed keys start with `04`. Other prefixes (e.g. `05`, `06`) are invalid.
- **Off-curve point.** The decoded X-coordinate does not correspond to a point on secp256k1.
- **Point-at-infinity.** SEC1 encoding of the identity point is invalid input; the tool rejects it.
- **Empty string.** The `--pubkey` value is empty (likely a quoting issue in the shell).
- **Hex decoding error.** The string contains non-hex characters or has odd length.

**Fix:** verify the pubkey is a valid SEC1 hex string. Example valid compressed key:

```
0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

### `Error: I/O error: ...` (with file paths)

A file operation failed. Common causes:

- **Permission denied.** The data or log directory is not writable by the current user.
- **Disk full.** The data directory is out of space (especially relevant for `--cache-points` which consumes ~32 GB per chunk).
- **Path not found.** The parent directory of `--output-dir` or `--log-dir` does not exist (the tool creates it on first write, but the *grandparent* must exist).
- **File too large.** The file system does not support files > 2 GB on 32-bit platforms (only relevant on 32-bit systems, which are not supported).

**Fix:** verify the directory exists, is writable, and has enough free space.

### `Error: Research integrity violation: Checkpoint X-coordinate mismatch: stored X, expected Y`

The checkpoint file is corrupted or was created by an incompatible binary version. The recalculated X-coordinate does not match the stored one.

**Fix:** delete the checkpoint and restart the search:

```bash
rm data/checkpoint.json
./find --pubkey 0279be66...
```

This is the only safe recovery; the tool will not proceed with a corrupted checkpoint because doing so would produce silent wrong results.

### `Error: Cache file corrupted: Cache file size N is not a multiple of 32 bytes`

A binary cache file has the wrong size. This can be caused by:

- **Truncation** from a disk-full condition during the precomputation.
- **Bit rot** (highly unlikely on modern storage).
- **Manual editing** of the cache file (do not do this).
- **Incompatible binary version** that wrote the file (none known; the format is stable).

**Fix:** delete the corrupted cache file. The next run will regenerate it:

```bash
rm data/checkpoints/chunk_<start_j>.bin
./find --pubkey 0279be66...
```

### `Error: ECC error: Scalar value exceeds curve order n`

A scalar input (in the `hex_to_scalar` path) is greater than or equal to the curve order `n = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141`. This is normally only encountered in tests or library code; the binary CLI does not currently accept arbitrary scalar inputs.

**Fix:** ensure the scalar is reduced modulo `n` before passing it to the API. The `hex_to_scalar` function does this automatically.

### `Error: Hex decoding error: Odd number of digits`

The input string has an odd number of hex digits. Each byte requires two hex digits; the input must have an even length.

**Fix:** pad the input with a leading zero if necessary: `0` + original.

### `Error: Serialization error: ...`

A JSON file (`checkpoint.json` or `points.json`) is malformed. This is usually caused by:

- **Manual editing** of a JSON file (use a JSON editor and validate before saving).
- **Truncation** from a disk-full condition during a checkpoint write.
- **Bit rot** (highly unlikely).

**Fix:** for the checkpoint, see the `Research integrity violation` entry above. For `points.json`, delete the file and restart (it will be regenerated).

## Performance issues

### Search takes longer than expected

Several factors can affect throughput:

1. **Debug build.** Always use `cargo build --release` for production searches. Debug builds are 10–100× slower due to disabled optimizations.
2. **CPU frequency scaling.** Set the CPU governor to `performance` rather than `powersave` or `schedutil`.
3. **Hyperthreading.** For consistent performance, disable hyperthreading in the BIOS or use `taskset` to bind to physical cores.
4. **Slow disk.** Binary cache reads are bounded by disk bandwidth. Use NVMe SSDs.
5. **Memory pressure.** The search engine has a small working set; if the system is swapping, throughput drops dramatically.

### High memory usage

The tool's heap usage is small (~16 KB for the variant index, ~3 KB per worker for batch arrays). If the process is consuming GB of memory:

- The `--cache-points` flag is **not** the cause (the cache file is memory-mapped by the kernel, not loaded into the process address space).
- Check the system allocator; if you are using `jemalloc` or `tcmalloc` for the system, they may be caching aggressively.
- Check the Rust nightly allocator; the default is the system allocator.

### Disk fills up

`--cache-points` consumes ~32 GB per billion scalars. To calculate the disk requirement for your search, see [operations.md#disk-budget](operations.md#disk-budget).

To free space while keeping the search running:

1. Stop the process.
2. Identify and delete the largest cache files you no longer need: `du -sh data/checkpoints/* | sort -h | tail`.
3. Restart the process. The deleted caches will be regenerated as needed.

## Logging issues

### Logs are not being written

Check the following:

- `--log-dir` exists and is writable.
- `RUST_LOG` is set to a level that emits events (e.g. `info`, not `off`).
- The process is still running (logs are buffered in memory; the buffer is only flushed on graceful exit or buffer-full).
- The non-blocking worker thread is alive; if the process is killed with `SIGKILL`, the buffer is lost.

### Log file is huge

`RUST_LOG=trace` emits per-batch events. For a 1-billion-point search, this is ~31 million events and easily consumes tens of GB. Use `RUST_LOG=info` (the default) for production.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Search completed; either a match was found or the space was exhausted without a match |
| `1` | Generic error (any `FindError` variant); the error message is printed to stderr |
| `101` | Rust panic (only if `RUST_BACKTRACE=1` is set) |

The exit code is set by `anyhow` based on the underlying `Result`. Any error from the tool's call chain produces a non-zero exit.

## Getting help

If the above does not resolve your issue:

1. Search [existing issues](https://github.com/sachncs/find/issues) on GitHub.
2. Open a [bug report](../.github/ISSUE_TEMPLATE/bug_report.md) with the full error message, the command line, and the environment details.
3. For security issues, follow the process in [SECURITY.md](../SECURITY.md) — do **not** open a public issue.

## See also

- [faq.md](faq.md) — conceptual questions
- [operations.md](operations.md) — backup, restore, monitoring
- [observability.md](observability.md) — log levels, tracing
- [security.md](security.md) — security model
