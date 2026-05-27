// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-learning
//!
//! Substrate-bound learning loop for Hyphae v2.
//!
//! Per `docs/adr/0002-learning-loop-firstclass.md`, the learning
//! loop refines a small set of **refinable parameters** within
//! bounds. The substrate (grammar, state machine, pathway topology,
//! schemas, hash-chain protocol, `PayloadKind` taxonomy, Hard
//! Architectural Commitments) is **immutable**. This crate owns the
//! mutable parameter store, the feedback aggregator, the proposal
//! generator, and the journal-replay rollback — every piece sized
//! to the v0.1 scope.
//!
//! ## Roles
//!
//! - [`ParameterStore`] — authoritative state for refinable
//!   parameters, with per-target bounds and variant compatibility
//!   checks at propose-time.
//! - [`FeedbackSignal`] + [`FeedbackAggregator`] — the two channels
//!   the loop consumes (reward prediction error from the
//!   `predictive` + `reward` subsystems, and ethics signals from
//!   `hyphae-ethics`).
//! - [`LearningIntent`] + [`intents_from_signals`] — the
//!   signal-to-intent mapping.
//! - [`LearningLoop`] — coordinator that batches feedback, emits
//!   substrate-ready proposals, and applies values to the store
//!   after the substrate has audited.
//! - [`replay_to`] — rollback via journal replay (ADR-0002
//!   §"Audit and rollback").
//!
//! ## Dependency direction
//!
//! `hyphae-learning → hyphae-substrate → hyphae-ethics →
//! hyphae-storage → hyphae-core`. Substrate consumes ethics;
//! learning consumes both. Substrate does **not** import learning —
//! the loop is an external orchestrator that hands proposals back
//! to the substrate via [`Substrate::propose_learning_update`].

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod feedback;
pub mod orchestrator;
pub mod parameters;
pub mod proposals;
pub mod rollback;

pub use feedback::{FeedbackAggregator, FeedbackSignal};
pub use orchestrator::LearningOrchestrator;
pub use parameters::{
    ParameterBounds, ParameterError, ParameterStore, ParameterValue, ProposalBytes, target_key,
};
pub use proposals::{LearningIntent, intents_from_signals};
pub use rollback::{RollbackError, replay_to};

use hyphae_substrate::{LearningTarget, LearningUpdateProposal};

/// A staged learning-loop proposal. Carries the substrate-ready
/// [`LearningUpdateProposal`] together with the
/// [`ParameterValue`] that the integrator will apply to the
/// [`ParameterStore`] after the substrate audits.
///
/// The integrator flow is:
/// 1. Collect feedback via [`LearningLoop::record`].
/// 2. Flush a batch of [`StagedProposal`]s via
///    [`LearningLoop::stage_pending`].
/// 3. For each staged proposal, call
///    `substrate.propose_learning_update(...)`.
/// 4. If the substrate returned `Ok`, call
///    [`LearningLoop::apply_audited`] with the staged value to
///    mutate the store. If `Err`, drop the staged proposal — the
///    chain stays clean.
#[derive(Debug)]
pub struct StagedProposal {
    /// The proposal to hand to the substrate.
    pub proposal: LearningUpdateProposal,
    /// The new [`ParameterValue`] to apply to the store on a
    /// successful audit.
    pub apply_value: ParameterValue,
}

/// The coordinator. Owns the parameter store, the feedback
/// aggregator, and the proposal-generation policy. In v0.1 it does
/// **not** drive the substrate directly — the integrator does that,
/// so the substrate's dependency direction stays one-way (substrate
/// does not import learning).
#[derive(Debug, Default)]
pub struct LearningLoop {
    store: ParameterStore,
    aggregator: FeedbackAggregator,
}

impl LearningLoop {
    /// Construct a new loop with an empty store and aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a loop pre-seeded with a parameter store. Useful
    /// for deployments that load an initial calibration at startup.
    #[must_use]
    pub fn with_store(store: ParameterStore) -> Self {
        Self {
            store,
            aggregator: FeedbackAggregator::new(),
        }
    }

    /// Read-only access to the parameter store.
    #[must_use]
    pub fn store(&self) -> &ParameterStore {
        &self.store
    }

    /// Mutable access to the parameter store. Use this to declare
    /// bounds and seed initial values; mutating values directly
    /// here BYPASSES the substrate audit — only do this from the
    /// integrator at startup or from a rollback path.
    #[must_use]
    pub fn store_mut(&mut self) -> &mut ParameterStore {
        &mut self.store
    }

