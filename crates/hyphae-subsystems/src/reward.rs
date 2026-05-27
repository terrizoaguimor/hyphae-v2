// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Reward — signed prediction error and expected-valence low-pass.
//!
//! Cherry-picked from v1's `DopaminergicMidbrain`. Computes the
//! signed reward prediction error
//!
//! ```text
//! δ = actual_valence − expected_valence
//! expected_valence ← α · actual + (1 − α) · expected
//! ```
//!
//! and emits a `BottomUpPredictionError` fragment whose `valence`
//! field carries `δ`. The fragment is one of the two feedback
//! channels the learning loop consumes (ADR-0002 §"Feedback
//! signals").
//!
//! Habituation **emerges from the low-pass**: a sustained run of
//! similarly-valenced inputs raises `expected_valence` toward those
//! inputs, shrinking subsequent RPEs. There is no separate
//! habituation state.
//!
//! Per ADR-0002, the `confabulation_risk` of the emitted error
//! fragment **propagates from the source** (the actual whose
//! valence was compared) — the RPE is a single-input
//! transformation, not a measurement emitter.

use hyphae_core::{
    CognitiveFragment, FragmentContent, FragmentId, PayloadKind, Provenance, Result, State,
    Subsystem, SubsystemId,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::time::SystemTime;

/// EMA coefficient on the expected-valence low-pass. Lower = more
/// inertia (slower habituation); higher = faster update. v1's M9
/// chose `0.2`; v2 inherits.
pub const EXPECTED_ALPHA: f32 = 0.2;

/// Snapshot of [`Reward`] state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RewardSnapshot {
    expected_valence: f32,
    rpe_emissions: u64,
}

/// Reward subsystem.
#[derive(Debug, Default)]
pub struct Reward {
    /// Running low-pass of the actual valences this subsystem has
    /// processed. Drives the RPE magnitude.
    expected_valence: f32,
    /// Lifetime count of RPE emissions.
    rpe_emissions: u64,
}

impl Reward {
    /// Construct a reward subsystem at neutral baseline (expected
    /// valence = 0).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current running expected valence in `[-1.0, +1.0]`.
    #[must_use]
    pub fn expected_valence(&self) -> f32 {
        self.expected_valence
    }

    /// Lifetime RPE emissions.
    #[must_use]
    pub fn rpe_emissions(&self) -> u64 {
        self.rpe_emissions
    }
}

impl Subsystem for Reward {
    fn id(&self) -> SubsystemId {
        SubsystemId::Reward
    }

    fn process(
        &mut self,
        fragment: CognitiveFragment,
        _incoming: PayloadKind,
        _state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        // RPE = actual − expected. The convex combination preserves
        // the [-1, +1] invariant of `expected_valence` (proof: if
        // both inputs ∈ [-1, +1] and α ∈ (0, 1), the weighted
        // average stays in [-1, +1]).
        let actual = fragment.valence;
        let rpe = actual - self.expected_valence;
        self.expected_valence =
            EXPECTED_ALPHA.mul_add(actual, (1.0 - EXPECTED_ALPHA) * self.expected_valence);
        self.rpe_emissions += 1;

        // Emit the RPE as a new fragment carrying δ on its valence
        // axis. The source's confabulation_risk propagates (ADR-0002
        // §"Confabulation risk discipline" — single-input
        // transformer).
        let body = format!(
            "RPE δ={rpe:+.4} actual={actual:+.4} expected={:+.4}",
            self.expected_valence
        );
        let mut rpe_fragment = CognitiveFragment {
            id: FragmentId::new(),
            content: FragmentContent::Reflection {
                body,
                about: vec![fragment.id],
            },
            created_at: SystemTime::now(),
            last_accessed_at: SystemTime::now(),
            saliency: rpe.abs().clamp(0.0, 1.0),
            valence: rpe.clamp(-1.0, 1.0),
            decay_rate: fragment.decay_rate,
            confidence: fragment.confidence,
            provenance: Provenance {
                source_subsystem: "reward".to_string(),
                source_pathway: None,
                parent_ids: vec![fragment.id],
                // Single-input transformer: propagate the source's
                // risk. Per ADR-0002 §"Confabulation risk discipline".
                confabulation_risk: fragment.provenance.confabulation_risk,
                namespace: fragment.provenance.namespace.clone(),
            },
            embedding: None,
            depth_level: fragment.depth_level,
            domain_tags: fragment.domain_tags.clone(),
            language: fragment.language.clone(),
            boundary_metadata: None,
        };
        // The original input is also part of the emission so the
        // downstream subsystem (composer, typically) sees both the
        // RPE and the input that produced it.
        let original = fragment;
        rpe_fragment.provenance.source_pathway = Some("reward.rpe".to_string());
        Ok(vec![original, rpe_fragment])
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = RewardSnapshot {
            expected_valence: self.expected_valence,
            rpe_emissions: self.rpe_emissions,
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("reward checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: RewardSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("reward restore deserialise: {e}"))
        })?;
        self.expected_valence = snap.expected_valence;
        self.rpe_emissions = snap.rpe_emissions;
        Ok(())
    }

