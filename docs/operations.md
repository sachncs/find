# Operations

This document is a runbook for production deployments. It covers backup, restore, monitoring, scaling, and hot upgrades. For build-and-package steps, see [deployment.md](deployment.md). For runtime behavior, see [configuration.md](configuration.md) and [observability.md](observability.md).

## Resource budgets

The default configuration assumes the following resource budgets for a single search session:

| Resource | Minimum | Recommended |
|---|---|---|
| CPU cores | 2 physical | 8+ physical (linear speedup up to physical core count) |
| RAM | 4 GB | 16 GB+ (cache file is memory-mapped) |
| Disk (data) | 1 GB | 100 GB+ (one cache chunk = 32 GB) |
| Disk (logs) | 1 GB | 10 GB+ (log rotation may consume significant space at trace level) |
| Network | None | None (the tool does not use the network) |

## Disk budget

| Search mode | Disk per billion scalars |
|---|---|
| CPU-bound, no cache | ~1 KB (just the checkpoint) |
| Cached (`--cache-points`) | ~32 GB (one binary cache file) |

To calculate the total disk requirement for a multi-chunk search:

```
total_disk_gb = (search_range / 1_000_000_000) × 32 GB
```

For a full 64-bit sweep with caching:

```
total_disk_gb = (2^64 / 10^9) × 32 GB ≈ 5.9 × 10^11 GB
```

This is not a practical search; the disk budget is a constraint that limits the *cached* sweep to ranges of `~10^9` to `~10^10` scalars.

## Backup

The tool produces three categories of durable state:

| State | Location | Importance | Recommended backup |
|---|---|---|---|
| Checkpoint | `<output_dir>/checkpoint.json` | High (loss forces restart) | Hourly, retained for 7 days |
| Binary cache | `<output_dir>/checkpoints/chunk_*.bin` | Medium (recomputable) | Daily, retained for 30 days |
| Logs | `<log_dir>/find.log.YYYY-MM-DD` | Low (informational) | Daily, retained for 30 days |
| Variant metadata | `<output_dir>/points.json` | Low (re-creatable from target) | On change |

### Backup commands

```bash
# Checkpoint (high-priority, fast)
cp data/checkpoint.json data/checkpoint.json.backup.$(date +%s)

# Binary cache (slow, may be many GB)
rsync -av --progress data/checkpoints/ /backup/data/checkpoints/

# Logs
rsync -av --progress logs/ /backup/logs/
```

### Verification

After a restore, verify the checkpoint integrity by re-running the tool. The first segment will be a no-op (the checkpoint's `last_j` matches the current state); the tool logs `Verified integrity. Resuming from j = ...`.

## Restore

To restore from a backup:

1. Stop the running process.
2. Restore the relevant files.
3. Verify the checkpoint JSON is valid JSON: `cat data/checkpoint.json | jq .`
4. Re-run the tool. The checkpoint verification will run automatically.

If the checkpoint verification fails (`ResearchIntegrityError`), the file is corrupted and the search must restart from `j = 1`.

## Monitoring

### Log monitoring

Logs are written to the configured log directory with daily rotation.

```bash
# Follow logs in real-time
tail -f logs/find.log.*

# Search for errors
grep -r "ERROR" logs/

# Search for match events
grep -r "MATCH FOUND" logs/

# Count segments processed
grep -c "STARTING SEGMENT" logs/find.log.*

# Show the most recent audit boundary
grep "Audit boundary" logs/find.log.* | tail -1
```

### Checkpoint monitoring

The checkpoint file is the most reliable indicator of progress.

```bash
# Current state
cat data/checkpoint.json | jq .

# Monitor for changes
watch -n 5 'cat data/checkpoint.json | jq .last_j'

# Per-chunk progress
while true; do cat data/checkpoint.json | jq -r .last_j; sleep 60; done
```

### Cache monitoring

```bash
# List cache files
ls -lah data/checkpoints/

# Total cache size
du -sh data/checkpoints/

# Verify a single cache file
stat data/checkpoints/chunk_1.bin
# File size MUST be a multiple of 32 (i.e. the last 5 bits of the size are zero)
```

### System metrics

```bash
# CPU usage (per-core)
htop
mpstat -P ALL 1

# Memory
free -h

# Disk I/O
iostat -x 1

# Per-process I/O
iotop -p $(pgrep find)
```

## Scaling

### Vertical scaling

Adding more CPU cores is the most effective single improvement. The search engine scales approximately linearly with the number of physical cores, up to the memory bandwidth limit.

```bash
# Pin to specific cores (Linux)
taskset -c 0-7 ./find --pubkey 0279be66...

# Real-time priority (Linux, requires root)
nice -n -20 ./find --pubkey 0279be66...
```

### Horizontal scaling (manual)

The tool does not provide built-in distributed coordination. To scale across machines:

1. **Partition the range.** Divide `[1, u64::MAX]` into N disjoint sub-ranges and assign one per machine.
2. **Run with a unique `--output-dir` per machine** to avoid checkpoint and cache collisions.
3. **Share the variant generation.** All machines need the same 512 variants. Either share `points.json` via a network filesystem or regenerate independently (it is deterministic).
4. **Aggregate results.** The first machine to find the match wins; cancel the others.

This is a manual process and is not part of the tool's first-class API. See [roadmap.md#medium-term](roadmap.md#medium-term) for the planned distributed coordination.

## Hot upgrades

The tool does not support hot upgrades (replacing a running process with a new version while preserving state). To upgrade:

1. Stop the running process (Ctrl-C, `kill`, or container stop).
2. Wait for the in-flight checkpoint write to complete.
3. Replace the binary.
4. Restart the process. The checkpoint is reloaded and verified.

There is no risk of data loss if the upgrade is performed between chunk boundaries; the most recent chunk's progress will be lost (and recomputed on the next run), but the checkpoint at the start of that chunk is intact.

## Container deployment

The official release artifacts are statically linked binaries (modulo `libc` on Linux). A minimal container can be built with:

```dockerfile
FROM scratch
COPY target/release/find /find
ENTRYPOINT ["/find"]
```

A more practical base for debugging:

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY target/release/find /usr/local/bin/find
ENTRYPOINT ["find"]
```

Mount the data and log directories as volumes:

```bash
docker run --rm \
  -v $(pwd)/data:/data \
  -v $(pwd)/logs:/logs \
  find:latest --pubkey 0279be66... --output-dir /data --log-dir /logs
```

See [deployment.md](deployment.md) for the full deployment guide including systemd units.

## See also

- [deployment.md](deployment.md) — build, cross-compile, install
- [observability.md](observability.md) — log levels, audit boundaries
- [troubleshooting.md](troubleshooting.md) — common errors
- [security.md](security.md) — security model and hardening
