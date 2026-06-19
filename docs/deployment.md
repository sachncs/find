# Deployment Guide

This guide covers deploying the Secp256k1 Find Tool in various environments.

## System Requirements

### Minimum Requirements

- **CPU**: 2+ cores (4+ recommended)
- **RAM**: 4GB minimum, 8GB+ recommended
- **Storage**: 10GB free disk space
- **OS**: Linux, macOS, or Windows

### Recommended for Production

- **CPU**: 8+ cores for parallel search
- **RAM**: 16GB+ for large searches
- **Storage**: 100GB+ SSD for binary caching
- **GPU**: Optional, for future CUDA acceleration

## Building for Production

### Release Build

```bash
# Optimized release build
cargo build --release

# Or using Make
make build
```

The release binary is optimized with:
- `opt-level = 3` — Maximum optimization
- `lto = "fat"` — Link-time optimization across all crates
- `codegen-units = 1` — Single codegen unit for maximum optimization
- `panic = 'abort'` — No unwinding, smaller binary
- `strip = true` — Strip debug symbols

### Cross-Compilation

For deploying to different architectures:

```bash
# Install target
rustup target add x86_64-unknown-linux-musl

# Build for target
cargo build --release --target x86_64-unknown-linux-musl
```

## Docker Deployment

### Dockerfile

```dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/find /usr/local/bin/

ENTRYPOINT ["find"]
```

### Build and Run

```bash
# Build image
docker build -t secp256k1-find .

# Run container
docker run --rm secp256k1-find --pubkey <HEX_SEC1>
```

## Systemd Service (Linux)

### Service File

```ini
[Unit]
Description=Secp256k1 Find Tool
After=network.target

[Service]
Type=simple
User=find
Group=find
WorkingDirectory=/opt/find
ExecStart=/opt/find/find --pubkey %i --output-dir /var/lib/find --log-dir /var/log/find
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Installation

```bash
# Copy binary
cp target/release/find /opt/find/

# Create service user
useradd -r -s /bin/false find

# Create directories
mkdir -p /var/lib/find /var/log/find
chown find:find /var/lib/find /var/log/find

# Install service
cp find.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable find@<PUBKEY_HASH>
```

## Environment Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level filter |
| `RUST_BACKTRACE` | `0` | Set to `1` for backtraces on panic |

### Configuration File

For complex deployments, consider wrapping the binary in a script:

```bash
#!/bin/bash
# /opt/find/run.sh

export RUST_LOG=info
export RUST_BACKTRACE=1

exec /opt/find/find \
  --pubkey "$PUBKEY" \
  --output-dir /var/lib/find \
  --log-dir /var/log/find
```

## Monitoring

### Log Monitoring

Logs are written to the configured log directory with daily rotation:

```bash
# Follow logs in real-time
tail -f logs/find.log.*

# Search for errors
grep -r "ERROR" logs/

# Check for matches
grep -r "MATCH DISCOVERED" logs/
```

### Checkpoint Monitoring

Monitor checkpoint progress:

```bash
# Check checkpoint file
cat data/checkpoint.json | jq .

# Monitor checkpoint updates
watch -n 5 'ls -la data/checkpoint.json'
```

### System Metrics

Monitor system resources during search:

```bash
# CPU usage
htop

# Disk I/O
iostat -x 1

# Memory usage
free -h
```

## Performance Tuning

### CPU Optimization

- Ensure the process runs on dedicated cores
- Disable hyperthreading for consistent performance
- Use `taskset` to bind to specific CPU cores

### Memory Optimization

- Reduce `CACHE_CHUNK_SIZE` for memory-constrained environments
- Monitor heap usage with `jemalloc` or `tcmalloc`
- Use `MALLOC_CONF` for allocator tuning

### I/O Optimization

- Use NVMe SSDs for binary cache storage
- Ensure filesystem supports `pwrite` atomically (ext4, XFS, APFS)
- Consider RAID 0 for maximum throughput

## Backup and Recovery

### Backup Strategy

```bash
# Backup checkpoints
cp data/checkpoint.json data/checkpoint.json.backup

# Backup binary caches
tar -czf cache-backup.tar.gz data/cache/

# Backup logs
tar -czf logs-backup.tar.gz logs/
```

### Recovery

```bash
# Restore checkpoint
cp data/checkpoint.json.backup data/checkpoint.json

# Verify integrity
# The tool will verify on next run
```

## Security Hardening

### File Permissions

```bash
# Restrict data directory
chmod 700 /var/lib/find
chown find:find /var/lib/find

# Restrict log directory
chmod 750 /var/log/find
chown find:find /var/log/find
```

### Network Security

- The tool does not require network access
- Block outbound connections if running on shared systems
- Use firewall rules to isolate the execution environment

### Input Validation

- All public keys are validated on input
- Checkpoint integrity is verified on resume
- Binary cache files are validated for correct size

## Scaling

### Vertical Scaling

- Add more CPU cores for parallel search
- Increase memory for larger batch sizes
- Use faster storage for I/O-bound operations

### Horizontal Scaling

- Distribute search ranges across multiple machines
- Share binary cache files via NFS or object storage
- Coordinate checkpoints via shared filesystem

## Troubleshooting

### Common Issues

1. **Out of Memory**: Reduce batch size or cache chunk size
2. **Checkpoint Corruption**: Delete checkpoint and restart
3. **Permission Denied**: Check file ownership and permissions
4. **Slow Performance**: Verify release build optimizations

### Debug Mode

```bash
# Run with debug logging
RUST_LOG=debug ./find --pubkey <HEX>

# Run with trace logging (very verbose)
RUST_LOG=trace ./find --pubkey <HEX>
```

### Performance Profiling

```bash
# Generate profiling data
perf record -g ./find --pubkey <HEX>

# Analyze with flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg
```

## Version Upgrades

### Upgrade Process

1. Stop the running service
2. Backup checkpoint and cache files
3. Install new binary
4. Verify checksum
5. Start service
6. Monitor logs for errors

### Rollback

If issues occur:
1. Stop the service
2. Restore previous binary
3. Restore checkpoint from backup
4. Restart service

## Support

For deployment issues:
- Check [FAQ](faq.md) for common problems
- Review [Architecture](architecture.md) for design details
- Open an issue on GitHub with deployment details
