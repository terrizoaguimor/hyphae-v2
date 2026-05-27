// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-core
//!
//! Core types and traits for the Hyphae v2 cognitive substrate.
//!
//! This crate defines the foundational primitives:
//! - [`CognitiveFragment`]: the atomic unit of cognitive content
//! - [`Pathway`]: the typed channel between subsystems
//! - [`Subsystem`]: the trait every functional subsystem implements
//! - [`State`]: the five global states of the substrate
//! - [`PayloadKind`]: prediction / error / modulation labels for the
//!   Active Inference engagement
//!
//! See `docs/rfc/v1-living.md` for the architectural specification.
//! Cherry-picked from v1 `hyphae-core` per ADR-0001; v2 corrections
//! baked in from commit zero (`FragmentId` uniqueness, [`Subsystem`]
//! trait carrying `incoming: PayloadKind`, six functional subsystem
//! identifiers).

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

pub mod actor;
pub mod cascade;
pub mod error;
pub mod fragment;
pub mod pathway;
pub mod state;
pub mod subsystem;
pub mod thread;

pub use actor::ActorContext;
pub use cascade::{
    ActivationLevel, CascadeActivation, CascadeParams, CascadeRetrieval, ConductivityGraph,
    ConductivityWeight, ConflictDetectionMode, ConflictRationale, ConflictReport, TemporalContext,
    WallClockTemporalContext,
};
pub use error::{HyphaeError, Result};
pub use fragment::{
    BoundaryMetadata, CognitiveFragment, ExternalInputPayload, FragmentContent, Gender,
    JournalEntryType, LanguageTag, Number, PoS, Provenance,
};
pub use pathway::{DirectPathway, Pathway, PathwayId, PayloadKind};
pub use state::State;
pub use subsystem::{ConsolidationGateSignal, Subsystem, SubsystemId};
pub use thread::{
    ContextAwareComposition, ConversationContext, ConversationEvent, ConversationEventKind,
    ConversationThread, KeywordExtractor, MetacognitivePrelude, OpenQuestion, PendingFollowup,
    StopwordFilteringExtractor, ThreadId, ThreadState, TopicSwitchOutcome,
};

/// Dimensionality of cognitive fragment embedding vectors.
///
/// The `episodic` subsystem projects fragment content into a vector
/// of this width and indexes queries of the same width. CPU-only;
/// no GPU dependency.
pub const EMBEDDING_DIM: usize = 256;

/// Process-monotonic counter for the low 8 bytes of a [`FragmentId`].
static FRAGMENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-process seed for the high 8 bytes of a [`FragmentId`], derived
/// once from the wall clock at first use. Distinguishes identifiers
/// minted by different runs so they cannot collide with identifiers
/// persisted by an earlier run. v2 ships this from commit zero — v1
/// shipped a clock-derived id that collided for fragments minted in
/// the same nanosecond (fixed in v1's M4a-1).
static FRAGMENT_SEED: LazyLock<u64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| {
            d.as_secs()
                .wrapping_mul(1_000_000_000)
                .wrapping_add(u64::from(d.subsec_nanos()))
        })
});

/// Unique identifier for a cognitive fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FragmentId(pub [u8; 16]);

impl FragmentId {
    /// Generate a new unique fragment identifier.
    ///
    /// The low 8 bytes are a process-monotonic counter; the high
    /// 8 bytes are a per-process seed. Identifiers are therefore
    /// unique both within a run and across restarts.
    #[must_use]
    pub fn new() -> Self {
        let count = FRAGMENT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&count.to_le_bytes());
        bytes[8..].copy_from_slice(&FRAGMENT_SEED.to_le_bytes());
        Self(bytes)
    }
}

impl Default for FragmentId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_ids_are_unique_within_a_run() {
        let a = FragmentId::new();
        let b = FragmentId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn fragment_id_low_bytes_are_monotonic() {
        let a = FragmentId::new();
        let b = FragmentId::new();
        let a_lo = u64::from_le_bytes(a.0[..8].try_into().unwrap());
        let b_lo = u64::from_le_bytes(b.0[..8].try_into().unwrap());
        assert!(b_lo > a_lo);
    }
}
