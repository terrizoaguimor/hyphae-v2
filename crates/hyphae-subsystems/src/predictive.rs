// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Predictive — forward model and prediction error.
//!
//! Cherry-picked from v1's `Cerebellum`. Maintains a forward model
//! (the current expectation about the upcoming input) and emits a
//! prediction-error fragment when an actual diverges from the
//! prediction.
//!
//! v1's ISSUE-M4c-2 documented that the prediction↔actual pairing
//! was loose — across interleaved encoding / recall flows an actual
//! could be compared against an unrelated prediction. v2 corrects
//! this from commit zero: predictions and actuals carry a
//! **cycle id**; the comparator only fires when the cycle ids
//! match. Mismatched pairs return the actual unchanged with no
//! error fragment (and a warning trace).
//!
//! Cycle ids are minted by the substrate's compose / recall / ingest
//! entry points and threaded through the fragment's
//! `provenance.source_pathway`. The encoding convention is
//! `"cycle/<id>:..."` — the subsystem reads the prefix to extract
//! the cycle id, and lets non-cycle-tagged input through without
//! pairing.

use hyphae_core::{
    CognitiveFragment, FragmentContent, FragmentId, PayloadKind, Provenance, Result, State,
    Subsystem, SubsystemId,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::time::SystemTime;

/// A pending prediction waiting to be paired with an actual.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingPrediction {
    /// The fragment id of the prediction itself.
    fragment_id: FragmentId,
    /// The predicted valence (the dimension currently compared
    /// against the actual). Future expansions can compare more
    /// dimensions; v0.1 sticks with the valence axis to keep the
    /// error semantics audit-grep simple.
    predicted_valence: f32,
    /// Confidence the prediction was emitted with. Propagated to
    /// the error fragment as the `confabulation_risk` floor.
    confabulation_risk: f32,
}

/// Snapshot of [`Predictive`] state. Drops `pending` per the v1 M5
/// invariant — a cycle's pending prediction must not survive a
/// checkpoint / restart, because the actual will not arrive in the
/// new process.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PredictiveSnapshot {
    predictions_made: u64,
    errors_emitted: u64,
}

/// Predictive subsystem.
#[derive(Debug, Default)]
pub struct Predictive {
    /// Pending predictions keyed by cycle id. Each cycle holds at
    /// most one prediction — a fresh prediction in the same cycle
    /// overwrites the prior one (the v1 convention: latest wins
    /// because the latest is the freshest expectation).
    pending: HashMap<String, PendingPrediction>,
    /// Lifetime predictions emitted.
    predictions_made: u64,
    /// Lifetime prediction errors emitted.
    errors_emitted: u64,
}

impl Predictive {
    /// Construct a predictive subsystem with no pending predictions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lifetime predictions made.
    #[must_use]
    pub fn predictions_made(&self) -> u64 {
        self.predictions_made
    }

    /// Lifetime errors emitted.
    #[must_use]
    pub fn errors_emitted(&self) -> u64 {
        self.errors_emitted
    }

    /// Number of pending predictions (cycles awaiting an actual).
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Extract the cycle id from a fragment's
    /// `provenance.source_pathway`. The substrate's encoding
    /// convention is `"cycle/<id>:..."`. Returns `None` for
    /// fragments that did not enter a cycle (the subsystem then
    /// passes them through without pairing).
    fn cycle_id_of(fragment: &CognitiveFragment) -> Option<String> {
        let pathway = fragment.provenance.source_pathway.as_deref()?;
        pathway
            .strip_prefix("cycle/")
            .and_then(|tail| tail.split_once(':').map(|(cycle, _)| cycle.to_string()))
    }
}

impl Subsystem for Predictive {
    fn id(&self) -> SubsystemId {
        SubsystemId::Predictive
    }

