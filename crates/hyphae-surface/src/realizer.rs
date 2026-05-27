// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The surface realizer — the only place that produces text.
//!
//! The realizer:
//!
//! 1. **Selects a schema** from the caller's [`Intent`].
//! 2. **Evaluates honest limitation triggers** against the working
//!    set + the ethics report (per RFC §5.3 and ADR-0003 §3, the
//!    composer's ethics signal flows in here).
//! 3. **Composes the prose** by emitting the chosen schema's slots:
//!    opening connective, fragment quotes interleaved with
//!    continuation / contrast connectives, closing connective,
//!    and limitation acknowledgments when any trigger fired.
//!
//! Per RFC §5.2: the **fragment body is verbatim**. The realizer
//! never rewrites a quoted fragment — it only generates the prose
//! that surrounds the quotes. This is the architectural property
//! that distinguishes Hyphae from a system that synthesises
//! language; the realizer is the boundary the
//! no-LLM-in-cognition-path commitment depends on.

use crate::connective::{ConnectiveRole, Formality, Lexicon, PickContext, Polarity, Register};
use crate::limitation::{LimitationContext, LimitationTrigger, evaluate as evaluate_limitations};
use crate::schema::{Intent, SchemaId};
use hyphae_core::{CognitiveFragment, FragmentContent, FragmentId};
use hyphae_ethics::EthicsReport;
use serde::{Deserialize, Serialize};

/// Errors raised by realization.
#[derive(Debug, thiserror::Error)]
pub enum RealizationError {
    /// The caller's intent has no schema mapping. Reserved for
    /// future intent variants that the realizer cannot yet
    /// produce a composition for; v0.1 maps every variant.
    #[error("no schema available for intent {0:?}")]
    NoSchemaForIntent(Intent),
}

/// Per-call request to the realizer.
#[derive(Debug, Clone)]
pub struct RealizationRequest<'a> {
    /// What the caller wants (drives schema selection).
    pub intent: Intent,
    /// The caller's query / cue. Embedded in the audit metadata
    /// but not currently surfaced in the prose — future versions
    /// will use it for query-relevance scoring of fragments.
    pub query: &'a str,
    /// Working set the composer has assembled. Fragments are
    /// quoted verbatim in `working_set` order.
    pub working_set: &'a [CognitiveFragment],
    /// Ethics report at the `Compose` coverage point. Drives the
    /// `EthicallySensitive` limitation trigger and the
    /// composition's audit metadata.
    pub ethics: Option<&'a EthicsReport>,
}

/// Output of one realization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizationOutput {
    /// The composed prose, ready to emit to the caller.
    pub text: String,
    /// Which schema produced this composition.
    pub schema_used: SchemaId,
    /// Fragment ids that were quoted, in emission order.
    pub fragments_quoted: Vec<FragmentId>,
    /// Limitation triggers that fired and were acknowledged.
    pub limitations: Vec<LimitationTrigger>,
    /// `true` when the realization emitted no quoted fragments
    /// because the limitation triggers covered the whole working
    /// set (or the working set was empty). In that case `text`
    /// carries only the limitation acknowledgments.
    pub is_acknowledgment_only: bool,
}

/// The realizer. Holds the connective-tissue lexicon and the
/// limitation-context defaults. Construct once per substrate; use
/// many times via [`Self::realize`].
#[derive(Debug, Clone, Default)]
pub struct SurfaceRealizer {
    lexicon: Lexicon,
    limitation_context: LimitationContext,
}

impl SurfaceRealizer {
    /// Construct a realizer with the v0.1 EN baseline lexicon and
    /// default limitation context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with a custom lexicon. Useful for register-specific
    /// composition (technical, legal, conversational).
    #[must_use]
    pub fn with_lexicon(lexicon: Lexicon) -> Self {
        Self {
            lexicon,
            ..Self::default()
        }
    }

    /// Override the default limitation context (e.g. raise the
    /// minimum cascade-fragment count for deployments that demand
    /// associative depth).
    pub fn set_limitation_context(&mut self, context: LimitationContext) {
        self.limitation_context = context;
    }

