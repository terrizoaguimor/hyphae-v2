// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Scorers — the honest layer that grades a realized composition
//! against the query's expectations.
//!
//! Per ADR-0001 §"Triangulation pre-commit for every foundation
//! milestone" and the bucket-1-close-report's Atlas caveat, v0.1's
//! scorers are designed to **catch the realiser-class violations
//! v1's scorer missed**:
//!
//! - **Verbatim compliance.** The body of every seed marked
//!   `verbatim_quotation = true` must appear in the output text.
//!   This catches paraphrase regressions — the boundary the
//!   no-LLM-in-cognition-path commitment depends on.
//! - **Schema fidelity.** The realizer must have selected the
//!   expected schema; downstream consumers depend on the schema
//!   discriminator.
//! - **Limitation recall + precision.** For each query, every
//!   `must_fire` trigger must appear and no `must_not_fire` trigger
//!   may appear. Asymmetric scoring: a missing required
//!   acknowledgment is worse than a spurious one (per ADR-0003's
//!   RADAR posture — over-acknowledging is conservative, under-
//!   acknowledging is the failure mode that confabulates).
//! - **Connective hygiene.** The output must not contain doubled
//!   connectives (a v1 wave-1 atlas-flagged regression: `"sin
//!   embargo sin embargo"`-style stutters were undetected because
//!   the v1 scorer matched single tokens).
//! - **Acknowledgment-only flag fidelity.** The realizer must mark
//!   `is_acknowledgment_only = true` exactly when the working set
//!   was empty.

use crate::corpus::EvalQuery;
use hyphae_surface::{LimitationTrigger, RealizationOutput};
use serde::{Deserialize, Serialize};

/// Per-query scoring breakdown. Boolean fields are pass/fail for
/// the dimension; the floats give partial credit (limitation
/// recall + precision are fractions). The struct carries more than
/// three booleans because each is a load-bearing pass/fail axis the
/// integrator needs to distinguish — collapsing them into a bitflag
/// set would hide which axis failed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryScore {
    /// Query id this score corresponds to.
    pub query_id: String,
    /// `true` when every seed body marked `verbatim_quotation` was
    /// found in the output text.
    pub verbatim_pass: bool,
    /// `true` when the realizer chose the expected schema.
    pub schema_pass: bool,
    /// Fraction in `[0.0, 1.0]` of expected `must_fire` triggers
    /// that did fire. `1.0` when there are no expectations.
    pub limitation_recall: f32,
    /// Fraction in `[0.0, 1.0]` of `must_not_fire` triggers that
    /// correctly did NOT fire. `1.0` when there are no
    /// expectations.
    pub limitation_precision: f32,
    /// `true` when the output is free of doubled-connective
    /// stutters.
    pub connective_hygiene_pass: bool,
    /// `true` when `is_acknowledgment_only` matches the expectation.
    pub acknowledgment_only_pass: bool,
    /// Per-trigger detail: triggers that were expected but did not
    /// fire.
    pub missing_triggers: Vec<LimitationTrigger>,
    /// Per-trigger detail: triggers that were not expected but
    /// fired.
    pub spurious_triggers: Vec<LimitationTrigger>,
}

impl QueryScore {
    /// `true` when every dimension passes. The overall pass/fail
    /// the integrator surfaces; the per-dimension fields drive the
    /// caveat list.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.verbatim_pass
            && self.schema_pass
            && (self.limitation_recall - 1.0).abs() < f32::EPSILON
            && (self.limitation_precision - 1.0).abs() < f32::EPSILON
            && self.connective_hygiene_pass
            && self.acknowledgment_only_pass
    }
}

/// Score one query's realized output against its expectations.
#[must_use]
pub fn score_query(query: &EvalQuery, output: &RealizationOutput) -> QueryScore {
    let verbatim_pass = if query.expectations.verbatim_quotation {
        query.seeds.iter().all(|s| output.text.contains(&s.body))
    } else {
        true
    };

    let schema_pass = output.schema_used == query.expectations.schema;

    // Limitation recall: did every must_fire trigger actually fire?
    let must_fire = &query.expectations.must_fire;
    let missing_triggers: Vec<LimitationTrigger> = must_fire
        .iter()
        .copied()
        .filter(|t| !output.limitations.contains(t))
        .collect();
    let limitation_recall = if must_fire.is_empty() {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let denom = must_fire.len() as f32;
        #[allow(clippy::cast_precision_loss)]
        let num = (must_fire.len() - missing_triggers.len()) as f32;
        num / denom
    };

    // Limitation precision: did every must_not_fire trigger
    // correctly NOT fire?
    let must_not_fire = &query.expectations.must_not_fire;
    let spurious_triggers: Vec<LimitationTrigger> = must_not_fire
        .iter()
        .copied()
        .filter(|t| output.limitations.contains(t))
        .collect();
    let limitation_precision = if must_not_fire.is_empty() {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let denom = must_not_fire.len() as f32;
        #[allow(clippy::cast_precision_loss)]
        let num = (must_not_fire.len() - spurious_triggers.len()) as f32;
        num / denom
    };

    let connective_hygiene_pass = !has_doubled_connectives(&output.text);

    let acknowledgment_only_pass =
        output.is_acknowledgment_only == query.expectations.acknowledgment_only;

    QueryScore {
        query_id: query.id.clone(),
        verbatim_pass,
        schema_pass,
        limitation_recall,
        limitation_precision,
        connective_hygiene_pass,
        acknowledgment_only_pass,
        missing_triggers,
        spurious_triggers,
    }
}

