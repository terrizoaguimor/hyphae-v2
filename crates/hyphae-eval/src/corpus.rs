// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Evaluation corpus — native English queries against native English
//! seed memories.
//!
//! Per `docs/adr/0001-fresh-from-v1.md` §"Multilingual lexicon
//! beyond EN", v0.1 corpus is **EN-only with native seed memories**.
//! v1's bucket-1 corpus tested friendly-ES queries against EN-only
//! seeds, which produced the 0.993 grammaticality baseline Atlas
//! flagged as "currently unreliable" — the realizer never produced
//! native-ES output the upgraded scorer could grade. v2 corrects
//! this by construction: queries and seeds are both EN.
//!
//! The corpus ships in-tree as a static `seed_corpus_en` constructor
//! so eval runs are reproducible without an external file dependency.
//! Future versions can layer a TOML loader on top; v0.1 keeps the
//! corpus in Rust source where it can be reviewed in the same PR
//! as the scorer.

use hyphae_core::{CognitiveFragment, FragmentContent, FragmentId};
use hyphae_surface::{Intent, LimitationTrigger, SchemaId};
use serde::{Deserialize, Serialize};

/// One evaluation query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQuery {
    /// Stable identifier for cross-run comparison.
    pub id: String,
    /// What the caller is asking. EN, native phrasing.
    pub query: String,
    /// What kind of response the caller wants.
    pub intent: Intent,
    /// Seed memories that the substrate's composer would have
    /// retrieved for this query. Each seed becomes a working-set
    /// fragment.
    pub seeds: Vec<EvalSeed>,
    /// What the eval harness should observe about the realized
    /// output for this query to count as correct.
    pub expectations: Expectations,
}

/// One seed memory for a query. Becomes a [`CognitiveFragment`] at
/// harness time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSeed {
    /// Body text. **EN, native** — this is the v1 correction: no
    /// friendly-EN-on-foreign-seeds artefacts.
    pub body: String,
    /// Affective valence in `[-1.0, +1.0]`.
    #[serde(default)]
    pub valence: f32,
    /// Confabulation risk in `[0.0, 1.0]`.
    #[serde(default)]
    pub confabulation_risk: f32,
    /// `true` when the substrate retrieved this seed via cascade
    /// propagation (not as a direct seed). Drives whether the
    /// fragment carries non-empty `provenance.parent_ids`, which is
    /// what the `ShallowCascade` limitation trigger keys on.
    #[serde(default)]
    pub from_cascade: bool,
}

impl EvalSeed {
    /// Materialise this seed into a [`CognitiveFragment`] suitable
    /// for the harness to hand the realizer.
    #[must_use]
    pub fn into_fragment(self) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation { body: self.body },
            "eval-corpus",
        );
        f.valence = self.valence.clamp(-1.0, 1.0);
        f.provenance.confabulation_risk = self.confabulation_risk.clamp(0.0, 1.0);
        if self.from_cascade {
            // A synthetic parent id — the eval harness only cares
            // that `parent_ids` is non-empty, not what the parent
            // is.
            f.provenance.parent_ids = vec![FragmentId::new()];
        }
        f
    }
}

/// What the harness should observe about the realized output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expectations {
    /// The schema the realizer should select.
    pub schema: SchemaId,
    /// Limitation triggers that MUST fire (per the seed configuration
    /// and the intent). Empty = no triggers expected.
    #[serde(default)]
    pub must_fire: Vec<LimitationTrigger>,
    /// Limitation triggers that MUST NOT fire. Useful for negative
    /// tests — a healthy working set with cascade depth must not
    /// fire `ShallowCascade`, for example.
    #[serde(default)]
    pub must_not_fire: Vec<LimitationTrigger>,
    /// `true` when the output should be acknowledgment-only (the
    /// empty-working-set path). Empty working sets trigger this
    /// implicitly; the expectation is here for declarative clarity.
    #[serde(default)]
    pub acknowledgment_only: bool,
    /// `true` when the realizer should quote every seed body
    /// verbatim. v0.1 default: `true`. Set to `false` for
    /// acknowledgment-only queries.
    #[serde(default = "default_verbatim")]
    pub verbatim_quotation: bool,
}

#[must_use]
const fn default_verbatim() -> bool {
    true
}

/// A loaded corpus.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    queries: Vec<EvalQuery>,
}

impl Corpus {
    /// Construct an empty corpus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from an explicit list of queries.
    #[must_use]
    pub fn from_queries(queries: Vec<EvalQuery>) -> Self {
        Self { queries }
    }

