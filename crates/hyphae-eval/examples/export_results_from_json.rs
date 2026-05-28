// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Export Hyphae's realizer output running against a corpus loaded
//! from JSON (rather than the embedded `seed_corpus_en`).
//!
//! Per ADR-0031 (planned): the multi-LLM matrix runs against a
//! standard benchmark (TriviaQA rc validation subset) in addition
//! to the project's own corpus. The benchmark is loaded from JSON
//! built by `bench/baseline-llm-rag` (`baseline_llm_rag.corpus_external`).
//!
//! Run as: `cargo run -p hyphae-eval --example export_results_from_json
//!   -- --corpus <path>` (or set HYPHAE_CORPUS_PATH env var). Emits
//! the same JSON envelope `export_results.rs` does so the Python
//! scoring path is shared.

use std::env;
use std::fs;
use std::time::Instant;

use hyphae_core::{CognitiveFragment, FragmentContent, FragmentId};
use hyphae_surface::{Intent, RealizationRequest, SurfaceRealizer};
use serde::{Deserialize, Serialize};

/// External corpus query shape — matches the JSON the Python
/// converter writes. Permissive on unknown fields so the same JSON
/// can carry additional provenance (`_source`, etc.) without
/// breaking this loader.
#[derive(Debug, Deserialize)]
struct ExternalQuery {
    id: String,
    query: String,
    intent: String,
    seeds: Vec<ExternalSeed>,
    #[serde(default)]
    #[allow(dead_code)]
    expectations: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ExternalSeed {
    body: String,
    #[serde(default)]
    valence: f32,
    #[serde(default)]
    confabulation_risk: f32,
    #[serde(default)]
    from_cascade: bool,
    #[serde(default)]
    domain_tags: Vec<String>,
}

#[derive(Serialize)]
struct ExportedQuery {
    query_id: String,
    response: String,
    retrieved_chunks: Vec<String>,
    latency_ms: f64,
}

fn parse_intent(s: &str) -> Intent {
    // Match the Rust serde naming used in `seed_corpus_en` exports.
    match s.to_lowercase().as_str() {
        "assert" => Intent::Assert,
        "summarize" | "summary" => Intent::Summarize,
        "compare" => Intent::Compare,
        "reflect" => Intent::Reflect,
        "narrate" => Intent::Narrate,
        _ => Intent::Dialogue, // dialogue is the default — TriviaQA queries map here
    }
}

fn into_fragment(seed: ExternalSeed) -> CognitiveFragment {
    let mut f = CognitiveFragment::new(
        FragmentContent::Observation { body: seed.body },
        "external-corpus",
    );
    f.valence = seed.valence.clamp(-1.0, 1.0);
    f.provenance.confabulation_risk = seed.confabulation_risk.clamp(0.0, 1.0);
    if seed.from_cascade {
        // Synthetic parent id — only that `parent_ids` is non-empty matters
        // for the `ShallowCascade` limitation trigger.
        f.provenance.parent_ids = vec![FragmentId::new()];
    }
    f.domain_tags = seed.domain_tags;
    f
}

fn main() {
    // Args: --corpus <path>, or env HYPHAE_CORPUS_PATH
    let args: Vec<String> = env::args().collect();
    let mut corpus_path: Option<String> = env::var("HYPHAE_CORPUS_PATH").ok();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--corpus" && i + 1 < args.len() {
            corpus_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    let path = corpus_path.expect("usage: --corpus <path> (or HYPHAE_CORPUS_PATH env)");

    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read corpus {path}: {e}"));
    let queries: Vec<ExternalQuery> = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse corpus JSON: {e}"));

    let realizer = SurfaceRealizer::new();
    let mut out: Vec<ExportedQuery> = Vec::with_capacity(queries.len());

    for q in queries {
        let intent = parse_intent(&q.intent);
        let seed_bodies: Vec<String> = q.seeds.iter().map(|s| s.body.clone()).collect();
        let working_set: Vec<CognitiveFragment> =
            q.seeds.into_iter().map(into_fragment).collect();

        // Acknowledgment-only path (empty working set) still goes through
        // the realizer; produces the empty-working-set acknowledgment.
        let t0 = Instant::now();
        let output = realizer
            .realize(&RealizationRequest {
                intent,
                query: &q.query,
                working_set: &working_set,
                ethics: None,
                shape: None,
            })
            .expect("v0.1 realizer maps every intent");
        let latency_ms = t0.elapsed().as_nanos() as f64 / 1_000_000.0;

        out.push(ExportedQuery {
            query_id: q.id,
            response: output.text,
            retrieved_chunks: seed_bodies,
            latency_ms,
        });
    }

    let json = serde_json::to_string_pretty(&out).expect("ExportedQuery derives Serialize");
    println!("{json}");
}
