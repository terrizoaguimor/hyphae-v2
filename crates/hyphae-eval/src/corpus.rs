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
    /// **ADR-0009.** Domain tags propagated to the fragment's
    /// `domain_tags`. The realizer's `register_for_fragment`
    /// heuristic reads these to derive a `Register` for picker
    /// context (e.g. `["engineering"]` → `Technical`,
    /// `["legal", "contract"]` → `Formal`, `["informal",
    /// "conversation"]` → `Conversational`). Empty leaves the
    /// fragment untagged and the realizer defaults to
    /// `Register::Neutral`.
    #[serde(default)]
    pub domain_tags: Vec<String>,
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
        f.domain_tags = self.domain_tags;
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

/// The native-EN baseline corpus, thirty queries (v0.2)
/// covering:
///
/// - 4 dialogue-reply queries with cascade-derived seeds (healthy
///   path; no limitations should fire).
/// - 2 grounded-assertion queries (single declarative claim, must
///   use attribution).
/// - 2 empty-working-set queries (acknowledgment-only path).
/// - 2 high-confab-risk queries (composition + limitation).
/// - 1 shallow-cascade query (direct-only seeds).
/// - 1 valence-opposed query (contrast connective selection).
/// - **3 ADR-0008 fluency-exercise queries**: multi-role
///   composition, causation-shape composition, opposed-valence
///   sequence. These drive `lexical_diversity`, `role_coverage`,
///   and `boundary_smoothness` above the trivial floors.
/// - **10 ADR-0009 bucket-coverage queries**: 3 Conversational,
///   3 Formal, 2 Neutral, 2 Mixed-register. These vary the
///   `domain_tags` axis so the realizer's
///   `register_for_fragment` heuristic exercises more than the
///   `Neutral` slice of the lexicon.
/// - **3 ADR-0016 Summary-schema queries**: multi-service
///   deployment synthesis, week-over-week metric summary,
///   shallow-cascade single-source summary. Drive
///   `Intent::Summarize → SchemaId::Summary` and exercise the
///   Summary-role closing slot.
/// - **2 ADR-0023 `ComparativeAnalysis` queries**: cross-service
///   deploy comparison, cross-quarter metric comparison. Drive
///   `Intent::Compare → SchemaId::ComparativeAnalysis` and
///   exercise the forced-Contrast inter-fragment role +
///   Summary closing.
///
/// The corpus is intentionally small for v0.1 — the harness's value
/// in v0.1 is the **honest scorer + bucket coverage**, not the
/// corpus size. Expansion to the v1-style 255-query corpus is a
/// separate ADR.
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

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the monitoring dashboards stayed green for the hour after the cutover"
                    .to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "no rollbacks were issued in the following six hours".to_string(),
                valence: 0.4,
                confabulation_risk: 0.05,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "unit coverage for the payment module stayed above 85 percent".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the team agreed to add chaos tests in the next quarter".to_string(),
                valence: 0.2,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "secondary cover is handled by the platform team".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
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
            domain_tags: vec!["legal".to_string(), "contract".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
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

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the rollback at 02:14 UTC was painful and lost three hours of writes"
                    .to_string(),
                valence: -0.7,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

    // ── ADR-0008 fluency exercise: multi-role composition ──────
    // Four fragments with mixed valence and cascade depth — the
    // realizer is expected to invoke opening + ≥2 distinct
    // inter-fragment roles + closing, exercising
    // `lexical_diversity` and `role_coverage` above the trivial
    // 1.0-by-default floor.
    q.push(EvalQuery {
        id: "fluency-multirole-001".to_string(),
        query: "what's the overall state of the q3 launch program".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the staging environment validated the new pricing engine end to end"
                    .to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the data warehouse migration finished six days ahead of plan".to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the customer support team flagged three onboarding regressions in the \
                       beta cohort"
                    .to_string(),
                valence: -0.4,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

    // ── ADR-0008 fluency exercise: causation-shape composition ─
    // Three fragments with parent-id chain — exercises the
    // ADR-0006 Causation-role projection and surfaces a different
    // connective bucket than the dialogue queries.
    q.push(EvalQuery {
        id: "fluency-causation-001".to_string(),
        query: "why did the cache hit rate drop after the deploy".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the deploy rolled out a new serialization format at 09:14 UTC".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the cache layer rejected all entries written in the previous format"
                    .to_string(),
                valence: -0.3,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the hit rate dropped from 94 percent to 41 percent over the next hour"
                    .to_string(),
                valence: -0.5,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

    // ── ADR-0008 fluency exercise: opposed-valence sequence ────
    // Two fragments with strongly opposed valence and identical
    // cascade depth — exercises Contrast-role selection and the
    // boundary smoothing pathway when adjacent bodies share a
    // determiner-led NP shape.
    q.push(EvalQuery {
        id: "fluency-opposed-001".to_string(),
        query: "how did the release perform across the two customer tiers".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the enterprise tier reported a measurable latency improvement".to_string(),
                valence: 0.7,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the free tier reported intermittent connection resets for six hours"
                    .to_string(),
                valence: -0.7,
                confabulation_risk: 0.1,
                from_cascade: true,

                domain_tags: vec!["engineering".to_string()],
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

    // ── ADR-0009 conversational-register queries (3) ──────────
    // Informal / chat-style bodies. Tags push the realizer's
    // `register_for_fragment` heuristic to `Conversational`, which
    // unlocks the `(_, Conversational, _, _)` slice of the lexicon
    // that ADR-0005 populated but no v0.1 query reaches.
    q.push(EvalQuery {
        id: "conv-001".to_string(),
        query: "how is the team feeling this week".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the team mood has been pretty upbeat since the holiday week".to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["informal".to_string(), "conversation".to_string()],
            },
            EvalSeed {
                body: "people are still buzzing about the offsite next month".to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["informal".to_string(), "conversation".to_string()],
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

    q.push(EvalQuery {
        id: "conv-002".to_string(),
        query: "anything memorable from the all-hands".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the demo broke mid-presentation and the whole room laughed".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["informal".to_string(), "chat".to_string()],
            },
            EvalSeed {
                body: "the recovery was quick and the audience stayed engaged".to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["informal".to_string(), "conversation".to_string()],
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

    q.push(EvalQuery {
        id: "conv-003".to_string(),
        query: "how is the new onboarding landing with people".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the new onboarding feels noticeably smoother than the previous version"
                    .to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["informal".to_string(), "conversation".to_string()],
            },
            EvalSeed {
                body: "people seem to find their bearings within the first day now".to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["casual".to_string(), "conversation".to_string()],
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

    // ── ADR-0009 formal-register queries (3) ──────────────────
    // Policy / compliance / legal bodies. Tags push register to
    // `Formal`, unlocking the `(_, Formal, _, _)` lexicon slice.
    q.push(EvalQuery {
        id: "formal-001".to_string(),
        query: "what does the data retention policy say".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the data retention policy requires deletion within ninety days of \
                       account closure"
                    .to_string(),
                valence: 0.0,
                confabulation_risk: 0.05,
                from_cascade: true,
                domain_tags: vec!["formal".to_string(), "policy".to_string()],
            },
            EvalSeed {
                body: "any deviation must be reviewed by the privacy office before approval"
                    .to_string(),
                valence: 0.0,
                confabulation_risk: 0.05,
                from_cascade: true,
                domain_tags: vec!["formal".to_string(), "compliance".to_string()],
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

    q.push(EvalQuery {
        id: "formal-002".to_string(),
        query: "summarise the quarterly compliance audit".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the quarterly audit confirmed compliance with the relevant regulatory \
                       standards"
                    .to_string(),
                valence: 0.4,
                confabulation_risk: 0.05,
                from_cascade: true,
                domain_tags: vec!["formal".to_string(), "compliance".to_string()],
            },
            EvalSeed {
                body: "the auditor noted no material findings during the engagement".to_string(),
                valence: 0.5,
                confabulation_risk: 0.05,
                from_cascade: true,
                domain_tags: vec!["formal".to_string(), "compliance".to_string()],
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

    q.push(EvalQuery {
        id: "formal-003".to_string(),
        query: "does indemnification survive termination of the agreement".to_string(),
        intent: Intent::Assert,
        seeds: vec![EvalSeed {
            body: "the indemnification clause survives termination of the agreement".to_string(),
            valence: 0.0,
            confabulation_risk: 0.05,
            from_cascade: true,
            domain_tags: vec!["legal".to_string(), "contract".to_string()],
        }],
        expectations: Expectations {
            schema: SchemaId::GroundedAssertion,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── ADR-0009 neutral-register queries (2) ─────────────────
    // Untagged bodies — the realizer defaults to `Register::Neutral`.
    // Verifies the v0.1 default path stays healthy when no register
    // marker is present.
    q.push(EvalQuery {
        id: "neutral-001".to_string(),
        query: "what was agreed about the upcoming planning cycle".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the team agreed to revisit the priority list at the end of the quarter"
                    .to_string(),
                valence: 0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
            },
            EvalSeed {
                body: "the working hours will stay flexible through the next two sprints"
                    .to_string(),
                valence: 0.1,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
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

    q.push(EvalQuery {
        id: "neutral-002".to_string(),
        query: "how does the cross-functional working group operate".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the cross-functional working group meets on alternating fridays".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
            },
            EvalSeed {
                body: "the rotation schedule is shared in the team handbook".to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![],
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

    // ── ADR-0009 mixed-register queries (2) ───────────────────
    // Both engineering and formal markers present. Tie-break in
    // `working_set_context_refs` picks the most common non-neutral
    // register; verifies the aggregator behaves under ambiguity.
    q.push(EvalQuery {
        id: "mixed-001".to_string(),
        query: "did the open source audit raise any licensing concerns".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the open source audit completed without any licensing concerns".to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string(), "legal".to_string()],
            },
            EvalSeed {
                body: "the dependency manifest was attached to the compliance record".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["legal".to_string(), "compliance".to_string()],
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

    q.push(EvalQuery {
        id: "mixed-002".to_string(),
        query: "how did the SOC 2 review handle the new infrastructure changes".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "the SOC 2 controls were reviewed against the new infrastructure changes"
                    .to_string(),
                valence: 0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec![
                    "formal".to_string(),
                    "compliance".to_string(),
                    "engineering".to_string(),
                ],
            },
            EvalSeed {
                body: "the operations team accepted the residual risk in writing".to_string(),
                valence: 0.1,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["formal".to_string(), "engineering".to_string()],
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

    // ── ADR-0016 Summary schema queries (3) ────────────────────
    // Exercise the new SchemaId::Summary slot. Lexicon's Summary
    // role provides the closing line ("Overall,", "On balance,",
    // "Taking it together,", …). v0.2 corpus expansion.
    q.push(EvalQuery {
        id: "summary-001".to_string(),
        query: "summarise the deployment situation across services".to_string(),
        intent: Intent::Summarize,
        seeds: vec![
            EvalSeed {
                body: "the payment service deploy completed without errors at 09:14 UTC"
                    .to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the notification service deploy needed a hot patch at 09:36 UTC".to_string(),
                valence: -0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the search service stayed on the previous version pending the q3 cutover"
                    .to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
        ],
        expectations: Expectations {
            schema: SchemaId::Summary,
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
        id: "summary-002".to_string(),
        query: "give me an overall read on this week's metrics".to_string(),
        intent: Intent::Summarize,
        seeds: vec![
            EvalSeed {
                body: "weekly active users grew six percent over the trailing seven days"
                    .to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body:
                    "the p95 request latency rose from 180 to 210 milliseconds on the same window"
                        .to_string(),
                valence: -0.4,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the support ticket queue dropped from 84 to 61 open items".to_string(),
                valence: 0.4,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
        ],
        expectations: Expectations {
            schema: SchemaId::Summary,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // ── ADR-0023 ComparativeAnalysis queries (2) ──────────────
    q.push(EvalQuery {
        id: "compare-001".to_string(),
        query: "compare the staging and production deploys".to_string(),
        intent: Intent::Compare,
        seeds: vec![
            EvalSeed {
                body: "the staging deploy completed cleanly at 09:14 UTC".to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "the production deploy required a hot patch at 09:36 UTC".to_string(),
                valence: -0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
        ],
        expectations: Expectations {
            schema: SchemaId::ComparativeAnalysis,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "compare-002".to_string(),
        query: "compare this quarter's metrics against the previous quarter".to_string(),
        intent: Intent::Compare,
        seeds: vec![
            EvalSeed {
                body: "weekly active users grew six percent quarter-over-quarter".to_string(),
                valence: 0.6,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body:
                    "the p95 request latency rose from 180 to 210 milliseconds quarter-over-quarter"
                        .to_string(),
                valence: -0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
        ],
        expectations: Expectations {
            schema: SchemaId::ComparativeAnalysis,
            must_fire: vec![],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    q.push(EvalQuery {
        id: "summary-003".to_string(),
        query: "what's the overall state from the single touchpoint we have".to_string(),
        intent: Intent::Summarize,
        seeds: vec![EvalSeed {
            // Single-fragment, direct-only seed → ShallowCascade
            // fires AND the realizer still emits a Summary closing
            // per ADR-0016's "no silent downgrade" rule.
            body: "the standup notes mention the migration is on track for next tuesday"
                .to_string(),
            valence: 0.3,
            confabulation_risk: 0.2,
            from_cascade: false,
            domain_tags: vec!["engineering".to_string()],
        }],
        expectations: Expectations {
            schema: SchemaId::Summary,
            must_fire: vec![LimitationTrigger::ShallowCascade],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    Corpus::from_queries(q)
}

/// **ADR-0018.** The v0.2 native-Spanish baseline corpus. Five
/// queries; architectural extension proof, NOT coverage parity
/// with the EN corpus.
///
/// Bodies and seeds are native Spanish — no machine translation.
/// `domain_tags` stay English per ADR-0017 (semantic identifiers,
/// not natural-language words).
///
/// **Known limitation**: the harness's `boundary_smoothness`
/// dimension reports inflated 1.0 for ES queries because the
/// boundary-smoothing rules in `hyphae_surface::boundary` are
/// EN-calibrated (determiners + anaphor surface forms). A future
/// ADR adds ES rules; until then, treat ES `boundary_smoothness`
/// as "not measured" rather than "perfect."
#[must_use]
#[allow(clippy::too_many_lines, clippy::vec_init_then_push)]
pub fn seed_corpus_es() -> Corpus {
    let mut q = Vec::new();

    // Healthy multi-fragment status check (Dialogue).
    q.push(EvalQuery {
        id: "es-dialogue-001".to_string(),
        query: "¿cuál es el estado de la migración?".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "la migración terminó a las 14:02 UTC".to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "los monitores se mantuvieron verdes durante la hora siguiente al cambio"
                    .to_string(),
                valence: 0.3,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
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

    // Empty working set → acknowledgment-only.
    q.push(EvalQuery {
        id: "es-empty-001".to_string(),
        query: "¿cuándo es el lanzamiento del proyecto orión?".to_string(),
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

    // High-confab-risk seed → must fire HighConfabRisk.
    q.push(EvalQuery {
        id: "es-risk-001".to_string(),
        query: "¿quién dijo que la arquitectura no iba a escalar?".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![EvalSeed {
            body: "un colega sin nombrar supuestamente dijo que la arquitectura no escalaría"
                .to_string(),
            valence: -0.2,
            confabulation_risk: 0.8,
            from_cascade: true,
            domain_tags: vec!["engineering".to_string()],
        }],
        expectations: Expectations {
            schema: SchemaId::DialogueReply,
            must_fire: vec![LimitationTrigger::HighConfabRisk],
            must_not_fire: vec![LimitationTrigger::EmptyWorkingSet],
            acknowledgment_only: false,
            verbatim_quotation: true,
        },
    });

    // Opposed-valence pair → exercises Contrast role.
    q.push(EvalQuery {
        id: "es-contrast-001".to_string(),
        query: "¿cómo se comportaron el lanzamiento y el rollback?".to_string(),
        intent: Intent::Dialogue,
        seeds: vec![
            EvalSeed {
                body: "el lanzamiento fue exitoso y el tráfico subió de forma estable"
                    .to_string(),
                valence: 0.7,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "el rollback a las 02:14 UTC fue doloroso y se perdieron tres horas de escrituras"
                    .to_string(),
                valence: -0.7,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
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

    // Three-fragment summary → ADR-0016 Summary in ES.
    q.push(EvalQuery {
        id: "es-summary-001".to_string(),
        query: "resúmeme el estado del despliegue entre los servicios".to_string(),
        intent: Intent::Summarize,
        seeds: vec![
            EvalSeed {
                body: "el despliegue del servicio de pagos completó sin errores a las 09:14 UTC"
                    .to_string(),
                valence: 0.5,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "el servicio de notificaciones requirió un parche en caliente a las 09:36 UTC"
                    .to_string(),
                valence: -0.2,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
            EvalSeed {
                body: "el servicio de búsqueda se mantuvo en la versión previa esperando el corte de q3"
                    .to_string(),
                valence: 0.0,
                confabulation_risk: 0.1,
                from_cascade: true,
                domain_tags: vec!["engineering".to_string()],
            },
        ],
        expectations: Expectations {
            schema: SchemaId::Summary,
            must_fire: vec![],
            must_not_fire: vec![
                LimitationTrigger::EmptyWorkingSet,
                LimitationTrigger::ShallowCascade,
            ],
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
    fn baseline_corpus_has_at_least_thirty_queries() {
        let corpus = seed_corpus_en();
        assert!(corpus.len() >= 30);
    }

    #[test]
    fn corpus_includes_compare_queries() {
        use hyphae_surface::Intent;
        let corpus = seed_corpus_en();
        let compare_count = corpus
            .queries()
            .iter()
            .filter(|q| matches!(q.intent, Intent::Compare))
            .count();
        assert!(
            compare_count >= 2,
            "ADR-0023 — corpus must contain ≥2 Compare queries, got {compare_count}",
        );
    }

    #[test]
    fn corpus_includes_summary_schema_queries() {
        use hyphae_surface::Intent;
        let corpus = seed_corpus_en();
        let summary_count = corpus
            .queries()
            .iter()
            .filter(|q| matches!(q.intent, Intent::Summarize))
            .count();
        assert!(
            summary_count >= 3,
            "ADR-0016 — corpus must contain ≥3 Summary queries, got {summary_count}",
        );
    }

    #[test]
    fn corpus_exercises_multiple_register_buckets() {
        // ADR-0009 bucket-coverage invariant: across all seeds,
        // at least one query carries Technical markers, one
        // carries Conversational markers, and one carries Formal
        // markers. Without this, the corpus would silently
        // regress to single-bucket coverage like the v0.1 state.
        let corpus = seed_corpus_en();
        let mut has_tech = false;
        let mut has_conv = false;
        let mut has_formal = false;
        for q in corpus.queries() {
            for s in &q.seeds {
                for t in &s.domain_tags {
                    let t = t.to_lowercase();
                    if matches!(
                        t.as_str(),
                        "engineering"
                            | "code"
                            | "systems"
                            | "infrastructure"
                            | "deploy"
                            | "migration"
                            | "monitoring"
                            | "technical"
                    ) {
                        has_tech = true;
                    }
                    if matches!(t.as_str(), "informal" | "conversation" | "chat" | "casual") {
                        has_conv = true;
                    }
                    if matches!(
                        t.as_str(),
                        "legal" | "contract" | "policy" | "formal" | "compliance"
                    ) {
                        has_formal = true;
                    }
                }
            }
        }
        assert!(has_tech, "corpus must contain Technical-tagged seeds");
        assert!(has_conv, "corpus must contain Conversational-tagged seeds");
        assert!(has_formal, "corpus must contain Formal-tagged seeds");
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

            domain_tags: vec!["engineering".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
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

            domain_tags: vec!["engineering".to_string()],
        };
        let frag = seed.into_fragment();
        assert!(frag.provenance.parent_ids.is_empty());
    }

    // ── ADR-0018 ES corpus invariants ──────────────────────────

    #[test]
    fn es_corpus_has_at_least_five_queries() {
        let corpus = seed_corpus_es();
        assert!(
            corpus.len() >= 5,
            "ES corpus floor is 5; got {}",
            corpus.len()
        );
    }

    #[test]
    fn es_corpus_bodies_are_spanish() {
        // Spot-check: at least one seed body should contain a
        // Spanish-indicative character or word that does not
        // appear in EN corpus bodies.
        let corpus = seed_corpus_es();
        let bodies: Vec<&str> = corpus
            .queries()
            .iter()
            .flat_map(|q| q.seeds.iter().map(|s| s.body.as_str()))
            .collect();
        let has_spanish_marker = bodies.iter().any(|b| {
            let lower = b.to_lowercase();
            lower.contains("la migración")
                || lower.contains("el despliegue")
                || lower.contains("los servicios")
                || lower.contains("el lanzamiento")
                || lower.contains("la arquitectura")
                || lower.contains("á")
                || lower.contains("é")
                || lower.contains("í")
                || lower.contains("ó")
                || lower.contains("ú")
                || lower.contains("ñ")
        });
        assert!(
            has_spanish_marker,
            "ES corpus seed bodies must contain Spanish-indicative markers",
        );
    }

    #[test]
    fn es_corpus_exercises_summary_schema() {
        let corpus = seed_corpus_es();
        let summary_count = corpus
            .queries()
            .iter()
            .filter(|q| matches!(q.intent, Intent::Summarize))
            .count();
        assert!(
            summary_count >= 1,
            "ES corpus should include at least one Summary query to exercise ADR-0016 in ES",
        );
    }

    #[test]
    fn es_corpus_domain_tags_stay_english() {
        // Per ADR-0017 §"Domain-tag semantics — stay English",
        // ES seeds tag with English markers even though bodies
        // are Spanish.
        let corpus = seed_corpus_es();
        let english_markers = [
            "engineering",
            "code",
            "legal",
            "informal",
            "formal",
            "compliance",
            "policy",
            "contract",
        ];
        for q in corpus.queries() {
            for s in &q.seeds {
                for tag in &s.domain_tags {
                    assert!(
                        english_markers.contains(&tag.to_lowercase().as_str()),
                        "ES seed in query `{}` carries non-English domain tag `{tag}` — \
                         ADR-0017 contract violated",
                        q.id,
                    );
                }
            }
        }
    }
}
