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
use hyphae_surface::{
    BoundarySignal, ConnectiveRole, Lexicon, LimitationTrigger, RealizationOutput, should_exclude,
};
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
    /// **ADR-0008.** Fraction in `[0.0, 1.0]` of emitted connective
    /// phrases that are distinct (no repetition). `1.0` when each
    /// phrase appears once OR no lexicon phrases were detected
    /// (acknowledgment-only paths).
    #[serde(default = "default_unit_score")]
    pub lexical_diversity: f32,
    /// **ADR-0008.** Fraction in `[0.0, 1.0]` of distinct connective
    /// roles invoked over the phrase emissions, capped at 10 (the
    /// taxonomy size from ADR-0005). `1.0` when role count meets
    /// or exceeds emission count, or when no phrases were detected.
    #[serde(default = "default_unit_score")]
    pub role_coverage: f32,
    /// **ADR-0008.** Fraction in `[0.0, 1.0]` of adjacent
    /// quote-quote boundaries whose connective complies with
    /// ADR-0007 smoothing rules. `1.0` when no boundaries were
    /// detected OR every boundary passes Rule 1 + Rule 3.
    #[serde(default = "default_unit_score")]
    pub boundary_smoothness: f32,
    /// **ADR-0008.** Distinct lexicon phrases detected in this
    /// query's output, surfaced for corpus-wide aggregation.
    #[serde(default)]
    pub phrases_detected: Vec<String>,
    /// **ADR-0008.** Distinct connective roles detected in this
    /// query's output. Surfaced for diagnostic reporting; the
    /// integrator uses this to spot a picker stuck in one role.
    #[serde(default)]
    pub roles_detected: Vec<ConnectiveRole>,
}

