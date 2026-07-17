# ADR-0011: Address-discovery input mode

- **Status:** Accepted
- **Date:** 2026-07-17
- **Supersedes:** —
- **Superseded by:** —

## Context

The `find` tool historically required a SEC1 hex-encoded compressed
pubkey as input (`--pubkey <hex>`) and swept the scalar range producing
candidate keys that matched the pubkey's X-coordinate against a
precomputed 512-variant set. This works well when the user already
has the SEC1 pubkey — e.g. an admin inspecting a transaction they
control.

But the common Bitcoin workflow starts the other way around: the user
**has an address** (e.g. a payment destination they want to associate
with a contact) and they want to know which key (`d` in `[1, n-1]`)
generated that address. The address is the higher-level identifier
humans use; the pubkey is an intermediary.

The user asked for a feature with three input fields:

  Bitcoin Address: <base58>
  Key Range (Bits): <b1>...<b2>
  Key Range (HEX): <hex_from>:<hex_to>

i.e. "give me the scalar behind this address, and the scalar lives
in this range". This is a separate problem shape from the variant-keyed
sweep. We adopt the new input mode.

## Decision

Add a third `find` discovery mode: **address-keyed sweep**.

The CLI surface accepts `--address <base58>` together with `--from` and
`--to` (both inclusive scalars, decimal or `0x`-hex), and excludes
`--pubkey` (clap-level `conflicts_with`).

Inputs are decoded by a strict Base58Check decoder:

  - mainnet P2PKH (`0x00`) and P2SH (`0x05`) version bytes only,
  - testnet (0x6f), bech32/segwit, and any other version byte are
    rejected at parse time with `FindError::InvalidAddress`.

The sweep iterates scalars `d` in `[from, to]` and tests each `d * G`
against the address's `hash40 = RIPEMD160(SHA256(compressed_pubkey))`.
There is no ±V offset because the target uniquely identifies one of
two parity-mirror candidates for each scalar (the address depends
only on the compressed pubkey, not on `V`).

Storage: an `Address40([u8; 20])` newtype. Plus `bitcoin_address_to_hash40`
returns `(version_byte, Address40)` so callers can distinguish P2PKH
from P2SH if needed; the orchestrator does not.

Hot loop: same `+ G` chain shape as [`sweep_parallel`], but the
post-process is RIPEMD-160(SHA-256(compressed)) on a per-step basis
instead of X-coordinate to affine normalization and a 9-bit binary
search. This costs ~1 µs per step on M3 Pro (mostly the SHA-256 +
RIPEMD-160 cost; the projective arithmetic is amortized the same way
as the variant sweep via SUPER_BATCHES / NORMALIZE_GROUP_BATCHES).

In the result, `SearchMatch.candidates = [d, n-d]` keeps the
two-candidate shape used elsewhere in the codebase; the relationship
`V+j = d, V-j = n-d` degenerates to `V = 0` in the address mode
(single candidate per hit). `SearchMatch.label = "address/d"` and
`offset_decimal` carries the full 40-character hash160 hex for
at-a-glance CLI output.

Performance characteristics:

  - per-scalar cost ≈ +G chain (~110 ns cold) + SHA-256 + RIPEMD-160
    (~1 µs on M3 Pro) ≈ **1 µs per scalar**,
  - range `[1, u64::MAX]` sweeps ~10^19 iterations → ~30 core-years,
  - range `[2^70, 2^71+2^68)` would be `~10^15 seconds` on a single
    M3 Pro.

The current build uses `u64` for the scalar range; values above
`u64::MAX` are out of scope. This is reflected in the CLI and in the
error message surfaced when the user supplies a hex range that
overflows.

## Consequences

**Positive:**

- The two-mode CLI matches how humans actually think about the
  problem: I-have-an-address vs I-have-a-pubkey.
- Base58Check strictness means invalid addresses (bad checksums, non-
  mainnet version bytes) are rejected up front, never producing a
  silent sweep.
- A new `Address40` newtype encodes the address's hash40 with
  minimal overhead, including a 40-character padded hex form for
  CLI display.

**Negative:**

- `--cache-points` is silently disabled in address mode: the cache
  stores X-coordinates of chain points, but the address sweep does
  not produce them. A future enhancement could lift this (cache the
  hash40 instead), but is out of scope for v1.
- The range is `u64`-bounded. Wider ranges (the form's "Bits 270..271"
  example, etc.) cannot be represented in the current build; would
  require a `u128` scalar range across the entire codebase. Out of
  scope.
- The address hash40 **is not invertible**. Given only an address
  hash, recovering the original pubkey would require a lookup against
  the entire Bitcoin UTXO set (~1 TB); this is not what a small
  research tool can do. We never pretend to invert.

## Implementation references

- `src/address.rs`: `Address40`, `bitcoin_address_to_hash40`,
  Base58Check decode/encode.
- `src/config.rs`: `range_from`, `range_to`, `target_address` fields
  on `Config`, with `try_with_range` and `try_with_target_address`
  builders.
- `src/search.rs`: `sweep_address` (the core loop), `hash160_matches`,
  `make_address_match`.
- `src/orchestrator.rs`: `run_address_mode` dispatcher.
- `src/main.rs`: `--address`, `--from`, `--to` CLI flags plus the
  clap-level `conflicts_with` with `--pubkey`.
- `tests/kat.rs`, `tests/orchestrator.rs`: end-to-end harness that
  builds a Bitcoin address from a known scalar and exercises the
  full pipeline.