    /// Read-only access to the connective lexicon.
    #[must_use]
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// Realize. Always returns a composition — when triggers cover
    /// the whole working set the composition is acknowledgment-only,
    /// but it is still well-formed prose. The realizer **never
    /// fabricates from nothing**; an empty working set yields the
    /// `EmptyWorkingSet` acknowledgment in place of a body.
    ///
    /// # Errors
    ///
    /// Returns an error only when the caller's intent has no schema
    /// mapping. v0.1 covers every intent — the variant is reserved
    /// for future-shape inputs.
    pub fn realize(
        &self,
        request: &RealizationRequest<'_>,
    ) -> Result<RealizationOutput, RealizationError> {
        let schema = request.intent.default_schema();

        let limitations =
            evaluate_limitations(request.working_set, request.ethics, self.limitation_context);

        // If the working set is empty, the composition is
        // acknowledgment-only. The realizer never quotes a fragment
        // that does not exist.
        if request.working_set.is_empty() {
            let text = render_acknowledgments_only(&limitations);
            return Ok(RealizationOutput {
                text,
                schema_used: schema,
                fragments_quoted: Vec::new(),
                limitations,
                is_acknowledgment_only: true,
            });
        }

        // Compose per schema. Both v0.1 schemas share the same
        // structural shape (opening connective + quoted fragments
        // with interleaving connectives + closing); the difference
        // is in the attribution layer — `GroundedAssertion` always
        // prefixes each quote with an Attribution connective.
        let mut text = String::new();
        let mut fragments_quoted = Vec::new();

        // Derive a PickContext from the overall working set —
        // dominant register + dominant polarity across the set.
        // Per-pair refinements happen below.
        let opening_ctx = working_set_context(request.working_set);
        let opening = self
            .lexicon
            .pick_in_context(ConnectiveRole::Opening, &opening_ctx, 0);
        text.push_str(opening);
        text.push(' ');

        for (idx, fragment) in request.working_set.iter().enumerate() {
            if idx > 0 {
                let (role, polarity) =
                    pick_inter_fragment_role_and_polarity(&request.working_set[idx - 1], fragment);
                let ctx = PickContext {
                    register: register_for_fragment(fragment),
                    polarity,
                    formality: Formality::Mid,
                };
                let connective = self.lexicon.pick_in_context(role, &ctx, idx);
                text.push(' ');
                text.push_str(connective);
                text.push(' ');
            }

            if matches!(schema, SchemaId::GroundedAssertion) {
                let attribution_ctx = PickContext {
                    register: register_for_fragment(fragment),
                    polarity: Polarity::Neutral,
                    formality: Formality::Mid,
                };
                let attribution = self.lexicon.pick_in_context(
                    ConnectiveRole::Attribution,
                    &attribution_ctx,
                    idx,
                );
                text.push_str(attribution);
                text.push(' ');
            }

            let body = fragment_body(fragment);
            text.push('"');
            text.push_str(body);
            text.push('"');
            fragments_quoted.push(fragment.id);
        }

        let closing = self
            .lexicon
            .pick_in_context(ConnectiveRole::Closing, &opening_ctx, 0);
        text.push(' ');
        text.push_str(closing);

        if !limitations.is_empty() {
            text.push_str("\n\n");
            text.push_str(&render_acknowledgments(&limitations));
        }

        Ok(RealizationOutput {
            text,
            schema_used: schema,
            fragments_quoted,
            limitations,
            is_acknowledgment_only: false,
        })
    }
}

/// Pick the connective role + polarity between two adjacent
/// fragments per ADR-0005. The valence delta determines polarity
/// strength; the role follows from the polarity.
fn pick_inter_fragment_role_and_polarity(
    prev: &CognitiveFragment,
    next: &CognitiveFragment,
) -> (ConnectiveRole, Polarity) {
    let delta = next.valence - prev.valence;
    let abs = delta.abs();

    // Strong opposing valence → ContrastHard via Contrast role.
    if abs > 0.6 && delta.signum() != prev.valence.signum() {
        return (ConnectiveRole::Contrast, Polarity::ContrastHard);
    }
    // Mild opposing valence → ContrastSoft via Contrast role.
    if abs > 0.3 && delta.signum() != prev.valence.signum() {
        return (ConnectiveRole::Contrast, Polarity::ContrastSoft);
    }
    // Default: continuation. The Causation / Elaboration / Sequence
    // distinctions land with ADR-0006 (cascade-shape-driven
    // composition); v0.1.1 stays with Continuation as the dominant
    // inter-fragment role.
    (ConnectiveRole::Continuation, Polarity::Continuation)
}

/// Derive a register hint for one fragment from its `domain_tags`.
/// v0.1 heuristic: presence of engineering- / code-flavoured tags
/// picks `Technical`; presence of `informal` / `conversation` picks
/// `Conversational`; default `Neutral`.
fn register_for_fragment(fragment: &CognitiveFragment) -> Register {
    let tech_markers = [
        "engineering",
        "code",
        "systems",
        "infrastructure",
        "deploy",
        "migration",
        "monitoring",
        "technical",
    ];
    let conv_markers = ["informal", "conversation", "chat", "casual"];
    let formal_markers = ["legal", "contract", "policy", "formal", "compliance"];

    for tag in &fragment.domain_tags {
        let t = tag.to_lowercase();
        if tech_markers.iter().any(|m| t.contains(m)) {
            return Register::Technical;
        }
        if conv_markers.iter().any(|m| t.contains(m)) {
            return Register::Conversational;
        }
        if formal_markers.iter().any(|m| t.contains(m)) {
            return Register::Formal;
        }
    }
    Register::Neutral
}