    fn process(
        &mut self,
        fragment: CognitiveFragment,
        incoming: PayloadKind,
        _state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        let cycle_id = Self::cycle_id_of(&fragment);

        if incoming == PayloadKind::TopDownPrediction {
            // Register the prediction under its cycle id. If no
            // cycle id is supplied the substrate skipped the
            // cycle convention and the prediction is dropped
            // (logged) — pairing without a cycle id is exactly
            // the v1 looseness this subsystem corrects.
            if let Some(cycle) = cycle_id {
                let pending = PendingPrediction {
                    fragment_id: fragment.id,
                    predicted_valence: fragment.valence,
                    confabulation_risk: fragment.provenance.confabulation_risk,
                };
                self.pending.insert(cycle.clone(), pending);
                self.predictions_made += 1;
                tracing::trace!("predictive: registered prediction for cycle {cycle}");
            } else {
                tracing::warn!(
                    "predictive: top-down prediction missing cycle id, dropped \
                         (the v1 ISSUE-M4c-2 loose-pairing pattern)"
                );
            }
            Ok(vec![fragment])
        } else {
            // Treat anything else as a potential actual. If we
            // have a pending prediction in this cycle, fire an
            // error.
            let Some(cycle) = cycle_id else {
                return Ok(vec![fragment]);
            };
            let Some(pending) = self.pending.remove(&cycle) else {
                return Ok(vec![fragment]);
            };
            let error_value = fragment.valence - pending.predicted_valence;
            self.errors_emitted += 1;
            let error_fragment = CognitiveFragment {
                id: FragmentId::new(),
                content: FragmentContent::Reflection {
                    body: format!(
                        "prediction_error cycle={cycle} ε={error_value:+.4} \
                             predicted={:+.4} actual={:+.4}",
                        pending.predicted_valence, fragment.valence
                    ),
                    about: vec![pending.fragment_id, fragment.id],
                },
                created_at: SystemTime::now(),
                last_accessed_at: SystemTime::now(),
                // Magnitude of the error drives downstream
                // saliency — large errors are surprising and
                // warrant the composer's attention.
                saliency: error_value.abs().clamp(0.0, 1.0),
                valence: error_value.clamp(-1.0, 1.0),
                decay_rate: fragment.decay_rate,
                confidence: fragment.confidence,
                provenance: Provenance {
                    source_subsystem: "predictive".to_string(),
                    source_pathway: Some(format!("cycle/{cycle}:error")),
                    parent_ids: vec![pending.fragment_id, fragment.id],
                    // Measurement emitter — risk floor is the
                    // higher of the two contributing risks. Per
                    // ADR-0002, measurement emitters that
                    // derive from multiple inputs propagate the
                    // worst case, not zero.
                    confabulation_risk: pending
                        .confabulation_risk
                        .max(fragment.provenance.confabulation_risk),
                    namespace: fragment.provenance.namespace.clone(),
                },
                embedding: None,
                depth_level: fragment.depth_level,
                domain_tags: Vec::new(),
                language: fragment.language.clone(),
                boundary_metadata: None,
            };
            Ok(vec![fragment, error_fragment])
        }
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = PredictiveSnapshot {
            predictions_made: self.predictions_made,
            errors_emitted: self.errors_emitted,
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("predictive checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: PredictiveSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("predictive restore deserialise: {e}"))
        })?;
        self.predictions_made = snap.predictions_made;
        self.errors_emitted = snap.errors_emitted;
        // Per the v1 M5 invariant: do not restore pending
        // predictions across a restart. The actual that would have
        // paired with them is not coming.
        self.pending.clear();
        Ok(())
    }

