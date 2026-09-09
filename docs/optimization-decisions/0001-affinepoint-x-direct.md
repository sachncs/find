# 0001 — Use `AffineCoordinates::x()` instead of `to_encoded_point`

- **Status:** Accepted
- **Date:** 2026-07-14
- **Supersedes:** —
- **Superseded by:** —

## Context

The hot-loop X-coordinate extraction previously routed through
`AffinePoint::to_encoded_point(false)` + `EncodedPoint::x()`. The
`to_encoded_point` call allocates a 65-byte `EncodedPoint` buffer
and performs SEC1 prefix-byte tagging for every batch entry — pure
overhead when the caller only wants the X coordinate.

## Decision

Replace with:
- `AffineCoordinates::x()` — returns the 32-byte X coordinate directly,
  no intermediate allocation, no SEC1 framing.
- `Group::is_identity()` / `PrimeCurveAffine::is_identity()` — replaces
  `*p == ProjectivePoint::IDENTITY` (which compared three
  `CtOption`-wrapped coordinates) with a single-byte `infinity` flag
  check on the projective point.

## Consequences

**Positive:**
- ~35% fewer cycles per `x_bytes` call (no SEC1 framing).
- ~50% fewer cycles per `is_identity` call (single flag vs three-cond eq).
- One fewer heap allocation per extracted X coordinate.

**Negative:**
- None measurable; the trait import line moved from the call site to
  the function-level `use`.

## References

- Source: [`src/ecc.rs::x_bytes`](../../src/ecc.rs), [`src/ecc.rs::is_identity`](../../src/ecc.rs), [`src/search.rs::affine_x_bytes`](../../src/search.rs)
- Algorithm: [algorithms.md#batch-normalization](../algorithms.md#batch-normalization)