# ADR-0010: k256-bmi2 scope is portable-only; BMI2/ADX removed

- **Status:** Accepted
- **Date:** 2026-07-17
- **Supersedes:** —
- **Superseded by:** —

## Context

The `k256-bmi2` crate was originally scoped to provide a drop-in
BMI2/ADX-accelerated `FieldElement::mul` for x86_64. Three forces
made the original scope untenable:

1. **The placeholder never delivered.** The `mul_bmi2_adx` function
   computed 2 of the 25 required partial products, then delegated
   to `mul_portable` — i.e. enabling `bmi2-adx` was a *slowdown*,
   not a speedup.
2. **The deployment target is arm64.** The project's primary
   development hardware is an Apple M3 Pro (arm64). BMI2/ADX is an
   Intel x86 instruction set; it does not exist on arm64. Even a
   completed BMI2/ADX implementation would deliver zero benefit on
   the developer's machine.
3. **k256's `FieldElement` is private.** A real drop-in would
   require maintaining a fork of k256 with the inner-field
   arithmetic replaced — a long-term maintenance cost that the
   project cannot justify given the perf characteristics of the
   existing implementation.

## Decision

Strip BMI2/ADX entirely. The crate becomes a portable 5×52
field-arithmetic reference implementation:

- `FieldElement5x52::mul` is a direct schoolbook multiplication on
  the 5×52 limbs, transcribed verbatim from k256's
  `FieldElement5x52::mul_inner`. The secp256k1 fast reduction uses
  the magic constant `r = 0x1000003D10 = 2^256 mod p` (in limb-0
  form), folding high-column carries back into the low half via
  the identity `2^256 ≡ 977 (mod p)`.
- `FieldElement5x52::square` uses the `a[i]*a[j] == a[j]*a[i]`
  symmetry (15 distinct products instead of 25, ~30% fewer
  128-bit multiplications), same reduction.
- Property tests cross-check every result byte-for-byte against
  `k256::FieldElement::mul` / `square`, so any deviation from the
  reference surfaces as a test failure.
- Zero `unsafe` blocks. No x86-specific code paths.
- The crate is **not** wired into the `find` hot path; `find` uses
  stock `k256::ProjectivePoint * scalar`.

## Consequences

**Positive:**
- Crate compiles on every target.
- Zero `unsafe`; audit surface limited to the schoolbook
  multiplication and reduction code, which is line-for-line
  identical to the k256 reference.
- Future SIMD backends (NEON for arm64, AVX-512 for x86) can
  replace the body of `mul` and inherit the property tests as a
  correctness oracle.

**Negative:**
- The crate does not accelerate `find`'s 27-30 M scalars/sec
  aggregate throughput. `find` continues to use stock k256.
- The 1 B scalars/sec target on M3 Pro is **not reachable** with
  single-scalar sweep algorithms: the chain step is ~12 field
  multiplications, so single-thread throughput is ~4 M scalars/sec
  and 12-core aggregate is ~50 M scalars/sec. See `docs/performance.md`
  for the quantified bottleneck breakdown.

## Path to actual acceleration

To meaningfully accelerate `find`'s 27-30 M/s on a single M3 Pro:

1. **NEON-vectorized field multiplication on arm64** (the
   equivalent of BMI2/ADX for arm64). Would lift `FieldElement::mul`
   from ~150 ns to ~50 ns — ~3× chain-step speedup, ~3× overall.
2. **wNAF windowed fixed-base scalar_mul_g**: precompute a table
   of 64 × 256 points, then a 256-bit scalar multiplication is
   64 mixed additions instead of 256. Drops the bootstrap cost
   from ~80 µs to ~15 µs, eliminating the dominant per-batch
   overhead.
3. **NAF-encoded `+/-G` chain**: combined add/sub per scalar
   (~6-8 mults instead of 12) — ~25% chain speedup.

Combined: ~8-10× over current, putting M3 Pro in the
200-300 M scalars/sec range. Still 3-5× short of 1 B/sec; that
target requires either server-class hardware (~33+ M3 Pro
machines) or a fundamentally different algorithm (Pippenger
multi-scalar, applicable only when many simultaneous targets
are searched).