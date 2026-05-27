// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! ADR-0011 integration test — `Substrate::recall_signal` exercises
//! cascade activation end-to-end.
//!
//! Before ADR-0011, `episodic.process()` in the Recall branch
//! returned only `pattern_complete` direct hits; the cascade engine
//! was wired internally but never invoked on the retrieval path,
//! contradicting RFC §3 ("cascade activation is the retrieval
//! mechanism, not optional enhancement").
//!
//! This test stores three co-encoded fragments through the real
//! `InputGate → Episodic` pathway, then issues a recall whose cue
//! aligns with one of them. With `working_set_size = 1`, only the
//! aligned fragment is a direct hit; the other two MUST arrive via
//! cascade propagation. The test asserts:
//!
//! 1. The recall terminals contain at least one cascade-derived
//!    fragment (one with `provenance.parent_ids` populated by the
//!    cascade tagging logic).
//! 2. The ethics report was emitted at `CoveragePoint::Recall`.

use hyphae_core::{
    ActorContext, CascadeParams, DirectPathway, ExternalInputPayload, PathwayId, PayloadKind,
    State, SubsystemId,
};
use hyphae_ethics::CoveragePoint;
use hyphae_substrate::Substrate;
use hyphae_subsystems::{Episodic, InputGate};
use tempfile::tempdir;

fn driver() -> ActorContext {
    ActorContext::new("test:driver", "integration:recall_cascade")
}

#[tokio::test]
async fn recall_signal_exercises_cascade_propagation_end_to_end() {
    let dir = tempdir().unwrap();
    let mut substrate = Substrate::new(dir.path()).unwrap();

    // working_set_size = 1 forces pattern_complete to return only
    // the closest direct hit. Co-encoded neighbours must arrive via
    // cascade or not at all — exactly the propagation channel
    // ADR-0011 wires.
    let episodic = Episodic::with_params(CascadeParams {
        working_set_size: 1,
        ..CascadeParams::SPREADR_DEFAULTS
    });

    substrate.register(Box::new(InputGate::new())).unwrap();
    substrate.register(Box::new(episodic)).unwrap();

    // Encoding pathway: InputGate → Episodic. Without this the
    // ingested fragment never reaches the Episodic store.
    substrate.register_pathway(Box::new(DirectPathway::new(
        PathwayId(1),
        PayloadKind::Encoding,
        SubsystemId::InputGate,
        SubsystemId::Episodic,
    )));

    // Substrate starts in Encoding; ingest three co-encodable
    // observations.
    let bodies = [
        "the alpha system shipped on monday",
        "the beta release rolled out the following week",
        "the gamma rollout completed without incidents",
    ];
    for body in &bodies {
        substrate
            .ingest(ExternalInputPayload::new(*body), driver())
            .await
            .expect("ingest should succeed");
    }

    // Transition to Recall and issue a cue aligned with one of the
    // stored fragments.
    substrate.transition_to(State::Recall).await.unwrap();
    let output = substrate
        .recall_signal("the alpha system shipped on monday", driver())
        .await
        .expect("recall_signal should succeed");

    // (1) Ethics report rode the Recall coverage point.
    assert_eq!(output.ethics.coverage_point, CoveragePoint::Recall);

    // (2) At least one cascade-derived fragment arrived. With
    //     working_set_size = 1, pattern_complete returns only the
    //     direct hit; any additional fragment in `terminals` MUST
    //     have arrived via cascade and therefore carry the
    //     ADR-0011 parent tag.
    let cascade_derived = output
        .terminals
        .iter()
        .filter(|f| !f.provenance.parent_ids.is_empty())
        .count();
    assert!(
        cascade_derived >= 1,
        "expected ≥1 cascade-derived fragment in recall terminals, got {}; \
         total terminals = {}",
        cascade_derived,
        output.terminals.len(),
    );
}
