// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Export the EN eval corpus as JSON.
//!
//! Run via `cargo run -p hyphae-eval --example export_corpus`.
//! Emits the corpus from [`hyphae_eval::seed_corpus_en`] to stdout.
//!
//! Per ADR-0027 §"Same corpus, same input semantics", the LLM+RAG
//! comparator in `bench/baseline-llm-rag/` consumes this JSON as its
//! single source of truth for queries and seed bodies. Never
//! duplicate the corpus in Python source.

use hyphae_eval::seed_corpus_en;

fn main() {
    let corpus = seed_corpus_en();
    let queries = corpus.queries();
    let json = serde_json::to_string_pretty(queries)
        .expect("EvalQuery derives Serialize and seeds are owned strings — never fails");
    println!("{json}");
}