/// Doubled-connective detector. Catches stutters like
/// `"However, However,"` and `"Extending that, Extending that,"`
/// that a single-token scorer misses.
///
/// The detector lowercases the input, splits on whitespace, and
/// scans for any **multi-word** connective phrase (`"however,"`,
/// `"extending that,"`, etc.) appearing twice in immediate
/// succession. The list is the same set the
/// [`hyphae_surface::Lexicon`] baseline ships — keeping it in sync
/// is the integrator's job.
fn has_doubled_connectives(text: &str) -> bool {
    let lower = text.to_lowercase();
    for phrase in DOUBLED_CHECK_PHRASES {
        let double = format!("{phrase} {phrase}");
        if lower.contains(&double) {
            return true;
        }
    }
    false
}

const DOUBLED_CHECK_PHRASES: &[&str] = &[
    "however,",
    "by contrast,",
    "on the other hand,",
    "extending that,",
    "building on it,",
    "likewise,",
    "drawing from working memory,",
    "the source states:",
    "per the recorded material:",
];

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_surface::SchemaId;

    fn output_with(text: &str, schema: SchemaId, ack_only: bool) -> RealizationOutput {
        RealizationOutput {
            text: text.to_string(),
            schema_used: schema,
            fragments_quoted: Vec::new(),
            limitations: Vec::new(),
            is_acknowledgment_only: ack_only,
        }
    }

    fn output_with_triggers(triggers: Vec<LimitationTrigger>) -> RealizationOutput {
        RealizationOutput {
            text: "Drawing from working memory, \"the deploy succeeded\". \
                   That is the substance available."
                .to_string(),
            schema_used: SchemaId::DialogueReply,
            fragments_quoted: Vec::new(),
            limitations: triggers,
            is_acknowledgment_only: false,
        }
    }

    #[test]
    fn doubled_however_is_detected() {
        let text = "Drawing from working memory, \"a\". However, However, \"b\".";
        assert!(has_doubled_connectives(text));
    }

    #[test]
    fn single_however_is_clean() {
        let text = "Drawing from working memory, \"a\". However, \"b\".";
        assert!(!has_doubled_connectives(text));
    }

    #[test]
    fn doubled_on_the_other_hand_is_detected() {
        let text = "x. on the other hand, on the other hand, y.";
        assert!(has_doubled_connectives(text));
    }

    #[test]
    fn verbatim_pass_when_all_seed_bodies_in_text() {
        use crate::corpus::{EvalSeed, Expectations};
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![EvalSeed {
                body: "alpha bravo charlie".to_string(),
                valence: 0.0,
                confabulation_risk: 0.0,
                from_cascade: true,
            }],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: true,
            },
        };
        let out = output_with(
            "...\"alpha bravo charlie\"...",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out);
        assert!(score.verbatim_pass);
    }

    #[test]
    fn verbatim_fail_when_seed_body_missing() {
        use crate::corpus::{EvalSeed, Expectations};
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![EvalSeed {
                body: "specific phrase that must appear".to_string(),
                valence: 0.0,
                confabulation_risk: 0.0,
                from_cascade: true,
            }],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: true,
            },
        };
        let out = output_with(
            "the output paraphrased it away",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out);
        assert!(!score.verbatim_pass);
    }

    #[test]
    fn limitation_recall_one_when_no_must_fire() {
        use crate::corpus::Expectations;
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        };
        let out = output_with("", SchemaId::DialogueReply, true);
        let score = score_query(&q, &out);
        assert!((score.limitation_recall - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn limitation_recall_one_when_all_must_fire_present() {
        use crate::corpus::Expectations;
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![LimitationTrigger::EmptyWorkingSet],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        };
        let mut out = output_with(
            "[limitation:empty_working_set]",
            SchemaId::DialogueReply,
            true,
        );
        out.limitations = vec![LimitationTrigger::EmptyWorkingSet];
        let score = score_query(&q, &out);
        assert!((score.limitation_recall - 1.0).abs() < f32::EPSILON);
        assert!(score.missing_triggers.is_empty());
    }

    #[test]
    fn limitation_recall_zero_when_missing() {
        use crate::corpus::Expectations;
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![LimitationTrigger::EmptyWorkingSet],
                must_not_fire: vec![],
                acknowledgment_only: true,
                verbatim_quotation: false,
            },
        };
        let out = output_with_triggers(vec![]);
        let score = score_query(&q, &out);
        assert!((score.limitation_recall - 0.0).abs() < f32::EPSILON);
        assert_eq!(
            score.missing_triggers,
            vec![LimitationTrigger::EmptyWorkingSet]
        );
    }

    #[test]
    fn limitation_precision_drops_when_spurious_trigger_fires() {
        use crate::corpus::Expectations;
        let q = EvalQuery {
            id: "t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds: vec![],
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![LimitationTrigger::HighConfabRisk],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        };
        let out = output_with_triggers(vec![LimitationTrigger::HighConfabRisk]);
        let score = score_query(&q, &out);
        assert!((score.limitation_precision - 0.0).abs() < f32::EPSILON);
        assert_eq!(
            score.spurious_triggers,
            vec![LimitationTrigger::HighConfabRisk]
        );
    }
}
