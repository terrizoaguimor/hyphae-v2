// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! ADR-0013 integration test — the full learning loop fires
//! end-to-end through a real `Substrate`.
//!
//! Demonstration shape:
//!
//! 1. Synthesize a fragment shaped like a `Reward` subsystem
//!    emission (signed RPE on the `valence` field,
//!    `source_subsystem = "reward"`, `parent_ids` populated).
//! 2. Hand it to `LearningOrchestrator::record_emission`. The
//!    orchestrator converts it to a `FeedbackSignal::
//!    RewardPredictionError` with synthesised `edge_hint`.
//! 3. Declare bounds on the synthesised target so
//!    `stage_pending` does not silently drop the proposal.
//! 4. Call `LearningOrchestrator::drain_and_propose` against a
//!    real `Substrate`. The substrate runs ethics at
//!    `CoveragePoint::LearningUpdate`, journals an
//!    `audit_learning_update` entry, and returns
//!    `LearningUpdateOutput { audit_seq: Some(_), ethics: _ }`.
//! 5. Assert the parameter store has been mutated.
//!
//! This validates the three loops ADR-0013 closes:
//! recording → proposal → application.

use hyphae_core::{ActorContext, FragmentContent, FragmentId, Provenance};
use hyphae_learning::{
    FeedbackSignal, LearningOrchestrator, ParameterBounds, ParameterStore, ParameterValue,
};
use hyphae_substrate::{LearningTarget, Substrate};
use tempfile::tempdir;

fn driver() -> ActorContext {
    ActorContext::new("test:driver", "integration:learning_loop")
}

/// Build a fragment that looks like a `Reward` subsystem emission.
fn synthetic_rpe(parent: FragmentId, valence: f32) -> hyphae_core::CognitiveFragment {
    let mut f = hyphae_core::CognitiveFragment::new(
        FragmentContent::Reflection {
            body: format!("RPE δ={valence:+.4}"),
            about: vec![parent],
        },
        "reward",
    );
    f.valence = valence;
    f.saliency = valence.abs();
    f.provenance = Provenance {
        source_subsystem: "reward".to_string(),
        source_pathway: None,
        parent_ids: vec![parent],
        confabulation_risk: 0.0,
        namespace: f.provenance.namespace.clone(),
    };
    f
}

#[tokio::test]
async fn learning_loop_records_proposes_applies_end_to_end() {
    let dir = tempdir().unwrap();
    let substrate = Substrate::new(dir.path()).unwrap();
    let mut orch = LearningOrchestrator::new();

    // (1) Synthesize an RPE emission and record it.
    let parent = FragmentId::new();
    let rpe = synthetic_rpe(parent, 0.3);
    orch.record_emission(&rpe, None);
    assert_eq!(
        orch.pending_signal_count(),
        1,
        "orchestrator must record exactly one signal from a single RPE emission",
    );

    // (2) Inspect the pending signal to learn what edge_id the
    //     orchestrator synthesised, then declare bounds at that
    //     target so the proposal pipeline does not drop the
    //     intent.
    let edge_id = match &orch.loop_ref().pending_signals()[0] {
        FeedbackSignal::RewardPredictionError { edge_hint, .. } => edge_hint
            .clone()
            .expect("orchestrator must synthesise an edge_hint"),
        other => panic!("expected RewardPredictionError, got {other:?}"),
    };
    let target = LearningTarget::EpisodicConductivityWeight {
        edge_id: edge_id.clone(),
    };
    orch.loop_mut()
        .store_mut()
        .set_bounds(&target, ParameterBounds::new(-1.0, 1.0));
    orch.loop_mut()
        .store_mut()
        .seed(&target, ParameterValue::Scalar(0.0));

    // (3) Drain the loop. The proposal flows to the substrate,
    //     gets audited at `CoveragePoint::LearningUpdate`,
    //     journaled, and the audited value is applied to the
    //     store.
    let outputs = orch
        .drain_and_propose(&substrate, driver())
        .await
        .expect("drain_and_propose must succeed");

    assert_eq!(
        outputs.len(),
        1,
        "exactly one proposal should have been forwarded",
    );
    let out = &outputs[0];
    assert!(
        out.audit_seq.is_some(),
        "the substrate must have journaled the audit entry",
    );

    // (4) The parameter store has been mutated by `apply_audited`.
    //     The intent's delta was +0.3, the initial value 0.0, the
    //     bounds [-1, 1], so the new value should be 0.3 (clamped
    //     within bounds).
    let new_value = orch
        .loop_ref()
        .store()
        .get(&target)
        .expect("store must hold a value after apply_audited");
    match new_value {
        ParameterValue::Scalar(v) => {
            assert!(
                (*v - 0.3).abs() < 1e-5,
                "expected new value ≈ 0.3 after applying +0.3 delta to seed 0.0; got {v}",
            );
        }
        other => panic!("expected Scalar value, got {other:?}"),
    }

    // (5) Pending signals should be drained.
    //     (The audit's own ethics report may have produced a new
    //     signal that the orchestrator records; in v0.1 the ethics
    //     engine emits no learning hints by default, so the
    //     aggregator should be empty.)
    assert_eq!(
        orch.pending_signal_count(),
        0,
        "all pending signals should have been drained by stage_pending",
    );
}

#[tokio::test]
async fn drain_with_no_pending_signals_returns_empty_output() {
    let dir = tempdir().unwrap();
    let substrate = Substrate::new(dir.path()).unwrap();
    let mut orch = LearningOrchestrator::new();

    let outputs = orch
        .drain_and_propose(&substrate, driver())
        .await
        .expect("drain on an empty aggregator must succeed");

    assert!(outputs.is_empty(), "no signals → no proposals → no outputs",);
}

#[tokio::test]
async fn parameter_store_can_be_pre_seeded_via_with_loop() {
    // Demonstrates the deployment shape: integrator declares
    // bounds + seeds initial values at startup using `with_loop`.
    let mut store = ParameterStore::new();
    let target = LearningTarget::EpisodicConductivityWeight {
        edge_id: "frag_a:frag_b".to_string(),
    };
    store.set_bounds(&target, ParameterBounds::new(0.0, 1.0));
    store.seed(&target, ParameterValue::Scalar(0.5));

    let inner = hyphae_learning::LearningLoop::with_store(store);
    let orch = LearningOrchestrator::with_loop(inner);

    let current = orch
        .loop_ref()
        .store()
        .get(&target)
        .expect("seeded value must be readable");
    assert_eq!(*current, ParameterValue::Scalar(0.5));
}