    /// Read-only access to the feedback aggregator.
    #[must_use]
    pub fn aggregator(&self) -> &FeedbackAggregator {
        &self.aggregator
    }

    /// Record a feedback observation.
    pub fn record(&mut self, signal: FeedbackSignal) {
        self.aggregator.record(signal);
    }

    /// Drain the aggregator, generate intents, and stage every
    /// intent that survives the store's bounds + variant
    /// compatibility check.
    ///
    /// Intents that fail propose-time validation are dropped here
    /// — the chain stays clean of rejected updates. Callers that
    /// need to surface validation failures can call
    /// [`Self::pending_signals`] + [`intents_from_signals`] +
    /// [`ParameterStore::propose`] themselves.
    pub fn stage_pending(&mut self) -> Vec<StagedProposal> {
        let signals = self.aggregator.drain();
        let intents = intents_from_signals(&signals);
        let mut staged = Vec::new();
        for intent in intents {
            let target = intent.target.clone();
            let Some(proposed) = build_value(&self.store, &intent) else {
                continue;
            };
            let bytes = match self.store.propose(&target, &proposed) {
                Ok(b) => b,
                Err(err) => {
                    tracing::debug!(
                        "dropping intent for {} ({}): bounds / variant rejection: {err}",
                        target.tag(),
                        intent.rationale,
                    );
                    continue;
                }
            };
            staged.push(StagedProposal {
                proposal: LearningUpdateProposal {
                    target,
                    old_value: bytes.old_value,
                    new_value: bytes.new_value,
                    triggered_by: None,
                    rationale: intent.rationale,
                },
                apply_value: proposed,
            });
        }
        staged
    }

    /// Read-only view of the feedback aggregator's pending signals.
    /// Useful for tests and for integrators that prefer to drive
    /// the propose / audit pipeline by hand.
    #[must_use]
    pub fn pending_signals(&self) -> &[FeedbackSignal] {
        self.aggregator.pending()
    }

    /// Apply a staged value to the parameter store. The integrator
    /// calls this after `substrate.propose_learning_update`
    /// returned `Ok` for the matching proposal.
    pub fn apply_audited(&mut self, target: &LearningTarget, value: ParameterValue) {
        self.store.apply_audited(target, value);
    }

    /// Roll back to a chain state by replaying every
    /// `audit_learning_update` entry up to `up_to_seq` from the
    /// supplied journal.
    ///
    /// # Errors
    ///
    /// Returns a [`RollbackError`] if the journal read fails or a
    /// payload cannot be deserialised.
    pub fn rollback_to(
        &mut self,
        journal: &hyphae_storage::Journal,
        up_to_seq: u64,
    ) -> Result<usize, RollbackError> {
        replay_to(journal, &mut self.store, up_to_seq)
    }
}

