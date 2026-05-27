// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-smoke
//!
//! End-to-end smoke runner for Hyphae v2.
//!
//! Starts the substrate, registers the six functional subsystems,
//! ingests a small batch of observations (with the encoding flow
//! routed through `Valence → Reward` so the learning loop receives
//! real RPE signals), drains the learning loop through the
//! substrate's audit pipeline, transitions to `Recall`, queries the
//! substrate, lets the surface realizer compose against the real
//! recall output, and finally runs the eval harness.
//!
//! This is the first run that exercises every v0.1 component in one
//! pass:
//!
//! - **ADR-0011**: real recall fires the cascade activation
//!   (`recall_signal → episodic.process → episodic.cascade`).
//! - **ADR-0013**: real RPE emissions feed the
//!   `LearningOrchestrator`, which drains via
//!   `substrate.propose_learning_update`.
//! - **ADR-0006/0007**: cascade-shape composition + boundary
//!   smoothing produce the realizer's output.
//! - **ADR-0008/0009/0010**: the eval harness reports fluency
//!   dimensions, bucket coverage, and sensitivity audit status.
//!
//! Run:
//!
//! ```bash
//! cargo run -p hyphae-smoke
//! ```

#![warn(clippy::pedantic)]

use anyhow::Result;
use hyphae_core::{
    ActivationLevel, ActorContext, CascadeActivation, CascadeRetrieval, CognitiveFragment,
    DirectPathway, ExternalInputPayload, Pathway, PathwayId, PayloadKind, State, SubsystemId,
};
use hyphae_eval::{EvalHarness, seed_corpus_en};
use hyphae_learning::{FeedbackSignal, LearningOrchestrator, ParameterBounds, ParameterValue};
use hyphae_substrate::{LearningTarget, Substrate};
use hyphae_subsystems::{Composer, Episodic, InputGate, Predictive, Reward, Valence};
use hyphae_surface::{Intent, RealizationRequest, SurfaceRealizer};
use std::collections::HashMap;
use tempfile::tempdir;

