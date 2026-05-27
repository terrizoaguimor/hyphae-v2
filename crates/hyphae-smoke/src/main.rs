// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-smoke
//!
//! End-to-end smoke runner for Hyphae v2.
//!
//! Starts the substrate, registers the six functional subsystems,
//! ingests a small batch of observations, drives the surface
//! realizer over a working set derived from those observations,
//! and finally runs the eval harness against the v0.1 baseline
//! corpus. The output is the **first time Hyphae runs as a system
//! end-to-end** — the moment v0.1 stops being a blueprint and
//! starts being a cognitive substrate that does something.
//!
//! Run:
//!
//! ```bash
//! cargo run -p hyphae-smoke
//! ```

#![warn(clippy::pedantic)]

use anyhow::Result;
use hyphae_core::{
    ActorContext, CognitiveFragment, DirectPathway, ExternalInputPayload, FragmentContent,
    FragmentId, Pathway, PathwayId, PayloadKind, SubsystemId,
};
use hyphae_eval::{EvalHarness, seed_corpus_en};
use hyphae_substrate::Substrate;
use hyphae_subsystems::{Composer, Episodic, InputGate, Predictive, Reward, Valence};
use hyphae_surface::{Intent, RealizationRequest, SurfaceRealizer};
use tempfile::tempdir;

const HRULE: &str = "─────────────────────────────────────────────────────────";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    print_banner();

    let dir = tempdir()?;
    let path = dir.path();
    println!("hyphae-smoke: substrate path = {}", path.display());

    // ── 1. Construct the substrate.
    //
    // Substrate::new opens the redb state store and the fjall
    // journal, wraps the journal in an Arc<Mutex<...>>, and
    // constructs an EthicsEngine that shares the same journal
    // handle — one chain per substrate per ADR-0003 §8.
    let mut substrate = Substrate::new(path)?;
    println!("hyphae-smoke: substrate initialised in Encoding state\n");

    // ── 2. Register the six functional subsystems.
    register_subsystems(&mut substrate)?;

    // ── 3. Register the pathways the encoding flow needs.
    register_pathways(&mut substrate);

    // ── 4. Ingest a small batch of observations.
    //
    // Each ingest threads through Substrate::ingest, which:
    //   - evaluates the content at the Remember coverage point,
    //   - writes an external_input entry on the shared chain,
    //   - hands the fragment to the InputGate subsystem,
    //   - routes the emitted fragments via the registered
    //     Encoding-kind pathways.
    let actor = ActorContext::new("smoke:operator", "memory:write");
    let observations = sample_observations();
    println!(
        "hyphae-smoke: ingesting {} observations\n{HRULE}",
        observations.len()
    );
    for obs in observations {
        let out = substrate
            .ingest(ExternalInputPayload::new(obs), actor.clone())
            .await?;
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

    // ── 5. Build a working set the realizer can compose against.
    //
    // In a future milestone the composer subsystem would assemble
    // this from substrate.recall_signal() + the episodic cascade.
    // The v0.1 smoke runner provides the working set directly so
    // the realizer's behaviour is observable in isolation.
    let working_set = build_working_set();
    println!(
        "hyphae-smoke: assembled working set: {} fragments (all cascade-derived)\n",
        working_set.len()
    );

    // ── 6. Realize a Dialogue reply over the working set.
    let realizer = SurfaceRealizer::new();
    let request = RealizationRequest {
        intent: Intent::Dialogue,
        query: "what is the status of the migration?",
        working_set: &working_set,
        ethics: None,
    };
    let realization = realizer.realize(&request)?;
    println!("hyphae-smoke: compose ─────────────────────────────────────");
    println!();
    println!("  query   : \"{}\"", request.query);
    println!("  schema  : {:?}", realization.schema_used);
    println!(
        "  quoted  : {} fragment(s)",
        realization.fragments_quoted.len()
    );
    println!("  flags   : {}", format_triggers(&realization.limitations));
    println!();
    println!("  composition:");
    for line in realization.text.lines() {
        println!("    {line}");
    }
    println!();
    println!("{HRULE}\n");

    // ── 7. Run the eval harness over the v0.1 baseline corpus.
    let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
    println!(
        "hyphae-smoke: running eval harness ({} queries)\n{HRULE}",
        harness.len()
    );
    let report = harness.run();
    println!("{}", report.render());

    // ── 8. Drop the substrate cleanly. The journal flushes on
    //    Drop; the tempdir is removed when `dir` falls out of
    //    scope. A "real" deployment would persist the path.
    drop(substrate);
    println!("hyphae-smoke: done.");
    Ok(())
}

/// Configure a minimal tracing subscriber for the smoke runner.
/// Quiet by default — the smoke runner's own `println!` lines
/// carry the user-facing narrative. Enable the substrate's per-step
/// tracing by depending on a richer subscriber feature set in a
/// future ADR; the smoke runner stays dependency-light.
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
    // Encoding flow: input-gate fans the dispatched fragment to
    // both episodic (for storage) and valence (for affective
    // stamping).
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
    println!(
        "hyphae-smoke: registered 2 pathways (encoding fan-out: input-gate → {{episodic, valence}})\n"
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

fn build_working_set() -> Vec<CognitiveFragment> {
    sample_observations()
        .into_iter()
        .map(|body| {
            let mut f = CognitiveFragment::new(
                FragmentContent::Observation {
                    body: body.to_string(),
                },
                "smoke",
            );
            // Mark the fragments as cascade-derived so the
            // ShallowCascade limitation trigger does not fire on
            // this healthy demo.
            f.provenance.parent_ids = vec![FragmentId::new()];
            // Tag the fragments with technical-domain markers so
            // the realizer's `register_for_fragment` heuristic
            // picks `Register::Technical` for the inter-fragment
            // connectives (per ADR-0005 §"Context-aware picker").
            // This is what makes the lexicon expansion visible in
            // the smoke output.
            f.domain_tags.push("deploy".to_string());
            f.domain_tags.push("infrastructure".to_string());
            f
        })
        .collect()
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
