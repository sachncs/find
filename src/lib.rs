// Copyright (c) 2026 Sachin (https://github.com/sachn-cs)
// Released under MIT OR Apache-2.0. See LICENSE-MIT or LICENSE-APACHE.
// THIS SOFTWARE IS FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.

//! # Secp256k1 Find Tool
//!
//! A high-performance Rust implementation of a multi-variant range-splitting
//! algorithm for secp256k1 private key discovery.
//!
//! The crate is organized into four layers:
//!
//! - [`ecc`] — Low-level elliptic-curve primitives (SEC1 parsing, scalar multiplication).
//! - [`search`] — Pure domain logic for variant generation and parallel sweeps.
//! - [`persistence`] — Atomic checkpoints, binary caches, and JSON exports.
//! - [`orchestrator`] — High-level session orchestration that wires the layers together.
//! - [`error`] — Unified error type used across all layers.

pub mod ecc;
pub mod error;
pub mod orchestrator;
pub mod persistence;
pub mod search;