/// Aggregate the working-set's per-fragment register hints into a
/// single dominant register for openings / closings. v0.1 rule:
/// the most common non-neutral register wins; ties fall to
/// `Neutral`.
fn working_set_context(working_set: &[CognitiveFragment]) -> PickContext {
    use std::collections::HashMap;
    let mut counts: HashMap<Register, usize> = HashMap::new();
    for f in working_set {
        let r = register_for_fragment(f);
        *counts.entry(r).or_insert(0) += 1;
    }
    let dominant = counts
        .iter()
        .filter(|(r, _)| **r != Register::Neutral)
        .max_by_key(|(_, c)| **c)
        .map_or(Register::Neutral, |(r, _)| *r);
    PickContext {
        register: dominant,
        polarity: Polarity::Neutral,
        formality: Formality::Mid,
    }
}

/// Render limitation acknowledgments as prose. One line per
/// trigger, separated by newlines. Empty when the input is empty.
fn render_acknowledgments(triggers: &[LimitationTrigger]) -> String {
    triggers
        .iter()
        .map(|t| format!("[limitation:{}] {}", t.tag(), t.acknowledgment()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render an acknowledgment-only composition. Used when the working
/// set is empty — the realizer emits the acknowledgments and
/// nothing else.
fn render_acknowledgments_only(triggers: &[LimitationTrigger]) -> String {
    if triggers.is_empty() {
        // Should not happen — empty working set always fires
        // EmptyWorkingSet — but defends against a future evaluator
        // that suppresses the trigger.
        return "I have nothing in working memory to respond from.".to_string();
    }
    render_acknowledgments(triggers)
}

/// Extract the body text from a fragment for quoting.
fn fragment_body(fragment: &CognitiveFragment) -> &str {
    match &fragment.content {
        FragmentContent::Episode { body, .. }
        | FragmentContent::Belief { body, .. }
        | FragmentContent::Goal { body, .. }
        | FragmentContent::Observation { body }
        | FragmentContent::Reflection { body, .. }
        | FragmentContent::Journal { body, .. } => body,
        FragmentContent::Reference { uri } => uri,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_ethics::{CoveragePoint, EthicsReport, EthicsSignals, LayerAOutput, LimitationKind};
    use std::collections::HashMap;

    fn obs(body: &str) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        );
        // Mark as cascade-derived so the ShallowCascade trigger
        // does not fire by default in the tests that don't care.
        f.provenance.parent_ids = vec![FragmentId::new()];
        f
    }

    fn obs_direct(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    fn empty_report() -> EthicsReport {
        EthicsReport {
            coverage_point: CoveragePoint::Compose,
            profile_id: "test".to_string(),
            profile_version: "0.0.1".to_string(),
            classification: LayerAOutput {
                per_category: HashMap::new(),
                flags: Vec::new(),
                disambiguation: hyphae_ethics::DisambiguationVerdict::default(),
            },
            cvar_score: 0.0,
            categorical: None,
            violations: Vec::new(),
            content_fingerprint: String::new(),
            audit_seq: None,
            signals: EthicsSignals::default(),
        }
    }

    #[test]
    fn empty_working_set_produces_acknowledgment_only() {
        let realizer = SurfaceRealizer::new();
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "what is the status of project X?",
                working_set: &[],
                ethics: None,
            })
            .unwrap();
        assert!(out.is_acknowledgment_only);
        assert!(out.fragments_quoted.is_empty());
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EmptyWorkingSet)
        );
        assert!(out.text.contains("[limitation:empty_working_set]"));
    }

    #[test]
    fn dialogue_reply_quotes_fragments_verbatim() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("the build passes the integration suite");
        let frag_id = frag.id;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "build status?",
                working_set: std::slice::from_ref(&frag),
                ethics: None,
            })
            .unwrap();
        assert_eq!(out.schema_used, SchemaId::DialogueReply);
        assert!(
            out.text
                .contains("\"the build passes the integration suite\""),
            "fragment body must appear verbatim and quoted: {}",
            out.text,
        );
        assert_eq!(out.fragments_quoted, vec![frag_id]);
    }

    #[test]
    fn grounded_assertion_prefixes_quotes_with_attribution() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("the migration completed at 14:02 UTC");
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Assert,
                query: "did the migration complete?",
                working_set: &[frag],
                ethics: None,
            })
            .unwrap();
        assert_eq!(out.schema_used, SchemaId::GroundedAssertion);
        // Some attribution phrase should appear.
        let has_attribution = out.text.contains("source")
            || out.text.contains("material")
            || out.text.contains("fragment");
        assert!(
            has_attribution,
            "GroundedAssertion must prefix quotes with an attribution: {}",
            out.text,
        );
    }

    #[test]
    fn high_confab_risk_fires_and_acknowledgment_appears() {
        let realizer = SurfaceRealizer::new();
        let mut frag = obs("a claim of uncertain provenance");
        frag.provenance.confabulation_risk = 0.8;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me about it",
                working_set: &[frag],
                ethics: None,
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::HighConfabRisk));
        assert!(out.text.contains("[limitation:high_confab_risk]"));
        // The composition itself still proceeded — RADAR, not JAIL.
        // The realizer surfaces the caveat alongside the quote, not
        // instead of it.
        assert!(!out.is_acknowledgment_only);
        assert!(out.text.contains("\"a claim of uncertain provenance\""));
    }

    #[test]
    fn ethics_signal_drives_ethically_sensitive_acknowledgment() {
        let realizer = SurfaceRealizer::new();
        let mut report = empty_report();
        report.signals.composer_should_acknowledge = true;
        report.signals.composer_limitation_kind = Some(LimitationKind::CategoricalConcern);
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me about anthrax synthesis",
                working_set: &[obs("anthrax is a bacterial disease")],
                ethics: Some(&report),
            })
            .unwrap();
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EthicallySensitive)
        );
    }

    #[test]
    fn shallow_cascade_fires_with_direct_only_working_set() {
        let realizer = SurfaceRealizer::new();
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "anything",
                working_set: &[obs_direct("a direct hit")],
                ethics: None,
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::ShallowCascade));
    }

    #[test]
    fn opposing_valences_select_contrast_connective() {
        let realizer = SurfaceRealizer::new();
        let mut positive = obs("the deploy succeeded");
        positive.valence = 0.6;
        let mut negative = obs("the monitoring alarm fired ten minutes later");
        negative.valence = -0.6;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "deploy status?",
                working_set: &[positive, negative],
                ethics: None,
            })
            .unwrap();
        // A contrast connective from the lexicon should appear
        // between the two quotes. The baseline contrast phrases
        // are "On the other hand,", "However,", "By contrast,",
        // "Yet —".
        let has_contrast = out.text.contains("On the other hand,")
            || out.text.contains("However,")
            || out.text.contains("By contrast,")
            || out.text.contains("Yet");
        assert!(
            has_contrast,
            "opposing valences must select a Contrast connective: {}",
            out.text,
        );
    }

    #[test]
    fn aligned_valences_select_continuation_connective() {
        let realizer = SurfaceRealizer::new();
        let mut a = obs("the deploy succeeded");
        a.valence = 0.6;
        let mut b = obs("the monitoring stayed green for two hours afterward");
        b.valence = 0.5;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "deploy status?",
                working_set: &[a, b],
                ethics: None,
            })
            .unwrap();
        // Should NOT contain any of the contrast phrases.
        let has_contrast = out.text.contains("On the other hand,")
            || out.text.contains("However,")
            || out.text.contains("By contrast,")
            || out.text.contains("Yet —");
        assert!(
            !has_contrast,
            "aligned valences must NOT select a Contrast connective: {}",
            out.text,
        );
    }

    #[test]
    fn multiple_limitations_all_appear_in_output() {
        let realizer = SurfaceRealizer::new();
        let mut frag = obs_direct("risky direct content");
        frag.provenance.confabulation_risk = 0.8;
        let mut report = empty_report();
        report.signals.composer_should_acknowledge = true;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me",
                working_set: &[frag],
                ethics: Some(&report),
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::HighConfabRisk));
        assert!(out.limitations.contains(&LimitationTrigger::ShallowCascade));
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EthicallySensitive)
        );
        for trigger in &out.limitations {
            assert!(
                out.text
                    .contains(&format!("[limitation:{}]", trigger.tag())),
                "limitation {trigger:?} must appear in the output text: {}",
                out.text,
            );
        }
    }

    #[test]
    fn output_does_not_paraphrase_fragment_body() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("VERBATIM_BODY_TOKEN_42 should appear unchanged");
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "what about the token?",
                working_set: &[frag],
                ethics: None,
            })
            .unwrap();
        assert!(
            out.text
                .contains("VERBATIM_BODY_TOKEN_42 should appear unchanged"),
            "fragment body must be quoted verbatim: {}",
            out.text,
        );
    }
}