    fn on_state_change(&mut self, _old: State, new: State) -> Result<()> {
        // Recovery wipes the running expectation — the context
        // before the crash is no longer authoritative.
        if new == State::Recovery {
            self.expected_valence = 0.0;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag_with_valence(v: f32) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: format!("v={v}"),
            },
            "test",
        );
        f.valence = v;
        f
    }

    #[test]
    fn first_input_produces_rpe_equal_to_its_valence() {
        let mut r = Reward::new();
        let out = r
            .process(
                frag_with_valence(0.7),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        assert_eq!(out.len(), 2, "original + RPE fragment");
        let rpe = &out[1];
        assert!((rpe.valence - 0.7).abs() < 1e-6);
    }

    #[test]
    fn habituation_shrinks_rpe_under_sustained_input() {
        let mut r = Reward::new();
        let mut last_rpe = f32::NAN;
        for _ in 0..20 {
            let out = r
                .process(
                    frag_with_valence(0.6),
                    PayloadKind::Encoding,
                    State::Encoding,
                )
                .unwrap();
            last_rpe = out[1].valence;
        }
        // After 20 sustained inputs the expectation should have
        // tracked toward 0.6 and the RPE should be tiny.
        assert!(
            last_rpe.abs() < 0.05,
            "expected habituation; got {last_rpe}"
        );
    }

    #[test]
    fn negative_input_after_positive_run_produces_negative_rpe() {
        let mut r = Reward::new();
        for _ in 0..10 {
            r.process(
                frag_with_valence(0.6),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        let out = r
            .process(
                frag_with_valence(-0.4),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        assert!(
            out[1].valence < 0.0,
            "negative input after positive run must yield negative RPE",
        );
    }

    #[test]
    fn expected_valence_stays_in_unit_range() {
        let mut r = Reward::new();
        for _ in 0..100 {
            r.process(
                frag_with_valence(1.0),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert!(r.expected_valence() <= 1.0);
        for _ in 0..100 {
            r.process(
                frag_with_valence(-1.0),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert!(r.expected_valence() >= -1.0);
    }

    #[test]
    fn rpe_fragment_propagates_source_confabulation_risk() {
        let mut r = Reward::new();
        let mut frag = frag_with_valence(0.5);
        frag.provenance.confabulation_risk = 0.4;
        let out = r
            .process(frag, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!((out[1].provenance.confabulation_risk - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn recovery_wipes_expected_valence() {
        let mut r = Reward::new();
        for _ in 0..5 {
            r.process(
                frag_with_valence(0.8),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert!(r.expected_valence() > 0.0);
        r.on_state_change(State::Encoding, State::Recovery).unwrap();
        assert!((r.expected_valence() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn checkpoint_restore_round_trips() {
        let mut r = Reward::new();
        for _ in 0..4 {
            r.process(
                frag_with_valence(0.3),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        let bytes = r.checkpoint().unwrap();
        let mut restored = Reward::new();
        restored.restore(&bytes).unwrap();
        assert!((restored.expected_valence() - r.expected_valence()).abs() < f32::EPSILON);
        assert_eq!(restored.rpe_emissions(), r.rpe_emissions());
    }
}
