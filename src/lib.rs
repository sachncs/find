// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT. See LICENSE-MIT.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! # Secp256k1 Find Tool
//!
//! A high-performance Rust implementation of a multi-variant range-splitting
//! algorithm for secp256k1 private key discovery. **This software is for
//! educational and research purposes only**; it is not constant-time and
//! must not be used for production signing or verification.
//!
//! The crate is organized into seven layers:
//!
//! - [`ecc`] — Low-level elliptic-curve primitives (SEC1 parsing, scalar multiplication).
//! - [`search`] — Pure domain logic for variant generation and parallel sweeps.
//! - [`persistence`] — Atomic checkpoints, binary caches, and JSON exports.
//! - [`config`] — Session configuration types and validation.
//! - [`telemetry`] — Tracing initialization helpers.
//! - [`orchestrator`] — High-level session orchestration that wires the layers together.
//! - [`error`] — Unified error type used across all layers.
//!
//! For the mathematical background, see
//! [`docs/algorithms.md`](../docs/algorithms.md). For design decisions and
//! trade-offs, see the [architecture decision records](../docs/adr/).

#![warn(missing_docs)]

pub mod config;
pub mod ecc;
pub mod error;
pub mod orchestrator;
pub mod persistence;
pub mod search;
pub mod telemetry;