const HRULE: &str = "─────────────────────────────────────────────────────────";

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<()> {
    init_tracing();
    print_banner();

    let dir = tempdir()?;
    let path = dir.path();
    println!("hyphae-smoke: substrate path = {}", path.display());

    // ── 1. Construct the substrate and the learning orchestrator.
    let mut substrate = Substrate::new(path)?;
    let mut orchestrator = LearningOrchestrator::new();
    println!("hyphae-smoke: substrate initialised in Encoding state");
    println!("hyphae-smoke: learning orchestrator initialised (empty store)\n");

    // ── 2. Register the six functional subsystems.
    register_subsystems(&mut substrate)?;

    // ── 3. Register the pathways the encoding flow needs.
    register_pathways(&mut substrate);

    // ── 4. Encoding phase — ingest observations.
    let actor = ActorContext::new("smoke:operator", "memory:write");
    let observations = sample_observations();
    println!(
        "hyphae-smoke: encoding phase — ingesting {} observations\n{HRULE}",
        observations.len()
    );
    for obs in &observations {
        let out = substrate
            .ingest(ExternalInputPayload::new(*obs), actor.clone())
            .await?;
        // ADR-0013: every emitted terminal is offered to the
        // orchestrator. Reward terminals carry signed RPE on their
        // valence axis; the orchestrator extracts the feedback
        // signal.
        for terminal in &out.terminals {
            orchestrator.record_emission(terminal, Some(&out.ethics));
        }
        println!("  • \"{obs}\"");
        println!(
            "      ethics @ Remember: audit_seq={:?}  cvar={:.4}  flags={}",
            out.ethics.audit_seq,
            out.ethics.cvar_score,
            out.ethics.violations.len()
        );
        println!("      routed_terminals: {}", out.terminals.len());
    }
    println!("{HRULE}\n");

    // ── 5. Learning phase — drain the orchestrator through the
    //    substrate's audit pipeline.
    println!(
        "hyphae-smoke: learning phase — orchestrator carries {} pending signal(s)\n{HRULE}",
        orchestrator.pending_signal_count()
    );
    declare_bounds_for_pending_rpe(&mut orchestrator);
    let learning_outputs = orchestrator
        .drain_and_propose(&substrate, actor.clone())
        .await?;
    println!(
        "  drained {} proposal(s) through substrate.propose_learning_update",
        learning_outputs.len()
    );
    for (i, out) in learning_outputs.iter().enumerate() {
        println!(
            "  proposal {i}: audit_seq={:?}  ethics_cvar={:.4}",
            out.audit_seq, out.ethics.cvar_score,
        );
    }
    println!("{HRULE}\n");

    // ── 6. Recall phase — transition to Recall and query.
    substrate.transition_to(State::Recall).await?;
    let cue = "what is the status of the migration?";
    let recall_out = substrate.recall_signal(cue, actor.clone()).await?;
    let working_set: Vec<CognitiveFragment> = recall_out
        .terminals
        .iter()
        .filter(|f| {
            !matches!(&f.content, hyphae_core::FragmentContent::Observation { body }
                              if body == cue)
        })
        .cloned()
        .collect();
    let with_parent_ids = working_set
        .iter()
        .filter(|f| !f.provenance.parent_ids.is_empty())
        .count();
    println!("hyphae-smoke: recall phase — cue = \"{cue}\"\n{HRULE}");
    println!(
        "  recall_signal terminals: {} (after dropping cue passthrough: {})",
        recall_out.terminals.len(),
        working_set.len(),
    );
    println!(
        "  fragments with parent_ids populated: {with_parent_ids} / {} (mixed: ADR-0011 \
         cascade tags + encoding-time routing fan-out audit)",
        working_set.len(),
    );
    println!(
        "  ethics @ Recall: audit_seq={:?}  cvar={:.4}",
        recall_out.ethics.audit_seq, recall_out.ethics.cvar_score,
    );
    println!("{HRULE}\n");

    // ── 7. Composition phase — build a cascade view from the real
    //    recall output, project the shape, realize.
    let cascade_view = cascade_view_from_recall(&working_set);
    let shape = hyphae_surface::shape_from_cascade(&cascade_view);
    println!(
        "hyphae-smoke: cascade-shape projection: {} step(s) (ADR-0006)",
        shape.len()
    );
    for (i, step) in shape.steps.iter().enumerate() {
        println!(
            "  step {i}: role={:?}  depth={}  valence={:+.2}",
            step.role, step.depth, step.fragment.valence
        );
    }
    println!();

    let realizer = SurfaceRealizer::new();
    let request = RealizationRequest {
        intent: Intent::Dialogue,
        query: cue,
        working_set: &working_set,
        shape: Some(&shape),
        ethics: None,
    };
    let realization = realizer.realize(&request)?;
    println!("hyphae-smoke: compose ─────────────────────────────────────\n");
    println!("  query   : \"{}\"", request.query);
    println!("  schema  : {:?}", realization.schema_used);
    println!(
        "  quoted  : {} fragment(s)",
        realization.fragments_quoted.len()
    );
    println!("  flags   : {}", format_triggers(&realization.limitations));
    println!("\n  composition:");
    for line in realization.text.lines() {
        println!("    {line}");
    }
    println!("\n{HRULE}\n");

    // ── 8. Run the eval harness over the v0.1 baseline corpus.
    let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
    println!(
        "hyphae-smoke: running eval harness ({} queries)\n{HRULE}",
        harness.len()
    );
    let report = harness.run();
    println!("{}", report.render());

    // ── 9. Drop the substrate cleanly.
    drop(substrate);
    println!("hyphae-smoke: done.");
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
}

fn print_banner() {
    println!();
    println!("  ╭──────────────────────────────────────────────────────╮");
    println!("  │              h y p h a e   v 0 . 1                   │");
    println!("  │      cognitive substrate — smoke runner              │");
    println!("  ╰──────────────────────────────────────────────────────╯");
    println!();
}

fn register_subsystems(substrate: &mut Substrate) -> Result<()> {
    substrate.register(Box::new(InputGate::new()))?;
    substrate.register(Box::new(Episodic::new()))?;
    substrate.register(Box::new(Valence::new()))?;
    substrate.register(Box::new(Composer::new()))?;
    substrate.register(Box::new(Predictive::new()))?;
    substrate.register(Box::new(Reward::new()))?;
    println!(
        "hyphae-smoke: registered 6 subsystems  (input-gate, episodic, valence, composer, predictive, reward)"
    );
    Ok(())
}

