// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Learning loop orchestration — per ADR-0013.
//!
//! Closes the three loops ADR-0002 mandated from v0.1 forward:
//!
//! 1. **Recording loop** — substrate operation terminals + ethics
//!    reports become [`FeedbackSignal`]s in the underlying
//!    [`LearningLoop`] aggregator.
//! 2. **Proposal loop** — aggregated signals become
//!    [`LearningUpdateProposal`]s and flow through the substrate's
//!    [`Substrate::propose_learning_update`] audit pipeline.
//! 3. **Application loop** — accepted proposals mutate the
//!    [`ParameterStore`]; the ethics signal from the audit feeds
//!    back into the next recording batch.
//!
//! ## Direction-preserving wiring
//!
//! Substrate does **not** import learning (per ADR-0002 §"Dependency
//! direction"). The orchestrator lives in this crate, which already
//! imports `hyphae-substrate`. The integrator (smoke binary, CLI)
//! owns one orchestrator alongside one substrate and drives both
//! explicitly. No hidden threads, no implicit polling.

use crate::feedback::FeedbackSignal;
use crate::{LearningLoop, StagedProposal};
use hyphae_core::{ActorContext, CognitiveFragment};
use hyphae_ethics::EthicsReport;
use hyphae_substrate::{LearningUpdateOutput, Substrate, SubstrateError};
use std::time::SystemTime;

/// Coordinates the v0.1 learning loop against a [`Substrate`]
/// instance. Owns a [`LearningLoop`] internally; observes substrate
/// emissions; drains and forwards proposals; applies audited values.
#[derive(Debug, Default)]
pub struct LearningOrchestrator {
    inner: LearningLoop,
}

impl LearningOrchestrator {
    /// Construct an orchestrator with a fresh [`LearningLoop`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an orchestrator wrapping a pre-seeded loop. Useful
    /// when the integrator wants to declare parameter bounds + seed
    /// initial values at startup.
    #[must_use]
    pub fn with_loop(inner: LearningLoop) -> Self {
        Self { inner }
    }

    /// Read-only access to the underlying loop. The integrator
    /// inspects `loop_.store()` for current values, audit counts,
    /// etc.
    #[must_use]
    pub fn loop_ref(&self) -> &LearningLoop {
        &self.inner
    }

    /// Mutable access to the underlying loop. Use for startup-time
    /// bound registration and seeding only — mutating through here
    /// bypasses the substrate audit.
    #[must_use]
    pub fn loop_mut(&mut self) -> &mut LearningLoop {
        &mut self.inner
    }

    /// Inspect a substrate operation's emitted terminal fragment and
    /// an optional ethics report; extract any [`FeedbackSignal`]s
    /// they carry and record them in the loop's aggregator.
    ///
    /// Conversion rules (per ADR-0013 §"Conversion rules"):
    ///
    /// - When `fragment.provenance.source_subsystem == "reward"`
    ///   and `parent_ids` is non-empty, the fragment is treated as
    ///   a reward-prediction-error emission. The signal's
    ///   `edge_hint` is synthesised as
    ///   `"{parent_id}:{fragment_id}"` so the proposal pipeline
    ///   fires end-to-end. Refining the edge attribution is a
    ///   future ADR.
    /// - When `report` is `Some(_)` and carries non-empty
    ///   `learning_weight_delta` hints, the signal is recorded via
    ///   [`FeedbackSignal::from_ethics_report`].
    ///
    /// Idempotent on emissions that produce no signals.
    pub fn record_emission(&mut self, fragment: &CognitiveFragment, report: Option<&EthicsReport>) {
        if fragment.provenance.source_subsystem == "reward" {
            if let Some(parent) = fragment.provenance.parent_ids.first() {
                // FragmentId derives Debug but not Display; the
                // synthetic edge_hint is audit-trail content, so
                // Debug formatting is the right v0.1 choice.
                let edge_hint = Some(format!("{parent:?}:{:?}", fragment.id));
                self.inner.record(FeedbackSignal::RewardPredictionError {
                    fragment_id: *parent,
                    error: fragment.valence,
                    edge_hint,
                    at: SystemTime::now(),
                });
            }
        }

        if let Some(rep) = report {
            if let Some(sig) = FeedbackSignal::from_ethics_report(rep) {
                self.inner.record(sig);
            }
        }
    }

