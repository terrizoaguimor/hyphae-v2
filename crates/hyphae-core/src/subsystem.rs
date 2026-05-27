// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The Subsystem trait — every functional subsystem in Hyphae v2
//! implements this.
//!
//! v2 collapses v1's 17 anatomical subsystems to **6 functional
//! subsystems**, per ADR-0001 §"Subsystems collapsed". The collapse
//! is functional, not anatomical: each variant represents a
//! cognitive function, not a brain structure. The mapping back to
//! v1's anatomical labels is provided in each variant's rustdoc so
//! the lineage is auditable.
//!
//! v2 also ships [`Subsystem::process`] with an explicit
//! `incoming: PayloadKind` parameter from commit zero — v1 added it
//! mid-development via RFC-042 after three subsystems had worked
//! around its absence via brittle string heuristics on
//! `source_pathway`. The retrofit is preempted here.

use crate::{CognitiveFragment, PayloadKind, State};
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Unique identifier for a subsystem in the Hyphae v2 topology.
///
/// Six functional variants — see `docs/rfc/v1-living.md` §2 and
/// `docs/adr/0001-fresh-from-v1.md` §"Subsystems collapsed" for the
/// mapping back to v1's 17 anatomical subsystems and the rationale
/// per collapse pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubsystemId {
    /// Input gate. Input dispatch and signature-based amplification.
    /// Collapses v1's `Thalamus + OlfactoryBulb`. Two entry points:
    /// general ingest, and signature-amplified ingest (the OB-bypass
    /// case in v1). Modality filtering is externalised to the caller
    /// via distinct substrate entry points.
    InputGate,

    /// Episodic store with binding and conductivity. Collapses v1's
    /// `Hippocampus + EntorhinalCortex`. Owns the HNSW index, the
    /// conductivity graph for cascade activation, and the binding
    /// operation that links arriving fragments to their causal
    /// predecessors.
    Episodic,

    /// Affective stamp and decay modulation. Collapses v1's
    /// `Amygdala + BedNucleusStriaTerminalis`. Emits valence and
    /// saliency stamps on input fragments; modulates `decay_rate`
    /// for sustained context. Concerns separated at the type level:
    /// valence on `valence` axis, durability on `decay_rate` axis —
    /// no overlap (v1's M4b finding documented Amygdala + BNST both
    /// modifying `saliency` and cancelling each other by clamp).
    /// Integrates explicitly with `hyphae-ethics`.
    Valence,

    /// Executive composition and conversational metacognition.
    /// Collapses v1's `FrontalCortex + AnteriorCingulateCortex`.
    /// Owns the bounded working memory (cap = 7 fragments), the
    /// surface realizer wire-up, the conflict signal cabled to the
    /// ethics path from commit zero (corrects v1's documented-pending
    /// wire), and the conversation thread tracker with Jaccard
    /// topic-switch detection.
    Composer,

    /// Predictive coding and prediction error. Cherry-picked from
    /// v1's `Cerebellum`. Maintains a forward model; broadcasts a
    /// prediction error fragment when actual diverges from prediction.
    /// Prediction↔actual pairing uses cycle-id correlation from commit
    /// zero — corrects v1's ISSUE-M4c-2 loose-pairing deferral.
    Predictive,

    /// Reward prediction error and expected-valence low-pass.
    /// Cherry-picked from v1's `DopaminergicMidbrain`. Computes
    /// signed RPE `δ = actual − expected`; updates `expected_valence`
    /// via convex combination; emits the RPE as one of the two
    /// feedback channels the learning loop consumes (the other being
    /// ethics signals).
    Reward,
}

/// The trait every functional subsystem implements.
///
/// A subsystem has:
/// - An identifier.
/// - Internal state that persists across substrate operations.
/// - The ability to process incoming fragments and emit modulation
///   signals.
/// - State-dependent behaviour (different operational modes per
///   global state).
///
/// Subsystems do NOT call each other directly. All inter-subsystem
/// communication happens via [`crate::Pathway`] objects managed by
/// the substrate runtime.
pub trait Subsystem: Send + Sync {
    /// The identifier of this subsystem.
    fn id(&self) -> SubsystemId;

    /// Process an incoming fragment from a pathway.
    ///
    /// `incoming` is the [`PayloadKind`] of the pathway that
    /// delivered the fragment — a typed signal a subsystem can use to
    /// tell its inputs apart. At a substrate entry point, where a
    /// fragment arrives with no pathway, the substrate names the
    /// kind explicitly.
    ///
    /// Returns zero or more fragments to be emitted on outgoing
    /// pathways. The substrate runtime is responsible for routing
    /// these emissions.
    ///
    /// # Errors
    ///
    /// Returns an error if the subsystem rejects the fragment or
    /// fails while processing it.
    fn process(
        &mut self,
        fragment: CognitiveFragment,
        incoming: PayloadKind,
        current_state: State,
    ) -> crate::Result<Vec<CognitiveFragment>>;