fn register_pathways(substrate: &mut Substrate) {
    // Encoding flow:
    //   InputGate → Episodic   (storage)
    //   InputGate → Valence    (affective stamp)
    //   Valence   → Reward     (RPE; feeds the learning loop)
    //
    // Reward emits a `BottomUpPredictionError`-flavoured fragment
    // carrying signed RPE on its valence axis. With no outgoing
    // pathway from Reward the emission becomes a terminal of
    // `substrate.ingest`, which the orchestrator observes.
    substrate.register_pathway(direct_pathway(
        1,
        PayloadKind::Encoding,
        SubsystemId::InputGate,
        SubsystemId::Episodic,
    ));
    substrate.register_pathway(direct_pathway(
        2,
        PayloadKind::Encoding,
        SubsystemId::InputGate,
        SubsystemId::Valence,
    ));
    substrate.register_pathway(direct_pathway(
        3,
        PayloadKind::Encoding,
        SubsystemId::Valence,
        SubsystemId::Reward,
    ));
    println!(
        "hyphae-smoke: registered 3 pathways (encoding: input-gate → {{episodic, valence → reward}})\n"
    );
}

fn direct_pathway(
    id: u32,
    kind: PayloadKind,
    src: SubsystemId,
    dst: SubsystemId,
) -> Box<dyn Pathway> {
    Box::new(DirectPathway::new(PathwayId(id), kind, src, dst))
}

fn sample_observations() -> Vec<&'static str> {
    vec![
        "the migration completed at 14:02 UTC",
        "the monitoring dashboards stayed green for the hour after the cutover",
        "the deploy succeeded on the first attempt",
    ]
}

/// Iterate the orchestrator's pending RPE signals and declare bounds
/// for each synthesised target so `stage_pending` does not silently
/// drop the proposal. v0.1 demo convention — a future ADR refines
/// the edge attribution so bounds can be declared at startup.
fn declare_bounds_for_pending_rpe(orch: &mut LearningOrchestrator) {
    let edges: Vec<String> = orch
        .loop_ref()
        .pending_signals()
        .iter()
        .filter_map(|s| match s {
            FeedbackSignal::RewardPredictionError { edge_hint, .. } => edge_hint.clone(),
            _ => None,
        })
        .collect();
    if !edges.is_empty() {
        println!(
            "  declaring bounds on {} synthesised conductivity-weight target(s)",
            edges.len()
        );
    }
    for edge_id in edges {
        let target = LearningTarget::EpisodicConductivityWeight { edge_id };
        let store = orch.loop_mut().store_mut();
        store.set_bounds(&target, ParameterBounds::new(-1.0, 1.0));
        store.seed(&target, ParameterValue::Scalar(0.0));
    }
}

/// Build a `CascadeRetrieval` view from the real recall terminals so
/// the cascade-shape projection has topology to work with. The first
/// element is treated as the direct anchor; the remaining terminals
/// with non-empty `parent_ids` are first-hop cascade supports.
fn cascade_view_from_recall(working_set: &[CognitiveFragment]) -> CascadeRetrieval {
    if working_set.is_empty() {
        return CascadeRetrieval::empty();
    }
    let anchor = working_set[0].clone();
    let anchor_id = anchor.id;
    let direct = vec![(0.0_f32, anchor)];

    let mut cascade = HashMap::new();
    for (idx, frag) in working_set.iter().enumerate().skip(1) {
        if frag.provenance.parent_ids.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let activation = 0.9 - (idx as f32) * 0.15;
        let act = CascadeActivation {
            fragment_id: frag.id,
            activation: ActivationLevel::new(activation.max(0.1)),
            hops_from_source: 1,
            parent_id: Some(anchor_id),
            propagated: false,
        };
        cascade.insert(frag.id, (act, frag.clone()));
    }

    CascadeRetrieval { direct, cascade }
}

fn format_triggers(triggers: &[hyphae_surface::LimitationTrigger]) -> String {
    if triggers.is_empty() {
        return "none".to_string();
    }
    triggers
        .iter()
        .map(|t| t.tag().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
