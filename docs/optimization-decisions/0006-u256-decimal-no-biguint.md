# 0006 — Direct 256-bit divmod-by-10 instead of `BigUint::to_string`

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

`u256_to_decimal` previously converted a `crypto_bigint::U256`
into a decimal string via `BigUint::from_bytes_be(...).to_string()`.
This routed through `num_bigint::BigUint`, which allocates a heap
`Vec<limb>` for the intermediate representation.

## Decision

Walk the 32 big-endian bytes of the U256 one at a time,
maintaining a 64-bit accumulator and a small helper
`div_rem_u256_by_u64` that performs divmod by a small divisor in
a single byte-walk pass. Emit one ASCII digit per iteration into a
stack-allocated `Vec<u8>` that is reversed and converted to a
`String`.

## Consequences

**Positive:**
- One heap allocation (the output `String`) per call instead of two
  (the intermediate `BigUint` and the output).
- Eliminates a dependency on `num_bigint` from the lib (still used
  by `tests/integration.rs`).

**Negative:**
- The decimal conversion loop is now inline in the search module
  rather than delegated to `num_bigint`. Tests of decimal formatting
  are no longer covered by `num_bigint`'s test suite.

## References

- Source: [`src/search.rs::u256_to_decimal`](../../src/search.rs)