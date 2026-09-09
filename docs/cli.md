# CLI Reference

The `find` binary accepts one of two input shapes:

| Mode | Required flags | Sweep target | Compared against |
|---|---|---|---|
| **Pubkey mode** (default) | `--pubkey <hex>` | 512-variant X-coordinate index | Each scalar's `x(j·G)` |
| **Address mode** | `--address <base58>`, `--from` and `--to` | Hash40 of compressed pubkey | Each scalar's `RIPEMD-160(SHA-256(j·G compressed))` |

The two modes are mutually exclusive (`clap` rejects the second arg-pair
when the first is set). `find --pubkey <X> --address <A>` exits non-zero.

## Synopsis

```bash
# Pubkey mode (default, multi-variant X-coord sweep):
find [OPTIONS] --pubkey <HEX_SEC1>

# Address mode (hash40 sweep over a user range):
find -a <base58_address> --from <hex_or_dec> --to <hex_or_dec> [OPTIONS]
```

## Flags

| Flag | Short | Type | Default | Range | Mode | Description |
|---|---|---|---|---|---|---|
| `--pubkey` | `-p` | `String` | — | — | pubkey | HEX-encoded SEC1 public key (compressed or uncompressed). Required in pubkey mode; ignored in address mode. |
| `--address` | `-a` | `String` | — | — | address | Base58 Bitcoin mainnet address (P2PKH `0x00` or P2SH `0x05`). Strict Base58Check; non-standard versions rejected. |
| `--from` | — | `hex or dec` | `1` | `0..=2^64-1` | address | Inclusive scalar lower bound. Hex accepted with `0x` prefix. |
| `--to` | — | `hex or dec` | `u64::MAX` | `0..=2^64-1` | address | Inclusive scalar upper bound. Hex accepted with `0x` prefix. |
| `--output-dir` | `-o` | `String` | `data` | — | both | Data and checkpoint root directory |
| `--log-dir` | `-l` | `String` | `logs` | — | both | Rolling log directory |
| `--cache-points` | `-c` | `bool` | `false` | — | pubkey | Persist `j·G` X-coordinates to binary caches for multi-pubkey reuse. **Auto-disabled in address mode** (the cache stores X-coords, which the address sweep does not produce). |
| `--batch-size` | `-b` | `u32` | `32` | `1..=256` | both | Points per iteration batch; honoured at runtime. |
| `--variants` | `-V` | `u32` | `512` | `1..=512` | pubkey | Powers-of-two + cumulative-sum variant count. Ignored in address mode. |
| `--help` | `-h` | — | — | — | — | Print help |
| `--version` | `-V` | — | — | — | — | Print version |

The two runtime tunables (`--batch-size`, `--variants`) flow through
`Config::try_with_batch_size` / `Config::try_with_variant_count`
(commit 7a). Out-of-range values produce `FindError::InvalidConfig`
and exit non-zero.

## Examples

### Basic pubkey search

```bash
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

Runs a CPU-bound parallel sweep without writing any cache files.

### Address discovery with explicit range

```bash
find --address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa --from 1 --to 100000000
```

Searches scalars `d ∈ [1, 10^8]` and reports each `d` whose compressed
pubkey hashes to `62e907b15cbf27d5425399ebf6f0fb50ebb88f18`. The genesis-block
coinbase address is supplied here as a worked example; in practice it
isn't a known private-key address, but the path is identical for any
address whose keyspace overlaps the supplied `[from, to]` window.

### Address discovery (hex range)

```bash
find --address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa \
     --from 0xff00 --to 0xffff
```

Hex scalars are accepted with a `0x` prefix or as plain hex. Decimal
requires no prefix. Either form is auto-detected.

### With binary caching (pubkey mode only)

```bash
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798 --cache-points
```

Precomputes a 32 GB cache file per billion scalars. Subsequent runs
against any public key reuse the cache. `--cache-points` is silently
disabled in address mode (the address sweep does not produce the
X-coordinates that the cache expects).

### Resuming a checkpointed search (pubkey mode only)

```bash
# First run (creates checkpoint)
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798

