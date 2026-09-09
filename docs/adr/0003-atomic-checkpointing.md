# ADR-0003: Atomic Checkpointing via Write-Then-Rename

- **Status:** Accepted
- **Date:** 2026-04-12
- **Supersedes:** —
- **Superseded by:** —

## Context

The orchestrator processes the 64-bit scalar range in `CACHE_CHUNK_SIZE` (one billion) segments. Each segment is approximately 30 GB of cache file or many minutes of CPU work. The process may be interrupted at any time by:

- A crash or panic.
- An out-of-memory condition.
- A power loss.
- A deliberate user interruption (Ctrl-C, container shutdown, system reboot).

Without persistence, a long-running search would have to start over from `j = 1` after any interruption — making large-scale sweeps impractical.

The checkpoint must:

1. Survive process termination without corruption.
2. Be detectable as corrupted if the process was killed mid-write.
3. Verify cryptographic consistency on resume (the stored state must match the recalculated state).
4. Be inexpensive enough to write at the end of every chunk without dominating run time.

## Decision

We use a **write-then-rename** strategy with a JSON-encoded [`Checkpoint`](../../src/persistence.rs) struct:

1. The new state is serialized to JSON.
2. The JSON is written to a temporary file at `<path>.json.tmp` in the same directory as the target.
3. The file is `sync_all`-ed to flush its data to the storage device.
4. The temporary file is `rename`-d over the target path.
5. On Unix, the **parent directory** is also `fsync`-ed to ensure the rename itself is durable.

On resume, the checkpoint is **verified** by recomputing the X-coordinate of `last_j · G` and comparing it to the stored `last_x`. A mismatch raises [`ResearchIntegrityError`](#) and refuses to proceed.

The `find_map_any` early-exit semantic in the search engine means the checkpoint is only written when a chunk completes without finding a match. If a match is found, the checkpoint write is skipped and the process exits normally.

## Consequences

**Positive:**

- The rename is atomic on POSIX-compliant file systems (ext4, XFS, APFS, NTFS). A process killed between the write and the rename leaves the previous valid checkpoint untouched.
- The parent-directory `fsync` on Unix closes a subtle durability gap: most file systems require the directory entry to be flushed for the rename to survive a crash.
- The integrity anchor (X-coordinate of `last_j · G`) detects all forms of corruption: bit rot, accidental edits, partial writes that somehow leaked through, and version-skew mismatches.
- The JSON format is human-readable and trivially auditable — a researcher can inspect `checkpoint.json` to see exactly where the search paused.

**Negative:**

- Requires a single write per chunk (~32 GB of work), which is negligible.
- On Windows, the `fsync(parent dir)` is a no-op; the rename is still atomic on NTFS but the parent-durability guarantee is weaker. This is an acceptable trade-off because the alternative is to take a hard dependency on a Win32 API.
- The integrity check costs one scalar multiplication per resume — negligible.

## Alternatives Considered

### 1. Append-only log
Write a sequence of `<event, data>` records to a log file. On resume, replay the log. Rejected because:
- The log can grow unbounded.
- Recovery requires parsing the entire log.
- Concurrent appenders are complex to coordinate.

### 2. SQLite or similar embedded database
A single-file relational database is durable, transactional, and queryable. Rejected because:
- The checkpoint is a single record — no need for a relational schema.
- Adds a heavy dependency for a one-row table.
- The JSON format is more auditable for a research project.

### 3. No persistence
Restart from `j = 1` on every invocation. Rejected as impractical for chunk sizes that can take hours.

### 4. Compressed checkpoint
Use `zstd` or `gzip` to reduce the on-disk size. Rejected because:
- The checkpoint is ~150 bytes uncompressed — compression is pointless.
- Compressed formats are not human-readable.

### 5. Checkpoint embedded in the binary cache
Store the `last_j` as a header in the cache file. Rejected because:
- Couples the checkpoint lifecycle to the cache file, which is deleted between sessions.
- Cannot detect cross-pubkey contamination (resuming a search with a different target).

## References

- Source: [`src/persistence.rs::Checkpoint`](../../src/persistence.rs), [`src/persistence.rs::save_atomic`](../../src/persistence.rs), [`src/persistence.rs::verify`](../../src/persistence.rs)
- Architecture: [architecture.md#persistence-layer](../architecture.md#persistence-layer)
- Tests: [`tests/orchestrator.rs::test_orchestrator_resumes_from_checkpoint`](../../tests/orchestrator.rs), [`src/persistence.rs::test_checkpoint_verify_corrupted`](../../src/persistence.rs)
- POSIX rename atomicity: <https://man7.org/linux/man-pages/man2/rename.2.html>