    fn on_state_change(&mut self, _old: State, new: State) -> Result<()> {
        // Recovery wipes all pending predictions for the same
        // reason as `restore` — a crash invalidates any pairing
        // that was in flight.
        if new == State::Recovery {
            self.pending.clear();
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

    fn cycle_fragment(cycle: &str, role: &str, valence: f32) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: format!("cycle={cycle} role={role} v={valence}"),
            },
            "test",
        );
        f.valence = valence;
        f.provenance.source_pathway = Some(format!("cycle/{cycle}:{role}"));
        f
    }

    #[test]
    fn prediction_without_cycle_id_is_dropped() {
        let mut p = Predictive::new();
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: "x".to_string(),
            },
            "test",
        );
        f.valence = 0.5;
        // No cycle id on the pathway → prediction is dropped.
        f.provenance.source_pathway = Some("not-a-cycle-format".to_string());
        let _ = p
            .process(f, PayloadKind::TopDownPrediction, State::Encoding)
            .unwrap();
        assert_eq!(p.predictions_made(), 0);
        assert_eq!(p.pending_count(), 0);
    }

    #[test]
    fn prediction_followed_by_matching_actual_fires_error() {
        let mut p = Predictive::new();
        let pred = cycle_fragment("c1", "prediction", 0.4);
        let _ = p
            .process(pred, PayloadKind::TopDownPrediction, State::Encoding)
            .unwrap();
        assert_eq!(p.pending_count(), 1);

        let actual = cycle_fragment("c1", "actual", 0.7);
        let out = p
            .process(actual, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert_eq!(out.len(), 2, "actual + error fragment");
        let err = &out[1];
        // ε = actual − predicted = 0.7 − 0.4 = 0.3
        assert!((err.valence - 0.3).abs() < 1e-5);
        assert_eq!(p.pending_count(), 0, "pairing consumed the prediction");
    }

    #[test]
    fn actual_from_different_cycle_does_not_pair() {
        let mut p = Predictive::new();
        let pred = cycle_fragment("c1", "prediction", 0.5);
        let _ = p
            .process(pred, PayloadKind::TopDownPrediction, State::Encoding)
            .unwrap();

        // Actual from cycle c2: no pairing, no error fragment.
        let actual = cycle_fragment("c2", "actual", 0.8);
        let out = p
            .process(actual, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert_eq!(out.len(), 1, "no error emitted across cycles");
        assert_eq!(
            p.pending_count(),
            1,
            "the c1 prediction must still be pending",
        );
    }

    #[test]
    fn same_cycle_double_prediction_overwrites_pending() {
        let mut p = Predictive::new();
        let _ = p
            .process(
                cycle_fragment("c1", "prediction", 0.2),
                PayloadKind::TopDownPrediction,
                State::Encoding,
            )
            .unwrap();
        let _ = p
            .process(
                cycle_fragment("c1", "prediction", 0.6),
                PayloadKind::TopDownPrediction,
                State::Encoding,
            )
            .unwrap();
        assert_eq!(p.pending_count(), 1, "second prediction supersedes first");
        let actual = cycle_fragment("c1", "actual", 0.6);
        let out = p
            .process(actual, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        // The fresh prediction (0.6) is what gets compared.
        let err = &out[1];
        assert!(err.valence.abs() < 1e-5);
    }

    #[test]
    fn error_fragment_propagates_max_confabulation_risk() {
        let mut p = Predictive::new();
        let mut pred = cycle_fragment("c1", "prediction", 0.3);
        pred.provenance.confabulation_risk = 0.4;
        let _ = p
            .process(pred, PayloadKind::TopDownPrediction, State::Encoding)
            .unwrap();
        let mut actual = cycle_fragment("c1", "actual", 0.5);
        actual.provenance.confabulation_risk = 0.6;
        let out = p
            .process(actual, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!((out[1].provenance.confabulation_risk - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn recovery_wipes_pending() {
        let mut p = Predictive::new();
        let _ = p
            .process(
                cycle_fragment("c1", "prediction", 0.5),
                PayloadKind::TopDownPrediction,
                State::Encoding,
            )
            .unwrap();
        assert_eq!(p.pending_count(), 1);
        p.on_state_change(State::Encoding, State::Recovery).unwrap();
        assert_eq!(p.pending_count(), 0);
    }

    #[test]
    fn restore_clears_pending() {
        let mut p = Predictive::new();
        let _ = p
            .process(
                cycle_fragment("c1", "prediction", 0.5),
                PayloadKind::TopDownPrediction,
                State::Encoding,
            )
            .unwrap();
        let bytes = p.checkpoint().unwrap();
        let mut restored = Predictive::new();
        // Seed a fake pending so we can verify `restore` wipes it.
        restored.pending.insert(
            "stale".to_string(),
            PendingPrediction {
                fragment_id: FragmentId::new(),
                predicted_valence: 0.0,
                confabulation_risk: 0.0,
            },
        );
        restored.restore(&bytes).unwrap();
        assert_eq!(restored.pending_count(), 0);
        assert_eq!(restored.predictions_made(), p.predictions_made());
    }
}
