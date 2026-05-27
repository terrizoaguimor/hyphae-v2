// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Aggregate report — what the harness emits at the end of a run.
//!
//! Per ADR-0001 §"Context" and the v1 bucket-1-close-report:
//! reports surface **honest caveats** alongside metrics. v1's
//! wave-1 bucket close shipped a 0.993 grammaticality baseline that
//! Atlas had to flag as "optimistically biased"; the realiser was
//! green because the scorer could not detect the violations the
//! corpus would have exposed.
//!
//! v2's report **carries the caveats inline**. The integrator
//! cannot publish the metrics without also publishing the caveats —
//! they share a struct.

use crate::scorers::QueryScore;
use serde::{Deserialize, Serialize};

/// Aggregate over a corpus run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    /// Number of queries evaluated.
    pub queries: usize,
    /// Number of queries whose every dimension passed.
    pub passing_queries: usize,
    /// Per-dimension mean scores, in `[0.0, 1.0]`.
    pub means: DimensionMeans,
    /// Per-query scores. Surfaced so the integrator can drill into
    /// failures without re-running.
    pub query_scores: Vec<QueryScore>,
    /// Caveats the harness attaches to this run. The integrator
    /// MUST publish these alongside the metrics — the v2
    /// correction of v1's silent caveat suppression.
    pub caveats: Vec<String>,
}

/// Means across the corpus, per scoring dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionMeans {
    /// Fraction of queries passing the verbatim-quotation check.
    pub verbatim_compliance: f32,
    /// Fraction of queries where the realizer selected the
    /// expected schema.
    pub schema_match_rate: f32,
    /// Mean limitation recall across all queries.
    pub limitation_recall: f32,
    /// Mean limitation precision across all queries.
    pub limitation_precision: f32,
    /// Fraction of queries with no doubled connectives.
    pub connective_hygiene_rate: f32,
    /// Fraction of queries with the correct
    /// `is_acknowledgment_only` flag.
    pub acknowledgment_only_rate: f32,
}

impl EvalReport {
    /// Build a report from per-query scores. Computes per-dimension
    /// means and per-run caveats.
    #[must_use]
    pub fn from_scores(query_scores: Vec<QueryScore>) -> Self {
        let queries = query_scores.len();
        let passing_queries = query_scores.iter().filter(|s| s.passes()).count();
        let means = DimensionMeans::from_scores(&query_scores);
        let caveats = build_caveats(&query_scores, &means);
        Self {
            queries,
            passing_queries,
            means,
            query_scores,
            caveats,
        }
    }

    /// Overall pass rate as a fraction in `[0.0, 1.0]`.
    #[must_use]
    pub fn pass_rate(&self) -> f32 {
        if self.queries == 0 {
            return 1.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let result = self.passing_queries as f32 / self.queries as f32;
        result
    }

    /// `true` when every query passed every dimension. The strict
    /// criterion the integrator uses to decide whether a release
    /// is publishable.
    #[must_use]
    pub fn fully_passing(&self) -> bool {
        self.queries > 0 && self.passing_queries == self.queries
    }

    /// Render a human-readable summary. Surfaces caveats verbatim
    /// at the top — they are not appendix material.
    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        writeln!(out, "Hyphae v2 eval report").ok();
        writeln!(out, "═════════════════════").ok();
        writeln!(
            out,
            "queries={}  passing={}  pass_rate={:.3}",
            self.queries,
            self.passing_queries,
            self.pass_rate(),
        )
        .ok();
        writeln!(out).ok();
        writeln!(
            out,
            "verbatim_compliance     = {:.3}",
            self.means.verbatim_compliance
        )
        .ok();
        writeln!(
            out,
            "schema_match_rate       = {:.3}",
            self.means.schema_match_rate
        )
        .ok();
        writeln!(
            out,
            "limitation_recall       = {:.3}",
            self.means.limitation_recall
        )
        .ok();
        writeln!(
            out,
            "limitation_precision    = {:.3}",
            self.means.limitation_precision
        )
        .ok();
        writeln!(
            out,
            "connective_hygiene_rate = {:.3}",
            self.means.connective_hygiene_rate
        )
        .ok();
        writeln!(
            out,
            "acknowledgment_only     = {:.3}",
            self.means.acknowledgment_only_rate
        )
        .ok();
        if !self.caveats.is_empty() {
            writeln!(out).ok();
            writeln!(out, "── Honest caveats (per ADR-0001) ──").ok();
            for c in &self.caveats {
                writeln!(out, "  • {c}").ok();
            }
        }
        out
    }
}