# Interrupted, then resumed (verifies checkpoint integrity, continues)
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

If a `checkpoint.json` exists in `--output-dir`, the tool:

1. Reads it.
2. Verifies the integrity anchor by recomputing `x(last_j · G)`.
3. If the pubkey matches and the anchor is valid → resumes from `last_j + 1`.
4. If the pubkey mismatches → starts a fresh search (and logs a warning).
5. If the anchor is invalid → refuses to proceed (`ResearchIntegrityError`).

See [architecture.md#persistence-layer](architecture.md#persistence-layer) and [ADR-0003](adr/0003-atomic-checkpointing.md) for the checkpoint lifecycle.

## Input format

The `--pubkey` value must be a valid hex-encoded SEC1 point:

| Format | Bytes | First byte | Example |
|---|---|---|---|
| Compressed | 33 | `0x02` or `0x03` (Y-parity) | `0279be66...` |
| Uncompressed | 65 | `0x04` | `0479be66...3c1f...` |

Hex digits may be upper- or lower-case. The string is passed directly to `k256::PublicKey::from_sec1_bytes` after hex decoding.

Empty or malformed input produces a [`FindError::InvalidPublicKey`](modules.md#error) or [`FindError::HexError`](modules.md#error) and the binary exits with a non-zero status. Out-of-range `--batch-size` or `--variants` produces a [`FindError::InvalidConfig`](modules.md#error).

## Output

### On success (match found)

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

| Field | Meaning |
|---|---|
| `Variant` | The variant label that produced the match (e.g. `"2^10"`, `"sum(2^0..2^7)"`) |
| `Shift scalar V` | The original unreduced offset value (decimal) |
| `Search scalar j` | The small scalar that matched the X-coordinate |
| `Target candidates` | The two possible private keys, hex-encoded via `m.candidates_hex()` (V+j and V-j, both reduced mod n) |
| `Total Search Duration` | Wall-clock time of the entire search session |

The two candidates are emitted because X-coordinate matching cannot distinguish the Y-parity of `P - V·G`. Since commit 12 the `SearchMatch` struct holds them as `[k256::Scalar; 2]` (the `m.candidates` field); the CLI's `render_success_report` formats them via the `candidates_hex()` accessor. Callers must verify each candidate externally (e.g. by checking `candidate·G = P`) to determine the correct one.

### On completion (no match)

```
Search completed. No match found.
```

This is printed if the search space is exhausted without finding a match. The exit status is `0`.

### On error

Any error from the toolchain is printed to stderr in the form:

```
Error: <message>
```

The exit status is non-zero. The specific [`FindError`](modules.md#error) variant determines the message prefix:

| Variant | Prefix |
|---|---|
| `EccError` | `ECC error: ...` |
| `ResearchIntegrityError` | `Research integrity violation: ...` |
| `InvalidPublicKey` | `Invalid public key format: ...` |
| `InvalidConfig` | `Invalid configuration: ...` |
| `Io` | `I/O error: ...` |
| `HexError` | `Hex decoding error: ...` |
| `SerializationError` | `Serialization error: ...` |
| `CacheCorrupted` | `Cache file corrupted: ...` |

## Files written

The binary writes to two locations:

1. **Data directory** (default: `./data`) — contains:
   - `points.json` — variant metadata (X-coordinate → offset mapping) for auditability. Written once at the start of each session.
   - `checkpoint.json` — durable progress checkpoint. Written atomically at the end of every cache chunk.
   - `checkpoints/chunk_<start_j>.bin` — binary cache file (only when `--cache-points` is set or when an existing cache is reused).
2. **Log directory** (default: `./logs`) — contains:
   - `find.log.YYYY-MM-DD` — daily-rolling structured logs. See [observability.md](observability.md).

## See also

- [Configuration](configuration.md) — environment variables and runtime constants
- [Operations](operations.md) — backup, restore, monitoring
- [Troubleshooting](troubleshooting.md) — common error messages and resolutions
- [Observability](observability.md) — log levels, tracing, audit boundaries