#[must_use]
const fn default_unit_score() -> f32 {
    1.0
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
///
/// `lexicon` is consulted post-hoc for ADR-0008 fluency dimensions
/// (`lexical_diversity`, `role_coverage`, `boundary_smoothness`).
/// Pass the realizer's own lexicon for accurate matching;
/// mismatched lexica will under-count phrases and inflate
/// "no-detection-so-1.0" trivial passes.
#[must_use]
pub fn score_query(query: &EvalQuery, output: &RealizationOutput, lexicon: &Lexicon) -> QueryScore {
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

    // ADR-0008 fluency dimensions.
    let matches = detect_phrases(&output.text, lexicon);
    let lexical_diversity = compute_lexical_diversity(&matches);
    let role_coverage = compute_role_coverage(&matches);
    let boundary_smoothness = compute_boundary_smoothness(&output.text, lexicon);
    let mut phrases_detected: Vec<String> = matches.iter().map(|m| m.phrase.clone()).collect();
    phrases_detected.sort();
    phrases_detected.dedup();
    let mut roles_detected: Vec<ConnectiveRole> = matches.iter().map(|m| m.role).collect();
    roles_detected.sort_by_key(|r| format!("{r:?}"));
    roles_detected.dedup();

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
        lexical_diversity,
        role_coverage,
        boundary_smoothness,
        phrases_detected,
        roles_detected,
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

/// One non-overlapping phrase match in the rendered output.
#[derive(Debug, Clone)]
struct PhraseMatch {
    /// The matched phrase, lowercased (the form actually compared).
    phrase: String,
    /// The connective's role in the lexicon.
    role: ConnectiveRole,
    /// Byte offset in the lowercased output where the match starts.
    start: usize,
    /// Length of the match in bytes.
    len: usize,
}

/// Detect non-overlapping lexicon phrases in the output text.
///
/// Algorithm: longest-match-wins. For every `(phrase, role)` in the
/// lexicon, find every occurrence in the lowercased text. Sort all
/// candidate matches by phrase length descending. Walk through;
/// claim each match whose region does not overlap a previously
/// claimed match. Returns matches in textual order.
///
/// This avoids the substring-overlap inflation a naive
/// `contains` count produces (e.g. "On the other," is a prefix of
/// "On the other hand,").
fn detect_phrases(text: &str, lexicon: &Lexicon) -> Vec<PhraseMatch> {
    let lower = text.to_lowercase();
    let mut candidates: Vec<PhraseMatch> = Vec::new();
    for entry in lexicon.entries() {
        let phrase = entry.phrase.to_lowercase();
        if phrase.is_empty() {
            continue;
        }
        let mut search_from = 0;
        while let Some(rel) = lower[search_from..].find(&phrase) {
            let start = search_from + rel;
            candidates.push(PhraseMatch {
                phrase: phrase.clone(),
                role: entry.role,
                start,
                len: phrase.len(),
            });
            search_from = start + phrase.len();
        }
    }
    // Longest matches win on overlap.
    candidates.sort_by(|a, b| b.len.cmp(&a.len).then(a.start.cmp(&b.start)));
    let mut claimed: Vec<(usize, usize)> = Vec::new();
    let mut kept: Vec<PhraseMatch> = Vec::new();
    for m in candidates {
        let m_end = m.start + m.len;
        let overlaps = claimed.iter().any(|(s, e)| m.start < *e && m_end > *s);
        if !overlaps {
            claimed.push((m.start, m_end));
            kept.push(m);
        }
    }
    kept.sort_by_key(|m| m.start);
    kept
}

/// Compute `lexical_diversity` per ADR-0008.
///
/// `1.0` when no phrases detected or every phrase distinct.
/// `0.0` when only one phrase is repeated across all emissions.
fn compute_lexical_diversity(matches: &[PhraseMatch]) -> f32 {
    if matches.is_empty() {
        return 1.0;
    }
    let mut distinct: Vec<&str> = matches.iter().map(|m| m.phrase.as_str()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    #[allow(clippy::cast_precision_loss)]
    let result = distinct.len() as f32 / matches.len() as f32;
    result.clamp(0.0, 1.0)
}

/// Compute `role_coverage` per ADR-0008.
///
/// `1.0` when distinct-roles ≥ min(emissions, 10) (the role
/// taxonomy size from ADR-0005). `1.0` trivially when no phrases
/// detected — single-phrase acknowledgments pass.
fn compute_role_coverage(matches: &[PhraseMatch]) -> f32 {
    if matches.is_empty() {
        return 1.0;
    }
    let mut distinct_roles: Vec<ConnectiveRole> = matches.iter().map(|m| m.role).collect();
    distinct_roles.sort_by_key(|r| format!("{r:?}"));
    distinct_roles.dedup();
    let denom = matches.len().min(10);
    #[allow(clippy::cast_precision_loss)]
    let result = distinct_roles.len() as f32 / denom as f32;
    result.clamp(0.0, 1.0)
}

/// Compute `boundary_smoothness` per ADR-0008.
///
/// Walks the output text quote-by-quote, extracts the
/// inter-quote connective slice, and asks the lexicon's
/// `should_exclude` predicate whether the slice's matching phrase
/// would have been filtered if the smoothing had been applied
/// retroactively. Returns the fraction of boundaries that comply.
///
/// `1.0` when there are no `"…" … "…"` adjacent-quote boundaries
/// (single-fragment outputs, acknowledgment-only outputs) OR every
/// boundary's connective passes Rule 1 + Rule 3.
fn compute_boundary_smoothness(text: &str, lexicon: &Lexicon) -> f32 {
    // Collect (body, end_pos) pairs for each double-quoted span.
    let mut quotes: Vec<(String, usize, usize)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            if j < bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[start..j]) {
                    quotes.push((s.to_string(), start, j));
                }
                i = j + 1;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    if quotes.len() < 2 {
        return 1.0;
    }

    let mut violations = 0_usize;
    let mut boundaries = 0_usize;
    for window in quotes.windows(2) {
        let (prev_body, _ps, pe) = &window[0];
        let (next_body, ns, _ne) = &window[1];
        // The inter-quote slice is the text between the closing
        // quote of `prev` and the opening quote of `next`, minus
        // the bounding `"` characters.
        let between = &text[*pe + 1..*ns - 1];
        boundaries += 1;
        let prev_sig = BoundarySignal::extract(prev_body);
        let next_sig = BoundarySignal::extract(next_body);
        // Find which lexicon phrase appears in `between`. If
        // multiple match (Rule 3 + Rule 1 can coexist with one
        // phrase), the first matched one drives the verdict.
        let lower_between = between.to_lowercase();
        for entry in lexicon.entries() {
            let phrase = entry.phrase.to_lowercase();
            if phrase.is_empty() {
                continue;
            }
            if lower_between.contains(&phrase) && should_exclude(entry, &prev_sig, &next_sig) {
                violations += 1;
                break;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let result = 1.0 - (violations as f32 / boundaries as f32);
    result.clamp(0.0, 1.0)
}

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
                domain_tags: Vec::new(),
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
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
                domain_tags: Vec::new(),
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.limitation_recall - 0.0).abs() < f32::EPSILON);
        assert_eq!(
            score.missing_triggers,
            vec![LimitationTrigger::EmptyWorkingSet]
        );
    }

    // ── ADR-0008 fluency dimensions ────────────────────────────

    fn dialogue_query_with_seeds(seeds: Vec<crate::corpus::EvalSeed>) -> EvalQuery {
        use crate::corpus::Expectations;
        EvalQuery {
            id: "fluency-t".to_string(),
            query: "?".to_string(),
            intent: hyphae_surface::Intent::Dialogue,
            seeds,
            expectations: Expectations {
                schema: SchemaId::DialogueReply,
                must_fire: vec![],
                must_not_fire: vec![],
                acknowledgment_only: false,
                verbatim_quotation: false,
            },
        }
    }

    #[test]
    fn detect_phrases_matches_lexicon_entries() {
        let lex = Lexicon::baseline_en();
        let text = "Drawing from working memory, \"a\". However, \"b\". \
                    That is the substance available.";
        let matches = detect_phrases(text, &lex);
        // At minimum the opening + the contrast + the closing are
        // baseline lexicon phrases; the matcher must surface all
        // three.
        assert!(
            matches.len() >= 3,
            "expected ≥3 lexicon phrases, got {}",
            matches.len(),
        );
    }

    #[test]
    fn detect_phrases_prefers_longest_match() {
        let lex = Lexicon::baseline_en();
        // "On the other hand," is a baseline phrase; "On the
        // other" is not (no comma) so this just verifies the
        // longest-match-wins logic doesn't double-count.
        let text = "x. On the other hand, y.";
        let matches = detect_phrases(text, &lex);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].phrase, "on the other hand,");
    }

    #[test]
    fn lexical_diversity_is_one_when_no_phrases_detected() {
        let q = dialogue_query_with_seeds(vec![]);
        let out = output_with(
            "plain text without lexicon phrases",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.lexical_diversity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lexical_diversity_drops_with_repeated_phrase() {
        let q = dialogue_query_with_seeds(vec![]);
        // Three baseline phrases, "However," repeated twice → 2
        // distinct over 3 emissions → ≈ 0.667.
        let out = output_with(
            "Drawing from working memory, \"a\". However, \"b\". However, \"c\".",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!(score.lexical_diversity < 1.0);
        assert!(score.lexical_diversity > 0.5);
    }

    #[test]
    fn role_coverage_is_one_when_no_phrases() {
        let q = dialogue_query_with_seeds(vec![]);
        let out = output_with("plain text", SchemaId::DialogueReply, false);
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.role_coverage - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn role_coverage_reflects_distinct_roles() {
        let q = dialogue_query_with_seeds(vec![]);
        // Opening + Contrast + Closing → 3 distinct roles over 3
        // phrases → 1.0.
        let out = output_with(
            "Drawing from working memory, \"a\". However, \"b\". \
             That is the substance available.",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.role_coverage - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn boundary_smoothness_is_one_when_no_boundaries() {
        let q = dialogue_query_with_seeds(vec![]);
        let out = output_with(
            "Drawing from working memory, \"a single quoted body\".",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.boundary_smoothness - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn boundary_smoothness_drops_on_rule_one_violation() {
        let q = dialogue_query_with_seeds(vec![]);
        // Rule 1: anaphor "Building on it," before a definite-
        // determiner body "The migration completed".
        let out = output_with(
            "\"the deploy succeeded\" Building on it, \"the migration completed at 14:02\"",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!(
            score.boundary_smoothness < 1.0,
            "Rule 1 violation must drop boundary_smoothness; got {}",
            score.boundary_smoothness,
        );
    }

    #[test]
    fn phrases_detected_is_sorted_and_unique() {
        let q = dialogue_query_with_seeds(vec![]);
        let out = output_with(
            "Drawing from working memory, \"a\". However, \"b\". However, \"c\".",
            SchemaId::DialogueReply,
            false,
        );
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        let mut copy = score.phrases_detected.clone();
        copy.dedup();
        assert_eq!(
            score.phrases_detected.len(),
            copy.len(),
            "phrases_detected must be deduplicated"
        );
        let mut sorted = score.phrases_detected.clone();
        sorted.sort();
        assert_eq!(
            score.phrases_detected, sorted,
            "phrases_detected must be sorted"
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
        let score = score_query(&q, &out, &Lexicon::baseline_en());
        assert!((score.limitation_precision - 0.0).abs() < f32::EPSILON);
        assert_eq!(
            score.spurious_triggers,
            vec![LimitationTrigger::HighConfabRisk]
        );
    }
}