impl DimensionMeans {
    fn from_scores(scores: &[QueryScore]) -> Self {
        if scores.is_empty() {
            return Self {
                verbatim_compliance: 1.0,
                schema_match_rate: 1.0,
                limitation_recall: 1.0,
                limitation_precision: 1.0,
                connective_hygiene_rate: 1.0,
                acknowledgment_only_rate: 1.0,
            };
        }
        #[allow(clippy::cast_precision_loss)]
        let n = scores.len() as f32;
        let verbatim = scores.iter().filter(|s| s.verbatim_pass).count();
        let schema = scores.iter().filter(|s| s.schema_pass).count();
        let connective = scores.iter().filter(|s| s.connective_hygiene_pass).count();
        let ack_only = scores.iter().filter(|s| s.acknowledgment_only_pass).count();
        let recall_sum: f32 = scores.iter().map(|s| s.limitation_recall).sum();
        let precision_sum: f32 = scores.iter().map(|s| s.limitation_precision).sum();
        #[allow(clippy::cast_precision_loss)]
        Self {
            verbatim_compliance: verbatim as f32 / n,
            schema_match_rate: schema as f32 / n,
            limitation_recall: recall_sum / n,
            limitation_precision: precision_sum / n,
            connective_hygiene_rate: connective as f32 / n,
            acknowledgment_only_rate: ack_only as f32 / n,
        }
    }
}

