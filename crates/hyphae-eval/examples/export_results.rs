// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Export Hyphae's harness output as JSON for the comparator.
//!
//! Run via `cargo run -p hyphae-eval --example export_results`.
//! Emits, for each EN corpus query, the rendered realizer output
//! plus the seed bodies that were in the working set. Same shape as
//! the JSON the Python comparator emits for the LLM+RAG baseline so
//! the same Python scoring routines apply to both.
//!
//! Per ADR-0027 §"What this comparison establishes", only the
//! comparable-subset metrics (verbatim, connective hygiene, n-gram
//! overlap, NLI unsupported-claim rate) are scored from this output.
//! Hyphae's own 9-dim eval lives in `hyphae-eval`'s test suite and
//! the writeup carries those numbers separately.

use std::time::Instant;

use hyphae_eval::seed_corpus_en;
use hyphae_surface::{RealizationRequest, SurfaceRealizer};
use serde::Serialize;

#[derive(Serialize)]
struct ExportedQuery {
    query_id: String,
    response: String,
    retrieved_chunks: Vec<String>,
    /// Realizer latency in milliseconds, sub-millisecond precision
    /// preserved as fractional ms. Hyphae's realizer runs in
    /// microseconds for typical working-set sizes (ADR-0015); the
    /// LLM baseline runs in seconds — the head-to-head spans three
    /// orders of magnitude, so an integer-ms field would truncate
    /// Hyphae's number to 0.
    latency_ms: f64,
}

fn main() {
    let realizer = SurfaceRealizer::new();
    let corpus = seed_corpus_en();
    let mut out: Vec<ExportedQuery> = Vec::with_capacity(corpus.len());

    for q in corpus.queries() {
        let working_set: Vec<hyphae_core::CognitiveFragment> = q
            .seeds
            .iter()
            .cloned()
            .map(hyphae_eval::EvalSeed::into_fragment)
            .collect();

        let t0 = Instant::now();
        let output = realizer
            .realize(&RealizationRequest {
                intent: q.intent,
                query: &q.query,
                working_set: &working_set,
                ethics: None,
                shape: None,
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

    let json = serde_json::to_string_pretty(&out).expect("ExportedQuery derives Serialize");
    println!("{json}");
}
