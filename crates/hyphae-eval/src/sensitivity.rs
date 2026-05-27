// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Scorer sensitivity audit — per ADR-0010.
//!
//! Proves that each scoring dimension defined in
//! [`crate::scorers`] actually detects the violation it is
//! designed to detect. The audit is **deterministic**: same
//! baseline + same mutation + same lexicon = same result across
//! runs.
//!
//! Without this audit, the ADR-0008 v1-pattern canary fires
//! whenever every dimension reads above 0.99 — and the
//! integrator cannot tell whether the run is healthy (correct
//! realizer + well-designed corpus + silent scorers) or
//! unreliable (correct realizer + corpus that produces
//! violations the scorers fail to detect, exactly the v1 wave-1
//! pattern).
//!
//! ## Mutation discipline
//!
//! Mutations apply to the [`RealizationOutput`] struct
//! **post-hoc**. The realizer is never touched — it is
//! deterministically correct by construction
//! (ADR-0001 §"Hard architectural commitments"). The audit only
//! validates that the scorers see what they claim to see.

use crate::corpus::{EvalQuery, EvalSeed, Expectations};
use crate::scorers::score_query;
use hyphae_surface::{Intent, Lexicon, LimitationTrigger, RealizationOutput, SchemaId};
use serde::{Deserialize, Serialize};

/// One sensitivity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityResult {
    /// The scorer dimension under test.
    pub dimension: String,
    /// Short human-readable description of the mutation applied.
    pub mutation: String,
    /// `true` when the baseline passes the dimension.
    pub baseline_passed: bool,
    /// `true` when the mutated output also passes the dimension
    /// (i.e. the mutation was NOT detected — bad).
    pub mutated_passed: bool,
    /// `true` when the scorer detected the mutation
    /// (`baseline_passed && !mutated_passed`). This is the
    /// dimension's sensitivity verdict.
    pub detected: bool,
}

/// Aggregate over all sensitivity checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityReport {
    /// Per-dimension verdicts.
    pub results: Vec<SensitivityResult>,
}

impl SensitivityReport {
    /// `true` when every dimension's mutation was detected.
    #[must_use]
    pub fn all_dimensions_sensitive(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.detected)
    }

    /// Number of dimensions whose mutation was detected.
    #[must_use]
    pub fn dimensions_sensitive(&self) -> usize {
        self.results.iter().filter(|r| r.detected).count()
    }

    /// Total dimensions audited.
    #[must_use]
    pub fn dimensions_total(&self) -> usize {
        self.results.len()
    }

    /// Names of dimensions that failed to detect their mutation.
    #[must_use]
    pub fn failing_dimensions(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|r| !r.detected)
            .map(|r| r.dimension.as_str())
            .collect()
    }
}