/// Build the caveat list. Caveats fire when a dimension reads
/// suspiciously high or shows a pattern that v1's wave-1 baseline
/// exhibited.
fn build_caveats(scores: &[QueryScore], means: &DimensionMeans) -> Vec<String> {
    let mut caveats = Vec::new();

    if scores.is_empty() {
        caveats.push(
            "empty corpus — the report's means default to 1.0 but no realizer behaviour was \
             observed; do NOT publish these numbers as evidence of correctness"
                .to_string(),
        );
        return caveats;
    }

    // v1's wave-1 close shipped 0.993 because the scorer could not
    // detect the violations the corpus would have exposed. v2's
    // canary: when every metric reads above 0.99 simultaneously,
    // surface the caveat so the integrator does not silently inherit
    // the v1 pattern.
    let very_high = |m: f32| m > 0.99;
    if very_high(means.verbatim_compliance)
        && very_high(means.schema_match_rate)
        && very_high(means.limitation_recall)
        && very_high(means.limitation_precision)
        && very_high(means.connective_hygiene_rate)
        && very_high(means.acknowledgment_only_rate)
    {
        caveats.push(
            "every dimension reads above 0.99 — v1's wave-1 baseline (0.993) exhibited the same \
             shape because the scorer could not see realiser-class violations; verify the corpus \
             actually exercises the realizer's failure modes (paraphrase, missed acknowledgment, \
             doubled connectives) before publishing this as evidence of competence"
                .to_string(),
        );
    }

    // Limitation precision matters more than recall in RADAR — a
    // missing acknowledgment is a confabulation; a spurious one is
    // a conservative overstatement. Surface the asymmetry when
    // precision drops.
    if means.limitation_precision < 0.95 {
        caveats.push(format!(
            "limitation precision {:.3} below 0.95 — spurious acknowledgments are firing; \
             tune limitation thresholds before publishing",
            means.limitation_precision,
        ));
    }
    if means.limitation_recall < 0.90 {
        caveats.push(format!(
            "limitation recall {:.3} below 0.90 — REQUIRED acknowledgments are missing; \
             this is the confabulation failure mode the architecture is designed to prevent. \
             Do NOT publish",
            means.limitation_recall,
        ));
    }

    // Verbatim is the boundary the no-LLM-in-cognition-path
    // commitment depends on. A single failure is load-bearing.
    let verbatim_failures = scores.iter().filter(|s| !s.verbatim_pass).count();
    if verbatim_failures > 0 {
        caveats.push(format!(
            "verbatim quotation failed in {verbatim_failures} query/queries — the realizer \
             paraphrased; the no-LLM-in-cognition-path commitment is at risk. Investigate \
             before any publication",
        ));
    }

    // Connective hygiene failures suggest the lexicon pick logic
    // is producing stutters — a v1 wave-1 atlas-flagged regression
    // pattern.
    let hygiene_failures = scores.iter().filter(|s| !s.connective_hygiene_pass).count();
    if hygiene_failures > 0 {
        caveats.push(format!(
            "connective-hygiene failed in {hygiene_failures} query/queries (doubled connectives \
             detected) — v1 wave-1 had the same regression pattern; tune the lexicon pick logic",
        ));
    }

    caveats
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_surface::LimitationTrigger;

    fn perfect_score(id: &str) -> QueryScore {
        QueryScore {
            query_id: id.to_string(),
            verbatim_pass: true,
            schema_pass: true,
            limitation_recall: 1.0,
            limitation_precision: 1.0,
            connective_hygiene_pass: true,
            acknowledgment_only_pass: true,
            missing_triggers: Vec::new(),
            spurious_triggers: Vec::new(),
        }
    }

    fn failing_score(id: &str) -> QueryScore {
        QueryScore {
            query_id: id.to_string(),
            verbatim_pass: false,
            schema_pass: true,
            limitation_recall: 0.5,
            limitation_precision: 0.5,
            connective_hygiene_pass: true,
            acknowledgment_only_pass: true,
            missing_triggers: vec![LimitationTrigger::EmptyWorkingSet],
            spurious_triggers: vec![LimitationTrigger::HighConfabRisk],
        }
    }

    #[test]
    fn fully_passing_report_with_perfect_scores_emits_caveat() {
        let scores = vec![perfect_score("a"), perfect_score("b"), perfect_score("c")];
        let report = EvalReport::from_scores(scores);
        assert!(report.fully_passing());
        // The "everything above 0.99" canary must fire — this is
        // exactly the v1 wave-1 pattern.
        assert!(
            !report.caveats.is_empty(),
            "perfect scores must trigger the v1 0.99-canary caveat",
        );
        assert!(
            report.caveats.iter().any(|c| c.contains("0.99")),
            "caveat must reference the 0.99 threshold",
        );
    }

    #[test]
    fn mixed_report_pass_rate_is_correct() {
        let scores = vec![perfect_score("a"), failing_score("b"), perfect_score("c")];
        let report = EvalReport::from_scores(scores);
        assert_eq!(report.queries, 3);
        assert_eq!(report.passing_queries, 2);
        assert!((report.pass_rate() - 2.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn verbatim_failure_surfaces_no_llm_commitment_caveat() {
        let scores = vec![failing_score("a")];
        let report = EvalReport::from_scores(scores);
        assert!(
            report
                .caveats
                .iter()
                .any(|c| c.contains("no-LLM-in-cognition-path")),
            "verbatim failure must surface the no-LLM commitment caveat",
        );
    }

    #[test]
    fn empty_corpus_caveat_fires() {
        let report = EvalReport::from_scores(Vec::new());
        assert!(
            report.caveats.iter().any(|c| c.contains("empty corpus")),
            "empty corpus must emit a 'do NOT publish' caveat",
        );
    }

    #[test]
    fn low_limitation_recall_emits_critical_caveat() {
        // A run where required acknowledgments are missing.
        let mut s = perfect_score("a");
        s.limitation_recall = 0.5;
        s.missing_triggers = vec![LimitationTrigger::EmptyWorkingSet];
        let report = EvalReport::from_scores(vec![s]);
        assert!(
            report.caveats.iter().any(|c| c.contains("Do NOT publish")),
            "low recall must emit the do-not-publish caveat",
        );
    }

    #[test]
    fn render_includes_caveats_section() {
        let scores = vec![perfect_score("a")];
        let report = EvalReport::from_scores(scores);
        let rendered = report.render();
        assert!(
            rendered.contains("Honest caveats"),
            "rendered report must include caveats section",
        );
    }
}
