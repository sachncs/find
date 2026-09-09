# find

<div class="hero" markdown>

**Sub-second secp256k1 scalar discovery via 512-variant range-splitting and Montgomery batch inversion.**

Educational and research software for cryptographic pedagogy and high-performance Rust systems engineering.

</div>

## What this tool does

`find` searches for scalars `j` and offsets `V` such that `x(j·G) = x(P − V·G)`, yielding private-key candidates `d = V ± j (mod n)` against a secp256k1 target. It is built for two audiences:

1. **Cryptography learners** who want to see a working, well-tested multi-variant range-splitting implementation in pure Rust.
2. **Performance engineers** who want a clean reference for Montgomery batch inversion, Radix16 fixed-base scalar multiplication, and `OnceLock`-based early-exit coordination in a Rust 2021 codebase.

It is **not** a tool for recovering live Bitcoin private keys, and it is **not** constant-time. See [Disclaimer](#disclaimer) below.

## Two input modes

<div class="mode-grid" markdown>

<div class="mode" markdown>

### Pubkey mode (default)

Search the full scalar space for an X-coordinate match against a SEC1 public key.

```bash
find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
```

</div>

<div class="mode" markdown>

### Address mode

Search a user-specified scalar range `[from, to]` for the scalar behind a Bitcoin mainnet P2PKH / P2SH address (hash40 compare).

```bash
find --address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa \
     --from 1 --to 100000000
```

See [ADR-0011](adr/0011-address-discovery.md).

</div>

</div>

## Why it matters

| Pain point | What `find` does |
|---|---|
| Naive scalar sweep is O(n) with one scalar multiplication per scalar | A `+G` chain + Montgomery batch normalization collapses the inner loop into ~12 mixed additions and one batched inversion per 32 scalars |
| `cargo install find` is ambiguous because the crate name collides with GNU/BSD `find` | Search for [`sachncs/find`](https://github.com/sachncs/find) on GitHub, or use the alias `secp-find` |
| Educational crypto code usually sacrifices performance, and vice versa | A curated `pedantic + nursery` clippy policy with `-D warnings`, a 5 % performance-regression gate on hot paths, and full Miri-clean `unsafe` (one reviewed `libc::fsync` block) |

## Highlights

<div class="pill-grid" markdown>

<div class="pill" markdown>
<strong>512-variant search engine</strong>
Range-splitting with 256 powers of two + 256 cumulative sums; interned once per process.
</div>

<div class="pill" markdown>
<strong>Runtime-sized batches</strong>
`--batch-size 1..=256` honoured at runtime; hot-path arrays are heap-sized, not stack-sized.
</div>

<div class="pill" markdown>
<strong>Lock-free early exit</strong>
`OnceLock<SearchMatch>` replaces `Mutex + AtomicBool` (commit 6). Worker panics cannot corrupt the result.
</div>

<div class="pill" markdown>
<strong>Atomic checkpoints</strong>
Write-then-rename + parent-dir `fsync` (Unix); integrity-anchor verified on resume.
</div>

<div class="pill" markdown>
<strong>Binary caching</strong>
Pre-compute X-coordinates to disk for I/O-bound re-runs (~100× faster than CPU path on NVMe).
</div>

<div class="pill" markdown>
<strong>Structured observability</strong>
`tracing` + daily-rolling file appender; non-blocking logs decoupled from the hot path.
</div>

<div class="pill" markdown>
<strong>Five-layer verification</strong>
KAT, differential, audit, integration, property tests + Criterion micro-benchmarks.
</div>

<div class="pill" markdown>
<strong>Differential testing</strong>
Cross-checked against the reference C `libsecp256k1` for 12 boundary scalars.
</div>

</div>

## Quick start

### CLI

```bash
# Install from crates.io
cargo install find

# Or build from source
git clone https://github.com/sachncs/find.git
cd find
cargo build --release

# Run a pubkey sweep
./target/release/find --pubkey 0279be66... 

# Run an address-discovery sweep
./target/release/find --address 1A1zP1... --from 1 --to 100000000
```

See [Getting Started](getting-started.md) for the full walkthrough.

### Rust API

```rust
use find::config::Config;
use find::orchestrator;

let pubkey = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
let config = Config::new(pubkey, "data", false)
    .try_with_batch_size(32)?
    .try_with_variant_count(512)?;

let match_ = orchestrator::run(&config)?;
if let Some(m) = match_ {
    println!("MATCH DISCOVERED via {} at j={}", m.label, m.j);
    println!("Candidates: {:?}", m.candidates_hex());
}
# Ok::<(), find::error::FindError>(())
```

The full API surface is documented at [docs.rs/find](https://docs.rs/find).

## Where to next?

<div class="pill-grid" markdown>

<div class="pill" markdown>
<strong>[Getting Started](getting-started.md)</strong>
Install the tool and run your first sweep end-to-end.
</div>

<div class="pill" markdown>
<strong>[Architecture](architecture.md)</strong>
Module responsibilities, data flow, concurrency model, sync primitives.
</div>

<div class="pill" markdown>
<strong>[Algorithms](algorithms.md)</strong>
The math behind multi-variant range-splitting and Montgomery batch inversion.
</div>

<div class="pill" markdown>
<strong>[Performance](performance.md)</strong>
Per-scalar cycle breakdown, throughput ceilings, and tuning guide.
</div>

<div class="pill" markdown>
<strong>[CLI Reference](cli.md)</strong>
Every flag, every default, every example.
</div>

<div class="pill" markdown>
<strong>[Security](security.md)</strong>
Threat model, hardening notes, supported versions.
</div>

</div>

## Disclaimer

!!! warning "Educational and research use only"

    This software is for pedagogical exploration of elliptic-curve mathematics and high-performance Rust systems engineering. It is **not** constant-time, must not be used for production signing or verification, and is **not** designed to recover live Bitcoin private keys. See the full [DISCLAIMER.md](https://github.com/sachncs/find/blob/master/DISCLAIMER.md) on GitHub.

## License

[MIT](https://github.com/sachncs/find/blob/master/LICENSE-MIT) &copy; 2026 Sachin.

## Project status

The current shipping release line is `0.1.6`. The next release (`0.2.0`) ships the breaking API changes documented in the [Migration table in the README](https://github.com/sachncs/find/blob/master/README.md#migration-016--020). See the [Roadmap](roadmap.md) for what's queued.