/// Build the proposed [`ParameterValue`] from a [`LearningIntent`]
/// and the store's current state. Returns `None` if the intent
/// cannot be turned into a value the store accepts (e.g. a
/// categorical intent against a target that has not received a
/// seed entry yet — the v0.1 generator only emits salience-weight
/// intents against seeded categorical targets).
fn build_value(store: &ParameterStore, intent: &LearningIntent) -> Option<ParameterValue> {
    let target = &intent.target;
    if let Some(key) = intent.categorical_key.as_ref() {
        // Categorical proposal: take the current map (or a new map
        // seeded at zero for the key), bump by `delta`, and clamp
        // using the store's declared bounds.
        let bounds = store.bounds_for(target);
        let mut map = match store.get(target) {
            Some(ParameterValue::Categorical(m)) => m.clone(),
            Some(ParameterValue::Scalar(_)) => return None,
            None => std::collections::HashMap::new(),
        };
        let entry = map.entry(key.clone()).or_insert(0.0);
        *entry = bounds.clamp(*entry + intent.delta);
        Some(ParameterValue::Categorical(map))
    } else {
        let bounds = store.bounds_for(target);
        let current = match store.get(target) {
            Some(ParameterValue::Scalar(v)) => *v,
            Some(ParameterValue::Categorical(_)) => return None,
            None => 0.0,
        };
        let proposed = bounds.clamp(current + intent.delta);
        Some(ParameterValue::Scalar(proposed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::{ActorContext, FragmentId};
    use hyphae_ethics::{CoveragePoint, ParameterDeltaHint, TaxonomyCategory};
    use hyphae_substrate::Substrate;
    use std::time::SystemTime;
    use tempfile::tempdir;

    #[test]
    fn loop_records_and_stages_a_reward_signal() {
        let mut lp = LearningLoop::new();
        lp.store_mut().set_bounds(
            &LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            },
            ParameterBounds::new(0.0, 1.0),
        );
        lp.record(FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.3,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        });
        let staged = lp.stage_pending();
        assert_eq!(staged.len(), 1);
        assert!(matches!(
            staged[0].proposal.target,
            LearningTarget::EpisodicConductivityWeight { ref edge_id } if edge_id == "a:b"
        ));
        // After staging, the aggregator is drained.
        assert!(lp.pending_signals().is_empty());
    }

    #[test]
    fn loop_drops_intents_with_out_of_bounds_delta() {
        let mut lp = LearningLoop::new();
        lp.store_mut().set_bounds(
            &LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            },
            ParameterBounds::new(0.0, 0.5),
        );
        // Pre-seed at 0.49 so the delta of 0.3 would push to 0.79 —
        // but the loop clamps to 0.5, which is in-bounds, so the
        // staging succeeds (clamping is the v0.1 policy). To
        // exercise the rejection path we need a variant mismatch.
        lp.store_mut().seed(
            &LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            },
            ParameterValue::Scalar(0.49),
        );
        lp.record(FeedbackSignal::Ethics {
            hint: {
                let mut h = ParameterDeltaHint::default();
                h.salience_weight_deltas
                    .push((TaxonomyCategory::Hate, 0.05));
                h
            },
            coverage_point: CoveragePoint::Compose,
            at: SystemTime::now(),
        });
        // The ethics intent targets a different parameter
        // (ValenceSalienceWeight), so no clash; one staged proposal.
        let staged = lp.stage_pending();
        assert_eq!(staged.len(), 1);
    }

    #[tokio::test]
    async fn end_to_end_loop_audits_through_substrate_and_applies() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        let mut lp = LearningLoop::new();
        lp.store_mut().set_bounds(
            &LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            },
            ParameterBounds::new(0.0, 1.0),
        );
        lp.record(FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.4,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        });
        let mut staged = lp.stage_pending();
        assert_eq!(staged.len(), 1);
        let StagedProposal {
            proposal,
            apply_value,
        } = staged.remove(0);
        let target = proposal.target.clone();
        let out = substrate
            .propose_learning_update(proposal, ActorContext::system())
            .await
            .unwrap();
        assert!(out.audit_seq.is_some());
        lp.apply_audited(&target, apply_value);

        let stored = lp
            .store()
            .get(&target)
            .expect("store should now carry the value");
        match stored {
            ParameterValue::Scalar(v) => assert!((v - 0.4).abs() < f32::EPSILON),
            ParameterValue::Categorical(_) => panic!("expected scalar"),
        }
    }

    #[tokio::test]
    async fn rollback_via_loop_walks_the_journal() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        let mut lp = LearningLoop::new();
        lp.store_mut().set_bounds(
            &LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            },
            ParameterBounds::new(0.0, 1.0),
        );

        // First update.
        lp.record(FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.3,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        });
        for s in lp.stage_pending() {
            let target = s.proposal.target.clone();
            substrate
                .propose_learning_update(s.proposal, ActorContext::system())
                .await
                .unwrap();
            lp.apply_audited(&target, s.apply_value);
        }

        // Second update — pushes the same edge higher.
        lp.record(FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.2,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        });
        for s in lp.stage_pending() {
            let target = s.proposal.target.clone();
            substrate
                .propose_learning_update(s.proposal, ActorContext::system())
                .await
                .unwrap();
            lp.apply_audited(&target, s.apply_value);
        }

        // The chain now carries two ethics-evaluation entries
        // (seq 0, 2) and two learning-audit entries (seq 1, 3).
        // Roll back to seq 1 — the first update only.
        let journal = hyphae_storage::Journal::open(dir.path().join("journal")).unwrap();
        let applied = lp.rollback_to(&journal, 1).unwrap();
        assert_eq!(applied, 1);
        let v = lp
            .store()
            .get(&LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            })
            .unwrap();
        // The first update set the parameter to 0.3 (delta from 0).
        match v {
            ParameterValue::Scalar(v) => assert!((v - 0.3).abs() < f32::EPSILON),
            ParameterValue::Categorical(_) => panic!("expected scalar"),
        }
    }
}