    /// Drain pending signals, forward each staged proposal to the
    /// substrate's audit pipeline, apply audited values to the
    /// store on success, and feed the audit's ethics report back
    /// into the loop so the next batch reacts.
    ///
    /// Returns the per-proposal substrate outputs in the order they
    /// were forwarded.
    ///
    /// # Errors
    ///
    /// Returns [`SubstrateError`] from the first failing
    /// `propose_learning_update` call. Earlier proposals in the same
    /// batch that succeeded have already mutated the store and been
    /// audited; the integrator can inspect the partial output vector
    /// to decide whether to roll back via journal replay (see
    /// [`LearningLoop::rollback_to`]).
    pub async fn drain_and_propose(
        &mut self,
        substrate: &Substrate,
        actor: ActorContext,
    ) -> Result<Vec<LearningUpdateOutput>, SubstrateError> {
        let staged: Vec<StagedProposal> = self.inner.stage_pending();
        let mut outputs = Vec::with_capacity(staged.len());
        for s in staged {
            let target = s.proposal.target.clone();
            let apply_value = s.apply_value.clone();
            let output = substrate
                .propose_learning_update(s.proposal, actor.clone())
                .await?;
            // RADAR — the substrate's `LearningUpdate` ethics
            // evaluation emits signals but does not veto. Apply on
            // Ok and propagate the audit's ethics signal back into
            // the loop so the next batch can react.
            if let Some(sig) = FeedbackSignal::from_ethics_report(&output.ethics) {
                self.inner.record(sig);
            }
            self.inner.apply_audited(&target, apply_value);
            outputs.push(output);
        }
        Ok(outputs)
    }

    /// Number of feedback signals waiting in the aggregator.
    /// Convenience for tests and diagnostics.
    #[must_use]
    pub fn pending_signal_count(&self) -> usize {
        self.inner.pending_signals().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::{FragmentContent, FragmentId};

    fn reward_fragment(parent: FragmentId, valence: f32) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Reflection {
                body: format!("RPE δ={valence:+.4}"),
                about: vec![parent],
            },
            "reward",
        );
        f.valence = valence;
        f.saliency = valence.abs();
        f.provenance.parent_ids = vec![parent];
        f
    }

    #[test]
    fn record_emission_picks_up_reward_subsystem_fragments() {
        let mut orch = LearningOrchestrator::new();
        let parent = FragmentId::new();
        let rpe = reward_fragment(parent, 0.42);
        let parent_dbg = format!("{parent:?}");

        orch.record_emission(&rpe, None);

        assert_eq!(orch.pending_signal_count(), 1);
        let pending = orch.inner.pending_signals();
        match &pending[0] {
            FeedbackSignal::RewardPredictionError {
                fragment_id,
                error,
                edge_hint,
                ..
            } => {
                assert_eq!(*fragment_id, parent);
                assert!((*error - 0.42).abs() < f32::EPSILON);
                let hint = edge_hint.as_ref().expect("edge_hint must be synthesised");
                assert!(
                    hint.starts_with(&parent_dbg),
                    "edge_hint should start with the parent debug repr; got {hint}",
                );
                assert!(
                    hint.contains(':'),
                    "edge_hint should be `parent:fragment` format",
                );
            }
            other => panic!("expected RewardPredictionError, got {other:?}"),
        }
    }

    #[test]
    fn record_emission_ignores_non_reward_fragments_without_ethics_signal() {
        let mut orch = LearningOrchestrator::new();
        let mut frag = reward_fragment(FragmentId::new(), 0.5);
        frag.provenance.source_subsystem = "input_gate".to_string();

        orch.record_emission(&frag, None);

        assert_eq!(
            orch.pending_signal_count(),
            0,
            "non-reward fragment with no ethics report must not produce a signal",
        );
    }

    #[test]
    fn record_emission_skips_reward_fragment_without_parent_ids() {
        let mut orch = LearningOrchestrator::new();
        let mut frag = reward_fragment(FragmentId::new(), 0.5);
        frag.provenance.parent_ids.clear();

        orch.record_emission(&frag, None);

        assert_eq!(
            orch.pending_signal_count(),
            0,
            "reward fragment without parent_ids cannot anchor an RPE signal",
        );
    }
}
