# References

External reading and standards that informed the design and implementation of the `find` tool.

## Standards

### SEC 1
- **Standards for Efficient Cryptography 1 (SEC 1)** — Certicom Research.
- The tool accepts SEC1 v2.0 encoded public keys (compressed and uncompressed). See [`ecc::parse_pubkey`](../src/ecc.rs).
- Reference: <https://www.secg.org/sec1-v2.pdf>

### SEC 2
- **Standards for Efficient Cryptography 2 (SEC 2)** — Certicom Research.
- Defines the secp256k1 curve parameters used in this project.
- Reference: <https://www.secg.org/sec2-v2.pdf>

## RFCs

- **RFC 6979** — *Deterministic Usage of the Digital Signature Algorithm (DSA) and Elliptic Curve Digital Signature Algorithm (ECDSA).* Informative for understanding scalar arithmetic in the secp256k1 field, though this project does not implement signing.
- <https://datatracker.ietf.org/doc/html/rfc6979>

## Algorithms and Techniques

### Montgomery simultaneous inversion
- *Speeding the Pollard and Elliptic Curve Methods of Factorization* — Peter L. Montgomery (1987).
- The technique used for [batch normalization](algorithms.md#batch-normalization).
- Modern exposition: <https://en.wikipedia.org/wiki/Montgomery%27s_modular_multiplication#Montgomery_simultaneous_inversion>

### Baby-step giant-step / range splitting
- *A Method of Solving a Class of Problems in Elementary Number Theory* — Shanks (1971).
- Conceptual ancestor of the multi-variant range-splitting used here. See [algorithms.md](algorithms.md#multi-variant-range-splitting).
- Background: <https://en.wikipedia.org/wiki/Baby-step_giant-step>

### Elliptic curve point symmetry
- The fact that `x(P) = x(-P)` on an elliptic curve is the algebraic foundation of the tool's matching invariant. See [algorithms.md](algorithms.md#matching-invariant).

## Rust ecosystem

- **`k256` crate** — Pure-Rust secp256k1 implementation.
  - Documentation: <https://docs.rs/k256>
  - Repository: <https://github.com/RustCrypto/elliptic-curves/tree/master/k256>
- **`rayon` crate** — Work-stealing data parallelism for Rust.
  - Documentation: <https://docs.rs/rayon>
  - Repository: <https://github.com/rayon-rs/rayon>
- **`tracing` crate** — Application-level tracing and logging.
  - Documentation: <https://docs.rs/tracing>
  - Repository: <https://github.com/tokio-rs/tracing>
- **`clap` crate** — Command-line argument parser.
  - Documentation: <https://docs.rs/clap>
- **`thiserror` crate** — Library error types via derive macros.
  - Documentation: <https://docs.rs/thiserror>
- **`criterion` crate** — Statistics-driven micro-benchmarks for Rust.
  - Documentation: <https://docs.rs/criterion>
- **`proptest` crate** — Property-based testing for Rust.
  - Documentation: <https://docs.rs/proptest>

## Security and dependency auditing

- **`cargo-deny`** — License and dependency-graph auditing.
  - Repository: <https://github.com/EmbarkStudios/cargo-deny>
- **`cargo-audit`** — Vulnerability database lookup.
  - Repository: <https://github.com/rustsec/rustsec>
- **`cargo-tarpaulin`** — Code coverage reporting.
  - Repository: <https://github.com/xd009642/tarpaulin>

## Bitcoin and secp256k1

- **Bitcoin developer reference** — secp256k1 is the curve used by Bitcoin and many other cryptocurrencies.
  - <https://developer.bitcoin.org/reference/block_chain.html#secp256k1-the-bitcoin-curve>
- **`libsecp256k1`** (C reference implementation) — Informative for understanding the curve, though this project does not depend on it.
  - <https://github.com/bitcoin-core/secp256k1>

## Educational reading

- *Programming Bitcoin* — Jimmy Song (O'Reilly, 2019). An accessible introduction to elliptic curve cryptography as used in Bitcoin, with secp256k1 examples.
- *Mastering Bitcoin* (2nd edition) — Andreas M. Antonopoulos (O'Reilly, 2017). Chapter 3 covers elliptic curve cryptography.
- *A Graduate Course in Applied Cryptography* — Dan Boneh and Victor Shoup. Chapter 15 covers elliptic curve cryptography in depth.
  - <https://toc.cryptobook.us/>

## Disclaimer

The references above are provided for educational context. The `find` tool is for research and educational use only. See [DISCLAIMER.md](../DISCLAIMER.md).
