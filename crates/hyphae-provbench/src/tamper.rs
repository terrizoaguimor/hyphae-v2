// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The tampering taxonomy.
//!
//! The minimal experiment (paper §5.2) crossed four modes. The
//! community-scale protocol broadens that to ten, spanning content
//! mutation, structural edits, replay, and freshness attacks. Each
//! mode is applied either *store-only* (the surface operation, leaving
//! the chain links stale) or *chain-aware* (recomputing every hash
//! forward and rewriting the head) depending on the adversary; see
//! [`crate::adversary`].

/// A post-ingest tampering operation against a system's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TamperMode {
    /// Overwrite an entry's body with different content.
    Edit,
    /// Drop an entry from the store.
    Delete,
    /// Forge a new entry and splice it in.
    Insert,
    /// Swap the bodies of two entries.
    Reorder,
    /// Flip a single byte of one entry's body.
    BitFlip,
    /// Cut an entry's body short.
    Truncate,
    /// Replay an entry's body into a new trailing slot.
    Duplicate,
    /// Skew one entry's timestamp.
    TimestampSkew,
    /// Revert the store to an earlier valid head (drop the tail).
    HeadRollback,
    /// Coordinated multi-entry edit.
    Batch,
}

impl TamperMode {
    /// Stable short name for tables/envelopes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TamperMode::Edit => "edit",
            TamperMode::Delete => "delete",
            TamperMode::Insert => "insert",
            TamperMode::Reorder => "reorder",
            TamperMode::BitFlip => "bitflip",
            TamperMode::Truncate => "truncate",
            TamperMode::Duplicate => "duplicate",
            TamperMode::TimestampSkew => "timestamp_skew",
            TamperMode::HeadRollback => "head_rollback",
            TamperMode::Batch => "batch",
        }
    }

    /// A stable index, mixed into the per-trial RNG so different modes
    /// pick different targets while staying reproducible.
    #[must_use]
    pub fn index(self) -> u64 {
        match self {
            TamperMode::Edit => 0,
            TamperMode::Delete => 1,
            TamperMode::Insert => 2,
            TamperMode::Reorder => 3,
            TamperMode::BitFlip => 4,
            TamperMode::Truncate => 5,
            TamperMode::Duplicate => 6,
            TamperMode::TimestampSkew => 7,
            TamperMode::HeadRollback => 8,
            TamperMode::Batch => 9,
        }
    }

    /// The full taxonomy, in table order.
    #[must_use]
    pub fn all() -> [TamperMode; 10] {
        [
            TamperMode::Edit,
            TamperMode::Delete,
            TamperMode::Insert,
            TamperMode::Reorder,
            TamperMode::BitFlip,
            TamperMode::Truncate,
            TamperMode::Duplicate,
            TamperMode::TimestampSkew,
            TamperMode::HeadRollback,
            TamperMode::Batch,
        ]
    }
}
