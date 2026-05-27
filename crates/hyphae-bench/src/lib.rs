// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-bench
//!
//! Performance baseline bench harness for Hyphae v2 — per
//! ADR-0015.
//!
//! Source files live under `benches/`. This crate has no public
//! API surface; it exists to host the criterion bench target with
//! the workspace's dependency graph available.

#![warn(missing_docs)]
