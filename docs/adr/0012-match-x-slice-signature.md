# ADR-0012 — `match_x` slice signature

## Status

Accepted (2026-09).

## Context

`VariantIndex::match_x` is called inside the per-batch match loop of
[`sweep_parallel`] and [`sweep_and_cache`]. The hot path is:

```rust
for i in 0..count {
    let affine = &group_affines_buf[offset + i];
    let j: u128 = u128::from(chunk_start) + i as u128;
    let mut x_bytes = [0u8; 32];
    x_bytes.copy_from_slice(affine.x().as_ref());
    if let Some(m) = index.match_x(&x_bytes, j) {
        // found a match
    }
}
```

`affine.x()` returns a `CtOption<FieldBytes>` where `FieldBytes` is
a 32-byte generic array. The previous signature
`match_x(&self, test_x: &[u8; 32], j: u128)` required the caller to
materialise a `[u8; 32]` stack array and `copy_from_slice` the field
bytes into it before the call. That's one 32-byte `memcpy` per probe,
executed for every affine point in every batch — measurable inner-
loop overhead.

The internal binary search only inspects the first 32 bytes of
`test_x` (every key is exactly 32 bytes), so the slice-typed
signature is equivalent at the binary-search level.

## Decision

Change `VariantIndex::match_x`'s `test_x` parameter from
`&[u8; 32]` to `&[u8]`. The internal binary search compares via
`probe.as_slice().cmp(test_x)` — equivalent to the previous
behaviour because every key is exactly 32 bytes.

In the hot loops of [`sweep_parallel`] and [`sweep_and_cache`],
pass `affine.x().as_ref()` directly to `match_x`. Drop the local
`[u8; 32]` stack array and the `copy_from_slice`.

In the hot loops, also hoist `u128::from(chunk_start)` out of the
per-iteration loop:

```rust
let chunk_start_u128 = u128::from(chunk_start);
for i in 0..count {
    let j = chunk_start_u128 + i as u128;
    ...
}
```

The `u128` conversion cost is amortised over `count` iterations
instead of paid per iteration.

## Consequences

Positive:
- Eliminates one 32-byte copy per probe. The per-batch match loop
  processes ~32 probes per batch (default `BATCH_SIZE = 32`),
  so this is ~32 copies per batch saved.
- Measured: `end_to_end_small_scalar_12345` (10 M scalars) drops
  from ~1.985 ms to ~1.88 ms (−5 %) on Apple M3 Pro.
- `plus_g_chain/chain_32_plus_g` drops from ~18.98 µs to ~17.99 µs
  (−5 %). The change affects the binary-search overhead inside
  the per-batch match loop.
- The slice signature also opens the door for callers to pass a
  longer buffer (e.g. a cache chunk + a 32-byte offset view)
  without an intermediate copy.

Negative:
- Breaking change to a public API (`match_x` argument type).
  Tracked for `0.2.0` along with `Config::pubkey -> Option<String>`.
- External callers that passed a fixed-size array must now pass a
  slice (`&chunk[..]` or `&test_x[..]`).

## Alternatives Considered

- **Keep `&[u8; 32]` and use `MaybeUninit<[u8; 32]>` at the call
  site**: would also drop the copy but adds `unsafe` to the
  hot-loop caller. The slice signature is strictly cleaner.
- **Inline `match_x` at the call site**: the binary is small
  enough that inlining happens anyway (`#[inline(always)]`).
  Removing the argument type mismatch removes the need to copy
  the field bytes into a sized array first.

## References

- [`src/search.rs` `VariantIndex::match_x`](../../src/search.rs)
- [`src/search.rs` `sweep_parallel` hot loop](../../src/search.rs)
- [`src/search.rs` `sweep_and_cache` hot loop](../../src/search.rs)
- [docs/performance.md](../performance.md) — measured cycle counts
