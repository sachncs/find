# Deployment

This document covers building, packaging, and deploying the `find` tool. For runtime operations (backup, monitoring, scaling), see [operations.md](operations.md). For performance tuning, see [performance.md](performance.md).

## System requirements

### Minimum requirements

- **CPU:** 2+ cores (4+ recommended)
- **RAM:** 4 GB minimum, 8 GB+ recommended
- **Storage:** 10 GB free disk space
- **OS:** Linux, macOS, or Windows

### Recommended for production

- **CPU:** 8+ physical cores for parallel search
- **RAM:** 16 GB+ for large searches
- **Storage:** 100 GB+ NVMe SSD for binary caching
- **GPU:** Not currently used; reserved for future CUDA acceleration (see [roadmap.md](roadmap.md))

For the full compatibility matrix, see [overview.md#compatibility-matrix](overview.md#compatibility-matrix).

## Building from source

### Release build

```bash
# Standard release build (recommended for production)
cargo build --release

# Or via Makefile
make build
```

The release binary is optimized for maximum throughput. The build profile (defined in `Cargo.toml`):

| Setting | Value | Effect |
|---|---|---|
| `opt-level` | `3` | Maximum LLVM optimization |
| `lto` | `"fat"` | Link-time optimization across all crates |
| `codegen-units` | `1` | Single code generation unit (enables inlining across crate boundaries) |
| `panic` | `'abort'` | No unwinding; smaller binary |
| `strip` | `true` | Strip debug symbols from the binary |
| `overflow-checks` | `true` | Enable integer overflow checks in release mode (correctness > speed) |

The binary is produced at `target/release/find` (or `find.exe` on Windows).

### Cross-compilation

For deploying to architectures other than the host:

```bash
# Add a target
rustup target add x86_64-unknown-linux-musl

# Build for the target
cargo build --release --target x86_64-unknown-linux-musl
```

The release pipeline (`.github/workflows/release.yml`) cross-compiles for all five supported targets; see [maintenance/release.md](maintenance/release.md#build-matrix) for the full matrix.

## Container deployment

### Multi-stage Dockerfile

A minimal, production-ready Dockerfile:

```dockerfile
# Build stage
FROM rust:1.70 AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/find /usr/local/bin/find

ENTRYPOINT ["find"]
```

The build stage uses the full `rust:1.70` image (required for the Rust toolchain); the runtime stage uses `debian:bookworm-slim` for a small footprint.

### Build and run

```bash
# Build the image
docker build -t secp256k1-find .

# Run a search
docker run --rm secp256k1-find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798

# Mount persistent data and log directories
docker run --rm \
  -v $(pwd)/data:/data \
  -v $(pwd)/logs:/logs \
  secp256k1-find --pubkey 0279be66... --output-dir /data --log-dir /logs
```

### Static binary variant

For a smaller image, use the `x86_64-unknown-linux-musl` target:

```dockerfile
FROM rust:1.70 AS builder
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /app
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/find /find
ENTRYPOINT ["/find"]
```

The resulting image is ~10 MB.

## Systemd service (Linux)

### Service unit file

Create `/etc/systemd/system/find@.service`:

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

# Hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/find /var/log/find

[Install]
WantedBy=multi-user.target
```

The `%i` instance parameter is the SEC1 pubkey (or a short hash of it).

### Installation

```bash
# Create a dedicated service user
useradd -r -s /bin/false find

# Copy the binary
cp target/release/find /opt/find/
chmod 755 /opt/find/find

# Create state directories
mkdir -p /var/lib/find /var/log/find
chown find:find /var/lib/find /var/log/find
chmod 700 /var/lib/find
chmod 750 /var/log/find

# Install the service unit
cp find@.service /etc/systemd/system/
systemctl daemon-reload

# Start the service for a specific pubkey
systemctl enable find@0279be66...hex...
systemctl start find@0279be66...hex...
```

The `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem`, and `ProtectHome` directives provide basic sandboxing.

## Environment configuration

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | Log level filter (`trace`, `debug`, `info`, `warn`, `error`) |
| `RUST_BACKTRACE` | `0` | Set to `1` for backtraces on panic |

For full configuration including compile-time constants, see [configuration.md](configuration.md).

### Configuration wrapper script

For complex deployments, wrap the binary in a script:

```bash
#!/bin/bash
# /opt/find/run.sh

set -euo pipefail

export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

exec /opt/find/find \
  --pubkey "${PUBKEY:?PUBKEY is required}" \
  --output-dir "${OUTPUT_DIR:-/var/lib/find}" \
  --log-dir "${LOG_DIR:-/var/log/find}" \
  "$@"
```

## Security hardening

See [security.md](security.md) for the full security model. The deployment-specific points:

### File permissions

```bash
# Restrict the data directory
chmod 700 /var/lib/find
chown find:find /var/lib/find

# Restrict the log directory
chmod 750 /var/log/find
chown find:find /var/log/find
```

### Network isolation

The tool does not require network access. Block outbound connections if running on shared systems:

```bash
# iptables (Linux)
iptables -A OUTPUT -m owner --uid-owner find -j REJECT

# Or use a network namespace
unshare -n -- /opt/find/find --pubkey 0279be66...
```

### Filesystem selection

For the binary cache, use a filesystem that supports atomic `pwrite_at`:

- **Linux:** ext4, XFS, btrfs
- **macOS:** APFS
- **Windows:** NTFS

Avoid network filesystems (NFS, SMB) for the cache; they may not support atomic `pwrite_at`.

## Monitoring

See [operations.md#monitoring](operations.md#monitoring) for log monitoring, checkpoint monitoring, and system metrics.

## Troubleshooting

For deployment-time errors, see [troubleshooting.md](troubleshooting.md#build-errors).

## See also

- [operations.md](operations.md) — runtime operations
- [configuration.md](configuration.md) — environment variables and constants
- [performance.md](performance.md) — performance tuning
- [security.md](security.md) — security model and hardening
- [maintenance/release.md](maintenance/release.md) — release process
