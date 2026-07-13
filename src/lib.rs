// Copyright (c) 2026 Sachin (https://github.com/sachncs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! # Secp256k1 Find Tool
//!
//! A high-performance Rust implementation of a multi-variant range-splitting
//! algorithm for secp256k1 private key discovery. **This software is for
//! educational and research purposes only**; it is not constant-time and
//! must not be used for production signing or verification.
//!
//! The crate is organized into seven layers, each with a single well-defined
//! responsibility. The dependency graph below shows how data and control
//! flow between them:
//!
//! ```mermaid
//! graph TD
//!     main[main.rs] --> orchestrator
//!     main --> config[config::Config]
//!     main --> telemetry[telemetry::init_tracing]
//!     main --> search[search::SearchMatch]
//!     lib[lib.rs] --> config
//!     lib --> telemetry
//!     lib --> ecc
//!     lib --> error
//!     lib --> search
//!     lib --> orchestrator
//!     lib --> persistence
//!     orchestrator --> config
//!     orchestrator --> ecc
//!     orchestrator --> error
//!     orchestrator --> search
//!     orchestrator --> persistence
//!     persistence --> ecc
//!     persistence --> error
//!     persistence --> search[search::CacheWriter<br/>search::OffsetVariant<br/>search::SearchMatch<br/>search::VariantIndex]
//!     search --> ecc
//!     search --> error
//!     ecc --> error
//!     telemetry --> error
//!     config --> error
//! ```
//!
//! - [`ecc`] — Low-level elliptic-curve primitives (SEC1 parsing, scalar multiplication).
//!   Depends only on [`error`].
//! - [`search`] — Pure domain logic for variant generation and parallel sweeps.
//!   Depends on [`ecc`] and [`error`]; contains no file I/O.
//!   See [ADR-0005](../docs/adr/0005-pure-search-module.md).
//! - [`persistence`] — Atomic checkpoints, binary caches, and JSON exports.
//!   The only module that performs file I/O; implements
//!   [`search::CacheWriter`].
//! - [`config`] — Session configuration types and validation.
//! - [`telemetry`] — Tracing initialization helpers.
//! - [`orchestrator`] — High-level session orchestration that wires the
//!   layers together. Owns the lifecycle loop.
//! - [`error`] — Unified [`error::FindError`] type used across all layers.
//!   Has no internal dependencies; it is the leaf of the dependency graph.
//!
//! # Quick start
//!
//! ```no_run
//! use find::config::Config;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config::new(
//!         "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
//!         "data",
//!         false,
//!     );
//!     let match_ = find::orchestrator::run(&config)?;
//!     if let Some(m) = match_ {
//!         println!("Found candidates: {:?}", m.candidates);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Thread safety
//!
//! Every public type in this crate is either:
//!
//! - **`Send + Sync`** because it is immutable after construction
//!   ([`search::VariantIndex`], [`search::OffsetVariant`], [`config::Config`]),
//! - or explicitly synchronised via atomics or mutexes
//!   ([`search::Progress`] uses [`AtomicU64`]; [`search::CacheWriter`]
//!   requires `Send + Sync`; [`persistence::FileCacheWriter`] guards its
//!   file handle with a [`std::sync::Mutex`]).
//!
//! The orchestrator entry point [`orchestrator::run`] is safe to call from a
//! single thread; it spawns its own Rayon worker pool internally.
//!
//! # Side-channel stance
//!
//! All elliptic-curve operations in [`ecc`] and [`search`] are
//! **not constant-time**. Scalar multiplication, modular inversion, and
//! point comparison all leak timing information. The crate must not be used
//! for signing or verifying messages where side-channel resistance is
//! required. See [`docs/security.md`](../docs/security.md) for the full
//! threat model.
//!
//! For the mathematical background, see
//! [`docs/algorithms.md`](../docs/algorithms.md). For design decisions and
//! trade-offs, see the [architecture decision records](../docs/adr/).
//!
//! [`AtomicU64`]: std::sync::atomic::AtomicU64

#![warn(missing_docs)]

pub mod config;
pub mod ecc;
pub mod error;
pub mod orchestrator;
pub mod persistence;
pub mod search;
pub mod telemetry;
