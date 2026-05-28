// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-storage
//!
//! Persistence layer for Hyphae v2:
//! - Transactional KV via `redb` for subsystem state and indexes.
//! - Append-only journal with SHA-256 hash chain via `fjall`.
//!
//! The hash chain provides auditability: every journal entry links
//! to the previous one via SHA-256 of its contents. Tampering with
//! history is detectable on verification.
//!
//! **One chain per substrate** (per `docs/adr/0003-ethics-radar-firstclass.md`
//! §8 "Why the shared hash chain"). The substrate journal and the
//! ethics audit log share this single chain — entries carry an
//! `event_kind` discriminant for grep-ability but they all link via
//! `prev_hash`. Causal ordering between memory ingest, journal
//! writes, ethics evaluations, and learning-loop parameter updates
//! is preserved by the totally-ordered chain. Tamper resistance is
//! one integrity surface, not two.

#![warn(missing_docs)]

pub mod anchor;
pub mod journal;
pub mod state_store;

pub use anchor::{AnchoredHead, HeadAnchor, verify_anchored_head};
pub use journal::{Journal, JournalEntry, JournalError};
pub use state_store::{StateStore, StateStoreError};
