# Security Model

This document describes the security model of the `find` tool. For vulnerability reporting, see [SECURITY.md](../SECURITY.md).

## Threat model

The tool is for **educational and research use only**. It is not designed to protect against adversaries; it is designed to demonstrate algorithms. With that scope in mind, the threat model is:

### In scope

| Threat | Mitigation |
|---|---|
| **Cryptographic correctness.** A bug in scalar arithmetic, modular reduction, or X-coordinate extraction could produce silently wrong candidates. | Property-based tests, integration tests with known scalars, deterministic RNG for randomized discovery, `k256` as the cryptographic primitive. |
| **File-system race conditions.** Concurrent writes to the checkpoint or cache could corrupt the state. | Write-then-rename atomic persistence (see [ADR-0003](adr/0003-atomic-checkpointing.md)); parent-directory `fsync` on Unix; `Mutex<File>` in `FileCacheWriter`. |
| **Cache corruption.** A truncated or modified cache file could produce wrong matches. | Size must be a multiple of 32 bytes; `CacheCorrupted` error otherwise (see [ADR-0006](adr/0006-binary-cache-format.md)). |
| **Memory safety.** A use-after-free, buffer overflow, or out-of-bounds access. | One reviewed `unsafe` call (`libc::fsync` in `src/persistence.rs`); no other `unsafe` in application code. |
| **Dependency vulnerabilities.** A bug in `k256`, `rayon`, or another dependency. | `cargo audit` in CI on every push (`.github/workflows/ci.yml`); `cargo deny check` for license and dependency-graph auditing. |
| **Panic in a Rayon worker.** A panic in one worker could abort the entire process. | Custom `panic_handler` in `src/main.rs` that logs and continues; `Mutex::lock()` callers tolerate poisoning via `into_inner()`. |

### Out of scope

| Threat | Reason |
|---|---|
| Social engineering, phishing | The tool is not a network service. |
| Local privileged access | The tool runs in user space; an attacker with root can read all state. |
| Side-channel attacks (timing, power) | The hot path is not constant-time. This is acceptable for the research-pedagogical use case; production cryptographic libraries must be constant-time. |
| Network attacks | The tool does not use the network. |
| Supply-chain attacks on the build environment | Mitigated by Dependabot, `cargo deny`, and CI; the project's threat model does not extend to CI hardening. |

## Security properties

The following properties are guaranteed by the implementation and verified by the test suite:

### 1. Cryptographic correctness

The tool uses `k256` for all elliptic-curve arithmetic. The wrapper in [`src/ecc.rs`](../src/ecc.rs) does not implement any cryptographic primitives itself; it composes `k256`'s audited primitives.

**Verified by:**

- [`src/ecc.rs::tests::test_sub_definition_consistency`](../src/ecc.rs)
- [`src/ecc.rs::tests::test_sub_self_identity`](../src/ecc.rs)
- [`src/ecc.rs::tests::prop_sub_reversibility`](../src/ecc.rs)
- [`src/ecc.rs::tests::prop_sub_curve_membership`](../src/ecc.rs)
- [`tests/audit.rs::test_rigorous_recovery_1234567890`](../tests/audit.rs) (end-to-end recovery of a known scalar)

### 2. Atomic state persistence

Checkpoints are written via `write-then-rename`, which is atomic on POSIX-compliant file systems. The parent directory is `fsync`-ed on Unix to ensure the rename is durable. See [ADR-0003](adr/0003-atomic-checkpointing.md) for the full design.

**Verified by:**

- [`src/persistence.rs::test_checkpoint_roundtrip`](../src/persistence.rs)
- [`src/persistence.rs::test_checkpoint_verify_corrupted`](../src/persistence.rs)
- [`tests/orchestrator.rs::test_orchestrator_resumes_from_checkpoint`](../tests/orchestrator.rs)

### 3. Cache integrity

Binary cache files are validated on open. A file that is not a multiple of 32 bytes returns `CacheCorrupted` and aborts the run, preventing silently wrong results.

**Verified by:**

- [`src/persistence.rs::test_cached_sweep_corrupted_size`](../src/persistence.rs)
- [`src/persistence.rs::test_cached_sweep_write_and_read_back`](../src/persistence.rs)
- [`src/persistence.rs::test_cached_sweep_empty_file`](../src/persistence.rs)

### 4. Input validation

