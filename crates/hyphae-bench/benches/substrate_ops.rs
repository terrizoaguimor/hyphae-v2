// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Substrate operation benches — per ADR-0015.
//!
//! Three groups:
//!
//! - `ingest_at_n_stored`: single-ingest latency over a substrate
//!   pre-populated with N fragments (N ∈ {10, 100, 1000}). The
//!   substrate is shared across iterations, so the actual
//!   population during measurement is `N + iter_count` —
//!   acceptable honesty caveat per ADR-0015.
//! - `recall_at_n_stored`: single-recall latency at the same
//!   population tiers. Substrate is transitioned to
//!   `State::Recall` at setup; iterations rotate through a small
//!   set of cues so the measurement is not purely cached.
//! - `compose_at_working_set_size`: single `realize` latency for
//!   working sets of size 1, 3, 7. The realizer is pure given a
//!   working set; this bench isolates surface complexity from
//!   substrate retrieval.
//!
//! All numbers ride the chain as honest measurements. No
//! threshold gates per ADR-0008 + ADR-0015 discipline.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use hyphae_core::{
    ActorContext, CognitiveFragment, DirectPathway, ExternalInputPayload, FragmentContent, Pathway,
    PathwayId, PayloadKind, State, SubsystemId,
};
use hyphae_substrate::Substrate;
use hyphae_subsystems::{Composer, Episodic, InputGate, Predictive, Reward, Valence};
use hyphae_surface::{Intent, RealizationRequest, SurfaceRealizer};
use tokio::runtime::Runtime;

const SIZES: [usize; 3] = [10, 100, 1000];

/// Build a fresh substrate, register subsystems + pathways, and
/// pre-populate with `n` observations. Returns the substrate, its
/// `tempdir` holder (drops the dir on drop), and an actor context.
async fn setup_populated(n: usize) -> (Substrate, tempfile::TempDir, ActorContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut substrate = Substrate::new(dir.path()).unwrap();
    register_subsystems(&mut substrate).unwrap();
    register_pathways(&mut substrate);
    let actor = ActorContext::new("bench:driver", "bench:populate");
    for i in 0..n {
        let body = format!("seed observation {i}");
        substrate
            .ingest(ExternalInputPayload::new(body), actor.clone())
            .await
            .unwrap();
    }
    (substrate, dir, actor)
}

fn register_subsystems(substrate: &mut Substrate) -> Result<(), hyphae_substrate::SubstrateError> {
    substrate.register(Box::new(InputGate::new()))?;
    substrate.register(Box::new(Episodic::new()))?;
    substrate.register(Box::new(Valence::new()))?;
    substrate.register(Box::new(Composer::new()))?;
    substrate.register(Box::new(Predictive::new()))?;
    substrate.register(Box::new(Reward::new()))?;
    Ok(())
}

fn register_pathways(substrate: &mut Substrate) {
    let mk = |id: u32, src, dst| -> Box<dyn Pathway> {
        Box::new(DirectPathway::new(
            PathwayId(id),
            PayloadKind::Encoding,
            src,
            dst,
        ))
    };
    substrate.register_pathway(mk(1, SubsystemId::InputGate, SubsystemId::Episodic));
    substrate.register_pathway(mk(2, SubsystemId::InputGate, SubsystemId::Valence));
    substrate.register_pathway(mk(3, SubsystemId::Valence, SubsystemId::Reward));
}

fn bench_ingest(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("ingest_at_n_stored");
    for &n in &SIZES {
        // Build one substrate per population tier; shared across
        // iterations. After the bench finishes the substrate has
        // ~N + (criterion iterations) fragments.
        let (substrate, _dir, actor) = rt.block_on(setup_populated(n));
        let mut counter = n;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.to_async(&rt).iter(|| {
                counter += 1;
                let body = format!("bench observation {counter}");
                let actor = actor.clone();
                let substrate_ref = &substrate;
                async move {
                    substrate_ref
                        .ingest(ExternalInputPayload::new(body), actor)
                        .await
                        .unwrap();
                }
            });
        });
    }
    group.finish();
}

fn bench_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("recall_at_n_stored");
    let cues = [
        "what was the seed observation",
        "tell me about the recent ingestions",
        "any updates about the stored items",
        "summarise the recent observations",
    ];
    for &n in &SIZES {
        let (substrate, _dir, actor) = rt.block_on(setup_populated(n));
        // Transition once; recalls do not change the state machine.
        rt.block_on(substrate.transition_to(State::Recall)).unwrap();
        let mut cue_idx: usize = 0;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.to_async(&rt).iter(|| {
                let cue = cues[cue_idx % cues.len()];
                cue_idx = cue_idx.wrapping_add(1);
                let actor = actor.clone();
                let substrate_ref = &substrate;
                async move {
                    substrate_ref.recall_signal(cue, actor).await.unwrap();
                }
            });
        });
    }
    group.finish();
}

fn bench_compose(c: &mut Criterion) {
    let realizer = SurfaceRealizer::new();
    let mut group = c.benchmark_group("compose_at_working_set_size");
    for size in [1_usize, 3, 7] {
        let working_set: Vec<CognitiveFragment> = (0..size)
            .map(|i| {
                let mut f = CognitiveFragment::new(
                    FragmentContent::Observation {
                        body: format!("working set fragment {i}"),
                    },
                    "bench",
                );
                if i > 0 {
                    f.provenance.parent_ids.push(hyphae_core::FragmentId::new());
                }
                f
            })
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let request = RealizationRequest {
                    intent: Intent::Dialogue,
                    query: "bench compose query",
                    working_set: &working_set,
                    shape: None,
                    ethics: None,
                };
                let _ = realizer.realize(&request).unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_ingest, bench_recall, bench_compose);
criterion_main!(benches);
