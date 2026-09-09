# ADR-0006: Raw 32-Byte X-Coordinate Binary Cache Format

- **Status:** Accepted
- **Date:** 2026-04-12
- **Supersedes:** —
- **Superseded by:** —

## Context

The orchestrator can be configured to write a **binary cache** of X-coordinates as a side-effect of the search. On a subsequent run, the cache is consumed by [`sweep_cached`](../../src/persistence.rs), which reads 32-byte blocks and matches them against the variant index without re-running scalar multiplications.

The cache file format must:

1. Be cheap to write and read sequentially at the maximum disk throughput.
2. Be self-describing for corruption detection.
3. Be self-contained: a file is enough; no auxiliary metadata is required.
4. Be portable across operating systems and endianness.
5. Support partial writes from concurrent workers without locking.

## Decision

The binary cache is a **flat sequence of 32-byte big-endian entries**, where each entry is the X-coordinate of `j·G` for `j ∈ [start, start+1, ..., end]`. Concretely:

```
+----------------+----------------+----------------+-----+----------------+
| x(start·G)[..]  | x((start+1)·G) | x((start+2)·G) | ... | x(end·G)[..]  |
+----------------+----------------+----------------+-----+----------------+
       32 bytes      32 bytes         32 bytes             32 bytes
```

Properties:

- **Total size** = `(end - start + 1) × 32` bytes. For the default `CACHE_CHUNK_SIZE = 10^9`, the file is ~32 GB.
- **No header or footer.** The file is a pure stream; all metadata (start, end, public key) is recovered from the orchestrator state.
- **End-of-file is implicit.** A truncated file is detected by the size check below.
- **Endianness:** all multi-byte integers are big-endian, matching the SEC1 X-coordinate encoding. This is portable across architectures.
- **Concurrent writes:** workers call `writer.write_block(offset, data)` with non-overlapping `offset` ranges. On Unix, `pwrite_at` is atomic; on other platforms, a `Mutex<File>` serializes writes. The per-batch size is small (~1 KB) so mutex contention is negligible.

**Corruption detection:** [`sweep_cached`](../../src/persistence.rs) verifies that the file size is a multiple of 32 bytes before reading. A file that fails the check returns [`CacheCorrupted`](../../src/error.rs) and the run aborts.

**File naming:** caches are stored under `<output_dir>/checkpoints/chunk_<start_j>.bin` so that the start scalar is recoverable from the filename. There is no need to read a header to know which range a cache file covers.

## Consequences

**Positive:**

- **Maximum sequential throughput.** No header parsing, no record boundaries, no deserialization. A `BufReader` reads `read_exact(&mut [u8; 32])` and feeds the bytes directly to the variant index. The disk is the bottleneck, not the parser.
- **Cache reuse across runs.** Once a chunk is cached, subsequent searches of the same range for a *different* public key reuse the cache — no recomputation. This is the primary motivator for the cache.
- **Self-validating.** The 32-byte alignment check catches accidental truncation, off-by-one errors in pre-allocation, and bit rot that flips a single byte.
- **Platform-portable.** The big-endian X-coordinate encoding is the same on every platform; no endian-conversion is needed.
- **Trivially recoverable.** A researcher can `cat` the binary file (hex dump) to inspect its contents. While not a primary use case, this is helpful for debugging.

**Negative:**

- **Large fixed disk footprint.** A 1-billion-point cache is ~32 GB. Disk space must be planned in advance; see [operations.md#disk-budget](../operations.md#disk-budget).
- **No partial validity.** A single corrupted byte invalidates the entire file (the alignment check fails). For long-running research this is a low-probability but high-impact failure mode.
- **No compression.** Compressed formats (`gzip`, `zstd`, `lz4`) were considered; the X-coordinate stream is effectively incompressible (high-entropy random-looking bytes), so compression would add CPU cost without disk savings.
- **No per-entry checksum.** A 4-byte CRC or SHA-256 per entry would catch corruption with finer granularity but would double or quadruple the file size and slow the I/O path. The current "all-or-nothing" model is the chosen trade-off.

## Alternatives Considered

### 1. Compressed binary (zstd)
Compress the X-coordinate stream. Rejected because:
- The data is high-entropy; expected compression ratio is ~1.0×.
- Decompression adds CPU cost on the hot read path.
- The format would no longer be trivially inspectable.

### 2. Per-entry checksum (CRC32 / SHA-256)
A 4-byte or 32-byte checksum per entry. Rejected because:
- The cache file is not durable-critical — if it corrupts, the next run regenerates it.
- The 32-byte alignment check at open time catches the most common failure modes.
- A checksum would not detect corruption that the alignment check misses (a 32-byte swap within a block would pass alignment but produce wrong X-coordinates).

### 3. Structured format (bincode, msgpack, parquet)
Encode the cache as a sequence of `{ j, x_bytes }` records. Rejected because:
- The `j` value is implicit in the file position; storing it would be redundant.
- Structured formats require deserialization, which is slower than direct byte reads.
- The cache is **not** intended to be portable across tool versions; a custom format is appropriate.

### 4. Indexed format (SQLite, LMDB, RocksDB)
A database would allow random access to a specific `j` without reading the entire file. Rejected because:
- The orchestrator always sweeps the cache sequentially, in order.
- A database adds a heavy dependency.
- Random-access I/O on a 32 GB file is not faster than sequential I/O on NVMe.

### 5. Header with metadata
A fixed-size header at the start of the file containing `start_j`, `end_j`, `version`, and a magic number. Rejected because:
- The header is a parse-time cost on every read.
- The filename already encodes `start_j`.
- The version is implicit in the binary that reads the file; a mismatched version would produce obviously wrong X-coordinates, not silent corruption.

## References

- Source: [`src/persistence.rs::BinaryCacheWriter`](../../src/persistence.rs), [`src/persistence.rs::sweep_cached`](../../src/persistence.rs)
- Architecture: [architecture.md#persistence-layer](../architecture.md#persistence-layer)
- Tests: [`src/persistence.rs::test_cached_sweep_corrupted_size`](../../src/persistence.rs), [`src/persistence.rs::test_cached_sweep_write_and_read_back`](../../src/persistence.rs)
- SEC1 X-coordinate encoding: <https://www.secg.org/sec1-v2.pdf>
- Related: [ADR-0005](0005-pure-search-module.md) — the cache format is consumed via the `CacheWriter` trait