/// Run the full sensitivity audit. Returns one
/// [`SensitivityResult`] per scored dimension.
///
/// The audit is independent of the realizer — it only exercises
/// the scorer's response to mutated outputs.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_sensitivity_audit(lexicon: &Lexicon) -> SensitivityReport {
    let mut results = Vec::new();

    // Each check follows the same pattern:
    //   1. Build a baseline (query, output) pair.
    //   2. Score it; assert the targeted dimension passes.
    //   3. Mutate the output.
    //   4. Score the mutation; assert the targeted dimension fails.
    //   5. Record the verdict.

    // ── verbatim_compliance ───────────────────────────────────
    {
        let query = simple_dialogue_query(
            "verbatim-check",
            vec![EvalSeed {
                body: "the deploy succeeded on the first attempt".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
            }],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: true,
            },
        );
        let baseline = sample_output(
            "Per the recorded fragments, \"the deploy succeeded on the first attempt\". \
             That is the substrate's current view.",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "Per the recorded fragments, the deploy was paraphrased away. \
             That is the substrate's current view.",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        results.push(verdict(
            "verbatim_compliance",
            "body replaced with paraphrase in output text",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.verbatim_pass,
        ));
    }

    // ── schema_match_rate ─────────────────────────────────────
    {
        let query = simple_dialogue_query(
            "schema-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![LimitationTrigger::EmptyWorkingSet],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "[limitation:empty_working_set]",
            SchemaId::DialogueReply,
            vec![LimitationTrigger::EmptyWorkingSet],
            true,
        );
        let mutated = sample_output(
            "[limitation:empty_working_set]",
            SchemaId::GroundedAssertion,
            vec![LimitationTrigger::EmptyWorkingSet],
            true,
        );
        results.push(verdict(
            "schema_match_rate",
            "schema_used swapped to a different SchemaId",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.schema_pass,
        ));
    }

    // ── limitation_recall ─────────────────────────────────────
    {
        let query = simple_dialogue_query(
            "recall-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![LimitationTrigger::EmptyWorkingSet],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "[limitation:empty_working_set]",
            SchemaId::DialogueReply,
            vec![LimitationTrigger::EmptyWorkingSet],
            true,
        );
        let mutated = sample_output("", SchemaId::DialogueReply, vec![], true);
        results.push(verdict(
            "limitation_recall",
            "required EmptyWorkingSet acknowledgment removed",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| (s.limitation_recall - 1.0).abs() < f32::EPSILON,
        ));
    }

    // ── limitation_precision ──────────────────────────────────
    {
        let query = simple_dialogue_query(
            "precision-check",
            vec![EvalSeed {
                body: "the deploy succeeded".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
            }],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![LimitationTrigger::HighConfabRisk],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "Per the recorded fragments, \"the deploy succeeded\".",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "Per the recorded fragments, \"the deploy succeeded\". \
             [limitation:high_confab_risk]",
            SchemaId::DialogueReply,
            vec![LimitationTrigger::HighConfabRisk],
            false,
        );
        results.push(verdict(
            "limitation_precision",
            "spurious HighConfabRisk acknowledgment injected",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| (s.limitation_precision - 1.0).abs() < f32::EPSILON,
        ));
    }

    // ── connective_hygiene_rate ───────────────────────────────
    {
        let query = simple_dialogue_query(
            "hygiene-check",
            vec![EvalSeed {
                body: "a".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
            }],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "Drawing from working memory, \"a\". However, \"b\".",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "Drawing from working memory, \"a\". However, However, \"b\".",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        results.push(verdict(
            "connective_hygiene_rate",
            "doubled `However,` connective injected",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.connective_hygiene_pass,
        ));
    }

    // ── acknowledgment_only_rate ──────────────────────────────
    {
        let query = simple_dialogue_query(
            "ack-only-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![LimitationTrigger::EmptyWorkingSet],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "[limitation:empty_working_set]",
            SchemaId::DialogueReply,
            vec![LimitationTrigger::EmptyWorkingSet],
            true,
        );
        let mutated = sample_output(
            "[limitation:empty_working_set]",
            SchemaId::DialogueReply,
            vec![LimitationTrigger::EmptyWorkingSet],
            false,
        );
        results.push(verdict(
            "acknowledgment_only_rate",
            "is_acknowledgment_only flag flipped",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.acknowledgment_only_pass,
        ));
    }

    // ── lexical_diversity ─────────────────────────────────────
    {
        let query = simple_dialogue_query(
            "diversity-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "Drawing from working memory, \"a\". However, \"b\". \
             That is the substance available.",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "However, \"a\". However, \"b\". However, \"c\".",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        results.push(verdict(
            "lexical_diversity",
            "three distinct phrases replaced with one repeated phrase",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.lexical_diversity > 0.95,
        ));
    }

    // ── role_coverage ─────────────────────────────────────────
    {
        let query = simple_dialogue_query(
            "role-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "Drawing from working memory, \"a\". However, \"b\". \
             That is the substance available.",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "However, \"a\". However, \"b\". However, \"c\".",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        results.push(verdict(
            "role_coverage",
            "three distinct roles replaced with one repeated role",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.role_coverage > 0.95,
        ));
    }

    // ── boundary_smoothness ───────────────────────────────────
    {
        let query = simple_dialogue_query(
            "smoothness-check",
            vec![],
            Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        );
        let baseline = sample_output(
            "\"the deploy succeeded\" However, \"the migration completed at 14:02\"",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        let mutated = sample_output(
            "\"the deploy succeeded\" Building on it, \"the migration completed at 14:02\"",
            SchemaId::DialogueReply,
            vec![],
            false,
        );
        results.push(verdict(
            "boundary_smoothness",
            "anaphor-before-definite-determiner Rule-1 violation introduced",
            &query,
            &baseline,
            &mutated,
            lexicon,
            |s| s.boundary_smoothness > 0.95,
        ));
    }

    SensitivityReport { results }
}

/// Build a minimal `EvalQuery` with the given seeds + expectations.
fn simple_dialogue_query(id: &str, seeds: Vec<EvalSeed>, expectations: Expectations) -> EvalQuery {
    EvalQuery {
        id: id.to_string(),
        query: "sensitivity audit".to_string(),
        intent: Intent::Dialogue,
        seeds,
        expectations,
    }
}

/// Build a minimal `RealizationOutput`.
fn sample_output(
    text: &str,
    schema: SchemaId,
    limitations: Vec<LimitationTrigger>,
    ack_only: bool,
) -> RealizationOutput {
    RealizationOutput {
        text: text.to_string(),
        schema_used: schema,
        fragments_quoted: Vec::new(),
        limitations,
        is_acknowledgment_only: ack_only,
    }
}

/// Score baseline + mutated outputs against the same query and
/// record the verdict using a dimension-extraction closure.
fn verdict<F>(
    dimension: &str,
    mutation: &str,
    query: &EvalQuery,
    baseline: &RealizationOutput,
    mutated: &RealizationOutput,
    lexicon: &Lexicon,
    passes: F,
) -> SensitivityResult
where
    F: Fn(&crate::scorers::QueryScore) -> bool,
{
    let baseline_score = score_query(query, baseline, lexicon);
    let mutated_score = score_query(query, mutated, lexicon);
    let baseline_passed = passes(&baseline_score);
    let mutated_passed = passes(&mutated_score);
    SensitivityResult {
        dimension: dimension.to_string(),
        mutation: mutation.to_string(),
        baseline_passed,
        mutated_passed,
        detected: baseline_passed && !mutated_passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_runs_to_completion_on_baseline_lexicon() {
        let lex = Lexicon::baseline_en();
        let report = run_sensitivity_audit(&lex);
        assert!(
            !report.results.is_empty(),
            "audit must emit at least one result"
        );
        // We expect exactly nine — the ADR-0008 + ADR-0010
        // dimension table.
        assert_eq!(
            report.dimensions_total(),
            9,
            "ADR-0010 covers 9 scored dimensions",
        );
    }

    #[test]
    fn every_dimension_is_sensitive() {
        let lex = Lexicon::baseline_en();
        let report = run_sensitivity_audit(&lex);
        let failing = report.failing_dimensions();
        assert!(
            failing.is_empty(),
            "every dimension must detect its mutation; failing: {failing:?}",
        );
        assert!(report.all_dimensions_sensitive());
    }

    #[test]
    fn report_dimensions_sensitive_count_is_correct() {
        let lex = Lexicon::baseline_en();
        let report = run_sensitivity_audit(&lex);
        assert_eq!(report.dimensions_sensitive(), report.dimensions_total());
    }
}