    /// Number of queries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queries.len()
    }

    /// `true` when the corpus has no queries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }

    /// Read-only access to the queries.
    #[must_use]
    pub fn queries(&self) -> &[EvalQuery] {
        &self.queries
    }
}

/// The v0.1 native-EN baseline corpus. Twelve queries covering:
///
/// - 4 dialogue-reply queries with cascade-derived seeds (healthy
///   path; no limitations should fire).
/// - 2 grounded-assertion queries (single declarative claim, must
///   use attribution).
/// - 2 empty-working-set queries (acknowledgment-only path).
/// - 2 high-confab-risk queries (composition + limitation).
/// - 1 shallow-cascade query (direct-only seeds).
/// - 1 valence-opposed query (contrast connective selection).
///
/// The corpus is intentionally small for v0.1 — the harness's value
/// in v0.1 is the **honest scorer**, not the corpus size. Expansion
/// to the v1-style 255-query corpus is a separate ADR.
#[must_use]
#[allow(clippy::too_many_lines, clippy::vec_init_then_push)]
pub fn seed_corpus_en() -> Corpus {
    let mut q = Vec::new();

    // ── Healthy dialogue queries (4) ────────────────────────────
    q.push(EvalQuery {
        id: "dialogue-001".to_string(),
        query: "what is the status of the migration?".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the migration completed at 14:02 UTC".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
            EvalSeed {
                body: "the monitoring dashboards stayed green for the hour after the cutover"
                    .to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
        ],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![],
            must_not_fire: vec![
                LimitationTrigger::EmptyWorkingSet,
                LimitationTrigger::HighConfabRisk,
                LimitationTrigger::ShallowCascade,
            ],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "dialogue-002".to_string(),
        query: "summarize the deployment outcome".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the deploy succeeded on the first attempt".to_string(),
                valence: 0.5,
                confabulation_risk: 0.05,
                from_cascade: true,
            },
            EvalSeed {
                body: "no rollbacks were issued in the following six hours".to_string(),
                valence: 0.4,
                confabulation_risk: 0.05,
                from_cascade: true,
            },
        ],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![],
            must_not_fire: vec![
                LimitationTrigger::ShallowCascade,
                LimitationTrigger::HighConfabRisk,
            ],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "dialogue-003".to_string(),
        query: "what does the team say about test coverage?".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the integration suite covers the auth path end to end".to_string(),
                valence: 0.4,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
            EvalSeed {
                body: "unit coverage for the payment module stayed above 85 percent".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
            EvalSeed {
                body: "the team agreed to add chaos tests in the next quarter".to_string(),
                valence: 0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
        ],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![],
            must_not_fire: vec![
                LimitationTrigger::EmptyWorkingSet,
                LimitationTrigger::ShallowCascade,
            ],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "dialogue-004".to_string(),
        query: "describe the on-call rotation".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the primary on-call rotates weekly across four engineers".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
            EvalSeed {
                body: "secondary cover is handled by the platform team".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
        ],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::ShallowCascade],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── Grounded-assertion queries (2) ──────────────────────────
    q.push(EvalQuery {
        id: "assert-001".to_string(),
        query: "did the migration complete cleanly".to_string(),
        intent: Intent::Assert,
        seeds: vec![EvalSeed {
            body: "the migration completed at 14:02 UTC with zero errors".to_string(),
            valence: 0.4,
            confabulation_risk: 0.1,
            from_cascade: true,
        }],
        expectations: Expectations {
            schema: SchemaId::GroundedAssertion,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::HighConfabRisk],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "assert-002".to_string(),
        query: "what does the contract say about termination".to_string(),
        intent: Intent::Assert,
        seeds: vec![EvalSeed {
            body: "either party may terminate the contract with thirty days written notice"
                .to_string(),
            valence: 0.0,
            confabulation_risk: 0.05,
            from_cascade: true,
        }],
        expectations: Expectations {
            schema: SchemaId::GroundedAssertion,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::ShallowCascade],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── Empty-working-set queries (2) ───────────────────────────
    q.push(EvalQuery {
        id: "empty-001".to_string(),
        query: "what is the launch date for project orion".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![LimitationTrigger::EmptyWorkingSet],
            must_not_fire: vec![],
            acknowledgment_only: true,
            verbatim_quotation: false,
        },
    });

    q.push(EvalQuery {
        id: "empty-002".to_string(),
        query: "who attended last week's quarterly review".to_string(),
        intent: Intent::Assert,
        seeds: vec![],
        expectations: Expectations {
            schema: SchemaId::GroundedAssertion,
            must_fire: vec![LimitationTrigger::EmptyWorkingSet],
            must_not_fire: vec![],
            acknowledgment_only: true,
            verbatim_quotation: false,
        },
    });

    // ── High-confab-risk queries (2) ────────────────────────────
    q.push(EvalQuery {
        id: "risk-001".to_string(),
        query: "who said the architecture would not scale".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![EvalSeed {
            body: "an unnamed colleague reportedly said the architecture would not scale"
                .to_string(),
            valence: -0.2,
            confabulation_risk: 0.8,
            from_cascade: true,
        }],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![LimitationTrigger::HighConfabRisk],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "risk-002".to_string(),
        query: "is the new release safe to ship".to_string(),
        intent: Intent::Assert,
        seeds: vec![EvalSeed {
            body: "a third-party blog post claims the new release is safe to ship".to_string(),
            valence: 0.1,
            confabulation_risk: 0.7,
            from_cascade: true,
        }],
        expectations: Expectations {
            schema: SchemaId::GroundedAssertion,
            must_fire: vec![LimitationTrigger::HighConfabRisk],
            must_not_fire: vec![],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── Shallow-cascade query (1) ───────────────────────────────
    q.push(EvalQuery {
        id: "shallow-001".to_string(),
        query: "what is the current sprint focus".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![EvalSeed {
            body: "the current sprint focuses on the auth refactor".to_string(),
            valence: 0.1,
            confabulation_risk: 0.1,
            from_cascade: false,
        }],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![LimitationTrigger::ShallowCascade],
            must_not_fire: vec![
                LimitationTrigger::EmptyWorkingSet,
                LimitationTrigger::HighConfabRisk,
            ],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── Valence-opposed query (1) ───────────────────────────────
    q.push(EvalQuery {
        id: "contrast-001".to_string(),
        query: "how did the launch and the rollback compare".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the launch succeeded and traffic ramped smoothly".to_string(),
                valence: 0.7,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
            EvalSeed {
                body: "the rollback at 02:14 UTC was painful and lost three hours of writes"
                    .to_string(),
                valence: -0.7,
                confabulation_risk: 0.1,
                from_cascade: true,
            },
        ],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    Corpus::from_queries(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_corpus_has_at_least_ten_queries() {
        let corpus = seed_corpus_en();
        assert!(corpus.len() >= 10);
    }

    #[test]
    fn every_query_has_a_unique_id() {
        let corpus = seed_corpus_en();
        let mut ids = std::collections::HashSet::new();
        for q in corpus.queries() {
            assert!(ids.insert(&q.id), "duplicate id: {}", q.id);
        }
    }

    #[test]
    fn empty_working_set_queries_have_zero_seeds() {
        let corpus = seed_corpus_en();
        for q in corpus.queries() {
            if q.expectations
                .must_fire
                .contains(&LimitationTrigger::EmptyWorkingSet)
            {
                assert!(
                    q.seeds.is_empty(),
                    "{}: EmptyWorkingSet expectation requires zero seeds",
                    q.id,
                );
            }
        }
    }

    #[test]
    fn high_confab_risk_queries_have_at_least_one_risky_seed() {
        use crate::corpus::seed_corpus_en;
        let corpus = seed_corpus_en();
        for q in corpus.queries() {
            if q.expectations
                .must_fire
                .contains(&LimitationTrigger::HighConfabRisk)
            {
                let any_risky = q.seeds.iter().any(|s| s.confabulation_risk >= 0.5);
                assert!(
                    any_risky,
                    "{}: HighConfabRisk expectation requires at least one seed with risk >= 0.5",
                    q.id,
                );
            }
        }
    }

    #[test]
    fn seed_into_fragment_clamps_valence_and_risk() {
        let seed = EvalSeed {
            body: "x".to_string(),
            valence: 5.0,
            confabulation_risk: -1.0,
            from_cascade: false,
        };
        let frag = seed.into_fragment();
        assert!((frag.valence - 1.0).abs() < f32::EPSILON);
        assert!((frag.provenance.confabulation_risk - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn from_cascade_seed_populates_parent_ids() {
        let seed = EvalSeed {
            body: "x".to_string(),
            valence: 0.0,
            confabulation_risk: 0.0,
            from_cascade: true,
        };
        let frag = seed.into_fragment();
        assert!(!frag.provenance.parent_ids.is_empty());
    }

    #[test]
    fn direct_seed_leaves_parent_ids_empty() {
        let seed = EvalSeed {
            body: "x".to_string(),
            valence: 0.0,
            confabulation_risk: 0.0,
            from_cascade: false,
        };
        let frag = seed.into_fragment();
        assert!(frag.provenance.parent_ids.is_empty());
    }
}