All public-key parsing goes through [`src/ecc.rs::parse_pubkey`](../src/ecc.rs), which:

- Decodes the hex string.
- Validates the SEC1 prefix (`0x02`, `0x03`, or `0x04`).
- Validates the point is on the secp256k1 curve (delegated to `k256`).
- Rejects the point-at-infinity.

**Verified by:**

- [`src/ecc.rs::test_parse_valid_compressed`](../src/ecc.rs)
- [`src/ecc.rs::test_parse_pubkey_empty_string`](../src/ecc.rs)
- [`src/ecc.rs::test_parse_pubkey_invalid_hex`](../src/ecc.rs)
- [`src/ecc.rs::test_parse_pubkey_malformed_sec1`](../src/ecc.rs)

### 5. Memory safety

The crate contains **one `unsafe` block** in application code: the parent-directory `fsync` in [`src/persistence.rs`](../src/persistence.rs), which is necessary to call the POSIX `fsync` syscall directly:

```rust
let _ = unsafe { libc::fsync(dir.as_raw_fd()) };
```

This `unsafe` is reviewed and acceptable: it calls a C function with a valid file descriptor and ignores the return value (since failure to `fsync` the parent directory is not fatal — the worst case is a slightly less durable rename). A `// SAFETY:` comment documenting the validity of the file descriptor and the rationale for discarding the result accompanies the block.

All other memory access is bounds-checked by the Rust compiler.

### 6. Dependency hygiene

CI enforces:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo audit` (vulnerability database)
- `cargo deny check all` (license allowlist, dependency graph)

The dependency allowlist in [`deny.toml`](../deny.toml) restricts licenses to MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-DFS-2016, and Zlib. Copyleft licenses are denied.

## Hardening for production

If the tool is deployed in a sensitive environment (e.g. a research cluster with shared storage), the following hardening steps are recommended:

### File permissions

```bash
# Restrict the data directory to a dedicated user
useradd -r -s /bin/false find
mkdir -p /var/lib/find /var/log/find
chown -R find:find /var/lib/find /var/log/find
chmod 700 /var/lib/find
chmod 750 /var/log/find
```

### Network isolation

The tool does not require network access. Block outbound connections at the firewall if running on a shared system:

```bash
# iptables (Linux)
iptables -A OUTPUT -m owner --uid-owner find -j REJECT
```

### Resource limits

Use cgroups or systemd unit limits to bound the process:

```ini
# systemd unit snippet
[Service]
MemoryMax=4G
CPUQuota=800%  # 8 cores
```

### Filesystem selection

For the binary cache, use a filesystem that supports atomic `pwrite_at`:

- **Linux:** `ext4`, `XFS`, `btrfs`
- **macOS:** APFS
- **Windows:** NTFS (parent-directory `fsync` is a no-op, but atomic rename is supported)

Avoid network filesystems (NFS, SMB) for the cache; they may not support atomic `pwrite_at`.

### Audit logging

Forward the log directory to a central log aggregator with append-only storage:

```bash
# rsyslog (Linux)
*.* @log-aggregator.internal:514
```

The tool's log format is greppable by timestamp, level, and module — see [observability.md](observability.md).

## What the security model is **not**

- **Not a digital signature implementation.** The tool performs *search* over the scalar field; it does not sign, verify, encrypt, or decrypt. Using it for anything other than search is a misuse.
- **Not constant-time.** The hot path includes branch decisions on the data (e.g. the `if let Some(x) = ...` pattern in batch normalization). This is acceptable for the research use case but disqualifies the tool for any setting where timing leakage is a concern.
- **Not formally verified.** The cryptographic correctness rests on the `k256` crate's implementation and the test suite. A formal proof of the matching invariant or the batch normalization correctness is on the [roadmap](roadmap.md#long-term).
- **Not audited by an external security firm.** The tool is a research project; no professional security audit has been performed.

## Reporting vulnerabilities

See [SECURITY.md](../SECURITY.md) for the disclosure process.

## See also

- [security policy](../SECURITY.md) — vulnerability reporting
- [disclaimer](../DISCLAIMER.md) — intended use restrictions
- [ADR-0003](adr/0003-atomic-checkpointing.md) — atomic checkpointing
- [ADR-0006](adr/0006-binary-cache-format.md) — binary cache format
- [testing.md](testing.md) — test methodology