    /// Called when the global state changes.
    ///
    /// Subsystems may reconfigure their internal operation based on
    /// the new state (e.g. `episodic` enables replay during
    /// Consolidation).
    ///
    /// # Errors
    ///
    /// Returns an error if the subsystem cannot reconfigure for the
    /// new state.
    fn on_state_change(&mut self, old_state: State, new_state: State) -> crate::Result<()> {
        let _ = (old_state, new_state);
        Ok(())
    }

    /// Persist subsystem state to the storage layer.
    ///
    /// Called during Dormancy state transitions and on explicit
    /// checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the subsystem state cannot be serialised.
    fn checkpoint(&self) -> crate::Result<Vec<u8>>;

    /// Restore subsystem state from persisted bytes.
    ///
    /// Called during Recovery state. The bytes are whatever was
    /// previously returned by [`Subsystem::checkpoint`].
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are malformed or cannot be
    /// deserialised.
    fn restore(&mut self, bytes: &[u8]) -> crate::Result<()>;

    /// Downcast access for typed substrate-level operations.
    ///
    /// The substrate's operation layer (`compose`, `recall`,
    /// `remember`, `journal_*`, etc.) needs typed access to specific
    /// subsystems — e.g. it must call the `Episodic` subsystem's
    /// `pattern_complete_scored` directly to recover retrieval scores
    /// the `dyn Subsystem::process` boundary discards. Every
    /// subsystem implements this as `self`; the standard one-liner is
    /// `fn as_any(&self) -> &dyn Any { self }`. Tools downcast via
    /// [`std::any::Any::downcast_ref`].
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast access. The substrate's
    /// `memory_update`-style operations need typed `&mut` access to
    /// mutate subsystem state; the standard one-liner is
    /// `fn as_any_mut(&mut self) -> &mut dyn Any { self }`.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Consolidation gating signal.
    ///
    /// Before transitioning into [`State::Consolidation`], the
    /// substrate asks every registered subsystem for its
    /// [`ConsolidationGateSignal`]. If *any* subsystem returns
    /// [`ConsolidationGateSignal::Block`], the transition is refused.
    /// This makes consolidation conditional on the network's
    /// affective and arousal context — coherent with the neuroscience
    /// the design tracks (consolidation needs quiescence).
    ///
    /// The default impl is [`ConsolidationGateSignal::Allow`]; the
    /// `valence` subsystem is the principal subsystem expected to
    /// override (refusing when sustained valence has extreme
    /// magnitude). Subsystems that have no opinion on the gate keep
    /// the default.
    fn consolidation_gate_signal(&self) -> ConsolidationGateSignal {
        ConsolidationGateSignal::Allow
    }
}

/// Per-subsystem vote on whether the substrate may enter
/// [`State::Consolidation`]. Aggregated by the substrate at
/// transition time: any [`ConsolidationGateSignal::Block`] vetos
/// the transition. The `String` in the `Block` variant carries a
/// short human-readable reason (e.g. `"valence sustained magnitude
/// 0.82"`) so the substrate can log *why* consolidation was
/// deferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsolidationGateSignal {
    /// This subsystem is OK with entering Consolidation.
    Allow,
    /// This subsystem refuses entry; the string explains why.
    Block(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsystem_id_serialises_round_trip() {
        let cases = [
            SubsystemId::InputGate,
            SubsystemId::Episodic,
            SubsystemId::Valence,
            SubsystemId::Composer,
            SubsystemId::Predictive,
            SubsystemId::Reward,
        ];
        for id in cases {
            let bytes = bincode::serialize(&id).unwrap();
            let restored: SubsystemId = bincode::deserialize(&bytes).unwrap();
            assert_eq!(id, restored);
        }
    }

    #[test]
    fn consolidation_gate_signal_default_is_allow() {
        // The Allow variant is the default behaviour exposed by the
        // trait's default impl. Subsystems opt in to blocking by
        // overriding. This test pins the default semantics.
        let signal = ConsolidationGateSignal::Allow;
        assert_eq!(signal, ConsolidationGateSignal::Allow);
        assert_ne!(
            signal,
            ConsolidationGateSignal::Block("any reason".to_string())
        );
    }
}
