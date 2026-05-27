// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-eval
//!
//! Honest evaluation harness for Hyphae v2.
//!
//! Per `docs/adr/0001-fresh-from-v1.md` §"Built-but-not-wired" and
//! the v1 `bucket-1-close-report`, the v1 harness shipped a 0.993
//! grammaticality baseline that Atlas flagged as "currently
//! unreliable": the smoke corpus tested friendly-ES queries
//! against EN-only seeds, so the realizer never produced native-ES
//! output the scorer could grade.
//!
//! v2 corrects this by construction:
//!
//! - **EN-only corpus** with native EN seed memories. No friendly-
//!   query-on-foreign-seed artefacts.
//! - **Honest scorer** that catches realiser-class violations the
//!   v1 single-token scorer missed: doubled connectives, missing
//!   acknowledgments, paraphrase.
//! - **Caveats live with the metrics**. The integrator cannot
//!   publish the numbers without also publishing the caveats —
//!   they share a struct ([`EvalReport`]).
//!
//! ## Usage
//!
//! ```ignore
//! use hyphae_eval::{EvalHarness, seed_corpus_en};
//! use hyphae_surface::SurfaceRealizer;
//!
//! let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
//! let report = harness.run();
//! println!("{}", report.render());
//! ```

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod corpus;
pub mod report;
pub mod scorers;

pub use corpus::{Corpus, EvalQuery, EvalSeed, Expectations, seed_corpus_en};
pub use report::{DimensionMeans, EvalReport};
pub use scorers::{QueryScore, score_query};

use hyphae_surface::{RealizationRequest, SurfaceRealizer};

/// The harness. Drives a [`SurfaceRealizer`] against a [`Corpus`]
/// and aggregates the per-query [`QueryScore`]s into an
/// [`EvalReport`].
#[derive(Debug, Clone)]
pub struct EvalHarness {
    realizer: SurfaceRealizer,
    corpus: Corpus,
}

impl EvalHarness {
    /// Construct a harness with the supplied realizer and corpus.
    #[must_use]
    pub fn new(realizer: SurfaceRealizer, corpus: Corpus) -> Self {
        Self { realizer, corpus }
    }

    /// Number of queries in the harness's corpus.
    #[must_use]
    pub fn len(&self) -> usize {
        self.corpus.len()
    }

    /// `true` when the corpus is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.corpus.is_empty()
    }

    /// Run the harness over every query in the corpus and return
    /// the aggregate report.
    #[must_use]
    pub fn run(&self) -> EvalReport {
        let scores: Vec<QueryScore> = self
            .corpus
            .queries()
            .iter()
            .map(|q| self.score_one(q))
            .collect();
        EvalReport::from_scores(scores)
    }

    /// Run a single query and return its score. Useful for
    /// integration tests that want to drill into one query without
    /// running the whole corpus.
    ///
    /// # Panics
    ///
    /// Panics if the realizer cannot produce a composition for the
    /// query's intent. v0.1 maps every [`hyphae_surface::Intent`]
    /// variant, so this branch is reserved for a future intent
    /// addition that the harness has not been updated to cover.
    #[must_use]
    pub fn score_one(&self, query: &EvalQuery) -> QueryScore {
        // Materialise the working set.
        let working_set: Vec<hyphae_core::CognitiveFragment> = query
            .seeds
            .iter()
            .cloned()
            .map(EvalSeed::into_fragment)
            .collect();

        // Realize. The harness uses no ethics report — the eval
        // harness exercises the realizer in isolation. Integration
        // tests at the substrate level cover the ethics-wired path.
        let output = self
            .realizer
            .realize(&RealizationRequest {
                intent: query.intent,
                query: &query.query,
                working_set: &working_set,
                ethics: None,
                shape: None,
            })
            .expect("v0.1 realizer maps every intent");

        score_query(query, &output, self.realizer.lexicon())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_harness_runs_to_completion() {
        let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
        assert!(!harness.is_empty());
        let report = harness.run();
        assert_eq!(report.queries, harness.len());
    }

    #[test]
    fn baseline_harness_does_not_silently_pass_every_query() {
        // The whole point of the v2 harness is that an honest
        // scorer surfaces realizer behaviour — both passes and
        // failures. A "fully passing" baseline report MUST also
        // carry the v1-pattern caveat about the 0.99 ceiling so
        // the integrator does not silently inherit v1's bucket-1
        // greenwashing.
        let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
        let report = harness.run();
        // If every dimension reads above 0.99 the canary caveat
        // must fire; the report itself must carry it.
        if report.means.verbatim_compliance > 0.99
            && report.means.schema_match_rate > 0.99
            && report.means.limitation_recall > 0.99
            && report.means.limitation_precision > 0.99
            && report.means.connective_hygiene_rate > 0.99
            && report.means.acknowledgment_only_rate > 0.99
        {
            assert!(
                !report.caveats.is_empty(),
                "fully passing baseline MUST surface the v1-pattern caveat",
            );
        }
    }

    #[test]
    fn score_one_isolates_a_single_query() {
        let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
        let queries = harness.corpus.queries();
        let first = &queries[0];
        let score = harness.score_one(first);
        assert_eq!(score.query_id, first.id);
    }

    #[test]
    fn empty_corpus_emits_caveat() {
        let harness = EvalHarness::new(SurfaceRealizer::new(), Corpus::new());
        let report = harness.run();
        assert!(report.caveats.iter().any(|c| c.contains("empty corpus")));
    }

    #[test]
    fn baseline_corpus_runs_passing_queries() {
        // Sanity check: the baseline corpus is well-constructed
        // enough that AT LEAST the healthy dialogue queries pass.
        // The harness's pass rate may be < 1.0 (that's OK — honest
        // scoring), but it must not be zero.
        let harness = EvalHarness::new(SurfaceRealizer::new(), seed_corpus_en());
        let report = harness.run();
        assert!(report.passing_queries > 0);
    }
}
