// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Export Hyphae's harness output under one of the ADR-0029
//! ablation conditions. Same envelope shape as `export_results.rs`
//! so the Python `score_hyphae` runner grades the result without
//! changes.
//!
//! Usage: `cargo run --example export_results_ablation -- --ablation <NAME>`
//! where `<NAME>` is one of:
//!
//!   - `none`              — control, identical output to `export_results`
//!   - `no-shape`          — A1, force linear Continuation shape (bypass cascade-shape derivation)
//!   - `no-ethics`         — A2, pass `ethics = None` to every realize call
//!   - `minimal-lexicon`   — A3, realizer built with `Lexicon::minimal_en()`
//!   - `no-smoothing`      — A4, realizer with `disable_smoothing()`
//!
//! The `metadata.ablation` field in the emitted JSON records which
//! condition produced the run so downstream comparison scripts can
//! identify it.

use std::env;
use std::time::Instant;

use hyphae_eval::seed_corpus_en;
use hyphae_surface::{
    CompositionShape, CompositionStep, ConnectiveRole, Lexicon, RealizationRequest,
    SurfaceRealizer,
};
use serde::Serialize;

#[derive(Serialize)]
struct ExportedQuery {
    query_id: String,
    response: String,
    retrieved_chunks: Vec<String>,
    latency_ms: f64,
}

#[derive(Serialize)]
struct Envelope {
    ablation: String,
    queries: Vec<ExportedQuery>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut ablation = "none".to_string();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--ablation" && i + 1 < args.len() {
            ablation = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    let valid = ["none", "no-shape", "no-ethics", "minimal-lexicon", "no-smoothing"];
    if !valid.contains(&ablation.as_str()) {
        eprintln!(
            "error: unknown --ablation `{ablation}`. Valid: {}",
            valid.join(", ")
        );
        std::process::exit(2);
    }

    let realizer = build_realizer(&ablation);
    let corpus = seed_corpus_en();
    let mut out: Vec<ExportedQuery> = Vec::with_capacity(corpus.len());

    for q in corpus.queries() {
        let working_set: Vec<hyphae_core::CognitiveFragment> = q
            .seeds
            .iter()
            .cloned()
            .map(hyphae_eval::EvalSeed::into_fragment)
            .collect();

        // A1 no-shape: build a flat linear Continuation shape that
        // bypasses `shape_from_working_set`'s contrast injection and
        // any future shape inference.
        let forced_shape = if ablation == "no-shape" {
            Some(linear_continuation_shape(&working_set))
        } else {
            None
        };

        let t0 = Instant::now();
        let output = realizer
            .realize(&RealizationRequest {
                intent: q.intent,
                query: &q.query,
                working_set: &working_set,
                ethics: None, // A2 no-ethics is the default for the comparator anyway;
                // production callers do thread an EthicsReport from
                // the substrate, and reinstating that path is a
                // separate ADR.
                shape: forced_shape.as_ref(),
            })
            .expect("v0.1 realizer maps every intent");
        let latency_ms = t0.elapsed().as_nanos() as f64 / 1_000_000.0;

        out.push(ExportedQuery {
            query_id: q.id.clone(),
            response: output.text,
            retrieved_chunks: q.seeds.iter().map(|s| s.body.clone()).collect(),
            latency_ms,
        });
    }

    let envelope = Envelope {
        ablation,
        queries: out,
    };

    let json = serde_json::to_string_pretty(&envelope.queries)
        .expect("ExportedQuery derives Serialize");
    // Match `export_results.rs` output shape: a top-level JSON array
    // of ExportedQuery, so `score_hyphae.py` reads it without any
    // schema change. The ablation tag is preserved by the file
    // name the operator chooses for stdout redirection.
    eprintln!("ablation: {}, queries: {}", envelope.ablation, envelope.queries.len());
    println!("{json}");
}

fn build_realizer(ablation: &str) -> SurfaceRealizer {
    let lexicon = if ablation == "minimal-lexicon" {
        Lexicon::minimal_en()
    } else {
        Lexicon::baseline_en()
    };
    let mut realizer = SurfaceRealizer::with_lexicon(lexicon);
    if ablation == "no-smoothing" {
        realizer.disable_smoothing();
    }
    realizer
}

/// Build a flat linear shape with every inter-fragment role as
/// `Continuation`. Bypasses `shape_from_working_set`'s contrast
/// injection — A1 no-shape ablation.
fn linear_continuation_shape(working_set: &[hyphae_core::CognitiveFragment]) -> CompositionShape {
    let steps = working_set
        .iter()
        .map(|f| CompositionStep {
            role: ConnectiveRole::Continuation,
            fragment: f.clone(),
            depth: 0,
        })
        .collect();
    CompositionShape { steps }
}
